//! `GET /v1/changes`: the feed a device pulls other devices' rows from.

use crate::AppState;
use crate::auth::Caller;
use crate::error::Result;
use crate::merge::row_hlc;
use crate::spaces;
use axum::extract::{Query, State};
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::{Map, Value};
use sqlx::types::Json as Jsonb;
use std::collections::BTreeMap;
use std::time::Duration;
use takt_sync_rules::wire::{ChangedRow, ChangesResponse};
use tokio::time::Instant;

const DEFAULT_LIMIT: i64 = 500;
const MAX_LIMIT: i64 = 2000;
/// Under the 30 s idle timeouts common in proxies in front of the server, so
/// an empty long-poll comes back as an answer rather than a dropped request.
const MAX_WAIT_SECONDS: u64 = 25;

#[derive(Debug, Deserialize)]
pub struct ChangesQuery {
    pub since: Option<i64>,
    pub limit: Option<i64>,
    pub wait: Option<u64>,
}

pub async fn changes(
    State(state): State<AppState>,
    Extension(caller): Extension<Caller>,
    Query(query): Query<ChangesQuery>,
) -> Result<Json<ChangesResponse>> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let wait = Duration::from_secs(query.wait.unwrap_or(0).min(MAX_WAIT_SECONDS));
    let deadline = Instant::now() + wait;
    let since = query.since.unwrap_or(0).max(0);

    let mut woken = state.changes.clone();
    let mut shutdown = state.shutdown.clone();
    loop {
        // Marked seen before reading, so a push that commits between the read
        // and the wait below still wakes it.
        woken.borrow_and_update();
        let page = scan(&state, caller.account, since, limit).await?;
        if !page.rows.is_empty() || page.has_more || Instant::now() >= deadline {
            return Ok(Json(page));
        }
        tokio::select! {
            changed = woken.changed() => if changed.is_err() { return Ok(Json(page)) },
            () = tokio::time::sleep_until(deadline) => return Ok(Json(page)),
            _ = shutdown.wait_for(|stopping| *stopping) => return Ok(Json(page)),
        }
    }
}

type ScanTuple = (
    i64,
    uuid::Uuid,
    String,
    String,
    Jsonb<Map<String, Value>>,
    Jsonb<BTreeMap<String, String>>,
    bool,
    Option<String>,
);

/// One page of the account's feed after `since`: its own space's rows and
/// those of every shared list it is in, each shared row tagged with its
/// space, and the shared lists themselves.
///
/// `seq` comes from one sequence shared by every account, so an account's
/// rows have gaps between their numbers. That's fine: a cursor only has to
/// rise, never to count.
///
/// Every row is sent, the caller's own pushes included. Leaving those out
/// looks free but loses data: a row the caller last wrote can still carry
/// another device's earlier edit to a column the caller never received, and
/// re-applying its own row is harmless. One more row is read than asked for,
/// so `hasMore` is exact rather than a guess from a full page.
async fn scan(
    state: &AppState,
    account: uuid::Uuid,
    since: i64,
    limit: i64,
) -> Result<ChangesResponse> {
    // In one snapshot, so the spaces sent are the ones the rows came from.
    let mut tx = state.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let readable = spaces::readable(&mut *tx, account).await?;
    let spaces = spaces::wire_spaces(&mut *tx, account).await?;
    let mut scanned = sqlx::query_as::<_, ScanTuple>(
        "SELECT seq, space_id, table_name, row_id, data, col_hlc, deleted, deleted_hlc \
         FROM rows WHERE space_id = ANY($1) AND seq > $2 ORDER BY seq LIMIT $3",
    )
    .bind(&readable)
    .bind(since)
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    let has_more = scanned.len() as i64 > limit;
    scanned.truncate(limit as usize);
    let cursor = scanned.last().map_or(since, |row| row.0);
    let rows = scanned
        .into_iter()
        .map(
            |(_, space, table, id, data, col_hlc, deleted, deleted_hlc)| ChangedRow {
                hlc: row_hlc(&col_hlc.0, deleted_hlc.as_deref()),
                space: (space != account).then(|| space.to_string()),
                table,
                id,
                deleted,
                values: data.0,
            },
        )
        .collect();
    Ok(ChangesResponse {
        rows,
        cursor,
        has_more,
        spaces: Some(spaces),
    })
}
