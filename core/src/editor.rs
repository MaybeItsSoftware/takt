//! The task editor's writes: saving the editor, editing a task, its details,
//! its schedule, and copying its planning to its subtasks.
//!
//! A task's editable state is one [`EditorSnapshot`]. Saving compares the
//! snapshot the editor opened with the one in the database, so a change made
//! meanwhile (by the CLI, a phone, another window) is refused rather than
//! overwritten. Replaces `WorkspaceStore+Editing.swift`, the planning half of
//! `WorkspaceStore+Conditions.swift`, and Kotlin's `Planning.kt`.

use std::collections::BTreeSet;

use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::CoreError;
use crate::board_columns::{self, THIS_WEEK_COLUMN_ID, TODAY_COLUMN_ID};
use crate::dailies;
use crate::habits;
use crate::periodic;
pub use crate::planning::Planning;
use crate::tasks::normalized_strings;
use crate::time::{decode_strings, non_empty_name, parse_stored, stored, swift_json_strings};

/// The parts of a task's metadata the editor shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct EditorMetadata {
    /// 1 to 4, or -4 to -1; see [`crate::tasks::is_priority`].
    pub priority: Option<i64>,
    pub tags: Vec<String>,
    pub recurrence_rule: Option<String>,
    pub external_links: Vec<String>,
}

/// Everything the task editor edits, as it stands. `TaskEditorSnapshot`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EditorSnapshot {
    pub workspace_id: String,
    pub task_id: String,
    pub title: String,
    pub notes: String,
    pub due_at_ms: Option<i64>,
    pub estimate_seconds: Option<i64>,
    pub metadata: EditorMetadata,
    /// Whether the task is tracked as an active daily.
    pub daily_progress: bool,
    pub planning: Option<Planning>,
}

/// A task's workspace, title, notes, stored due time and estimate.
type TaskFields = (String, String, String, Option<String>, Option<i64>);

/// Reads a task's editable state. `WorkspaceStore.taskEditorSnapshot`.
pub fn snapshot(
    transaction: &rusqlite::Connection,
    task_id: &str,
) -> Result<EditorSnapshot, CoreError> {
    let task: Option<TaskFields> = transaction
        .query_row(
            "SELECT l.workspaceId, t.title, t.notes, t.dueAt, t.estimateSeconds
             FROM tasks t JOIN task_lists l ON l.id = t.listId WHERE t.id = ?1",
            [task_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?;
    let Some((workspace_id, title, notes, due_at, estimate_seconds)) = task else {
        return Err(CoreError::MissingTask {
            id: task_id.to_string(),
        });
    };
    let metadata = stored_metadata(transaction, task_id)?;
    let daily_progress: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM dailies WHERE taskId = ?1 AND archivedAt IS NULL)",
        [task_id],
        |row| row.get(0),
    )?;
    Ok(EditorSnapshot {
        workspace_id,
        task_id: task_id.to_string(),
        title,
        notes,
        due_at_ms: due_at
            .as_deref()
            .and_then(parse_stored)
            .map(|at| at.timestamp_millis()),
        estimate_seconds,
        metadata: metadata
            .as_ref()
            .map(|m| m.editor.clone())
            .unwrap_or_default(),
        daily_progress,
        planning: match &metadata {
            Some(stored) => stored.planning()?,
            None => None,
        },
    })
}

struct StoredMetadata {
    editor: EditorMetadata,
    start_at: Option<String>,
    planning_json: Option<String>,
}

impl StoredMetadata {
    /// `WorkspaceStore.planning(_:)`: the JSON, with the start from its own
    /// column, normalised.
    fn planning(&self) -> Result<Option<Planning>, CoreError> {
        let mut planning = match &self.planning_json {
            Some(json) => Planning::from_json(json)?,
            None => Planning::default(),
        };
        planning.start_at_ms = self
            .start_at
            .as_deref()
            .and_then(parse_stored)
            .map(|at| at.timestamp_millis());
        Ok(planning.normalized())
    }

