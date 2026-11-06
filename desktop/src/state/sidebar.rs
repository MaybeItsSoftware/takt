//! The left dock: its rows, top to bottom, as the core orders them
//! (`sidebar::outline`), and the cursor that walks them.
//!
//! Moving the cursor selects the row it lands on, as the Swift sidebar does,
//! so `j` through the lists shows each one.

use super::{App, Region, Scope, ViewMode};
use takt_core::sidebar::outline::{
    SidebarFolder, SidebarList, SidebarNestedList, SidebarRowKind,
    sidebar_outline_rows_with_filter_lists,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowKind {
    Today,
    Everything,
    List(String),
    /// A list nested inside a task, by the task's id.
    NestedList(String),
    Folder(String),
    FilterList(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarRow {
    /// Unique per row: a pinned nested list is drawn twice.
    pub id: String,
    pub kind: RowKind,
    pub depth: u32,
}

/// One step of the cursor, as a key asks for it.
pub enum CursorStep {
    By(i64),
    First,
    Last,
}

impl CursorStep {
    pub fn for_key(key: &str) -> Option<Self> {
        Some(match key {
            "down" | "j" => CursorStep::By(1),
            "up" | "k" => CursorStep::By(-1),
            "pagedown" | "ctrl+d" => CursorStep::By(8),
            "pageup" | "ctrl+u" => CursorStep::By(-8),
            "home" | "cmd+up" | "gg" => CursorStep::First,
            "end" | "cmd+down" | "shift+g" => CursorStep::Last,
            _ => return None,
        })
    }
}

impl App {
    pub fn sidebar_lists(&self) -> Vec<SidebarList> {
        self.lists
            .iter()
            .filter(|list| list.system_role.as_deref() != Some("inbox"))
            .map(|list| SidebarList {
                id: list.id.clone(),
                folder_id: list.folder_id.clone(),
            })
            .collect()
    }

    pub fn sidebar_folders(&self) -> Vec<SidebarFolder> {
        self.folders
            .iter()
            .map(|folder| SidebarFolder {
                id: folder.id.clone(),
                parent_folder_id: folder.parent_folder_id.clone(),
            })
            .collect()
    }

    /// Reads the lists, folders, nested lists and filter lists, and lays
    /// the rows out again.
    pub fn reload_sidebar(&mut self) {
        let core = &self.store.core;
        let workspace = self.store.workspace_id.clone();
        match core.lists(workspace.clone(), true) {
            Ok(all) => {
                let (archived, active): (Vec<_>, Vec<_>) =
                    all.into_iter().partition(|list| list.is_archived);
                self.lists = active;
                self.archived_lists = archived;
            }
            Err(error) => self.message = Some(error.to_string()),
        }
        self.folders = core.folders(workspace.clone()).unwrap_or_default();
        self.filter_lists = core
            .filter_lists(workspace)
            .unwrap_or_default()
            .into_iter()
            .filter(|filter| !filter.is_builtin)
            .collect();
        let ids: Vec<String> = self.lists.iter().map(|list| list.id.clone()).collect();
        if let Ok(index) = core.sidebar_index(ids) {
            self.nested_lists = index.nested_lists;
            self.task_counts = index
                .task_counts
                .into_iter()
                .map(|count| (count.list_id, count.count))
                .collect();
        }
        self.layout_sidebar();
    }

    pub fn layout_sidebar(&mut self) {
        self.sidebar_rows = self.sidebar_rows_expanding(false);
    }

    fn sidebar_rows_expanding(&self, every_folder: bool) -> Vec<SidebarRow> {
        let lists = self.sidebar_lists();
        let folders = self.sidebar_folders();
        let nested: Vec<SidebarNestedList> = self
            .nested_lists
            .iter()
            .map(|item| SidebarNestedList {
                id: item.task.id.clone(),
                list_id: item.task.list_id.clone(),
                depth: item.depth.max(0) as u32,
                is_promoted: item.task.is_promoted == Some(true) && item.task.status == "open",
            })
            .collect();
        let expanded: Vec<String> = if every_folder {
            self.folders
                .iter()
                .map(|folder| folder.id.clone())
                .collect()
        } else {
            self.prefs.expanded_folders.iter().cloned().collect()
        };
        let inbox = self.inbox().map(|inbox| inbox.id.clone());
        sidebar_outline_rows_with_filter_lists(
            inbox.clone(),
            lists.clone(),
            folders.clone(),
            nested.clone(),
            expanded,
            true,
            self.filter_lists.len() as u32,
        )
        .into_iter()
        .filter_map(|row| {
            let index = row.subject as usize;
            let (id, kind) = match row.kind {
                SidebarRowKind::Today => ("row:today".into(), RowKind::Today),
                SidebarRowKind::Everything => ("row:everything".into(), RowKind::Everything),
                SidebarRowKind::Inbox => {
                    let id = inbox.clone()?;
                    (format!("list:{id}"), RowKind::List(id))
                }
                SidebarRowKind::List => {
                    let id = lists.get(index)?.id.clone();
                    (format!("list:{id}"), RowKind::List(id))
                }
                SidebarRowKind::NestedList => {
                    let item = nested.get(index)?;
                    (
                        format!("nested:{}:{}", item.list_id, item.id),
                        RowKind::NestedList(item.id.clone()),
                    )
                }
                SidebarRowKind::PinnedNestedList => {
                    let id = nested.get(index)?.id.clone();
                    (format!("pinned:{id}"), RowKind::NestedList(id))
                }
                SidebarRowKind::Folder => {
                    let id = folders.get(index)?.id.clone();
                    (format!("folder:{id}"), RowKind::Folder(id))
                }
                SidebarRowKind::FilterList => {
                    let id = self.filter_lists.get(index)?.id.clone();
                    (format!("filter:{id}"), RowKind::FilterList(id))
                }
            };
            Some(SidebarRow {
                id,
                kind,
                depth: row.depth,
            })
        })
        .collect()
    }

    /// Every list, folders opened through, in the order the sidebar draws.
    pub fn lists_in_sidebar_order(&self) -> Vec<String> {
        self.sidebar_rows_expanding(true)
            .into_iter()
            .filter_map(|row| match row.kind {
                RowKind::List(id) => Some(id),
                _ => None,
            })
            .collect()
    }

    /// The row the cursor is on: the one it was put on, or else the row of
    /// whatever is selected.
    pub fn sidebar_cursor_row(&self) -> Option<&SidebarRow> {
        if let Some(id) = &self.sidebar_cursor
            && let Some(row) = self.sidebar_rows.iter().find(|row| &row.id == id)
        {
            return Some(row);
        }
        self.sidebar_rows
            .iter()
            .find(|row| self.row_is_current(row))
    }

    /// Whether a row is the place on screen.
    pub fn row_is_current(&self, row: &SidebarRow) -> bool {
        if self.view_mode == ViewMode::Today {
            return row.kind == RowKind::Today;
        }
        match (&row.kind, &self.scope) {
            (RowKind::Everything, Scope::Everything) => true,
            (
                RowKind::List(id),
                Scope::List {
                    list_id,
                    entered: None,
                },
            ) => id == list_id,
            (
                RowKind::NestedList(id),
                Scope::List {
                    entered: Some(entered),
                    ..
                },
            ) => id == entered,
            (RowKind::Folder(id), Scope::Folder(folder)) => id == folder,
            (RowKind::FilterList(id), Scope::FilterList(filter)) => id == filter,
            _ => false,
        }
    }

    pub fn move_sidebar_cursor(&mut self, key: &str) {
        let Some(step) = CursorStep::for_key(key) else {
            return;
        };
        let count = self.sidebar_rows.len();
        if count == 0 {
            return;
        }
        let current = self
            .sidebar_cursor_row()
            .and_then(|row| self.sidebar_rows.iter().position(|r| r.id == row.id));
        let index = match (step, current) {
            (CursorStep::First, _) => 0,
            (CursorStep::Last, _) => count - 1,
            (CursorStep::By(offset), None) => {
                if offset < 0 {
                    count - 1
                } else {
                    0
                }
            }
            (CursorStep::By(offset), Some(current)) => {
                takt_core::shaping::cursor_step(current as i64, offset, count as i64) as usize
            }
        };
        let row = self.sidebar_rows[index].clone();
        self.apply_sidebar_cursor(&row);
    }

    /// Selects what the row points at and puts the cursor on it.
    pub fn apply_sidebar_cursor(&mut self, row: &SidebarRow) {
        match &row.kind {
            RowKind::Today => self.select_today(),
            RowKind::Everything => self.select_everything(),
            RowKind::List(id) => self.select_list(id),
            RowKind::NestedList(id) => self.select_nested_list(id),
            RowKind::Folder(id) => self.select_folder(id),
            RowKind::FilterList(id) => self.select_filter_list(id),
        }
        self.sidebar_cursor = Some(row.id.clone());
    }

    /// `l`, `→` and Return on a row: a closed folder opens, anything else
    /// hands the keyboard to the tasks.
    pub fn activate_sidebar_cursor(&mut self, expand_only: bool, enters: bool) {
        let Some(row) = self.sidebar_cursor_row().cloned() else {
            self.enter_tasks_from_sidebar();
            return;
        };
        if let RowKind::Folder(id) = &row.kind {
            let open = self.prefs.expanded_folders.contains(id);
            if expand_only && !open {
                self.set_folder_expanded(id, true);
            } else if enters || expand_only {
                self.select_folder(id);
                self.enter_tasks_from_sidebar();
            } else {
                self.set_folder_expanded(id, !open);
            }
            return;
        }
        self.enter_tasks_from_sidebar();
    }

    /// `h`, `←`, `-`: close the folder, leave the nested list, or go up a
    /// level.
    pub fn collapse_sidebar_cursor(&mut self) {
        match self.scope.clone() {
            Scope::Folder(id) if self.prefs.expanded_folders.contains(&id) => {
                self.set_folder_expanded(&id, false);
            }
            Scope::List {
                entered: Some(_), ..
            } => self.leave_entered_task(),
            Scope::Folder(id) => {
                if let Some(parent) = self.folder(&id).and_then(|f| f.parent_folder_id.clone()) {
                    self.select_folder(&parent);
                } else {
                    self.select_everything();
                }
            }
            Scope::List { list_id, .. } => {
                if let Some(folder) = self.list(&list_id).and_then(|l| l.folder_id.clone()) {
                    self.select_folder(&folder);
                } else {
                    self.select_everything();
                }
            }
            Scope::FilterList(_) => self.select_everything(),
            Scope::Everything => {}
        }
        self.sidebar_cursor = None;
    }

    pub fn set_folder_expanded(&mut self, id: &str, expanded: bool) {
        if expanded {
            self.prefs.expanded_folders.insert(id.to_string());
        } else {
            self.prefs.expanded_folders.remove(id);
        }
        self.prefs.save();
        self.layout_sidebar();
    }

    pub fn set_all_folders_expanded(&mut self, expanded: bool) {
        if expanded {
            self.prefs.expanded_folders = self.folders.iter().map(|f| f.id.clone()).collect();
        } else {
            self.prefs.expanded_folders.clear();
        }
        self.prefs.save();
        self.layout_sidebar();
    }

    /// The keyboard leaves the sidebar for the tasks, landing on the first.
    pub fn enter_tasks_from_sidebar(&mut self) {
        self.selected_task = self.rows.first().map(|row| row.task.id.clone());
        self.region = Region::Tasks;
    }

    /// The keyboard goes back to the sidebar, opening the folders above the
    /// current list so its row can be seen.
    pub fn return_to_sidebar(&mut self) {
        let mut folder = match &self.scope {
            Scope::Folder(id) => self.folder(id).and_then(|f| f.parent_folder_id.clone()),
            Scope::List { list_id, .. } => self.list(list_id).and_then(|l| l.folder_id.clone()),
            _ => None,
        };
        let mut seen = std::collections::HashSet::new();
        while let Some(id) = folder {
            if !seen.insert(id.clone()) {
                break;
            }
            self.prefs.expanded_folders.insert(id.clone());
            folder = self.folder(&id).and_then(|f| f.parent_folder_id.clone());
        }
        self.prefs.save();
        self.layout_sidebar();
        self.sidebar_cursor = None;
        self.region = Region::Sidebar;
    }
}
