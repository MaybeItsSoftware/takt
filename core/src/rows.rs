//! The rest of the workspace's rows as the clients see them, and the reads
//! that hand them back: dailies and their contributions, conditions, task
//! metadata, focus sessions and their queues, work blocks, awards, themes and
//! preferences. `records.rs` holds tasks, lists, folders and workspaces.
//!
//! Dates cross as epoch milliseconds; a bound passed in is compared the way
//! GRDB compared a `Date`, as the stored text it would have written.

use rusqlite::{Connection, Row};

use crate::CoreError;
use crate::records::{TaskRow, all, optional_ms, required_ms};
use crate::time::stored;

/// A row of `dailies`. `WorkspaceDaily`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DailyRow {
    pub id: String,
    pub task_id: String,
    pub active_weekdays_mask: i64,
    pub interval_days: Option<i64>,
    pub interval_anchor_ms: Option<i64>,
    pub target_seconds: Option<i64>,
    pub sort_order: i64,
    pub archived_at_ms: Option<i64>,
    pub legacy_daily_id: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub source_task_id: Option<String>,
    pub placement_column: Option<String>,
    pub drops_at_day_end: bool,
    pub expiry_rule: String,
    pub expires_at_ms: Option<i64>,
}

/// A row of `daily_contributions`. `DailyContribution`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ContributionRow {
    pub id: String,
    pub daily_id: String,
    pub task_id: String,
    pub day_key: String,
    pub seconds_logged: i64,
    pub completed_at_ms: Option<i64>,
    pub created_at_ms: i64,
    /// When the day was set aside without being done; a skipped day is not
    /// owed (`v29_daily_contribution_skips`).
    pub skipped_at_ms: Option<i64>,
}

/// A daily shown on a day, with its task and that day's contribution.
/// `DailyItem`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DayDaily {
    pub daily: DailyRow,
    pub task: TaskRow,
    pub contribution: Option<ContributionRow>,
}

/// A row of `task_conditions`. `TaskCondition`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ConditionRow {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub is_location: bool,
    pub is_archived: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// The category as stored ("Location", "Equipment"), or none. A place an
    /// older client made has none; `condition_effective_category` reads it
    /// as "Location".
    pub category: Option<String>,
    /// Where it sits in the tab, workspace-wide: under its heading in this
    /// order, and the headings in the order of their first. None for one
    /// nobody has placed, which comes after the placed ones under its heading.
    pub sort_order: Option<i64>,
    /// The conditions it switches on while it holds (Home: Floor space,
    /// Desk), by id, sorted. Empty before v34 and for one that provides
    /// nothing. See `conditions::provides`.
    pub provides: Vec<String>,
}

/// A row of `task_metadata`. `TaskMetadata`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MetadataRow {
    pub task_id: String,
    pub priority: Option<i64>,
    pub start_at_ms: Option<i64>,
    pub tags_json: String,
    pub recurrence_rule: Option<String>,
    pub matrix_urgency: Option<i64>,
    pub matrix_importance: Option<i64>,
    /// The task's place within its matrix quadrant, lowest first.
    pub matrix_rank: Option<i64>,
    pub kanban_column: Option<String>,
    pub external_links_json: String,
    pub focus_rank: Option<i64>,
    pub updated_at_ms: i64,
    pub planning_json: Option<String>,
    pub waiting_on: Option<String>,
    pub waiting_follow_up_at_ms: Option<i64>,
    pub waiting_follow_up_task_id: Option<String>,
    pub follow_up_of_task_id: Option<String>,
    /// Chase again every this many days after a follow-up is done; `None`
    /// is once (`v32_follow_up_repeat`).
    pub waiting_follow_up_every_days: Option<u32>,
    /// How many follow-ups this spell of waiting has made.
    pub waiting_follow_up_count: u32,
}

