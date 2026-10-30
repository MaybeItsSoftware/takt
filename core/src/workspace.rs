//! The handle a client holds on its workspace database through UniFFI.
//!
//! One connection, opened the way every client opens the file: a five-second
//! wait for another writer and foreign keys on. It sits beside the client's
//! own connection (GRDB on Apple, androidx's driver on Android) on the same
//! SQLite library, so the two cannot disturb each other's locks. As behaviour
//! moves into the core, it moves onto this type.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use rusqlite::Connection;

use crate::CoreError;
use crate::board::{self, BoardRead};
use crate::commitments::{self, Commitment, CommitmentDraft};
use crate::conditions;
use crate::conversions;
use crate::dailies::{self, DailyEdit};
use crate::dependencies;
use crate::editor::{self, EditorMetadata, EditorSnapshot};
use crate::focus::Unavailable;
use crate::focus::{self, BlockFinished, Candidate, FocusContext};
use crate::habits::{self, HabitDraft};
use crate::imports::{self, BoardBaseline, ImportOutcome, ImportedTaskSeed, LegacyDailySeed};
use crate::journal::{self, HistoryTarget, UndoStep};
use crate::lists::{self, CreatedItem, DeletedList, ListSettings};
use crate::next_up::{self, DayEntry, NextUp};
use crate::ranking::{self, Ranking, Scored};
use crate::records::{self, FolderRow, ListRow, OutlineItem, TaskRow, WorkspaceRow};
use crate::search::{self, SearchHit};
use crate::setup;
use crate::sidebar::{self, SidebarIndex};
use crate::sync::{self, IncomingRow, LocalSyncState, PendingChanges};
use crate::tasks::{self, DeletedTask, NewTask};
use crate::today;
use crate::waiting;

/// An open workspace database.
///
/// Two connections. Every write, and most reads, go through `connection`,
/// whose `data_version` the apps poll. `reader` serves the reads a client
/// makes off its main thread (the next-up snapshot), so a main-thread read
/// does not queue behind them on one lock. It is `query_only` and never
/// holds a transaction open between calls, so under WAL each of its reads
/// sees every commit made before it began, the handle's own included, and it
/// cannot move the writer's `data_version`: reads do not commit.
#[derive(uniffi::Object)]
pub struct CoreWorkspace {
    connection: Mutex<Connection>,
    /// Absent for an in-memory database, which a second connection would not
    /// share; reads then use `connection`.
    reader: Option<Mutex<Connection>>,
}

