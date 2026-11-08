//! `POST /v1/push`: a device's local changes, merged into the stored rows.

use crate::AppState;
use crate::auth::{Caller, DEVICE_HEADER};
use crate::error::{AppError, Result};
use crate::merge::{self, Change, Op, Outcome, StoredRow};
use crate::{realtime, spaces};
use axum::extract::State;
use axum::{Extension, Json};
use serde_json::{Map, Value};
use sqlx::types::Json as Jsonb;
use sqlx::{Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use takt_sync_rules::wire::{PushRequest, PushResponse, WireChange, WireOp};
use uuid::Uuid;

/// Serialises every push against every other, across every account.
///
/// Row locks alone would keep two pushes from corrupting one row, but not the
/// feed. Both would draw `seq`s from the one sequence and could commit out of
/// order: a reader that saw seq 6 commit before seq 5 would move its cursor
/// past 5 and never see it. Holding one transaction-scoped lock from before
/// the first `nextval` until commit makes `seq` order commit order, which is
/// the property the cursor depends on.
///
/// It used to be per account, when a device read only its own account's
/// rows. A device now reads its own space and every shared list it is in,
/// with one cursor, so the order has to hold across all of them; and since
/// lists link accounts to each other, only one lock for everything does
/// that. A push is a few milliseconds of row writes, so at this server's
/// size the serialisation costs nothing measurable.
const PUSH_LOCK_SQL: &str = "SELECT pg_advisory_xact_lock(hashtextextended('push', 0))";

/// Takes the push lock for the rest of `tx`. Anything that gives rows new
/// `seq`s — a push, joining a list — holds it.
pub async fn lock(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    sqlx::query(PUSH_LOCK_SQL).execute(&mut **tx).await?;
    Ok(())
}

/// Longest table name or row id accepted. Generous for uuids and SQLite table
/// names; only there to stop a broken client storing megabyte keys.
const MAX_KEY_LEN: usize = 256;

/// Checks a change before anything is written, so a batch is applied whole
/// or refused whole: the client keeps its outbox and the bug shows.
fn validate(change: WireChange) -> Result<(Option<Uuid>, Change)> {
    let where_ = || format!("{} {}", change.table, change.id);
    let space = match change.space.as_deref() {
        None => None,
        Some(text) => Some(
            Uuid::parse_str(text)
                .map_err(|_| AppError::BadRequest(format!("bad space {text:?} on {}", where_())))?,
        ),
    };
    if change.table.is_empty() || change.table.len() > MAX_KEY_LEN {
        return Err(AppError::BadRequest(format!(
            "bad table name {:?}",
            change.table
        )));
    }
    if change.id.is_empty() || change.id.len() > MAX_KEY_LEN {
        return Err(AppError::BadRequest(format!(
            "bad row id {:?} in {}",
            change.id, change.table
        )));
    }
    if !merge::is_valid_hlc(&change.hlc) {
        return Err(AppError::BadRequest(format!(
            "malformed hlc {:?} on {}",
            change.hlc,
            where_()
        )));
    }
    let op = match change.op {
        WireOp::Delete => Op::Delete,
        WireOp::Upsert => {
            let values = change.values.unwrap_or_default();
            // SQLite stores null, numbers and text; anything else would
            // arrive on another device as something it cannot write back.
            if let Some((column, _)) = values.iter().find(|(_, value)| {
                !matches!(value, Value::Null | Value::Number(_) | Value::String(_))
            }) {
                return Err(AppError::BadRequest(format!(
                    "column {column} of {} is not null, a number or a string",
                    where_()
                )));
            }
            Op::Upsert(values)
        }
    };
    Ok((
        space,
        Change {
            table: change.table,
            id: change.id,
            op,
            hlc: change.hlc,
        },
    ))
}

/// What to do with a change for a shared space the caller named.
enum Shared {
    Write,
    /// Not written, and not an error: the caller was removed from the list,
    /// or a member's device deleted the list itself, and only finds out on
    /// its next pull. Refusing the batch would leave that device retrying it
    /// for ever; skipping lets the pull put it right.
    Skip(&'static str),
}

/// Whether the caller may write `change` into the shared space they named:
/// they are in it, the row is one a shared list holds, and deleting the list
/// itself is the owner's. A list's rows can't be smuggled into another
/// list's space: the list row and its instructions must be that list's.
fn check_shared(membership: Option<&spaces::Membership>, change: &Change) -> Result<Shared> {
    let Some(membership) = membership else {
        return Ok(Shared::Skip("not a member"));
    };
    if !spaces::SHARED_TABLES.contains(&change.table.as_str()) {
        return Err(AppError::BadRequest(format!(
            "{} rows aren't shared",
            change.table
        )));
    }
    match change.table.as_str() {
        "task_lists" if change.id != membership.list_id => {
            return Err(AppError::BadRequest(
                "a shared list holds only its own list row".into(),
            ));
        }
        "task_lists" if matches!(change.op, Op::Delete) && membership.role != "owner" => {
            return Ok(Shared::Skip("a member deleting the list"));
        }
        "agent_instructions" if change.id != format!("list:{}", membership.list_id) => {
            return Err(AppError::BadRequest(
                "a shared list holds only its own instructions".into(),
            ));
        }
        _ => {}
    }
    Ok(Shared::Write)
}

pub async fn push(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Json(request): Json<PushRequest>,
) -> Result<Json<PushResponse>> {
    let account = caller.account;
    // Every write records its device, so a push has to say which it is.
    let device = caller
        .device
        .ok_or_else(|| AppError::BadRequest(format!("a push needs {DEVICE_HEADER}")))?;
    let changes = request
        .changes
        .into_iter()
        .map(validate)
        .collect::<Result<Vec<_>>>()?;

    let mut tx = state.pool.begin().await?;
    lock(&mut tx).await?;
    let memberships = spaces::memberships(&mut *tx, account).await?;
    // Checked before anything is written, so a batch is applied whole or
    // refused whole.
    let received = changes.len();
    let mut skipped = 0usize;
    let mut accepted = Vec::with_capacity(received);
    for (space, change) in changes {
        match space {
            Some(space) if space != account => {
                match check_shared(memberships.get(&space), &change)? {
                    Shared::Write => accepted.push((space, change)),
                    Shared::Skip(why) => {
                        skipped += 1;
                        tracing::info!(%account, %space, table = %change.table, why, "skipped a shared change");
                    }
                }
            }
            _ => accepted.push((account, change)),
        }
    }
    let changes = accepted;

    let mut written = BTreeSet::new();
    for (space, change) in &changes {
        let stored = load(&mut tx, *space, &change.table, &change.id).await?;
        if let Outcome::Changed(row) = merge::merge(stored.as_ref(), change, device) {
            store(&mut tx, *space, account, &change.table, &change.id, &row).await?;
            written.insert(*space);
        }
    }

    let readable = spaces::readable(&mut *tx, account).await?;
    let cursor = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(seq), 0) FROM rows WHERE space_id = ANY($1)",
    )
    .bind(&readable)
    .fetch_one(&mut *tx)
    .await?;
    let audience = spaces::audience(&mut tx, &written).await?;
    realtime::nudge(&mut tx, state.realtime, &audience).await?;
    tx.commit().await?;

    let written = written.len();
    tracing::info!(%device, %account, received, skipped, spaces = written, cursor, "push");
    Ok(Json(PushResponse {
        accepted: received,
        cursor,
    }))
}

