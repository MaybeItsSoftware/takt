//! The sync cycle's bodies, made and read in the core (docs/sync.md, "Wire
//! protocol"). A client moves bytes: it asks for a push body, sends it, and
//! hands each pulled page back as it came. Rows never cross the FFI as
//! records, and the structs are the server's own (`takt_sync_rules::wire`),
//! so the two ends encode one way.
//!
//! The clock is the stored one: a push stamps from `sync_state.hlc`, which
//! acknowledging it stores, and a pull receives into it and stores it with
//! the rows. A client keeps no clock of its own between the steps.

use std::sync::Mutex;

use rusqlite::{Connection, Transaction};
use serde_json::{Map, Number, Value};
use takt_sync_rules::hlc::Hlc;
use takt_sync_rules::wire::{
    ChangedRow, ChangesResponse, ErrorBody, PushRequest, WireChange, WireOp,
};

use std::collections::{HashMap, HashSet};

use super::spaces::{self, PERSONAL_SUFFIX};
use super::{
    IncomingRow, OutgoingChange, SyncValue, acknowledge, apply_rows, pending_changes,
    record_progress,
};
use crate::CoreError;

/// A push body ready to send, and what to acknowledge once the server has it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SyncPushBatch {
    /// `POST /v1/push`'s JSON body.
    pub body: String,
    /// The newest outbox entry folded in.
    pub through_seq: i64,
    /// How many rows the body carries.
    pub count: u32,
    /// The clock after stamping them, stored by `finish_push`.
    pub hlc: String,
}

/// What a pulled page said about the feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct SyncPullPage {
    /// Where the next page starts.
    pub cursor: i64,
    /// Whether to ask for another page straight away.
    pub has_more: bool,
}

/// What applying a pull came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct SyncPullOutcome {
    pub pulled: u32,
    /// Whether the workspace changed, so the UI knows to reload.
    pub changed: bool,
}

/// A pull being gathered: every page of a cycle lands in one transaction,
/// because a task can arrive a page before its list and only the end of the
/// whole pull is a consistent state. The rows wait here, in the core, rather
/// than crossing back to the client between pages.
#[derive(Debug, Default, uniffi::Object)]
pub struct SyncPull {
    pages: Mutex<Gathered>,
}

#[derive(Debug, Default)]
struct Gathered {
    rows: Vec<ChangedRow>,
    cursor: Option<i64>,
    /// The shared spaces the last page listed, when the server sends them.
    spaces: Option<Vec<takt_sync_rules::wire::WireSpace>>,
}

#[uniffi::export]
impl SyncPull {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes in one `GET /v1/changes` body.
    pub fn add_page(&self, body: String) -> Result<SyncPullPage, CoreError> {
        let page: ChangesResponse =
            serde_json::from_str(&body).map_err(|error| CoreError::File {
                detail: format!("The sync server's answer couldn't be read: {error}"),
            })?;
        let mut gathered = self.gathered();
        gathered.rows.extend(page.rows);
        gathered.cursor = Some(page.cursor);
        if page.spaces.is_some() {
            gathered.spaces = page.spaces;
        }
        Ok(SyncPullPage {
            cursor: page.cursor,
            has_more: page.has_more,
        })
    }

    /// The rows gathered so far.
    pub fn row_count(&self) -> u32 {
        u32::try_from(self.gathered().rows.len()).unwrap_or(u32::MAX)
    }
}

