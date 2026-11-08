//! The sync engine's side of the workspace database: what the device
//! remembers about its pairing, the outbox of local changes coalesced for a
//! push, and applying a pull. `wire` makes the push body and reads the pulled
//! pages, stamping and receiving the clock on the way, and `endpoints` reads
//! typed server addresses. The transport (HTTP, auth, the long poll) stays
//! with each client. Replaces `WorkspaceStore+Sync.swift` and Kotlin's
//! `SyncStore`.
//!
//! Writes made while applying a pull run with `sync_control.applying` on, so
//! the outbox triggers do not echo them back to the server.

use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension, Transaction, params, params_from_iter};

use crate::CoreError;
use crate::schema::triggers::{SYNCED_TABLES, WHOLE_ROW_TABLES};
use crate::time::stored;

/// One SQLite value as it travels: null, a number or text, as the column
/// stores it. A blob travels as base64 text.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum SyncValue {
    Null,
    Integer { value: i64 },
    Real { value: f64 },
    Text { value: String },
}

impl SyncValue {
    fn from_ref(value: ValueRef<'_>) -> SyncValue {
        match value {
            ValueRef::Null => SyncValue::Null,
            ValueRef::Integer(value) => SyncValue::Integer { value },
            ValueRef::Real(value) => SyncValue::Real { value },
            ValueRef::Text(text) => SyncValue::Text {
                value: String::from_utf8_lossy(text).into_owned(),
            },
            ValueRef::Blob(bytes) => SyncValue::Text {
                value: base64(bytes),
            },
        }
    }

    fn to_sql(&self) -> rusqlite::types::Value {
        use rusqlite::types::Value;
        match self {
            SyncValue::Null => Value::Null,
            SyncValue::Integer { value } => Value::Integer(*value),
            SyncValue::Real { value } => Value::Real(*value),
            SyncValue::Text { value } => Value::Text(value.clone()),
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            SyncValue::Text { value } => Some(value),
            _ => None,
        }
    }
}

/// A row's local change, coalesced from its outbox entries and read from the
/// live row, ready to be stamped and pushed.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OutgoingChange {
    pub table: String,
    pub row_id: String,
    /// "upsert" or "delete".
    pub operation: String,
    /// The columns to push and their current values; empty for a delete.
    pub values: HashMap<String, SyncValue>,
    /// When the newest of the coalesced edits was made.
    pub changed_at_ms: i64,
}

/// A batch of outgoing changes and the newest outbox entry folded in, which
/// is what to acknowledge once the server has them.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PendingChanges {
    pub changes: Vec<OutgoingChange>,
    pub through_seq: Option<i64>,
}

/// A row as the server holds it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct IncomingRow {
    pub table: String,
    pub id: String,
    pub deleted: bool,
    pub values: HashMap<String, SyncValue>,
    pub hlc: Option<String>,
}

/// What the device remembers about its sync.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LocalSyncState {
    pub device_id: String,
    pub cursor: i64,
    pub hlc: Option<String>,
    pub server_url: Option<String>,
    pub canonical_workspace_id: Option<String>,
    pub needs_snapshot: bool,
    pub last_synced_at_ms: Option<i64>,
    pub is_recording: bool,
}

fn sync_key(table: &str) -> Option<&'static str> {
    SYNCED_TABLES
        .iter()
        .find(|(name, _)| *name == table)
        .map(|(_, key)| *key)
}

/// The device's sync state, or nothing while it has never been paired.
pub fn state(connection: &Connection) -> Result<Option<LocalSyncState>, CoreError> {
    let recording: i64 = connection
        .query_row(
            "SELECT recording FROM sync_control WHERE id = 0",
            [],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0);
    Ok(connection
        .query_row(
            "SELECT deviceId, cursor, hlc, serverURL, canonicalWorkspaceId, needsSnapshot, lastSyncedAt
             FROM sync_state WHERE id = 0",
            [],
            |row| {
                Ok(LocalSyncState {
                    device_id: row.get(0)?,
                    cursor: row.get(1)?,
                    hlc: row.get(2)?,
                    server_url: row.get(3)?,
                    canonical_workspace_id: row.get(4)?,
                    needs_snapshot: row.get::<_, i64>(5)? != 0,
                    last_synced_at_ms: row
                        .get::<_, Option<String>>(6)?
                        .as_deref()
                        .and_then(crate::time::parse_stored)
                        .map(|at| at.timestamp_millis()),
                    is_recording: recording != 0,
                })
            },
        )
        .optional()?)
}

