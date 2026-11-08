//! Focus sessions: the queue of tasks a sitting works through, the clock on
//! the block in hand, and finishing a block, which completes the task or
//! credits a daily. Also the part of the next-up engine they need: which
//! open tasks are candidates and whether one is available in a context.
//! Replaces `WorkspaceStore+Focus.swift` and `+Work.swift`'s writes,
//! `focusCandidates`, and `TaskAvailabilityPolicy` in TaktCore.
//!
//! Focus sessions and their queue are not journalled: a timer is a record of
//! what happened, not an edit to take back. Finishing a block is, because it
//! completes a task.

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::CoreError;
use crate::dailies;
use crate::habits;
use crate::lists::new_id;
use crate::periodic;
use crate::planning::Planning;
use crate::time::{parse_stored, stored};

/// The circumstances a sitting is planned in: which conditions hold, when the
/// time available ends, and whether the aim is progress or finishing.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct FocusContext {
    pub condition_ids: Vec<String>,
    pub ends_at_ms: Option<i64>,
    /// "progress" or "finish".
    pub mode: String,
}

impl FocusContext {
    fn finishing(&self) -> bool {
        self.mode == "finish"
    }
}

/// An open task the next-up engine can choose. `NextUpCandidate`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Candidate {
    pub id: String,
    pub title: String,
    pub is_daily_due_today: bool,
    pub due_at_ms: Option<i64>,
    pub start_at_ms: Option<i64>,
    pub matrix_urgency: Option<i64>,
    pub matrix_importance: Option<i64>,
    pub priority: Option<i64>,
    pub estimate_seconds: Option<i64>,
    pub kanban_column: Option<String>,
    pub focus_rank: Option<i64>,
    pub sort_order: i64,
    pub created_at_ms: i64,
    pub due_date: Option<String>,
    pub requirement_groups: Vec<Vec<String>>,
    pub logged_seconds: i64,
    pub minimum_block_seconds: Option<i64>,
    pub requires_single_sitting: bool,
    pub daily_remaining_seconds: Option<i64>,
    /// "dailyNotScheduled" or "dailyAlreadyMet" when its daily rules it out.
    pub daily_unavailable: Option<String>,
}

impl Candidate {
    /// What is left of the work: the daily's remainder, or the estimate less
    /// what is logged.
    pub fn remaining_seconds(&self) -> Option<i64> {
        if let Some(daily) = self.daily_remaining_seconds {
            return Some(daily.max(0));
        }
        self.estimate_seconds
            .map(|estimate| (estimate - self.logged_seconds.max(0)).max(0))
    }
}

/// Why a candidate cannot be worked on now. `TaskUnavailableReason`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Unavailable {
    StartsLater { at_ms: i64 },
    MissingConditions { groups: Vec<Vec<String>> },
    InsufficientTime { seconds: i64 },
    NeedsEstimate,
    ExpiredWindow,
    DailyNotScheduled,
    DailyAlreadyMet,
}

/// Every reason `candidate` is not available in `context` at `now`; empty
/// when it is. `TaskAvailabilityPolicy.reasons`.
pub fn reasons(candidate: &Candidate, context: &FocusContext, now_ms: i64) -> Vec<Unavailable> {
    let mut result = Vec::new();
    match candidate.daily_unavailable.as_deref() {
        Some("dailyNotScheduled") => result.push(Unavailable::DailyNotScheduled),
        Some("dailyAlreadyMet") => result.push(Unavailable::DailyAlreadyMet),
        _ => {}
    }
    if let Some(start) = candidate.start_at_ms.filter(|start| *start > now_ms) {
        result.push(Unavailable::StartsLater { at_ms: start });
    }
    let held: HashSet<&str> = context.condition_ids.iter().map(String::as_str).collect();
    let missing: Vec<Vec<String>> = candidate
        .requirement_groups
        .iter()
        .filter(|group| !crate::conditions::group_is_met(group, &held))
        .cloned()
        .collect();
    if !missing.is_empty() {
        result.push(Unavailable::MissingConditions { groups: missing });
    }
    let window = context
        .ends_at_ms
        .map(|end| ((end - now_ms) as f64 / 1000.0).max(0.0));
    if window.is_some_and(|window| window < 60.0) {
        result.push(Unavailable::ExpiredWindow);
    }
    let minimum = candidate.minimum_block_seconds.unwrap_or(60).max(60);
    if candidate.requires_single_sitting || context.finishing() {
        let Some(remaining) = candidate
            .remaining_seconds()
            .filter(|remaining| *remaining > 0)
        else {
            result.push(Unavailable::NeedsEstimate);
            return result;
        };
        let needed = remaining.max(minimum);
        if window.is_some_and(|window| needed as f64 > window) {
            result.push(Unavailable::InsufficientTime { seconds: needed });
        }
    } else if window.is_some_and(|window| minimum as f64 > window) {
        result.push(Unavailable::InsufficientTime { seconds: minimum });
    }
    result
}