#[uniffi::export]
impl CoreWorkspace {
    /// Opens the database at `path`, which must already be migrated
    /// ([`crate::schema::migrate_workspace`]).
    #[uniffi::constructor]
    pub fn open(path: String) -> Result<Arc<Self>, CoreError> {
        let connection = Connection::open(&path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        // WAL lets the CLI read while the app writes, and is a property of
        // the file: already set, this changes nothing.
        connection.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        connection.execute_batch("PRAGMA foreign_keys = ON")?;
        let in_memory = path.is_empty() || path == ":memory:" || path.contains("mode=memory");
        let reader = if in_memory {
            None
        } else {
            // Opened read-write but `query_only`, rather than read-only:
            // a read-only connection to a WAL file depends on the writer
            // having made the -shm file, and gains nothing here.
            let reader = Connection::open(&path)?;
            reader.busy_timeout(Duration::from_secs(5))?;
            reader.execute_batch("PRAGMA query_only = ON")?;
            Some(Mutex::new(reader))
        };
        Ok(Arc::new(Self {
            connection: Mutex::new(connection),
            reader,
        }))
    }

    /// SQLite's `data_version` on this handle's connection: it moves when
    /// another connection commits (the CLI, another process) and never for
    /// this handle's own writes, so a client that writes only through the
    /// core can poll it to learn that someone else changed the file.
    pub fn data_version(&self) -> Result<i64, CoreError> {
        Ok(self
            .lock()
            .query_row("PRAGMA data_version", [], |row| row.get(0))?)
    }

    /// How many rows this handle's own writes have changed since it opened:
    /// SQLite's `total_changes()` on the writing connection. It moves for
    /// every write of the handle's, where `data_version` moves for everyone
    /// else's, so the two together tell a client the file has changed since
    /// it last looked, whoever changed it.
    pub fn own_changes(&self) -> Result<i64, CoreError> {
        Ok(self
            .lock()
            .query_row("SELECT total_changes()", [], |row| row.get(0))?)
    }

    /// Reverses the most recent step; returns its label, or nothing when there
    /// was nothing to undo.
    pub fn undo(&self) -> Result<Option<String>, CoreError> {
        journal::undo(&mut self.lock())
    }

    /// Puts back the most recently undone step.
    pub fn redo(&self) -> Result<Option<String>, CoreError> {
        journal::redo(&mut self.lock())
    }

    /// What undo would take back, phrased for a menu item.
    pub fn undoable_label(&self) -> Result<Option<String>, CoreError> {
        journal::undoable_label(&self.lock())
    }

    /// What redo would put back.
    pub fn redoable_label(&self) -> Result<Option<String>, CoreError> {
        journal::redoable_label(&self.lock())
    }

    /// The journal's named steps, newest first.
    pub fn undo_history(&self, limit: u32) -> Result<Vec<UndoStep>, CoreError> {
        journal::history(&self.lock(), limit)
    }

    /// The task and list the next undo (`for_undo`) or redo affects.
    pub fn history_target(&self, for_undo: bool) -> Result<HistoryTarget, CoreError> {
        journal::history_target(&self.lock(), for_undo)
    }

    /// Creates a task as one "New Task" step and returns its id.
    pub fn create_task(&self, task: NewTask, now_ms: i64) -> Result<String, CoreError> {
        journal::journalled(&mut self.lock(), "New Task", |tx| {
            tasks::create_task(tx, &task, now_ms)
        })
    }

    /// Creates a task and the conditions it needs as one "New Task" step.
    pub fn create_task_requiring(
        &self,
        task: NewTask,
        requirement_groups: Vec<Vec<String>>,
        now_ms: i64,
        zone: String,
    ) -> Result<String, CoreError> {
        journal::journalled(&mut self.lock(), "New Task", |tx| {
            tasks::create_task_requiring(tx, &task, &requirement_groups, now_ms, &zone)
        })
    }

    /// Moves a task and its subtree as one "Move Task" step.
    pub fn move_task(
        &self,
        id: String,
        list_id: String,
        parent_task_id: Option<String>,
        to_visible_root: bool,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Move Task", |tx| {
            tasks::move_task(
                tx,
                &id,
                &list_id,
                parent_task_id.as_deref(),
                to_visible_root,
                now_ms,
            )
        })
    }

    /// Moves a task among its siblings as one "Reorder Task" step.
    pub fn move_task_within_siblings(
        &self,
        id: String,
        offset: i32,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Reorder Task", |tx| {
            tasks::move_task_within_siblings(tx, &id, offset, now_ms)
        })
    }

    /// Moves a task to the top of its siblings as one "Reorder Task" step.
    pub fn move_task_to_start(&self, id: String, now_ms: i64) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Reorder Task", |tx| {
            tasks::move_task_to_start(tx, &id, now_ms)
        })
    }

    /// Drops a task before a sibling, and into a board column, as one
    /// "Reorder Task" step.
    pub fn move_task_before(
        &self,
        id: String,
        target_id: String,
        kanban_column: Option<String>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Reorder Task", |tx| {
            tasks::move_task_before(tx, &id, &target_id, kanban_column.as_deref(), now_ms)
        })
    }

    /// Indents a task under the sibling above as one "Indent Task" step.
    pub fn indent_task(&self, id: String, now_ms: i64) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Indent Task", |tx| {
            tasks::indent_task(tx, &id, now_ms)
        })
    }

    /// Outdents a task to follow its parent as one "Outdent Task" step.
    pub fn outdent_task(&self, id: String, now_ms: i64) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Outdent Task", |tx| {
            tasks::outdent_task(tx, &id, now_ms)
        })
    }

    /// Puts tasks in a board column as one "Move Task" step.
    pub fn set_kanban_column(
        &self,
        task_ids: Vec<String>,
        column: Option<String>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Move Task", |tx| {
            tasks::set_kanban_column(tx, &task_ids, column.as_deref(), now_ms)
        })
    }

    /// Places a task on the priority matrix as one "Move Task" step.
    pub fn set_matrix_position(
        &self,
        id: String,
        urgency: Option<i64>,
        importance: Option<i64>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Move Task", |tx| {
            tasks::set_matrix_position(tx, &id, urgency, importance, now_ms)
        })
    }

    /// Places a task in a matrix quadrant at a point in its order as one
    /// "Move Task" step (`tasks::place_in_matrix`).
    pub fn place_in_matrix(
        &self,
        id: String,
        urgency: i64,
        importance: i64,
        order: Vec<String>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Move Task", |tx| {
            tasks::place_in_matrix(tx, &id, urgency, importance, &order, now_ms)
        })
    }

    /// Deletes a task and its subtree as one "Delete Task" step.
    pub fn delete_task(&self, id: String) -> Result<DeletedTask, CoreError> {
        journal::journalled(&mut self.lock(), "Delete Task", |tx| {
            tasks::delete_task(tx, &id)
        })
    }

    /// Makes `task_id` wait on `blocked_by_task_id` as one "Add Blocker" step.
    /// Returns whether it was new. A task waiting on itself, or an edge that
    /// would close a cycle, is refused (`dependencies::add_blocker`).
    pub fn add_task_blocker(
        &self,
        task_id: String,
        blocked_by_task_id: String,
        now_ms: i64,
    ) -> Result<bool, CoreError> {
        journal::journalled(&mut self.lock(), "Add Blocker", |tx| {
            dependencies::add_blocker(tx, &task_id, &blocked_by_task_id, now_ms)
        })
    }

    /// Creates `task` and makes `task_id` wait on it, as one "New Blocker"
    /// step (`dependencies::create_blocker`). Returns the new task's id.
    pub fn create_task_blocker(
        &self,
        task_id: String,
        task: NewTask,
        now_ms: i64,
    ) -> Result<String, CoreError> {
        journal::journalled(&mut self.lock(), "New Blocker", |tx| {
            dependencies::create_blocker(tx, &task_id, &task, now_ms)
        })
    }

    /// Stops `task_id` waiting on `blocked_by_task_id` as one "Remove
    /// Blocker" step. Returns whether there was such a dependency.
    pub fn remove_task_blocker(
        &self,
        task_id: String,
        blocked_by_task_id: String,
    ) -> Result<bool, CoreError> {
        journal::journalled(&mut self.lock(), "Remove Blocker", |tx| {
            dependencies::remove_blocker(tx, &task_id, &blocked_by_task_id)
        })
    }

    /// The tasks `task_id` waits on, open ones first.
    pub fn task_blockers(&self, task_id: String) -> Result<Vec<TaskRow>, CoreError> {
        dependencies::blockers(&self.lock(), &task_id)
    }

    /// The tasks waiting on `task_id`, open ones first.
    pub fn tasks_blocked_by(&self, task_id: String) -> Result<Vec<TaskRow>, CoreError> {
        dependencies::blocked_by(&self.lock(), &task_id)
    }

    /// Every open task waiting on an open task: what a client draws dimmed.
    pub fn blocked_task_ids(&self) -> Result<Vec<String>, CoreError> {
        dependencies::blocked_task_ids(&self.read())
    }

    /// Every open task whose start is still to come at `now_ms`: what a
    /// client draws greyed in the buried section (`buried`).
    pub fn buried_task_ids(&self, now_ms: i64) -> Result<Vec<String>, CoreError> {
        crate::buried::buried_task_ids(&self.read(), now_ms)
    }

    /// Whether `task_id` waits on an open task now.
    pub fn is_task_blocked(&self, task_id: String) -> Result<bool, CoreError> {
        dependencies::is_blocked(&self.lock(), &task_id)
    }

    /// The task's links, described for drawing (`links::task_links`).
    pub fn task_links(&self, task_id: String) -> Result<Vec<crate::links::TaskLink>, CoreError> {
        crate::links::task_links(&self.read(), &task_id)
    }

    /// Every task with a link: what a client marks on its rows.
    pub fn linked_task_ids(&self) -> Result<Vec<String>, CoreError> {
        crate::links::linked_task_ids(&self.read())
    }

    /// Links `url` to the task as one "Add Link" step, naming it `title`
    /// with the file type `mime_type` when the picker knew them. Returns
    /// whether the address is new; an address already there is renamed.
    pub fn add_task_link(
        &self,
        task_id: String,
        url: String,
        title: Option<String>,
        mime_type: Option<String>,
        now_ms: i64,
    ) -> Result<bool, CoreError> {
        journal::journalled(&mut self.lock(), "Add Link", |tx| {
            crate::links::add_link(
                tx,
                &task_id,
                &url,
                title.as_deref(),
                mime_type.as_deref(),
                now_ms,
            )
        })
    }

    /// Takes `url` off the task's links as one "Remove Link" step. Returns
    /// whether it was there.
    pub fn remove_task_link(
        &self,
        task_id: String,
        url: String,
        now_ms: i64,
    ) -> Result<bool, CoreError> {
        journal::journalled(&mut self.lock(), "Remove Link", |tx| {
            crate::links::remove_link(tx, &task_id, &url, now_ms)
        })
    }

    /// Deletes a list and its tasks as one "Delete List" step.
    pub fn delete_list(&self, id: String) -> Result<DeletedList, CoreError> {
        journal::journalled(&mut self.lock(), "Delete List", |tx| {
            lists::delete_list(tx, &id)
        })
    }

    /// Creates a folder as one "New Folder" step.
    pub fn create_folder(
        &self,
        workspace_id: String,
        name: String,
        parent_folder_id: Option<String>,
        now_ms: i64,
    ) -> Result<CreatedItem, CoreError> {
        journal::journalled(&mut self.lock(), "New Folder", |tx| {
            lists::create_folder(
                tx,
                &workspace_id,
                &name,
                parent_folder_id.as_deref(),
                now_ms,
            )
        })
    }

    /// Creates a list as one "New List" step.
    pub fn create_list(
        &self,
        workspace_id: String,
        name: String,
        folder_id: Option<String>,
        now_ms: i64,
    ) -> Result<CreatedItem, CoreError> {
        journal::journalled(&mut self.lock(), "New List", |tx| {
            lists::create_list(tx, &workspace_id, &name, folder_id.as_deref(), now_ms)
        })
    }

    /// Moves a folder into another (or to the top) as one "Move Folder" step.
    pub fn move_folder(
        &self,
        id: String,
        parent_folder_id: Option<String>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Move Folder", |tx| {
            lists::move_folder(tx, &id, parent_folder_id.as_deref(), now_ms)
        })
    }

    /// Moves a list into a folder (or to the top) as one "Move List" step.
    pub fn move_list(
        &self,
        id: String,
        folder_id: Option<String>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Move List", |tx| {
            lists::move_list(tx, &id, folder_id.as_deref(), now_ms)
        })
    }

    /// Moves a list among its siblings as one "Reorder List" step.
    pub fn move_list_within_folder(
        &self,
        id: String,
        offset: i32,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Reorder List", |tx| {
            lists::move_list_within_folder(tx, &id, offset, now_ms)
        })
    }

    /// Moves a folder among its siblings as one "Reorder Folder" step.
    pub fn move_folder_within_siblings(
        &self,
        id: String,
        offset: i32,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Reorder Folder", |tx| {
            lists::move_folder_within_siblings(tx, &id, offset, now_ms)
        })
    }

    /// Drops a list before another, in a folder, as one "Reorder List" step.
    pub fn place_list(
        &self,
        id: String,
        before_id: Option<String>,
        folder_id: Option<String>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Reorder List", |tx| {
            lists::place_list(tx, &id, before_id.as_deref(), folder_id.as_deref(), now_ms)
        })
    }

    /// Drops a folder before another, in a parent, as one "Reorder Folder" step.
    pub fn place_folder(
        &self,
        id: String,
        before_id: Option<String>,
        parent_folder_id: Option<String>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Reorder Folder", |tx| {
            lists::place_folder(
                tx,
                &id,
                before_id.as_deref(),
                parent_folder_id.as_deref(),
                now_ms,
            )
        })
    }

    /// Creates a condition as one "New Condition" step; returns its id.
    pub fn create_condition(
        &self,
        workspace_id: String,
        name: String,
        is_location: bool,
        now_ms: i64,
    ) -> Result<String, CoreError> {
        journal::journalled(&mut self.lock(), "New Condition", |tx| {
            conditions::create_condition(tx, &workspace_id, &name, is_location, now_ms)
        })
    }

    /// Creates a condition under a category (none for uncategorised) as one
    /// "New Condition" step; returns its id. Location makes it a place.
    pub fn create_condition_in_category(
        &self,
        workspace_id: String,
        name: String,
        category: Option<String>,
        now_ms: i64,
    ) -> Result<String, CoreError> {
        journal::journalled(&mut self.lock(), "New Condition", |tx| {
            conditions::create_condition_in_category(
                tx,
                &workspace_id,
                &name,
                category.as_deref(),
                now_ms,
            )
        })
    }

    /// Files a condition under a category, or none, as one "Edit Condition"
    /// step. Location makes it a place, anything else makes it not one.
    pub fn set_condition_category(
        &self,
        id: String,
        category: Option<String>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Edit Condition", |tx| {
            conditions::set_condition_category(tx, &id, category.as_deref(), now_ms)
        })
    }

    /// Saves a condition as one "Edit Condition" step.
    pub fn save_condition(
        &self,
        id: String,
        name: String,
        is_location: bool,
        is_archived: bool,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Edit Condition", |tx| {
            conditions::save_condition(tx, &id, &name, is_location, is_archived, now_ms)
        })
    }

    /// Sets what a condition provides, replacing what it did, as one "Edit
    /// Condition" step. False when it already provided exactly that.
    pub fn set_condition_provides(
        &self,
        id: String,
        provided_ids: Vec<String>,
        now_ms: i64,
    ) -> Result<bool, CoreError> {
        journal::journalled(&mut self.lock(), "Edit Condition", |tx| {
            conditions::set_condition_provides(tx, &id, provided_ids, now_ms)
        })
    }

    /// Moves a condition one place up (`offset` < 0) or down under its
    /// heading, as one "Move Condition" step. False at the end of the run.
    pub fn move_condition(&self, id: String, offset: i64, now_ms: i64) -> Result<bool, CoreError> {
        journal::journalled(&mut self.lock(), "Move Condition", |tx| {
            conditions::move_condition(tx, &id, offset, now_ms)
        })
    }

    /// Moves a heading — a category, or none for "Other" — one place up or
    /// down, as one "Move Category" step. False at either end.
    pub fn move_condition_category(
        &self,
        workspace_id: String,
        category: Option<String>,
        offset: i64,
        now_ms: i64,
    ) -> Result<bool, CoreError> {
        journal::journalled(&mut self.lock(), "Move Category", |tx| {
            conditions::move_condition_category(
                tx,
                &workspace_id,
                category.as_deref(),
                offset,
                now_ms,
            )
        })
    }

    /// Files every condition under `from` under `to`, as one "Rename
    /// Category" step; returns how many moved. Location cannot be renamed.
    pub fn rename_condition_category(
        &self,
        workspace_id: String,
        from: Option<String>,
        to: Option<String>,
        now_ms: i64,
    ) -> Result<u32, CoreError> {
        journal::journalled(&mut self.lock(), "Rename Category", |tx| {
            conditions::rename_condition_category(
                tx,
                &workspace_id,
                from.as_deref(),
                to.as_deref(),
                now_ms,
            )
            .map(|count| count as u32)
        })
    }

    /// Merges one condition into another as one "Merge Conditions" step:
    /// its tasks require the other, and it is archived. Returns how many
    /// tasks changed.
    pub fn merge_conditions(
        &self,
        from: String,
        into: String,
        now_ms: i64,
    ) -> Result<u32, CoreError> {
        journal::journalled(&mut self.lock(), "Merge Conditions", |tx| {
            conditions::merge_conditions(tx, &from, &into, now_ms).map(|count| count as u32)
        })
    }

    /// Sets what a task requires, as one "Edit Conditions" step. False when
    /// it already required exactly that.
    pub fn set_task_requirements(
        &self,
        task_id: String,
        groups: Vec<Vec<String>>,
        now_ms: i64,
    ) -> Result<bool, CoreError> {
        journal::journalled(&mut self.lock(), "Edit Conditions", |tx| {
            conditions::set_task_requirements(tx, &task_id, groups, now_ms)
        })
    }

    /// Saves what holds now. Not an undo step: where you are is not an edit.
    pub fn save_focus_context(
        &self,
        workspace_id: String,
        context: crate::focus_context::StoredFocusContext,
        now_ms: i64,
    ) -> Result<bool, CoreError> {
        self.unjournalled(|tx| crate::focus_context::save(tx, &workspace_id, &context, now_ms))
    }

    /// Makes a task a standalone list in a folder (or at the top); returns the list's id.
    pub fn move_task_to_folder(
        &self,
        id: String,
        folder_id: Option<String>,
        now_ms: i64,
    ) -> Result<String, CoreError> {
        let label = if folder_id.is_none() {
            "Move Item to Top Level"
        } else {
            "Move Item to Folder"
        };
        journal::journalled(&mut self.lock(), label, |tx| {
            conversions::move_task_to_folder(tx, &id, folder_id.as_deref(), now_ms)
        })
    }

    /// Turns a standalone list into a task in the Inbox; returns the task's id.
    pub fn convert_list_to_task(&self, id: String, now_ms: i64) -> Result<String, CoreError> {
        journal::journalled(&mut self.lock(), "Convert List to Task", |tx| {
            conversions::convert_list_to_task(tx, &id, now_ms)
        })
    }

    /// Nests a standalone list inside another list; returns its task's id.
    pub fn nest_list(
        &self,
        id: String,
        into_list_id: String,
        parent_task_id: Option<String>,
        now_ms: i64,
    ) -> Result<String, CoreError> {
        journal::journalled(&mut self.lock(), "Move List into List", |tx| {
            conversions::nest_list(tx, &id, &into_list_id, parent_task_id.as_deref(), now_ms)
        })
    }

    /// Makes an item a task or a nested list.
    pub fn set_item_kind(&self, id: String, kind: String, now_ms: i64) -> Result<(), CoreError> {
        let label = if kind == "list" {
            "Convert to List"
        } else {
            "Convert to Task"
        };
        journal::journalled(&mut self.lock(), label, |tx| {
            conversions::set_item_kind(tx, &id, &kind, now_ms)
        })
    }

    /// Pins a nested list to the sidebar, or unpins it.
    pub fn set_nested_list_promoted(
        &self,
        id: String,
        promoted: bool,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        let label = if promoted {
            "Promote List"
        } else {
            "Unpin List"
        };
        journal::journalled(&mut self.lock(), label, |tx| {
            conversions::set_nested_list_promoted(tx, &id, promoted, now_ms)
        })
    }

    /// Archives or restores a nested list.
    pub fn set_nested_list_archived(
        &self,
        id: String,
        archived: bool,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        let label = if archived {
            "Archive Nested List"
        } else {
            "Restore Nested List"
        };
        journal::journalled(&mut self.lock(), label, |tx| {
            conversions::set_nested_list_archived(tx, &id, archived, now_ms)
        })
    }

    /// Completes or reopens a standalone list.
    pub fn set_list_completed(
        &self,
        id: String,
        completed: bool,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        let label = if completed {
            "Complete List"
        } else {
            "Reopen List"
        };
        journal::journalled(&mut self.lock(), label, |tx| {
            conversions::set_list_completed(tx, &id, completed, now_ms)
        })
    }

    /// A task's editable state, as the editor opens it.
    pub fn editor_snapshot(&self, task_id: String) -> Result<EditorSnapshot, CoreError> {
        editor::snapshot(&self.lock(), &task_id)
    }

    /// Saves the task editor as one "Edit Task" step, refusing if the task
    /// changed since `baseline`. Returns the task as saved.
    pub fn save_editor(
        &self,
        edit: EditorSnapshot,
        baseline: EditorSnapshot,
        now_ms: i64,
        zone: String,
    ) -> Result<EditorSnapshot, CoreError> {
        journal::journalled(&mut self.lock(), "Edit Task", |tx| {
            editor::save_editor(tx, &edit, &baseline, now_ms, &zone)
        })
    }

    /// [`Self::save_editor`], keeping the deadline where it is when the start
    /// moves rather than letting it follow: the Mac's "Keep the due date".
    pub fn save_editor_keeping_due(
        &self,
        edit: EditorSnapshot,
        baseline: EditorSnapshot,
        now_ms: i64,
        zone: String,
    ) -> Result<EditorSnapshot, CoreError> {
        journal::journalled(&mut self.lock(), "Edit Task", |tx| {
            editor::save_editor_with(tx, &edit, &baseline, now_ms, &zone, true)
        })
    }

    /// Sets a task's title, notes, due time and estimate as one "Edit Task" step.
    #[allow(clippy::too_many_arguments)]
    pub fn update_task(
        &self,
        id: String,
        title: String,
        notes: String,
        due_at_ms: Option<i64>,
        estimate_seconds: Option<i64>,
        now_ms: i64,
        zone: String,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Edit Task", |tx| {
            editor::update_task(
                tx,
                &id,
                &title,
                &notes,
                due_at_ms,
                estimate_seconds,
                now_ms,
                &zone,
            )
        })
    }

    /// Sets a task's priority, tags, links and repeat as one "Edit Task Details" step.
    pub fn update_editor_metadata(
        &self,
        task_id: String,
        metadata: EditorMetadata,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Edit Task Details", |tx| {
            editor::update_editor_metadata(tx, &task_id, &metadata, now_ms)
        })
    }

    /// Moves a task's start as one "Schedule Task" step.
    pub fn schedule_task(
        &self,
        id: String,
        start_at_ms: Option<i64>,
        now_ms: i64,
        zone: String,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Schedule Task", |tx| {
            editor::schedule_task(tx, &id, start_at_ms, now_ms, &zone)
        })
    }

    /// Whether moving a repeating task to these dates lands it on or past
    /// its next turn; see `tasks::recurrence_clash`. Read-only.
    pub fn recurrence_clash(
        &self,
        task_id: String,
        due_at_ms: Option<i64>,
        start_at_ms: Option<i64>,
        zone: String,
    ) -> Result<Option<tasks::RecurrenceClash>, CoreError> {
        tasks::recurrence_clash(
            &self.lock(),
            &task_id,
            due_at_ms,
            start_at_ms,
            crate::time::now_ms(),
            &zone,
        )
    }

    /// `recurrence_clash` for the dates an editor save would write. Read-only.
    pub fn editor_recurrence_clash(
        &self,
        edit: EditorSnapshot,
        zone: String,
    ) -> Result<Option<tasks::RecurrenceClash>, CoreError> {
        tasks::editor_recurrence_clash(&self.lock(), &edit, crate::time::now_ms(), &zone)
    }

    /// Keeps a one-off copy of a repeating task at the dates `edit` gives,
    /// and returns its id; see `tasks::add_occurrence_copy`. Called once the
    /// move it answers is saved, it joins that move's step, relabelled "Keep
    /// Both", so one undo takes back the move and the copy together
    /// (`journal::begin_joining_last`); with no such step it is one of its own.
    pub fn add_occurrence_copy(
        &self,
        edit: EditorSnapshot,
        now_ms: i64,
        zone: String,
    ) -> Result<String, CoreError> {
        let mut connection = self.lock();
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let group = journal::begin_joining_last(&transaction, "Keep Both", &edit.task_id)?;
        let id = tasks::add_occurrence_copy(&transaction, &edit, now_ms, &zone)?;
        journal::finish(&transaction, &group)?;
        transaction.commit()?;
        Ok(id)
    }

    /// Says whether a rename just saved for a repeating task was for this
    /// occurrence alone (`previous_title`, the title before it) or for every
    /// turn (`None`); see `tasks::keep_series_title`. It joins the rename's
    /// step, so one undo takes back both (`journal::begin_joining_last`).
    pub fn keep_series_title(
        &self,
        task_id: String,
        previous_title: Option<String>,
        now_ms: i64,
    ) -> Result<bool, CoreError> {
        let mut connection = self.lock();
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let group = journal::begin_joining_last(&transaction, "Edit Task", &task_id)?;
        let wrote =
            tasks::keep_series_title(&transaction, &task_id, previous_title.as_deref(), now_ms)?;
        journal::finish(&transaction, &group)?;
        transaction.commit()?;
        Ok(wrote)
    }

    /// Copies a task's planning onto its subtasks as one step.
    pub fn apply_planning_to_descendants(
        &self,
        task_id: String,
        now_ms: i64,
        zone: String,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Apply Planning to Subtasks", |tx| {
            editor::apply_planning_to_descendants(tx, &task_id, now_ms, &zone)
        })
    }

    /// Opens, completes or cancels a task as one "Change Status" step,
    /// writing a repeating task's next occurrence in `zone` (an IANA name).
    pub fn set_status(
        &self,
        task_id: String,
        status: String,
        now_ms: i64,
        zone: String,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Change Status", |tx| {
            tasks::set_status(tx, &task_id, &status, now_ms, &zone)
        })
    }

    /// Makes a task a daily as one "Make Daily" step; returns the daily's id.
    pub fn make_daily(
        &self,
        task_id: String,
        weekdays: Vec<u32>,
        interval_days: Option<i64>,
        target_seconds: Option<i64>,
        now_ms: i64,
    ) -> Result<String, CoreError> {
        journal::journalled(&mut self.lock(), "Make Daily", |tx| {
            dailies::make_daily(
                tx,
                &task_id,
                &weekdays,
                interval_days,
                target_seconds,
                now_ms,
            )
        })
    }

    /// Archives a task's daily as one "Archive Daily" step.
    pub fn archive_daily(&self, task_id: String, now_ms: i64) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Archive Daily", |tx| {
            dailies::archive_daily(tx, &task_id, now_ms)
        })
    }

    /// Edits a daily as one "Edit Daily" step.
    pub fn update_daily(&self, id: String, edit: DailyEdit, now_ms: i64) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Edit Daily", |tx| {
            dailies::update_daily(tx, &id, &edit, now_ms)
        })
    }

    /// Logs progress on a daily as one "Log Daily" step; returns the
    /// contribution's id.
    pub fn log_contribution(
        &self,
        daily_id: String,
        seconds: i64,
        complete: bool,
        now_ms: i64,
        zone: String,
    ) -> Result<String, CoreError> {
        journal::journalled(&mut self.lock(), "Log Daily", |tx| {
            dailies::log_contribution(tx, &daily_id, seconds, complete, now_ms, &zone)
        })
    }

    /// Un-ticks a day of a daily as one "Clear Daily" step.
    pub fn clear_contribution(
        &self,
        daily_id: String,
        day_ms: i64,
        zone: String,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Clear Daily", |tx| {
            dailies::clear_contribution(tx, &daily_id, day_ms, &zone)
        })
    }

    /// Saves the folder settings sheet as one "Edit Folder" step.
    pub fn save_folder_settings(
        &self,
        id: String,
        name: String,
        parent_folder_id: Option<String>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Edit Folder", |tx| {
            lists::save_folder_settings(tx, &id, &name, parent_folder_id.as_deref(), now_ms)
        })
    }

    /// Saves the list settings sheet as one "Edit List" step.
    pub fn save_list_settings(
        &self,
        id: String,
        settings: ListSettings,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Edit List", |tx| {
            lists::save_list_settings(tx, &id, &settings, now_ms)
        })
    }

    /// Puts tasks in Today or takes them out, as one step.
    pub fn set_planned_for_today(
        &self,
        planned: bool,
        task_ids: Vec<String>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        let label = if planned {
            "Plan for Today"
        } else {
            "Take off Today"
        };
        journal::journalled(&mut self.lock(), label, |tx| {
            today::set_planned_for_today(tx, planned, &task_ids, now_ms)
        })
    }

    /// Sets what a task waits on, when to chase it and every how many days
    /// to chase again (`None` for once), as one "Waiting On" step.
    pub fn set_waiting(
        &self,
        task_id: String,
        waiting_on: Option<String>,
        follow_up_at_ms: Option<i64>,
        follow_up_every_days: Option<u32>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Waiting On", |tx| {
            waiting::set_waiting(
                tx,
                &task_id,
                waiting_on.as_deref(),
                follow_up_at_ms,
                follow_up_every_days,
                now_ms,
            )
        })
    }

    /// Makes every follow-up that has come due, outside the undo journal, and
    /// returns whether it made any.
    pub fn reconcile_waiting_follow_ups(&self, now_ms: i64) -> Result<bool, CoreError> {
        let mut connection = self.lock();
        // Reading first keeps the usual pass, with nothing due, off the writer.
        if !waiting::any_due(&connection, now_ms)? {
            return Ok(false);
        }
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let made = waiting::make_due_follow_ups(&transaction, None, now_ms)?;
        transaction.commit()?;
        Ok(made)
    }

    /// Creates a habit, or rewrites the one on `habit_task_id`, as one step;
    /// returns the habit's daily id.
    pub fn save_habit(
        &self,
        draft: HabitDraft,
        habit_task_id: Option<String>,
        now_ms: i64,
        zone: String,
    ) -> Result<String, CoreError> {
        let label = if habit_task_id.is_none() {
            "New Habit"
        } else {
            "Edit Habit"
        };
        journal::journalled(&mut self.lock(), label, |tx| {
            habits::save_habit(tx, &draft, habit_task_id.as_deref(), now_ms, &zone)
        })
    }

    /// Commits to `task_id` periodically, or rewrites the commitment on
    /// `commitment_task_id`, as one step; returns the subtask's id.
    pub fn save_commitment(
        &self,
        task_id: String,
        draft: CommitmentDraft,
        commitment_task_id: Option<String>,
        now_ms: i64,
        zone: String,
    ) -> Result<String, CoreError> {
        let label = if commitment_task_id.is_none() {
            "Commit to Task"
        } else {
            "Edit Commitment"
        };
        journal::journalled(&mut self.lock(), label, |tx| {
            commitments::save_commitment(
                tx,
                &task_id,
                &draft,
                commitment_task_id.as_deref(),
                now_ms,
                &zone,
            )
        })
    }

    /// Stops the commitment on `commitment_task_id` as one step.
    pub fn stop_commitment(
        &self,
        commitment_task_id: String,
        now_ms: i64,
        zone: String,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Stop Commitment", |tx| {
            commitments::stop_commitment(tx, &commitment_task_id, now_ms, &zone)
        })
    }

    /// The live commitments on a task, in the order of its subtasks.
    pub fn commitments(&self, task_id: String) -> Result<Vec<Commitment>, CoreError> {
        commitments::commitments(&self.read(), &task_id)
    }

    /// Every live commitment in the workspace.
    pub fn all_commitments(&self) -> Result<Vec<Commitment>, CoreError> {
        commitments::all_commitments(&self.read())
    }

    /// The live commitment a subtask carries, if it is one.
    pub fn commitment(&self, task_id: String) -> Result<Option<Commitment>, CoreError> {
        commitments::commitment(&self.read(), &task_id)
    }

    /// How many of a stacking commitment's missed appearances are still
    /// owed on the day `now_ms` falls on; zero for one that vanishes.
    pub fn commitment_missed(
        &self,
        commitment_task_id: String,
        now_ms: i64,
        zone: String,
    ) -> Result<u32, CoreError> {
        commitments::missed(&self.read(), &commitment_task_id, now_ms, &zone)
    }

    /// Applies every placed habit's options for `now`, outside the undo
    /// journal; returns whether anything changed.
    pub fn reconcile_habits(&self, now_ms: i64, zone: String) -> Result<bool, CoreError> {
        let mut connection = self.lock();
        if !habits::any_live(&connection)? {
            return Ok(false);
        }
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let changed = habits::reconcile_habits(&transaction, now_ms, &zone)?;
        transaction.commit()?;
        Ok(changed)
    }

    /// Starts a focus session on a task, or returns the one running.
    #[allow(clippy::too_many_arguments)]
    pub fn start_focus_session(
        &self,
        task_id: String,
        planned_seconds: Option<i64>,
        work_seconds: i64,
        break_seconds: i64,
        context: Option<FocusContext>,
        override_availability: bool,
        now_ms: i64,
        zone: String,
    ) -> Result<String, CoreError> {
        self.unjournalled(|tx| {
            focus::start_session(
                tx,
                &task_id,
                planned_seconds,
                work_seconds,
                break_seconds,
                context.as_ref(),
                override_availability,
                now_ms,
                &zone,
            )
        })
    }

    /// Queues a task in a focus session.
    pub fn add_to_focus_queue(
        &self,
        session_id: String,
        task_id: String,
        planned_seconds: Option<i64>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        self.unjournalled(|tx| {
            focus::add_to_queue(tx, &session_id, &task_id, planned_seconds, now_ms)
        })
    }

    /// Finishes the block in hand as one step: "Complete Task", or "Log Daily
    /// Progress" when the task stays open.
    #[allow(clippy::too_many_arguments)]
    pub fn finish_focus_block(
        &self,
        session_id: String,
        elapsed_seconds: i64,
        quality_multiplier: Option<f64>,
        complete_task: bool,
        expected_block_id: Option<String>,
        context: FocusContext,
        now_ms: i64,
        zone: String,
    ) -> Result<BlockFinished, CoreError> {
        let label = if complete_task {
            "Complete Task"
        } else {
            "Log Daily Progress"
        };
        journal::journalled(&mut self.lock(), label, |tx| {
            focus::finish_block(
                tx,
                &session_id,
                elapsed_seconds,
                quality_multiplier,
                complete_task,
                expected_block_id.as_deref(),
                &context,
                now_ms,
                &zone,
            )
        })
    }

    /// Logs the block in hand as progress and starts a break on it, as one
    /// step.
    #[allow(clippy::too_many_arguments)]
    pub fn take_focus_break(
        &self,
        session_id: String,
        elapsed_seconds: i64,
        quality_multiplier: Option<f64>,
        expected_block_id: Option<String>,
        now_ms: i64,
        zone: String,
    ) -> Result<BlockFinished, CoreError> {
        journal::journalled(&mut self.lock(), "Log Daily Progress", |tx| {
            focus::take_break(
                tx,
                &session_id,
                elapsed_seconds,
                quality_multiplier,
                expected_block_id.as_deref(),
                now_ms,
                &zone,
            )
        })
    }

    /// Back from a break to the task in hand, or on through the queue.
    pub fn end_focus_break(
        &self,
        session_id: String,
        context: FocusContext,
        now_ms: i64,
        zone: String,
    ) -> Result<(), CoreError> {
        self.unjournalled(|tx| focus::end_break(tx, &session_id, &context, now_ms, &zone))
    }

    /// Ends a focus session.
    pub fn finish_focus_session(&self, id: String, now_ms: i64) -> Result<(), CoreError> {
        self.unjournalled(|tx| focus::finish_session(tx, &id, now_ms))
    }

    /// Pauses a running block.
    pub fn pause_focus_session(&self, id: String, now_ms: i64) -> Result<(), CoreError> {
        self.unjournalled(|tx| focus::pause(tx, &id, now_ms))
    }

    /// Resumes a paused block.
    pub fn resume_focus_session(&self, id: String, now_ms: i64) -> Result<(), CoreError> {
        self.unjournalled(|tx| focus::resume(tx, &id, now_ms))
    }

    /// Banks a running block's time.
    pub fn checkpoint_focus_session(&self, id: String, now_ms: i64) -> Result<(), CoreError> {
        self.unjournalled(|tx| focus::checkpoint(tx, &id, now_ms))
    }

    /// Resets a running block's clock after the system clock jumped.
    pub fn rebase_focus_clock(
        &self,
        id: String,
        elapsed_seconds: i64,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        self.unjournalled(|tx| focus::rebase(tx, &id, elapsed_seconds, now_ms))
    }

    /// Pauses every running session at its last checkpoint, on reopening.
    pub fn recover_interrupted_focus(&self) -> Result<(), CoreError> {
        self.unjournalled(focus::recover_interrupted)
    }

    /// Whether a session waiting on a blocked queue can resume in `context`.
    pub fn has_resumable_focus_queue_task(
        &self,
        context: FocusContext,
        now_ms: i64,
        zone: String,
    ) -> Result<bool, CoreError> {
        focus::has_resumable(&self.lock(), &context, now_ms, &zone)
    }

    /// Resumes a session waiting on a blocked queue; whether it did.
    pub fn resume_eligible_focus_queue(
        &self,
        context: FocusContext,
        now_ms: i64,
        zone: String,
    ) -> Result<bool, CoreError> {
        self.unjournalled(|tx| focus::resume_eligible_queue(tx, &context, now_ms, &zone))
    }

    /// Copies tasks from an outside service into the workspace, safely re-run,
    /// outside the undo journal.
    pub fn import_tasks(
        &self,
        workspace_id: String,
        list_name: String,
        source_system: String,
        seeds: Vec<ImportedTaskSeed>,
        now_ms: i64,
    ) -> Result<Option<ImportOutcome>, CoreError> {
        self.unjournalled(|tx| {
            imports::import_tasks(
                tx,
                &workspace_id,
                &list_name,
                &source_system,
                &seeds,
                now_ms,
            )
        })
    }

    /// Brings the plugin-era dailies in once; how many it made.
    pub fn import_legacy_dailies(
        &self,
        legacy: Vec<LegacyDailySeed>,
        progress_task_ids: Vec<String>,
        now_ms: i64,
    ) -> Result<u32, CoreError> {
        self.unjournalled(|tx| {
            imports::import_legacy_dailies(tx, &legacy, &progress_task_ids, now_ms)
        })
    }

    /// Seeds the saved boards from the old preferences and the defaults;
    /// returns every board.
    pub fn kanban_board_baseline(
        &self,
        legacy: Vec<BoardBaseline>,
        current_key: String,
    ) -> Result<Vec<BoardBaseline>, CoreError> {
        self.unjournalled(|tx| imports::kanban_board_baseline(tx, &legacy, &current_key))
    }

    /// The workspace, made on first launch with its Inbox and conditions;
    /// returns its id.
    pub fn bootstrap(&self, now_ms: i64) -> Result<String, CoreError> {
        self.unjournalled(|tx| setup::bootstrap(tx, now_ms))
    }

    /// Stores a theme's JSON; whether it changed anything.
    pub fn upsert_theme(&self, id: String, json: String, now_ms: i64) -> Result<bool, CoreError> {
        self.unjournalled(|tx| setup::upsert_theme(tx, &id, &json, now_ms))
    }

    /// Removes a theme; whether there was one.
    pub fn delete_theme(&self, id: String) -> Result<bool, CoreError> {
        self.unjournalled(|tx| setup::delete_theme(tx, &id))
    }

    /// Stores a preference, keeping a cleared one as null; whether it changed.
    pub fn set_preference(
        &self,
        key: String,
        value: Option<String>,
        now_ms: i64,
    ) -> Result<bool, CoreError> {
        self.unjournalled(|tx| setup::set_preference(tx, &key, value.as_deref(), now_ms))
    }

    /// The device's sync state, or nothing while it has never been paired.
    pub fn sync_state(&self) -> Result<Option<LocalSyncState>, CoreError> {
        sync::state(&self.lock())
    }

    /// Pairs the store with a sync server.
    pub fn begin_sync(&self, device_id: String, server_url: String) -> Result<(), CoreError> {
        self.unjournalled(|tx| sync::begin(tx, &device_id, &server_url))
    }

    /// The newest outbox entry, or nothing when nothing is waiting.
    pub fn latest_sync_outbox_seq(&self) -> Result<Option<i64>, CoreError> {
        sync::latest_outbox_seq(&self.lock())
    }

    /// Unpairs: stops recording and forgets what was waiting to be sent.
    pub fn end_sync(&self) -> Result<(), CoreError> {
        self.unjournalled(sync::end)
    }

    /// Queues every existing row for the first push after pairing.
    pub fn enqueue_sync_snapshot(&self, now_ms: i64) -> Result<(), CoreError> {
        self.unjournalled(|tx| sync::enqueue_snapshot(tx, now_ms))
    }

    /// The outbox, coalesced per row, up to `limit` rows.
    pub fn pending_sync_changes(&self, limit: u32) -> Result<PendingChanges, CoreError> {
        sync::pending_changes(&self.lock(), limit)
    }

    /// Forgets the outbox entries the server has accepted.
    pub fn acknowledge_sync_changes(&self, through_seq: i64) -> Result<(), CoreError> {
        self.unjournalled(|tx| sync::acknowledge(tx, through_seq))
    }

    /// Writes a pull into the workspace; whether anything changed.
    pub fn apply_remote_rows(
        &self,
        rows: Vec<IncomingRow>,
        cursor: i64,
        hlc: Option<String>,
        now_ms: i64,
    ) -> Result<bool, CoreError> {
        self.unjournalled(|tx| sync::apply_remote_rows(tx, &rows, cursor, hlc.as_deref(), now_ms))
    }

    /// The outbox's next batch as a `POST /v1/push` body, stamped from the
    /// stored clock; nothing when the outbox is empty.
    pub fn prepare_sync_push(
        &self,
        limit: u32,
        wall_ms: i64,
    ) -> Result<Option<sync::wire::SyncPushBatch>, CoreError> {
        sync::wire::prepare_push(&self.lock(), limit, wall_ms)
    }

    /// The server has a push: forgets its outbox entries and keeps its clock.
    pub fn finish_sync_push(
        &self,
        through_seq: i64,
        hlc: String,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        self.unjournalled(|tx| sync::wire::finish_push(tx, through_seq, &hlc, now_ms))
    }

    /// Writes a gathered pull into the workspace in one transaction.
    pub fn apply_sync_pull(
        &self,
        pull: Arc<sync::wire::SyncPull>,
        wall_ms: i64,
        now_ms: i64,
    ) -> Result<sync::wire::SyncPullOutcome, CoreError> {
        self.unjournalled(|tx| sync::wire::apply_pull(tx, &pull, wall_ms, now_ms))
    }

    /// Advances the stored cursor and clock without applying rows.
    pub fn record_sync_progress(
        &self,
        cursor: Option<i64>,
        hlc: Option<String>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        self.unjournalled(|tx| sync::record_progress(tx, cursor, hlc.as_deref(), now_ms))
    }

    /// The open tasks the next-up engine and the day choose from.
    /// The workspaces, oldest first.
    pub fn workspaces(&self) -> Result<Vec<WorkspaceRow>, CoreError> {
        records::workspaces(&self.lock())
    }

    /// A workspace's folders in sidebar order.
    pub fn folders(&self, workspace_id: String) -> Result<Vec<FolderRow>, CoreError> {
        records::folders(&self.lock(), &workspace_id)
    }

    /// A workspace's lists in sidebar order.
    pub fn lists(
        &self,
        workspace_id: String,
        including_archived: bool,
    ) -> Result<Vec<ListRow>, CoreError> {
        records::lists(&self.lock(), &workspace_id, including_archived)
    }

    /// One list, if it exists.
    pub fn list(&self, id: String) -> Result<Option<ListRow>, CoreError> {
        records::list(&self.lock(), &id)
    }

    /// A workspace's Inbox.
    pub fn inbox(&self, workspace_id: String) -> Result<Option<ListRow>, CoreError> {
        records::inbox(&self.lock(), &workspace_id)
    }

    /// One task, if it exists.
    pub fn task(&self, id: String) -> Result<Option<TaskRow>, CoreError> {
        records::task(&self.lock(), &id)
    }

    /// Tasks by id; missing ids are absent.
    pub fn tasks_by_id(&self, ids: Vec<String>) -> Result<Vec<TaskRow>, CoreError> {
        records::tasks_by_id(&self.read(), &ids)
    }

    /// The children of a task in a list, or its roots, in outline order;
    /// none resting (`commitments::resting_task_ids`).
    pub fn child_tasks(
        &self,
        list_id: String,
        parent_task_id: Option<String>,
    ) -> Result<Vec<TaskRow>, CoreError> {
        let connection = self.lock();
        let rows = records::children(&connection, &list_id, parent_task_id.as_deref())?;
        without_resting(&connection, rows, |row| &row.id)
    }

    /// Every task in the given lists, each list in outline order, but a
    /// commitment resting today (`commitments::resting_task_ids`): what every
    /// client's outline and list board is built from.
    pub fn tasks_in_lists(&self, list_ids: Vec<String>) -> Result<Vec<TaskRow>, CoreError> {
        let connection = self.lock();
        let rows = records::tasks_in_lists(&connection, &list_ids)?;
        without_resting(&connection, rows, |row| &row.id)
    }

    /// `tasks_in_lists`, packed by `packed_rows::pack_task_rows` so the
    /// rows cross as one buffer rather than field by field.
    pub fn tasks_in_lists_packed(&self, list_ids: Vec<String>) -> Result<Vec<u8>, CoreError> {
        let connection = self.lock();
        let rows = records::tasks_in_lists(&connection, &list_ids)?;
        let rows = without_resting(&connection, rows, |row| &row.id)?;
        Ok(crate::packed_rows::pack_task_rows(rows.iter()))
    }

    /// What the sidebar draws beneath the given lists, walked here so only
    /// the nested lists and the counts cross.
    pub fn sidebar_index(&self, list_ids: Vec<String>) -> Result<SidebarIndex, CoreError> {
        sidebar::sidebar_index(&self.lock(), &list_ids)
    }

    /// `sidebar_index` with each list's and nested list's open count, for
    /// Android's badges.
    pub fn sidebar_index_with_open_counts(
        &self,
        list_ids: Vec<String>,
    ) -> Result<SidebarIndex, CoreError> {
        sidebar::sidebar_index_with_open_counts(&self.lock(), &list_ids)
    }

    /// A combined scope's board — Everything's or a folder's — selected and
    /// walked here, so only the rows it draws cross. See `board.rs`.
    pub fn combined_board(
        &self,
        list_ids: Vec<String>,
        hide_completed_before_ms: Option<i64>,
    ) -> Result<BoardRead, CoreError> {
        board::combined_board(&self.lock(), &list_ids, hide_completed_before_ms)
    }

    /// A combined scope's cards alone, packed, for an outline that draws
    /// them flat. See `board::actionable_tasks`.
    pub fn actionable_tasks_packed(&self, list_ids: Vec<String>) -> Result<Vec<u8>, CoreError> {
        board::actionable_tasks(&self.read(), &list_ids)
    }

    /// A list's outline under a task, or the whole list.
    pub fn outline(
        &self,
        list_id: String,
        parent_task_id: Option<String>,
    ) -> Result<Vec<OutlineItem>, CoreError> {
        let connection = self.lock();
        let items = records::outline(&connection, &list_id, parent_task_id.as_deref())?;
        without_resting(&connection, items, |item| &item.task.id)
    }

    /// The imported wrapper a list shows its children in place of, if any.
    pub fn visible_root_parent(&self, list_id: String) -> Result<Option<String>, CoreError> {
        records::visible_root_parent(&self.lock(), &list_id)
    }

    /// The root a list may show its children in place of, if there is one.
    pub fn visible_root_candidates(&self, list_id: String) -> Result<Vec<TaskRow>, CoreError> {
        records::visible_root_candidates(&self.lock(), &list_id)
    }

    /// The folders a folder may move into.
    pub fn valid_parent_folders(&self, folder_id: String) -> Result<Vec<FolderRow>, CoreError> {
        records::valid_parent_folders(&self.lock(), &folder_id)
    }

    /// Prefix search over titles and notes, best match first.
    pub fn search_tasks(
        &self,
        workspace_id: String,
        query: String,
        including_completed: bool,
        including_archived_lists: bool,
        limit: i64,
    ) -> Result<Vec<SearchHit>, CoreError> {
        search::search(
            &self.lock(),
            &workspace_id,
            &query,
            including_completed,
            including_archived_lists,
            limit,
        )
    }

    pub fn next_up_candidates(
        &self,
        now_ms: i64,
        zone: String,
    ) -> Result<Vec<Candidate>, CoreError> {
        focus::candidates(&self.lock(), now_ms, &zone)
    }

    /// The day and the focus ladder in one read: candidates read, the day
    /// planned and the ladder ranked without crossing, the ladder cut to its
    /// first `ladder_limit` entries and the day's tasks when a limit is given.
    pub fn next_up(
        &self,
        now_ms: i64,
        zone: String,
        context: FocusContext,
        running_id: Option<String>,
        ladder_limit: Option<u32>,
    ) -> Result<NextUp, CoreError> {
        next_up::next_up(
            &self.read(),
            now_ms,
            &zone,
            &context,
            running_id.as_deref(),
            ladder_limit,
        )
    }

    /// Ranks tasks in Today's focus order as one "Reorder Today" step.
    pub fn arrange_day(&self, ordered_task_ids: Vec<String>, now_ms: i64) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Reorder Today", |tx| {
            today::arrange_day(tx, &ordered_task_ids, now_ms)
        })
    }

    /// Pins a task in the focus order as one "Pin Task" step.
    pub fn pin_task(&self, task_id: String, index: i64, now_ms: i64) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Pin Task", |tx| {
            today::pin_task(tx, &task_id, index, now_ms)
        })
    }

    /// Releases a task to the ranking as one "Unpin Task" step.
    pub fn unpin_task(&self, task_id: String, now_ms: i64) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Unpin Task", |tx| {
            today::unpin_task(tx, &task_id, now_ms)
        })
    }

    /// Clears the focus order as one "Clear Focus Order" step.
    pub fn clear_focus_order(&self, now_ms: i64) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Clear Focus Order", |tx| {
            today::clear_focus_order(tx, now_ms)
        })
    }

    /// Renames a folder as one "Rename Folder" step.
    pub fn rename_folder(&self, id: String, name: String, now_ms: i64) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Rename Folder", |tx| {
            lists::rename_folder(tx, &id, &name, now_ms)
        })
    }

    /// Renames a list as one "Rename List" step.
    pub fn rename_list(&self, id: String, name: String, now_ms: i64) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Rename List", |tx| {
            lists::rename_list(tx, &id, &name, now_ms)
        })
    }

    /// Sets a list's name and colour as one "Edit List" step.
    pub fn update_list(
        &self,
        id: String,
        name: String,
        colour_hex: Option<String>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Edit List", |tx| {
            lists::update_list(tx, &id, &name, colour_hex.as_deref(), now_ms)
        })
    }

    /// Archives or restores a list as one "Archive List" step.
    pub fn set_list_archived(
        &self,
        id: String,
        archived: bool,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Archive List", |tx| {
            lists::set_list_archived(tx, &id, archived, now_ms)
        })
    }

    /// Deletes a folder as one "Delete Folder" step; its lists move to the top.
    pub fn delete_folder(&self, id: String) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Delete Folder", |tx| {
            lists::delete_folder(tx, &id)
        })
    }
}

