//! Writes to lists and folders.
//!
//! Like [`crate::tasks`], each takes the caller's transaction and names the
//! Swift and Kotlin methods it replaced.

use rusqlite::{OptionalExtension, Transaction};

use crate::CoreError;
use crate::time::{non_empty_name, stored};

/// A folder or list [`create_folder`] or [`create_list`] made: what a client
/// needs to build its own model of it, beside what it already passed in.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CreatedItem {
    pub id: String,
    /// The name as stored, trimmed.
    pub name: String,
    /// Its place among its siblings: after the last of them.
    pub sort_order: i64,
}

/// Creates a folder at the end of its siblings, inside `parent_folder_id`
/// or at the top. Replaces `WorkspaceStore.createFolder`,
/// `WorkspaceRepository.createFolder` and the CLI's `create_folder`.
pub fn create_folder(
    transaction: &Transaction,
    workspace_id: &str,
    name: &str,
    parent_folder_id: Option<&str>,
    now_ms: i64,
) -> Result<CreatedItem, CoreError> {
    let name = non_empty_name(name)?;
    if let Some(parent) = parent_folder_id {
        require_folder_in(transaction, parent, workspace_id)?;
    }
    let sort_order: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(sortOrder), -1) + 1 FROM list_folders
         WHERE workspaceId = ?1 AND parentFolderId IS ?2",
        rusqlite::params![workspace_id, parent_folder_id],
        |row| row.get(0),
    )?;
    let id = new_id();
    let now = stored(now_ms);
    transaction.execute(
        "INSERT INTO list_folders (id, workspaceId, parentFolderId, name, sortOrder, createdAt, updatedAt)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        rusqlite::params![id, workspace_id, parent_folder_id, name, sort_order, now],
    )?;
    Ok(CreatedItem {
        id,
        name,
        sort_order,
    })
}

/// Creates a list at the end of its siblings, in `folder_id` or at the top.
/// Replaces `WorkspaceStore.createList`, `WorkspaceRepository.createList` and
/// the CLI's `create_list`.
pub fn create_list(
    transaction: &Transaction,
    workspace_id: &str,
    name: &str,
    folder_id: Option<&str>,
    now_ms: i64,
) -> Result<CreatedItem, CoreError> {
    let name = non_empty_name(name)?;
    if let Some(folder) = folder_id {
        require_folder_in(transaction, folder, workspace_id)?;
    }
    let sort_order: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(sortOrder), -1) + 1 FROM task_lists
         WHERE workspaceId = ?1 AND folderId IS ?2",
        rusqlite::params![workspace_id, folder_id],
        |row| row.get(0),
    )?;
    let id = new_id();
    let now = stored(now_ms);
    transaction.execute(
        "INSERT INTO task_lists (id, workspaceId, folderId, name, colorHex, sortOrder, isArchived,
                                 createdAt, updatedAt, systemRole, visibleRootTaskId, completedAt)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5, 0, ?6, ?6, NULL, NULL, NULL)",
        rusqlite::params![id, workspace_id, folder_id, name, sort_order, now],
    )?;
    Ok(CreatedItem {
        id,
        name,
        sort_order,
    })
}

/// Moves a folder into `parent_folder_id` (or to the top), after the
/// folders already there. Refuses a parent inside the folder itself.
/// Replaces `WorkspaceStore.moveFolder` and `WorkspaceRepository.moveFolder`.
pub fn move_folder(
    transaction: &Transaction,
    id: &str,
    parent_folder_id: Option<&str>,
    now_ms: i64,
) -> Result<(), CoreError> {
    let (workspace_id, current_parent) = folder_place(transaction, id)?;
    if let Some(parent) = parent_folder_id {
        if parent == id {
            return Err(CoreError::InvalidFolderMove);
        }
        require_folder_in(transaction, parent, &workspace_id)?;
        if folder_is_within(transaction, parent, id)? {
            return Err(CoreError::InvalidFolderMove);
        }
    }
    if current_parent.as_deref() == parent_folder_id {
        return Ok(());
    }
    let sort_order: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(sortOrder), -1) + 1 FROM list_folders
         WHERE workspaceId = ?1 AND parentFolderId IS ?2",
        rusqlite::params![workspace_id, parent_folder_id],
        |row| row.get(0),
    )?;
    transaction.execute(
        "UPDATE list_folders SET parentFolderId = ?1, sortOrder = ?2, updatedAt = ?3 WHERE id = ?4",
        rusqlite::params![parent_folder_id, sort_order, stored(now_ms), id],
    )?;
    Ok(())
}

