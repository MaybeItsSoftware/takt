//! The app's local task tree — folders, lists and tasks — read and written.
//!
//! The app's workspace database is its source of truth, and the Checkvist tools
//! only reach the Checkvist edge of it. These are the tools that edit the tree
//! the app actually shows. Every write is the Rust core's (`takt_core`), the
//! same function the Mac, iPhone and Android apps call, run here inside the
//! CLI's own journalled step; this file adds only the CLI's named refusals and
//! its JSON. There is no second copy of a write to keep level.
//!
//! What a second writer has to get right:
//!
//! - **Undo.** Every write runs inside [`Workspace::journalled`], which brackets
//!   it with the Rust core's `journal::begin` and `journal::finish`
//!   (core/src/journal.rs). The core arms `undo_control` with a fresh group and
//!   a label, the database's own `change_log` triggers record the rows, and it
//!   disarms again inside the same transaction. The app's Undo menu then offers
//!   the step as "Undo MCP: New Task", and the app undoes it through the same
//!   core code. A write that changes something clears the redo stack, and the
//!   journal is trimmed to the core's depth.
//! - **Concurrency.** `BEGIN IMMEDIATE` takes the write lock up front, and a
//!   busy timeout waits out the app's own writes rather than failing on them.
//!   The app waits out ours the same way. Foreign keys are switched on, as
//!   `WorkspaceStore` does for every connection: a delete that does not
//!   cascade leaves orphaned subtasks behind.
//! - **Being seen.** The app polls `PRAGMA data_version` on its writer
//!   connection, which moves only when another connection commits, and
//!   reloads. See `WorkspaceViewModel+ExternalWrites.swift`.
//! - **Row shape.** Uppercase UUIDs, GRDB's `YYYY-MM-DD HH:MM:SS.SSS` UTC
//!   timestamps, `COALESCE(MAX(sortOrder), -1) + 1` for appends, dense
//!   re-numbering for reorders, `task_metadata` rows created lazily with
//!   `'[]'` defaults. `tasks_fts` is maintained by the schema's own triggers.
//!
//! What stays out: anything that is policy rather than rows. Completing a
//! repeating task has to schedule its next occurrence from a `PeriodicSchedule`
//! that only `TaktCore` can parse, so that is refused here rather than
//! done half-way.

use crate::error::{Result, ToolError};
use crate::workspace::{Workspace, local_string, map_query_error, stored_string};
use chrono::Utc;
use rusqlite::{Connection, OpenFlags, OptionalExtension, Row, Transaction, TransactionBehavior};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

// The journal group of the last write on this thread that changed
// anything, for `take_last_step`.
thread_local! {
    static LAST_STEP: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// The undo step the last journalled write on this thread made, if it made
/// one, clearing it. A tool's result carries it as `undo_step`, so the agent
/// panel can take back exactly that change (`CoreWorkspace::undo_step`).
pub fn take_last_step() -> Option<String> {
    LAST_STEP.with(|step| step.borrow_mut().take())
}

mod board;
mod conditions;
mod filters;
mod instructions;
pub use board::ColumnEdit;
pub use conditions::{ContextEdit, DayStartEdit};
pub use filters::{FilterEdit, rule_catalog_text, rule_fields};
pub use instructions::instruction_scope;

/// The newest migration whose schema these writes are written against. An
/// older database is missing columns they name; a newer one is fine, because
/// `WorkspaceStore` only ever adds nullable columns.
const REQUIRED_MIGRATION: &str = "v16_task_completion_time";

/// What the app's Undo menu shows ahead of the action, so that a step an
/// assistant took is recognisable as one before it is taken back.
const LABEL_PREFIX: &str = "MCP: ";

/// Long enough to outlast any write the app makes, short enough that a stuck
/// lock is reported rather than waited on for ever.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

const TASK_COLUMNS: &str = "t.id, t.listId, t.parentTaskId, t.title, t.notes, t.status, \
     t.sortOrder, t.dueAt, t.estimateSeconds, t.itemKind, t.isPromoted, t.archivedAt, \
     t.completedAt, m.kanbanColumn, m.externalLinksJSON, m.recurrenceRule, \
     m.taskId IS NOT NULL";

/// What `v20_waiting_follow_ups` adds to a task row, and what stands in for
/// it on a database the app has not yet migrated.
const WAITING_COLUMNS: &str = "m.waitingOn, m.waitingFollowUpAt, m.followUpOfTaskId";
const NO_WAITING_COLUMNS: &str = "NULL, NULL, NULL";
const WAITING_MIGRATION: &str = "v20_waiting_follow_ups";

/// What `v32_follow_up_repeat` adds: how often to chase again, and how many
/// follow-ups this spell of waiting has made.
const REPEAT_COLUMNS: &str = "m.waitingFollowUpEveryDays, COALESCE(m.waitingFollowUpCount, 0)";
const NO_REPEAT_COLUMNS: &str = "NULL, 0";
const REPEAT_MIGRATION: &str = "v32_follow_up_repeat";

/// What adds who added, finished and holds a task in a shared list.
const AUTHORSHIP_MIGRATION: &str = "v38_task_authorship";

/// What adds `task_dependencies`. Before it, the dependency reads come back
/// empty and the dependency writes are refused.
const DEPENDENCIES_MIGRATION: &str = "v21_task_dependencies";
/// The migration that added list priorities (`task_lists.priorityRank`).
const LIST_PRIORITY_MIGRATION: &str = "v40_list_priority";

/// One task row with the metadata the tools show beside it.
struct TaskRow {
    id: String,
    list_id: String,
    parent_task_id: Option<String>,
    title: String,
    notes: String,
    status: String,
    sort_order: i64,
    due_at: Option<String>,
    estimate_seconds: Option<i64>,
    item_kind: Option<String>,
    is_promoted: Option<bool>,
    archived_at: Option<String>,
    completed_at: Option<String>,
    kanban_column: Option<String>,
    links_json: Option<String>,
    recurrence_rule: Option<String>,
    waiting_on: Option<String>,
    follow_up_at: Option<String>,
    follow_up_of: Option<String>,
    follow_up_every_days: Option<u32>,
    follow_up_count: u32,
    planning_json: Option<String>,
    priority: Option<i64>,
    start_at: Option<String>,
    /// The conditions it requires, by name: every group must hold, any one
    /// condition in a group will do. Filled in by [`task_rows`].
    requires: Vec<Vec<String>>,
    /// What the lists it is in require of it, outermost first: each
    /// `{list_id, list, nested, requires}`. Filled in by [`task_rows`].
    requires_from_list: Vec<Value>,
}

impl TaskRow {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(TaskRow {
            id: row.get(0)?,
            list_id: row.get(1)?,
            parent_task_id: row.get(2)?,
            title: row.get(3)?,
            notes: row.get(4)?,
            status: row.get(5)?,
            sort_order: row.get(6)?,
            due_at: row.get(7)?,
            estimate_seconds: row.get(8)?,
            item_kind: row.get(9)?,
            is_promoted: row.get(10)?,
            archived_at: row.get(11)?,
            completed_at: row.get(12)?,
            kanban_column: row.get(13)?,
            links_json: row.get(14)?,
            recurrence_rule: row.get(15)?,
            waiting_on: row.get(17)?,
            follow_up_at: row.get(18)?,
            follow_up_of: row.get(19)?,
            planning_json: row.get(20)?,
            priority: row.get(21)?,
            start_at: row.get(22)?,
            follow_up_every_days: row.get(23)?,
            follow_up_count: row.get(24)?,
            requires: Vec::new(),
            requires_from_list: Vec::new(),
        })
    }

    fn links(&self) -> Vec<String> {
        decode_links(self.links_json.as_deref())
    }

    fn is_list(&self) -> bool {
        self.item_kind.as_deref() == Some("list")
    }

    /// The planning's due day, `yyyy-MM-dd`, if it has one.
    fn due_date(&self) -> Option<String> {
        self.planning_json
            .as_deref()
            .and_then(|json| takt_core::planning::Planning::from_json(json).ok())
            .and_then(|planning| planning.due_date)
    }

    fn to_json(&self) -> Value {
        let mut value = json!({
            "id": self.id,
            "list_id": self.list_id,
            "parent_task_id": self.parent_task_id,
            "title": self.title,
            "notes": self.notes,
            "status": self.status,
            // Nil is a pre-existing ordinary task; the app reads it as one.
            "kind": self.item_kind.as_deref().unwrap_or("task"),
            "sort_order": self.sort_order,
            "kanban_column": self.kanban_column,
            "external_links": self.links(),
        });
        let object = value.as_object_mut().expect("object literal");
        // Only what is set, so a tree of a hundred tasks is not a hundred
        // copies of the same six nulls.
        for (key, field) in [
            ("due_at", &self.due_at),
            ("archived_at", &self.archived_at),
            ("completed_at", &self.completed_at),
        ] {
            if field.is_some() {
                object.insert(key.into(), json!(local_string(field)));
            }
        }
        // A due day, the editor's "any time that day", lives in the
        // planning; a due time in `due_at`. A task has one or the other.
        if let Some(day) = self.due_date() {
            object.insert("due_date".into(), json!(day));
        }
        if self.start_at.is_some() {
            object.insert("start_at".into(), json!(local_string(&self.start_at)));
        }
        if let Some(priority) = self.priority.filter(|p| takt_core::tasks::is_priority(*p)) {
            object.insert("priority".into(), json!(priority));
        }
        if let Some(seconds) = self.estimate_seconds {
            object.insert("estimate_seconds".into(), json!(seconds));
        }
        if self.is_promoted == Some(true) {
            object.insert("is_promoted".into(), json!(true));
        }
        if let Some(tag) = &self.waiting_on {
            object.insert("waiting_on".into(), json!(tag));
        }
        if self.follow_up_at.is_some() {
            object.insert(
                "follow_up_at".into(),
                json!(local_string(&self.follow_up_at)),
            );
        }
        if let Some(days) = self.follow_up_every_days {
            object.insert("follow_up_every_days".into(), json!(days));
        }
        if self.follow_up_count > 0 {
            object.insert("follow_up_count".into(), json!(self.follow_up_count));
        }
        if let Some(source) = &self.follow_up_of {
            object.insert("follow_up_of".into(), json!(source));
        }
        if !self.requires.is_empty() {
            object.insert("requires".into(), json!(self.requires));
        }
        if !self.requires_from_list.is_empty() {
            object.insert("requires_from_list".into(), json!(self.requires_from_list));
        }
        if let Some(rule) = self
            .recurrence_rule
            .as_deref()
            .filter(|rule| !rule.is_empty())
        {
            object.insert("recurrence_rule".into(), json!(rule));
        }
        value
    }
}