/// How long a block on `candidate` should be. `TaskAvailabilityPolicy.plannedSeconds`.
pub fn planned_seconds(
    candidate: &Candidate,
    requested: Option<i64>,
    context: &FocusContext,
    now_ms: i64,
) -> i64 {
    let needed = if candidate.requires_single_sitting {
        candidate.remaining_seconds().unwrap_or(60)
    } else {
        60
    };
    let seconds = needed
        .max(60)
        .max(candidate.minimum_block_seconds.unwrap_or(60))
        .max(requested.unwrap_or_else(|| suggested_seconds(candidate, context, now_ms)));
    match context.ends_at_ms {
        Some(end) => seconds.min(((end - now_ms) / 1000).max(0)),
        None => seconds,
    }
}

/// The block length to offer. `TaskAvailabilityPolicy.suggestedSeconds`.
pub fn suggested_seconds(candidate: &Candidate, context: &FocusContext, now_ms: i64) -> i64 {
    let remaining = candidate
        .remaining_seconds()
        .filter(|remaining| *remaining > 0);
    let suggested = 60
        .max(candidate.minimum_block_seconds.unwrap_or(60))
        .max(remaining.unwrap_or(25 * 60));
    match context.ends_at_ms {
        Some(end) if !candidate.requires_single_sitting => {
            suggested.min(((end - now_ms) / 1000).max(0))
        }
        _ => suggested,
    }
}