/// A row of `focus_sessions`. `FocusSession`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SessionRow {
    pub id: String,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    /// "running", "onBreak", "paused" or "finished".
    pub phase: String,
    pub active_task_id: Option<String>,
    pub active_task_started_at_ms: i64,
    pub work_duration_seconds: i64,
    pub break_duration_seconds: i64,
    pub break_ends_at_ms: Option<i64>,
    pub active_block_id: Option<String>,
    pub accumulated_seconds: Option<i64>,
    pub paused_at_ms: Option<i64>,
    pub checkpoint_at_ms: Option<i64>,
    /// A timer: the block in hand has no length and counts up, and
    /// `work_duration_seconds` is only a stand-in for clients that predate
    /// it. See [`crate::focus::OPEN_ENDED_SECONDS`].
    pub open_ended: bool,
}

/// A row of `focus_queue_items`. `FocusQueueItem`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct QueueItemRow {
    pub id: String,
    pub session_id: String,
    pub task_id: String,
    pub sort_order: i64,
    pub state: String,
    pub planned_seconds: Option<i64>,
    pub completed_at_ms: Option<i64>,
    pub skipped_at_ms: Option<i64>,
    pub created_at_ms: i64,
    /// Timed rather than a pomodoro when it takes over.
    pub open_ended: bool,
}

/// A queued item with its task. `FocusQueueTask`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct QueueEntry {
    pub item: QueueItemRow,
    pub task: TaskRow,
}

/// A row of `focus_work_blocks`. `FocusWorkBlock`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct WorkBlockRow {
    pub id: String,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub task_title: String,
    pub seconds: i64,
    pub recorded_at_ms: i64,
    pub original_task_id: Option<String>,
}

/// A row of `focus_awards`. `FocusAward`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AwardRow {
    pub id: String,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub task_title: String,
    pub seconds: i64,
    pub minutes: f64,
    pub multiplier: f64,
    pub points: f64,
    pub awarded_at_ms: i64,
}

/// Focus points over three windows. `FocusPointsSummary`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PointsSummary {
    pub today: f64,
    pub last_7_days: f64,
    pub all_time: f64,
    pub blocks_today: i64,
}

/// Seconds logged against a task, across every block that names it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LoggedWork {
    pub task_id: String,
    pub seconds: i64,
}

/// A row of `themes`. `StoredTheme`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ThemeRow {
    pub id: String,
    pub json: String,
    pub updated_at_ms: i64,
}

/// A row of `preferences`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PreferenceRow {
    pub key: String,
    pub value: Option<String>,
}

impl DailyRow {
    pub(crate) fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            task_id: row.get("taskId")?,
            active_weekdays_mask: row.get("activeWeekdaysMask")?,
            interval_days: row.get("intervalDays")?,
            interval_anchor_ms: optional_ms(row, "intervalAnchor")?,
            target_seconds: row.get("targetSeconds")?,
            sort_order: row.get("sortOrder")?,
            archived_at_ms: optional_ms(row, "archivedAt")?,
            legacy_daily_id: row.get("legacyDailyId")?,
            created_at_ms: required_ms(row, "createdAt")?,
            updated_at_ms: required_ms(row, "updatedAt")?,
            source_task_id: row.get("sourceTaskId")?,
            placement_column: row.get("placementColumn")?,
            drops_at_day_end: row.get("dropsAtDayEnd")?,
            expiry_rule: row.get("expiryRule")?,
            expires_at_ms: optional_ms(row, "expiresAt")?,
        })
    }
}

impl ContributionRow {
    pub(crate) fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            daily_id: row.get("dailyId")?,
            task_id: row.get("taskId")?,
            day_key: row.get("dayKey")?,
            seconds_logged: row.get("secondsLogged")?,
            completed_at_ms: optional_ms(row, "completedAt")?,
            created_at_ms: required_ms(row, "createdAt")?,
            skipped_at_ms: optional_ms(row, "skippedAt")?,
        })
    }
}

impl ConditionRow {
    pub(crate) fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            workspace_id: row.get("workspaceId")?,
            name: row.get("name")?,
            is_location: row.get("isLocation")?,
            is_archived: row.get("isArchived")?,
            created_at_ms: required_ms(row, "createdAt")?,
            updated_at_ms: required_ms(row, "updatedAt")?,
            category: row.get("category")?,
            // Absent before v27; a database the CLI opens unmigrated has none.
            sort_order: row.get("sortOrder").unwrap_or(None),
            // Absent before v34, like the order before v27.
            provides: row
                .get::<_, Option<String>>("providesJSON")
                .unwrap_or(None)
                .map(|json| crate::conditions::provided_ids(&json))
                .unwrap_or_default(),
        })
    }
}

