//! Writes to tasks.
//!
//! Step four of docs/rust-core-migration.md moves the task writes here one at
//! a time. Each function takes the caller's transaction, so the CLI can run
//! it inside its own journalled step under its "MCP: " label, and
//! [`crate::workspace::CoreWorkspace`] runs it as the apps' step. Each one
//! names the Swift `WorkspaceStore` method and the Kotlin
//! `WorkspaceRepository` method it replaced.

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::CoreError;
use crate::editor::EditorSnapshot;
use crate::time::stored;

/// Whether a task can carry this priority: 1 (the most urgent) to 4, or -4
/// to -1, which send it behind every task that has none, -1 the very back.
/// Anything else is no priority.
pub fn is_priority(value: i64) -> bool {
    matches!(value, 1..=4 | -4..=-1)
}

/// Where a priority puts a task among others, smallest first: 1 to 4, then
/// no priority, then -4 up to -1, so -1 comes last of all and -2 just
/// before it.
pub fn priority_rank(priority: Option<i64>) -> i64 {
    match priority.filter(|p| is_priority(*p)) {
        Some(p) if p > 0 => p,
        Some(p) => 10 + p,
        None => 5,
    }
}

/// A task to create: everything any client can set when it adds one.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct NewTask {
    pub list_id: String,
    pub title: String,
    pub parent_task_id: Option<String>,
    /// `"task"`, or `"list"` for a nested list.
    pub kind: String,
    pub notes: String,
    pub kanban_column: Option<String>,
    pub start_at_ms: Option<i64>,
    pub due_at_ms: Option<i64>,
    pub estimate_seconds: Option<i64>,
    pub tags: Vec<String>,
    /// 1 to 4, or -4 to -1 for later than the rest; see [`is_priority`].
    pub priority: Option<i64>,
    /// Who it waits on; puts it in the waiting column.
    pub waiting_on: Option<String>,
    pub external_links: Vec<String>,
    /// First among its siblings rather than last.
    pub at_top: bool,
    /// A sibling to go beside: below it, or above with `above`.
    pub adjacent_task_id: Option<String>,
    pub above: bool,
}

/// The board column a task waiting on someone goes in.
pub use crate::waiting::WAITING_COLUMN;

/// Creates a task and returns its id. Everything the add field read off the
/// title (tags, priority, estimate, waiting) goes in the same undo step, so
/// undoing a typed task never leaves its estimate behind as a step of its
/// own. Replaces `WorkspaceStore.createTask`, its Kotlin copy and the CLI's
/// `add_task`.
///
/// Its dates are checked and filed as the editor's save of them would be: a
/// start on or after the due time is refused, and a task added to Today or
/// This week with a start that belongs elsewhere is filed where it belongs.
/// The device's time zone says which day is today; see [`create_task_in`].
pub fn create_task(
    transaction: &Transaction,
    new: &NewTask,
    now_ms: i64,
) -> Result<String, CoreError> {
    let zone = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string());
    create_task_in(transaction, new, now_ms, &zone)
}

/// [`create_task`] in the user's `zone`, an IANA name.
pub fn create_task_in(
    transaction: &Transaction,
    new: &NewTask,
    now_ms: i64,
    zone: &str,
) -> Result<String, CoreError> {
    let title = crate::time::non_empty_name(&new.title)?;
    let waiting_on = crate::waiting::normalized_tag(new.waiting_on.as_deref());
    // Waiting on someone puts it in that column, as `set_waiting` does.
    let kanban_column = if waiting_on.is_some() {
        Some(WAITING_COLUMN.to_string())
    } else {
        new.kanban_column.clone()
    };
    let tags = normalized_strings(&new.tags);
    let priority = new.priority.filter(|p| is_priority(*p));
    let estimate = new.estimate_seconds.filter(|e| *e > 0);

    let list_exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM task_lists WHERE id = ?1)",
        [&new.list_id],
        |row| row.get(0),
    )?;
    if !list_exists {
        return Err(CoreError::MissingList {
            id: new.list_id.clone(),
        });
    }
    let parent = new.parent_task_id.as_deref();
    if let Some(parent) = parent {
        let parent_list: Option<String> = transaction
            .query_row("SELECT listId FROM tasks WHERE id = ?1", [parent], |row| {
                row.get(0)
            })
            .optional()?;
        // A parent that is gone, or in another list, is a place the task
        // cannot go, as Swift reported it.
        if parent_list.as_deref() != Some(new.list_id.as_str()) {
            return Err(CoreError::InvalidTaskMove);
        }
    }
    let sort_order = next_task_order(transaction, &new.list_id, parent)?;
    let id = crate::lists::new_id();
    let now = stored(now_ms);
    transaction.execute(
        "INSERT INTO tasks (id, listId, parentTaskId, title, notes, status, sortOrder, dueAt,
                            estimateSeconds, createdAt, updatedAt, sourceSystem, sourceId, itemKind,
                            isPromoted, archivedAt, completedAt)
         VALUES (?1, ?2, ?3, ?4, ?5, 'open', ?6, ?7, ?8, ?9, ?9, NULL, NULL, ?10, NULL, NULL, NULL)",
        params![
            id,
            new.list_id,
            parent,
            title,
            new.notes,
            sort_order,
            new.due_at_ms.map(stored),
            estimate,
            now,
            if new.kind == "list" { "list" } else { "task" },
        ],
    )?;
    let has_metadata = kanban_column.is_some()
        || !tags.is_empty()
        || priority.is_some()
        || waiting_on.is_some()
        || !new.external_links.is_empty();
    if has_metadata {
        transaction.execute(
            "INSERT INTO task_metadata(taskId, priority, tagsJSON, externalLinksJSON,
                                       kanbanColumn, waitingOn, updatedAt)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                priority,
                crate::time::swift_json_strings(&tags),
                crate::time::swift_json_strings(&new.external_links),
                kanban_column,
                waiting_on,
                now,
            ],
        )?;
    }
    if new.at_top || new.adjacent_task_id.is_some() {
        let mut siblings = task_siblings(transaction, &new.list_id, parent)?;
        siblings.retain(|s| *s != id);
        let insertion = match new.adjacent_task_id.as_deref() {
            Some(adjacent) => {
                let index = siblings
                    .iter()
                    .position(|s| s == adjacent)
                    .ok_or(CoreError::InvalidTaskMove)?;
                index + usize::from(!new.above)
            }
            None => 0,
        };
        siblings.insert(insertion, id.clone());
        persist_task_order(transaction, &siblings, now_ms)?;
    }
    // The start goes in as the editor's would, checked against the deadline
    // and refiling the task by the day it falls on.
    if let Some(start) = new.start_at_ms {
        crate::editor::plan_new_task(transaction, &id, start, now_ms, zone)?;
    }
    // A priority puts it first among its siblings, wherever it was added:
    // setting one is saying it comes before the rest. A negative one says
    // the opposite, and sends it to the back.
    match priority {
        Some(p) if p > 0 => move_task_to_start(transaction, &id, now_ms)?,
        Some(_) => move_task_to_end(transaction, &id, now_ms)?,
        None => {}
    }
    Ok(id)
}