/// The open tasks the next-up engine and the focus queue choose from: not
/// lists, not inside a closed or archived list item, not a list's wrapper,
/// not in an archived or completed list, and not a parent of open work. A
/// daily that rules a task out keeps it only if it has a deadline.
/// `WorkspaceStore.focusCandidates`.
pub fn candidates(
    connection: &Connection,
    now_ms: i64,
    zone: &str,
) -> Result<Vec<Candidate>, CoreError> {
    let tz = periodic::zone(zone);
    let day_key = dailies::day_key(now_ms, zone);
    let closed_lists = string_set(
        connection,
        "SELECT id FROM task_lists WHERE isArchived OR completedAt IS NOT NULL",
    )?;
    let wrappers = string_set(
        connection,
        "SELECT visibleRootTaskId FROM task_lists WHERE visibleRootTaskId IS NOT NULL",
    )?;
    let parents = string_set(
        connection,
        "SELECT DISTINCT parentTaskId FROM tasks WHERE parentTaskId IS NOT NULL AND status = 'open'",
    )?;
    let inactive = inactive_container_items(connection, None)?;
    // A task waiting on an open task is not next, whatever else it has going
    // for it (`dependencies`).
    let blocked: HashSet<String> = crate::dependencies::blocked_task_ids(connection)?
        .into_iter()
        .collect();
    // A shared task someone else has taken on is theirs to do
    // (`assignment`).
    let elsewhere = crate::assignment::assigned_elsewhere(connection)?;
    let mut work: HashMap<String, i64> = HashMap::new();
    {
        let mut statement = connection.prepare(
            "SELECT COALESCE(taskId, originalTaskId), SUM(seconds) FROM focus_work_blocks
             WHERE COALESCE(taskId, originalTaskId) IS NOT NULL GROUP BY COALESCE(taskId, originalTaskId)",
        )?;
        for row in statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })? {
            let (id, seconds) = row?;
            work.insert(id, seconds);
        }
    }

    // A task filed nowhere stands in its nearest filed ancestor's column, as
    // the board draws it, so a subtask under a card planned for today is on
    // today too.
    let mut filing: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    {
        let mut statement = connection.prepare(
            "SELECT t.id, t.parentTaskId, m.kanbanColumn FROM tasks t
             LEFT JOIN task_metadata m ON m.taskId = t.id",
        )?;
        for row in statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get(1)?, row.get(2)?))
        })? {
            let (id, parent, column) = row?;
            filing.insert(id, (parent, column));
        }
    }
    let inherited = |id: &str| -> Option<String> {
        let mut current = filing.get(id).and_then(|(parent, _)| parent.clone());
        for _ in 0..filing.len() {
            let (parent, column) = filing.get(current.as_deref()?)?;
            if column.is_some() {
                return column.clone();
            }
            current = parent.clone();
        }
        None
    };

    let mut statement = connection.prepare(
        "SELECT t.id, t.title, t.listId, t.dueAt, t.estimateSeconds, t.sortOrder, t.createdAt,
                COALESCE(t.itemKind, 'task'),
                m.startAt, m.matrixUrgency, m.matrixImportance, m.priority, m.kanbanColumn, m.focusRank, m.planningJSON,
                d.id, d.targetSeconds, d.placementColumn, d.intervalDays, d.intervalAnchor, d.createdAt,
                d.activeWeekdaysMask, c.secondsLogged, COALESCE(c.completedAt, c.skippedAt)
         FROM tasks t
         LEFT JOIN task_metadata m ON m.taskId = t.id
         LEFT JOIN dailies d ON d.id = (SELECT id FROM dailies WHERE taskId = t.id AND archivedAt IS NULL ORDER BY rowid LIMIT 1)
         LEFT JOIN daily_contributions c ON c.dailyId = d.id AND c.dayKey = ?1
         WHERE t.status = 'open'
         ORDER BY t.rowid",
    )?;
    let rows = statement.query_map([&day_key], |row| {
        Ok(Row {
            id: row.get(0)?,
            title: row.get(1)?,
            list_id: row.get(2)?,
            due_at: row.get(3)?,
            estimate: row.get(4)?,
            sort_order: row.get(5)?,
            created_at: row.get(6)?,
            kind: row.get(7)?,
            start_at: row.get(8)?,
            urgency: row.get(9)?,
            importance: row.get(10)?,
            priority: row.get(11)?,
            column: row.get(12)?,
            rank: row.get(13)?,
            planning: row.get(14)?,
            daily_id: row.get(15)?,
            daily_target: row.get(16)?,
            daily_placement: row.get(17)?,
            daily_interval: row.get(18)?,
            daily_anchor: row.get(19)?,
            daily_created: row.get(20)?,
            daily_mask: row.get(21)?,
            logged_today: row.get(22)?,
            done_today: row.get(23)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        let row = row?;
        if row.kind == "list"
            || inactive.contains(&row.id)
            || wrappers.contains(&row.id)
            || closed_lists.contains(&row.list_id)
            || parents.contains(&row.id)
            || blocked.contains(&row.id)
            || elsewhere.contains(&row.id)
        {
            continue;
        }
        // Unreadable planning is no planning here, so one bad blob cannot
        // empty the ladder.
        let planning = row
            .planning
            .as_deref()
            .and_then(|json| Planning::from_json(json).ok())
            .unwrap_or_default();
        let due_date = planning.due_date;
        let mut daily_unavailable = None;
        if let Some(daily_id) = &row.daily_id {
            let shows = if row
                .daily_placement
                .as_deref()
                .is_some_and(|p| matches!(p, "today" | "this-week" | "waiting-on"))
            {
                habits::habit_shows(connection, daily_id, now_ms, tz)?
            } else {
                daily_is_due(&row, now_ms, tz)
            };
            if !shows {
                daily_unavailable = Some("dailyNotScheduled".to_string());
            } else if row.done_today.is_some()
                || row
                    .daily_target
                    .is_some_and(|target| target > 0 && row.logged_today.unwrap_or(0) >= target)
            {
                daily_unavailable = Some("dailyAlreadyMet".to_string());
            }
            // A met or unscheduled daily disappears, but work with a deadline
            // stays visible among the blocked.
            if daily_unavailable.is_some() && row.due_at.is_none() && due_date.is_none() {
                continue;
            }
        }
        result.push(Candidate {
            is_daily_due_today: row.daily_id.is_some() && daily_unavailable.is_none(),
            due_at_ms: millis(row.due_at.as_deref()),
            start_at_ms: millis(row.start_at.as_deref()),
            matrix_urgency: row.urgency,
            matrix_importance: row.importance,
            priority: row.priority,
            estimate_seconds: row.estimate,
            kanban_column: row.column.or_else(|| inherited(&row.id)),
            focus_rank: row.rank,
            sort_order: row.sort_order,
            created_at_ms: millis(Some(&row.created_at)).unwrap_or(0),
            due_date,
            requirement_groups: planning.requirement_groups.unwrap_or_default(),
            logged_seconds: work.get(&row.id).copied().unwrap_or(0),
            minimum_block_seconds: planning.minimum_block_seconds,
            requires_single_sitting: planning.requires_single_sitting.unwrap_or(false),
            daily_remaining_seconds: row
                .daily_id
                .as_ref()
                .and(row.daily_target)
                .map(|target| (target - row.logged_today.unwrap_or(0)).max(0)),
            daily_unavailable,
            id: row.id,
            title: row.title,
        });
    }
    Ok(result)
}

struct Row {
    id: String,
    title: String,
    list_id: String,
    due_at: Option<String>,
    estimate: Option<i64>,
    sort_order: i64,
    created_at: String,
    kind: String,
    start_at: Option<String>,
    urgency: Option<i64>,
    importance: Option<i64>,
    priority: Option<i64>,
    column: Option<String>,
    rank: Option<i64>,
    planning: Option<String>,
    daily_id: Option<String>,
    daily_target: Option<i64>,
    daily_placement: Option<String>,
    daily_interval: Option<i64>,
    daily_anchor: Option<String>,
    daily_created: Option<String>,
    daily_mask: Option<i64>,
    logged_today: Option<i64>,
    done_today: Option<String>,
}

/// `WorkspaceDaily.isDue(on:)`: on its interval from its anchor, or on one of
/// its weekdays.
fn daily_is_due(row: &Row, now_ms: i64, zone: chrono_tz::Tz) -> bool {
    let anchor_ms = row
        .daily_anchor
        .as_deref()
        .or(row.daily_created.as_deref())
        .and_then(parse_stored)
        .map_or(0, |at| at.timestamp_millis());
    crate::dailies::workspace_daily_is_due(
        row.daily_mask,
        row.daily_interval,
        habits::local_day(anchor_ms, zone),
        habits::local_day(now_ms, zone),
    )
}

/// Every task inside a list item that is archived or closed, and those items
/// themselves, optionally within one list. `WorkspaceStore.inactiveContainerItems`.
pub(crate) fn inactive_container_items(
    connection: &Connection,
    list_id: Option<&str>,
) -> Result<HashSet<String>, CoreError> {
    let mut statement = connection.prepare(
        "WITH RECURSIVE inactive(id) AS (
           SELECT id FROM tasks
           WHERE COALESCE(itemKind, 'task') = 'list' AND (archivedAt IS NOT NULL OR status <> 'open')
             AND (?1 IS NULL OR listId = ?1)
           UNION
           SELECT tasks.id FROM tasks JOIN inactive ON tasks.parentTaskId = inactive.id
         )
         SELECT id FROM inactive",
    )?;
    let ids = statement
        .query_map([list_id], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

fn string_set(connection: &Connection, sql: &str) -> Result<HashSet<String>, CoreError> {
    let mut statement = connection.prepare(sql)?;
    let set = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(set)
}

fn millis(text: Option<&str>) -> Option<i64> {
    text.and_then(parse_stored).map(|at| at.timestamp_millis())
}

/// A task that focus can be started on: not a list, in a live list, not the
/// list's wrapper, not inside a closed list item.
/// `WorkspaceStore.validateActionableTask`.
fn require_actionable(connection: &Connection, id: &str) -> Result<(), CoreError> {
    let task: Option<(String, String)> = connection
        .query_row(
            "SELECT listId, COALESCE(itemKind, 'task') FROM tasks WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((list_id, kind)) = task else {
        return Err(CoreError::MissingTask { id: id.to_string() });
    };
    let list: Option<(bool, Option<String>, Option<String>)> = connection
        .query_row(
            "SELECT isArchived, completedAt, visibleRootTaskId FROM task_lists WHERE id = ?1",
            [&list_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let live = list.is_some_and(|(archived, completed, root)| {
        !archived && completed.is_none() && root.as_deref() != Some(id)
    });
    if kind == "list" || !live || inactive_container_items(connection, Some(&list_id))?.contains(id)
    {
        return Err(CoreError::InvalidTaskMove);
    }
    Ok(())
}

/// Starts a focus session on a task and returns its id, or returns the one
/// already running. With a `context`, the task has to be available in it and
/// the block long enough for it, unless `override_availability` is set.
/// `WorkspaceStore.startFocusSession`.
#[allow(clippy::too_many_arguments)]
pub fn start_session(
    transaction: &Transaction,
    task_id: &str,
    planned_seconds: Option<i64>,
    work_seconds: i64,
    break_seconds: i64,
    context: Option<&FocusContext>,
    override_availability: bool,
    now_ms: i64,
    zone: &str,
) -> Result<String, CoreError> {
    let active: Option<String> = transaction
        .query_row(
            "SELECT id FROM focus_sessions WHERE phase <> 'finished' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(active) = active {
        return Ok(active);
    }
    require_actionable(transaction, task_id)?;
    if let Some(context) = context.filter(|_| !override_availability) {
        let candidate = candidates(transaction, now_ms, zone)?
            .into_iter()
            .find(|candidate| candidate.id == task_id)
            .filter(|candidate| reasons(candidate, context, now_ms).is_empty())
            .ok_or(CoreError::Unavailable)?;
        let chosen = planned_seconds.unwrap_or(work_seconds);
        let fits_minimum = chosen >= candidate.minimum_block_seconds.unwrap_or(60).max(60);
        let fits_sitting = !candidate.requires_single_sitting
            || chosen >= candidate.remaining_seconds().unwrap_or(i64::MAX);
        let fits_window = context
            .ends_at_ms
            .is_none_or(|end| chosen as f64 <= (end - now_ms) as f64 / 1000.0);
        if !(fits_minimum && fits_sitting && fits_window) {
            return Err(CoreError::Unavailable);
        }
    }
    let id = new_id();
    let now = stored(now_ms);
    transaction.execute(
        "INSERT INTO focus_sessions (id, startedAt, endedAt, phase, activeTaskId, workDurationSeconds,
                                     breakDurationSeconds, breakEndsAt, activeTaskStartedAt, activeBlockId,
                                     accumulatedSeconds, pausedAt, checkpointAt)
         VALUES (?1, ?2, NULL, 'running', ?3, ?4, ?5, NULL, ?2, ?6, NULL, NULL, ?2)",
        params![
            id,
            now,
            task_id,
            planned_seconds.unwrap_or(work_seconds).max(60),
            break_seconds.max(60),
            new_id()
        ],
    )?;
    transaction.execute(
        "INSERT INTO focus_queue_items (id, sessionId, taskId, sortOrder, state, plannedSeconds,
                                        completedAt, skippedAt, createdAt)
         VALUES (?1, ?2, ?3, 0, 'queued', ?4, NULL, NULL, ?5)",
        params![new_id(), id, task_id, planned_seconds, now],
    )?;
    Ok(id)
}

/// Queues a task in a session; queuing one already waiting does nothing.
/// `WorkspaceStore.addToFocusQueue`.
pub fn add_to_queue(
    transaction: &Transaction,
    session_id: &str,
    task_id: &str,
    planned_seconds: Option<i64>,
    now_ms: i64,
) -> Result<(), CoreError> {
    let session_exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM focus_sessions WHERE id = ?1)",
        [session_id],
        |row| row.get(0),
    )?;
    if !session_exists {
        return Err(CoreError::MissingTask {
            id: task_id.to_string(),
        });
    }
    require_actionable(transaction, task_id)?;
    let queued: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM focus_queue_items WHERE sessionId = ?1 AND taskId = ?2 AND state = 'queued')",
        params![session_id, task_id],
        |row| row.get(0),
    )?;
    if queued {
        return Ok(());
    }
    let count: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM focus_queue_items WHERE sessionId = ?1",
        [session_id],
        |row| row.get(0),
    )?;
    transaction.execute(
        "INSERT INTO focus_queue_items (id, sessionId, taskId, sortOrder, state, plannedSeconds,
                                        completedAt, skippedAt, createdAt)
         VALUES (?1, ?2, ?3, ?4, 'queued', ?5, NULL, NULL, ?6)",
        params![
            new_id(),
            session_id,
            task_id,
            count,
            planned_seconds,
            stored(now_ms)
        ],
    )?;
    Ok(())
}

