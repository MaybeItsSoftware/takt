//! Who did what in a shared list (docs/shared-lists-plan.md): who added a
//! task, who finished it, and who it is assigned to. Each is an account id
//! from the list's members, as `sync_spaces` last heard them; a task in a
//! list of your own has none of them.
//!
//! The stamps are written where the core creates and closes tasks, so every
//! client gets them. Assignment changes what is offered: a shared task
//! assigned to someone else is not yours to do, so Today, next up and the
//! focus ladder leave it out (`focus::candidates`). One assigned to you, or
//! to nobody, is offered as before.

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::CoreError;
use crate::sync::spaces::{self, SharedListMember};

/// This account's id in the shared list `list_id`, when it is one.
pub fn you_in_list(connection: &Connection, list_id: &str) -> Result<Option<String>, CoreError> {
    Ok(spaces::shared_list(connection, list_id)?
        .and_then(|shared| shared.your_account_id().map(str::to_string)))
}

fn list_of(connection: &Connection, task_id: &str) -> Result<Option<String>, CoreError> {
    Ok(connection
        .query_row("SELECT listId FROM tasks WHERE id = ?1", [task_id], |row| {
            row.get(0)
        })
        .optional()?)
}

/// Marks a task just made in a shared list as added by this account.
pub fn stamp_created(transaction: &Transaction, task_id: &str) -> Result<(), CoreError> {
    let Some(list) = list_of(transaction, task_id)? else {
        return Ok(());
    };
    if let Some(you) = you_in_list(transaction, &list)? {
        transaction.execute(
            "UPDATE tasks SET createdBy = ?1 WHERE id = ?2 AND createdBy IS NULL",
            params![you, task_id],
        )?;
    }
    Ok(())
}

/// After a task's status changed: a task (and the subtasks closed with it,
/// stamped `closed_at`) completed in a shared list is marked as finished by
/// this account; reopened or cancelled, the mark goes.
pub fn stamp_status(
    transaction: &Transaction,
    task_id: &str,
    status: &str,
    closed_at: &str,
) -> Result<(), CoreError> {
    let Some(list) = list_of(transaction, task_id)? else {
        return Ok(());
    };
    if status != "completed" {
        transaction.execute(
            "UPDATE tasks SET completedBy = NULL WHERE id = ?1 AND completedBy IS NOT NULL",
            [task_id],
        )?;
        return Ok(());
    }
    let Some(you) = you_in_list(transaction, &list)? else {
        return Ok(());
    };
    transaction.execute(
        "WITH RECURSIVE subtree(id) AS (
           SELECT ?2 UNION SELECT t.id FROM tasks t JOIN subtree ON t.parentTaskId = subtree.id
         )
         UPDATE tasks SET completedBy = ?1
         WHERE id IN (SELECT id FROM subtree) AND status = 'completed' AND completedBy IS NULL
           AND (id = ?2 OR completedAt = ?3)",
        params![you, task_id, closed_at],
    )?;
    Ok(())
}

/// Assigns a task in a shared list to one of its members, or to nobody.
/// Refused for a task in a list of your own and for someone not in the list.
pub fn assign_task(
    transaction: &Transaction,
    task_id: &str,
    assignee: Option<&str>,
    now_ms: i64,
) -> Result<(), CoreError> {
    let list = list_of(transaction, task_id)?.ok_or_else(|| CoreError::MissingTask {
        id: task_id.to_string(),
    })?;
    let shared = spaces::shared_list(transaction, &list)?.ok_or_else(|| CoreError::File {
        detail: "Only a task in a shared list can be assigned.".into(),
    })?;
    if let Some(assignee) = assignee
        && !shared.members.iter().any(|m| m.account_id == assignee)
    {
        return Err(CoreError::File {
            detail: "They aren't in this list.".into(),
        });
    }
    let now = crate::time::stored(now_ms);
    transaction.execute(
        "INSERT INTO task_metadata (taskId, assigneeAccountId, updatedAt) VALUES (?1, ?2, ?3)
         ON CONFLICT(taskId) DO UPDATE SET assigneeAccountId = excluded.assigneeAccountId,
           updatedAt = excluded.updatedAt
         WHERE assigneeAccountId IS NOT excluded.assigneeAccountId",
        params![task_id, assignee, now],
    )?;
    Ok(())
}

/// The ids of open tasks assigned to someone other than this account, which
/// are not offered.
pub fn assigned_elsewhere(
    connection: &Connection,
) -> Result<std::collections::HashSet<String>, CoreError> {
    let mut statement = connection.prepare(
        "SELECT m.taskId FROM task_metadata m
         JOIN tasks t ON t.id = m.taskId
         JOIN sync_spaces s ON s.listId = t.listId
         WHERE m.assigneeAccountId IS NOT NULL
           AND m.assigneeAccountId IS NOT (
             SELECT json_extract(value, '$.accountId') FROM json_each(s.membersJSON)
             WHERE json_extract(value, '$.you') LIMIT 1)",
    )?;
    let ids = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// A task's part in a shared list, for a client's task detail.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TaskSharing {
    pub list_id: String,
    pub members: Vec<SharedListMember>,
    pub created_by: Option<SharedListMember>,
    pub completed_by: Option<SharedListMember>,
    pub assignee: Option<SharedListMember>,
}

/// Who added, finished and holds `task_id`, when it is in a shared list.
/// An id no longer among the members reads as a member with no email.
pub fn task_sharing(
    connection: &Connection,
    task_id: &str,
) -> Result<Option<TaskSharing>, CoreError> {
    type Stamps = (String, Option<String>, Option<String>, Option<String>);
    let row: Option<Stamps> = connection
        .query_row(
            "SELECT t.listId, t.createdBy, t.completedBy, m.assigneeAccountId
             FROM tasks t LEFT JOIN task_metadata m ON m.taskId = t.id WHERE t.id = ?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((list_id, created, completed, assignee)) = row else {
        return Ok(None);
    };
    let Some(shared) = spaces::shared_list(connection, &list_id)? else {
        return Ok(None);
    };
    let member = |id: Option<String>| {
        id.map(|id| {
            shared
                .members
                .iter()
                .find(|m| m.account_id == id)
                .cloned()
                .unwrap_or(SharedListMember {
                    account_id: id,
                    email: None,
                    role: "member".into(),
                    you: false,
                })
        })
    };
    Ok(Some(TaskSharing {
        created_by: member(created),
        completed_by: member(completed),
        assignee: member(assignee),
        members: shared.members.clone(),
        list_id,
    }))
}

/// "you", or the member's email, or "someone": how a member is named in a
/// sentence.
#[uniffi::export]
pub fn shared_member_name(member: SharedListMember) -> String {
    if member.you {
        "you".into()
    } else {
        member.email.unwrap_or_else(|| "someone".into())
    }
}

#[cfg(test)]
mod tests;
