//! Writes that turn tasks into lists and lists into tasks, the nested-list
//! flags, and a board's columns. Each keeps the identities and hierarchy of
//! everything it moves, and is one undo step. Replaces
//! `WorkspaceStore+Lists.swift` and its Kotlin copy.

use rusqlite::{OptionalExtension, Transaction, params};

use crate::CoreError;
use crate::lists::new_id;
use crate::tasks::descendant_ids;
use crate::time::stored;

/// A task's list, title, kind, archive stamp and status.
type MovingTask = (String, String, Option<String>, Option<String>, String);

/// Drops a task onto a folder (or the top level), making it a standalone
/// list that keeps its own id as the list's visible root and takes its
/// subtree with it. Returns the new list's id.
/// `WorkspaceStore.moveTaskToFolder`.
pub fn move_task_to_folder(
    transaction: &Transaction,
    id: &str,
    folder_id: Option<&str>,
    now_ms: i64,
) -> Result<String, CoreError> {
    let task: Option<MovingTask> = transaction
        .query_row(
            "SELECT listId, title, itemKind, archivedAt, status FROM tasks WHERE id = ?1",
            [id],
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
    let Some((source_list, title, kind, archived_at, status)) = task else {
        return Err(CoreError::MissingTask { id: id.to_string() });
    };
    let (workspace_id, colour, visible_root): (String, Option<String>, Option<String>) =
        transaction
            .query_row(
                "SELECT workspaceId, colorHex, visibleRootTaskId FROM task_lists WHERE id = ?1",
                [&source_list],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| CoreError::MissingTask { id: id.to_string() })?;
    if let Some(folder) = folder_id {
        let owner: Option<String> = transaction
            .query_row(
                "SELECT workspaceId FROM list_folders WHERE id = ?1",
                [folder],
                |row| row.get(0),
            )
            .optional()?;
        if owner.as_deref() != Some(workspace_id.as_str()) {
            return Err(CoreError::MissingFolder {
                id: folder.to_string(),
            });
        }
    }
    // Extracting a list's transport wrapper would leave the source broken.
    if visible_root.as_deref() == Some(id) {
        return Err(CoreError::InvalidTaskMove);
    }
    let is_list = kind.as_deref() == Some("list");
    let now = stored(now_ms);
    let list_id = new_id();
    let list_order: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(sortOrder), -1) + 1 FROM task_lists WHERE workspaceId = ?1 AND folderId IS ?2",
        params![workspace_id, folder_id],
        |row| row.get(0),
    )?;
    transaction.execute(
        "INSERT INTO task_lists (id, workspaceId, folderId, name, colorHex, sortOrder, isArchived, createdAt,
                                 updatedAt, systemRole, visibleRootTaskId, completedAt)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, NULL, ?9, ?10)",
        params![
            list_id,
            workspace_id,
            folder_id,
            title,
            colour,
            list_order,
            is_list && archived_at.is_some(),
            now,
            id,
            (is_list && status != "open").then(|| now.clone())
        ],
    )?;
    let mut moving = descendant_ids(transaction, id)?;
    moving.push(id.to_string());
    moving.sort();
    {
        let mut statement =
            transaction.prepare("UPDATE tasks SET listId = ?1, updatedAt = ?2 WHERE id = ?3")?;
        for task_id in &moving {
            statement.execute(params![list_id, now, task_id])?;
        }
    }
    let task_order: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(sortOrder), -1) + 1 FROM tasks WHERE listId = ?1 AND parentTaskId IS NULL",
        [&list_id],
        |row| row.get(0),
    )?;
    transaction.execute(
        "UPDATE tasks SET parentTaskId = NULL, sortOrder = ?1, itemKind = 'list', isPromoted = NULL,
                          archivedAt = NULL, status = 'open', updatedAt = ?2
         WHERE id = ?3",
        params![task_order, now, id],
    )?;
    Ok(list_id)
}