/// What finishing a block did. `WorkspaceStore.FocusCompletion`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BlockFinished {
    pub session_id: String,
    /// "taskCompleted", "progressLogged" or "contributionLogged".
    pub outcome: String,
    pub seconds: i64,
    /// The award's id when the block was scored.
    pub award_id: Option<String>,
}

/// Finishes the block in hand, crediting `elapsed_seconds`. A task with a
/// daily due today stays open and the time lands on today's contribution;
/// otherwise the task is completed (or only credited when `complete_task` is
/// false). A `quality_multiplier` scores the block. Then the next available
/// queued task takes over, or the session finishes, or it waits paused on a
/// blocked queue. A block already recorded, or not the one expected, is not
/// recorded twice. `WorkspaceStore.completeActiveFocusTask`.
#[allow(clippy::too_many_arguments)]
pub fn finish_block(
    transaction: &Transaction,
    session_id: &str,
    elapsed_seconds: i64,
    quality_multiplier: Option<f64>,
    complete_task: bool,
    expected_block_id: Option<&str>,
    context: &FocusContext,
    now_ms: i64,
    zone: &str,
) -> Result<BlockFinished, CoreError> {
    finish(
        transaction,
        session_id,
        elapsed_seconds,
        quality_multiplier,
        complete_task,
        expected_block_id,
        Then::HandOff(context),
        now_ms,
        zone,
    )
}