/// Pairs the store with a server. The first cycle afterwards snapshots every
/// row and pulls before it pushes, so the device adopts what the server holds.
pub fn begin(
    transaction: &Transaction,
    device_id: &str,
    server_url: &str,
) -> Result<(), CoreError> {
    transaction.execute(
        "INSERT INTO sync_state (id, deviceId, serverURL, cursor, needsSnapshot) VALUES (0, ?1, ?2, 0, 1)
         ON CONFLICT(id) DO UPDATE SET deviceId = excluded.deviceId, serverURL = excluded.serverURL,
           cursor = 0, needsSnapshot = 1, hlc = NULL, canonicalWorkspaceId = NULL",
        params![device_id, server_url],
    )?;
    Ok(())
}

/// The newest outbox entry, or nothing when nothing is waiting.
pub fn latest_outbox_seq(connection: &Connection) -> Result<Option<i64>, CoreError> {
    Ok(connection.query_row("SELECT MAX(seq) FROM sync_outbox", [], |row| row.get(0))?)
}

/// Unpairs: stops recording and forgets what was waiting. The workspace is
/// untouched.
pub fn end(transaction: &Transaction) -> Result<(), CoreError> {
    transaction.execute_batch(
        "UPDATE sync_control SET recording = 0, applying = 0 WHERE id = 0;
         DELETE FROM sync_outbox;
         DELETE FROM sync_state;",
    )?;
    Ok(())
}

/// Turns recording on and queues every existing row, parents first, as an
/// insert. Run once, by the first cycle after pairing.
pub fn enqueue_snapshot(transaction: &Transaction, now_ms: i64) -> Result<(), CoreError> {
    transaction.execute("UPDATE sync_control SET recording = 1 WHERE id = 0", [])?;
    for (table, key) in SYNCED_TABLES {
        transaction.execute(
            &format!(
                "INSERT INTO sync_outbox(tableName, rowId, operation, changedJSON, changedAtMs)
                 SELECT ?1, \"{key}\", 'insert', NULL, ?2 FROM {table}"
            ),
            params![table, now_ms],
        )?;
    }
    transaction.execute("UPDATE sync_state SET needsSnapshot = 0 WHERE id = 0", [])?;
    Ok(())
}

/// The outbox, coalesced per row and read from the live rows, up to `limit`
/// rows; a row's entries are never split across two batches.
pub fn pending_changes(connection: &Connection, limit: u32) -> Result<PendingChanges, CoreError> {
    struct Pending {
        table: String,
        row_id: String,
        last_operation: String,
        every_column: bool,
        columns: HashSet<String>,
        changed_at_ms: i64,
    }
    let mut statement = connection.prepare(
        "SELECT seq, tableName, rowId, operation, changedJSON, changedAtMs FROM sync_outbox ORDER BY seq",
    )?;
    let entries: Vec<(i64, String, String, String, Option<String>, i64)> = statement
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    if entries.is_empty() {
        return Ok(PendingChanges {
            changes: vec![],
            through_seq: None,
        });
    }
    let mut order: Vec<String> = Vec::new();
    let mut pending: HashMap<String, Pending> = HashMap::new();
    let mut through_seq = 0;
    for (seq, table, row_id, operation, changed_json, changed_at_ms) in entries {
        let key = format!("{table}\u{1F}{row_id}");
        if !pending.contains_key(&key) {
            if order.len() as u32 == limit {
                break;
            }
            order.push(key.clone());
            pending.insert(
                key.clone(),
                Pending {
                    table: table.clone(),
                    row_id: row_id.clone(),
                    last_operation: operation.clone(),
                    every_column: false,
                    columns: HashSet::new(),
                    changed_at_ms: 0,
                },
            );
        }
        through_seq = seq;
        let item = pending.get_mut(&key).expect("just inserted");
        item.last_operation = operation.clone();
        item.changed_at_ms = item.changed_at_ms.max(changed_at_ms);
        match operation.as_str() {
            "insert" => item.every_column = true,
            _ if WHOLE_ROW_TABLES.contains(&table.as_str()) => item.every_column = true,
            "update" => {
                if let Some(names) = changed_json
                    .as_deref()
                    .and_then(|json| serde_json::from_str::<Vec<serde_json::Value>>(json).ok())
                {
                    item.columns.extend(
                        names
                            .iter()
                            .filter_map(|name| name.as_str().map(str::to_string)),
                    );
                }
            }
            _ => {}
        }
    }
    let mut changes = Vec::new();
    for key in order {
        let item = &pending[&key];
        let Some(key_column) = sync_key(&item.table) else {
            continue;
        };
        let live = if item.last_operation == "delete" {
            None
        } else {
            let mut statement = connection.prepare(&format!(
                "SELECT * FROM {} WHERE \"{key_column}\" = ?1",
                item.table
            ))?;
            let names: Vec<String> = statement
                .column_names()
                .iter()
                .map(|n| n.to_string())
                .collect();
            statement
                .query_row([&item.row_id], |row| {
                    let mut values = HashMap::new();
                    for (index, name) in names.iter().enumerate() {
                        if item.every_column || item.columns.contains(name) || name == key_column {
                            values.insert(name.clone(), SyncValue::from_ref(row.get_ref(index)?));
                        }
                    }
                    Ok(values)
                })
                .optional()?
        };
        changes.push(match live {
            Some(values) => OutgoingChange {
                table: item.table.clone(),
                row_id: item.row_id.clone(),
                operation: "upsert".into(),
                values,
                changed_at_ms: item.changed_at_ms,
            },
            None => OutgoingChange {
                table: item.table.clone(),
                row_id: item.row_id.clone(),
                operation: "delete".into(),
                values: HashMap::new(),
                changed_at_ms: item.changed_at_ms,
            },
        });
    }
    Ok(PendingChanges {
        changes,
        through_seq: Some(through_seq),
    })
}