impl MetadataRow {
    pub(crate) fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            task_id: row.get("taskId")?,
            priority: row.get("priority")?,
            start_at_ms: optional_ms(row, "startAt")?,
            tags_json: row.get("tagsJSON")?,
            recurrence_rule: row.get("recurrenceRule")?,
            matrix_urgency: row.get("matrixUrgency")?,
            matrix_importance: row.get("matrixImportance")?,
            matrix_rank: row.get("matrixRank")?,
            kanban_column: row.get("kanbanColumn")?,
            external_links_json: row.get("externalLinksJSON")?,
            focus_rank: row.get("focusRank")?,
            updated_at_ms: required_ms(row, "updatedAt")?,
            planning_json: row.get("planningJSON")?,
            waiting_on: row.get("waitingOn")?,
            waiting_follow_up_at_ms: optional_ms(row, "waitingFollowUpAt")?,
            waiting_follow_up_task_id: row.get("waitingFollowUpTaskId")?,
            follow_up_of_task_id: row.get("followUpOfTaskId")?,
            waiting_follow_up_every_days: row.get("waitingFollowUpEveryDays")?,
            waiting_follow_up_count: row.get("waitingFollowUpCount")?,
        })
    }
}

impl SessionRow {
    pub(crate) fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            started_at_ms: required_ms(row, "startedAt")?,
            ended_at_ms: optional_ms(row, "endedAt")?,
            phase: row.get("phase")?,
            active_task_id: row.get("activeTaskId")?,
            active_task_started_at_ms: required_ms(row, "activeTaskStartedAt")?,
            work_duration_seconds: row.get("workDurationSeconds")?,
            break_duration_seconds: row.get("breakDurationSeconds")?,
            break_ends_at_ms: optional_ms(row, "breakEndsAt")?,
            active_block_id: row.get("activeBlockId")?,
            accumulated_seconds: row.get("accumulatedSeconds")?,
            paused_at_ms: optional_ms(row, "pausedAt")?,
            checkpoint_at_ms: optional_ms(row, "checkpointAt")?,
            open_ended: row.get("openEnded")?,
        })
    }
}

impl QueueItemRow {
    pub(crate) fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            session_id: row.get("sessionId")?,
            task_id: row.get("taskId")?,
            sort_order: row.get("sortOrder")?,
            state: row.get("state")?,
            planned_seconds: row.get("plannedSeconds")?,
            completed_at_ms: optional_ms(row, "completedAt")?,
            skipped_at_ms: optional_ms(row, "skippedAt")?,
            created_at_ms: required_ms(row, "createdAt")?,
            open_ended: row.get("openEnded")?,
        })
    }
}

impl WorkBlockRow {
    pub(crate) fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            session_id: row.get("sessionId")?,
            task_id: row.get("taskId")?,
            task_title: row.get("taskTitle")?,
            seconds: row.get("seconds")?,
            recorded_at_ms: required_ms(row, "recordedAt")?,
            original_task_id: row.get("originalTaskId")?,
        })
    }
}

impl AwardRow {
    pub(crate) fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            session_id: row.get("sessionId")?,
            task_id: row.get("taskId")?,
            task_title: row.get("taskTitle")?,
            seconds: row.get("seconds")?,
            minutes: row.get("minutes")?,
            multiplier: row.get("multiplier")?,
            points: row.get("points")?,
            awarded_at_ms: required_ms(row, "awardedAt")?,
        })
    }
}

// Dailies.

/// The live dailies in order. `WorkspaceStore.allDailies`.
pub fn all_dailies(connection: &Connection) -> Result<Vec<DailyRow>, CoreError> {
    all(
        connection,
        "SELECT * FROM dailies WHERE archivedAt IS NULL ORDER BY sortOrder, createdAt",
        [],
        DailyRow::from_row,
    )
}