type RowTuple = (
    Jsonb<Map<String, Value>>,
    Jsonb<BTreeMap<String, String>>,
    bool,
    Option<String>,
    Option<Uuid>,
);

async fn load(
    tx: &mut Transaction<'_, Postgres>,
    space: Uuid,
    table: &str,
    id: &str,
) -> Result<Option<StoredRow>> {
    let row = sqlx::query_as::<_, RowTuple>(
        "SELECT data, col_hlc, deleted, deleted_hlc, last_device_id FROM rows \
         WHERE space_id = $1 AND table_name = $2 AND row_id = $3 FOR UPDATE",
    )
    .bind(space)
    .bind(table)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(
        |(data, col_hlc, deleted, deleted_hlc, last_device_id)| StoredRow {
            data: data.0,
            col_hlc: col_hlc.0,
            deleted,
            deleted_hlc,
            last_device_id,
        },
    ))
}

async fn store(
    tx: &mut Transaction<'_, Postgres>,
    space: Uuid,
    account: Uuid,
    table: &str,
    id: &str,
    row: &StoredRow,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO rows (space_id, account_id, table_name, row_id, data, col_hlc, deleted, deleted_hlc, seq, last_device_id) \
         VALUES ($1, $9, $2, $3, $4, $5, $6, $7, nextval('row_seq'), $8) \
         ON CONFLICT (space_id, table_name, row_id) DO UPDATE SET \
           account_id = EXCLUDED.account_id, \
           data = EXCLUDED.data, col_hlc = EXCLUDED.col_hlc, deleted = EXCLUDED.deleted, \
           deleted_hlc = EXCLUDED.deleted_hlc, seq = EXCLUDED.seq, \
           last_device_id = EXCLUDED.last_device_id",
    )
    .bind(space)
    .bind(table)
    .bind(id)
    .bind(Jsonb(&row.data))
    .bind(Jsonb(&row.col_hlc))
    .bind(row.deleted)
    .bind(&row.deleted_hlc)
    .bind(row.last_device_id)
    .bind(account)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