struct ListRow {
    id: String,
    folder_id: Option<String>,
    name: String,
    sort_order: i64,
    is_archived: bool,
    system_role: Option<String>,
    visible_root_task_id: Option<String>,
    completed_at: Option<String>,
}

const LIST_COLUMNS: &str = "id, workspaceId, folderId, name, colorHex, sortOrder, isArchived, \
     systemRole, visibleRootTaskId, completedAt";

impl ListRow {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(ListRow {
            id: row.get(0)?,
            folder_id: row.get(2)?,
            name: row.get(3)?,
            sort_order: row.get(5)?,
            is_archived: row.get(6)?,
            system_role: row.get(7)?,
            visible_root_task_id: row.get(8)?,
            completed_at: row.get(9)?,
        })
    }

    fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "folder_id": self.folder_id,
            "sort_order": self.sort_order,
            "is_archived": self.is_archived,
            "system_role": self.system_role,
            "visible_root_task_id": self.visible_root_task_id,
            "completed_at": local_string(&self.completed_at),
        })
    }
}

/// What a write changes about a task. `None` leaves the field alone.
#[derive(Default)]
pub struct TaskEdit {
    pub title: Option<String>,
    pub notes: Option<String>,
    pub external_links: Option<Vec<String>>,
    pub status: Option<String>,
    /// `Some(None)` clears the column, which sends the card back to the
    /// board's first column.
    pub kanban_column: Option<Option<String>>,
    /// "task" or "list": `setItemKind`, which makes a task a nested list in
    /// place, or back.
    pub kind: Option<String>,
    /// Pin a nested list to the sidebar: `setNestedListPromoted`.
    pub pinned: Option<bool>,
    /// Who or what it waits on. `Some(None)` clears it. Either this or
    /// `follow_up_at` files the task in Waiting on, as `setWaiting` does.
    pub waiting_on: Option<Option<String>>,
    /// When to chase it, as typed: `2026-10-08 14:00`, local time, or RFC 3339.
    /// `Some(None)` clears it. The app makes the follow-up task when it is due.
    pub follow_up_at: Option<Option<String>>,
    /// Chase again every this many days (1 to 365) after a follow-up is
    /// done. `Some(None)` makes it once.
    pub follow_up_every_days: Option<Option<i64>>,
    /// When it is due, as typed: `2026-10-17` for any time that day (the
    /// editor's due date), `2026-10-17 17:00` local or RFC 3339 for a due
    /// time. `Some(None)` clears both.
    pub due: Option<Option<String>>,
    /// When it may start, as typed: a day (its start, local time), a local
    /// time or RFC 3339. `Some(None)` clears it.
    pub start: Option<Option<String>>,
    /// 1 (urgent) to 4 (low). `Some(None)` clears it.
    pub priority: Option<Option<i64>>,
    /// How long it takes, in minutes. `Some(None)` clears it.
    pub estimate_minutes: Option<Option<i64>>,
    /// Who in its shared list does it: an email, an account id or "me".
    /// `Some(None)` assigns it to nobody.
    pub assignee: Option<Option<String>>,
}

impl TaskEdit {
    /// Whether it sets anything `setWaiting` writes.
    fn waits(&self) -> bool {
        self.waiting_on.is_some()
            || self.follow_up_at.is_some()
            || self.follow_up_every_days.is_some()
    }

    fn is_empty(&self) -> bool {
        self.changes() == 0
    }

    fn changes(&self) -> usize {
        [
            self.title.is_some() || self.notes.is_some(),
            self.external_links.is_some(),
            self.status.is_some(),
            self.kanban_column.is_some(),
            self.kind.is_some(),
            self.pinned.is_some(),
            self.waits(),
            self.due.is_some(),
            self.start.is_some(),
            self.priority.is_some(),
            self.estimate_minutes.is_some(),
            self.assignee.is_some(),
        ]
        .iter()
        .filter(|set| **set)
        .count()
    }

    /// The label the app gives the same edit when it is only one edit, and
    /// "Edit Task" for a combination the app has no single action for.
    fn label(&self) -> &'static str {
        if self.changes() > 1 {
            return "Edit Task";
        }
        if self.status.is_some() {
            return "Change Status";
        }
        if self.kanban_column.is_some() {
            return "Move Task";
        }
        if self.waits() {
            return "Waiting On";
        }
        if self.due.is_some() {
            return "Set Due Date";
        }
        if self.start.is_some() {
            return "Schedule Task";
        }
        if self.priority.is_some() {
            return "Set Priority";
        }
        if self.assignee.is_some() {
            return "Assign Task";
        }
        match (self.kind.as_deref(), self.pinned) {
            (Some("list"), _) => "Convert to List",
            (Some(_), _) => "Convert to Task",
            (_, Some(true)) => "Promote List",
            (_, Some(false)) => "Unpin List",
            _ => "Edit Task",
        }
    }
}

pub struct NewTask {
    pub list_id: String,
    pub title: String,
    pub parent_task_id: Option<String>,
    pub notes: String,
    pub external_links: Vec<String>,
    pub kanban_column: Option<String>,
    pub kind: String,
    pub at_top: bool,
}

impl Workspace {
    // -- reading -------------------------------------------------------------