/// Logs the block in hand as progress, exactly as [`finish_block`] does
/// without completing the task, then rests: the task stays in hand with a
/// fresh block whose clock is stopped at nothing, and the session is on a
/// break until the session's break length from now. [`end_break`] goes back
/// to it. `WorkspaceStore.takeFocusBreak`.
pub fn take_break(
    transaction: &Transaction,
    session_id: &str,
    elapsed_seconds: i64,
    quality_multiplier: Option<f64>,
    expected_block_id: Option<&str>,
    now_ms: i64,
    zone: &str,
) -> Result<BlockFinished, CoreError> {
    finish(
        transaction,
        session_id,
        elapsed_seconds,
        quality_multiplier,
        false,
        expected_block_id,
        Then::Break,
        now_ms,
        zone,
    )
}

/// Back from a break: the task that was in hand starts a fresh block, or,
/// when it was finished or deleted meanwhile, the queue moves on as after
/// any block. A session not on a break is left alone.
/// `WorkspaceStore.endFocusBreak`.
pub fn end_break(
    transaction: &Transaction,
    session_id: &str,
    context: &FocusContext,
    now_ms: i64,
    zone: &str,
) -> Result<(), CoreError> {
    let row: Option<(String, Option<String>)> = transaction
        .query_row(
            "SELECT phase, activeTaskId FROM focus_sessions WHERE id = ?1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((phase, active_id)) = row else {
        return Ok(());
    };
    if phase != "onBreak" {
        return Ok(());
    }
    let now = stored(now_ms);
    let still_open = match &active_id {
        Some(id) => transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id = ?1 AND status = 'open')",
            [id],
            |row| row.get(0),
        )?,
        None => false,
    };
    transaction.execute(
        "UPDATE focus_sessions SET phase = 'running', breakEndsAt = NULL, accumulatedSeconds = 0,
                                   pausedAt = NULL, activeTaskStartedAt = ?1, checkpointAt = ?1
         WHERE id = ?2",
        params![now, session_id],
    )?;
    if !still_open {
        if let Some(id) = active_id {
            transaction.execute(
                "UPDATE focus_queue_items SET state = 'completed', completedAt = ?1
                 WHERE id = (SELECT id FROM focus_queue_items WHERE sessionId = ?2 AND taskId = ?3
                             AND state = 'queued' ORDER BY rowid LIMIT 1)",
                params![now, session_id, id],
            )?;
        }
        hand_off(transaction, session_id, context, now_ms, zone, false)?;
    }
    Ok(())
}

/// What follows a recorded block.
enum Then<'a> {
    /// The queue moves on in this context.
    HandOff(&'a FocusContext),
    /// The task stays in hand and the session rests.
    Break,
}