    /// Whether the deadline was changed after the task was made, which stops
    /// it following the start ([`follow_start`]).
    fn due_pinned(&self) -> bool {
        self.planning_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
            .and_then(|value| value.get(DUE_PINNED_KEY)?.as_bool())
            .unwrap_or(false)
    }
}

/// The key in `planningJSON` that pins a deadline against its start. Kept
/// out of [`Planning`], which every client round-trips, so that a client
/// saving its planning cannot drop it; the decoders ignore it.
const DUE_PINNED_KEY: &str = "duePinned";

fn stored_metadata(
    connection: &rusqlite::Connection,
    task_id: &str,
) -> Result<Option<StoredMetadata>, CoreError> {
    Ok(connection
        .query_row(
            "SELECT priority, tagsJSON, recurrenceRule, externalLinksJSON, startAt, planningJSON
             FROM task_metadata WHERE taskId = ?1",
            [task_id],
            |row| {
                Ok(StoredMetadata {
                    editor: EditorMetadata {
                        priority: row.get(0)?,
                        tags: decode_strings(&row.get::<_, String>(1)?),
                        recurrence_rule: row.get(2)?,
                        external_links: decode_strings(&row.get::<_, String>(3)?),
                    },
                    start_at: row.get(4)?,
                    planning_json: row.get(5)?,
                })
            },
        )
        .optional()?)
}

/// Saves the editor as one step, refusing if the task has changed since
/// `baseline` was read. Returns the task as saved.
/// `WorkspaceStore.saveTaskEditor`.
pub fn save_editor(
    transaction: &Transaction,
    edit: &EditorSnapshot,
    baseline: &EditorSnapshot,
    now_ms: i64,
    zone: &str,
) -> Result<EditorSnapshot, CoreError> {
    save_editor_with(transaction, edit, baseline, now_ms, zone, false)
}

/// [`save_editor`], saying what a moved start does to the deadline: with
/// `keep_due` the deadline stays where it is, and is pinned there, rather
/// than following the start. What the Mac's "Keep the due date" saves.
pub fn save_editor_with(
    transaction: &Transaction,
    edit: &EditorSnapshot,
    baseline: &EditorSnapshot,
    now_ms: i64,
    zone: &str,
    keep_due: bool,
) -> Result<EditorSnapshot, CoreError> {
    let current = snapshot(transaction, &edit.task_id)?;
    if &current != baseline {
        return Err(CoreError::EditorConflict);
    }
    let edit = &mut edit.clone();
    update_planning(
        transaction,
        edit,
        current.planning.as_ref(),
        current.due_at_ms,
        now_ms,
        zone,
        keep_due,
    )?;
    update_task_record(transaction, edit, now_ms)?;
    update_editor_metadata(transaction, &edit.task_id, &edit.metadata, now_ms)?;
    set_daily_attachment(
        transaction,
        &edit.task_id,
        edit.daily_progress,
        edit.estimate_seconds,
        now_ms,
    )?;
    snapshot(transaction, &edit.task_id)
}

/// Sets a task's title, notes, due time and estimate. A due time replaces a
/// due date in its planning. `WorkspaceStore.updateTask`.
#[allow(clippy::too_many_arguments)]
pub fn update_task(
    transaction: &Transaction,
    id: &str,
    title: &str,
    notes: &str,
    due_at_ms: Option<i64>,
    estimate_seconds: Option<i64>,
    now_ms: i64,
    zone: &str,
) -> Result<(), CoreError> {
    let title = non_empty_name(title)?;
    let previous = snapshot(transaction, id)?;
    let mut edit = previous.clone();
    edit.title = title;
    edit.notes = notes.to_string();
    edit.due_at_ms = due_at_ms;
    edit.estimate_seconds = estimate_seconds;
    if due_at_ms.is_some() {
        edit.planning = edit.planning.and_then(|mut planning| {
            planning.due_date = None;
            planning.normalized()
        });
    }
    update_planning(
        transaction,
        &mut edit,
        previous.planning.as_ref(),
        previous.due_at_ms,
        now_ms,
        zone,
        false,
    )?;
    update_task_record(transaction, &edit, now_ms)
}