/// Moves a list into `folder_id` (or to the top), after the lists already
/// there. Replaces `WorkspaceStore.moveList`, `WorkspaceRepository.moveList`
/// and the CLI's `move_list`.
pub fn move_list(
    transaction: &Transaction,
    id: &str,
    folder_id: Option<&str>,
    now_ms: i64,
) -> Result<(), CoreError> {
    let (workspace_id, current_folder, _) = list_place(transaction, id)?;
    if let Some(folder) = folder_id {
        require_folder_in(transaction, folder, &workspace_id)?;
    }
    if current_folder.as_deref() == folder_id {
        return Ok(());
    }
    let sort_order: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(sortOrder), -1) + 1 FROM task_lists
         WHERE workspaceId = ?1 AND folderId IS ?2",
        rusqlite::params![workspace_id, folder_id],
        |row| row.get(0),
    )?;
    transaction.execute(
        "UPDATE task_lists SET folderId = ?1, sortOrder = ?2, updatedAt = ?3 WHERE id = ?4",
        rusqlite::params![folder_id, sort_order, stored(now_ms), id],
    )?;
    Ok(())
}

/// Moves a list `offset` places among its siblings (same folder, same
/// archived state), stopping at either end. Replaces
/// `WorkspaceStore.moveListWithinFolder` and its Kotlin copy.
pub fn move_list_within_folder(
    transaction: &Transaction,
    id: &str,
    offset: i32,
    now_ms: i64,
) -> Result<(), CoreError> {
    let (workspace_id, folder, archived) = list_place(transaction, id)?;
    let mut siblings = list_siblings(transaction, &workspace_id, folder.as_deref(), archived)?;
    if nudge(&mut siblings, id, offset) {
        persist_order(transaction, "task_lists", &siblings, now_ms)?;
    }
    Ok(())
}

/// Moves a folder `offset` places among its siblings. Replaces
/// `WorkspaceStore.moveFolderWithinSiblings` and its Kotlin copy.
pub fn move_folder_within_siblings(
    transaction: &Transaction,
    id: &str,
    offset: i32,
    now_ms: i64,
) -> Result<(), CoreError> {
    let (workspace_id, parent) = folder_place(transaction, id)?;
    let mut siblings = folder_siblings(transaction, &workspace_id, parent.as_deref())?;
    if nudge(&mut siblings, id, offset) {
        persist_order(transaction, "list_folders", &siblings, now_ms)?;
    }
    Ok(())
}

/// Drops a list before `before_id` (or at the end) in `folder_id`, as a
/// sidebar drag does. Replaces `WorkspaceStore.placeList` and its Kotlin copy.
pub fn place_list(
    transaction: &Transaction,
    id: &str,
    before_id: Option<&str>,
    folder_id: Option<&str>,
    now_ms: i64,
) -> Result<(), CoreError> {
    let (workspace_id, current_folder, archived) = list_place(transaction, id)?;
    if before_id == Some(id) {
        return Ok(());
    }
    if let Some(folder) = folder_id {
        require_folder_in(transaction, folder, &workspace_id)?;
    }
    if current_folder.as_deref() != folder_id {
        transaction.execute(
            "UPDATE task_lists SET folderId = ?1, updatedAt = ?2 WHERE id = ?3",
            rusqlite::params![folder_id, stored(now_ms), id],
        )?;
    }
    let mut siblings = list_siblings(transaction, &workspace_id, folder_id, archived)?;
    if place(&mut siblings, id, before_id) {
        persist_order(transaction, "task_lists", &siblings, now_ms)?;
    }
    Ok(())
}