/// Turns a standalone list into one task in the Inbox, keeping every task
/// in it below that task. Returns the task's id.
/// `WorkspaceStore.convertListToTask`.
pub fn convert_list_to_task(
    transaction: &Transaction,
    id: &str,
    now_ms: i64,
) -> Result<String, CoreError> {
    let list = list_row(transaction, id)?;
    if list.system_role.is_some() {
        return Err(CoreError::SystemListIsPermanent);
    }
    crate::sync::spaces::refuse_if_shared(transaction, id)?;
    let inbox: String = transaction
        .query_row(
            "SELECT id FROM task_lists WHERE workspaceId = ?1 AND systemRole = 'inbox'",
            [&list.workspace_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::MissingList { id: "inbox".into() })?;
    relocate_list(transaction, &list, &inbox, None, "task", now_ms)
}

/// Drags a standalone list into another list, under `parent_task_id` or the
/// destination's visible root, as a nested list. Returns its task's id.
/// `WorkspaceStore.nestList`.
pub fn nest_list(
    transaction: &Transaction,
    id: &str,
    into_list_id: &str,
    parent_task_id: Option<&str>,
    now_ms: i64,
) -> Result<String, CoreError> {
    let list = list_row(transaction, id)?;
    let destination = list_row(transaction, into_list_id)?;
    if list.system_role.is_some() {
        return Err(CoreError::SystemListIsPermanent);
    }
    crate::sync::spaces::refuse_if_shared(transaction, id)?;
    if id == into_list_id || list.workspace_id != destination.workspace_id {
        return Err(CoreError::InvalidTaskMove);
    }
    let parent = match parent_task_id {
        Some(parent) => Some(parent.to_string()),
        // Only a transport wrapper that still is the visible root.
        None => match &destination.visible_root {
            Some(root) => {
                let roots: Vec<String> = {
                    let mut statement = transaction.prepare(
                        "SELECT id FROM tasks WHERE listId = ?1 AND parentTaskId IS NULL",
                    )?;
                    statement
                        .query_map([into_list_id], |row| row.get(0))?
                        .collect::<Result<_, _>>()?
                };
                (roots.len() == 1 && roots[0] == *root).then(|| root.clone())
            }
            None => None,
        },
    };
    if let Some(parent) = &parent {
        let found: Option<(String, Option<String>)> = transaction
            .query_row(
                "SELECT listId, itemKind FROM tasks WHERE id = ?1",
                [parent],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let fits = found.is_some_and(|(list_id, kind)| {
            list_id == into_list_id
                && (kind.as_deref() == Some("list")
                    || destination.visible_root.as_deref() == Some(parent))
        });
        if !fits {
            return Err(CoreError::InvalidTaskMove);
        }
    }
    relocate_list(
        transaction,
        &list,
        into_list_id,
        parent.as_deref(),
        "list",
        now_ms,
    )
}

struct ListRow {
    id: String,
    workspace_id: String,
    name: String,
    system_role: Option<String>,
    visible_root: Option<String>,
    completed_at: Option<String>,
}

fn list_row(transaction: &Transaction, id: &str) -> Result<ListRow, CoreError> {
    transaction
        .query_row(
            "SELECT id, workspaceId, name, systemRole, visibleRootTaskId, completedAt FROM task_lists WHERE id = ?1",
            [id],
            |row| {
                Ok(ListRow {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    name: row.get(2)?,
                    system_role: row.get(3)?,
                    visible_root: row.get(4)?,
                    completed_at: row.get(5)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::MissingList { id: id.to_string() })
}

/// Moves every task in `list` into `destination` under one root task (the
/// list's wrapper if it has one, a new task named for it otherwise), then
/// deletes the emptied list. `WorkspaceStore.relocateList`.
fn relocate_list(
    transaction: &Transaction,
    list: &ListRow,
    destination: &str,
    parent_task_id: Option<&str>,
    kind: &str,
    now_ms: i64,
) -> Result<String, CoreError> {
    let now = stored(now_ms);
    let tasks: Vec<(String, Option<String>)> = {
        let mut statement =
            transaction.prepare("SELECT id, parentTaskId FROM tasks WHERE listId = ?1")?;
        statement
            .query_map([&list.id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?
    };
    let wrapper = list
        .visible_root
        .as_ref()
        .filter(|root| {
            tasks
                .iter()
                .any(|(id, parent)| id == *root && parent.is_none())
        })
        .cloned();
    let sort_order: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(sortOrder), -1) + 1 FROM tasks WHERE listId = ?1 AND parentTaskId IS ?2",
        params![destination, parent_task_id],
        |row| row.get(0),
    )?;
    let status = if list.completed_at.is_none() {
        "open"
    } else {
        "completed"
    };
    let root = match &wrapper {
        Some(root) => root.clone(),
        None => {
            let id = new_id();
            transaction.execute(
                "INSERT INTO tasks (id, listId, parentTaskId, title, notes, status, sortOrder, dueAt, estimateSeconds,
                                    sourceSystem, sourceId, itemKind, isPromoted, archivedAt, completedAt, createdAt, updatedAt)
                 VALUES (?1, ?2, ?3, ?4, '', ?5, ?6, NULL, NULL, NULL, NULL, ?7, NULL, NULL, ?8, ?9, ?9)",
                params![id, destination, parent_task_id, list.name, status, sort_order, kind, list.completed_at, now],
            )?;
            id
        }
    };
    {
        let mut statement = transaction.prepare(
            "UPDATE tasks SET listId = ?1, parentTaskId = COALESCE(parentTaskId, ?2), updatedAt = ?3 WHERE id = ?4",
        )?;
        for (id, _) in tasks.iter().filter(|(id, _)| *id != root) {
            statement.execute(params![destination, root, now, id])?;
        }
    }
    if wrapper.is_some() {
        transaction.execute(
            "UPDATE tasks SET listId = ?1, parentTaskId = ?2, title = ?3, itemKind = ?4, isPromoted = NULL,
                              archivedAt = NULL, status = ?5, completedAt = ?6, sortOrder = ?7, updatedAt = ?8
             WHERE id = ?9",
            params![destination, parent_task_id, list.name, kind, status, list.completed_at, sort_order, now, root],
        )?;
    }
    transaction.execute("DELETE FROM task_lists WHERE id = ?1", [&list.id])?;
    Ok(root)
}

/// Makes an item a task or a nested list. A list's transport wrapper, or a
/// task a focus session is on, cannot change. `WorkspaceStore.setItemKind`.
pub fn set_item_kind(
    transaction: &Transaction,
    id: &str,
    kind: &str,
    now_ms: i64,
) -> Result<(), CoreError> {
    let current: Option<Option<String>> = transaction
        .query_row("SELECT itemKind FROM tasks WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .optional()?;
    let Some(current) = current else {
        return Err(CoreError::MissingTask { id: id.to_string() });
    };
    if current.as_deref().unwrap_or("task") == kind {
        return Ok(());
    }
    let busy: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM task_lists WHERE visibleRootTaskId = ?1)
             OR EXISTS(SELECT 1 FROM focus_sessions WHERE activeTaskId = ?1 AND phase <> 'finished')",
        [id],
        |row| row.get(0),
    )?;
    if busy {
        return Err(CoreError::InvalidTaskMove);
    }
    let sql = if kind == "task" {
        "UPDATE tasks SET itemKind = ?1, isPromoted = NULL, archivedAt = NULL, updatedAt = ?2 WHERE id = ?3"
    } else {
        "UPDATE tasks SET itemKind = ?1, updatedAt = ?2 WHERE id = ?3"
    };
    transaction.execute(sql, params![kind, stored(now_ms), id])?;
    Ok(())
}

/// Pins a nested list to the sidebar, or unpins it.
/// `WorkspaceStore.setNestedListPromoted`.
pub fn set_nested_list_promoted(
    transaction: &Transaction,
    id: &str,
    promoted: bool,
    now_ms: i64,
) -> Result<(), CoreError> {
    let current: Option<bool> = nested_list(transaction, id, "COALESCE(isPromoted, 0)")?;
    if current == Some(promoted) {
        return Ok(());
    }
    transaction.execute(
        "UPDATE tasks SET isPromoted = ?1, updatedAt = ?2 WHERE id = ?3",
        params![promoted, stored(now_ms), id],
    )?;
    Ok(())
}

/// Archives or restores a nested list. `WorkspaceStore.setNestedListArchived`.
pub fn set_nested_list_archived(
    transaction: &Transaction,
    id: &str,
    archived: bool,
    now_ms: i64,
) -> Result<(), CoreError> {
    let current: Option<bool> = nested_list(transaction, id, "archivedAt IS NOT NULL")?;
    if current == Some(archived) {
        return Ok(());
    }
    let now = stored(now_ms);
    transaction.execute(
        "UPDATE tasks SET archivedAt = ?1, updatedAt = ?2 WHERE id = ?3",
        params![archived.then(|| now.clone()), now, id],
    )?;
    Ok(())
}

/// A nested list's flag, or a missing list if `id` is not a nested list.
fn nested_list(transaction: &Transaction, id: &str, flag: &str) -> Result<Option<bool>, CoreError> {
    let found: Option<bool> = transaction
        .query_row(
            &format!("SELECT {flag} FROM tasks WHERE id = ?1 AND itemKind = 'list'"),
            [id],
            |row| row.get(0),
        )
        .optional()?;
    match found {
        Some(flag) => Ok(Some(flag)),
        None => Err(CoreError::MissingList { id: id.to_string() }),
    }
}

/// Completes or reopens a standalone list; the Inbox cannot be completed.
/// `WorkspaceStore.setListCompleted`.
pub fn set_list_completed(
    transaction: &Transaction,
    id: &str,
    completed: bool,
    now_ms: i64,
) -> Result<(), CoreError> {
    let list = list_row(transaction, id)?;
    if list.system_role.is_some() {
        return Err(CoreError::SystemListIsPermanent);
    }
    if list.completed_at.is_some() == completed {
        return Ok(());
    }
    let now = stored(now_ms);
    transaction.execute(
        "UPDATE task_lists SET completedAt = ?1, updatedAt = ?2 WHERE id = ?3",
        params![completed.then(|| now.clone()), now, id],
    )?;
    if completed {
        crate::lists::drop_priority(transaction, id, now_ms)?;
    }
    Ok(())
}

pub use crate::board_columns::{BoardColumn, set_board_columns};

#[cfg(test)]
mod tests;