/// Forgets the outbox entries the server has accepted.
pub fn acknowledge(transaction: &Transaction, through_seq: i64) -> Result<(), CoreError> {
    transaction.execute("DELETE FROM sync_outbox WHERE seq <= ?1", [through_seq])?;
    Ok(())
}

/// Advances the stored cursor and clock and the last-synced time without
/// applying rows.
pub fn record_progress(
    transaction: &Transaction,
    cursor: Option<i64>,
    hlc: Option<&str>,
    now_ms: i64,
) -> Result<(), CoreError> {
    transaction.execute(
        "UPDATE sync_state SET cursor = COALESCE(?1, cursor), hlc = COALESCE(?2, hlc), lastSyncedAt = ?3 WHERE id = 0",
        params![cursor, hlc, stored(now_ms)],
    )?;
    Ok(())
}

/// Writes a pull into the workspace, then folds any second workspace into the
/// canonical one and removes rows orphaned by deletions elsewhere. A row with
/// a local edit still waiting to be pushed is left alone: it is newer, and the
/// server settles it next cycle. Returns whether anything changed.
pub fn apply_remote_rows(
    transaction: &Transaction,
    rows: &[IncomingRow],
    cursor: i64,
    hlc: Option<&str>,
    now_ms: i64,
) -> Result<bool, CoreError> {
    let rows: Vec<(&IncomingRow, Option<&str>)> = rows.iter().map(|row| (row, None)).collect();
    apply_rows(transaction, &rows, None, cursor, hlc, now_ms)
}

/// [`apply_remote_rows`] for rows that each name the space they came from
/// (nothing for the account's own), with the shared spaces the server
/// listed beside them when it did. See `spaces.rs` for what a space changes.
pub(crate) fn apply_rows(
    transaction: &Transaction,
    rows: &[(&IncomingRow, Option<&str>)],
    shared: Option<&[takt_sync_rules::wire::WireSpace]>,
    cursor: i64,
    hlc: Option<&str>,
    now_ms: i64,
) -> Result<bool, CoreError> {
    transaction.execute_batch("PRAGMA defer_foreign_keys = ON")?;
    let mut changed = false;
    if let Some(shared) = shared {
        changed |= spaces::store_spaces(transaction, shared)?;
    }
    let sharing = spaces::active(transaction)?;
    set_applying(transaction, true)?;
    // Applied after everything else: see `spaces.rs`, "Tombstones".
    let mut shared_tombstones: Vec<(&IncomingRow, &str)> = Vec::new();
    // Personal columns for a row that hasn't arrived yet.
    let mut companions: Vec<(&IncomingRow, &'static str)> = Vec::new();
    let applied = (|| -> Result<(), CoreError> {
        for (row, space) in rows {
            if let Some(base) = spaces::companion_base(&row.table) {
                let Some(base) = SYNCED_TABLES
                    .iter()
                    .map(|(name, _)| *name)
                    .find(|name| *name == base)
                else {
                    continue;
                };
                if !row.deleted && !waiting(transaction, base, &row.id)? {
                    match apply_companion(transaction, row, base)? {
                        Some(applied) => changed |= applied,
                        None => companions.push((row, base)),
                    }
                }
                continue;
            }
            let Some(key_column) = sync_key(&row.table) else {
                continue;
            };
            if waiting(transaction, &row.table, &row.id)? {
                continue;
            }
            match space {
                Some(space) if row.deleted => shared_tombstones.push((row, space)),
                Some(space) => {
                    upsert_shared(transaction, row, key_column, now_ms)?;
                    changed |= transaction.changes() > 0;
                    spaces::record(transaction, &row.table, &row.id, Some(space))?;
                }
                // A copy in the account's own space of a row that now lives
                // in a shared list: left over from before it was shared, or a
                // tidy-up after a member's delete. The shared space speaks for
                // it; only the row's personal columns, and a move out of the
                // list, come from here.
                None if sharing
                    && exists(transaction, &row.table, key_column, &row.id)?
                    && spaces::current_space(transaction, &row.table, &row.id)?
                        .is_some_and(|here| !moves_out(transaction, row, &here)) =>
                {
                    if !row.deleted {
                        changed |= apply_personal_part(transaction, row, key_column)?;
                    }
                }
                None if row.deleted => {
                    transaction.execute(
                        &format!("DELETE FROM {} WHERE \"{key_column}\" = ?1", row.table),
                        [&row.id],
                    )?;
                    changed |= transaction.changes() > 0;
                    if sharing {
                        spaces::record(transaction, &row.table, &row.id, None)?;
                    }
                }
                None => {
                    upsert_remote(transaction, row, key_column)?;
                    changed |= transaction.changes() > 0;
                }
            }
        }
        for (row, base) in companions.drain(..) {
            changed |= apply_companion(transaction, row, base)?.unwrap_or(false);
        }
        for (row, space) in shared_tombstones.drain(..) {
            changed |= apply_shared_tombstone(transaction, row, space)?;
        }
        Ok(())
    })();
    set_applying(transaction, false)?;
    applied?;
    changed |= adopt_workspaces(transaction, now_ms)?;
    changed |= remove_orphans(transaction)?;
    transaction.execute(
        "UPDATE sync_state SET cursor = ?1, hlc = COALESCE(?2, hlc), lastSyncedAt = ?3 WHERE id = 0",
        params![cursor, hlc, stored(now_ms)],
    )?;
    Ok(changed)
}

/// Whether a local edit of the row still waits to be pushed.
fn waiting(transaction: &Transaction, table: &str, id: &str) -> Result<bool, CoreError> {
    Ok(transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM sync_outbox WHERE tableName = ?1 AND rowId = ?2)",
        params![table, id],
        |r| r.get(0),
    )?)
}