/// Drops a folder before `before_id` (or at the end) in `parent_folder_id`.
/// Walking up from the new parent must not reach the folder itself, or the
/// tree stops being a tree. Replaces `WorkspaceStore.placeFolder` and its
/// Kotlin copy.
pub fn place_folder(
    transaction: &Transaction,
    id: &str,
    before_id: Option<&str>,
    parent_folder_id: Option<&str>,
    now_ms: i64,
) -> Result<(), CoreError> {
    let (workspace_id, current_parent) = folder_place(transaction, id)?;
    if before_id == Some(id) {
        return Ok(());
    }
    if let Some(parent) = parent_folder_id {
        require_folder_in(transaction, parent, &workspace_id)?;
        if parent == id || folder_is_within(transaction, parent, id)? {
            return Err(CoreError::InvalidFolderMove);
        }
    }
    if current_parent.as_deref() != parent_folder_id {
        transaction.execute(
            "UPDATE list_folders SET parentFolderId = ?1, updatedAt = ?2 WHERE id = ?3",
            rusqlite::params![parent_folder_id, stored(now_ms), id],
        )?;
    }
    let mut siblings = folder_siblings(transaction, &workspace_id, parent_folder_id)?;
    if place(&mut siblings, id, before_id) {
        persist_order(transaction, "list_folders", &siblings, now_ms)?;
    }
    Ok(())
}

/// Saves the folder settings sheet: a name and a parent, moving the folder
/// after the folders already there when the parent changes. Replaces
/// `WorkspaceStore.saveFolderSettings` and its Kotlin copy.
pub fn save_folder_settings(
    transaction: &Transaction,
    id: &str,
    name: &str,
    parent_folder_id: Option<&str>,
    now_ms: i64,
) -> Result<(), CoreError> {
    let name = non_empty_name(name)?;
    let (workspace_id, current_parent) = folder_place(transaction, id)?;
    if let Some(parent) = parent_folder_id {
        if parent == id {
            return Err(CoreError::InvalidFolderMove);
        }
        require_folder_in(transaction, parent, &workspace_id)?;
        if folder_is_within(transaction, parent, id)? {
            return Err(CoreError::InvalidFolderMove);
        }
    }
    let current_name: String =
        transaction.query_row("SELECT name FROM list_folders WHERE id = ?1", [id], |row| {
            row.get(0)
        })?;
    let moves = current_parent.as_deref() != parent_folder_id;
    if current_name == name && !moves {
        return Ok(());
    }
    let sort_order: Option<i64> = if moves {
        Some(transaction.query_row(
            "SELECT COALESCE(MAX(sortOrder), -1) + 1 FROM list_folders WHERE workspaceId = ?1 AND parentFolderId IS ?2",
            rusqlite::params![workspace_id, parent_folder_id],
            |row| row.get(0),
        )?)
    } else {
        None
    };
    transaction.execute(
        "UPDATE list_folders SET name = ?1, parentFolderId = ?2, sortOrder = COALESCE(?3, sortOrder), updatedAt = ?4
         WHERE id = ?5",
        rusqlite::params![name, parent_folder_id, sort_order, stored(now_ms), id],
    )?;
    Ok(())
}

/// What the list settings sheet saves.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ListSettings {
    pub name: String,
    pub colour_hex: Option<String>,
    pub folder_id: Option<String>,
    pub is_archived: bool,
    /// The wrapper task the list shows the children of, if any.
    pub visible_root_task_id: Option<String>,
}