    /// Folders, lists and nested lists: everything the sidebar shows, flat,
    /// with the ids that connect them.
    pub fn tree(&self, include_archived: bool) -> Result<Value> {
        let connection = self.open()?;
        let workspace = workspace_row(&connection)?;

        let folders = collect(
            &connection,
            "SELECT id, name, parentFolderId, sortOrder FROM list_folders \
             WHERE workspaceId = ?1 ORDER BY sortOrder, createdAt, id",
            [&workspace.0],
            |row| {
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "name": row.get::<_, String>(1)?,
                    "parent_folder_id": row.get::<_, Option<String>>(2)?,
                    "sort_order": row.get::<_, i64>(3)?,
                }))
            },
        )?;

        let archived = if include_archived {
            ""
        } else {
            "AND isArchived = 0"
        };
        let lists = collect(
            &connection,
            &format!(
                "SELECT {LIST_COLUMNS}, \
                 (SELECT COUNT(*) FROM tasks t WHERE t.listId = task_lists.id AND t.status = 'open') \
                 FROM task_lists WHERE workspaceId = ?1 {archived} \
                 ORDER BY sortOrder, createdAt, id"
            ),
            [&workspace.0],
            |row| {
                let mut list = ListRow::from_row(row)?.to_json();
                list["open_task_count"] = json!(row.get::<_, i64>(10)?);
                Ok(list)
            },
        )?;
        let shared = shared_lists(&connection)?;
        let priorities = list_priorities(&connection, &workspace.0)?;
        let names = conditions::condition_names(&connection)?;
        let list_requires = |id: &str| -> Result<Value> {
            let groups = takt_core::conditions::list_requirement_groups(&connection, id)
                .map_err(map_core_error)?;
            Ok(if groups.is_empty() {
                Value::Null
            } else {
                json!(conditions::named_groups(&names, groups))
            })
        };
        let mut lists = lists;
        for list in &mut lists {
            let id = list["id"].as_str().unwrap_or_default().to_owned();
            list["priority_rank"] = json!(priorities.get(&id));
            // What every task in it requires; absent for nothing.
            let requires = list_requires(&id)?;
            if !requires.is_null() {
                list["requires"] = requires;
            }
            list["shared"] = shared
                .iter()
                .find(|shared| shared.list_id == id)
                .map_or(Value::Null, shared_summary);
        }

        // A nested list is a task with `itemKind = 'list'`, other than a
        // list's own visible root, which the sidebar shows as the list itself.
        let nested = collect(
            &connection,
            &format!(
                "SELECT t.id, t.title, t.listId, t.parentTaskId, t.status, t.isPromoted, \
                 t.archivedAt FROM tasks t JOIN task_lists l ON l.id = t.listId \
                 WHERE l.workspaceId = ?1 AND t.itemKind = 'list' \
                 AND t.id IS NOT l.visibleRootTaskId {archived} \
                 ORDER BY t.listId, t.sortOrder, t.createdAt, t.id",
                archived = if include_archived {
                    ""
                } else {
                    "AND l.isArchived = 0 AND t.archivedAt IS NULL"
                }
            ),
            [&workspace.0],
            |row| {
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "title": row.get::<_, String>(1)?,
                    "list_id": row.get::<_, String>(2)?,
                    "parent_task_id": row.get::<_, Option<String>>(3)?,
                    "status": row.get::<_, String>(4)?,
                    "is_promoted": row.get::<_, Option<bool>>(5)?.unwrap_or(false),
                    "archived_at": local_string(&row.get::<_, Option<String>>(6)?),
                }))
            },
        )?;

        let mut nested = nested;
        for list in &mut nested {
            let id = list["id"].as_str().unwrap_or_default().to_owned();
            let requires = list_requires(&id)?;
            if !requires.is_null() {
                list["requires"] = requires;
            }
        }

        Ok(json!({
            "workspace_id": workspace.0,
            "workspace": workspace.1,
            "folders": folders,
            "lists": lists,
            "nested_lists": nested,
        }))
    }

    /// The lists shared with this account or by it, with their members:
    /// what the app last heard from the sync server. The CLI never talks to
    /// the server itself, so it can't share, invite or leave; the apps do.
    pub fn shared_lists(&self) -> Result<Value> {
        let connection = self.open()?;
        let lists: Vec<Value> = shared_lists(&connection)?
            .iter()
            .map(|shared| {
                let mut summary = shared_summary(shared);
                summary["list_id"] = json!(shared.list_id);
                summary["name"] = json!(shared.name);
                summary
            })
            .collect();
        Ok(json!({ "shared_lists": lists }))
    }

    /// One list's tasks as a tree, children nested under their parents.
    pub fn tasks(
        &self,
        list_id: &str,
        parent_task_id: Option<&str>,
        include_closed: bool,
    ) -> Result<Value> {
        let connection = self.open()?;
        let list = list_row(&connection, list_id)?;
        let rows = task_rows(&connection, "t.listId = ?1", [list_id])?;
        if let Some(parent) = parent_task_id
            && !rows.iter().any(|row| row.id == parent)
        {
            return Err(ToolError::new(format!(
                "No task {parent} in list {list_id}."
            )));
        }

        let mut children: HashMap<Option<&str>, Vec<&TaskRow>> = HashMap::new();
        for row in &rows {
            children
                .entry(row.parent_task_id.as_deref())
                .or_default()
                .push(row);
        }
        let blocked: HashSet<String> = takt_core::dependencies::blocked_task_ids(&connection)
            .map_err(map_core_error)?
            .into_iter()
            .collect();
        // A blocked task sorts after its unblocked siblings, otherwise keeping
        // its place, as every client draws it (`dependencies::BLOCKED_LAST`).
        for siblings in children.values_mut() {
            siblings.sort_by_key(|row| blocked.contains(&row.id));
        }
        let mut visited = HashSet::new();
        let mut shown = 0;
        let tree = build_tree(
            parent_task_id,
            &children,
            &TreeOptions {
                include_closed,
                blocked: &blocked,
            },
            &mut visited,
            &mut shown,
        );

        let mut tree = tree;
        if has_migration(&connection, AUTHORSHIP_MIGRATION)? {
            for node in &mut tree {
                attach_sharing(&connection, node)?;
            }
        }
        Ok(json!({
            "list": list.to_json(),
            "parent_task_id": parent_task_id,
            "task_count": shown,
            "tasks": tree,
        }))
    }

    /// The list a task belongs to, so a caller adding a subtask need only
    /// name its parent.
    pub fn list_of_task(&self, task_id: &str) -> Result<String> {
        Ok(task_row(&self.open()?, task_id)?.list_id)
    }

    // -- writing -------------------------------------------------------------

    /// `WorkspaceStore.createTask`, with the notes, links and column the
    /// editor would otherwise add in a second step folded into the one undo
    /// group.
    pub fn add_task(&self, new: NewTask) -> Result<Value> {
        let title = non_empty(&new.title, "title")?;
        let column = normalized_column(new.kanban_column.as_deref());
        let links = normalized_links(&new.external_links);
        if !matches!(new.kind.as_str(), "task" | "list") {
            return Err(ToolError::new("kind must be 'task' or 'list'."));
        }
        let label = if new.kind == "list" {
            "New Nested List"
        } else {
            "New Task"
        };
        self.journalled(label, |tx, _now| {
            list_row(tx, &new.list_id)?;
            if let Some(parent) = new.parent_task_id.as_deref() {
                let parent = task_row(tx, parent)?;
                if parent.list_id != new.list_id {
                    return Err(ToolError::new(format!(
                        "Task {} is in list {}, not {}. Pass the parent's own list, or omit list_id.",
                        parent.id, parent.list_id, new.list_id
                    )));
                }
            }
            // The write is the Rust core's, shared with the apps.
            let id = takt_core::tasks::create_task(
                tx,
                &takt_core::tasks::NewTask {
                    list_id: new.list_id.clone(),
                    title: title.clone(),
                    parent_task_id: new.parent_task_id.clone(),
                    kind: new.kind.clone(),
                    notes: new.notes.clone(),
                    kanban_column: column.clone(),
                    external_links: links.clone(),
                    at_top: new.at_top,
                    ..Default::default()
                },
                Utc::now().timestamp_millis(),
            )
            .map_err(map_core_error)?;
            Ok(task_row(tx, &id)?.to_json())
        })
    }

    /// The editor's save (`updateTaskRecord`, `updateEditorMetadata`),
    /// `setStatus` and `setKanbanColumn`, as one undo step. Only fields that
    /// actually change are written, so a no-op call leaves no undo step.
    pub fn update_task(&self, task_id: &str, edit: TaskEdit) -> Result<Value> {
        if edit.is_empty() {
            return Err(ToolError::new(
                "No updates provided. Pass title, notes, external_links, status, kanban_column, \
                 kind, pinned, waiting_on, follow_up_at, follow_up_every_days, due, start, priority, \
                 estimate_minutes and/or assignee.",
            ));
        }
        let title = edit
            .title
            .as_deref()
            .map(|title| non_empty(title, "title"))
            .transpose()?;
        if let Some(status) = edit.status.as_deref()
            && !matches!(status, "open" | "completed" | "cancelled")
        {
            return Err(ToolError::new(
                "status must be 'open', 'completed' or 'cancelled'.",
            ));
        }
        if let Some(kind) = edit.kind.as_deref()
            && !matches!(kind, "task" | "list")
        {
            return Err(ToolError::new("kind must be 'task' or 'list'."));
        }
        let follow_up_at = match edit.follow_up_at.as_ref() {
            Some(Some(text)) if !text.trim().is_empty() => Some(Some(parse_follow_up(text)?)),
            Some(_) => Some(None),
            None => None,
        };
        let every_days = match edit.follow_up_every_days {
            Some(Some(days)) => Some(Some(
                u32::try_from(days)
                    .ok()
                    .filter(|days| {
                        (takt_core::waiting::MINIMUM_EVERY_DAYS
                            ..=takt_core::waiting::MAXIMUM_EVERY_DAYS)
                            .contains(days)
                    })
                    .ok_or_else(|| {
                        ToolError::new(format!(
                            "follow_up_every_days must be 1 to 365, not {days}. Null makes it once."
                        ))
                    })?,
            )),
            Some(None) => Some(None),
            None => None,
        };
        let due = match edit.due.as_ref() {
            Some(Some(text)) if !text.trim().is_empty() => Some(Some(parse_when(text, "due")?)),
            Some(_) => Some(None),
            None => None,
        };
        let start = match edit.start.as_ref() {
            Some(Some(text)) if !text.trim().is_empty() => {
                Some(Some(parse_when(text, "start")?.start_ms()))
            }
            Some(_) => Some(None),
            None => None,
        };
        let priority = match edit.priority {
            // 0 is the app's own "clear the priority" key.
            Some(Some(0) | None) => Some(None),
            Some(Some(level @ (1..=4 | -4..=-1))) => Some(Some(level)),
            Some(Some(other)) => {
                return Err(ToolError::new(format!(
                    "priority must be 1 (urgent), 2 (high), 3 (medium) or 4 (low), -4 to -1 to \
                     send it behind every task with none (-1 last of all), or null to clear it, \
                     not {other}."
                )));
            }
            None => None,
        };
        let estimate_seconds = match edit.estimate_minutes {
            Some(Some(0) | None) => Some(None),
            Some(Some(minutes)) if minutes > 0 => Some(Some(minutes * 60)),
            Some(Some(other)) => {
                return Err(ToolError::new(format!(
                    "estimate_minutes must be a positive number of minutes, or null to clear \
                     it, not {other}."
                )));
            }
            None => None,
        };
        let label = edit.label();

        self.journalled(label, |tx, _now| {
            let task = task_row(tx, task_id)?;
            // Every write here is the Rust core's, shared with the apps.
            let now_ms = Utc::now().timestamp_millis();
            let zone = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into());
            let stored_ms = |text: Option<&str>| {
                text.and_then(takt_core::time::parse_stored)
                    .map(|at| at.timestamp_millis())
            };

            // Moving the start and the deadline together: the start goes
            // last, so neither is checked against the other's old value.
            if start.is_some() && due.is_some() && task.start_at.is_some() {
                takt_core::editor::schedule_task(tx, &task.id, None, now_ms, &zone)
                    .map_err(map_core_error)?;
            }

            let due_at = match &due {
                Some(Some(When::At(at))) => Some(*at),
                _ => None,
            };
            if title.is_some()
                || edit.notes.is_some()
                || estimate_seconds.is_some()
                || due_at.is_some()
            {
                takt_core::editor::update_task(
                    tx,
                    &task.id,
                    title.as_deref().unwrap_or(&task.title),
                    edit.notes.as_deref().unwrap_or(&task.notes),
                    due_at.or_else(|| stored_ms(task.due_at.as_deref())),
                    estimate_seconds.unwrap_or(task.estimate_seconds),
                    now_ms,
                    &zone,
                )
                .map_err(map_core_error)?;
            }
            // A due day replaces a due time, and clearing clears both.
            match &due {
                Some(Some(When::Day(day))) => {
                    takt_core::editor::set_due_day(tx, &task.id, Some(day), now_ms, &zone)
                        .map_err(map_core_error)?;
                }
                Some(None) => {
                    takt_core::editor::set_due_day(tx, &task.id, None, now_ms, &zone)
                        .map_err(map_core_error)?;
                }
                _ => {}
            }
            if let Some(start) = start {
                takt_core::editor::schedule_task(tx, &task.id, start, now_ms, &zone)
                    .map_err(map_core_error)?;
            }

            if let Some(status) = edit.status.as_deref()
                && status != task.status
            {
                takt_core::tasks::set_status(tx, &task.id, status, now_ms, &zone)
                    .map_err(map_core_error)?;
            }

            if edit.external_links.is_some() || priority.is_some() {
                let mut metadata = takt_core::editor::snapshot(tx, &task.id)
                    .map_err(map_core_error)?
                    .metadata;
                if let Some(links) = edit.external_links.as_ref() {
                    metadata.external_links = links.clone();
                }
                if let Some(priority) = priority {
                    metadata.priority = priority;
                }
                takt_core::editor::update_editor_metadata(tx, &task.id, &metadata, now_ms)
                    .map_err(map_core_error)?;
            }

            if let Some(assignee) = &edit.assignee {
                let account = match assignee.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
                    None => None,
                    Some(who) => Some(resolve_member(tx, &task.list_id, who)?),
                };
                takt_core::assignment::assign_task(tx, &task.id, account.as_deref(), now_ms)
                    .map_err(map_core_error)?;
            }

            if edit.waits() {
                // A field left out keeps what the task already has.
                let waiting_on = match &edit.waiting_on {
                    Some(tag) => tag.clone(),
                    None => task.waiting_on.clone(),
                };
                let follow_up = match &follow_up_at {
                    Some(at) => stored_ms(at.as_deref()),
                    None => stored_ms(task.follow_up_at.as_deref()),
                };
                // Clearing the follow-up time clears its stored repeat with
                // it, unless the same call names one (which is refused below).
                let every_days = match (every_days, &follow_up_at) {
                    (Some(days), _) => days,
                    (None, Some(None)) => None,
                    (None, _) => task.follow_up_every_days,
                };
                // A repeat chases a follow-up time; on its own it has nothing
                // to repeat, and the core would drop it without a word.
                if every_days.is_some() && follow_up.is_none() {
                    return Err(ToolError::new(
                        "follow_up_every_days repeats a follow-up, and this task has no \
                         follow_up_at. Pass follow_up_at in the same call.",
                    ));
                }
                takt_core::waiting::set_waiting(
                    tx,
                    &task.id,
                    waiting_on.as_deref(),
                    follow_up,
                    every_days,
                    now_ms,
                )
                .map_err(map_core_error)?;
            }

            // After the waiting fields, which file the task in Waiting on or
            // take it out: a column named in the same call is where it goes.
            if let Some(column) = edit.kanban_column.as_ref() {
                let column = normalized_column(column.as_deref());
                let current = task_row(tx, &task.id)?.kanban_column;
                if column != current {
                    takt_core::tasks::set_kanban_column(
                        tx,
                        std::slice::from_ref(&task.id),
                        column.as_deref(),
                        now_ms,
                    )
                    .map_err(map_core_error)?;
                }
            }

            // Kind before pinning, so "make this a nested list and pin it" is
            // one call rather than two.
            let mut is_list = task.is_list();
            if let Some(kind) = edit.kind.as_deref()
                && kind != task.item_kind.as_deref().unwrap_or("task")
            {
                // Named refusals before the core's, which says only that the
                // change is not allowed.
                if exists(
                    tx,
                    "SELECT EXISTS(SELECT 1 FROM task_lists WHERE visibleRootTaskId = ?1)",
                    &task.id,
                )? {
                    return Err(ToolError::new(format!(
                        "\"{}\" is the visible root of a list, and already is that list.",
                        task.title
                    )));
                }
                if exists(
                    tx,
                    "SELECT EXISTS(SELECT 1 FROM focus_sessions \
                     WHERE activeTaskId = ?1 AND phase <> 'finished')",
                    &task.id,
                )? {
                    return Err(ToolError::new(format!(
                        "\"{}\" is the focus timer's current task. Finish the block first.",
                        task.title
                    )));
                }
                takt_core::conversions::set_item_kind(tx, &task.id, kind, now_ms)
                    .map_err(map_core_error)?;
                is_list = kind == "list";
            }

            if let Some(pinned) = edit.pinned {
                if !is_list {
                    return Err(ToolError::new(format!(
                        "\"{}\" is a task, not a nested list. Pass kind \"list\" to make it one.",
                        task.title
                    )));
                }
                takt_core::conversions::set_nested_list_promoted(tx, &task.id, pinned, now_ms)
                    .map_err(map_core_error)?;
            }

            Ok(task_row(tx, &task.id)?.to_json())
        })
    }

    /// Reparent and/or reorder. `moveTask` for the reparent, including its
    /// "a list means that list's visible root" rule, then the dense
    /// re-numbering `moveTaskWithinSiblings` does for the position.
    pub fn move_task(
        &self,
        task_id: &str,
        parent_task_id: Option<&str>,
        list_id: Option<&str>,
        position: Option<i64>,
    ) -> Result<Value> {
        if parent_task_id.is_none() && list_id.is_none() && position.is_none() {
            return Err(ToolError::new(
                "Nothing to do. Pass parent_task_id, list_id and/or position.",
            ));
        }
        if position.is_some_and(|position| position < 1) {
            return Err(ToolError::new("position must be 1 or greater."));
        }
        let label = if parent_task_id.is_none() && list_id.is_none() {
            "Reorder Task"
        } else {
            "Move Task"
        };

        self.journalled(label, |tx, _now| {
            let task = task_row(tx, task_id)?;
            let (destination_list, destination_parent) = match (parent_task_id, list_id) {
                (Some(parent), list) => {
                    let parent = task_row(tx, parent)?;
                    if let Some(list) = list
                        && list != parent.list_id
                    {
                        return Err(ToolError::new(format!(
                            "Task {} is in list {}, not {list}.",
                            parent.id, parent.list_id
                        )));
                    }
                    (parent.list_id, Some(parent.id))
                }
                (None, Some(list)) => {
                    list_row(tx, list)?;
                    (list.to_string(), visible_root_parent(tx, list)?)
                }
                (None, None) => (task.list_id.clone(), task.parent_task_id.clone()),
            };

            // The writes are the Rust core's, shared with the apps: the move
            // itself, then the CLI's `position` among the new siblings.
            let millis = Utc::now().timestamp_millis();
            takt_core::tasks::move_task(
                tx,
                &task.id,
                &destination_list,
                destination_parent.as_deref(),
                false,
                millis,
            )
            .map_err(map_core_error)?;
            if let Some(position) = position {
                let index = u32::try_from(position - 1).unwrap_or(0);
                takt_core::tasks::place_task_at(tx, &task.id, index, millis)
                    .map_err(map_core_error)?;
            }

            Ok(task_row(tx, &task.id)?.to_json())
        })
    }

    /// `WorkspaceStore.moveTaskToFolder`: the task becomes a standalone list of
    /// its own, named after it, in `folder_id` or at the top level. The task
    /// record survives as the new list's visible root, so its subtasks become
    /// the list's contents with their identities and hierarchy intact. One
    /// undo step, as in the app.
    pub fn task_to_list(&self, task_id: &str, folder_id: Option<&str>) -> Result<Value> {
        let label = if folder_id.is_some() {
            "Move Item to Folder"
        } else {
            "Move Item to Top Level"
        };
        self.journalled(label, |tx, _now| {
            let task = task_row(tx, task_id)?;
            let source = list_row(tx, &task.list_id)?;
            // Named refusal before the core's generic one.
            if source.visible_root_task_id.as_deref() == Some(task.id.as_str()) {
                return Err(ToolError::new(format!(
                    "\"{}\" is the visible root of list \"{}\" — it already is that list. \
                     Move the list with workspace_list_move instead.",
                    task.title, source.name
                )));
            }
            let moved = 1 + descendant_ids(tx, &task.id)?.len();
            // The write is the Rust core's, shared with the apps.
            let list_id = takt_core::conversions::move_task_to_folder(
                tx,
                &task.id,
                folder_id,
                Utc::now().timestamp_millis(),
            )
            .map_err(map_core_error)?;
            Ok(json!({
                "list": list_row(tx, &list_id)?.to_json(),
                "root_task": task_row(tx, &task.id)?.to_json(),
                "moved_task_count": moved,
            }))
        })
    }

    /// `WorkspaceStore.deleteTask`. The subtree goes with it, by the schema's
    /// cascade, and all of it comes back on undo.
    pub fn delete_task(&self, task_id: &str) -> Result<Value> {
        self.journalled("Delete Task", |tx, _now| {
            // The write is the Rust core's, shared with the apps.
            let deleted = takt_core::tasks::delete_task(tx, task_id).map_err(map_core_error)?;
            Ok(json!({
                "deleted": deleted.id,
                "title": deleted.title,
                "subtasks_deleted": deleted.subtasks_deleted,
            }))
        })
    }

    /// `WorkspaceStore.createFolder`.
    pub fn create_folder(&self, name: &str, parent_folder_id: Option<&str>) -> Result<Value> {
        let name = non_empty(name, "name")?;
        self.journalled("New Folder", |tx, _now| {
            let workspace = workspace_row(tx)?.0;
            // The write is the Rust core's, shared with the apps.
            let created = takt_core::lists::create_folder(
                tx,
                &workspace,
                &name,
                parent_folder_id,
                Utc::now().timestamp_millis(),
            )
            .map_err(map_core_error)?;
            Ok(json!({
                "id": created.id,
                "name": created.name,
                "parent_folder_id": parent_folder_id,
                "sort_order": created.sort_order,
            }))
        })
    }

    /// `WorkspaceStore.createList`.
    pub fn create_list(&self, name: &str, folder_id: Option<&str>) -> Result<Value> {
        let name = non_empty(name, "name")?;
        self.journalled("New List", |tx, _now| {
            let workspace = workspace_row(tx)?.0;
            // The write is the Rust core's, shared with the apps.
            let created = takt_core::lists::create_list(
                tx,
                &workspace,
                &name,
                folder_id,
                Utc::now().timestamp_millis(),
            )
            .map_err(map_core_error)?;
            Ok(list_row(tx, &created.id)?.to_json())
        })
    }

    /// `WorkspaceStore.moveList`: into a folder, or to the top level with
    /// `None`. Appended at the end of its new siblings.
    pub fn move_list(&self, list_id: &str, folder_id: Option<&str>) -> Result<Value> {
        self.journalled("Move List", |tx, _now| {
            // The write is the Rust core's, shared with the apps.
            takt_core::lists::move_list(tx, list_id, folder_id, Utc::now().timestamp_millis())
                .map_err(map_core_error)?;
            Ok(list_row(tx, list_id)?.to_json())
        })
    }

    /// `lists::set_list_priority`: puts a list at `position` among the
    /// prioritised lists (1 the most important; past the end, last), or
    /// takes it out of them with `None`. Answers with the list and the
    /// prioritised lists in their new order.
    pub fn set_list_priority(&self, list_id: &str, position: Option<i64>) -> Result<Value> {
        let label = if position.is_some() {
            "Prioritise List"
        } else {
            "Remove List Priority"
        };
        self.journalled(label, |tx, _now| {
            require_list_priority(tx)?;
            list_row(tx, list_id)?;
            // The write is the Rust core's, shared with the apps.
            takt_core::lists::set_list_priority(
                tx,
                list_id,
                position,
                Utc::now().timestamp_millis(),
            )
            .map_err(map_core_error)?;
            let workspace = workspace_row(tx)?.0;
            let ordered =
                takt_core::records::prioritised_lists(tx, &workspace).map_err(map_core_error)?;
            let mut list = list_row(tx, list_id)?.to_json();
            list["priority_rank"] = json!(
                ordered
                    .iter()
                    .find(|row| row.id == list_id)
                    .and_then(|row| row.priority_rank)
            );
            let priorities: Vec<Value> = ordered
                .iter()
                .map(|row| {
                    json!({
                        "id": row.id,
                        "name": row.name,
                        "priority_rank": row.priority_rank,
                    })
                })
                .collect();
            Ok(json!({ "list": list, "priorities": priorities }))
        })
    }

    /// `WorkspaceStore.deleteList`. Its tasks go with it, by the schema's
    /// cascade, and all of it comes back on undo. The Inbox is refused.
    pub fn delete_list(&self, list_id: &str) -> Result<Value> {
        self.journalled("Delete List", |tx, _now| {
            // The write is the Rust core's, shared with the apps.
            let deleted = takt_core::lists::delete_list(tx, list_id).map_err(map_core_error)?;
            Ok(json!({
                "deleted": deleted.id,
                "name": deleted.name,
                "tasks_deleted": deleted.tasks_deleted,
            }))
        })
    }

    // -- dependencies --------------------------------------------------------

    /// What `task_id` waits on and what waits on it, and whether it is
    /// blocked now. A database without the table answers with empty lists.
    pub fn task_dependencies(&self, task_id: &str) -> Result<Value> {
        dependencies_json(&self.open()?, task_id)
    }

    /// `dependencies::add_blocker`, as one "Add Blocker" step, the app's
    /// label for the same edit. Adding an edge that is already there changes
    /// nothing and leaves no undo step. A cycle is refused.
    pub fn add_blocker(&self, task_id: &str, blocked_by_task_id: &str) -> Result<Value> {
        self.journalled("Add Blocker", |tx, _now| {
            require_dependencies(tx)?;
            // The write is the Rust core's, shared with the apps.
            let changed = takt_core::dependencies::add_blocker(
                tx,
                task_id,
                blocked_by_task_id,
                Utc::now().timestamp_millis(),
            )
            .map_err(map_core_error)?;
            let mut value = dependencies_json(tx, task_id)?;
            value["changed"] = json!(changed);
            Ok(value)
        })
    }

    /// `dependencies::remove_blocker`, as one "Remove Blocker" step. Removing
    /// an edge that is not there changes nothing.
    pub fn remove_blocker(&self, task_id: &str, blocked_by_task_id: &str) -> Result<Value> {
        self.journalled("Remove Blocker", |tx, _now| {
            require_dependencies(tx)?;
            task_row(tx, task_id)?;
            task_row(tx, blocked_by_task_id)?;
            // The write is the Rust core's, shared with the apps.
            let changed = takt_core::dependencies::remove_blocker(tx, task_id, blocked_by_task_id)
                .map_err(map_core_error)?;
            let mut value = dependencies_json(tx, task_id)?;
            value["changed"] = json!(changed);
            Ok(value)
        })
    }

    /// `commitments::save_commitment`, as one "Commit to Task" step: a
    /// subtask of `task_id` carrying a habit, which lands in Today on its
    /// schedule until the task is done. The title defaults as the app's
    /// form does ("Work on …"), and a missed appearance vanishes unless
    /// `stacks_missed`.
    pub fn commit_to_task(
        &self,
        task_id: &str,
        title: Option<&str>,
        rule: &str,
        stacks_missed: bool,
    ) -> Result<Value> {
        self.journalled("Commit to Task", |tx, _now| {
            let task = task_row(tx, task_id)?;
            let now_ms = Utc::now().timestamp_millis();
            let zone = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into());
            let draft = takt_core::commitments::CommitmentDraft {
                title: title
                    .map(str::to_string)
                    .unwrap_or_else(|| takt_core::commitments::default_title(&task.title)),
                rule: rule.to_string(),
                expires_at_ms: None,
                stacks_missed,
            };
            // The write is the Rust core's, shared with the apps.
            let id =
                takt_core::commitments::save_commitment(tx, &task.id, &draft, None, now_ms, &zone)
                    .map_err(map_core_error)?;
            let commitment = takt_core::commitments::commitment(tx, &id)
                .map_err(map_core_error)?
                .ok_or_else(|| ToolError::new("The commitment was not saved."))?;
            Ok(json!({
                "task_id": commitment.task_id,
                "parent_task_id": commitment.parent_task_id,
                "daily_id": commitment.daily_id,
                "title": commitment.title,
                "rule": commitment.rule,
                "stacks_missed": commitment.stacks_missed,
            }))
        })
    }

    // -- the journal ---------------------------------------------------------

    /// Runs one write as one undoable step: `journalledWrite` in
    /// `WorkspaceStore+Undo.swift`, statement for statement.
    ///
    /// A failure anywhere inside drops the transaction, which rolls back the
    /// rows, the journal entries and the armed `undo_control` together, so a
    /// refused write leaves nothing behind — not even a disarmed flag.
    fn journalled<T>(
        &self,
        label: &str,
        work: impl FnOnce(&Transaction, &str) -> Result<T>,
    ) -> Result<T> {
        let mut connection = self.open_for_writing()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_write_error)?;
        let now = stored_string(Utc::now());
        // The journal's bookkeeping is the Rust core's (core/src/journal.rs),
        // the same code the app's undo replays these steps with.
        let group = takt_core::journal::begin(&tx, &format!("{LABEL_PREFIX}{label}"))
            .map_err(map_core_error)?;
        let result = work(&tx, &now)?;
        let changed = takt_core::journal::finish(&tx, &group).map_err(map_core_error)?;
        tx.commit().map_err(map_write_error)?;
        if changed {
            LAST_STEP.with(|step| *step.borrow_mut() = Some(group));
        }
        Ok(result)
    }

    fn open_for_writing(&self) -> Result<Connection> {
        if !self.database_path.exists() {
            return Err(ToolError::new(format!(
                "No workspace database at {}. Open Takt once, or set database_path.",
                self.database_path.display()
            )));
        }
        // No CREATE flag: a missing database is reported above, never made.
        let connection = Connection::open_with_flags(
            &self.database_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_URI,
        )
        .map_err(|error| {
            ToolError::new(format!(
                "Could not open {} for writing: {error}",
                self.database_path.display()
            ))
        })?;
        connection
            .busy_timeout(BUSY_TIMEOUT)
            .map_err(map_write_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON")
            .map_err(map_write_error)?;
        // An error here (no `grdb_migrations` table at all) is the same
        // answer as a missing row: this is not a database the app has made.
        let migrated = has_migration(&connection, REQUIRED_MIGRATION).unwrap_or(false);
        if !migrated {
            return Err(ToolError::new(format!(
                "The workspace at {} predates the schema this build of takt writes \
                 ({REQUIRED_MIGRATION}). Open an up-to-date Takt once to migrate it.",
                self.database_path.display()
            )));
        }
        Ok(connection)
    }
}