/// One daily by id, archived or not.
pub fn daily(connection: &Connection, id: &str) -> Result<Option<DailyRow>, CoreError> {
    Ok(all(
        connection,
        "SELECT * FROM dailies WHERE id = ?1",
        [id],
        DailyRow::from_row,
    )?
    .pop())
}

/// A task's live daily, if it has one. `WorkspaceStore.daily(forTaskId:)`.
pub fn daily_for_task(
    connection: &Connection,
    task_id: &str,
) -> Result<Option<DailyRow>, CoreError> {
    Ok(all(
        connection,
        "SELECT * FROM dailies WHERE taskId = ?1 AND archivedAt IS NULL LIMIT 1",
        [task_id],
        DailyRow::from_row,
    )?
    .pop())
}

/// One contribution by id.
pub fn contribution(
    connection: &Connection,
    id: &str,
) -> Result<Option<ContributionRow>, CoreError> {
    Ok(all(
        connection,
        "SELECT * FROM daily_contributions WHERE id = ?1",
        [id],
        ContributionRow::from_row,
    )?
    .pop())
}

/// A daily's contributions on the given days, oldest first.
/// `WorkspaceStore.contributionHistory`.
pub fn contributions(
    connection: &Connection,
    daily_id: &str,
    day_keys: &[String],
) -> Result<Vec<ContributionRow>, CoreError> {
    let mut result = Vec::new();
    for chunk in day_keys.chunks(400) {
        let marks = vec!["?"; chunk.len()].join(", ");
        let mut params: Vec<&str> = vec![daily_id];
        params.extend(chunk.iter().map(String::as_str));
        result.extend(all(
            connection,
            &format!(
                "SELECT * FROM daily_contributions WHERE dailyId = ? AND dayKey IN ({marks}) ORDER BY dayKey"
            ),
            rusqlite::params_from_iter(params),
            ContributionRow::from_row,
        )?);
    }
    result.sort_by(|a, b| a.day_key.cmp(&b.day_key));
    Ok(result)
}

/// The dailies a day shows, each with its task and that day's contribution:
/// a habit when its rule shows it, any other daily when it is due, and never
/// one whose task is gone or is a list. `WorkspaceStore.dailies(on:)`.
pub fn dailies_on(
    connection: &Connection,
    day_ms: i64,
    zone: &str,
) -> Result<Vec<DayDaily>, CoreError> {
    let tz = crate::periodic::zone(zone);
    let key = crate::dailies::day_key(day_ms, zone);
    let mut result = Vec::new();
    for daily in all_dailies(connection)? {
        let shows = if daily.placement_column.is_some() {
            crate::habits::habit_shows(connection, &daily.id, day_ms, tz)?
        } else {
            crate::focus::daily_due(connection, &daily.id, day_ms, tz)?
        };
        if !shows {
            continue;
        }
        let Some(task) = crate::records::task(connection, &daily.task_id)? else {
            continue;
        };
        if task.item_kind.as_deref() == Some("list") {
            continue;
        }
        let contribution = all(
            connection,
            "SELECT * FROM daily_contributions WHERE dailyId = ?1 AND dayKey = ?2 LIMIT 1",
            [&daily.id, &key],
            ContributionRow::from_row,
        )?
        .pop();
        result.push(DayDaily {
            daily,
            task,
            contribution,
        });
    }
    Ok(result)
}

// Conditions and metadata.

/// A workspace's conditions, oldest first. `WorkspaceStore.conditions(in:)`.
pub fn conditions(
    connection: &Connection,
    workspace_id: &str,
) -> Result<Vec<ConditionRow>, CoreError> {
    all(
        connection,
        "SELECT * FROM task_conditions WHERE workspaceId = ?1 ORDER BY createdAt, id",
        [workspace_id],
        ConditionRow::from_row,
    )
}

/// One condition by id.
pub fn condition(connection: &Connection, id: &str) -> Result<Option<ConditionRow>, CoreError> {
    Ok(all(
        connection,
        "SELECT * FROM task_conditions WHERE id = ?1",
        [id],
        ConditionRow::from_row,
    )?
    .pop())
}