/// Saves the list settings sheet. The Inbox cannot be archived, and a
/// changed visible root must be the list's single root, imported or a list,
/// with children. Replaces `WorkspaceStore.saveListSettings` and its Kotlin
/// copy.
pub fn save_list_settings(
    transaction: &Transaction,
    id: &str,
    settings: &ListSettings,
    now_ms: i64,
) -> Result<(), CoreError> {
    let name = non_empty_name(&settings.name)?;
    let colour = settings
        .colour_hex
        .as_deref()
        .map(str::trim)
        .filter(|colour| !colour.is_empty())
        .map(str::to_string);
    let current: Option<StoredListSettings> = transaction
        .query_row(
            "SELECT name, colorHex, folderId, isArchived, visibleRootTaskId, systemRole, workspaceId
             FROM task_lists WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
        )
        .optional()?;
    let Some((
        current_name,
        current_colour,
        current_folder,
        archived,
        root,
        system_role,
        workspace_id,
    )) = current
    else {
        return Err(CoreError::MissingList { id: id.to_string() });
    };
    if let Some(folder) = settings.folder_id.as_deref() {
        require_folder_in(transaction, folder, &workspace_id)?;
    }
    if settings.is_archived && system_role.is_some() {
        return Err(CoreError::SystemListIsPermanent);
    }
    if root != settings.visible_root_task_id {
        validate_visible_root(transaction, id, settings.visible_root_task_id.as_deref())?;
    }
    let unchanged = current_name == name
        && current_colour == colour
        && current_folder == settings.folder_id
        && archived == settings.is_archived
        && root == settings.visible_root_task_id;
    if unchanged {
        return Ok(());
    }
    let sort_order: Option<i64> = if current_folder != settings.folder_id {
        Some(transaction.query_row(
            "SELECT COALESCE(MAX(sortOrder), -1) + 1 FROM task_lists WHERE workspaceId = ?1 AND folderId IS ?2",
            rusqlite::params![workspace_id, settings.folder_id],
            |row| row.get(0),
        )?)
    } else {
        None
    };
    transaction.execute(
        "UPDATE task_lists SET name = ?1, colorHex = ?2, folderId = ?3, isArchived = ?4, visibleRootTaskId = ?5,
                               sortOrder = COALESCE(?6, sortOrder), updatedAt = ?7
         WHERE id = ?8",
        rusqlite::params![
            name,
            colour,
            settings.folder_id,
            settings.is_archived,
            settings.visible_root_task_id,
            sort_order,
            stored(now_ms),
            id
        ],
    )?;
    Ok(())
}

/// A list's name, colour, folder, archived flag, visible root, system role
/// and workspace, as stored.
type StoredListSettings = (
    String,
    Option<String>,
    Option<String>,
    bool,
    Option<String>,
    Option<String>,
    String,
);