// -- row helpers --------------------------------------------------------------

/// What [`build_tree`] needs besides the rows.
struct TreeOptions<'a> {
    include_closed: bool,
    /// `dependencies::blocked_task_ids`: open tasks waiting on an open task.
    blocked: &'a HashSet<String>,
}

fn build_tree(
    parent: Option<&str>,
    children: &HashMap<Option<&str>, Vec<&TaskRow>>,
    options: &TreeOptions,
    visited: &mut HashSet<String>,
    shown: &mut usize,
) -> Vec<Value> {
    let mut nodes = Vec::new();
    for row in children.get(&parent).map(Vec::as_slice).unwrap_or_default() {
        // A closed task hides its subtree, as the app's "hide completed" does.
        if (!options.include_closed && row.status != "open") || !visited.insert(row.id.clone()) {
            continue;
        }
        *shown += 1;
        let mut node = row.to_json();
        // Only when set, like the other flags.
        if options.blocked.contains(&row.id) {
            node["blocked"] = json!(true);
        }
        let nested = build_tree(Some(&row.id), children, options, visited, shown);
        if !nested.is_empty() {
            node["children"] = json!(nested);
        }
        nodes.push(node);
    }
    nodes
}

fn exists(connection: &Connection, sql: &str, id: &str) -> Result<bool> {
    connection
        .query_row(sql, [id], |row| row.get(0))
        .map_err(map_query_error)
}

