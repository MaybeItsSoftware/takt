//! The workspace's rows as the clients see them: tasks, lists, folders and
//! workspaces. Every read that hands back whole rows builds them here, so a
//! column added to a table is added once, not once per read and per client.
//!
//! Dates cross as epoch milliseconds, like everywhere else in the core.

use rusqlite::{Connection, OptionalExtension, Row, RowIndex, Statement};

use crate::CoreError;
use crate::time::parse_stored;

/// A row of `tasks`. Swift's `WorkspaceTask`, Kotlin's `WorkspaceTask`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TaskRow {
    pub id: String,
    pub list_id: String,
    pub parent_task_id: Option<String>,
    pub title: String,
    pub notes: String,
    /// "open", "completed" or "cancelled".
    pub status: String,
    pub sort_order: i64,
    pub due_at_ms: Option<i64>,
    pub estimate_seconds: Option<i64>,
    pub source_system: Option<String>,
    pub source_id: Option<String>,
    /// "task" or "list"; absent on rows written before the column existed.
    pub item_kind: Option<String>,
    pub is_promoted: Option<bool>,
    pub archived_at_ms: Option<i64>,
    pub completed_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// A row of `task_lists`. `TaskList`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ListRow {
    pub id: String,
    pub workspace_id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub color_hex: Option<String>,
    pub sort_order: i64,
    pub is_archived: bool,
    /// "inbox" for the Inbox; absent for an ordinary list.
    pub system_role: Option<String>,
    pub visible_root_task_id: Option<String>,
    pub completed_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// Its place among the prioritised lists, 1 the most important; absent
    /// when it is not prioritised. As stored: [`prioritised_lists`] gives
    /// the order with any gap or tie a merge left closed up.
    #[uniffi(default = None)]
    pub priority_rank: Option<i64>,
    /// What every task in it requires, as a task's requirement groups are
    /// kept: every group met, any member of one, "!id" for a condition not
    /// holding. Empty for none. See `conditions::inherited`.
    #[uniffi(default = [])]
    pub requirement_groups: Vec<Vec<String>>,
}

/// A row of `list_folders`. `ListFolder`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FolderRow {
    pub id: String,
    pub workspace_id: String,
    pub parent_folder_id: Option<String>,
    pub name: String,
    pub sort_order: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// A row of `workspaces`. `Workspace`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct WorkspaceRow {
    pub id: String,
    pub name: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// A stored date column as milliseconds; absent when empty or unreadable.
pub(crate) fn optional_ms<I: RowIndex>(row: &Row, column: I) -> rusqlite::Result<Option<i64>> {
    let text: Option<String> = row.get(column)?;
    Ok(text
        .as_deref()
        .and_then(parse_stored)
        .map(|at| at.timestamp_millis()))
}

/// A stored date column that is always written, as milliseconds.
pub(crate) fn required_ms<I: RowIndex>(row: &Row, column: I) -> rusqlite::Result<i64> {
    Ok(optional_ms(row, column)?.unwrap_or(0))
}

impl TaskRow {
    /// The columns `from_row` reads, qualified, for a `SELECT` that joins.
    pub(crate) const COLUMNS: &'static str = "tasks.id, tasks.listId, tasks.parentTaskId, \
         tasks.title, tasks.notes, tasks.status, tasks.sortOrder, tasks.dueAt, \
         tasks.estimateSeconds, tasks.sourceSystem, tasks.sourceId, tasks.itemKind, \
         tasks.isPromoted, tasks.archivedAt, tasks.completedAt, tasks.createdAt, tasks.updatedAt";

    pub(crate) fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            list_id: row.get("listId")?,
            parent_task_id: row.get("parentTaskId")?,
            title: row.get("title")?,
            notes: row.get::<_, Option<String>>("notes")?.unwrap_or_default(),
            status: row.get("status")?,
            sort_order: row.get("sortOrder")?,
            due_at_ms: optional_ms(row, "dueAt")?,
            estimate_seconds: row.get("estimateSeconds")?,
            source_system: row.get("sourceSystem")?,
            source_id: row.get("sourceId")?,
            item_kind: row.get("itemKind")?,
            is_promoted: row.get("isPromoted")?,
            archived_at_ms: optional_ms(row, "archivedAt")?,
            completed_at_ms: optional_ms(row, "completedAt")?,
            created_at_ms: required_ms(row, "createdAt")?,
            updated_at_ms: required_ms(row, "updatedAt")?,
        })
    }
}