impl SyncPull {
    fn gathered(&self) -> std::sync::MutexGuard<'_, Gathered> {
        self.pages
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The clock a device stamps from: the stored one, or zero on `device_id`
/// before its first push or pull.
fn stored_clock(connection: &Connection) -> Result<Option<Hlc>, CoreError> {
    let Some(state) = super::state(connection)? else {
        return Ok(None);
    };
    Ok(Some(
        state
            .hlc
            .as_deref()
            .and_then(Hlc::parse)
            .unwrap_or_else(|| Hlc::new(0, 0, state.device_id)),
    ))
}

/// The outbox's next batch as a push body, each row stamped in the order its
/// edits were made, so a later edit to a column always carries the later
/// clock. Nothing when the outbox is empty. Errors when the device is not
/// paired.
pub fn prepare_push(
    connection: &Connection,
    limit: u32,
    wall_ms: i64,
) -> Result<Option<SyncPushBatch>, CoreError> {
    let Some(mut clock) = stored_clock(connection)? else {
        return Err(not_paired());
    };
    let pending = pending_changes(connection, limit)?;
    let sharing = spaces::active(connection)?;
    let mut items = pending.changes;
    if sharing {
        // Rows whose space changed under them, which the outbox doesn't
        // name: they go whole to where they now belong.
        let named: HashSet<(String, String)> = items
            .iter()
            .map(|change| (change.table.clone(), change.row_id.clone()))
            .collect();
        let room = limit.saturating_sub(u32::try_from(items.len()).unwrap_or(u32::MAX));
        for (table, id) in spaces::moved_rows(connection, room.max(1))? {
            if named.contains(&(table.clone(), id.clone())) {
                continue;
            }
            if let Some(values) = super::live_values(connection, &table, &id)? {
                items.push(OutgoingChange {
                    table,
                    row_id: id,
                    operation: "upsert".into(),
                    values,
                    changed_at_ms: wall_ms,
                });
            }
        }
    }
    if items.is_empty() {
        return Ok(None);
    }
    let through_seq = pending.through_seq.unwrap_or(0);
    items.sort_by_key(|change| change.changed_at_ms);
    let mut changes = Vec::new();
    let mut plan = Vec::new();
    for item in items {
        let at = item.changed_at_ms.max(wall_ms);
        let routed = if sharing {
            route(connection, &item, &mut plan)?
        } else {
            vec![(
                None,
                item.table.clone(),
                item.operation.clone(),
                item.values,
            )]
        };
        for (space, table, operation, values) in routed {
            clock = clock.tick(at);
            let delete = operation == "delete";
            changes.push(WireChange {
                table,
                id: item.row_id.clone(),
                op: if delete {
                    WireOp::Delete
                } else {
                    WireOp::Upsert
                },
                hlc: clock.to_string(),
                values: (!delete).then(|| {
                    values
                        .iter()
                        .map(|(name, value)| (name.clone(), json_value(value)))
                        .collect()
                }),
                space,
            });
        }
    }
    if sharing {
        spaces::plan(connection, &plan)?;
    }
    if changes.is_empty() {
        return Ok(None);
    }
    let count = u32::try_from(changes.len()).unwrap_or(u32::MAX);
    let body =
        serde_json::to_string(&PushRequest { changes }).map_err(|error| CoreError::File {
            detail: error.to_string(),
        })?;
    Ok(Some(SyncPushBatch {
        body,
        through_seq,
        count,
        hlc: clock.to_string(),
    }))
}

/// The server has a push: its outbox entries go, and its clock and the
/// last-synced time are kept, in one transaction.
pub fn finish_push(
    transaction: &Transaction,
    through_seq: i64,
    hlc: &str,
    now_ms: i64,
) -> Result<(), CoreError> {
    acknowledge(transaction, through_seq)?;
    spaces::carry_out_plan(transaction)?;
    record_progress(transaction, None, Some(hlc), now_ms)
}

/// Writes a gathered pull into the workspace, the clock moved past every
/// stamp in it. A pull with no page leaves the cursor where it was.
pub fn apply_pull(
    transaction: &Transaction,
    pull: &SyncPull,
    wall_ms: i64,
    now_ms: i64,
) -> Result<SyncPullOutcome, CoreError> {
    let Some(mut clock) = stored_clock(transaction)? else {
        return Err(not_paired());
    };
    let gathered = std::mem::take(&mut *pull.gathered());
    let cursor = match gathered.cursor {
        Some(cursor) => cursor,
        None => super::state(transaction)?.map_or(0, |state| state.cursor),
    };
    for stamp in gathered.rows.iter().filter_map(|row| row.hlc.as_deref()) {
        if let Some(remote) = Hlc::parse(stamp) {
            clock = clock.receiving(&remote, wall_ms);
        }
    }
    let rows: Vec<(IncomingRow, Option<String>)> = gathered
        .rows
        .into_iter()
        .map(|row| {
            let space = row.space.clone();
            (incoming(row), space)
        })
        .collect();
    let borrowed: Vec<(&IncomingRow, Option<&str>)> = rows
        .iter()
        .map(|(row, space)| (row, space.as_deref()))
        .collect();
    let changed = apply_rows(
        transaction,
        &borrowed,
        gathered.spaces.as_deref(),
        cursor,
        Some(&clock.to_string()),
        now_ms,
    )?;
    Ok(SyncPullOutcome {
        pulled: u32::try_from(rows.len()).unwrap_or(u32::MAX),
        changed,
    })
}

/// One outgoing row, as the changes to send for it: which space each goes
/// to (nothing for the account's own), its table name, "upsert" or
/// "delete", and its columns. Adds to `plan` where the row will live once
/// the server has them. See `spaces.rs`.
type Routed = (Option<String>, String, String, HashMap<String, SyncValue>);

fn route(
    connection: &Connection,
    item: &OutgoingChange,
    plan: &mut Vec<(String, String, Option<String>)>,
) -> Result<Vec<Routed>, CoreError> {
    let table = item.table.as_str();
    if !spaces::SHARED_TABLES.contains(&table) {
        return Ok(vec![(
            None,
            item.table.clone(),
            item.operation.clone(),
            item.values.clone(),
        )]);
    }
    let known = spaces::known(connection)?;
    // `''` marks a row already gone from its shared space.
    let recorded = spaces::recorded_space(connection, table, &item.row_id)?;
    let sent_to = recorded
        .clone()
        .filter(|space| !space.is_empty() && known.contains(space));
    let key = super::key_of(table).unwrap_or("id");
    let mut out: Vec<Routed> = Vec::new();
    if item.operation == "delete" {
        if let Some(space) = &sent_to {
            out.push((
                Some(space.clone()),
                item.table.clone(),
                "delete".into(),
                HashMap::new(),
            ));
        }
        out.push((None, item.table.clone(), "delete".into(), HashMap::new()));
        if recorded.is_some()
            && let Some(companion) = spaces::companion_table(table)
        {
            out.push((None, companion, "delete".into(), HashMap::new()));
        }
        if recorded.is_some() {
            plan.push((item.table.clone(), item.row_id.clone(), None));
        }
        return Ok(out);
    }
    let current = spaces::current_space(connection, table, &item.row_id)?;
    let was = recorded.filter(|space| !space.is_empty());
    let moved = current != was;
    let values = if moved {
        super::live_values(connection, table, &item.row_id)?.unwrap_or_else(|| item.values.clone())
    } else {
        item.values.clone()
    };
    if moved && let Some(space) = &sent_to {
        out.push((
            Some(space.clone()),
            item.table.clone(),
            "delete".into(),
            HashMap::new(),
        ));
    }
    match &current {
        None => out.push((None, item.table.clone(), "upsert".into(), values)),
        Some(space) => {
            let (personal, shared): (HashMap<_, _>, HashMap<_, _>) = values
                .into_iter()
                .partition(|(name, _)| spaces::is_personal_column(table, name));
            let key_value = shared.get(key).cloned();
            if moved || shared.keys().any(|name| name != key) {
                out.push((
                    Some(space.clone()),
                    item.table.clone(),
                    "upsert".into(),
                    shared,
                ));
            }
            if !personal.is_empty() {
                let mut personal = personal;
                if let Some(value) = key_value {
                    personal.insert(key.to_string(), value);
                }
                out.push((
                    None,
                    format!("{}{PERSONAL_SUFFIX}", item.table),
                    "upsert".into(),
                    personal,
                ));
            }
            // Moving in from your own space: the copy there goes, after the
            // shared one is written, so it can't bring back an old version
            // on a new device once the list's members have changed it.
            if moved && was.is_none() {
                out.push((None, item.table.clone(), "delete".into(), HashMap::new()));
            }
        }
    }
    if moved {
        plan.push((item.table.clone(), item.row_id.clone(), current));
    }
    Ok(out)
}

fn not_paired() -> CoreError {
    CoreError::File {
        detail: "This device isn't signed in to sync.".into(),
    }
}

fn incoming(row: ChangedRow) -> IncomingRow {
    IncomingRow {
        table: row.table,
        id: row.id,
        deleted: row.deleted,
        values: row
            .values
            .into_iter()
            .map(|(name, value)| (name, sync_value(&value)))
            .collect(),
        hlc: row.hlc,
    }
}

/// A column value as JSON. A real with no fraction goes as an integer, as
/// Foundation's encoder writes it; one JSON cannot hold (infinity) goes as
/// null.
pub(crate) fn json_value(value: &SyncValue) -> Value {
    match value {
        SyncValue::Null => Value::Null,
        SyncValue::Integer { value } => Value::from(*value),
        SyncValue::Real { value } => {
            if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
                Value::from(*value as i64)
            } else {
                Number::from_f64(*value).map_or(Value::Null, Value::Number)
            }
        }
        SyncValue::Text { value } => Value::String(value.clone()),
    }
}

