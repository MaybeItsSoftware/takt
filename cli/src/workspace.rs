//! Access to the app's workspace database.
//!
//! Everything else in this crate is a peer of the macOS app that happens to
//! read the same files. This module and `workspace_tasks.rs` are the places
//! that reach into the app's own store —
//! `~/Library/Application Support/Takt/priority.sqlite` (or the pre-rename
//! `Priority/` directory, until the app has carried it forward).
//!
//! This half is **read-only**, and says so by opening the connection with
//! `SQLITE_OPEN_READ_ONLY` rather than by promising to behave. It exists
//! because of a bug that took a hand-written `SELECT` to find: the menu bar
//! reported a focus block as nineteen hours long, and nothing reachable from
//! the CLI or the MCP server could see a focus session at all. An assistant
//! asked "what is the timer doing?" could only guess. Now it can look.
//!
//! The focus tables stay read-only on purpose: finishing a block has day-log
//! and scoring side effects that live in `WorkspaceStore`, and a second writer
//! would quietly skip them. The task tree — folders, lists and tasks — is the
//! exception, written by `workspace_tasks.rs` under the same undo journal the
//! app uses. Its header says what it copies from `WorkspaceStore` and why.
//!
//! What neither does is recompute policy. Which task is next, whether one is
//! available in the current context, how a day is scored: those live in
//! `TaktCore` and would be a second implementation to keep in step, which
//! is exactly what the app's MCP server stopped having. These are row reads,
//! row writes, and arithmetic the schema already implies.

use crate::error::{Result, ToolError};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::{Value, json};
use std::path::PathBuf;

pub struct Workspace {
    pub database_path: PathBuf,
}

impl Workspace {
    /// `$PRIORITY_MCP_DB_PATH`, then `database_path` in the CLI's config file,
    /// then the app's own location. The overrides are how the tests reach a
    /// fixture database, and how a non-standard install stays reachable.
    pub fn resolve(config: &crate::config::Config) -> Self {
        Workspace {
            database_path: config
                .resolve_path("PRIORITY_MCP_DB_PATH", "database_path")
                .0
                .unwrap_or_else(default_database_path),
        }
    }

    pub(crate) fn open(&self) -> Result<Connection> {
        if !self.database_path.exists() {
            return Err(ToolError::new(format!(
                "No workspace database at {}. Open Takt once, or set database_path.",
                self.database_path.display()
            )));
        }
        let failed = |error: rusqlite::Error| {
            ToolError::new(format!(
                "Could not read {}: {error}",
                self.database_path.display()
            ))
        };
        let probe = |connection: &Connection| {
            connection.query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| {
                row.get::<_, i64>(0)
            })
        };

        let read_only = Connection::open_with_flags(
            &self.database_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .map_err(failed)?;
        if probe(&read_only).is_ok() {
            return Ok(read_only);
        }
        // The database is in WAL mode, and a read-only connection cannot
        // create the `-shm` index a WAL reader needs. While the app is running
        // it has one open, but its last connection deletes it on quit, so a
        // strictly read-only open works only while Priority is up. The
        // fallback is still read-only, just enforced by SQLite's
        // `query_only` rather than by the open flags. It has no CREATE flag,
        // so it cannot make a database that was not there.
        let connection = Connection::open_with_flags(
            &self.database_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_URI,
        )
        .map_err(failed)?;
        connection
            .pragma_update(None, "query_only", true)
            .map_err(failed)?;
        probe(&connection).map_err(failed)?;
        Ok(connection)
    }