/// [`create_task`], then the conditions it needs, in the caller's one
/// transaction: the add field's slots, which can tick conditions before the
/// task exists, file them with it so one undo takes back the task and all it
/// was given. The requirements go through the editor's save, so a condition
/// from another workspace, or archived, is refused as the editor refuses it.
pub fn create_task_requiring(
    transaction: &Transaction,
    new: &NewTask,
    requirement_groups: &[Vec<String>],
    now_ms: i64,
    zone: &str,
) -> Result<String, CoreError> {
    let id = create_task_in(transaction, new, now_ms, zone)?;
    if requirement_groups.is_empty() {
        return Ok(id);
    }
    let baseline = crate::editor::snapshot(transaction, &id)?;
    let mut edit = baseline.clone();
    let mut planning = edit.planning.clone().unwrap_or_default();
    planning.requirement_groups = Some(requirement_groups.to_vec());
    edit.planning = Some(planning);
    crate::editor::save_editor(transaction, &edit, &baseline, now_ms, zone)?;
    Ok(id)
}

/// Tags trimmed, blanks dropped and repeats (ignoring case) kept once, in
/// the order given: `WorkspaceStore.normalizedStrings`.
pub fn normalized_strings(values: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    values
        .iter()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty() && seen.insert(v.to_lowercase()))
        .map(str::to_string)
        .collect()
}

/// What [`delete_task`] removed.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DeletedTask {
    pub id: String,
    pub title: String,
    /// How many tasks below it went with it.
    pub subtasks_deleted: u32,
}

/// Deletes a task and, through the foreign keys, everything below it: its
/// subtasks, their metadata, and whatever hangs off them. One undo step
/// brings the whole subtree back. Replaces `WorkspaceStore.deleteTask` and
/// `WorkspaceRepository.deleteTask`.
pub fn delete_task(transaction: &Transaction, id: &str) -> Result<DeletedTask, CoreError> {
    let title: String = transaction
        .query_row("SELECT title FROM tasks WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .optional()?
        .ok_or_else(|| CoreError::MissingTask { id: id.to_string() })?;
    let subtasks_deleted: u32 = transaction.query_row(
        "WITH RECURSIVE subtree(id) AS (
           SELECT id FROM tasks WHERE parentTaskId = ?1
           UNION ALL
           SELECT tasks.id FROM tasks JOIN subtree ON tasks.parentTaskId = subtree.id
         )
         SELECT COUNT(*) FROM subtree",
        [id],
        |row| row.get(0),
    )?;
    transaction.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
    Ok(DeletedTask {
        id: id.to_string(),
        title,
        subtasks_deleted,
    })
}

/// Moves a task, with its subtree, under `parent_task_id` in `list_id` (or to
/// the top of that list), after the tasks already there. With
/// `to_visible_root` the parent is the list's visible root, the wrapper a
/// list made from an imported project shows the children of. A move into
/// the task itself or below it is refused, as is a parent from another list.
/// Replaces `WorkspaceStore.moveTask`, its Kotlin copy, and the move half of
/// the CLI's `move_task`.
pub fn move_task(
    transaction: &Transaction,
    id: &str,
    list_id: &str,
    parent_task_id: Option<&str>,
    to_visible_root: bool,
    now_ms: i64,
) -> Result<(), CoreError> {
    let (current_list, current_parent) = task_place(transaction, id)?;
    let list_exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM task_lists WHERE id = ?1)",
        [list_id],
        |row| row.get(0),
    )?;
    if !list_exists {
        return Err(CoreError::MissingList {
            id: list_id.to_string(),
        });
    }
    let parent = if to_visible_root {
        visible_root_parent(transaction, list_id)?
    } else {
        parent_task_id.map(str::to_string)
    };
    let descendants = descendant_ids(transaction, id)?;
    if let Some(parent) = parent.as_deref() {
        if parent == id || descendants.iter().any(|d| d == parent) {
            return Err(CoreError::InvalidTaskMove);
        }
        let parent_list: Option<String> = transaction
            .query_row("SELECT listId FROM tasks WHERE id = ?1", [parent], |row| {
                row.get(0)
            })
            .optional()?;
        if parent_list.as_deref() != Some(list_id) {
            return Err(CoreError::InvalidTaskMove);
        }
    }
    if current_list == list_id && current_parent == parent {
        return Ok(());
    }
    let now = stored(now_ms);
    if current_list != list_id {
        // One statement over the subtree, sorted, so the journal records the
        // same rows in the same order whichever client asked.
        let mut ids = descendants;
        ids.push(id.to_string());
        ids.sort_unstable();
        let placeholders = (0..ids.len())
            .map(|index| format!("?{}", index + 3))
            .collect::<Vec<_>>()
            .join(", ");
        let mut values: Vec<&dyn rusqlite::ToSql> = vec![&list_id, &now];
        values.extend(ids.iter().map(|id| id as &dyn rusqlite::ToSql));
        transaction.execute(
            &format!("UPDATE tasks SET listId = ?1, updatedAt = ?2 WHERE id IN ({placeholders})"),
            values.as_slice(),
        )?;
    }
    let sort_order = next_task_order(transaction, list_id, parent.as_deref())?;
    transaction.execute(
        "UPDATE tasks SET parentTaskId = ?1, sortOrder = ?2, updatedAt = ?3 WHERE id = ?4",
        params![parent, sort_order, now, id],
    )?;
    Ok(())
}

/// Puts a task at `index` (from zero, clamped to the end) among its
/// siblings. A position it already holds changes nothing. The CLI's
/// `move_task` with a `position`.
pub fn place_task_at(
    transaction: &Transaction,
    id: &str,
    index: u32,
    now_ms: i64,
) -> Result<(), CoreError> {
    let (list_id, parent) = task_place(transaction, id)?;
    let before = task_siblings(transaction, &list_id, parent.as_deref())?;
    let mut siblings: Vec<String> = before.iter().filter(|s| *s != id).cloned().collect();
    let index = (index as usize).min(siblings.len());
    siblings.insert(index, id.to_string());
    if siblings != before {
        persist_task_order(transaction, &siblings, now_ms)?;
    }
    Ok(())
}

