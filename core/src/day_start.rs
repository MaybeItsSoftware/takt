//! Starting each day afresh: at the hour the day changes, what holds goes
//! back to the conditions the user starts every day with (Home), the
//! overrides and the time window go with whatever was said yesterday.
//!
//! The hour is the one Takt already counts days from — the daily log's
//! rollover (`day_log`, 04:00 by default), so a session finishing at 01:30
//! and the context said at 01:30 belong to the same day. What used
//! to be one Mac's preference is now the account's, kept with the starting
//! conditions as synced `preferences` rows, as Google Calendar's choices
//! are: every device reads the same hour and the same set.
//!
//! Nothing runs at the boundary. Each client calls [`apply`] whenever it
//! evaluates — at launch, on coming to the foreground, after a pull, and the
//! Mac on a timer — and the first call after the boundary resets the
//! context, so a device closed for days resets once when it next opens. The
//! day it last reset is a synced preference too, so once any device has
//! reset today, none does again. A context said since the day began wins
//! over the reset: a device that pulls what you said at 06:00 on another
//! one, before its own first evaluation, keeps it rather than putting you
//! back at Home.

use chrono::NaiveTime;
use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::CoreError;
use crate::day_log::DEFAULT_ROLLOVER_HOUR;
use crate::focus_context::{self, StoredFocusContext};
use crate::time::{decode_strings, parse_stored, stored, swift_json_strings};
use crate::workspace::CoreWorkspace;

/// Whether each day starts afresh: "true" or "false".
pub const PREF_RESETS: &str = "dayStart.resets";
/// The hour the day changes, 0 to 23, local to each device.
pub const PREF_HOUR: &str = "dayStart.hour";
/// The conditions every day starts with, a JSON array of ids.
pub const PREF_CONDITIONS: &str = "dayStart.conditionIds";
/// The last day reset, `yyyy-MM-dd`.
pub const PREF_LAST_RESET: &str = "dayStart.lastReset";

/// How each day starts, as the account keeps it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DayStartSettings {
    /// Whether the context goes back to `condition_ids` when the day changes.
    pub resets: bool,
    /// The hour the day changes, 0 to 23. Also where the daily log's days
    /// begin.
    pub hour: i32,
    /// The conditions each day starts with, sorted; empty starts it with
    /// nothing on.
    pub condition_ids: Vec<String>,
    /// The last day reset, `yyyy-MM-dd`; none before the first.
    pub last_reset_day: Option<String>,
}

impl Default for DayStartSettings {
    fn default() -> Self {
        Self {
            resets: false,
            hour: DEFAULT_ROLLOVER_HOUR,
            condition_ids: Vec::new(),
            last_reset_day: None,
        }
    }
}

fn preference(connection: &Connection, key: &str) -> Result<Option<String>, CoreError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'preferences')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    Ok(connection
        .query_row(
            "SELECT value FROM preferences WHERE key = ?1",
            [key],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten())
}

/// The account's settings, the defaults where it has none.
pub fn settings(connection: &Connection) -> Result<DayStartSettings, CoreError> {
    let mut ids = preference(connection, PREF_CONDITIONS)?
        .map(|json| decode_strings(&json))
        .unwrap_or_default();
    ids.sort();
    ids.dedup();
    Ok(DayStartSettings {
        resets: preference(connection, PREF_RESETS)?.as_deref() == Some("true"),
        hour: preference(connection, PREF_HOUR)?
            .and_then(|text| text.trim().parse::<i32>().ok())
            .map_or(DEFAULT_ROLLOVER_HOUR, |hour| hour.clamp(0, 23)),
        condition_ids: ids,
        last_reset_day: preference(connection, PREF_LAST_RESET)?.filter(|day| !day.is_empty()),
    })
}

/// Whether the account has said what hour its day changes at, as opposed to
/// reading the default: a Mac brings its daily log's hour in only when not.
pub fn has_hour(connection: &Connection) -> Result<bool, CoreError> {
    Ok(preference(connection, PREF_HOUR)?.is_some())
}