/// Sets a task's priority, tags, links and repeat rule.
/// `WorkspaceStore.updateTaskEditorMetadata`.
///
/// A priority set or changed puts the task first among its siblings — row
/// `:1` of its list — and a negative one last. Clearing one leaves it where
/// it is.
///
/// Sending a task behind the back supersedes those already there: it takes
/// the very back, and the open tasks at its level or behind it each move one
/// step forward (-1 to -2, -2 to -3, -3 to -4), so the last one sent is the
/// last of the day. -4 is as far forward as they go.
pub fn update_editor_metadata(
    transaction: &Transaction,
    task_id: &str,
    metadata: &EditorMetadata,
    now_ms: i64,
) -> Result<(), CoreError> {
    let before = stored_metadata(transaction, task_id)?.and_then(|stored| stored.editor.priority);
    write_editor_metadata(transaction, task_id, metadata, now_ms)?;
    let after = metadata
        .priority
        .filter(|priority| crate::tasks::is_priority(*priority));
    if after != before {
        match after {
            Some(p) if p > 0 => crate::tasks::move_task_to_start(transaction, task_id, now_ms)?,
            Some(p) => {
                supersede_behind_the_back(transaction, task_id, p, now_ms)?;
                crate::tasks::move_task_to_end(transaction, task_id, now_ms)?
            }
            None => {}
        }
    }
    Ok(())
}

/// Moves the other open tasks at `priority` or further behind one step
/// forward, as [`update_editor_metadata`] describes. Its own series is left
/// alone: the priority is the series', shared with it.
fn supersede_behind_the_back(
    transaction: &Transaction,
    task_id: &str,
    priority: i64,
    now_ms: i64,
) -> Result<(), CoreError> {
    if !(-3..=-1).contains(&priority) {
        return Ok(());
    }
    transaction.execute(
        "UPDATE task_metadata SET priority = priority - 1, updatedAt = ?3
         WHERE priority BETWEEN -3 AND ?2
           AND COALESCE(seriesTaskId, taskId) IS NOT
               (SELECT COALESCE(seriesTaskId, taskId) FROM task_metadata WHERE taskId = ?1)
           AND taskId IN (SELECT id FROM tasks WHERE status = 'open' AND archivedAt IS NULL)",
        params![task_id, priority, stored(now_ms)],
    )?;
    Ok(())
}

/// [`update_editor_metadata`] without the move: a board's rule column sets a
/// priority as part of a drop, and the drop has already said where it goes.
pub(crate) fn write_editor_metadata(
    transaction: &Transaction,
    task_id: &str,
    metadata: &EditorMetadata,
    now_ms: i64,
) -> Result<(), CoreError> {
    let exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE id = ?1)",
        [task_id],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(CoreError::MissingTask {
            id: task_id.to_string(),
        });
    }
    let tags = normalized_strings(&metadata.tags);
    let links = normalized_strings(&metadata.external_links);
    let recurrence = metadata
        .recurrence_rule
        .as_deref()
        .map(str::trim)
        .filter(|rule| !rule.is_empty())
        .map(str::to_string);
    let priority = metadata
        .priority
        .filter(|priority| crate::tasks::is_priority(*priority));
    // A task with no metadata row compares as one with nothing set, so
    // saving nothing onto it writes nothing, as Swift's did.
    let current = stored_metadata(transaction, task_id)?
        .map(|stored| stored.editor)
        .unwrap_or_default();
    if current.priority == priority
        && current.tags == tags
        && current.recurrence_rule == recurrence
        && current.external_links == links
    {
        return Ok(());
    }
    transaction.execute(
        "INSERT INTO task_metadata (taskId, priority, tagsJSON, recurrenceRule, externalLinksJSON, updatedAt)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(taskId) DO UPDATE SET
           priority = excluded.priority, tagsJSON = excluded.tagsJSON,
           recurrenceRule = excluded.recurrenceRule, externalLinksJSON = excluded.externalLinksJSON,
           updatedAt = excluded.updatedAt",
        params![
            task_id,
            priority,
            swift_json_strings(&tags),
            recurrence,
            swift_json_strings(&links),
            stored(now_ms)
        ],
    )?;
    if current.priority != priority {
        share_priority_with_series(transaction, task_id, priority, now_ms)?;
    }
    Ok(())
}