/// Moves a task `offset` places among its siblings, stopping at either end.
/// Replaces `WorkspaceStore.moveTaskWithinSiblings` and its Kotlin copy.
///
/// It counts only the siblings drawn in the same section as the task: an open
/// task steps past open ones, a blocked task past blocked ones, a buried task
/// past buried ones (`buried`). Blocked and buried tasks always sort last, so
/// stepping an open task "down" past one of them moved it nowhere you could
/// see, and each such press then had to be undone with another on the way
/// back up. A buried task stepped past keeps its own place among the rest, so
/// it comes back there when it starts.
pub fn move_task_within_siblings(
    transaction: &Transaction,
    id: &str,
    offset: i32,
    now_ms: i64,
) -> Result<(), CoreError> {
    let (list_id, parent) = task_place(transaction, id)?;
    let mut siblings = task_siblings(transaction, &list_id, parent.as_deref())?;
    if !siblings.iter().any(|s| s == id) {
        return Ok(());
    }
    let (blocked, buried) = sections(transaction, now_ms)?;
    let section = crate::buried::sibling_section(id, &blocked, &buried);
    let drawn = drawn_siblings(transaction, &list_id, parent.as_deref(), &blocked, &buried)?;
    let peers: Vec<&String> = drawn
        .iter()
        .filter(|s| crate::buried::sibling_section(s, &blocked, &buried) == section)
        .collect();
    let Some(position) = peers.iter().position(|s| *s == id) else {
        return Ok(());
    };
    let last = peers.len() as i64 - 1;
    let target = (position as i64 + i64::from(offset)).clamp(0, last) as usize;
    if target == position {
        return Ok(());
    }
    let target_id = peers[target].clone();
    let index = siblings
        .iter()
        .position(|s| s == id)
        .expect("checked above");
    let moved = siblings.remove(index);
    let target_index = siblings
        .iter()
        .position(|s| *s == target_id)
        .expect("a peer is a sibling");
    siblings.insert(
        if offset < 0 {
            target_index
        } else {
            target_index + 1
        },
        moved,
    );
    persist_task_order(transaction, &siblings, now_ms)
}

/// Moves a task to the top of its siblings. Replaces
/// `WorkspaceStore.moveTaskToStart` and its Kotlin copy.
pub fn move_task_to_start(
    transaction: &Transaction,
    id: &str,
    now_ms: i64,
) -> Result<(), CoreError> {
    let (list_id, parent) = task_place(transaction, id)?;
    let mut siblings = task_siblings(transaction, &list_id, parent.as_deref())?;
    match siblings.iter().position(|s| s == id) {
        Some(index) if index > 0 => {
            let moved = siblings.remove(index);
            siblings.insert(0, moved);
            persist_task_order(transaction, &siblings, now_ms)
        }
        _ => Ok(()),
    }
}

/// Moves a task to the end of its siblings: what a negative priority does.
pub fn move_task_to_end(transaction: &Transaction, id: &str, now_ms: i64) -> Result<(), CoreError> {
    let (list_id, parent) = task_place(transaction, id)?;
    let mut siblings = task_siblings(transaction, &list_id, parent.as_deref())?;
    match siblings.iter().position(|s| s == id) {
        Some(index) if index + 1 < siblings.len() => {
            let moved = siblings.remove(index);
            siblings.push(moved);
            persist_task_order(transaction, &siblings, now_ms)
        }
        _ => Ok(()),
    }
}

/// Puts a task directly before a sibling, as a drag does, and on a board
/// drops it into `kanban_column` too. Both must share a list and a parent.
/// Replaces `WorkspaceStore.moveTaskBefore` and its Kotlin copy.
pub fn move_task_before(
    transaction: &Transaction,
    id: &str,
    target_id: &str,
    kanban_column: Option<&str>,
    now_ms: i64,
) -> Result<(), CoreError> {
    let (list_id, parent) = task_place(transaction, id)?;
    let target = task_place(transaction, target_id)?;
    if target != (list_id.clone(), parent.clone()) {
        return Err(CoreError::InvalidTaskMove);
    }
    if id == target_id {
        return Ok(());
    }
    let mut siblings = task_siblings(transaction, &list_id, parent.as_deref())?;
    let Some(index) = siblings.iter().position(|s| s == id) else {
        return Ok(());
    };
    let moved = siblings.remove(index);
    let Some(target_index) = siblings.iter().position(|s| s == target_id) else {
        return Ok(());
    };
    siblings.insert(target_index, moved);
    persist_task_order(transaction, &siblings, now_ms)?;
    if let Some(column) = kanban_column {
        transaction.execute(
            "INSERT INTO task_metadata(taskId, tagsJSON, externalLinksJSON, kanbanColumn, updatedAt)
             VALUES (?1, '[]', '[]', ?2, ?3)
             ON CONFLICT(taskId) DO UPDATE SET
               kanbanColumn = excluded.kanbanColumn,
               focusRank = CASE WHEN kanbanColumn IS 'today' AND excluded.kanbanColumn IS NOT 'today'
                                THEN NULL ELSE focusRank END,
               updatedAt = excluded.updatedAt",
            params![id, column, stored(now_ms)],
        )?;
    }
    Ok(())
}

/// Makes a task the last child of the sibling above it, and closes the gap
/// it leaves. The first sibling has nothing to go under. "Above" is as drawn,
/// buried tasks last, so a task never goes under a buried one it was not
/// beside. Replaces `WorkspaceStore.indentTask` and its Kotlin copy.
pub fn indent_task(transaction: &Transaction, id: &str, now_ms: i64) -> Result<(), CoreError> {
    let (list_id, parent) = task_place(transaction, id)?;
    let siblings = task_siblings(transaction, &list_id, parent.as_deref())?;
    let (blocked, buried) = sections(transaction, now_ms)?;
    let drawn = drawn_siblings(transaction, &list_id, parent.as_deref(), &blocked, &buried)?;
    let Some(index) = drawn.iter().position(|s| s == id).filter(|&i| i > 0) else {
        return Ok(());
    };
    let new_parent = drawn[index - 1].clone();
    let sort_order = next_task_order(transaction, &list_id, Some(&new_parent))?;
    transaction.execute(
        "UPDATE tasks SET parentTaskId = ?1, sortOrder = ?2, updatedAt = ?3 WHERE id = ?4",
        params![new_parent, sort_order, stored(now_ms), id],
    )?;
    let rest: Vec<String> = siblings.into_iter().filter(|s| s != id).collect();
    persist_task_order(transaction, &rest, now_ms)
}