    /// What the focus timer is actually doing.
    ///
    /// The elapsed figure is the session's own definition — accumulated
    /// seconds plus, only when it is not paused, the time since the active
    /// task started. Wall clock since `startedAt` is the calculation that
    /// produced the nineteen-hour reading, and it is not offered here even as
    /// a convenience.
    pub fn focus_status(&self, now: DateTime<Utc>) -> Result<Value> {
        let connection = self.open()?;
        // The CLI reads without migrating: a file from before
        // `v39_open_ended_focus` has no timers.
        let has_timers: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('focus_sessions') WHERE name = 'openEnded')",
                [],
                |row| row.get(0),
            )
            .map_err(map_query_error)?;
        let open_ended = if has_timers { "openEnded" } else { "0" };
        let session = connection
            .query_row(
                &format!(
                    "SELECT id, startedAt, phase, activeTaskId, workDurationSeconds, \
                     activeTaskStartedAt, accumulatedSeconds, pausedAt, checkpointAt, {open_ended} \
                     FROM focus_sessions WHERE phase <> 'finished' \
                     ORDER BY startedAt DESC LIMIT 1"
                ),
                [],
                |row| {
                    Ok(SessionRow {
                        id: row.get(0)?,
                        started_at: row.get(1)?,
                        phase: row.get(2)?,
                        active_task_id: row.get(3)?,
                        work_duration_seconds: row.get(4)?,
                        active_task_started_at: row.get(5)?,
                        accumulated_seconds: row.get::<_, Option<i64>>(6)?.unwrap_or(0),
                        paused_at: row.get(7)?,
                        checkpoint_at: row.get(8)?,
                        open_ended: row.get(9)?,
                    })
                },
            )
            .optional()
            .map_err(map_query_error)?;

        // Only "no such row" is "not running". A locked file, a missing table
        // or a changed column is an error, and saying "no focus session" for
        // it would be a confident wrong answer.
        let Some(session) = session else {
            return Ok(json!({ "running": false }));
        };

        let paused = session.paused_at.is_some();
        let live = if paused {
            0
        } else {
            parse_stored(&session.active_task_started_at)
                .map(|start| (now - start).num_seconds().max(0))
                .unwrap_or(0)
        };
        let elapsed = session.accumulated_seconds.max(0) + live;
        // A timer has no length: its clock counts up, and nothing remains.
        let open_ended = session.open_ended;
        let remaining = session.work_duration_seconds - elapsed;

        let title = session
            .active_task_id
            .as_deref()
            .and_then(|id| self.task_title(&connection, id));

        Ok(json!({
            "running": true,
            "session_id": session.id,
            "phase": session.phase,
            "paused": paused,
            "paused_at": local_string(&session.paused_at),
            "started_at": local_string(&Some(session.started_at.clone())),
            "checkpoint_at": local_string(&session.checkpoint_at),
            "task_id": session.active_task_id,
            "task": title,
            "elapsed_seconds": elapsed,
            "timer": open_ended,
            "planned_seconds": (!open_ended).then_some(session.work_duration_seconds),
            "remaining_seconds": (!open_ended).then_some(remaining),
            "overrun": !open_ended && remaining < 0,
            "clock": clock(if open_ended { elapsed } else { remaining.abs() }),
            "queue": self.queue(&connection, &session.id)?,
        }))
    }

    /// What the day's focused time was spent on, newest first. Straight out of
    /// `focus_work_blocks`, which is what the app's own timeline reads.
    pub fn focus_history(&self, since: DateTime<Local>, until: DateTime<Local>) -> Result<Value> {
        let connection = self.open()?;
        let mut statement = connection
            .prepare(
                "SELECT taskTitle, seconds, recordedAt, taskId FROM focus_work_blocks \
                 WHERE recordedAt >= ?1 AND recordedAt < ?2 ORDER BY recordedAt DESC",
            )
            .map_err(map_query_error)?;
        let rows = statement
            .query_map(
                [
                    stored_string(since.with_timezone(&Utc)),
                    stored_string(until.with_timezone(&Utc)),
                ],
                |row| {
                    Ok(json!({
                        "task": row.get::<_, String>(0)?,
                        "seconds": row.get::<_, i64>(1)?,
                        "recorded_at": local_string(&Some(row.get::<_, String>(2)?)),
                        "task_id": row.get::<_, Option<String>>(3)?,
                    }))
                },
            )
            .map_err(map_query_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_query_error)?;
        let total: i64 = rows
            .iter()
            .map(|block| block["seconds"].as_i64().unwrap_or(0))
            .sum();
        Ok(json!({
            "from": since.format("%Y-%m-%d %H:%M").to_string(),
            "until": until.format("%Y-%m-%d %H:%M").to_string(),
            "total_seconds": total,
            "total": clock(total),
            "blocks": rows,
        }))
    }

    fn queue(&self, connection: &Connection, session_id: &str) -> Result<Vec<Value>> {
        let mut statement = connection
            .prepare(
                "SELECT q.state, q.plannedSeconds, t.title FROM focus_queue_items q \
                 LEFT JOIN tasks t ON t.id = q.taskId \
                 WHERE q.sessionId = ?1 ORDER BY q.sortOrder",
            )
            .map_err(map_query_error)?;
        let rows = statement
            .query_map([session_id], |row| {
                Ok(json!({
                    "state": row.get::<_, String>(0)?,
                    "planned_seconds": row.get::<_, Option<i64>>(1)?,
                    "task": row.get::<_, Option<String>>(2)?,
                }))
            })
            .map_err(map_query_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_query_error)?;
        Ok(rows)
    }

    fn task_title(&self, connection: &Connection, id: &str) -> Option<String> {
        connection
            .query_row("SELECT title FROM tasks WHERE id = ?1", [id], |row| {
                row.get::<_, String>(0)
            })
            .ok()
    }
}

struct SessionRow {
    id: String,
    started_at: String,
    phase: String,
    active_task_id: Option<String>,
    work_duration_seconds: i64,
    active_task_started_at: String,
    accumulated_seconds: i64,
    paused_at: Option<String>,
    checkpoint_at: Option<String>,
    open_ended: bool,
}

pub fn default_database_path() -> PathBuf {
    crate::config::app_support_path(Some("priority.sqlite"))
}

/// GRDB stores dates as `YYYY-MM-DD HH:MM:SS.SSS` in UTC, with the
/// milliseconds sometimes absent.
pub(crate) fn parse_stored(value: &str) -> Option<DateTime<Utc>> {
    for format in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.fZ"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(value, format) {
            return Some(Utc.from_utc_datetime(&naive));
        }
    }
    None
}

pub(crate) fn stored_string(value: DateTime<Utc>) -> String {
    value.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

/// Stored UTC in, the reader's own clock out — a timestamp you have to convert
/// in your head is a timestamp you misread.
pub(crate) fn local_string(value: &Option<String>) -> Option<String> {
    value.as_deref().and_then(parse_stored).map(|date| {
        date.with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    })
}

fn clock(seconds: i64) -> String {
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

pub(crate) fn map_query_error(error: rusqlite::Error) -> ToolError {
    ToolError::new(format!("Workspace query failed: {error}"))
}
