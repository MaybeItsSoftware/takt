//! Shared lists, on the device (docs/shared-lists-plan.md).
//!
//! The server keeps a shared list's rows in a space of their own, which its
//! members read beside their own space. This module decides which space a
//! local row belongs in, splits a shared row into what everyone sees and
//! what stays one person's, and keeps `sync_spaces` and `sync_row_spaces`
//! (v37) up to date.
//!
//! **What goes where.** A row of one of [`SHARED_TABLES`] belongs to a shared
//! list's space when it is the list, a task in it, that task's metadata, a
//! dependency between two of its tasks, or its agent instructions. Its
//! [personal columns](personal_columns) — where the list sits in your
//! sidebar, your board column and matrix placement for a task, your
//! conditions for it — never go there. They go to your own space as a
//! companion row, the table name with [`PERSONAL_SUFFIX`], so the other
//! members neither see nor overwrite them.
//!
//! **Moving.** A task moved into a shared list is sent to its space whole;
//! one moved out is deleted there and sent to your own space whole. A row
//! whose space changes without an edit of its own (a task's metadata, when
//! the task moves) is found by comparing where it belongs with where it was
//! last sent ([`moved_rows`]).
//!
//! **Tombstones.** A delete from a shared space is applied last in a pull,
//! and only to a row that still belongs to that space, so a task moved out
//! on another device is not deleted when its old space's tombstone arrives
//! beside its new home. A delete applied that way is passed on to your own
//! space too, so a copy left there from before the task was shared can't
//! come back on a new device.

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use takt_sync_rules::wire::WireSpace;

use crate::CoreError;

/// The tables a shared list's rows come from, parents first.
pub const SHARED_TABLES: [&str; 5] = [
    "task_lists",
    "tasks",
    "task_metadata",
    "task_dependencies",
    "agent_instructions",
];

/// Marks a companion row: a shared row's personal columns, kept in the
/// account's own space under `<table>#personal`. A client that predates
/// sharing skips a table it doesn't know.
pub const PERSONAL_SUFFIX: &str = "#personal";

/// The columns of a shared row that are one person's own.
pub fn personal_columns(table: &str) -> &'static [&'static str] {
    match table {
        // Where it sits in your sidebar, the subtree you're focused on, how
        // much it matters to you, and the conditions its tasks need, which
        // name conditions of your own.
        "task_lists" => &[
            "workspaceId",
            "folderId",
            "sortOrder",
            "visibleRootTaskId",
            "priorityRank",
            "requirementGroupsJSON",
        ],
        // How you plan to work on it: the planning JSON names conditions,
        // which are each person's own.
        "task_metadata" => &[
            "matrixUrgency",
            "matrixImportance",
            "matrixRank",
            "kanbanColumn",
            "focusRank",
            "planningJSON",
            "agentContext",
        ],
        _ => &[],
    }
}

pub fn is_personal_column(table: &str, column: &str) -> bool {
    personal_columns(table).contains(&column)
}

/// The base table of a companion row's table name.
pub fn companion_base(table: &str) -> Option<&str> {
    table
        .strip_suffix(PERSONAL_SUFFIX)
        .filter(|base| !personal_columns(base).is_empty())
}

pub fn companion_table(table: &str) -> Option<String> {
    (!personal_columns(table).is_empty()).then(|| format!("{table}{PERSONAL_SUFFIX}"))
}

/// Whether this device knows of any shared list or still has rows it sent
/// to one. Without either, nothing here has anything to do, and the push
/// and pull take their old paths.
pub fn active(connection: &Connection) -> Result<bool, CoreError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sync_spaces) OR EXISTS(SELECT 1 FROM sync_row_spaces)",
        [],
        |row| row.get(0),
    )?)
}