fn collect<P: rusqlite::Params>(
    connection: &Connection,
    sql: &str,
    params: P,
    map: impl FnMut(&Row) -> rusqlite::Result<Value>,
) -> Result<Vec<Value>> {
    let mut statement = connection.prepare(sql).map_err(map_query_error)?;
    let rows = statement
        .query_map(params, map)
        .map_err(map_query_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_query_error)?;
    Ok(rows)
}

fn task_rows<P: rusqlite::Params>(
    connection: &Connection,
    filter: &str,
    params: P,
) -> Result<Vec<TaskRow>> {
    let waiting = if has_migration(connection, WAITING_MIGRATION)? {
        WAITING_COLUMNS
    } else {
        NO_WAITING_COLUMNS
    };
    let repeat = if has_migration(connection, REPEAT_MIGRATION)? {
        REPEAT_COLUMNS
    } else {
        NO_REPEAT_COLUMNS
    };
    let mut statement = connection
        .prepare(&format!(
            "SELECT {TASK_COLUMNS}, {waiting}, m.planningJSON, m.priority, m.startAt, {repeat} FROM tasks t \
             LEFT JOIN task_metadata m ON m.taskId = t.id \
             WHERE {filter} ORDER BY t.sortOrder, t.createdAt, t.id"
        ))
        .map_err(map_query_error)?;
    let mut rows = statement
        .query_map(params, TaskRow::from_row)
        .map_err(map_query_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_query_error)?;
    conditions::name_requirements(connection, &mut rows)?;
    Ok(rows)
}