/// A JSON value as a column value, the way the apps always read one: a whole
/// number (`1`, `1.0`, `1e3`) is an integer, any other number a real. A
/// boolean, which no column stores, is 0 or 1, and an array or object is its
/// JSON text.
pub(crate) fn sync_value(value: &Value) -> SyncValue {
    match value {
        Value::Null => SyncValue::Null,
        Value::Bool(flag) => SyncValue::Integer {
            value: i64::from(*flag),
        },
        Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                return SyncValue::Integer { value };
            }
            let real = number.as_f64().unwrap_or(0.0);
            if number.is_f64()
                && real.fract() == 0.0
                && real >= i64::MIN as f64
                && real < i64::MAX as f64
            {
                SyncValue::Integer { value: real as i64 }
            } else {
                SyncValue::Real { value: real }
            }
        }
        Value::String(text) => SyncValue::Text {
            value: text.clone(),
        },
        other => SyncValue::Text {
            value: other.to_string(),
        },
    }
}

/// The `{"error": "..."}` a refusal carries, or nothing (empty) when its
/// body is not one.
#[uniffi::export]
pub fn sync_server_message(body: String) -> String {
    serde_json::from_str::<ErrorBody>(&body)
        .map(|body| body.error)
        .unwrap_or_default()
}

/// What to show for a refusal with `message`, the server's own words: those
/// as a sentence, or its status when it gave none.
#[uniffi::export]
pub fn sync_refusal_text(status: i32, message: String) -> String {
    if message.trim().is_empty() {
        format!("The sync server answered {status}.")
    } else {
        sync_sentence(message)
    }
}