/// The shared space the live row `table`/`id` belongs in, or nothing for the
/// account's own (or when there is no such row).
pub fn current_space(
    connection: &Connection,
    table: &str,
    id: &str,
) -> Result<Option<String>, CoreError> {
    let sql = match table {
        "task_lists" => "SELECT spaceId FROM sync_spaces WHERE listId = ?1",
        "tasks" => {
            "SELECT s.spaceId FROM tasks t JOIN sync_spaces s ON s.listId = t.listId WHERE t.id = ?1"
        }
        "task_metadata" => {
            "SELECT s.spaceId FROM task_metadata m JOIN tasks t ON t.id = m.taskId
             JOIN sync_spaces s ON s.listId = t.listId WHERE m.taskId = ?1"
        }
        "task_dependencies" => {
            "SELECT s.spaceId FROM task_dependencies d
             JOIN tasks a ON a.id = d.taskId JOIN tasks b ON b.id = d.blockedByTaskId
             JOIN sync_spaces s ON s.listId = a.listId
             WHERE d.id = ?1 AND b.listId = a.listId"
        }
        "agent_instructions" => "SELECT spaceId FROM sync_spaces WHERE 'list:' || listId = ?1",
        _ => return Ok(None),
    };
    Ok(connection
        .query_row(sql, [id], |row| row.get(0))
        .optional()?)
}

/// The space `listId` names, when that list is shared.
pub fn space_of_list(connection: &Connection, list_id: &str) -> Result<Option<String>, CoreError> {
    Ok(connection
        .query_row(
            "SELECT spaceId FROM sync_spaces WHERE listId = ?1",
            [list_id],
            |row| row.get(0),
        )
        .optional()?)
}

/// The shared space the row was last sent to or received from.
pub fn recorded_space(
    connection: &Connection,
    table: &str,
    id: &str,
) -> Result<Option<String>, CoreError> {
    Ok(connection
        .query_row(
            "SELECT spaceId FROM sync_row_spaces WHERE tableName = ?1 AND rowId = ?2",
            params![table, id],
            |row| row.get(0),
        )
        .optional()?)
}

pub fn record(
    connection: &Connection,
    table: &str,
    id: &str,
    space: Option<&str>,
) -> Result<(), CoreError> {
    match space {
        Some(space) => connection.execute(
            "INSERT INTO sync_row_spaces (tableName, rowId, spaceId) VALUES (?1, ?2, ?3)
             ON CONFLICT(tableName, rowId) DO UPDATE SET spaceId = excluded.spaceId",
            params![table, id, space],
        )?,
        None => connection.execute(
            "DELETE FROM sync_row_spaces WHERE tableName = ?1 AND rowId = ?2",
            params![table, id],
        )?,
    };
    Ok(())
}

/// The shared spaces this device knows, by id.
pub fn known(connection: &Connection) -> Result<HashSet<String>, CoreError> {
    let mut statement = connection.prepare("SELECT spaceId FROM sync_spaces")?;
    let ids = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Live rows whose space differs from the one they were last sent to, up to
/// `limit`, parents first: rows of a list that was just shared, a task's
/// metadata when the task moved, a list's rows once sharing stopped.
pub fn moved_rows(connection: &Connection, limit: u32) -> Result<Vec<(String, String)>, CoreError> {
    if !active(connection)? || limit == 0 {
        return Ok(Vec::new());
    }
    // Each arm is "where it belongs" against "where it was sent", for every
    // live row of one table.
    let arms = [
        (
            "task_lists",
            "SELECT l.id AS id, s.spaceId AS space FROM task_lists l LEFT JOIN sync_spaces s ON s.listId = l.id",
        ),
        (
            "tasks",
            "SELECT t.id AS id, s.spaceId AS space FROM tasks t LEFT JOIN sync_spaces s ON s.listId = t.listId",
        ),
        (
            "task_metadata",
            "SELECT m.taskId AS id, s.spaceId AS space FROM task_metadata m JOIN tasks t ON t.id = m.taskId
             LEFT JOIN sync_spaces s ON s.listId = t.listId",
        ),
        (
            "task_dependencies",
            "SELECT d.id AS id, CASE WHEN b.listId = a.listId THEN s.spaceId END AS space
             FROM task_dependencies d
             JOIN tasks a ON a.id = d.taskId JOIN tasks b ON b.id = d.blockedByTaskId
             LEFT JOIN sync_spaces s ON s.listId = a.listId",
        ),
        (
            "agent_instructions",
            "SELECT i.scope AS id, s.spaceId AS space FROM agent_instructions i
             LEFT JOIN sync_spaces s ON 'list:' || s.listId = i.scope",
        ),
    ];
    let mut moved = Vec::new();
    for (table, belongs) in arms {
        let remaining = limit as usize - moved.len();
        if remaining == 0 {
            break;
        }
        let sql = format!(
            "SELECT here.id FROM ({belongs}) AS here
             LEFT JOIN sync_row_spaces r ON r.tableName = ?1 AND r.rowId = here.id
             WHERE here.space IS NOT r.spaceId
               AND NOT (r.spaceId = '' AND here.space IS NULL)
             LIMIT ?2"
        );
        let mut statement = connection.prepare(&sql)?;
        let ids: Vec<String> = statement
            .query_map(params![table, remaining as i64], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        moved.extend(ids.into_iter().map(|id| (table.to_string(), id)));
    }
    Ok(moved)
}

/// The list a row of a list's space hangs off, from the row's own values
/// as they arrive. Used to tell a task's new home from its old one.
pub fn incoming_list_id<'a>(
    table: &str,
    values: &'a HashMap<String, super::SyncValue>,
) -> Option<&'a str> {
    match table {
        "tasks" => values.get("listId").and_then(super::SyncValue::text),
        _ => None,
    }
}