/// Where `TaskRow::from_row`'s columns sit in a statement's results, found
/// once per statement. Reading a column by name searches the statement's
/// names on every call, which for a read of every task was half the core's
/// time; `SELECT *` keeps its own column order, so positions are looked up
/// rather than assumed.
pub(crate) struct TaskColumns {
    id: usize,
    list_id: usize,
    parent_task_id: usize,
    title: usize,
    notes: usize,
    status: usize,
    sort_order: usize,
    due_at: usize,
    estimate_seconds: usize,
    source_system: usize,
    source_id: usize,
    item_kind: usize,
    is_promoted: usize,
    archived_at: usize,
    completed_at: usize,
    created_at: usize,
    updated_at: usize,
}

impl TaskColumns {
    pub(crate) fn of(statement: &Statement) -> rusqlite::Result<Self> {
        let at = |name: &str| statement.column_index(name);
        Ok(Self {
            id: at("id")?,
            list_id: at("listId")?,
            parent_task_id: at("parentTaskId")?,
            title: at("title")?,
            notes: at("notes")?,
            status: at("status")?,
            sort_order: at("sortOrder")?,
            due_at: at("dueAt")?,
            estimate_seconds: at("estimateSeconds")?,
            source_system: at("sourceSystem")?,
            source_id: at("sourceId")?,
            item_kind: at("itemKind")?,
            is_promoted: at("isPromoted")?,
            archived_at: at("archivedAt")?,
            completed_at: at("completedAt")?,
            created_at: at("createdAt")?,
            updated_at: at("updatedAt")?,
        })
    }

    /// `TaskRow::from_row`, by position.
    pub(crate) fn read(&self, row: &Row) -> rusqlite::Result<TaskRow> {
        Ok(TaskRow {
            id: row.get(self.id)?,
            list_id: row.get(self.list_id)?,
            parent_task_id: row.get(self.parent_task_id)?,
            title: row.get(self.title)?,
            notes: row
                .get::<_, Option<String>>(self.notes)?
                .unwrap_or_default(),
            status: row.get(self.status)?,
            sort_order: row.get(self.sort_order)?,
            due_at_ms: optional_ms(row, self.due_at)?,
            estimate_seconds: row.get(self.estimate_seconds)?,
            source_system: row.get(self.source_system)?,
            source_id: row.get(self.source_id)?,
            item_kind: row.get(self.item_kind)?,
            is_promoted: row.get(self.is_promoted)?,
            archived_at_ms: optional_ms(row, self.archived_at)?,
            completed_at_ms: optional_ms(row, self.completed_at)?,
            created_at_ms: required_ms(row, self.created_at)?,
            updated_at_ms: required_ms(row, self.updated_at)?,
        })
    }
}

impl ListRow {
    pub(crate) fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            workspace_id: row.get("workspaceId")?,
            folder_id: row.get("folderId")?,
            name: row.get("name")?,
            color_hex: row.get("colorHex")?,
            sort_order: row.get("sortOrder")?,
            is_archived: row.get("isArchived")?,
            system_role: row.get("systemRole")?,
            visible_root_task_id: row.get("visibleRootTaskId")?,
            completed_at_ms: optional_ms(row, "completedAt")?,
            created_at_ms: required_ms(row, "createdAt")?,
            updated_at_ms: required_ms(row, "updatedAt")?,
            // A database from before `v40_list_priority` (the CLI reads
            // without migrating) has no ranks.
            priority_rank: row.get("priorityRank").unwrap_or(None),
            // Likewise none before `v41_device_and_list_conditions`.
            requirement_groups: row
                .get::<_, Option<String>>("requirementGroupsJSON")
                .unwrap_or(None)
                .map(|json| crate::conditions::stored_groups(&json))
                .unwrap_or_default(),
        })
    }
}

impl FolderRow {
    pub(crate) fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            workspace_id: row.get("workspaceId")?,
            parent_folder_id: row.get("parentFolderId")?,
            name: row.get("name")?,
            sort_order: row.get("sortOrder")?,
            created_at_ms: required_ms(row, "createdAt")?,
            updated_at_ms: required_ms(row, "updatedAt")?,
        })
    }
}

impl WorkspaceRow {
    pub(crate) fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            created_at_ms: required_ms(row, "createdAt")?,
            updated_at_ms: required_ms(row, "updatedAt")?,
        })
    }
}

#[cfg(test)]
mod tests;