/// A row from a shared list's space: everything but the personal columns,
/// which are each member's own. A list arriving for the first time goes at
/// the end of the sidebar's top level, in this device's workspace.
fn upsert_shared(
    transaction: &Transaction,
    row: &IncomingRow,
    key_column: &str,
    now_ms: i64,
) -> Result<(), CoreError> {
    let mut shared = row.clone();
    shared
        .values
        .retain(|name, _| !spaces::is_personal_column(&row.table, name));
    if row.table == "task_lists" {
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM task_lists WHERE id = ?1)",
            [&row.id],
            |r| r.get(0),
        )?;
        if !exists {
            let workspace = home_workspace(transaction)?;
            let order: i64 = transaction.query_row(
                "SELECT COALESCE(MAX(sortOrder) + 1, 0) FROM task_lists
                 WHERE workspaceId IS ?1 AND folderId IS NULL",
                [&workspace],
                |r| r.get(0),
            )?;
            if let Some(workspace) = workspace {
                shared
                    .values
                    .insert("workspaceId".into(), SyncValue::Text { value: workspace });
            }
            shared
                .values
                .insert("sortOrder".into(), SyncValue::Integer { value: order });
            shared
                .values
                .entry("createdAt".into())
                .or_insert_with(|| SyncValue::Text {
                    value: stored(now_ms),
                });
            shared
                .values
                .entry("updatedAt".into())
                .or_insert_with(|| SyncValue::Text {
                    value: stored(now_ms),
                });
        }
    }
    upsert_remote(transaction, &shared, key_column)
}

