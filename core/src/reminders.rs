//! Reminders: the local notifications a device sets for what is due.
//!
//! The core decides what goes off and when, so the Mac, the iPhone and
//! Android cannot disagree; each client only hands the list to its own
//! notification system (`UNUserNotificationCenter`, `AlarmManager`) and sets
//! it again whenever the workspace changes. Nothing is sent from a server:
//! the sync server never sees what a task says (`docs/e2e-sync-plan.md`).
//!
//! Three kinds, each switched on per device ([`ReminderSettings`]):
//!
//! - **Due time.** A task due at a time goes off at that time.
//! - **Due today.** One notification a day, at a time of the user's choosing,
//!   naming what is due that day, timed or not. None on a day with nothing
//!   due.
//! - **Dailies left.** One a day, at its own time, naming the dailies due
//!   that day and not yet done. None once they are all done.
//!
//! Text is fixed when the list is read, so a client sets the list again
//! after every write and every pull; finishing a task takes its reminders
//! with it on the next read.

use std::collections::HashSet;

use chrono::{DateTime, Duration, NaiveDate, NaiveTime};
use chrono_tz::Tz;
use rusqlite::Connection;

use crate::CoreError;
use crate::records::all;
use crate::time::parse_stored;
use crate::workspace::CoreWorkspace;

/// What a device reminds about, and when. Times are minutes after local
/// midnight. Kept per device by each client, not synced: they are about
/// which device buzzes.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReminderSettings {
    pub due_time: bool,
    pub due_today: bool,
    pub due_today_minute: i32,
    pub dailies_left: bool,
    pub dailies_left_minute: i32,
}