impl CoreWorkspace {
    /// Runs a write outside the undo journal, in its own immediate
    /// transaction: the focus clock, and passes nobody asked for.
    pub(crate) fn unjournalled<T>(
        &self,
        work: impl FnOnce(&rusqlite::Transaction) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let mut connection = self.lock();
        let transaction =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let result = work(&transaction)?;
        transaction.commit()?;
        Ok(result)
    }

    /// A poisoned lock only means another call panicked mid-way; SQLite rolled
    /// its transaction back, so the connection is still sound to use.
    pub(crate) fn lock(&self) -> MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The reading connection, for a pure read a client makes off its main
    /// thread. Nothing that writes, or that must see a transaction still
    /// open on the writer, may use it.
    pub(crate) fn read(&self) -> MutexGuard<'_, Connection> {
        match &self.reader {
            Some(reader) => reader
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            None => self.lock(),
        }
    }
}

/// `rows` without the commitments resting at this moment, and what is
/// beneath them (`commitments::resting_task_ids_now`).
fn without_resting<T>(
    connection: &Connection,
    mut rows: Vec<T>,
    id: impl Fn(&T) -> &String,
) -> Result<Vec<T>, CoreError> {
    let resting = commitments::resting_task_ids_now(connection)?;
    if !resting.is_empty() {
        rows.retain(|row| !resting.contains(id(row)));
    }
    Ok(rows)
}