/// A task's metadata row, if it has one.
pub fn metadata(connection: &Connection, task_id: &str) -> Result<Option<MetadataRow>, CoreError> {
    Ok(all(
        connection,
        "SELECT * FROM task_metadata WHERE taskId = ?1",
        [task_id],
        MetadataRow::from_row,
    )?
    .pop())
}

/// The metadata rows of the given tasks; a task without one is absent.
/// `WorkspaceStore.boardMetadata(for:)`.
pub fn metadata_for_tasks(
    connection: &Connection,
    task_ids: &[String],
) -> Result<Vec<MetadataRow>, CoreError> {
    let mut unique: Vec<&String> = task_ids.iter().collect();
    unique.sort();
    unique.dedup();
    let mut result = Vec::new();
    // Stays below SQLite's parameter limit even for large imported trees.
    for chunk in unique.chunks(500) {
        let marks = vec!["?"; chunk.len()].join(", ");
        result.extend(all(
            connection,
            &format!("SELECT * FROM task_metadata WHERE taskId IN ({marks})"),
            rusqlite::params_from_iter(chunk),
            MetadataRow::from_row,
        )?);
    }
    Ok(result)
}

/// Every metadata row.
pub fn all_metadata(connection: &Connection) -> Result<Vec<MetadataRow>, CoreError> {
    all(
        connection,
        "SELECT * FROM task_metadata",
        [],
        MetadataRow::from_row,
    )
}

/// The metadata rows that say a task waits on someone, when to chase it, or
/// whose follow-up it is: the few `WorkspaceStore.waitingDetails` reads,
/// rather than every row.
pub fn waiting_metadata(connection: &Connection) -> Result<Vec<MetadataRow>, CoreError> {
    all(
        connection,
        "SELECT * FROM task_metadata
         WHERE waitingOn IS NOT NULL OR waitingFollowUpAt IS NOT NULL OR followUpOfTaskId IS NOT NULL",
        [],
        MetadataRow::from_row,
    )
}

/// The metadata rows that carry planning, a planning blob or a start time:
/// the few `WorkspaceStore.taskPlanningValues` reads, rather than every row.
pub fn planning_metadata(connection: &Connection) -> Result<Vec<MetadataRow>, CoreError> {
    all(
        connection,
        "SELECT * FROM task_metadata WHERE planningJSON IS NOT NULL OR startAt IS NOT NULL",
        [],
        MetadataRow::from_row,
    )
}

// Focus.

/// The newest session that has not finished. `WorkspaceStore.activeFocusSession`.
pub fn active_session(connection: &Connection) -> Result<Option<SessionRow>, CoreError> {
    Ok(all(
        connection,
        "SELECT * FROM focus_sessions WHERE phase <> 'finished' ORDER BY startedAt DESC LIMIT 1",
        [],
        SessionRow::from_row,
    )?
    .pop())
}

/// One session by id.
pub fn session(connection: &Connection, id: &str) -> Result<Option<SessionRow>, CoreError> {
    Ok(all(
        connection,
        "SELECT * FROM focus_sessions WHERE id = ?1",
        [id],
        SessionRow::from_row,
    )?
    .pop())
}

/// A session's queue in order, each with its task, leaving out lists and
/// tasks that are gone. `WorkspaceStore.focusQueue(for:)`.
pub fn queue(connection: &Connection, session_id: &str) -> Result<Vec<QueueEntry>, CoreError> {
    let items = all(
        connection,
        "SELECT * FROM focus_queue_items WHERE sessionId = ?1 ORDER BY sortOrder, createdAt",
        [session_id],
        QueueItemRow::from_row,
    )?;
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        let Some(task) = crate::records::task(connection, &item.task_id)? else {
            continue;
        };
        if task.item_kind.as_deref() == Some("list") {
            continue;
        }
        result.push(QueueEntry { item, task });
    }
    Ok(result)
}