/// Writes a repeating task's new priority to every other open occurrence of
/// its series: the priority belongs to the series, and the one set last wins.
/// A series is the tasks whose `seriesTaskId` — or, for its first occurrence,
/// whose own id — is the same: the next turn and a "keep both" copy are given
/// the series of the task they are made from (`tasks::schedule_next_occurrence`,
/// `tasks::add_occurrence_copy`), and the next turn carries the priority of
/// the one finished, so it inherits this too. A task that has never repeated
/// is a series of one, and this writes nothing.
fn share_priority_with_series(
    transaction: &Transaction,
    task_id: &str,
    priority: Option<i64>,
    now_ms: i64,
) -> Result<(), CoreError> {
    transaction.execute(
        "UPDATE task_metadata SET priority = ?2, updatedAt = ?3
         WHERE taskId <> ?1
           AND priority IS NOT ?2
           AND COALESCE(seriesTaskId, taskId) =
               (SELECT COALESCE(seriesTaskId, taskId) FROM task_metadata WHERE taskId = ?1)
           AND taskId IN (SELECT id FROM tasks WHERE status = 'open' AND archivedAt IS NULL)",
        params![task_id, priority, stored(now_ms)],
    )?;
    Ok(())
}

/// Sets a task's due day (`yyyy-MM-dd`), or clears its due day and time:
/// what dropping it into a board column whose rules ask for one does. A due
/// day replaces a due time, as the editor's does.
pub fn set_due_day(
    transaction: &Transaction,
    id: &str,
    day: Option<&str>,
    now_ms: i64,
    zone: &str,
) -> Result<(), CoreError> {
    let previous = snapshot(transaction, id)?;
    let mut edit = previous.clone();
    edit.due_at_ms = None;
    let mut planning = edit.planning.clone().unwrap_or_default();
    planning.due_date = day.map(str::to_string);
    edit.planning = planning.normalized();
    update_planning(
        transaction,
        &mut edit,
        previous.planning.as_ref(),
        previous.due_at_ms,
        now_ms,
        zone,
        false,
    )?;
    update_task_record(transaction, &edit, now_ms)
}

/// Moves a task's start, or clears it. `WorkspaceStore.scheduleTask`.
pub fn schedule_task(
    transaction: &Transaction,
    id: &str,
    start_at_ms: Option<i64>,
    now_ms: i64,
    zone: &str,
) -> Result<(), CoreError> {
    let previous = snapshot(transaction, id)?;
    let mut edit = previous.clone();
    let mut planning = edit.planning.clone().unwrap_or_default();
    planning.start_at_ms = start_at_ms;
    edit.planning = planning.normalized();
    update_planning(
        transaction,
        &mut edit,
        previous.planning.as_ref(),
        previous.due_at_ms,
        now_ms,
        zone,
        false,
    )?;
    // A deadline that followed the start is the task row's to write.
    update_task_record(transaction, &edit, now_ms)
}

/// Gives a task just created its start, checked and filed as a save of it
/// would be: refused on or after its deadline, and moved between Today and
/// This week by the day it falls on. The deadline it was made with is not
/// pinned, so it follows the start, as one set when a task is added does.
pub(crate) fn plan_new_task(
    transaction: &Transaction,
    id: &str,
    start_at_ms: i64,
    now_ms: i64,
    zone: &str,
) -> Result<(), CoreError> {
    let mut edit = snapshot(transaction, id)?;
    let mut planning = edit.planning.clone().unwrap_or_default();
    planning.start_at_ms = Some(start_at_ms);
    edit.planning = planning.normalized();
    update_planning(transaction, &mut edit, None, None, now_ms, zone, false)
}