/// A shared list as the device knows it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SharedList {
    pub space_id: String,
    pub list_id: String,
    /// The list's name here, or the name it was shared under while its row
    /// hasn't arrived.
    pub name: String,
    /// "owner" or "member": this account's part in it.
    pub role: String,
    pub members: Vec<SharedListMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SharedListMember {
    pub account_id: String,
    pub email: Option<String>,
    pub role: String,
    /// This account.
    pub you: bool,
}

impl SharedList {
    pub fn is_owner(&self) -> bool {
        self.role == "owner"
    }

    /// This account's id, as the list's members name it.
    pub fn your_account_id(&self) -> Option<&str> {
        self.members
            .iter()
            .find(|member| member.you)
            .map(|member| member.account_id.as_str())
    }
}

fn members_json(space: &WireSpace) -> String {
    serde_json::to_string(&space.members).unwrap_or_else(|_| "[]".into())
}

fn members_from_json(json: &str) -> Vec<SharedListMember> {
    serde_json::from_str::<Vec<takt_sync_rules::wire::WireMember>>(json)
        .unwrap_or_default()
        .into_iter()
        .map(|member| SharedListMember {
            account_id: member.account_id,
            email: member.email,
            role: member.role,
            you: member.you,
        })
        .collect()
}

/// Every shared list this device knows, by name.
pub fn shared_lists(connection: &Connection) -> Result<Vec<SharedList>, CoreError> {
    let mut statement = connection.prepare(
        "SELECT s.spaceId, s.listId, COALESCE(l.name, s.name), s.role, s.membersJSON
         FROM sync_spaces s LEFT JOIN task_lists l ON l.id = s.listId
         ORDER BY COALESCE(l.name, s.name) COLLATE NOCASE, s.spaceId",
    )?;
    let lists = statement
        .query_map([], |row| {
            Ok(SharedList {
                space_id: row.get(0)?,
                list_id: row.get(1)?,
                name: row.get(2)?,
                role: row.get(3)?,
                members: members_from_json(&row.get::<_, String>(4)?),
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(lists)
}

/// The shared list `list_id` is, when it is one.
pub fn shared_list(
    connection: &Connection,
    list_id: &str,
) -> Result<Option<SharedList>, CoreError> {
    Ok(shared_lists(connection)?
        .into_iter()
        .find(|list| list.list_id == list_id))
}

/// Records a space the server described: the answer to sharing a list or
/// accepting an invite. Rows already here are left to be sent, which is what
/// sharing wants: the list's rows go up to its new space on the next push.
pub fn record_space(transaction: &Transaction, space: &WireSpace) -> Result<(), CoreError> {
    transaction.execute(
        "INSERT INTO sync_spaces (spaceId, listId, role, name, membersJSON) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(spaceId) DO UPDATE SET listId = excluded.listId, role = excluded.role,
           name = excluded.name, membersJSON = excluded.membersJSON",
        params![
            space.id,
            space.list_id,
            space.role,
            space.name,
            members_json(space)
        ],
    )?;
    Ok(())
}

/// Brings `sync_spaces` to the server's word, from a page of changes.
///
/// A space new to this device whose list is already here (the owner's other
/// devices, when one of them shares it) has its rows marked as sent, so they
/// aren't all pushed again over whatever the members have changed since; the
/// pull brings the space's own copy. A space that has gone either takes its
/// rows with it (a list you were in) or leaves them as a private list again
/// (a list you owned and stopped sharing). Returns whether a list went.
pub fn store_spaces(transaction: &Transaction, spaces: &[WireSpace]) -> Result<bool, CoreError> {
    let had = known(transaction)?;
    let now: HashSet<&str> = spaces.iter().map(|space| space.id.as_str()).collect();
    for space in spaces {
        let new = !had.contains(&space.id);
        record_space(transaction, space)?;
        if new {
            mark_sent(transaction, &space.id)?;
        }
    }
    let mut removed = false;
    for gone in had.iter().filter(|id| !now.contains(id.as_str())) {
        let role: Option<String> = transaction
            .query_row(
                "SELECT role FROM sync_spaces WHERE spaceId = ?1",
                [gone],
                |row| row.get(0),
            )
            .optional()?;
        forget(transaction, gone, role.as_deref() != Some("owner"), false)?;
        removed = true;
    }
    Ok(removed)
}

/// Marks every row that belongs in `space` as sent there.
fn mark_sent(transaction: &Transaction, space: &str) -> Result<(), CoreError> {
    for table in SHARED_TABLES {
        let ids: Vec<String> = {
            let sql = match table {
                "task_lists" => "SELECT listId FROM sync_spaces WHERE spaceId = ?1",
                "tasks" => {
                    "SELECT t.id FROM tasks t JOIN sync_spaces s ON s.listId = t.listId WHERE s.spaceId = ?1"
                }
                "task_metadata" => {
                    "SELECT m.taskId FROM task_metadata m JOIN tasks t ON t.id = m.taskId
                     JOIN sync_spaces s ON s.listId = t.listId WHERE s.spaceId = ?1"
                }
                "task_dependencies" => {
                    "SELECT d.id FROM task_dependencies d JOIN tasks a ON a.id = d.taskId
                     JOIN tasks b ON b.id = d.blockedByTaskId JOIN sync_spaces s ON s.listId = a.listId
                     WHERE s.spaceId = ?1 AND b.listId = a.listId"
                }
                _ => "SELECT 'list:' || listId FROM sync_spaces WHERE spaceId = ?1",
            };
            let mut statement = transaction.prepare(sql)?;
            statement
                .query_map([space], |row| row.get(0))?
                .collect::<Result<_, _>>()?
        };
        for id in ids {
            record(transaction, table, &id, Some(space))?;
        }
    }
    Ok(())
}

/// Forgets a shared list. With `drop_rows` (a list you were in) its rows go
/// from this device without a word to the server. Without it (a list you
/// owned) they stay, as a private list. With `push_back` as well, they are
/// sent to your own space whole, which the device that stopped sharing
/// does, since your own space may never have seen the tasks added while the
/// list was shared; the owner's other devices only forget.
pub fn forget(
    transaction: &Transaction,
    space: &str,
    drop_rows: bool,
    push_back: bool,
) -> Result<(), CoreError> {
    if drop_rows {
        let list: Option<String> = transaction
            .query_row(
                "SELECT listId FROM sync_spaces WHERE spaceId = ?1",
                [space],
                |row| row.get(0),
            )
            .optional()?;
        let was_applying: bool = transaction.query_row(
            "SELECT applying FROM sync_control WHERE id = 0",
            [],
            |row| row.get(0),
        )?;
        transaction.execute("UPDATE sync_control SET applying = 1 WHERE id = 0", [])?;
        let result = (|| -> Result<(), CoreError> {
            // The list's tasks go with it, by cascade; its instructions by name.
            if let Some(list) = &list {
                transaction.execute(
                    "DELETE FROM agent_instructions WHERE scope = 'list:' || ?1",
                    [list],
                )?;
                transaction.execute("DELETE FROM task_lists WHERE id = ?1", [list])?;
            }
            // Anything still recorded there that moved in from elsewhere.
            transaction.execute(
                "DELETE FROM tasks WHERE id IN
                   (SELECT rowId FROM sync_row_spaces WHERE tableName = 'tasks' AND spaceId = ?1)",
                [space],
            )?;
            transaction.execute(
                "DELETE FROM sync_outbox WHERE EXISTS (SELECT 1 FROM sync_row_spaces r
                   WHERE r.tableName = sync_outbox.tableName AND r.rowId = sync_outbox.rowId
                     AND r.spaceId = ?1)",
                [space],
            )?;
            Ok(())
        })();
        transaction.execute(
            "UPDATE sync_control SET applying = ?1 WHERE id = 0",
            [was_applying],
        )?;
        result?;
    }
    if !push_back {
        transaction.execute("DELETE FROM sync_row_spaces WHERE spaceId = ?1", [space])?;
    }
    transaction.execute("DELETE FROM sync_spaces WHERE spaceId = ?1", [space])?;
    Ok(())
}

/// The plan a push in flight made: which space each of its rows now lives
/// in, recorded once the server has it.
pub fn plan(
    connection: &Connection,
    assignments: &[(String, String, Option<String>)],
) -> Result<(), CoreError> {
    connection.execute("DELETE FROM sync_push_plan", [])?;
    let mut statement = connection.prepare(
        "INSERT OR REPLACE INTO sync_push_plan (tableName, rowId, spaceId) VALUES (?1, ?2, ?3)",
    )?;
    for (table, id, space) in assignments {
        statement.execute(params![table, id, space])?;
    }
    Ok(())
}

/// The server has the push: what it planned becomes what was sent.
pub fn carry_out_plan(transaction: &Transaction) -> Result<(), CoreError> {
    transaction.execute_batch(
        "DELETE FROM sync_row_spaces WHERE (tableName, rowId) IN
           (SELECT tableName, rowId FROM sync_push_plan WHERE spaceId IS NULL);
         INSERT INTO sync_row_spaces (tableName, rowId, spaceId)
           SELECT tableName, rowId, spaceId FROM sync_push_plan WHERE spaceId IS NOT NULL
           ON CONFLICT(tableName, rowId) DO UPDATE SET spaceId = excluded.spaceId;
         DELETE FROM sync_push_plan;",
    )?;
    Ok(())
}

/// Refuses to delete, convert or nest a shared list. A member leaves it
/// instead; its owner stops sharing it first, so nobody is left in a space
/// whose list has gone.
pub fn refuse_if_shared(connection: &Connection, list_id: &str) -> Result<(), CoreError> {
    let role: Option<String> = connection
        .query_row(
            "SELECT role FROM sync_spaces WHERE listId = ?1",
            [list_id],
            |row| row.get(0),
        )
        .optional()?;
    match role.as_deref() {
        None => Ok(()),
        Some("owner") => Err(CoreError::File {
            detail: "This list is shared. Stop sharing it first.".into(),
        }),
        Some(_) => Err(CoreError::File {
            detail: "This list belongs to someone else. You can leave it instead.".into(),
        }),
    }
}

/// Whether `list_id` can be shared from here: a list of your own that isn't
/// the Inbox. Returns its name.
pub fn shareable_name(connection: &Connection, list_id: &str) -> Result<String, CoreError> {
    let (name, system_role): (String, Option<String>) = connection
        .query_row(
            "SELECT name, systemRole FROM task_lists WHERE id = ?1",
            [list_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::MissingList {
            id: list_id.to_string(),
        })?;
    if system_role.is_some() {
        return Err(CoreError::File {
            detail: "The Inbox can't be shared. Share a list of its own instead.".into(),
        });
    }
    if let Some(role) = connection
        .query_row(
            "SELECT role FROM sync_spaces WHERE listId = ?1",
            [list_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        && role != "owner"
    {
        return Err(CoreError::File {
            detail: "Only the list's owner can invite people to it.".into(),
        });
    }
    Ok(name)
}

// What the apps call around the sharing endpoints (docs/sync.md, "Shared
// lists"). They send the requests; the core makes and reads the bodies.

/// `POST /v1/spaces`'s body, sharing `list_id` under `name`.
#[uniffi::export]
pub fn sync_share_list_body(list_id: String, name: String) -> String {
    serde_json::json!({ "listId": list_id, "name": name }).to_string()
}

/// An invite the server made.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SyncInvite {
    pub code: String,
    pub space_id: String,
    pub expires_at: String,
}

/// Reads `POST /v1/spaces/<id>/invites`'s answer.
#[uniffi::export]
pub fn sync_decode_invite(body: String) -> Result<SyncInvite, CoreError> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Invite {
        code: String,
        space_id: String,
        expires_at: String,
    }
    let invite: Invite = serde_json::from_str(&body).map_err(|error| CoreError::File {
        detail: format!("The sync server's invite couldn't be read: {error}"),
    })?;
    Ok(SyncInvite {
        code: invite.code,
        space_id: invite.space_id,
        expires_at: invite.expires_at,
    })
}

/// What an invite is for, before joining.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SyncInvitePreview {
    pub space_id: String,
    pub name: String,
    pub owner_email: Option<String>,
    pub expires_at: String,
    /// This account is in the list already.
    pub joined: bool,
}

/// Reads `GET /v1/invites/<code>`'s answer.
#[uniffi::export]
pub fn sync_decode_invite_preview(body: String) -> Result<SyncInvitePreview, CoreError> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Preview {
        space_id: String,
        #[serde(default)]
        name: String,
        owner_email: Option<String>,
        expires_at: String,
        #[serde(default)]
        joined: bool,
    }
    let preview: Preview = serde_json::from_str(&body).map_err(|error| CoreError::File {
        detail: format!("The sync server's invite couldn't be read: {error}"),
    })?;
    Ok(SyncInvitePreview {
        space_id: preview.space_id,
        name: preview.name,
        owner_email: preview.owner_email,
        expires_at: preview.expires_at,
        joined: preview.joined,
    })
}

/// The invite code in what someone pasted or typed: a bare code, an
/// `https://…/join/<code>` link or a `takt://join/<code>` one. Dashes and
/// spaces are dropped, so a code read out in groups still works. Nothing
/// when no code is there.
#[uniffi::export]
pub fn sync_invite_code(text: String) -> Option<String> {
    let trimmed = text.trim();
    let tail = match trimmed.rfind("/join/") {
        Some(at) => &trimmed[at + "/join/".len()..],
        None => trimmed,
    };
    let tail = tail.split(['?', '#', '/']).next().unwrap_or("");
    let code: String = tail
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_ascii_uppercase();
    (code.len() >= 20 && code.len() <= 64 && code.chars().all(|c| c.is_ascii_alphanumeric()))
        .then_some(code)
}

/// The link to send someone: the server's page for the code, which opens
/// the app or shows the code to type in.
#[uniffi::export]
pub fn sync_invite_link(server_url: String, code: String) -> String {
    format!("{}/join/{code}", server_url.trim_end_matches('/'))
}

/// A code in groups of four, for reading out or typing.
#[uniffi::export]
pub fn sync_invite_code_grouped(code: String) -> String {
    code.as_bytes()
        .chunks(4)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect::<Vec<_>>()
        .join("-")
}

/// The request paths, so every client names them the same way.
#[uniffi::export]
pub fn sync_spaces_path() -> String {
    "v1/spaces".into()
}

#[uniffi::export]
pub fn sync_space_path(space_id: String) -> String {
    format!("v1/spaces/{space_id}")
}

#[uniffi::export]
pub fn sync_space_invites_path(space_id: String) -> String {
    format!("v1/spaces/{space_id}/invites")
}

#[uniffi::export]
pub fn sync_space_member_path(space_id: String, account_id: String) -> String {
    format!("v1/spaces/{space_id}/members/{account_id}")
}

#[uniffi::export]
pub fn sync_invite_path(code: String) -> String {
    format!("v1/invites/{code}")
}

#[uniffi::export]
pub fn sync_accept_invite_path(code: String) -> String {
    format!("v1/invites/{code}/accept")
}

/// Reads a space the server answered with: sharing a list, or joining one.
pub fn decode_space(body: &str) -> Result<WireSpace, CoreError> {
    serde_json::from_str(body).map_err(|error| CoreError::File {
        detail: format!("The sync server's answer couldn't be read: {error}"),
    })
}