/// The day containing `now_ms`, with days changing at `hour` on the wall
/// clock in `zone`: its `yyyy-MM-dd` and the instant it began. Read off the
/// wall clock rather than by counting back `hour` hours, so on the nights
/// the clocks change the day still turns at 04:00 as the clock shows it; a
/// change hour the night skips turns at the first moment after the gap, and
/// one it repeats at its first occurrence.
fn day_of(hour: i32, now_ms: i64, zone: &str) -> (String, i64) {
    let zone = crate::periodic::zone(zone);
    let hour = hour.clamp(0, 23) as u32;
    let wall = crate::day_log::instant(now_ms)
        .with_timezone(&zone)
        .naive_local();
    let turn = NaiveTime::from_hms_opt(hour, 0, 0).unwrap_or_default();
    let date = if wall.time() >= turn {
        wall.date()
    } else {
        wall.date().pred_opt().unwrap_or(wall.date())
    };
    let began = crate::periodic::resolve(zone, date.and_time(turn)).timestamp_millis();
    (date.format("%Y-%m-%d").to_string(), began)
}

/// `yyyy-MM-dd` for the day containing `now_ms`, with days changing at
/// `hour` in `zone`.
pub fn day_key(hour: i32, now_ms: i64, zone: &str) -> String {
    day_of(hour, now_ms, zone).0
}

/// Stores how each day starts. Any change counts today as started, so
/// changing the settings never resets the context there and then: the
/// first reset is at the next change of day. False, writing nothing, when
/// nothing changed.
pub fn save(
    transaction: &Transaction,
    next: &DayStartSettings,
    now_ms: i64,
    zone: &str,
) -> Result<bool, CoreError> {
    let current = settings(transaction)?;
    let mut ids = next.condition_ids.clone();
    ids.sort();
    ids.dedup();
    let hour = next.hour.clamp(0, 23);
    if current.resets == next.resets
        && current.hour == hour
        && current.condition_ids == ids
        && has_hour(transaction)?
    {
        return Ok(false);
    }
    let set = |key: &str, value: &str| {
        crate::setup::set_preference(transaction, key, Some(value), now_ms)
    };
    set(PREF_RESETS, if next.resets { "true" } else { "false" })?;
    set(PREF_HOUR, &hour.to_string())?;
    set(PREF_CONDITIONS, &swift_json_strings(&ids))?;
    set(PREF_LAST_RESET, &day_key(hour, now_ms, zone))?;
    Ok(true)
}

/// Takes a condition out of the ones each day starts with, when it becomes
/// one no context can say holds (a device condition). Writes nothing when
/// it was not among them.
pub(crate) fn forget_condition(
    transaction: &Transaction,
    id: &str,
    now_ms: i64,
) -> Result<(), CoreError> {
    let Some(json) = preference(transaction, PREF_CONDITIONS)? else {
        return Ok(());
    };
    let ids = decode_strings(&json);
    if !ids.iter().any(|known| known == id) {
        return Ok(());
    }
    let kept: Vec<String> = ids.into_iter().filter(|known| known != id).collect();
    crate::setup::set_preference(
        transaction,
        PREF_CONDITIONS,
        Some(&swift_json_strings(&kept)),
        now_ms,
    )?;
    Ok(())
}

/// Brings in an hour kept somewhere older (the Mac's daily log) when the
/// account has none yet. False, writing nothing, when it already had one.
pub fn adopt_hour(transaction: &Transaction, hour: i32, now_ms: i64) -> Result<bool, CoreError> {
    if has_hour(transaction)? {
        return Ok(false);
    }
    crate::setup::set_preference(
        transaction,
        PREF_HOUR,
        Some(&hour.clamp(0, 23).to_string()),
        now_ms,
    )
}