/// `WorkspaceStore.validateVisibleRoot`.
fn validate_visible_root(
    transaction: &Transaction,
    list_id: &str,
    root_id: Option<&str>,
) -> Result<(), CoreError> {
    let Some(root_id) = root_id else {
        return Ok(());
    };
    let mut statement = transaction.prepare(
        "SELECT id, sourceSystem IS NOT NULL, COALESCE(itemKind, 'task') = 'list'
         FROM tasks WHERE listId = ?1 AND parentTaskId IS NULL",
    )?;
    let roots: Vec<(String, bool, bool)> = statement
        .query_map([list_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<Result<_, _>>()?;
    let valid = match roots.as_slice() {
        [(id, imported, is_list)] if id == root_id && (*imported || *is_list) => {
            *is_list
                || transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM tasks WHERE parentTaskId = ?1)",
                    [root_id],
                    |row| row.get::<_, bool>(0),
                )?
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(CoreError::InvalidVisibleRoot)
    }
}

/// Moves `id` by `offset` within `ids`, clamped to the ends. False when it
/// does not move, so nothing is written and redo survives.
fn nudge(ids: &mut Vec<String>, id: &str, offset: i32) -> bool {
    let Some(index) = ids.iter().position(|sibling| sibling == id) else {
        return false;
    };
    let last = ids.len() as i64 - 1;
    let target = (index as i64 + i64::from(offset)).clamp(0, last) as usize;
    if target == index {
        return false;
    }
    let moved = ids.remove(index);
    ids.insert(target, moved);
    true
}

/// Puts `id` before `before_id` within `ids`, or last when that is absent.
/// False only when `id` is not among them.
fn place(ids: &mut Vec<String>, id: &str, before_id: Option<&str>) -> bool {
    let Some(index) = ids.iter().position(|sibling| sibling == id) else {
        return false;
    };
    let moved = ids.remove(index);
    let target = before_id
        .and_then(|before| ids.iter().position(|sibling| sibling == before))
        .unwrap_or(ids.len());
    ids.insert(target, moved);
    true
}

/// Numbers `ids` from zero in order and stamps each one, as both clients'
/// `persistListOrder` and `persistFolderOrder` did: every sibling, moved or
/// not, so each is one journalled and synced change.
fn persist_order(
    transaction: &Transaction,
    table: &str,
    ids: &[String],
    now_ms: i64,
) -> Result<(), CoreError> {
    let now = stored(now_ms);
    let mut statement = transaction.prepare(&format!(
        "UPDATE {table} SET sortOrder = ?1, updatedAt = ?2 WHERE id = ?3"
    ))?;
    for (index, id) in ids.iter().enumerate() {
        statement.execute(rusqlite::params![index as i64, now, id])?;
    }
    Ok(())
}

fn list_place(
    transaction: &Transaction,
    id: &str,
) -> Result<(String, Option<String>, bool), CoreError> {
    transaction
        .query_row(
            "SELECT workspaceId, folderId, isArchived FROM task_lists WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::MissingList { id: id.to_string() })
}

fn folder_place(
    transaction: &Transaction,
    id: &str,
) -> Result<(String, Option<String>), CoreError> {
    transaction
        .query_row(
            "SELECT workspaceId, parentFolderId FROM list_folders WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::MissingFolder { id: id.to_string() })
}

fn list_siblings(
    transaction: &Transaction,
    workspace_id: &str,
    folder_id: Option<&str>,
    archived: bool,
) -> Result<Vec<String>, CoreError> {
    let mut statement = transaction.prepare(
        "SELECT id FROM task_lists WHERE workspaceId = ?1 AND folderId IS ?2 AND isArchived = ?3
         ORDER BY sortOrder, createdAt, id",
    )?;
    let ids = statement
        .query_map(
            rusqlite::params![workspace_id, folder_id, archived],
            |row| row.get(0),
        )?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

fn folder_siblings(
    transaction: &Transaction,
    workspace_id: &str,
    parent_folder_id: Option<&str>,
) -> Result<Vec<String>, CoreError> {
    let mut statement = transaction.prepare(
        "SELECT id FROM list_folders WHERE workspaceId = ?1 AND parentFolderId IS ?2
         ORDER BY sortOrder, createdAt, id",
    )?;
    let ids = statement
        .query_map(rusqlite::params![workspace_id, parent_folder_id], |row| {
            row.get(0)
        })?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Whether `folder_id` sits somewhere inside `ancestor_id`.
fn folder_is_within(
    transaction: &Transaction,
    folder_id: &str,
    ancestor_id: &str,
) -> Result<bool, CoreError> {
    Ok(transaction.query_row(
        "WITH RECURSIVE inside(id) AS (
           SELECT id FROM list_folders WHERE parentFolderId = ?1
           UNION
           SELECT list_folders.id FROM list_folders JOIN inside ON list_folders.parentFolderId = inside.id
         )
         SELECT EXISTS(SELECT 1 FROM inside WHERE id = ?2)",
        rusqlite::params![ancestor_id, folder_id],
        |row| row.get(0),
    )?)
}

/// A folder that exists and belongs to `workspace_id`; anything else is a
/// missing folder, as both clients reported it.
fn require_folder_in(
    transaction: &Transaction,
    folder_id: &str,
    workspace_id: &str,
) -> Result<(), CoreError> {
    let owner: Option<String> = transaction
        .query_row(
            "SELECT workspaceId FROM list_folders WHERE id = ?1",
            [folder_id],
            |row| row.get(0),
        )
        .optional()?;
    match owner {
        Some(owner) if owner == workspace_id => Ok(()),
        _ => Err(CoreError::MissingFolder {
            id: folder_id.to_string(),
        }),
    }
}

/// An identifier as GRDB's `UUID().uuidString` writes one: uppercase.
pub(crate) fn new_id() -> String {
    uuid::Uuid::new_v4().to_string().to_uppercase()
}

/// What [`delete_list`] removed.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DeletedList {
    pub id: String,
    pub name: String,
    /// How many tasks went with it.
    pub tasks_deleted: u32,
}

/// Deletes a list and, through the foreign keys, every task in it. The Inbox
/// is permanent: quick capture lands there. Replaces
/// `WorkspaceStore.deleteList` and `WorkspaceRepository.deleteList`.
pub fn delete_list(transaction: &Transaction, id: &str) -> Result<DeletedList, CoreError> {
    let (name, system_role): (String, Option<String>) = transaction
        .query_row(
            "SELECT name, systemRole FROM task_lists WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::MissingList { id: id.to_string() })?;
    if system_role.is_some() {
        return Err(CoreError::SystemListIsPermanent);
    }
    crate::sync::spaces::refuse_if_shared(transaction, id)?;
    let tasks_deleted: u32 = transaction.query_row(
        "SELECT COUNT(*) FROM tasks WHERE listId = ?1",
        [id],
        |row| row.get(0),
    )?;
    transaction.execute("DELETE FROM task_lists WHERE id = ?1", [id])?;
    Ok(DeletedList {
        id: id.to_string(),
        name,
        tasks_deleted,
    })
}

/// Deletes a folder. Its lists are kept: the schema's SET NULL moves them to
/// the top of the sidebar. Folders inside it cascade with it. Replaces
/// `WorkspaceStore.deleteFolder` and `WorkspaceRepository.deleteFolder`.
pub fn delete_folder(transaction: &Transaction, id: &str) -> Result<(), CoreError> {
    let deleted = transaction.execute("DELETE FROM list_folders WHERE id = ?1", [id])?;
    if deleted == 0 {
        return Err(CoreError::MissingFolder { id: id.to_string() });
    }
    Ok(())
}

/// Renames a folder. A name that is already the folder's changes nothing,
/// so it records no step and leaves redo alone. Replaces
/// `WorkspaceStore.updateFolder` and `WorkspaceRepository.updateFolder`.
pub fn rename_folder(
    transaction: &Transaction,
    id: &str,
    name: &str,
    now_ms: i64,
) -> Result<(), CoreError> {
    let name = non_empty_name(name)?;
    let current: String = transaction
        .query_row("SELECT name FROM list_folders WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .optional()?
        .ok_or_else(|| CoreError::MissingFolder { id: id.to_string() })?;
    if current != name {
        transaction.execute(
            "UPDATE list_folders SET name = ?1, updatedAt = ?2 WHERE id = ?3",
            rusqlite::params![name, stored(now_ms), id],
        )?;
    }
    Ok(())
}

/// Renames a list and touches nothing else, so undo offers "Rename List".
/// Replaces `WorkspaceStore.renameList` and `WorkspaceRepository.renameList`.
pub fn rename_list(
    transaction: &Transaction,
    id: &str,
    name: &str,
    now_ms: i64,
) -> Result<(), CoreError> {
    let name = non_empty_name(name)?;
    let (current, _) = list_name_and_colour(transaction, id)?;
    if current != name {
        transaction.execute(
            "UPDATE task_lists SET name = ?1, updatedAt = ?2 WHERE id = ?3",
            rusqlite::params![name, stored(now_ms), id],
        )?;
    }
    Ok(())
}

/// Sets a list's name and colour together, as the list settings sheet saves
/// them. A blank colour clears it. Replaces `WorkspaceStore.updateList` and
/// `WorkspaceRepository.updateList`.
pub fn update_list(
    transaction: &Transaction,
    id: &str,
    name: &str,
    colour_hex: Option<&str>,
    now_ms: i64,
) -> Result<(), CoreError> {
    let name = non_empty_name(name)?;
    let colour = colour_hex.map(str::trim).filter(|c| !c.is_empty());
    let (current_name, current_colour) = list_name_and_colour(transaction, id)?;
    if current_name != name || current_colour.as_deref() != colour {
        transaction.execute(
            "UPDATE task_lists SET name = ?1, colorHex = ?2, updatedAt = ?3 WHERE id = ?4",
            rusqlite::params![name, colour, stored(now_ms), id],
        )?;
    }
    Ok(())
}

/// Archives or restores a list. The Inbox cannot be archived: the app puts
/// things there by itself, so it has to stay somewhere the user can see.
/// Replaces `WorkspaceStore.setListArchived` and
/// `WorkspaceRepository.setListArchived`.
pub fn set_list_archived(
    transaction: &Transaction,
    id: &str,
    archived: bool,
    now_ms: i64,
) -> Result<(), CoreError> {
    let system_role: Option<String> = transaction
        .query_row(
            "SELECT systemRole FROM task_lists WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::MissingList { id: id.to_string() })?;
    if archived && system_role.is_some() {
        return Err(CoreError::SystemListIsPermanent);
    }
    transaction.execute(
        "UPDATE task_lists SET isArchived = ?1, updatedAt = ?2 WHERE id = ?3",
        rusqlite::params![archived, stored(now_ms), id],
    )?;
    Ok(())
}

fn list_name_and_colour(
    transaction: &Transaction,
    id: &str,
) -> Result<(String, Option<String>), CoreError> {
    transaction
        .query_row(
            "SELECT name, colorHex FROM task_lists WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::MissingList { id: id.to_string() })
}

#[cfg(test)]
mod tests;