fn task_row(connection: &Connection, id: &str) -> Result<TaskRow> {
    task_rows(connection, "t.id = ?1", [id])?
        .into_iter()
        .next()
        .ok_or_else(|| ToolError::new(format!("No task with id {id}.")))
}

fn list_row(connection: &Connection, id: &str) -> Result<ListRow> {
    connection
        .query_row(
            &format!("SELECT {LIST_COLUMNS} FROM task_lists WHERE id = ?1"),
            [id],
            ListRow::from_row,
        )
        .optional()
        .map_err(map_query_error)?
        .ok_or_else(|| {
            ToolError::new(format!(
                "No list with id {id}. workspace_tree lists the lists and their ids."
            ))
        })
}

/// The workspace there is. The app makes exactly one. The Rust core's
/// `records::workspaces`, oldest first.
fn workspace_row(connection: &Connection) -> Result<(String, String)> {
    takt_core::records::workspaces(connection)
        .map_err(map_core_error)?
        .into_iter()
        .next()
        .map(|workspace| (workspace.id, workspace.name))
        .ok_or_else(|| {
            ToolError::new("The workspace database has no workspace yet. Open Takt once.")
        })
}

/// `visibleRootParentTaskID`: the imported wrapper whose children the app
/// shows as the list's top level, while it is still the only root. The Rust
/// core's `records::visible_root_parent`, after the list's own check.
fn visible_root_parent(connection: &Connection, list_id: &str) -> Result<Option<String>> {
    list_row(connection, list_id)?;
    takt_core::records::visible_root_parent(connection, list_id).map_err(map_core_error)
}