impl Default for ReminderSettings {
    /// Everything off; due today at 09:00, dailies left at 20:00.
    fn default() -> Self {
        ReminderSettings {
            due_time: false,
            due_today: false,
            due_today_minute: 9 * 60,
            dailies_left: false,
            dailies_left_minute: 20 * 60,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ReminderKind {
    DueTime,
    DueToday,
    DailiesLeft,
}

/// One notification to set.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Reminder {
    /// Stable for the same reminder across reads (`due.<task>`,
    /// `today.<yyyy-MM-dd>`, `dailies.<yyyy-MM-dd>`), so a client can
    /// replace one in place.
    pub id: String,
    pub kind: ReminderKind,
    pub fire_at_ms: i64,
    pub title: String,
    pub body: String,
    /// The task a due-time reminder is for, to open on a tap.
    pub task_id: Option<String>,
}

/// How many titles a summary names before "and N more".
const NAMED: usize = 3;

/// The reminders that go off after `now_ms` and within `days` local days
/// of it (today counting as the first), soonest first, at most `limit`.
/// iOS keeps 64 pending notifications per app, so a client asks for fewer.
pub fn reminders(
    connection: &Connection,
    settings: &ReminderSettings,
    now_ms: i64,
    zone: &str,
    days: u32,
    limit: u32,
) -> Result<Vec<Reminder>, CoreError> {
    let zone = crate::periodic::zone(zone);
    let today = local_day(now_ms, zone);
    let horizon = today + Duration::days(i64::from(days.max(1)));
    let end_ms = midnight_ms(horizon, zone);
    let tasks = open_dated_tasks(connection, zone)?;

    let mut out = Vec::new();
    if settings.due_time {
        for task in &tasks {
            if let Some(at) = task.due_at_ms.filter(|at| *at > now_ms && *at < end_ms) {
                out.push(Reminder {
                    id: format!("due.{}", task.id),
                    kind: ReminderKind::DueTime,
                    fire_at_ms: at,
                    title: "Due now".into(),
                    body: task.title.clone(),
                    task_id: Some(task.id.clone()),
                });
            }
        }
    }
    let dailies = if settings.dailies_left {
        workspace_dailies(connection)?
    } else {
        Vec::new()
    };
    let mut day = today;
    while day < horizon {
        if settings.due_today {
            let at = at_minute(day, settings.due_today_minute, zone);
            if at > now_ms {
                let titles: Vec<&str> = tasks
                    .iter()
                    .filter(|task| task.due_day == Some(day))
                    .map(|task| task.title.as_str())
                    .collect();
                if !titles.is_empty() {
                    out.push(Reminder {
                        id: format!("today.{day}"),
                        kind: ReminderKind::DueToday,
                        fire_at_ms: at,
                        title: count(titles.len(), "due today", "due today"),
                        body: summary(&titles),
                        task_id: None,
                    });
                }
            }
        }
        if settings.dailies_left {
            let at = at_minute(day, settings.dailies_left_minute, zone);
            if at > now_ms {
                let done = done_dailies(connection, day)?;
                let titles: Vec<&str> = dailies
                    .iter()
                    .filter(|daily| !done.contains(&daily.id) && daily.is_due(day, zone))
                    .map(|daily| daily.title.as_str())
                    .collect();
                if !titles.is_empty() {
                    out.push(Reminder {
                        id: format!("dailies.{day}"),
                        kind: ReminderKind::DailiesLeft,
                        fire_at_ms: at,
                        title: count(titles.len(), "daily left", "dailies left"),
                        body: summary(&titles),
                        task_id: None,
                    });
                }
            }
        }
        day += Duration::days(1);
    }
    out.sort_by(|a, b| a.fire_at_ms.cmp(&b.fire_at_ms).then(a.id.cmp(&b.id)));
    out.truncate(limit as usize);
    Ok(out)
}

/// An open task with a due time or a due day, and the local day it falls on.
struct DatedTask {
    id: String,
    title: String,
    due_at_ms: Option<i64>,
    due_day: Option<NaiveDate>,
}

/// Open tasks (not lists, not archived, not inside a closed or archived
/// list item) that have a due time or a due day.
fn open_dated_tasks(connection: &Connection, zone: Tz) -> Result<Vec<DatedTask>, CoreError> {
    let inactive = crate::focus::inactive_container_items(connection, None)?;
    let rows: Vec<(String, String, Option<String>, Option<String>)> = all(
        connection,
        "SELECT t.id, t.title, t.dueAt, m.planningJSON FROM tasks t
         LEFT JOIN task_metadata m ON m.taskId = t.id
         WHERE t.status = 'open' AND t.archivedAt IS NULL
           AND COALESCE(t.itemKind, 'task') = 'task'
           AND (t.dueAt IS NOT NULL OR m.planningJSON IS NOT NULL)
         ORDER BY t.dueAt, t.sortOrder, t.id",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    Ok(rows
        .into_iter()
        .filter(|(id, ..)| !inactive.contains(id))
        .filter_map(|(id, title, due_at, planning)| {
            let due_at_ms = due_at
                .as_deref()
                .and_then(parse_stored)
                .map(|at| at.timestamp_millis());
            let planned_day = planning
                .as_deref()
                .and_then(|json| crate::planning::Planning::from_json(json).ok())
                .and_then(|planning| planning.due_date)
                .and_then(|day| NaiveDate::parse_from_str(&day, "%Y-%m-%d").ok());
            let due_day = due_at_ms.map(|at| local_day(at, zone)).or(planned_day);
            due_day.map(|due_day| DatedTask {
                id,
                title,
                due_at_ms,
                due_day: Some(due_day),
            })
        })
        .collect())
}

/// A daily that is not archived, on a task that is open.
struct DailyRow {
    id: String,
    title: String,
    mask: Option<i64>,
    interval: Option<i64>,
    anchor_ms: i64,
}

impl DailyRow {
    fn is_due(&self, day: NaiveDate, zone: Tz) -> bool {
        crate::dailies::workspace_daily_is_due(
            self.mask,
            self.interval,
            local_day(self.anchor_ms, zone),
            day,
        )
    }
}

fn workspace_dailies(connection: &Connection) -> Result<Vec<DailyRow>, CoreError> {
    all(
        connection,
        "SELECT d.id, t.title, d.activeWeekdaysMask, d.intervalDays,
                COALESCE(d.intervalAnchor, d.createdAt)
         FROM dailies d JOIN tasks t ON t.id = d.taskId
         WHERE d.archivedAt IS NULL AND t.archivedAt IS NULL AND t.status = 'open'
         ORDER BY d.sortOrder, d.id",
        [],
        |row| {
            let anchor: Option<String> = row.get(4)?;
            Ok(DailyRow {
                id: row.get(0)?,
                title: row.get(1)?,
                mask: row.get(2)?,
                interval: row.get(3)?,
                anchor_ms: anchor
                    .as_deref()
                    .and_then(parse_stored)
                    .map_or(0, |at| at.timestamp_millis()),
            })
        },
    )
}

/// The dailies ticked off on `day`, whose contributions are keyed by the
/// local calendar day (`dailies::day_key`).
fn done_dailies(connection: &Connection, day: NaiveDate) -> Result<HashSet<String>, CoreError> {
    let ids: Vec<String> = all(
        connection,
        "SELECT dailyId FROM daily_contributions WHERE dayKey = ?1 AND completedAt IS NOT NULL",
        [day.format("%Y-%m-%d").to_string()],
        |row| row.get(0),
    )?;
    Ok(ids.into_iter().collect())
}

fn local_day(at_ms: i64, zone: Tz) -> NaiveDate {
    DateTime::from_timestamp_millis(at_ms)
        .unwrap_or_default()
        .with_timezone(&zone)
        .date_naive()
}

fn midnight_ms(day: NaiveDate, zone: Tz) -> i64 {
    crate::periodic::resolve(zone, day.and_time(NaiveTime::MIN)).timestamp_millis()
}

/// `minute` minutes after `day`'s local midnight, clamped to the day.
fn at_minute(day: NaiveDate, minute: i32, zone: Tz) -> i64 {
    let minute = minute.clamp(0, 24 * 60 - 1) as u32;
    let time = NaiveTime::from_hms_opt(minute / 60, minute % 60, 0).unwrap_or(NaiveTime::MIN);
    crate::periodic::resolve(zone, day.and_time(time)).timestamp_millis()
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Up to [`NAMED`] titles, comma-separated, then how many more.
fn summary(titles: &[&str]) -> String {
    let named = titles[..titles.len().min(NAMED)].join(", ");
    match titles.len().saturating_sub(NAMED) {
        0 => named,
        more => format!("{named} and {more} more"),
    }
}

#[uniffi::export]
impl CoreWorkspace {
    /// The reminders a device sets now (`reminders::reminders`).
    pub fn reminders(
        &self,
        settings: ReminderSettings,
        now_ms: i64,
        zone: String,
        days: u32,
        limit: u32,
    ) -> Result<Vec<Reminder>, CoreError> {
        reminders(&self.read(), &settings, now_ms, &zone, days, limit)
    }
}

/// Everything off, at the default times: what a client shows before the
/// user has chosen.
#[uniffi::export]
pub fn default_reminder_settings() -> ReminderSettings {
    ReminderSettings::default()
}

#[cfg(test)]
mod tests;