/// The workspace a shared list lands in: the canonical one, or the oldest.
fn home_workspace(transaction: &Transaction) -> Result<Option<String>, CoreError> {
    let canonical: Option<String> = transaction
        .query_row(
            "SELECT canonicalWorkspaceId FROM sync_state WHERE id = 0
               AND canonicalWorkspaceId IN (SELECT id FROM workspaces)",
            [],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    match canonical {
        Some(id) => Ok(Some(id)),
        None => Ok(transaction
            .query_row(
                "SELECT id FROM workspaces ORDER BY createdAt LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?),
    }
}

/// A shared row's personal columns, from the account's own space. Nothing
/// (`None`) while the row itself hasn't arrived.
fn apply_companion(
    transaction: &Transaction,
    row: &IncomingRow,
    base: &str,
) -> Result<Option<bool>, CoreError> {
    let Some(key_column) = sync_key(base) else {
        return Ok(Some(false));
    };
    let exists: bool = transaction.query_row(
        &format!("SELECT EXISTS(SELECT 1 FROM {base} WHERE \"{key_column}\" = ?1)"),
        [&row.id],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    let mut personal = row.clone();
    personal.table = base.to_string();
    personal
        .values
        .retain(|name, _| spaces::is_personal_column(base, name));
    upsert_remote(transaction, &personal, key_column)?;
    Ok(Some(transaction.changes() > 0))
}

/// A delete from a shared list's space, for a row that still belongs there.
/// It is passed on to the account's own space as well, with recording on,
/// so a copy of the row left there from before it was shared goes too.
fn apply_shared_tombstone(
    transaction: &Transaction,
    row: &IncomingRow,
    space: &str,
) -> Result<bool, CoreError> {
    let Some(key_column) = sync_key(&row.table) else {
        return Ok(false);
    };
    let exists: bool = transaction.query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM {} WHERE \"{key_column}\" = ?1)",
            row.table
        ),
        [&row.id],
        |r| r.get(0),
    )?;
    if !exists {
        spaces::record(transaction, &row.table, &row.id, None)?;
        return Ok(false);
    }
    if spaces::current_space(transaction, &row.table, &row.id)?.as_deref() != Some(space) {
        // It has moved on since; its new home's row is what counts. The old
        // space no longer holds it, so moving it back in sends it whole.
        if spaces::recorded_space(transaction, &row.table, &row.id)?.as_deref() == Some(space) {
            spaces::record(transaction, &row.table, &row.id, None)?;
        }
        return Ok(false);
    }
    transaction.execute(
        &format!("DELETE FROM {} WHERE \"{key_column}\" = ?1", row.table),
        [&row.id],
    )?;
    let recording: bool =
        transaction.query_row("SELECT recording FROM sync_control WHERE id = 0", [], |r| {
            r.get(0)
        })?;
    if recording {
        // Gone from the shared space already: the push only tidies the
        // account's own copy (`''` says so).
        spaces::record(transaction, &row.table, &row.id, Some(""))?;
        transaction.execute(
            "INSERT INTO sync_outbox (tableName, rowId, operation, changedJSON, changedAtMs)
             VALUES (?1, ?2, 'delete', NULL, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))",
            params![row.table, row.id],
        )?;
    } else {
        spaces::record(transaction, &row.table, &row.id, None)?;
    }
    Ok(true)
}

fn exists(
    transaction: &Transaction,
    table: &str,
    key_column: &str,
    id: &str,
) -> Result<bool, CoreError> {
    Ok(transaction.query_row(
        &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE \"{key_column}\" = ?1)"),
        [id],
        |r| r.get(0),
    )?)
}

/// Whether an own-space copy of a task in shared list `space` moves it out:
/// its list is one that isn't that space's.
fn moves_out(transaction: &Transaction, row: &IncomingRow, space: &str) -> bool {
    // A tombstone still carries the row's last values, which say nothing of
    // where it lives now.
    if row.deleted {
        return false;
    }
    match spaces::incoming_list_id(&row.table, &row.values) {
        Some(list) => {
            spaces::space_of_list(transaction, list)
                .ok()
                .flatten()
                .as_deref()
                != Some(space)
        }
        None => false,
    }
}

/// Writes only a row's personal columns over the live row.
fn apply_personal_part(
    transaction: &Transaction,
    row: &IncomingRow,
    key_column: &str,
) -> Result<bool, CoreError> {
    let values: HashMap<String, SyncValue> = row
        .values
        .iter()
        .filter(|(name, _)| spaces::is_personal_column(&row.table, name))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    if values.is_empty() {
        return Ok(false);
    }
    let part = IncomingRow {
        table: row.table.clone(),
        id: row.id.clone(),
        deleted: false,
        values,
        hlc: row.hlc.clone(),
    };
    upsert_remote(transaction, &part, key_column)?;
    Ok(transaction.changes() > 0)
}

/// Every column of the live row, or nothing when it is gone.
pub(crate) fn live_values(
    connection: &Connection,
    table: &str,
    id: &str,
) -> Result<Option<HashMap<String, SyncValue>>, CoreError> {
    let Some(key_column) = sync_key(table) else {
        return Ok(None);
    };
    let mut statement = connection.prepare(&format!(
        "SELECT * FROM {table} WHERE \"{key_column}\" = ?1"
    ))?;
    let names: Vec<String> = statement
        .column_names()
        .iter()
        .map(|n| n.to_string())
        .collect();
    Ok(statement
        .query_row([id], |row| {
            let mut values = HashMap::new();
            for (index, name) in names.iter().enumerate() {
                values.insert(name.clone(), SyncValue::from_ref(row.get_ref(index)?));
            }
            Ok(values)
        })
        .optional()?)
}

pub(crate) fn key_of(table: &str) -> Option<&'static str> {
    sync_key(table)
}