/// What to show for a refusal whose body was `body`.
#[uniffi::export]
pub fn sync_failure_message(status: i32, body: String) -> String {
    sync_refusal_text(status, sync_server_message(body))
}

/// The server and Supabase write lowercase fragments; the apps show
/// sentences: trimmed, capitalised, and closed with a full stop unless they
/// already end in one.
#[uniffi::export]
pub fn sync_sentence(message: String) -> String {
    let trimmed = message.trim();
    let mut characters = trimmed.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    let mut sentence: String = first.to_uppercase().collect();
    sentence.push_str(characters.as_str());
    if !sentence.ends_with(['.', '!', '?']) {
        sentence.push('.');
    }
    sentence
}

/// How long to wait before the next long-poll after `failures` failed cycles
/// in a row: 2, 4, 8 … seconds, capped at five minutes, so an outage costs
/// nothing.
#[uniffi::export]
pub fn sync_backoff_seconds(failures: u32) -> u32 {
    300.min(1 << failures.min(8))
}

/// `POST /v1/devices`'s body: this device on the account.
#[uniffi::export]
pub fn sync_register_device_body(id: String, name: String, platform: String) -> String {
    let mut body = Map::new();
    body.insert("id".into(), Value::String(id));
    body.insert("name".into(), Value::String(name));
    body.insert("platform".into(), Value::String(platform));
    Value::Object(body).to_string()
}

/// A device on the account, from `GET /v1/account`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SyncAccountDevice {
    pub id: String,
    pub name: Option<String>,
    pub platform: Option<String>,
    pub created_at: String,
    pub last_seen_at: Option<String>,
    /// The device asking.
    pub current: bool,
}

/// `GET /v1/account`: who this device is signed in as, and every device on
/// the account.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SyncAccountBody {
    pub account_id: String,
    pub email: Option<String>,
    pub devices: Vec<SyncAccountDevice>,
}

/// Reads `GET /v1/account`'s body. Fields the apps do not know are ignored.
#[uniffi::export]
pub fn sync_decode_account(body: String) -> Result<SyncAccountBody, CoreError> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Device {
        id: String,
        name: Option<String>,
        platform: Option<String>,
        created_at: String,
        last_seen_at: Option<String>,
        #[serde(default)]
        current: bool,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Account {
        account_id: String,
        email: Option<String>,
        #[serde(default)]
        devices: Vec<Device>,
    }
    let account: Account = serde_json::from_str(&body).map_err(|error| CoreError::File {
        detail: format!("The sync server's answer couldn't be read: {error}"),
    })?;
    Ok(SyncAccountBody {
        account_id: account.account_id,
        email: account.email,
        devices: account
            .devices
            .into_iter()
            .map(|device| SyncAccountDevice {
                id: device.id,
                name: device.name,
                platform: device.platform,
                created_at: device.created_at,
                last_seen_at: device.last_seen_at,
                current: device.current,
            })
            .collect(),
    })
}

/// A server timestamp (RFC 3339, with up to nine fractional digits, as
/// chrono writes them) in milliseconds since 1970, cut to the millisecond.
#[uniffi::export]
pub fn sync_timestamp_ms(text: String) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(text.trim())
        .ok()
        .map(|at| at.timestamp_millis())
}

/// Whether a `/health` body is a Takt sync server's `{"ok": true}`.
#[uniffi::export]
pub fn sync_health_is_ok(body: String) -> bool {
    serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|value| value.get("ok").and_then(Value::as_bool))
        == Some(true)
}

/// Whether a body is a JSON object, as Supabase's `/auth/v1/settings` is.
#[uniffi::export]
pub fn sync_body_is_json_object(body: String) -> bool {
    matches!(serde_json::from_str::<Value>(&body), Ok(Value::Object(_)))
}