fn descendant_ids(connection: &Connection, task_id: &str) -> Result<HashSet<String>> {
    let ids = connection
        .prepare(
            "WITH RECURSIVE subtree(id) AS ( \
               SELECT id FROM tasks WHERE parentTaskId = ?1 \
               UNION SELECT t.id FROM tasks t JOIN subtree s ON t.parentTaskId = s.id) \
             SELECT id FROM subtree",
        )
        .and_then(|mut statement| {
            statement
                .query_map([task_id], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<HashSet<_>>>()
        })
        .map_err(map_query_error)?;
    Ok(ids)
}

/// A task's blockers and the tasks it blocks, each with its list's name.
fn dependencies_json(connection: &Connection, task_id: &str) -> Result<Value> {
    let task = task_row(connection, task_id)?;
    let blockers =
        takt_core::dependencies::blockers(connection, task_id).map_err(map_core_error)?;
    let blocking =
        takt_core::dependencies::blocked_by(connection, task_id).map_err(map_core_error)?;
    let blocked =
        takt_core::dependencies::is_blocked(connection, task_id).map_err(map_core_error)?;

    let mut list_names: HashMap<String, String> = HashMap::new();
    let mut summary = |row: &takt_core::records::TaskRow| -> Result<Value> {
        if !list_names.contains_key(&row.list_id) {
            let name = list_row(connection, &row.list_id)?.name;
            list_names.insert(row.list_id.clone(), name);
        }
        Ok(json!({
            "id": row.id,
            "title": row.title,
            "status": row.status,
            "list_id": row.list_id,
            "list_name": list_names[&row.list_id],
        }))
    };
    let blockers = blockers
        .iter()
        .map(&mut summary)
        .collect::<Result<Vec<_>>>()?;
    let blocking = blocking
        .iter()
        .map(&mut summary)
        .collect::<Result<Vec<_>>>()?;

    let mut task_json = task.to_json();
    if blocked {
        task_json["blocked"] = json!(true);
    }
    Ok(json!({
        "task": task_json,
        "blocked": blocked,
        "blocked_by": blockers,
        "blocking": blocking,
    }))
}

/// Refuses a dependency write on a database the app has not migrated to
/// `task_dependencies` yet, rather than failing on the missing table.
fn require_dependencies(connection: &Connection) -> Result<()> {
    if has_migration(connection, DEPENDENCIES_MIGRATION)? {
        return Ok(());
    }
    Err(ToolError::new(format!(
        "This workspace predates task dependencies ({DEPENDENCIES_MIGRATION}). \
         Open an up-to-date Takt once to migrate it."
    )))
}

fn require_list_priority(connection: &Connection) -> Result<()> {
    if has_migration(connection, LIST_PRIORITY_MIGRATION)? {
        return Ok(());
    }
    Err(ToolError::new(format!(
        "This workspace predates list priorities ({LIST_PRIORITY_MIGRATION}). \
         Open an up-to-date Takt once to migrate it."
    )))
}

/// Each prioritised list's place, 1 the most important. Empty for a
/// workspace from before list priorities: the CLI reads without migrating.
fn list_priorities(connection: &Connection, workspace_id: &str) -> Result<HashMap<String, i64>> {
    if !has_migration(connection, LIST_PRIORITY_MIGRATION)? {
        return Ok(HashMap::new());
    }
    Ok(
        takt_core::records::prioritised_lists(connection, workspace_id)
            .map_err(map_core_error)?
            .into_iter()
            .filter_map(|row| Some((row.id, row.priority_rank?)))
            .collect(),
    )
}

// -- values -------------------------------------------------------------------

/// Whether the app has run `identifier` on this database.
fn has_migration(connection: &Connection, identifier: &str) -> Result<bool> {
    exists(
        connection,
        "SELECT EXISTS(SELECT 1 FROM grdb_migrations WHERE identifier = ?1)",
        identifier,
    )
}

/// A follow-up time as stored: UTC, whole minutes. Takes `2026-10-08 14:00`
/// or `2026-10-08T14:00` in local time, or an RFC 3339 instant.
fn parse_follow_up(text: &str) -> Result<String> {
    let text = text.trim();
    let instant = parse_instant(text).ok_or_else(|| {
        ToolError::new(format!(
            "follow_up_at must be a date and time such as 2026-10-08 14:00, not \"{text}\"."
        ))
    })?;
    Ok(stored_string(instant))
}

/// A moment, to the minute: `2026-10-08 14:00` or `2026-10-08T14:00` in
/// local time, or an RFC 3339 instant.
fn parse_instant(text: &str) -> Option<chrono::DateTime<Utc>> {
    use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Timelike};
    let text = text.trim();
    let instant = DateTime::parse_from_rfc3339(text)
        .map(|date| date.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            [
                "%Y-%m-%d %H:%M",
                "%Y-%m-%dT%H:%M",
                "%Y-%m-%d %H:%M:%S",
                "%Y-%m-%dT%H:%M:%S",
            ]
            .iter()
            .find_map(|format| NaiveDateTime::parse_from_str(text, format).ok())
            .and_then(|naive| Local.from_local_datetime(&naive).earliest())
            .map(|local| local.with_timezone(&Utc))
        })?;
    Some(
        instant
            .with_second(0)
            .and_then(|date| date.with_nanosecond(0))
            .unwrap_or(instant),
    )
}