/// Every row `sql` returns, built by `build`.
pub(crate) fn all<T>(
    connection: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
    build: fn(&Row) -> rusqlite::Result<T>,
) -> Result<Vec<T>, CoreError> {
    let mut statement = connection.prepare_cached(sql)?;
    let rows = statement.query_map(params, build)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The workspaces, oldest first. `WorkspaceStore.workspaces`.
pub fn workspaces(connection: &Connection) -> Result<Vec<WorkspaceRow>, CoreError> {
    all(
        connection,
        "SELECT * FROM workspaces ORDER BY createdAt",
        [],
        WorkspaceRow::from_row,
    )
}

/// A workspace's folders in sidebar order. `WorkspaceStore.folders`.
pub fn folders(connection: &Connection, workspace_id: &str) -> Result<Vec<FolderRow>, CoreError> {
    all(
        connection,
        "SELECT * FROM list_folders WHERE workspaceId = ?1 ORDER BY sortOrder, createdAt, id",
        [workspace_id],
        FolderRow::from_row,
    )
}

/// A workspace's lists in sidebar order. `WorkspaceStore.lists`.
pub fn lists(
    connection: &Connection,
    workspace_id: &str,
    including_archived: bool,
) -> Result<Vec<ListRow>, CoreError> {
    all(
        connection,
        "SELECT * FROM task_lists WHERE workspaceId = ?1 AND (?2 OR isArchived = 0)
         ORDER BY sortOrder, createdAt, id",
        rusqlite::params![workspace_id, including_archived],
        ListRow::from_row,
    )
}

/// A workspace's prioritised lists, most important first, numbered 1..n
/// whatever their stored ranks: two devices that each prioritised a list
/// offline can leave a tie or a gap until the next edit closes it. Archived
/// and completed lists are left out; archiving or completing one drops its
/// rank, and this keeps one a merge brought back from showing.
pub fn prioritised_lists(
    connection: &Connection,
    workspace_id: &str,
) -> Result<Vec<ListRow>, CoreError> {
    let mut lists = all(
        connection,
        "SELECT * FROM task_lists
         WHERE workspaceId = ?1 AND priorityRank IS NOT NULL
           AND isArchived = 0 AND completedAt IS NULL
         ORDER BY priorityRank, sortOrder, createdAt, id",
        [workspace_id],
        ListRow::from_row,
    )?;
    for (index, list) in lists.iter_mut().enumerate() {
        list.priority_rank = Some(index as i64 + 1);
    }
    Ok(lists)
}

/// One list, if it exists.
pub fn list(connection: &Connection, id: &str) -> Result<Option<ListRow>, CoreError> {
    Ok(all(
        connection,
        "SELECT * FROM task_lists WHERE id = ?1",
        [id],
        ListRow::from_row,
    )?
    .pop())
}

/// A workspace's Inbox, found by its role rather than its name.
pub fn inbox(connection: &Connection, workspace_id: &str) -> Result<Option<ListRow>, CoreError> {
    Ok(all(
        connection,
        "SELECT * FROM task_lists WHERE workspaceId = ?1 AND systemRole = 'inbox' LIMIT 1",
        [workspace_id],
        ListRow::from_row,
    )?
    .pop())
}

/// One task, if it exists. `WorkspaceStore.task(id:)`.
pub fn task(connection: &Connection, id: &str) -> Result<Option<TaskRow>, CoreError> {
    Ok(all(
        connection,
        "SELECT * FROM tasks WHERE id = ?1",
        [id],
        TaskRow::from_row,
    )?
    .pop())
}

/// Tasks by id, in one read; missing ids are simply absent.
pub fn tasks_by_id(connection: &Connection, ids: &[String]) -> Result<Vec<TaskRow>, CoreError> {
    let mut result = Vec::with_capacity(ids.len());
    // Bounded batches keep well under SQLite's variable limit.
    for chunk in ids.chunks(500) {
        let marks = vec!["?"; chunk.len()].join(", ");
        result.extend(all(
            connection,
            &format!("SELECT * FROM tasks WHERE id IN ({marks})"),
            rusqlite::params_from_iter(chunk),
            TaskRow::from_row,
        )?);
    }
    Ok(result)
}

/// The children of `parent` in a list (its roots when `parent` is absent), in
/// outline order. `WorkspaceStore.tasks(in:parentTaskId:)`.
pub fn children(
    connection: &Connection,
    list_id: &str,
    parent: Option<&str>,
) -> Result<Vec<TaskRow>, CoreError> {
    let blocked = crate::dependencies::blocked_last(connection)?;
    let sections = crate::buried::section_order(blocked);
    all(
        connection,
        &format!(
            "SELECT * FROM tasks WHERE listId = ?1 AND parentTaskId IS ?2 ORDER BY {sections}, sortOrder, createdAt"
        ),
        rusqlite::params![list_id, parent],
        TaskRow::from_row,
    )
}

/// Every task in the given lists, each list's rows in outline order (by
/// list, then sort order, then age), blocked tasks after their unblocked
/// siblings (`dependencies`) and buried ones after both (`buried`).
/// `WorkspaceStore.listTrees`.
pub fn tasks_in_lists(
    connection: &Connection,
    list_ids: &[String],
) -> Result<Vec<TaskRow>, CoreError> {
    let mut unique: Vec<&String> = list_ids.iter().collect();
    unique.sort();
    unique.dedup();
    let blocked = crate::dependencies::blocked_last(connection)?;
    let sections = crate::buried::section_order(blocked);
    let mut result = Vec::new();
    for chunk in unique.chunks(500) {
        let marks = vec!["?"; chunk.len()].join(", ");
        let mut statement = connection.prepare_cached(&format!(
            "SELECT * FROM tasks WHERE listId IN ({marks}) ORDER BY listId, {sections}, sortOrder, createdAt"
        ))?;
        let columns = TaskColumns::of(&statement)?;
        let mut rows = statement.query(rusqlite::params_from_iter(chunk))?;
        while let Some(row) = rows.next()? {
            result.push(columns.read(row)?);
        }
    }
    Ok(result)
}

/// A task in an outline, with how deep it sits. `TaskOutlineItem`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OutlineItem {
    pub task: TaskRow,
    pub depth: i64,
}