/// Copies a task's requirements, start and block rules (not its due date,
/// which stays each subtask's own) onto every task below it.
/// `WorkspaceStore.applyPlanningToDescendants`.
pub fn apply_planning_to_descendants(
    transaction: &Transaction,
    task_id: &str,
    now_ms: i64,
    zone: &str,
) -> Result<(), CoreError> {
    let parent = snapshot(transaction, task_id)?;
    let mut descendants = crate::tasks::descendant_ids(transaction, task_id)?;
    descendants.sort();
    for id in descendants {
        let mut saved = snapshot(transaction, &id)?;
        let own_due_date = saved.planning.as_ref().and_then(|p| p.due_date.clone());
        saved.planning = parent.planning.clone().map(|mut planning| {
            planning.due_date = own_due_date;
            planning
        });
        update_planning(transaction, &mut saved, None, None, now_ms, zone, false)?;
    }
    Ok(())
}

/// Checks and stores a task's planning. `WorkspaceStore.updatePlanning`.
///
/// A due date must be a real calendar date. A start must come before the
/// deadline (the end of the due date in `zone`, or the due time), checked
/// only when the schedule changed. A minimum block is at least a minute, a
/// one-sitting task needs an estimate at least that long, and every required
/// condition must exist in the task's workspace and not be archived unless it
/// was already required.
fn update_planning(
    transaction: &Transaction,
    edit: &mut EditorSnapshot,
    previous: Option<&Planning>,
    previous_due_at_ms: Option<i64>,
    now_ms: i64,
    zone: &str,
    keep_due: bool,
) -> Result<(), CoreError> {
    let zone = periodic::zone(zone);
    let current = stored_metadata(transaction, &edit.task_id)?;
    let was_pinned = current.as_ref().is_some_and(StoredMetadata::due_pinned);
    // Copying planning down to subtasks (no `previous`) moves no deadline.
    let pinned = match previous {
        Some(_) if keep_due => true,
        Some(_) => follow_start(edit, previous, previous_due_at_ms, was_pinned, zone),
        None => was_pinned,
    } && (edit.due_at_ms.is_some()
        || edit.planning.as_ref().is_some_and(|p| p.due_date.is_some()));
    let planning = edit.planning.clone().unwrap_or_default();
    let deadline_ms = match &planning.due_date {
        Some(date) => {
            let midnight = calendar_date(date).ok_or(CoreError::InvalidDate)?;
            Some(periodic::resolve(zone, midnight + Duration::days(1)).timestamp_millis())
        }
        None => edit.due_at_ms,
    };
    let changed_schedule = planning.start_at_ms != previous.and_then(|p| p.start_at_ms)
        || planning.due_date != previous.and_then(|p| p.due_date.clone())
        || edit.due_at_ms != previous_due_at_ms;
    if changed_schedule
        && let (Some(start), Some(deadline)) = (planning.start_at_ms, deadline_ms)
        && start >= deadline
    {
        return Err(CoreError::InvalidSchedule);
    }
    if planning
        .minimum_block_seconds
        .is_some_and(|minimum| minimum < 60)
    {
        return Err(CoreError::InvalidMinimum);
    }
    if planning.requires_single_sitting == Some(true) {
        let floor = planning.minimum_block_seconds.unwrap_or(60);
        if !edit
            .estimate_seconds
            .is_some_and(|estimate| estimate > 0 && estimate >= floor)
        {
            return Err(CoreError::EstimateRequired);
        }
    }
    let groups = planning.requirement_groups.clone().unwrap_or_default();
    let distinct: BTreeSet<Vec<String>> = groups
        .iter()
        .map(|group| {
            let mut sorted = group.clone();
            sorted.sort();
            sorted
        })
        .collect();
    if distinct.len() != groups.len() {
        return Err(CoreError::InvalidCondition);
    }
    let previously_required: Vec<&str> = previous
        .and_then(|p| p.requirement_groups.as_ref())
        .map(|groups| {
            groups
                .iter()
                .flatten()
                .map(|member| crate::conditions::condition_id_of(member))
                .collect()
        })
        .unwrap_or_default();
    for group in &groups {
        let unique: BTreeSet<&String> = group.iter().collect();
        if group.is_empty() || unique.len() != group.len() {
            return Err(CoreError::InvalidCondition);
        }
        for member in group {
            // A negated requirement ("!id") names its condition all the same.
            let id = crate::conditions::condition_id_of(member);
            let condition: Option<(String, bool)> = transaction
                .query_row(
                    "SELECT workspaceId, isArchived FROM task_conditions WHERE id = ?1",
                    [id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let usable = condition.is_some_and(|(workspace, archived)| {
                workspace == edit.workspace_id && (!archived || previously_required.contains(&id))
            });
            if !usable {
                return Err(CoreError::InvalidCondition);
            }
        }
    }
    let current_planning = match &current {
        Some(stored) => stored.planning()?,
        None => None,
    };
    if current_planning != edit.planning || was_pinned != pinned {
        let json = stored_planning_json(&planning, pinned);
        let start = planning.start_at_ms.map(stored);
        transaction.execute(
            "INSERT INTO task_metadata (taskId, tagsJSON, externalLinksJSON, startAt, planningJSON, updatedAt)
             VALUES (?1, '[]', '[]', ?2, ?3, ?4)
             ON CONFLICT(taskId) DO UPDATE SET
               startAt = excluded.startAt, planningJSON = excluded.planningJSON, updatedAt = excluded.updatedAt",
            params![edit.task_id, start, json, stored(now_ms)],
        )?;
    }
    if changed_schedule {
        refile_by_schedule(
            transaction,
            &edit.task_id,
            planning.start_at_ms,
            edit.due_at_ms,
            planning.due_date.as_deref(),
            now_ms,
            zone,
        )?;
    }
    Ok(())
}

/// The `planningJSON` a task's planning is stored as: everything but the
/// start, which has its own column, and the pin when the deadline has one.
fn stored_planning_json(planning: &Planning, pinned: bool) -> Option<String> {
    let mut kept = planning.clone();
    kept.start_at_ms = None;
    match (kept.normalized().map(|p| p.to_json()), pinned) {
        (json, false) => json,
        (Some(json), true) => Some(format!(
            "{},\"{DUE_PINNED_KEY}\":true}}",
            json.strip_suffix('}').unwrap_or(&json)
        )),
        (None, true) => Some(format!("{{\"{DUE_PINNED_KEY}\":true}}")),
    }
}

/// A stored `planningJSON` with its due day replaced by `day`, keeping
/// everything else it holds, the pin included: the next occurrence of a
/// repeating task carries its planning on to its own due day.
pub(crate) fn planning_json_with_due_day(json: &str, day: &str) -> Result<String, CoreError> {
    let pinned = serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|value| value.get(DUE_PINNED_KEY)?.as_bool())
        .unwrap_or(false);
    let mut planning = Planning::from_json(json)?;
    planning.due_date = Some(day.to_string());
    Ok(stored_planning_json(&planning, pinned).unwrap_or_else(|| json.to_string()))
}

/// A deadline moves with its start, by as much — a due time by the same
/// span, a due date by the same number of days — until the deadline is
/// changed after the task was made, which pins it; clearing it drops the
/// pin. A deadline set when the task was added, by the app, quick add or the
/// assistant, follows. Returns whether the deadline is pinned now.
fn follow_start(
    edit: &mut EditorSnapshot,
    previous: Option<&Planning>,
    previous_due_at_ms: Option<i64>,
    pinned: bool,
    zone: chrono_tz::Tz,
) -> bool {
    let previous_due_date = previous.and_then(|p| p.due_date.clone());
    let due_date = edit.planning.as_ref().and_then(|p| p.due_date.clone());
    let from = previous.and_then(|p| p.start_at_ms);
    let to = edit.planning.as_ref().and_then(|p| p.start_at_ms);
    // Where following the start puts the deadline, when the start moved.
    let followed = match (from, to) {
        (Some(from), Some(to)) if from != to => {
            let days = (habits::local_day(to, zone) - habits::local_day(from, zone)).num_days();
            Some((
                previous_due_at_ms.map(|at| at + (to - from)),
                previous_due_date
                    .as_deref()
                    .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
                    .map(|date| (date + Duration::days(days)).format("%Y-%m-%d").to_string()),
            ))
        }
        _ => None,
    };
    if edit.due_at_ms != previous_due_at_ms || due_date != previous_due_date {
        // Moved by hand to exactly where following would have put it — the
        // Mac's "move it with the start" — is still following: it is not pinned.
        return pinned || followed != Some((edit.due_at_ms, due_date));
    }
    if pinned {
        return true;
    }
    let Some((due_at_ms, due_date)) = followed else {
        return false;
    };
    edit.due_at_ms = due_at_ms;
    if let Some(planning) = edit.planning.as_mut() {
        planning.due_date = due_date;
    }
    false
}

/// Moves a task between Today and This week, or kicks it out, when its
/// start or deadline changes:
/// - A start on or before today is Today's, one later this week (weeks start on
///   Monday, as capture's `nextweek` does) This week's, and anything later
///   files it nowhere, back in the first column.
/// - With no start, a deadline on or before today is Today's, one later this
///   week This week's, and anything later files it nowhere.
/// - With neither start nor deadline, the task is filed nowhere.
///
/// Only a task in one of those two columns moves; one in progress, waiting or
/// filed anywhere else stays put, and so does one in the backlog, which a
/// schedule does not plan. Leaving Today drops the task's place in the day.
/// This week is used only if the task's list board draws it; otherwise the
/// task is filed nowhere.
fn refile_by_schedule(
    transaction: &Transaction,
    task_id: &str,
    start_at_ms: Option<i64>,
    due_at_ms: Option<i64>,
    due_date: Option<&str>,
    now_ms: i64,
    zone: chrono_tz::Tz,
) -> Result<(), CoreError> {
    let column = habits::kanban_column(transaction, task_id)?;
    let Some(column) = column.filter(|c| c == TODAY_COLUMN_ID || c == THIS_WEEK_COLUMN_ID) else {
        return Ok(());
    };
    let today = habits::local_day(now_ms, zone);
    let next_monday = today + Duration::days(7 - i64::from(today.weekday().num_days_from_monday()));
    let target = if let Some(start_ms) = start_at_ms {
        let day = habits::local_day(start_ms, zone);
        if day <= today {
            Some(TODAY_COLUMN_ID)
        } else if day < next_monday && draws_this_week(transaction, task_id)? {
            Some(THIS_WEEK_COLUMN_ID)
        } else {
            None
        }
    } else if let Some(due_day) = due_date.and_then(calendar_date).map(|dt| dt.date()) {
        if due_day <= today {
            Some(TODAY_COLUMN_ID)
        } else if due_day < next_monday && draws_this_week(transaction, task_id)? {
            Some(THIS_WEEK_COLUMN_ID)
        } else {
            None
        }
    } else if let Some(due_ms) = due_at_ms {
        let due_day = habits::local_day(due_ms, zone);
        if due_day <= today {
            Some(TODAY_COLUMN_ID)
        } else if due_day < next_monday && draws_this_week(transaction, task_id)? {
            Some(THIS_WEEK_COLUMN_ID)
        } else {
            None
        }
    } else {
        None
    };
    if target == Some(column.as_str()) {
        return Ok(());
    }
    transaction.execute(
        "UPDATE task_metadata SET kanbanColumn = ?1,
           focusRank = CASE WHEN ?1 = 'today' THEN focusRank ELSE NULL END,
           updatedAt = ?2
         WHERE taskId = ?3",
        params![target, stored(now_ms), task_id],
    )?;
    Ok(())
}

/// Whether the board of the task's list has a This week column.
fn draws_this_week(transaction: &Transaction, task_id: &str) -> Result<bool, CoreError> {
    let list: String =
        transaction.query_row("SELECT listId FROM tasks WHERE id = ?1", [task_id], |row| {
            row.get(0)
        })?;
    let (layout, _) = board_columns::layout(transaction, &format!("{list}/root"))?;
    Ok(layout.iter().any(|column| column.id == THIS_WEEK_COLUMN_ID))
}

/// `WorkspaceStore.updateTaskRecord`: the task row's own fields, written
/// only when one of them changed.
fn update_task_record(
    transaction: &Transaction,
    edit: &EditorSnapshot,
    now_ms: i64,
) -> Result<(), CoreError> {
    let current: Option<(String, String, Option<String>, Option<i64>)> = transaction
        .query_row(
            "SELECT title, notes, dueAt, estimateSeconds FROM tasks WHERE id = ?1",
            [&edit.task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((title, notes, due_at, estimate)) = current else {
        return Err(CoreError::MissingTask {
            id: edit.task_id.clone(),
        });
    };
    let due_ms = due_at
        .as_deref()
        .and_then(parse_stored)
        .map(|at| at.timestamp_millis());
    if title == edit.title
        && notes == edit.notes
        && due_ms == edit.due_at_ms
        && estimate == edit.estimate_seconds
    {
        return Ok(());
    }
    transaction.execute(
        "UPDATE tasks SET title = ?1, notes = ?2, dueAt = ?3, estimateSeconds = ?4, updatedAt = ?5 WHERE id = ?6",
        params![
            edit.title,
            edit.notes,
            edit.due_at_ms.map(stored),
            edit.estimate_seconds,
            stored(now_ms),
            edit.task_id
        ],
    )?;
    Ok(())
}

/// Tracks or stops tracking a task as a daily, only when that changes.
/// `WorkspaceStore.setDailyAttachment`.
fn set_daily_attachment(
    transaction: &Transaction,
    task_id: &str,
    enabled: bool,
    estimate_seconds: Option<i64>,
    now_ms: i64,
) -> Result<(), CoreError> {
    let active: Option<bool> = transaction
        .query_row(
            "SELECT archivedAt IS NULL FROM dailies WHERE taskId = ?1",
            [task_id],
            |row| row.get(0),
        )
        .optional()?;
    if active.unwrap_or(false) == enabled {
        return Ok(());
    }
    if enabled {
        dailies::make_daily(
            transaction,
            task_id,
            &[1, 2, 3, 4, 5, 6, 7],
            None,
            estimate_seconds,
            now_ms,
        )?;
    } else {
        dailies::archive_daily(transaction, task_id, now_ms)?;
    }
    Ok(())
}

/// The start of a `yyyy-MM-dd` date, as a wall-clock time; `None`
/// for anything that is not exactly such a date. `TaskCalendarDate.date`.
pub(crate) fn calendar_date(text: &str) -> Option<chrono::NaiveDateTime> {
    let pieces: Vec<i64> = text
        .split('-')
        .map(|piece| piece.parse().ok())
        .collect::<Option<_>>()?;
    let [year, month, day] = pieces.as_slice() else {
        return None;
    };
    let date = NaiveDate::from_ymd_opt(
        i32::try_from(*year).ok()?,
        u32::try_from(*month).ok()?,
        u32::try_from(*day).ok()?,
    )?;
    // The round trip refuses "2026-3-8" and the like, as Swift's did.
    if date.format("%Y-%m-%d").to_string() != text {
        return None;
    }
    date.and_hms_opt(0, 0, 0)
}

#[cfg(test)]
mod tests;