/// When the workspace's context was last saved, by any device.
fn context_saved_at(connection: &Connection, workspace_id: &str) -> Result<Option<i64>, CoreError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'focus_contexts')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    let text: Option<String> = connection
        .query_row(
            "SELECT updatedAt FROM focus_contexts WHERE workspaceId = ?1",
            [workspace_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(text
        .as_deref()
        .and_then(parse_stored)
        .map(|at| at.timestamp_millis()))
}

/// Starts the day if it has changed since the last reset: what holds goes
/// back to the starting conditions — the ones still there and not archived,
/// one place at most — with nothing switched off for now, no time window
/// and no expiry, stamped as said now. Records the day either way, so it
/// happens once. A context saved since the day began is kept: someone said
/// what holds today already. True when the context was reset.
pub fn apply(
    transaction: &Transaction,
    workspace_id: &str,
    now_ms: i64,
    zone: &str,
) -> Result<bool, CoreError> {
    let settings = settings(transaction)?;
    if !settings.resets {
        return Ok(false);
    }
    let (today, day_began) = day_of(settings.hour, now_ms, zone);
    // Keys sort as dates; a device a zone behind one that already reset
    // reads that day as still to come, and leaves it.
    if settings
        .last_reset_day
        .as_deref()
        .is_some_and(|last| last >= today.as_str())
    {
        return Ok(false);
    }
    crate::setup::set_preference(transaction, PREF_LAST_RESET, Some(&today), now_ms)?;
    if context_saved_at(transaction, workspace_id)?.is_some_and(|saved| saved >= day_began) {
        return Ok(false);
    }
    let conditions = crate::rows::conditions(transaction, workspace_id)?;
    let usable: Vec<String> = settings
        .condition_ids
        .into_iter()
        .filter(|id| conditions.iter().any(|c| c.id == *id && !c.is_archived))
        .collect();
    let places: Vec<String> = conditions
        .iter()
        .filter(|c| c.is_location)
        .map(|c| c.id.clone())
        .collect();
    let ids = focus_context::focus_context_changed(Vec::new(), usable, Vec::new(), places);
    let current = focus_context::load(transaction, workspace_id)?.unwrap_or_default();
    let next = StoredFocusContext {
        condition_ids: ids.clone(),
        conditions_set_at_ms: None,
        available_until_ms: None,
        expires_at_ms: None,
        suggested_ids: Vec::new(),
        off_ids: Vec::new(),
        ..current
    };
    focus_context::save(transaction, workspace_id, &next, now_ms)?;
    // The same conditions as yesterday are still today's answer, not a stale
    // one: the stamp moves to now, as if they had been said again.
    if !ids.is_empty() {
        transaction.execute(
            "UPDATE focus_contexts SET conditionsSetAt = ?1, updatedAt = ?1 WHERE workspaceId = ?2",
            rusqlite::params![stored(now_ms), workspace_id],
        )?;
    }
    Ok(true)
}

#[uniffi::export]
impl CoreWorkspace {
    /// How each day starts.
    pub fn day_start_settings(&self) -> Result<DayStartSettings, CoreError> {
        settings(&self.lock())
    }

    /// Stores how each day starts; whether anything changed. Not an undo
    /// step, like the rest of what holds now.
    pub fn set_day_start_settings(
        &self,
        settings: DayStartSettings,
        now_ms: i64,
        zone: String,
    ) -> Result<bool, CoreError> {
        self.unjournalled(|tx| save(tx, &settings, now_ms, &zone))
    }

    /// Brings in an hour kept before the account had one; whether it did.
    pub fn adopt_day_start_hour(&self, hour: i32, now_ms: i64) -> Result<bool, CoreError> {
        self.unjournalled(|tx| adopt_hour(tx, hour, now_ms))
    }

    /// Starts the day if it has changed since the last reset; whether the
    /// context was reset. Every client calls it when it evaluates.
    pub fn apply_day_start(
        &self,
        workspace_id: String,
        now_ms: i64,
        zone: String,
    ) -> Result<bool, CoreError> {
        self.unjournalled(|tx| apply(tx, &workspace_id, now_ms, &zone))
    }
}

/// `yyyy-MM-dd` for the day containing `now_ms`, days changing at `hour`
/// in `zone`: which day a reset would count as.
#[uniffi::export]
pub fn day_start_day_key(hour: i32, now_ms: i64, zone: String) -> String {
    day_key(hour, now_ms, &zone)
}

#[cfg(test)]
mod tests;