/// Orders next-up candidates for `now` in `context`: a pure function, no
/// database, for clients that already hold the candidates.
#[uniffi::export]
pub fn rank_next_up(
    candidates: Vec<Candidate>,
    now_ms: i64,
    zone: String,
    context: FocusContext,
) -> Ranking {
    ranking::evaluate(&candidates, now_ms, &zone, &context)
}

/// Which candidates make up the day at `now`, and why: a pure function, no
/// database. `DayPlanSelector.plan`.
#[uniffi::export]
pub fn plan_day(
    candidates: Vec<Candidate>,
    running_id: Option<String>,
    now_ms: i64,
    zone: String,
) -> Vec<DayEntry> {
    next_up::plan(&candidates, running_id.as_deref(), now_ms, &zone)
}

/// Why a candidate is not available in `context` at `now`; empty when it is.
#[uniffi::export]
pub fn availability_reasons(
    candidate: Candidate,
    context: FocusContext,
    now_ms: i64,
) -> Vec<Unavailable> {
    focus::reasons(&candidate, &context, now_ms)
}

/// The block length to offer for a candidate.
#[uniffi::export]
pub fn suggested_block_seconds(candidate: Candidate, context: FocusContext, now_ms: i64) -> i64 {
    focus::suggested_seconds(&candidate, &context, now_ms)
}

/// The block length to run for a candidate, given what was asked for.
#[uniffi::export]
pub fn planned_block_seconds(
    candidate: Candidate,
    requested: Option<i64>,
    context: FocusContext,
    now_ms: i64,
) -> i64 {
    focus::planned_seconds(&candidate, requested, &context, now_ms)
}

/// Why one available task ranks where it does.
#[uniffi::export]
pub fn score_next_up(candidate: Candidate, now_ms: i64, zone: String) -> Scored {
    ranking::score_one(candidate, now_ms, &zone)
}