/// A list's tasks under `parent` (the whole list when absent), depth first in
/// sort order, each task once even if the rows hold a cycle.
/// `WorkspaceListTree.outline(under:)`.
pub fn outline(
    connection: &Connection,
    list_id: &str,
    parent: Option<&str>,
) -> Result<Vec<OutlineItem>, CoreError> {
    let rows = tasks_in_lists(connection, &[list_id.to_string()])?;
    let mut children: std::collections::HashMap<Option<String>, Vec<TaskRow>> =
        std::collections::HashMap::new();
    for task in rows {
        children
            .entry(task.parent_task_id.clone())
            .or_default()
            .push(task);
    }
    let mut result = Vec::new();
    let mut visited = std::collections::HashSet::new();
    // An explicit stack, in reverse, so a deep outline cannot overflow ours.
    let mut stack: Vec<(TaskRow, i64)> = children
        .get(&parent.map(str::to_string))
        .into_iter()
        .flatten()
        .rev()
        .map(|task| (task.clone(), 0))
        .collect();
    while let Some((task, depth)) = stack.pop() {
        if !visited.insert(task.id.clone()) {
            continue;
        }
        if let Some(kids) = children.get(&Some(task.id.clone())) {
            stack.extend(kids.iter().rev().map(|kid| (kid.clone(), depth + 1)));
        }
        result.push(OutlineItem { task, depth });
    }
    Ok(result)
}

/// The imported wrapper a list shows its children in place of, while it is
/// still the list's only root. `WorkspaceStore.visibleRootParentTaskID`.
pub fn visible_root_parent(
    connection: &Connection,
    list_id: &str,
) -> Result<Option<String>, CoreError> {
    let Some(root) = list(connection, list_id)?.and_then(|l| l.visible_root_task_id) else {
        return Ok(None);
    };
    let roots = children(connection, list_id, None)?;
    Ok((roots.len() == 1 && roots[0].id == root).then_some(root))
}

/// The root a list may show its children in place of: its only root, when that
/// is an imported wrapper with children or a nested list.
/// `WorkspaceStore.visibleRootCandidates`.
pub fn visible_root_candidates(
    connection: &Connection,
    list_id: &str,
) -> Result<Vec<TaskRow>, CoreError> {
    let mut roots = children(connection, list_id, None)?;
    if roots.len() != 1 {
        return Ok(Vec::new());
    }
    let root = roots.remove(0);
    let is_list = root.item_kind.as_deref() == Some("list");
    if root.source_system.is_none() && !is_list {
        return Ok(Vec::new());
    }
    if !is_list {
        let has_children: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE parentTaskId = ?1)",
            [&root.id],
            |row| row.get(0),
        )?;
        if !has_children {
            return Ok(Vec::new());
        }
    }
    Ok(vec![root])
}

/// The folders a folder may move into: every other folder in its workspace
/// but its own descendants. `WorkspaceStore.validParentFolders`.
pub fn valid_parent_folders(
    connection: &Connection,
    folder_id: &str,
) -> Result<Vec<FolderRow>, CoreError> {
    let workspace: Option<String> = connection
        .query_row(
            "SELECT workspaceId FROM list_folders WHERE id = ?1",
            [folder_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(workspace) = workspace else {
        return Err(CoreError::MissingFolder {
            id: folder_id.to_string(),
        });
    };
    let excluded: std::collections::HashSet<String> = all(
        connection,
        "WITH RECURSIVE subtree(id) AS (
           SELECT ?1 UNION SELECT f.id FROM list_folders f JOIN subtree s ON f.parentFolderId = s.id)
         SELECT id FROM subtree",
        [folder_id],
        |row| row.get(0),
    )?
    .into_iter()
    .collect();
    Ok(folders(connection, &workspace)?
        .into_iter()
        .filter(|folder| !excluded.contains(&folder.id))
        .collect())
}