#[allow(clippy::too_many_arguments)]
fn finish(
    transaction: &Transaction,
    session_id: &str,
    elapsed_seconds: i64,
    quality_multiplier: Option<f64>,
    complete_task: bool,
    expected_block_id: Option<&str>,
    then: Then,
    now_ms: i64,
    zone: &str,
) -> Result<BlockFinished, CoreError> {
    let session: Option<(Option<String>, Option<String>)> = transaction
        .query_row(
            "SELECT activeTaskId, activeBlockId FROM focus_sessions WHERE id = ?1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((active_id, active_block)) = session else {
        return Err(CoreError::NoActiveFocusTask);
    };
    let already = |id: &str| -> Result<bool, CoreError> {
        Ok(transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM focus_work_blocks WHERE id = ?1)",
            [id],
            |row| row.get(0),
        )?)
    };
    let nothing = BlockFinished {
        session_id: session_id.to_string(),
        outcome: "progressLogged".into(),
        seconds: 0,
        award_id: None,
    };
    if let Some(expected) = expected_block_id
        && already(expected)?
    {
        return Ok(nothing);
    }
    let Some(active_id) = active_id else {
        return Err(CoreError::NoActiveFocusTask);
    };
    let block_id = expected_block_id
        .map(str::to_string)
        .or(active_block.clone())
        .unwrap_or_else(|| format!("legacy-{session_id}/{active_id}"));
    if already(&block_id)? {
        return Ok(nothing);
    }
    if expected_block_id.is_some() && expected_block_id != active_block.as_deref() {
        return Err(CoreError::NoActiveFocusTask);
    }
    let elapsed = elapsed_seconds.max(0);
    let now = stored(now_ms);
    let tz = periodic::zone(zone);
    let task: Option<(String, Option<String>)> = transaction
        .query_row(
            "SELECT title, completedAt FROM tasks WHERE id = ?1",
            [&active_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let daily: Option<(String, Option<i64>)> = transaction
        .query_row(
            "SELECT id, targetSeconds FROM dailies WHERE taskId = ?1 AND archivedAt IS NULL",
            [&active_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
        )
        .optional()?;
    let due_daily = match daily {
        Some((daily_id, target)) if daily_due(transaction, &daily_id, now_ms, tz)? => {
            Some((daily_id, target))
        }
        _ => None,
    };
    let mut outcome = if complete_task {
        "taskCompleted"
    } else {
        "progressLogged"
    };
    if let Some((daily_id, target)) = due_daily {
        let key = dailies::day_key(now_ms, zone);
        let logged: i64 = transaction
            .query_row(
                "SELECT secondsLogged FROM daily_contributions WHERE dailyId = ?1 AND dayKey = ?2",
                params![daily_id, key],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0);
        let met =
            complete_task || target.is_some_and(|target| target > 0 && logged + elapsed >= target);
        dailies::log_contribution(transaction, &daily_id, elapsed, met, now_ms, zone)?;
        outcome = "contributionLogged";
    } else if complete_task && task.is_some() {
        // Completing here is the status change, with its next occurrence and
        // habit expiry, exactly as completing from a list does.
        crate::tasks::set_status(transaction, &active_id, "completed", now_ms, zone)?;
    }
    let title = task.as_ref().map(|(title, _)| title.clone());
    transaction.execute(
        "INSERT INTO focus_work_blocks (id, sessionId, taskId, taskTitle, seconds, recordedAt, originalTaskId)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?3)",
        params![
            block_id,
            session_id,
            task.as_ref().map(|_| &active_id),
            title.clone().unwrap_or_else(|| "Deleted task".into()),
            elapsed,
            now
        ],
    )?;
    let mut award_id = None;
    if let Some(multiplier) = quality_multiplier
        && elapsed > 0
    {
        let minutes = crate::progress::focus_minutes(elapsed);
        let points = crate::progress::focus_score(elapsed, multiplier);
        let multiplier = crate::progress::clamped_focus_multiplier(multiplier);
        transaction.execute(
            "INSERT INTO focus_awards (id, sessionId, taskId, taskTitle, seconds, minutes, multiplier, points, awardedAt)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                block_id,
                session_id,
                task.as_ref().map(|_| &active_id),
                title.clone().unwrap_or_else(|| "Untitled task".into()),
                elapsed,
                minutes,
                multiplier,
                points,
                now
            ],
        )?;
        award_id = Some(block_id.clone());
    }
    match then {
        Then::HandOff(context) => {
            transaction.execute(
                "UPDATE focus_queue_items SET state = 'completed', completedAt = ?1
                 WHERE id = (SELECT id FROM focus_queue_items WHERE sessionId = ?2 AND taskId = ?3 AND state = 'queued'
                             ORDER BY rowid LIMIT 1)",
                params![now, session_id, active_id],
            )?;
            hand_off(transaction, session_id, context, now_ms, zone, false)?;
        }
        Then::Break => {
            let break_seconds: i64 = transaction.query_row(
                "SELECT breakDurationSeconds FROM focus_sessions WHERE id = ?1",
                [session_id],
                |row| row.get(0),
            )?;
            transaction.execute(
                "UPDATE focus_sessions SET phase = 'onBreak', breakEndsAt = ?1, accumulatedSeconds = 0,
                                           pausedAt = ?2, activeTaskStartedAt = ?2, checkpointAt = ?2,
                                           activeBlockId = ?3
                 WHERE id = ?4",
                params![
                    stored(now_ms + break_seconds.max(60) * 1000),
                    now,
                    new_id(),
                    session_id
                ],
            )?;
        }
    }
    Ok(BlockFinished {
        session_id: session_id.to_string(),
        outcome: outcome.into(),
        seconds: if outcome == "taskCompleted" {
            0
        } else {
            elapsed
        },
        award_id,
    })
}

/// After a block: the first queued task available in `context` takes over
/// with a fresh block, or the session finishes when nothing is queued, or it
/// waits paused on a blocked queue. With `resuming`, a session that is not
/// waiting with nothing in hand is left alone, and so is one with nothing
/// available. Returns whether it changed the session.
fn hand_off(
    transaction: &Transaction,
    session_id: &str,
    context: &FocusContext,
    now_ms: i64,
    zone: &str,
    resuming: bool,
) -> Result<bool, CoreError> {
    let all = candidates(transaction, now_ms, zone)?;
    let by_id: HashMap<&str, &Candidate> = all.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut statement = transaction.prepare(
        "SELECT taskId, plannedSeconds FROM focus_queue_items WHERE sessionId = ?1 AND state = 'queued' ORDER BY sortOrder",
    )?;
    let pending: Vec<(String, Option<i64>)> = statement
        .query_map([session_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let next = pending.iter().find_map(|(task_id, planned)| {
        by_id
            .get(task_id.as_str())
            .filter(|candidate| reasons(candidate, context, now_ms).is_empty())
            .map(|candidate| (*candidate, *planned))
    });
    let now = stored(now_ms);
    match next {
        Some((candidate, planned)) => {
            transaction.execute(
                "UPDATE focus_sessions SET activeTaskId = ?1, activeTaskStartedAt = ?2, accumulatedSeconds = 0,
                                           pausedAt = NULL, checkpointAt = ?2, activeBlockId = ?3,
                                           workDurationSeconds = ?4
                 WHERE id = ?5",
                params![
                    candidate.id,
                    now,
                    new_id(),
                    planned_seconds(candidate, planned, context, now_ms),
                    session_id
                ],
            )?;
            Ok(true)
        }
        None if resuming => Ok(false),
        None if pending.is_empty() => {
            transaction.execute(
                "UPDATE focus_sessions SET activeTaskId = NULL, activeTaskStartedAt = ?1, accumulatedSeconds = 0,
                                           pausedAt = NULL, checkpointAt = ?1, activeBlockId = NULL,
                                           phase = 'finished', endedAt = ?1
                 WHERE id = ?2",
                params![now, session_id],
            )?;
            Ok(true)
        }
        None => {
            // Keep the blocked queue; never invent a running task.
            transaction.execute(
                "UPDATE focus_sessions SET activeTaskId = NULL, activeTaskStartedAt = ?1, accumulatedSeconds = 0,
                                           pausedAt = ?1, checkpointAt = ?1, activeBlockId = NULL
                 WHERE id = ?2",
                params![now, session_id],
            )?;
            Ok(true)
        }
    }
}

pub(crate) fn daily_due(
    connection: &Connection,
    daily_id: &str,
    now_ms: i64,
    zone: chrono_tz::Tz,
) -> Result<bool, CoreError> {
    let row: Option<(Option<i64>, Option<String>, String, i64)> = connection
        .query_row(
            "SELECT intervalDays, intervalAnchor, createdAt, activeWeekdaysMask FROM dailies WHERE id = ?1",
            [daily_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((interval, anchor, created, mask)) = row else {
        return Ok(false);
    };
    let probe = Row {
        id: String::new(),
        title: String::new(),
        list_id: String::new(),
        due_at: None,
        estimate: None,
        sort_order: 0,
        created_at: String::new(),
        kind: String::new(),
        start_at: None,
        urgency: None,
        importance: None,
        priority: None,
        column: None,
        rank: None,
        planning: None,
        daily_id: Some(daily_id.to_string()),
        daily_target: None,
        daily_placement: None,
        daily_interval: interval,
        daily_anchor: anchor,
        daily_created: Some(created),
        daily_mask: Some(mask),
        logged_today: None,
        done_today: None,
    };
    Ok(daily_is_due(&probe, now_ms, zone))
}

/// Ends a session. `WorkspaceStore.finishFocusSession`.
pub fn finish_session(transaction: &Transaction, id: &str, now_ms: i64) -> Result<(), CoreError> {
    transaction.execute(
        "UPDATE focus_sessions SET phase = 'finished', endedAt = ?1, breakEndsAt = NULL WHERE id = ?2",
        params![stored(now_ms), id],
    )?;
    Ok(())
}

/// What the clock has run to: the seconds banked plus, while running, the
/// time since the block last started. `FocusSession.elapsedSeconds(now:)`.
fn elapsed(transaction: &Transaction, id: &str, now_ms: i64) -> Result<Option<i64>, CoreError> {
    let row: Option<(String, Option<i64>, Option<String>, String)> = transaction
        .query_row(
            "SELECT phase, accumulatedSeconds, pausedAt, activeTaskStartedAt FROM focus_sessions WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((phase, accumulated, paused, started)) = row else {
        return Ok(None);
    };
    if phase != "running" || paused.is_some() {
        return Ok(None);
    }
    let started = millis(Some(&started)).unwrap_or(now_ms);
    Ok(Some(
        accumulated.unwrap_or(0).max(0) + ((now_ms - started) / 1000).max(0),
    ))
}

/// Pauses a running block, banking its time. `WorkspaceStore.pauseFocusSession`.
pub fn pause(transaction: &Transaction, id: &str, now_ms: i64) -> Result<(), CoreError> {
    if let Some(elapsed) = elapsed(transaction, id, now_ms)? {
        transaction.execute(
            "UPDATE focus_sessions SET accumulatedSeconds = ?1, pausedAt = ?2, checkpointAt = ?2 WHERE id = ?3",
            params![elapsed, stored(now_ms), id],
        )?;
    }
    Ok(())
}

/// Resumes a paused block. `WorkspaceStore.resumeFocusSession`.
pub fn resume(transaction: &Transaction, id: &str, now_ms: i64) -> Result<(), CoreError> {
    transaction.execute(
        "UPDATE focus_sessions SET pausedAt = NULL, activeTaskStartedAt = ?1, checkpointAt = ?1
         WHERE id = ?2 AND phase = 'running' AND pausedAt IS NOT NULL",
        params![stored(now_ms), id],
    )?;
    Ok(())
}

/// Banks a running block's time without pausing it, so a crash loses at most
/// the time since. `WorkspaceStore.checkpointFocusSession`.
pub fn checkpoint(transaction: &Transaction, id: &str, now_ms: i64) -> Result<(), CoreError> {
    if let Some(elapsed) = elapsed(transaction, id, now_ms)? {
        transaction.execute(
            "UPDATE focus_sessions SET accumulatedSeconds = ?1, activeTaskStartedAt = ?2, checkpointAt = ?2 WHERE id = ?3",
            params![elapsed, stored(now_ms), id],
        )?;
    }
    Ok(())
}

/// Sets a running block's clock to `elapsed_seconds` from now, after the
/// system clock jumped. `WorkspaceStore.rebaseFocusClock`.
pub fn rebase(
    transaction: &Transaction,
    id: &str,
    elapsed_seconds: i64,
    now_ms: i64,
) -> Result<(), CoreError> {
    transaction.execute(
        "UPDATE focus_sessions SET accumulatedSeconds = ?1, activeTaskStartedAt = ?2, checkpointAt = ?2
         WHERE id = ?3 AND phase = 'running' AND pausedAt IS NULL",
        params![elapsed_seconds.max(0), stored(now_ms), id],
    )?;
    Ok(())
}

/// On reopening, keeps only checkpointed time: every running session is
/// paused at its last checkpoint. `WorkspaceStore.recoverInterruptedFocus`.
pub fn recover_interrupted(transaction: &Transaction) -> Result<(), CoreError> {
    transaction.execute(
        "UPDATE focus_sessions SET pausedAt = COALESCE(checkpointAt, activeTaskStartedAt),
                                   accumulatedSeconds = COALESCE(accumulatedSeconds, 0)
         WHERE phase = 'running' AND pausedAt IS NULL",
        [],
    )?;
    Ok(())
}

/// Whether a running session waiting with nothing in hand has a queued task
/// available in `context`. A read, so the app can ask before writing.
/// `WorkspaceStore.hasResumableFocusQueueTask`.
pub fn has_resumable(
    connection: &Connection,
    context: &FocusContext,
    now_ms: i64,
    zone: &str,
) -> Result<bool, CoreError> {
    let Some(session) = waiting_session(connection)? else {
        return Ok(false);
    };
    let all = candidates(connection, now_ms, zone)?;
    let mut statement = connection.prepare(
        "SELECT taskId FROM focus_queue_items WHERE sessionId = ?1 AND state = 'queued'",
    )?;
    let queued: Vec<String> = statement
        .query_map([&session], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(queued.iter().any(|task_id| {
        all.iter()
            .find(|candidate| &candidate.id == task_id)
            .is_some_and(|candidate| reasons(candidate, context, now_ms).is_empty())
    }))
}

/// Resumes a session whose queue was blocked at the last handoff, if a
/// queued task is now available. Returns whether it resumed one.
/// `WorkspaceStore.resumeEligibleFocusQueue`.
pub fn resume_eligible_queue(
    transaction: &Transaction,
    context: &FocusContext,
    now_ms: i64,
    zone: &str,
) -> Result<bool, CoreError> {
    let Some(session) = waiting_session(transaction)? else {
        return Ok(false);
    };
    hand_off(transaction, &session, context, now_ms, zone, true)
}

fn waiting_session(connection: &Connection) -> Result<Option<String>, CoreError> {
    Ok(connection
        .query_row(
            "SELECT id FROM focus_sessions WHERE phase = 'running' AND activeTaskId IS NULL LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?)
}

#[cfg(test)]
mod tests;
