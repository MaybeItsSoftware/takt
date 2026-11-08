//! `POST /v1/push`: a device's local changes, merged into the stored rows.

use crate::AppState;
use crate::auth::{Caller, DEVICE_HEADER};
use crate::error::{AppError, Result};
use crate::merge::{self, Change, Op, Outcome, StoredRow};
use crate::notify::CHANNEL;
use crate::realtime;
use axum::extract::State;
use axum::{Extension, Json};
use serde_json::{Map, Value};
use sqlx::types::Json as Jsonb;
use sqlx::{Postgres, Transaction};
use std::collections::BTreeMap;
use takt_sync_rules::wire::{PushRequest, PushResponse, WireChange, WireOp};
use uuid::Uuid;

/// Serialises every push against every other.
///
/// Row locks alone would keep two pushes from corrupting one row, but not the
/// feed. Both would draw `seq`s from the one sequence and could commit out of
/// order: a reader that saw seq 6 commit before seq 5 would move its cursor
/// past 5 and never see it. Holding one transaction-scoped lock from before
/// the first `nextval` until commit makes `seq` order commit order, which is
/// the property the cursor depends on.
///
/// The lock is per account, because a device only reads its own account's
/// rows: an account's pushes are in commit order among themselves, which is
/// all its readers can see, while different accounts push side by side. One
/// person's handful of devices never pushes hard enough for the serialisation
/// to cost anything.
const PUSH_LOCK_SQL: &str =
    "SELECT pg_advisory_xact_lock(hashtextextended('push:' || $1::text, 0))";

/// Longest table name or row id accepted. Generous for uuids and SQLite table
/// names; only there to stop a broken client storing megabyte keys.
const MAX_KEY_LEN: usize = 256;

/// Checks a change before anything is written, so a batch is applied whole
/// or refused whole: the client keeps its outbox and the bug shows.
fn validate(change: WireChange) -> Result<Change> {
    let where_ = || format!("{} {}", change.table, change.id);
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
    Ok(Change {
        table: change.table,
        id: change.id,
        op,
        hlc: change.hlc,
    })
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
    sqlx::query(PUSH_LOCK_SQL)
        .bind(account)
        .execute(&mut *tx)
        .await?;

    let mut written = 0usize;
    for change in &changes {
        let stored = load(&mut tx, account, &change.table, &change.id).await?;
        if let Outcome::Changed(row) = merge::merge(stored.as_ref(), change, device) {
            store(&mut tx, account, &change.table, &change.id, &row).await?;
            written += 1;
        }
    }

    if written > 0 {
        // Postgres holds a NOTIFY until the transaction commits, and drops it
        // if it rolls back, so a long-poll is never woken for rows it cannot
        // yet read.
        sqlx::query("SELECT pg_notify($1, '')")
            .bind(CHANNEL)
            .execute(&mut *tx)
            .await?;
    }
    let cursor = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(seq), 0) FROM rows WHERE account_id = $1",
    )
    .bind(account)
    .fetch_one(&mut *tx)
    .await?;
    if written > 0 && state.realtime {
        // Rides the transaction, like the NOTIFY: Realtime reads it from the
        // log once the push commits, and never if it rolls back.
        // `realtime.send` catches its own errors, so a Realtime outage costs
        // the nudge, not the push.
        sqlx::query("SELECT realtime.send(jsonb_build_object('cursor', $1::bigint), $2, $3, true)")
            .bind(cursor)
            .bind(realtime::EVENT)
            .bind(realtime::topic(account))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    tracing::info!(%device, %account, received = changes.len(), written, cursor, "push");
    Ok(Json(PushResponse {
        accepted: changes.len(),
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
    account: Uuid,
    table: &str,
    id: &str,
) -> Result<Option<StoredRow>> {
    let row = sqlx::query_as::<_, RowTuple>(
        "SELECT data, col_hlc, deleted, deleted_hlc, last_device_id FROM rows \
         WHERE account_id = $1 AND table_name = $2 AND row_id = $3 FOR UPDATE",
    )
    .bind(account)
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
    account: Uuid,
    table: &str,
    id: &str,
    row: &StoredRow,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO rows (account_id, table_name, row_id, data, col_hlc, deleted, deleted_hlc, seq, last_device_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, nextval('row_seq'), $8) \
         ON CONFLICT (account_id, table_name, row_id) DO UPDATE SET \
           data = EXCLUDED.data, col_hlc = EXCLUDED.col_hlc, deleted = EXCLUDED.deleted, \
           deleted_hlc = EXCLUDED.deleted_hlc, seq = EXCLUDED.seq, \
           last_device_id = EXCLUDED.last_device_id",
    )
    .bind(account)
    .bind(table)
    .bind(id)
    .bind(Jsonb(&row.data))
    .bind(Jsonb(&row.col_hlc))
    .bind(row.deleted)
    .bind(&row.deleted_hlc)
    .bind(row.last_device_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