/// Seconds logged per task, counting a block against the task it was moved
/// from when its own is gone. `WorkspaceStore.loggedWorkTotals`.
pub fn logged_work(connection: &Connection) -> Result<Vec<LoggedWork>, CoreError> {
    all(
        connection,
        "SELECT COALESCE(taskId, originalTaskId) AS taskId, SUM(seconds) AS seconds
         FROM focus_work_blocks WHERE COALESCE(taskId, originalTaskId) IS NOT NULL
         GROUP BY COALESCE(taskId, originalTaskId)",
        [],
        |row| {
            Ok(LoggedWork {
                task_id: row.get("taskId")?,
                seconds: row.get("seconds")?,
            })
        },
    )
}

/// A task's work blocks, oldest first. `WorkspaceStore.workBlocks(for:)`.
pub fn work_blocks_for_task(
    connection: &Connection,
    task_id: &str,
) -> Result<Vec<WorkBlockRow>, CoreError> {
    all(
        connection,
        "SELECT * FROM focus_work_blocks WHERE taskId = ?1 OR originalTaskId = ?1 ORDER BY recordedAt",
        [task_id],
        WorkBlockRow::from_row,
    )
}

/// Work blocks recorded in `[from, to)`. `WorkspaceStore.focusWorkBlocks(in:)`.
pub fn work_blocks_between(
    connection: &Connection,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<WorkBlockRow>, CoreError> {
    all(
        connection,
        "SELECT * FROM focus_work_blocks WHERE recordedAt >= ?1 AND recordedAt < ?2 ORDER BY recordedAt, id",
        [stored(from_ms), stored(to_ms)],
        WorkBlockRow::from_row,
    )
}

/// The seconds and moment of each work block recorded in `[from, to)`, for
/// sums that need neither titles nor ids.
pub fn work_block_seconds_between(
    connection: &Connection,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<crate::progress::WorkBlockSeconds>, CoreError> {
    all(
        connection,
        "SELECT seconds, recordedAt FROM focus_work_blocks WHERE recordedAt >= ?1 AND recordedAt < ?2",
        [stored(from_ms), stored(to_ms)],
        |row| {
            Ok(crate::progress::WorkBlockSeconds {
                seconds: row.get("seconds")?,
                recorded_at_ms: required_ms(row, "recordedAt")?,
            })
        },
    )
}

/// One award by id.
pub fn award(connection: &Connection, id: &str) -> Result<Option<AwardRow>, CoreError> {
    Ok(all(
        connection,
        "SELECT * FROM focus_awards WHERE id = ?1",
        [id],
        AwardRow::from_row,
    )?
    .pop())
}

/// The newest awards. `WorkspaceStore.focusAwards(limit:)`.
pub fn recent_awards(connection: &Connection, limit: i64) -> Result<Vec<AwardRow>, CoreError> {
    all(
        connection,
        "SELECT * FROM focus_awards ORDER BY awardedAt DESC LIMIT ?1",
        [limit.max(0)],
        AwardRow::from_row,
    )
}

/// Awards in `[from, to)`, newest first. `WorkspaceStore.focusAwards(onDayOf:)`.
pub fn awards_between(
    connection: &Connection,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<AwardRow>, CoreError> {
    all(
        connection,
        "SELECT * FROM focus_awards WHERE awardedAt >= ?1 AND awardedAt < ?2 ORDER BY awardedAt DESC",
        [stored(from_ms), stored(to_ms)],
        AwardRow::from_row,
    )
}

/// Points today, over the last seven days and ever, and today's blocks.
/// `WorkspaceStore.focusPointsSummary`.
pub fn points_summary(
    connection: &Connection,
    today_ms: i64,
    tomorrow_ms: i64,
    week_start_ms: i64,
) -> Result<PointsSummary, CoreError> {
    let total = |from: Option<i64>, to: Option<i64>| -> Result<f64, CoreError> {
        Ok(connection.query_row(
            "SELECT COALESCE(SUM(points), 0) FROM focus_awards
             WHERE (?1 IS NULL OR awardedAt >= ?1) AND (?2 IS NULL OR awardedAt < ?2)",
            [from.map(stored), to.map(stored)],
            |row| row.get(0),
        )?)
    };
    Ok(PointsSummary {
        today: total(Some(today_ms), Some(tomorrow_ms))?,
        last_7_days: total(Some(week_start_ms), Some(tomorrow_ms))?,
        all_time: total(None, None)?,
        blocks_today: connection.query_row(
            "SELECT COUNT(*) FROM focus_awards WHERE awardedAt >= ?1 AND awardedAt < ?2",
            [stored(today_ms), stored(tomorrow_ms)],
            |row| row.get(0),
        )?,
    })
}

// Tasks over time.

/// When tasks, not lists, were completed in `[from, to)`, in order.
/// `WorkspaceStore.taskCompletions(in:)`.
pub fn completions_between(
    connection: &Connection,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<i64>, CoreError> {
    moments(
        connection,
        "SELECT completedAt FROM tasks
         WHERE completedAt IS NOT NULL AND completedAt >= ?1 AND completedAt < ?2
           AND COALESCE(itemKind, 'task') <> 'list'
         ORDER BY completedAt",
        from_ms,
        to_ms,
    )
}

/// When tasks, not lists, were created in `[from, to)`, in order.
/// `WorkspaceStore.taskCreations(in:)`.
pub fn creations_between(
    connection: &Connection,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<i64>, CoreError> {
    moments(
        connection,
        "SELECT createdAt FROM tasks
         WHERE createdAt >= ?1 AND createdAt < ?2 AND COALESCE(itemKind, 'task') <> 'list'
         ORDER BY createdAt",
        from_ms,
        to_ms,
    )
}

fn moments(
    connection: &Connection,
    sql: &str,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<i64>, CoreError> {
    all(connection, sql, [stored(from_ms), stored(to_ms)], |row| {
        Ok(row
            .get::<_, Option<String>>(0)?
            .as_deref()
            .and_then(crate::time::parse_stored)
            .map(|at| at.timestamp_millis()))
    })
    .map(|values| values.into_iter().flatten().collect())
}

/// Tasks, not lists, completed since `since`, newest first.
/// `WorkspaceStore.completedTasks(since:limit:)`.
pub fn completed_since(
    connection: &Connection,
    since_ms: i64,
    limit: i64,
) -> Result<Vec<TaskRow>, CoreError> {
    all(
        connection,
        "SELECT * FROM tasks
         WHERE completedAt IS NOT NULL AND completedAt >= ?1 AND COALESCE(itemKind, 'task') <> 'list'
         ORDER BY completedAt DESC, id DESC LIMIT ?2",
        rusqlite::params![stored(since_ms), limit],
        TaskRow::from_row,
    )
}

/// Tasks, not lists, closed in `[from, to)`, oldest first; cancellations
/// included. The review's day.
pub fn closed_between(
    connection: &Connection,
    from_ms: i64,
    to_ms: i64,
) -> Result<Vec<TaskRow>, CoreError> {
    all(
        connection,
        "SELECT * FROM tasks
         WHERE completedAt IS NOT NULL AND completedAt >= ?1 AND completedAt < ?2
           AND COALESCE(itemKind, 'task') <> 'list'
         ORDER BY completedAt, id",
        [stored(from_ms), stored(to_ms)],
        TaskRow::from_row,
    )
}

/// Where a completion falls in its day and in a run of days.
/// `WorkspaceStore.CompletionContext`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CompletionContext {
    /// Counting the completion about to happen, so the first of the day is 1.
    pub ordinal_today: i64,
    /// Days in a row with something finished, today included.
    pub streak_days: i64,
}

/// Tasks completed on a local day plus the day's completed contributions.
fn finished_on(
    connection: &Connection,
    day: chrono::NaiveDate,
    zone: chrono_tz::Tz,
) -> Result<i64, CoreError> {
    let start = |day: chrono::NaiveDate| {
        crate::periodic::resolve(zone, day.and_hms_opt(0, 0, 0).expect("midnight exists"))
            .timestamp_millis()
    };
    let tasks: i64 = connection.query_row(
        "SELECT COUNT(*) FROM tasks WHERE status = 'completed' AND updatedAt >= ?1 AND updatedAt < ?2",
        [stored(start(day)), stored(start(day + chrono::Duration::days(1)))],
        |row| row.get(0),
    )?;
    let contributions: i64 = connection.query_row(
        "SELECT COUNT(*) FROM daily_contributions WHERE dayKey = ?1 AND completedAt IS NOT NULL",
        [day.format("%Y-%m-%d").to_string()],
        |row| row.get(0),
    )?;
    Ok(tasks + contributions)
}

/// How many things today has finished, and how many days in a row have
/// finished something. An empty today does not break the run: the
/// completion being described is about to fill it. Bounded at a year.
/// `WorkspaceStore.completionContext`.
pub fn completion_context(
    connection: &Connection,
    now_ms: i64,
    zone: &str,
) -> Result<CompletionContext, CoreError> {
    let tz = crate::periodic::zone(zone);
    let today = crate::habits::local_day(now_ms, tz);
    let ordinal_today = finished_on(connection, today, tz)? + 1;
    let mut streak_days = 0;
    for offset in 0..366 {
        let day = today - chrono::Duration::days(offset);
        if offset == 0 || finished_on(connection, day, tz)? > 0 {
            streak_days += 1;
        } else {
            break;
        }
    }
    Ok(CompletionContext {
        ordinal_today,
        streak_days,
    })
}

/// Whether any task is pinned in Today's order. `WorkspaceStore.hasManualFocusOrder`.
pub fn has_manual_focus_order(connection: &Connection) -> Result<bool, CoreError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM task_metadata WHERE focusRank IS NOT NULL)",
        [],
        |row| row.get(0),
    )?)
}

// Boards and counts.

/// Every saved board's columns, as stored.
pub fn kanban_boards(
    connection: &Connection,
) -> Result<Vec<crate::imports::BoardBaseline>, CoreError> {
    all(
        connection,
        "SELECT id, columnsJSON FROM kanban_boards ORDER BY id",
        [],
        |row| {
            Ok(crate::imports::BoardBaseline {
                key: row.get(0)?,
                columns_json: row.get(1)?,
            })
        },
    )
}

/// Open tasks in one list.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ListCount {
    pub list_id: String,
    pub open: i64,
}