/// A due or start value: a whole day, or a moment.
enum When {
    /// `yyyy-MM-dd`.
    Day(String),
    /// Unix milliseconds, whole minutes.
    At(i64),
}

impl When {
    /// A start on a day starts at the beginning of it, local time.
    fn start_ms(&self) -> i64 {
        use chrono::{Local, NaiveDate, TimeZone};
        match self {
            When::At(at) => *at,
            When::Day(day) => NaiveDate::parse_from_str(day, "%Y-%m-%d")
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .and_then(|midnight| Local.from_local_datetime(&midnight).earliest())
                .map(|local| local.timestamp_millis())
                .unwrap_or_default(),
        }
    }
}

/// `2026-10-17` is a day; anything [`parse_follow_up`] reads is a moment.
fn parse_when(text: &str, field: &str) -> Result<When> {
    let text = text.trim();
    if let Ok(day) = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Ok(When::Day(day.format("%Y-%m-%d").to_string()));
    }
    parse_instant(text)
        .map(|at| When::At(at.timestamp_millis()))
        .ok_or_else(|| {
            ToolError::new(format!(
                "{field} must be a date such as 2026-10-17, or a date and time such as \
                 2026-10-17 17:00, not \"{text}\". Work out a phrase like \"friday\" from today's \
                 date first."
            ))
        })
}

/// `WorkspaceStore.nonEmptyName`.
fn non_empty(raw: &str, field: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ToolError::new(format!("{field} must not be empty.")));
    }
    Ok(trimmed.to_string())
}

/// `WorkspaceStore.normalizedStrings`: trimmed, blanks dropped, and the first
/// of any case-insensitive duplicates kept.
fn normalized_links(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty() && seen.insert(value.to_lowercase()))
        .map(str::to_string)
        .collect()
}

fn decode_links(json: Option<&str>) -> Vec<String> {
    let values: Vec<String> = json
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    normalized_links(&values)
}

/// `setKanbanColumn`'s trim-to-nil.
fn normalized_column(column: Option<&str>) -> Option<String> {
    column
        .map(str::trim)
        .filter(|column| !column.is_empty())
        .map(str::to_string)
}

fn map_core_error(error: takt_core::CoreError) -> ToolError {
    match error {
        takt_core::CoreError::NoJournal => ToolError::new(
            "The workspace has no undo journal to record into. Open Takt once, then try again.",
        ),
        missing @ takt_core::CoreError::MissingTask { .. } => ToolError::new(missing.to_string()),
        takt_core::CoreError::MissingList { id } => ToolError::new(format!(
            "No list with id {id}. workspace_tree lists the lists and their ids."
        )),
        takt_core::CoreError::MissingFolder { id } => ToolError::new(format!(
            "No folder with id {id}. workspace_tree lists the folders and their ids."
        )),
        takt_core::CoreError::MissingFilterList { id } => ToolError::new(format!(
            "No filter list with id {id}. workspace_filter_lists lists them and their ids."
        )),
        takt_core::CoreError::InvalidFilter { detail } => ToolError::new(detail),
        refused @ (takt_core::CoreError::InvalidFolderMove
        | takt_core::CoreError::InvalidTaskMove
        | takt_core::CoreError::DependencyCycle
        | takt_core::CoreError::InvalidSchedule
        | takt_core::CoreError::InvalidDate
        | takt_core::CoreError::EstimateRequired
        | takt_core::CoreError::EmptyName) => ToolError::new(refused.to_string()),
        permanent @ takt_core::CoreError::SystemListIsPermanent => {
            ToolError::new(permanent.to_string())
        }
        other => ToolError::new(format!("Workspace write failed: {other}")),
    }
}

fn map_write_error(error: rusqlite::Error) -> ToolError {
    let busy = matches!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
    );
    if busy {
        return ToolError::new(format!(
            "The workspace stayed locked for {}s — Takt is in the middle of a long write. \
             Nothing was changed; try again.",
            BUSY_TIMEOUT.as_secs()
        ));
    }
    ToolError::new(format!("Workspace write failed: {error}"))
}

/// The shared lists this database knows of; none in one from before v37.
fn shared_lists(connection: &Connection) -> Result<Vec<takt_core::sync::spaces::SharedList>> {
    let has_table: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'sync_spaces')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| ToolError::new(error.to_string()))?;
    if !has_table {
        return Ok(Vec::new());
    }
    takt_core::sync::spaces::shared_lists(connection)
        .map_err(|error| ToolError::new(error.to_string()))
}

/// A shared list's part of a list: your role in it and who else is in it.
fn shared_summary(shared: &takt_core::sync::spaces::SharedList) -> Value {
    json!({
        "role": shared.role,
        "members": shared.members.iter().map(|member| json!({
            "email": member.email,
            "role": member.role,
            "you": member.you,
        })).collect::<Vec<_>>(),
    })
}

/// A shared list's member by email, account id or "me".
fn resolve_member(connection: &Connection, list_id: &str, who: &str) -> Result<String> {
    let shared = takt_core::sync::spaces::shared_list(connection, list_id)
        .map_err(map_core_error)?
        .ok_or_else(|| ToolError::new("Only a task in a shared list can be assigned."))?;
    shared
        .members
        .iter()
        .find(|member| {
            (who.eq_ignore_ascii_case("me") && member.you)
                || member.account_id == who
                || member
                    .email
                    .as_deref()
                    .is_some_and(|email| email.eq_ignore_ascii_case(who))
        })
        .map(|member| member.account_id.clone())
        .ok_or_else(|| {
            let emails: Vec<String> = shared
                .members
                .iter()
                .filter_map(|member| member.email.clone())
                .collect();
            ToolError::new(format!(
                "{who} isn't in this list. Its people: {}.",
                emails.join(", ")
            ))
        })
}

/// Adds who added, finished and holds a task in a shared list to its node
/// and its children's: an email, or "you".
fn attach_sharing(connection: &Connection, node: &mut Value) -> Result<()> {
    if let Some(id) = node["id"].as_str().map(str::to_string)
        && let Some(sharing) =
            takt_core::assignment::task_sharing(connection, &id).map_err(map_core_error)?
        && let Some(object) = node.as_object_mut()
    {
        for (key, member) in [
            ("added_by", sharing.created_by),
            ("completed_by", sharing.completed_by),
            ("assignee", sharing.assignee),
        ] {
            if let Some(member) = member {
                object.insert(
                    key.into(),
                    json!(takt_core::assignment::shared_member_name(member)),
                );
            }
        }
    }
    if let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) {
        for child in children {
            attach_sharing(connection, child)?;
        }
    }
    Ok(())
}