/// Makes a task the sibling just after its parent. A top-level task stays.
/// Its old siblings keep their numbers, as before. Replaces
/// `WorkspaceStore.outdentTask` and its Kotlin copy.
pub fn outdent_task(transaction: &Transaction, id: &str, now_ms: i64) -> Result<(), CoreError> {
    let (list_id, parent) = task_place(transaction, id)?;
    let Some(parent_id) = parent else {
        return Ok(());
    };
    let grandparent: Option<Option<String>> = transaction
        .query_row(
            "SELECT parentTaskId FROM tasks WHERE id = ?1",
            [&parent_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(grandparent) = grandparent else {
        return Ok(());
    };
    let mut siblings = task_siblings(transaction, &list_id, grandparent.as_deref())?;
    let parent_index = siblings
        .iter()
        .position(|s| *s == parent_id)
        .unwrap_or(siblings.len().saturating_sub(1));
    let at = (parent_index + 1).min(siblings.len());
    siblings.insert(at, id.to_string());
    // The task's new parent is written in the same row update as its new
    // place, so the step journals one change for it, as Swift's did.
    let now = stored(now_ms);
    for (index, sibling) in siblings.iter().enumerate() {
        if sibling == id {
            transaction.execute(
                "UPDATE tasks SET parentTaskId = ?1, sortOrder = ?2, updatedAt = ?3 WHERE id = ?4",
                params![grandparent, index as i64, now, id],
            )?;
        } else {
            transaction.execute(
                "UPDATE tasks SET sortOrder = ?1, updatedAt = ?2 WHERE id = ?3",
                params![index as i64, now, sibling],
            )?;
        }
    }
    Ok(())
}

/// Puts tasks in a board column (`None`, or blank, clears it), as one step
/// however many there are. Every id must be a task, or nothing is written.
/// Ids are written once each, in the order given. Replaces both
/// `WorkspaceStore.setKanbanColumn` overloads and their Kotlin copies.
pub fn set_kanban_column(
    transaction: &Transaction,
    task_ids: &[String],
    column: Option<&str>,
    now_ms: i64,
) -> Result<(), CoreError> {
    let mut ids: Vec<&str> = Vec::with_capacity(task_ids.len());
    for id in task_ids {
        if !ids.contains(&id.as_str()) {
            ids.push(id);
        }
    }
    for id in &ids {
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id = ?1)",
            [id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(CoreError::MissingTask { id: id.to_string() });
        }
    }
    let value = column.map(str::trim).filter(|c| !c.is_empty());
    let now = stored(now_ms);
    let mut statement = transaction.prepare(
        "INSERT INTO task_metadata(taskId, tagsJSON, externalLinksJSON, kanbanColumn, updatedAt)
         VALUES (?1, '[]', '[]', ?2, ?3)
         ON CONFLICT(taskId) DO UPDATE SET
           kanbanColumn = excluded.kanbanColumn,
           -- Leaving Today drops the place in the day, as planning for today does.
           focusRank = CASE WHEN kanbanColumn IS 'today' AND excluded.kanbanColumn IS NOT 'today'
                            THEN NULL ELSE focusRank END,
           updatedAt = excluded.updatedAt",
    )?;
    for id in ids {
        statement.execute(params![id, value, now])?;
    }
    Ok(())
}

/// Places a task on the priority matrix by urgency and importance (either
/// may be unset). Replaces `WorkspaceStore.setMatrixPosition` and its
/// Kotlin copy.
///
/// A task that changes quadrant loses its place within the old one
/// (`matrixRank`), so it lands after the ranked tasks of the new one; a
/// task placed where it already is keeps it.
pub fn set_matrix_position(
    transaction: &Transaction,
    id: &str,
    urgency: Option<i64>,
    importance: Option<i64>,
    now_ms: i64,
) -> Result<(), CoreError> {
    task_place(transaction, id)?;
    transaction.execute(
        "INSERT INTO task_metadata(taskId, tagsJSON, externalLinksJSON, matrixUrgency, matrixImportance, updatedAt)
         VALUES (?1, '[]', '[]', ?2, ?3, ?4)
         ON CONFLICT(taskId) DO UPDATE SET
           matrixRank = CASE
             WHEN matrixUrgency IS excluded.matrixUrgency AND matrixImportance IS excluded.matrixImportance
             THEN matrixRank ELSE NULL END,
           matrixUrgency = excluded.matrixUrgency,
           matrixImportance = excluded.matrixImportance,
           updatedAt = excluded.updatedAt",
        params![id, urgency, importance, stored(now_ms)],
    )?;
    Ok(())
}

/// Places a task in a matrix quadrant at a given point: `order` is the
/// quadrant as it should now read, top to bottom, with `id` where it goes
/// (it is put last when missing). Each task in it that is in that quadrant
/// takes its index as its `matrixRank`; one in another quadrant, or not on
/// the matrix, is left alone, and only ranks that change are written.
///
/// A client passes the quadrant as it draws it, which in a single list's
/// scope is the whole quadrant. In a combined scope it is the whole of it
/// too, since that matrix draws every list's cards; tasks it does not draw
/// keep their ranks and sort among the drawn ones by them.
pub fn place_in_matrix(
    transaction: &Transaction,
    id: &str,
    urgency: i64,
    importance: i64,
    order: &[String],
    now_ms: i64,
) -> Result<(), CoreError> {
    set_matrix_position(transaction, id, Some(urgency), Some(importance), now_ms)?;
    let mut order: Vec<&str> = order.iter().map(String::as_str).collect();
    if !order.contains(&id) {
        order.push(id);
    }
    let mut statement = transaction.prepare(
        "UPDATE task_metadata SET matrixRank = ?2, updatedAt = ?3
         WHERE taskId = ?1 AND matrixUrgency = ?4 AND matrixImportance = ?5 AND matrixRank IS NOT ?2",
    )?;
    let now = stored(now_ms);
    let mut seen = HashSet::new();
    let mut rank: i64 = 0;
    for task in order {
        if !seen.insert(task) {
            continue;
        }
        statement.execute(params![task, rank, now, urgency, importance])?;
        rank += 1;
    }
    Ok(())
}

/// The parent a list shows the children of: its visible root, but only while
/// that is still the list's single top-level task.
pub fn visible_root_parent(
    transaction: &Transaction,
    list_id: &str,
) -> Result<Option<String>, CoreError> {
    let root: Option<String> = transaction
        .query_row(
            "SELECT visibleRootTaskId FROM task_lists WHERE id = ?1",
            [list_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    let Some(root) = root else {
        return Ok(None);
    };
    let mut statement =
        transaction.prepare("SELECT id FROM tasks WHERE listId = ?1 AND parentTaskId IS NULL")?;
    let roots: Vec<String> = statement
        .query_map([list_id], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok((roots == [root.clone()]).then_some(root))
}

/// A task's list and parent.
fn task_place(transaction: &Transaction, id: &str) -> Result<(String, Option<String>), CoreError> {
    transaction
        .query_row(
            "SELECT listId, parentTaskId FROM tasks WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::MissingTask { id: id.to_string() })
}

/// The blocked tasks and the tasks buried at `now_ms`, for the sections
/// siblings are drawn in.
#[allow(clippy::type_complexity)]
fn sections(
    transaction: &Transaction,
    now_ms: i64,
) -> Result<(HashSet<String>, HashSet<String>), CoreError> {
    Ok((
        crate::dependencies::blocked_task_ids(transaction)?
            .into_iter()
            .collect(),
        crate::buried::buried_task_ids(transaction, now_ms)?
            .into_iter()
            .collect(),
    ))
}

/// A task's siblings as they are drawn at the moment `blocked` and `buried`
/// were read: open work, then blocked, then buried, each in its own order —
/// the reads' `buried::section_order`, with the moment the caller's.
fn drawn_siblings(
    transaction: &Transaction,
    list_id: &str,
    parent: Option<&str>,
    blocked: &HashSet<String>,
    buried: &HashSet<String>,
) -> Result<Vec<String>, CoreError> {
    let mut statement = transaction.prepare(
        "SELECT id FROM tasks WHERE listId = ?1 AND parentTaskId IS ?2 ORDER BY sortOrder, createdAt, id",
    )?;
    let mut ids: Vec<String> = statement
        .query_map(params![list_id, parent], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    // Stable: each section keeps the order the numbers give it.
    ids.sort_by_key(|id| crate::buried::sibling_section(id, blocked, buried));
    Ok(ids)
}

/// A task's siblings in order. The id breaks a tie in position and creation
/// time, so every client sees the same order.
fn task_siblings(
    transaction: &Transaction,
    list_id: &str,
    parent: Option<&str>,
) -> Result<Vec<String>, CoreError> {
    // The order the siblings are drawn in, blocked ones last, so a reorder
    // moves a task past the row the user sees beside it. Buried ones are left
    // where their numbers put them: renumbering them last would keep them at
    // the bottom after they start. The moves that read a position as drawn
    // (`move_task_within_siblings`, `indent_task`) set them apart themselves.
    let blocked = crate::dependencies::blocked_last(transaction)?;
    let mut statement = transaction.prepare(&format!(
        "SELECT id FROM tasks WHERE listId = ?1 AND parentTaskId IS ?2 ORDER BY {blocked}, sortOrder, createdAt, id"
    ))?;
    let ids = statement
        .query_map(params![list_id, parent], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

fn next_task_order(
    transaction: &Transaction,
    list_id: &str,
    parent: Option<&str>,
) -> Result<i64, CoreError> {
    Ok(transaction.query_row(
        "SELECT COALESCE(MAX(sortOrder), -1) + 1 FROM tasks WHERE listId = ?1 AND parentTaskId IS ?2",
        params![list_id, parent],
        |row| row.get(0),
    )?)
}

/// Numbers tasks from zero in order and stamps each, as both clients'
/// `persistTaskOrder` did.
fn persist_task_order(
    transaction: &Transaction,
    ids: &[String],
    now_ms: i64,
) -> Result<(), CoreError> {
    let now = stored(now_ms);
    let mut statement =
        transaction.prepare("UPDATE tasks SET sortOrder = ?1, updatedAt = ?2 WHERE id = ?3")?;
    for (index, id) in ids.iter().enumerate() {
        statement.execute(params![index as i64, now, id])?;
    }
    Ok(())
}

pub(crate) fn descendant_ids(
    transaction: &Transaction,
    id: &str,
) -> Result<Vec<String>, CoreError> {
    let mut statement = transaction.prepare(
        "WITH RECURSIVE subtree(id) AS (
           SELECT id FROM tasks WHERE parentTaskId = ?1
           UNION
           SELECT tasks.id FROM tasks JOIN subtree ON tasks.parentTaskId = subtree.id
         )
         SELECT id FROM subtree",
    )?;
    let ids = statement
        .query_map([id], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Opens, completes or cancels a task. Only a task that is newly closed is
/// stamped as completed, so a sync or a repeated command closing it again does
/// not move it into today. Closing one occurrence of a repeating task writes
/// the next one, and ends the habits made from it. A task that is gone is
/// left alone, as both clients did. Replaces `WorkspaceStore.setStatus` and
/// `WorkspaceRepository.setStatus`.
///
/// `zone` is the user's IANA time zone, which the next occurrence's dates
/// are stepped in.
pub fn set_status(
    transaction: &Transaction,
    task_id: &str,
    status: &str,
    now_ms: i64,
    zone: &str,
) -> Result<(), CoreError> {
    if !matches!(status, "open" | "completed" | "cancelled") {
        return Err(CoreError::InvalidStatus {
            status: status.to_string(),
        });
    }
    let completed_at: Option<Option<String>> = transaction
        .query_row(
            "SELECT completedAt FROM tasks WHERE id = ?1",
            [task_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(completed_at) = completed_at else {
        return Ok(());
    };
    // A commitment's subtask is done for the day, not for good, and
    // cancelling it skips the day rather than ending it (`commitments.rs`).
    let handled = match status {
        "completed" => {
            crate::commitments::tick_instead_of_completing(transaction, task_id, now_ms, zone)?
        }
        "cancelled" => {
            crate::commitments::skip_instead_of_cancelling(transaction, task_id, now_ms, zone)?
        }
        _ => false,
    };
    if handled {
        return Ok(());
    }
    let was_open = completed_at.is_none();
    let closed_at = completed_at.clone();
    let now = stored(now_ms);
    let completed_at = if status == "open" {
        None
    } else {
        Some(completed_at.unwrap_or_else(|| now.clone()))
    };
    transaction.execute(
        "UPDATE tasks SET status = ?1, completedAt = ?2, updatedAt = ?3 WHERE id = ?4",
        params![status, completed_at, now, task_id],
    )?;
    // Reopening a finished occurrence takes back the next one its closing
    // wrote, while nobody has touched it: the task it stood in for is open
    // again. One that has been edited, moved or finished stays.
    if status == "open"
        && let Some(closed_at) = closed_at.as_deref()
    {
        remove_untouched_next_occurrence(transaction, task_id, closed_at)?;
    }
    if status != "open" && was_open {
        schedule_next_occurrence(transaction, task_id, now_ms, crate::periodic::zone(zone))?;
        expire_habits(transaction, task_id, now_ms)?;
    }
    // Doing a follow-up whose task repeats its chase sets the next one; a
    // follow-up cancelled or deleted sets nothing, so the chain stops there.
    if status == "completed" && was_open {
        crate::waiting::schedule_next_follow_up(
            transaction,
            task_id,
            now_ms,
            crate::periodic::zone(zone),
        )?;
    }
    if status != "open" {
        close_open_descendants(transaction, task_id, status, &now, now_ms)?;
    }
    Ok(())
}

/// Deletes the occurrence `task_id`'s closing at `closed_at` wrote, if it is
/// still as it was written: open, made at that moment, its row and its
/// metadata unchanged since (every write stamps `updatedAt`), and nothing
/// hung under it or made a daily of it. Returns whether it did.
fn remove_untouched_next_occurrence(
    transaction: &Transaction,
    task_id: &str,
    closed_at: &str,
) -> Result<bool, CoreError> {
    let next: Option<String> = transaction
        .query_row(
            "SELECT t.id FROM tasks t JOIN task_metadata m ON m.taskId = t.id
             WHERE m.seriesTaskId = (SELECT COALESCE(seriesTaskId, taskId) FROM task_metadata WHERE taskId = ?1)
               AND t.id <> ?1 AND t.status = 'open' AND t.completedAt IS NULL AND t.archivedAt IS NULL
               AND t.createdAt = ?2 AND t.updatedAt = ?2 AND m.updatedAt = ?2
               AND m.recurrenceRule IS NOT NULL
               AND NOT EXISTS (SELECT 1 FROM tasks c WHERE c.parentTaskId = t.id)
               AND NOT EXISTS (SELECT 1 FROM dailies d WHERE d.taskId = t.id)
             ORDER BY t.id LIMIT 1",
            params![task_id, closed_at],
            |row| row.get(0),
        )
        .optional()?;
    let Some(next) = next else {
        return Ok(false);
    };
    transaction.execute("DELETE FROM tasks WHERE id = ?1", [&next])?;
    Ok(true)
}

/// Closing a task closes what is still open beneath it, the same way, in the
/// same step, so one undo brings the branch back. Subtasks already closed
/// keep their own status and stamp, and reopening never cascades: a subtask
/// finished before its parent was reopened stays finished. A repeating
/// subtask does not write its next occurrence — its parent is closed, so the
/// occurrence would only be hidden under it — but its habits end, since the
/// task they were made of has.
///
/// Lists are the exception, both ways. Closing a list hides what is in it
/// without finishing it, so the cascade neither starts at a list nor walks
/// into one nested in the branch.
fn close_open_descendants(
    transaction: &Transaction,
    task_id: &str,
    status: &str,
    now: &str,
    now_ms: i64,
) -> Result<(), CoreError> {
    let mut statement = transaction.prepare(
        "WITH RECURSIVE subtree(id) AS (
           SELECT id FROM tasks
           WHERE id = ?1 AND COALESCE(itemKind, 'task') != 'list'
           UNION
           SELECT tasks.id FROM tasks JOIN subtree ON tasks.parentTaskId = subtree.id
           WHERE COALESCE(tasks.itemKind, 'task') != 'list'
         )
         SELECT id FROM subtree WHERE id != ?1",
    )?;
    let ids: Vec<String> = statement
        .query_map([task_id], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    for id in ids {
        let closed = transaction.execute(
            "UPDATE tasks SET status = ?1, completedAt = ?2, updatedAt = ?2
             WHERE id = ?3 AND completedAt IS NULL",
            params![status, now, id],
        )?;
        if closed > 0 {
            expire_habits(transaction, &id, now_ms)?;
        }
    }
    Ok(())
}

/// Writes the next occurrence of a repeating task just after it, and returns
/// its id; nothing for a list, a task with no rule this app wrote, or one
/// with no dates to step from. `WorkspaceStore.scheduleNextOccurrence`.
///
/// Steps from the dates the occurrence carried, so "every 3 days" keeps its
/// own rhythm. Everything describing the work carries over; what describes
/// this sitting (its board column, its rung on the ladder) does not.
pub fn schedule_next_occurrence(
    transaction: &Transaction,
    task_id: &str,
    now_ms: i64,
    zone: chrono_tz::Tz,
) -> Result<Option<String>, CoreError> {
    use crate::periodic::Cadence;
    use crate::time::{parse_stored, stored_instant};

    let Some(task) = transaction
        .query_row(
            "SELECT listId, parentTaskId, title, notes, sortOrder, dueAt, estimateSeconds, itemKind, isPromoted
             FROM tasks WHERE id = ?1",
            [task_id],
            |row| {
                Ok(Occurrence {
                    list_id: row.get(0)?,
                    parent: row.get(1)?,
                    title: row.get(2)?,
                    notes: row.get(3)?,
                    sort_order: row.get(4)?,
                    due_at: row.get(5)?,
                    estimate: row.get(6)?,
                    // Older rows predate both columns; the clients read them
                    // as an ordinary, unpromoted task.
                    kind: row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "task".into()),
                    promoted: row.get::<_, Option<bool>>(8)?.unwrap_or(false),
                })
            },
        )
        .optional()?
    else {
        return Ok(None);
    };
    let Occurrence {
        list_id,
        parent,
        title,
        notes,
        sort_order,
        due_at,
        estimate,
        kind,
        promoted,
    } = task;
    if kind == "list" {
        return Ok(None);
    }
    type Metadata = (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let metadata: Option<Metadata> = transaction
        .query_row(
            "SELECT recurrenceRule, startAt, planningJSON, seriesTitle FROM task_metadata WHERE taskId = ?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((Some(rule), start_at, planning_json, series_title)) = metadata else {
        return Ok(None);
    };
    // An occurrence renamed for itself alone hands the series' title on.
    let title = series_title.unwrap_or(title);
    let Some(cadence) = Cadence::parse(&rule) else {
        return Ok(None);
    };
    let Some(dates) = next_dates(
        cadence,
        due_at.as_deref().and_then(parse_stored),
        planning_due_day(planning_json.as_deref()).as_deref(),
        start_at.as_deref().and_then(parse_stored),
        now_ms,
        zone,
    ) else {
        return Ok(None);
    };
    let planning_json = match (&planning_json, &dates.due_day) {
        (Some(json), Some(day)) => Some(crate::editor::planning_json_with_due_day(json, day)?),
        (json, _) => json.clone(),
    };

    let id = crate::lists::new_id();
    let stamp = stored(now_ms);
    transaction.execute(
        "INSERT INTO tasks (id, listId, parentTaskId, title, notes, status, sortOrder, dueAt, estimateSeconds,
                            sourceSystem, sourceId, itemKind, isPromoted, archivedAt, completedAt, createdAt, updatedAt)
         VALUES (?1, ?2, ?3, ?4, ?5, 'open', ?6, ?7, ?8, NULL, NULL, ?9, ?10, NULL, NULL, ?11, ?11)",
        params![
            id,
            list_id,
            parent,
            title,
            notes,
            sort_order,
            dates.due_at.map(stored_instant),
            estimate,
            kind,
            promoted,
            stamp
        ],
    )?;
    transaction.execute(
        "INSERT INTO task_metadata (taskId, priority, startAt, tagsJSON, recurrenceRule, matrixUrgency,
                                    matrixImportance, kanbanColumn, externalLinksJSON, focusRank, planningJSON,
                                    linkDetailsJSON, seriesTaskId, updatedAt)
         SELECT ?1, priority, ?2, tagsJSON, recurrenceRule, matrixUrgency, matrixImportance, NULL,
                externalLinksJSON, NULL, ?5, linkDetailsJSON, COALESCE(seriesTaskId, taskId), ?3
         FROM task_metadata WHERE taskId = ?4",
        params![
            id,
            dates.start_at.map(stored_instant),
            stamp,
            task_id,
            planning_json
        ],
    )?;

    // The finished occurrence keeps its place; the next one follows it.
    let mut siblings = task_siblings(transaction, &list_id, parent.as_deref())?;
    siblings.retain(|sibling| sibling != &id);
    if let Some(index) = siblings.iter().position(|sibling| sibling == task_id) {
        siblings.insert(index + 1, id.clone());
        persist_task_order(transaction, &siblings, now_ms)?;
    }
    Ok(Some(id))
}

/// The dates a repeating task's next occurrence carries.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NextDates {
    due_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The next due day, `yyyy-MM-dd`, for a task due on a day.
    due_day: Option<String>,
    start_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// The due day a stored `planningJSON` gives, if any.
fn planning_due_day(json: Option<&str>) -> Option<String> {
    crate::planning::Planning::from_json(json?).ok()?.due_date
}

/// When the occurrence after one with these dates falls, finished at
/// `now_ms`; nothing when there is nothing to step from.
///
/// A due time steps on until it is after now, a due day — from its local
/// midnight — until it is today or later, so a task finished late keeps its
/// rhythm and its next turn is not already behind it. A start moves by as
/// many days as the deadline did, keeping its time of day, so it stays as far
/// ahead of the deadline as it was; with no deadline it steps on like a due
/// time. A task with a rule and no dates at all starts at the beginning of
/// the day of its next turn, not to the second a period after it was done.
fn next_dates(
    cadence: crate::periodic::Cadence,
    due_at: Option<chrono::DateTime<chrono::Utc>>,
    due_day: Option<&str>,
    start_at: Option<chrono::DateTime<chrono::Utc>>,
    now_ms: i64,
    zone: chrono_tz::Tz,
) -> Option<NextDates> {
    use crate::periodic::resolve;
    let now = chrono::DateTime::from_timestamp_millis(now_ms).unwrap_or_default();
    let local_day = |at: chrono::DateTime<chrono::Utc>| at.with_timezone(&zone).date_naive();
    let midnight = |day: chrono::NaiveDate| resolve(zone, day.and_time(chrono::NaiveTime::MIN));

    let next_due = due_at.and_then(|due| cadence.next_occurrence(due, Some(now), zone));
    let due_day = due_day.and_then(crate::editor::calendar_date);
    let next_due_day = due_day.and_then(|day| {
        // Strictly after the moment before today began: today or later.
        let floor = midnight(local_day(now)) - chrono::Duration::milliseconds(1);
        cadence
            .next_occurrence(resolve(zone, day), Some(floor), zone)
            .map(local_day)
    });
    // The deadline the start keeps its distance from: the due time, else the day.
    let deadline_days = match (due_at, next_due) {
        (Some(from), Some(to)) => Some((local_day(from), local_day(to))),
        _ => due_day
            .zip(next_due_day)
            .map(|(from, to)| (from.date(), to)),
    };
    let next_start = match (start_at, deadline_days) {
        (Some(start), Some((from, to))) => {
            let local = start.with_timezone(&zone).naive_local();
            Some(resolve(
                zone,
                local + chrono::Duration::days((to - from).num_days()),
            ))
        }
        (Some(start), None) => cadence.next_occurrence(start, Some(now), zone),
        (None, _) => None,
    }
    .or_else(|| {
        (next_due.is_none() && next_due_day.is_none())
            .then(|| cadence.next_occurrence(now, Some(now), zone))
            .flatten()
            .map(|turn| midnight(local_day(turn)))
    });
    if next_start.is_none() && next_due.is_none() && next_due_day.is_none() {
        return None;
    }
    Some(NextDates {
        due_at: next_due,
        due_day: next_due_day.map(|day| day.format("%Y-%m-%d").to_string()),
        start_at: next_start,
    })
}

/// A repeating task's dates moved onto or past its own next turn: the day
/// it would come round again anyway. The clients ask before saving, and let
/// the person choose between one task standing for both days and keeping a
/// second, one-off copy on that day.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RecurrenceClash {
    pub task_id: String,
    pub rule: String,
    /// When the task would have come round again, stepping from its dates.
    pub next_turn_ms: i64,
    /// Where it is being moved to: the due date when it has one, else the start.
    pub moved_to_ms: i64,
}

/// Whether moving `task_id` to these dates lands it on or past its next
/// turn. Only a move counts: a date added where there was none has nothing
/// to collide with, and neither has one moved earlier. The due date decides
/// when the task has one; the start, when it only has that.
///
/// The next turn is the first one after `now_ms` as well as after the task's
/// own date: an overdue daily's turn yesterday is gone, so bringing it to
/// today is catching up, not landing on a turn, and only a move onto one
/// still to come asks.
pub fn recurrence_clash(
    connection: &Connection,
    task_id: &str,
    due_at_ms: Option<i64>,
    start_at_ms: Option<i64>,
    now_ms: i64,
    zone: &str,
) -> Result<Option<RecurrenceClash>, CoreError> {
    use crate::periodic::Cadence;

    let open: Option<bool> = connection
        .query_row(
            "SELECT status = 'open' FROM tasks WHERE id = ?1",
            [task_id],
            |row| row.get(0),
        )
        .optional()?;
    if open != Some(true) {
        return Ok(None);
    }
    let saved = crate::editor::snapshot(connection, task_id)?;
    let Some(rule) = saved.metadata.recurrence_rule.clone() else {
        return Ok(None);
    };
    let Some(cadence) = Cadence::parse(&rule) else {
        return Ok(None);
    };
    let tz = crate::periodic::zone(zone);
    let saved_start = saved.planning.as_ref().and_then(|p| p.start_at_ms);
    let (current, proposed) = match effective_due(&saved, tz) {
        Some(current) => (current, due_at_ms),
        None => match saved_start {
            Some(current) => (current, start_at_ms),
            None => return Ok(None),
        },
    };
    let Some(proposed) = proposed.filter(|proposed| *proposed > current) else {
        return Ok(None);
    };
    let instant = |ms: i64| chrono::DateTime::from_timestamp_millis(ms).unwrap_or_default();
    let Some(next) = cadence.next_occurrence(instant(current), Some(instant(now_ms)), tz) else {
        return Ok(None);
    };
    if instant(proposed).with_timezone(&tz).date_naive() < next.with_timezone(&tz).date_naive() {
        return Ok(None);
    }
    Ok(Some(RecurrenceClash {
        task_id: task_id.to_string(),
        rule,
        next_turn_ms: next.timestamp_millis(),
        moved_to_ms: proposed,
    }))
}

/// `recurrence_clash` for an editor save: the dates `edit` would write.
pub fn editor_recurrence_clash(
    connection: &Connection,
    edit: &EditorSnapshot,
    now_ms: i64,
    zone: &str,
) -> Result<Option<RecurrenceClash>, CoreError> {
    let due = effective_due(edit, crate::periodic::zone(zone));
    let start = edit.planning.as_ref().and_then(|p| p.start_at_ms);
    recurrence_clash(connection, &edit.task_id, due, start, now_ms, zone)
}

/// A due time, or the first moment of a due day, in milliseconds.
fn effective_due(snapshot: &EditorSnapshot, zone: chrono_tz::Tz) -> Option<i64> {
    snapshot.due_at_ms.or_else(|| {
        let day = snapshot.planning.as_ref()?.due_date.as_deref()?;
        let midnight = crate::editor::calendar_date(day)?;
        Some(crate::periodic::resolve(zone, midnight).timestamp_millis())
    })
}

/// Whether a rename of a repeating task is for this occurrence alone, set
/// once the rename is saved. `previous_title` is the title before it: the
/// next turn goes back to it (or to the one an earlier rename for this
/// occurrence alone kept), unless the rename put that title back. `None` is
/// a rename for every turn, which lets go of any title kept. Returns
/// whether it wrote anything.
pub fn keep_series_title(
    transaction: &Transaction,
    task_id: &str,
    previous_title: Option<&str>,
    now_ms: i64,
) -> Result<bool, CoreError> {
    let current: Option<(String, Option<String>)> = transaction
        .query_row(
            "SELECT t.title, m.seriesTitle FROM tasks t JOIN task_metadata m ON m.taskId = t.id
             WHERE t.id = ?1 AND m.recurrenceRule IS NOT NULL",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((title, kept)) = current else {
        return Ok(false);
    };
    let wanted = previous_title
        .map(|previous| kept.clone().unwrap_or_else(|| previous.to_string()))
        .filter(|series| series != &title);
    if wanted == kept {
        return Ok(false);
    }
    transaction.execute(
        "UPDATE task_metadata SET seriesTitle = ?1, updatedAt = ?2 WHERE taskId = ?3",
        params![wanted, stored(now_ms), task_id],
    )?;
    Ok(true)
}

/// Writes a one-off copy of a repeating task just after it, carrying the
/// dates `edit` gives, and returns its id: "keep both" when a move lands on
/// the task's next turn. The copy does not repeat — the original still
/// carries the rule, so finishing the copy schedules nothing and the rhythm
/// is not doubled.
pub fn add_occurrence_copy(
    transaction: &Transaction,
    edit: &EditorSnapshot,
    now_ms: i64,
    zone: &str,
) -> Result<String, CoreError> {
    let task_id = edit.task_id.as_str();
    let (list_id, parent): (String, Option<String>) = transaction.query_row(
        "SELECT listId, parentTaskId FROM tasks WHERE id = ?1",
        [task_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let id = crate::lists::new_id();
    let stamp = stored(now_ms);
    transaction.execute(
        "INSERT INTO tasks (id, listId, parentTaskId, title, notes, status, sortOrder, dueAt, estimateSeconds,
                            sourceSystem, sourceId, itemKind, isPromoted, archivedAt, completedAt, createdAt, updatedAt)
         SELECT ?1, listId, parentTaskId, title, notes, 'open', sortOrder, dueAt, estimateSeconds,
                NULL, NULL, itemKind, isPromoted, NULL, NULL, ?2, ?2
         FROM tasks WHERE id = ?3",
        params![id, stamp, task_id],
    )?;
    transaction.execute(
        "INSERT INTO task_metadata (taskId, priority, startAt, tagsJSON, recurrenceRule, matrixUrgency,
                                    matrixImportance, kanbanColumn, externalLinksJSON, focusRank, planningJSON,
                                    linkDetailsJSON, seriesTaskId, updatedAt)
         SELECT ?1, priority, startAt, tagsJSON, NULL, matrixUrgency, matrixImportance, NULL,
                externalLinksJSON, NULL, planningJSON, linkDetailsJSON, COALESCE(seriesTaskId, taskId), ?2
         FROM task_metadata WHERE taskId = ?3",
        params![id, stamp, task_id],
    )?;
    // The dates go through the editor, so a due day stays a day and a start
    // after the deadline is refused as it is anywhere else.
    let baseline = crate::editor::snapshot(transaction, &id)?;
    let mut copy = edit.clone();
    copy.task_id = id.clone();
    copy.workspace_id = baseline.workspace_id.clone();
    copy.metadata.recurrence_rule = None;
    copy.daily_progress = false;
    crate::editor::save_editor(transaction, &copy, &baseline, now_ms, zone)?;
    let mut siblings = task_siblings(transaction, &list_id, parent.as_deref())?;
    siblings.retain(|sibling| sibling != &id);
    if let Some(index) = siblings.iter().position(|sibling| sibling == task_id) {
        siblings.insert(index + 1, id.clone());
        persist_task_order(transaction, &siblings, now_ms)?;
    }
    Ok(id)
}

/// The parts of a finished occurrence the next one is made from.
struct Occurrence {
    list_id: String,
    parent: Option<String>,
    title: String,
    notes: String,
    sort_order: i64,
    due_at: Option<String>,
    estimate: Option<i64>,
    kind: String,
    promoted: bool,
}

/// Ends the habits made from a task that is set to end when it is done, and
/// its commitments (habits on its own subtasks; see `commitments.rs`)
/// whatever end they were given, and takes them out of the board column they were placed in if they are still
/// there. `WorkspaceStore.expireHabits`.
fn expire_habits(
    transaction: &Transaction,
    source_task_id: &str,
    now_ms: i64,
) -> Result<(), CoreError> {
    let now = stored(now_ms);
    let mut statement = transaction.prepare(
        "SELECT id, taskId, placementColumn FROM dailies
         WHERE sourceTaskId = ?1 AND archivedAt IS NULL
           AND (expiryRule = 'source' OR taskId IN (SELECT id FROM tasks WHERE parentTaskId = ?1))",
    )?;
    let habits: Vec<(String, String, Option<String>)> = statement
        .query_map([source_task_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<Result<_, _>>()?;
    for (habit_id, habit_task_id, placement) in habits {
        transaction.execute(
            "UPDATE dailies SET archivedAt = ?1, updatedAt = ?1 WHERE id = ?2",
            params![now, habit_id],
        )?;
        let Some(placement) = placement else { continue };
        let column: Option<String> = transaction
            .query_row(
                "SELECT kanbanColumn FROM task_metadata WHERE taskId = ?1",
                [&habit_task_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        if column.as_deref() == Some(placement.as_str()) {
            transaction.execute(
                "INSERT INTO task_metadata(taskId, tagsJSON, externalLinksJSON, kanbanColumn, updatedAt)
                 VALUES (?1, '[]', '[]', NULL, ?2)
                 ON CONFLICT(taskId) DO UPDATE SET
                   kanbanColumn = NULL, focusRank = NULL, updatedAt = excluded.updatedAt",
                params![habit_task_id, now],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