fn set_applying(transaction: &Transaction, on: bool) -> Result<(), CoreError> {
    transaction.execute("UPDATE sync_control SET applying = ?1 WHERE id = 0", [on])?;
    Ok(())
}

fn columns(connection: &Connection, table: &str) -> Result<HashSet<String>, CoreError> {
    let mut statement = connection.prepare("SELECT name FROM pragma_table_info(?1)")?;
    let names = statement
        .query_map([table], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(names)
}

fn upsert_remote(
    transaction: &Transaction,
    row: &IncomingRow,
    key_column: &str,
) -> Result<(), CoreError> {
    let table_columns = columns(transaction, &row.table)?;
    let values: BTreeMap<&String, &SyncValue> = row
        .values
        .iter()
        .filter(|(name, _)| table_columns.contains(*name) && name.as_str() != key_column)
        .collect();
    let exists: bool = transaction.query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM {} WHERE \"{key_column}\" = ?1)",
            row.table
        ),
        [&row.id],
        |r| r.get(0),
    )?;
    if exists {
        if values.is_empty() {
            return Ok(());
        }
        let assignments: Vec<String> = values
            .keys()
            .map(|name| format!("\"{name}\" = ?"))
            .collect();
        let mut arguments: Vec<rusqlite::types::Value> =
            values.values().map(|v| v.to_sql()).collect();
        arguments.push(rusqlite::types::Value::Text(row.id.clone()));
        transaction.execute(
            &format!(
                "UPDATE {} SET {} WHERE \"{key_column}\" = ?",
                row.table,
                assignments.join(", ")
            ),
            params_from_iter(arguments),
        )?;
        return Ok(());
    }
    let mut names = vec![key_column.to_string()];
    names.extend(values.keys().map(|name| name.to_string()));
    let mut arguments = vec![rusqlite::types::Value::Text(row.id.clone())];
    arguments.extend(values.values().map(|v| v.to_sql()));
    let quoted: Vec<String> = names.iter().map(|name| format!("\"{name}\"")).collect();
    let placeholders = vec!["?"; names.len()].join(", ");
    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({placeholders})",
        row.table,
        quoted.join(", ")
    );
    match transaction.execute(&sql, params_from_iter(arguments.clone())) {
        Ok(_) => Ok(()),
        Err(rusqlite::Error::SqliteFailure(error, message)) if error.extended_code == 2067 => {
            // Another key already names this row locally: two devices logged
            // the same daily on the same day, or imported the same source.
            // The smaller id wins on every device; the loser goes with a
            // tombstone.
            match resolve_unique_rival(transaction, row, &sql, &arguments)? {
                Rival::None => Err(rusqlite::Error::SqliteFailure(error, message).into()),
                Rival::Removed(loser) => {
                    transaction.execute(&sql, params_from_iter(arguments))?;
                    if let Some(loser) = loser {
                        merge_contribution(transaction, &row.id, &loser)?;
                    }
                    Ok(())
                }
                Rival::Inserted | Rival::Kept => Ok(()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

enum Rival {
    None,
    Removed(Option<IncomingRow>),
    Inserted,
    Kept,
}

fn resolve_unique_rival(
    transaction: &Transaction,
    row: &IncomingRow,
    insert_sql: &str,
    arguments: &[rusqlite::types::Value],
) -> Result<Rival, CoreError> {
    let text = |column: &str| {
        row.values
            .get(column)
            .and_then(SyncValue::text)
            .map(str::to_string)
    };
    let rival: Option<String> = match row.table.as_str() {
        "daily_contributions" => transaction
            .query_row(
                "SELECT id FROM daily_contributions WHERE dailyId = ?1 AND dayKey = ?2",
                params![text("dailyId"), text("dayKey")],
                |r| r.get(0),
            )
            .optional()?,
        "tasks" => transaction
            .query_row(
                "SELECT id FROM tasks WHERE sourceSystem IS ?1 AND sourceId = ?2",
                params![text("sourceSystem"), text("sourceId")],
                |r| r.get(0),
            )
            .optional()?,
        "task_lists" => transaction
            .query_row(
                "SELECT id FROM task_lists WHERE workspaceId = ?1 AND systemRole = ?2",
                params![text("workspaceId"), text("systemRole")],
                |r| r.get(0),
            )
            .optional()?,
        _ => None,
    };
    let (Some(rival), Some(key_column)) = (rival, sync_key(&row.table)) else {
        return Ok(Rival::None);
    };
    if rival == row.id {
        return Ok(Rival::None);
    }
    set_applying(transaction, false)?;
    let result = (|| -> Result<Rival, CoreError> {
        if row.table != "task_lists" {
            if rival < row.id {
                transaction.execute(
                    "INSERT INTO sync_outbox (tableName, rowId, operation, changedJSON, changedAtMs)
                     VALUES (?1, ?2, 'delete', NULL, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))",
                    params![row.table, row.id],
                )?;
                merge_contribution(transaction, &rival, row)?;
                return Ok(Rival::Kept);
            }
            let mut loser = None;
            if row.table == "daily_contributions"
                && let Some((seconds, completed, skipped)) = transaction
                    .query_row(
                        "SELECT secondsLogged, completedAt, skippedAt FROM daily_contributions WHERE id = ?1",
                        [&rival],
                        |r| {
                            Ok((
                                r.get::<_, i64>(0)?,
                                r.get::<_, Option<String>>(1)?,
                                r.get::<_, Option<String>>(2)?,
                            ))
                        },
                    )
                    .optional()?
            {
                let mut values = HashMap::from([(
                    "secondsLogged".to_string(),
                    SyncValue::Integer { value: seconds },
                )]);
                if let Some(completed) = completed {
                    values.insert("completedAt".into(), SyncValue::Text { value: completed });
                }
                if let Some(skipped) = skipped {
                    values.insert("skippedAt".into(), SyncValue::Text { value: skipped });
                }
                loser = Some(IncomingRow {
                    table: row.table.clone(),
                    id: rival.clone(),
                    deleted: false,
                    values,
                    hlc: None,
                });
            }
            transaction.execute(
                &format!("DELETE FROM {} WHERE \"{key_column}\" = ?1", row.table),
                [&rival],
            )?;
            return Ok(Rival::Removed(loser));
        }
        // A rival Inbox keeps its tasks: they move into the incoming one.
        transaction.execute(
            "UPDATE task_lists SET systemRole = NULL WHERE id = ?1",
            [&rival],
        )?;
        set_applying(transaction, true)?;
        transaction.execute(insert_sql, params_from_iter(arguments.iter().cloned()))?;
        set_applying(transaction, false)?;
        transaction.execute(
            "UPDATE tasks SET listId = ?1 WHERE listId = ?2",
            params![row.id, rival],
        )?;
        transaction.execute("DELETE FROM task_lists WHERE id = ?1", [&rival])?;
        Ok(Rival::Inserted)
    })();
    set_applying(transaction, true)?;
    result
}

/// A day ticked on two devices keeps the most time either logged and the
/// earlier finish. Runs with recording on, so the combined row syncs.
fn merge_contribution(
    transaction: &Transaction,
    into: &str,
    row: &IncomingRow,
) -> Result<(), CoreError> {
    if row.table != "daily_contributions" {
        return Ok(());
    }
    let seconds = match row.values.get("secondsLogged") {
        Some(SyncValue::Integer { value }) => Some(*value),
        _ => None,
    };
    let completed = row
        .values
        .get("completedAt")
        .and_then(SyncValue::text)
        .map(str::to_string);
    let skipped = row
        .values
        .get("skippedAt")
        .and_then(SyncValue::text)
        .map(str::to_string);
    set_applying(transaction, false)?;
    let result = transaction.execute(
        "UPDATE daily_contributions SET
           secondsLogged = MAX(secondsLogged, COALESCE(?1, 0)),
           completedAt = CASE WHEN completedAt IS NULL THEN ?2 WHEN ?2 IS NULL THEN completedAt
             ELSE MIN(completedAt, ?2) END,
           skippedAt = CASE WHEN skippedAt IS NULL THEN ?4 WHEN ?4 IS NULL THEN skippedAt
             ELSE MIN(skippedAt, ?4) END
         WHERE id = ?3
           AND (secondsLogged < COALESCE(?1, 0) OR (?2 IS NOT NULL AND (completedAt IS NULL OR completedAt > ?2))
                OR (?4 IS NOT NULL AND (skippedAt IS NULL OR skippedAt > ?4)))",
        params![seconds, completed, into, skipped],
    );
    set_applying(transaction, true)?;
    result?;
    Ok(())
}

/// Folds every workspace but the canonical one into it: the first one this
/// device received from the server, or its own when the server had none.
fn adopt_workspaces(transaction: &Transaction, now_ms: i64) -> Result<bool, CoreError> {
    let mut statement = transaction.prepare("SELECT id FROM workspaces ORDER BY createdAt")?;
    let ids: Vec<String> = statement
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut canonical: Option<String> = transaction
        .query_row(
            "SELECT canonicalWorkspaceId FROM sync_state WHERE id = 0",
            [],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    if canonical.as_ref().is_none_or(|id| !ids.contains(id)) {
        let remote: Option<String> = transaction
            .query_row(
                "SELECT id FROM workspaces
                 WHERE id NOT IN (SELECT rowId FROM sync_outbox WHERE tableName = 'workspaces')
                 ORDER BY createdAt LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        canonical = remote.or_else(|| ids.first().cloned());
        transaction.execute(
            "UPDATE sync_state SET canonicalWorkspaceId = ?1 WHERE id = 0",
            [&canonical],
        )?;
    }
    let Some(canonical) = canonical else {
        return Ok(false);
    };
    let others: Vec<&String> = ids.iter().filter(|id| **id != canonical).collect();
    if others.is_empty() {
        return Ok(false);
    }
    let inbox_of = |workspace: &str| -> Result<Option<String>, CoreError> {
        Ok(transaction
            .query_row(
                "SELECT id FROM task_lists WHERE workspaceId = ?1 AND systemRole = 'inbox'",
                [workspace],
                |r| r.get(0),
            )
            .optional()?)
    };
    let canonical_inbox = inbox_of(&canonical)?;
    let now = stored(now_ms);
    for other in others {
        if let Some(inbox) = inbox_of(other)? {
            match &canonical_inbox {
                Some(canonical_inbox) => {
                    transaction.execute(
                        "UPDATE tasks SET listId = ?1 WHERE listId = ?2",
                        params![canonical_inbox, inbox],
                    )?;
                    transaction.execute("DELETE FROM task_lists WHERE id = ?1", [&inbox])?;
                }
                None => {
                    transaction.execute(
                        "UPDATE task_lists SET workspaceId = ?1 WHERE id = ?2",
                        params![canonical, inbox],
                    )?;
                }
            }
        }
        for table in ["list_folders", "task_lists", "filter_lists"] {
            transaction.execute(
                &format!(
                    "UPDATE {table} SET workspaceId = ?1, updatedAt = ?2 WHERE workspaceId = ?3"
                ),
                params![canonical, now, other],
            )?;
        }
        // Before its duplicate conditions go, so the context can be put in
        // terms of the canonical workspace's.
        crate::focus_context::adopt(transaction, other, &canonical)?;
        crate::conditions::adopt_links(transaction, other, &canonical)?;
        transaction.execute(
            "DELETE FROM task_conditions WHERE workspaceId = ?1
               AND name IN (SELECT name FROM task_conditions WHERE workspaceId = ?2)",
            params![other, canonical],
        )?;
        transaction.execute(
            "UPDATE task_conditions SET workspaceId = ?1 WHERE workspaceId = ?2",
            params![canonical, other],
        )?;
        transaction.execute("DELETE FROM workspaces WHERE id = ?1", [other])?;
    }
    Ok(true)
}

/// Deletes rows whose parent was deleted on another device, repeating until
/// the foreign keys check clean. Recording is on, so the deletions sync.
fn remove_orphans(transaction: &Transaction) -> Result<bool, CoreError> {
    let mut removed = false;
    for _ in 0..16 {
        let mut statement =
            transaction.prepare("SELECT \"table\", rowid FROM pragma_foreign_key_check")?;
        let violations: Vec<(String, i64)> = statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        if violations.is_empty() {
            break;
        }
        for (table, rowid) in violations {
            transaction.execute(&format!("DELETE FROM {table} WHERE rowid = ?1"), [rowid])?;
            removed = true;
        }
    }
    Ok(removed)
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].iter().enumerate() {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub mod endpoints;
pub mod spaces;
#[cfg(test)]
mod spaces_tests;
#[cfg(test)]
mod tests;
pub mod wire;
#[cfg(test)]
mod wire_tests;

/// The stamp for a local edit at `wall_ms`, after `clock` (absent before the
/// first push, when the clock starts at zero on `device_id`). Absent when
/// `clock` is not a stamp. `HybridLogicalClock.tick`, shared with the server
/// through `takt-sync-rules`.
#[uniffi::export]
pub fn hlc_tick(clock: Option<String>, device_id: String, wall_ms: i64) -> Option<String> {
    let start = match clock {
        Some(text) => takt_sync_rules::hlc::Hlc::parse(&text)?,
        None => takt_sync_rules::hlc::Hlc::new(0, 0, device_id),
    };
    Some(start.tick(wall_ms).to_string())
}

/// `clock` moved past `remote`, a stamp another device made, at `wall_ms`.
/// Absent when either is not a stamp. `HybridLogicalClock.receiving`.
#[uniffi::export]
pub fn hlc_receive(clock: String, remote: String, wall_ms: i64) -> Option<String> {
    let local = takt_sync_rules::hlc::Hlc::parse(&clock)?;
    let remote = takt_sync_rules::hlc::Hlc::parse(&remote)?;
    Some(local.receiving(&remote, wall_ms).to_string())
}