/// Open and closed task counts, for overview screens.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TaskCounts {
    /// Open tasks, not lists.
    pub open: i64,
    /// Completed or cancelled tasks, not lists.
    pub completed: i64,
    /// Open rows per list, lists included, as the sidebar counts them.
    pub by_list: Vec<ListCount>,
}

/// The overview's counts in one read.
pub fn task_counts(connection: &Connection) -> Result<TaskCounts, CoreError> {
    let count = |sql: &str| -> Result<i64, CoreError> {
        Ok(connection.query_row(sql, [], |row| row.get(0))?)
    };
    Ok(TaskCounts {
        open: count(
            "SELECT COUNT(*) FROM tasks WHERE status = 'open' AND COALESCE(itemKind, 'task') <> 'list'",
        )?,
        completed: count(
            "SELECT COUNT(*) FROM tasks WHERE status <> 'open' AND COALESCE(itemKind, 'task') <> 'list'",
        )?,
        by_list: all(
            connection,
            "SELECT listId, COUNT(*) FROM tasks WHERE status = 'open' GROUP BY listId ORDER BY listId",
            [],
            |row| {
                Ok(ListCount {
                    list_id: row.get(0)?,
                    open: row.get(1)?,
                })
            },
        )?,
    })
}

// Themes and preferences.

/// The stored themes by id. `WorkspaceStore.themes`.
pub fn themes(connection: &Connection) -> Result<Vec<ThemeRow>, CoreError> {
    all(
        connection,
        "SELECT id, json, updatedAt FROM themes ORDER BY id",
        [],
        |row| {
            Ok(ThemeRow {
                id: row.get("id")?,
                json: row.get("json")?,
                updated_at_ms: required_ms(row, "updatedAt")?,
            })
        },
    )
}

/// Every preference. `WorkspaceStore.preferences`.
pub fn preferences(connection: &Connection) -> Result<Vec<PreferenceRow>, CoreError> {
    all(
        connection,
        "SELECT key, value FROM preferences ORDER BY key",
        [],
        |row| {
            Ok(PreferenceRow {
                key: row.get("key")?,
                value: row.get("value")?,
            })
        },
    )
}

#[cfg(test)]
mod tests;
