//! The commands on a task or a list that are more than an edit to its
//! fields: bury, convert, pin, extract, links, blockers, waiting, and the
//! list commands the sidebar runs (restore, complete, delete, icon, where
//! new tasks go). `WorkspaceViewModel+Commands.swift` and `+Sidebar.swift`.

use super::pickers::{Picker, open_address};
use super::sidebar::RowKind;
use super::store::now_ms;
use super::{App, Region, Scope};

/// Something waiting on Return before it is deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingDelete {
    Task(String),
    List(String),
    Folder(String),
}

impl App {
    fn is_list_item(&self, id: &str) -> bool {
        self.task(id)
            .is_some_and(|t| t.item_kind.as_deref() == Some("list"))
    }

    /// The task a sidebar command acts on: the nested list the scope is, or
    /// the selection.
    fn sidebar_or_selected(&self) -> Option<String> {
        let entered = match &self.scope {
            Scope::List { entered, .. } => entered.clone(),
            _ => None,
        };
        if self.region == Region::Sidebar {
            entered
        } else {
            self.selected_task.clone().or(entered)
        }
    }

    /// `b`: starts the task at the beginning of tomorrow, which buries it
    /// until then.
    pub fn bury_selected(&mut self) {
        let tomorrow = super::commands::start_of_local_day(1);
        self.schedule_selected(Some(tomorrow));
        if let Some(id) = self.selected_task.clone() {
            self.patch_details(&id);
            self.message = Some("Buried until tomorrow.".into());
        }
    }

    pub fn toggle_hide_later(&mut self) {
        self.prefs.hide_later = !self.prefs.hide_later;
        self.prefs.save();
        self.layout_rows();
        self.message = Some(
            if self.prefs.hide_later {
                "Buried tasks hidden."
            } else {
                "Buried tasks shown."
            }
            .into(),
        );
    }

    /// `.`: shows a folded task's subtasks.
    pub fn grow_selected(&mut self) {
        if let Some(id) = self.selected_task.clone()
            && self.prefs.folded_tasks.remove(&id)
        {
            self.prefs.save();
            self.layout_rows();
        }
    }

    /// `,`: hides the task's subtasks, or those of the task it sits under.
    pub fn shrink_selected(&mut self) {
        let Some(id) = self.selected_task.clone() else {
            return;
        };
        let row = self.rows.iter().find(|row| row.task.id == id).cloned();
        let target = match row {
            Some(row) if row.has_children && !self.prefs.folded_tasks.contains(&id) => Some(id),
            Some(row) => row
                .task
                .parent_task_id
                .filter(|parent| self.rows.iter().any(|r| r.task.id == *parent)),
            None => None,
        };
        if let Some(target) = target {
            self.prefs.folded_tasks.insert(target.clone());
            self.prefs.save();
            if self.selected_task.as_deref() != Some(target.as_str()) {
                self.selected_task = Some(target);
            }
            self.layout_rows();
        }
    }

    /// `1`–`4`, `⌥⇧1`…: the priority, or none when it already has it.
    pub fn set_priority_from_key(&mut self, key: &str) {
        let digits: String = key
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '-')
            .collect();
        let Ok(level) = digits.parse::<i64>() else {
            return;
        };
        if !(1..=4).contains(&level.abs()) {
            return;
        }
        let current = self
            .selected_task
            .as_deref()
            .and_then(|id| self.meta(id))
            .and_then(|m| m.priority);
        let next = (current != Some(level)).then_some(level);
        self.edit_selected(|edit| edit.metadata.priority = next);
    }

    // MARK: `:12`

    pub fn begin_number_jump(&mut self) {
        if self.region == Region::Tasks {
            self.sequence.reset();
            self.number_jump = Some(String::new());
        }
    }

    /// A key while `:` is typed: a digit, ⌫, Return, or anything else to
    /// stop. True when it was the jump's.
    pub fn number_jump_key(&mut self, key: &str) -> bool {
        let Some(mut digits) = self.number_jump.take() else {
            return false;
        };
        match key {
            _ if key.len() == 1 && key.chars().all(|c| c.is_ascii_digit()) => {
                if digits.len() < 5 {
                    digits.push_str(key);
                }
                self.number_jump = Some(digits);
            }
            "backspace" | "delete" => {
                if !digits.is_empty() {
                    digits.pop();
                    self.number_jump = Some(digits);
                }
            }
            "enter" => {
                if let Ok(number) = digits.parse::<usize>() {
                    let ids = self.visible_ids();
                    if !ids.is_empty() {
                        let ix = number.clamp(1, ids.len()) - 1;
                        self.selected_task = Some(ids[ix].clone());
                    }
                }
            }
            _ => {}
        }
        true
    }

    // MARK: Converting

    /// `⌘⇧L`: a task becomes a nested list and back; from the sidebar a
    /// list becomes a task in the inbox.
    pub fn convert_selected(&mut self) {
        let now = now_ms();
        if let Some(id) = self.sidebar_or_selected() {
            let to = if self.is_list_item(&id) || self.nested_lists.iter().any(|n| n.task.id == id)
            {
                "task"
            } else {
                "list"
            };
            if self
                .write(|s| s.core.set_item_kind(id.clone(), to.into(), now))
                .is_some()
            {
                self.message = Some(if to == "list" {
                    "Now a list.".into()
                } else {
                    "Now a task.".into()
                });
                self.after_structure_change();
            }
            return;
        }
        if let Scope::List {
            list_id,
            entered: None,
        } = self.scope.clone()
            && self.list(&list_id).is_some_and(|l| l.system_role.is_none())
            && let Some(task_id) = self.write(|s| s.core.convert_list_to_task(list_id.clone(), now))
        {
            self.reload_sidebar();
            if let Some(task) = self.store.core.task(task_id.clone()).ok().flatten() {
                self.select_list(&task.list_id);
            }
            self.selected_task = Some(task_id);
            self.region = Region::Tasks;
        }
    }

    /// Pins a nested list to the sidebar, or unpins it.
    pub fn toggle_promoted(&mut self) {
        let Some(id) = self.sidebar_or_selected() else {
            return;
        };
        let Some(task) = self
            .task(&id)
            .cloned()
            .or_else(|| self.store.core.task(id.clone()).ok().flatten())
        else {
            return;
        };
        if task.item_kind.as_deref() != Some("list") {
            self.message = Some("Only a nested list can be pinned.".into());
            return;
        }
        let promoted = task.is_promoted != Some(true);
        if self
            .write(|s| {
                s.core
                    .set_nested_list_promoted(id.clone(), promoted, now_ms())
            })
            .is_some()
        {
            self.after_structure_change();
        }
    }

    /// The branch under the selection leaves for the sidebar as a list.
    pub fn extract_branch(&mut self) {
        let Some(id) = self.selected_task.clone() else {
            return;
        };
        if let Some(list_id) =
            self.write(|s| s.core.move_task_to_folder(id.clone(), None, now_ms()))
        {
            self.reload_sidebar();
            self.select_list(&list_id);
            self.region = Region::Tasks;
            self.message = Some("Extracted as a list.".into());
        }
    }

    // MARK: Links

    pub fn open_first_link(&mut self) {
        let Some(id) = self.selected_task.clone() else {
            return;
        };
        match self
            .store
            .core
            .task_links(id)
            .ok()
            .and_then(|links| links.into_iter().next())
        {
            Some(link) => open_address(&link.url),
            None => self.message = Some("No link on this task.".into()),
        }
    }

    pub fn begin_add_link(&mut self) {
        if let Some(id) = self.selected_task.clone() {
            self.open_picker(Picker::Link { task_id: id });
        }
    }

    // MARK: The sidebar's list commands

    /// `⌘⇧R`: the archived list archived last comes back.
    pub fn restore_last_archived(&mut self) {
        let Some(list) = self
            .archived_lists
            .iter()
            .max_by_key(|l| l.updated_at_ms)
            .cloned()
        else {
            self.message = Some("No archived lists.".into());
            return;
        };
        if self
            .write(|s| s.core.set_list_archived(list.id.clone(), false, now_ms()))
            .is_some()
        {
            self.reload_sidebar();
            self.select_list(&list.id);
            self.message = Some(format!("Restored {}.", list.name));
        }
    }

    /// `⌘⇧X`: the list (or the nested list you are in) is completed or
    /// reopened.
    pub fn toggle_list_completed(&mut self) {
        match self.scope.clone() {
            Scope::List {
                entered: Some(id), ..
            } if self.is_list_item(&id) || self.nested_lists.iter().any(|n| n.task.id == id) => {
                let open = self
                    .store
                    .core
                    .task(id.clone())
                    .ok()
                    .flatten()
                    .is_some_and(|t| t.status == "open");
                self.set_status(&id, if open { "completed" } else { "open" });
            }
            Scope::List { list_id, .. } => {
                let Some(list) = self.list(&list_id).cloned() else {
                    return;
                };
                if list.system_role.is_some() {
                    return;
                }
                let completed = list.completed_at_ms.is_none();
                if self
                    .write(|s| {
                        s.core
                            .set_list_completed(list_id.clone(), completed, now_ms())
                    })
                    .is_some()
                {
                    self.message = Some(if completed {
                        format!("Completed {}.", list.name)
                    } else {
                        format!("Reopened {}.", list.name)
                    });
                    self.reload_sidebar();
                }
            }
            _ => {}
        }
    }

    /// `⌘⇧⌫`: asks to delete the list or folder; Return deletes it.
    pub fn ask_to_delete_list(&mut self) {
        let kind = if self.region == Region::Sidebar {
            self.sidebar_cursor_row().map(|row| row.kind.clone())
        } else {
            match &self.scope {
                Scope::List {
                    list_id,
                    entered: None,
                } => Some(RowKind::List(list_id.clone())),
                Scope::Folder(id) => Some(RowKind::Folder(id.clone())),
                _ => None,
            }
        };
        let (pending, name) = match kind {
            Some(RowKind::List(id)) => {
                let Some(list) = self.list(&id) else {
                    return;
                };
                if list.system_role.is_some() {
                    self.message = Some("The inbox can't be deleted.".into());
                    return;
                }
                (PendingDelete::List(id.clone()), list.name.clone())
            }
            Some(RowKind::Folder(id)) => {
                let Some(folder) = self.folder(&id) else {
                    return;
                };
                (PendingDelete::Folder(id.clone()), folder.name.clone())
            }
            Some(RowKind::NestedList(id)) => {
                let title = self
                    .nested_lists
                    .iter()
                    .find(|n| n.task.id == id)
                    .map(|n| n.task.title.clone())
                    .unwrap_or_default();
                (PendingDelete::Task(id), title)
            }
            _ => return,
        };
        self.pending_delete = Some(pending);
        self.message = Some(format!(
            "Delete \u{201c}{name}\u{201d} and everything in it? Return deletes, any other key keeps it."
        ));
    }

    pub fn delete_pending(&mut self) {
        let Some(pending) = self.pending_delete.take() else {
            return;
        };
        self.message = None;
        match pending {
            PendingDelete::Task(id) => self.delete_task(&id),
            PendingDelete::List(id) => {
                if self.write(|s| s.core.delete_list(id.clone())).is_some() {
                    self.reload_sidebar();
                    if matches!(&self.scope, Scope::List { list_id, .. } if *list_id == id) {
                        self.select_everything();
                    }
                }
            }
            PendingDelete::Folder(id) => {
                if self.write(|s| s.core.delete_folder(id.clone())).is_some() {
                    self.reload_sidebar();
                    if matches!(&self.scope, Scope::Folder(f) if *f == id) {
                        self.select_everything();
                    }
                }
            }
        }
    }

    /// `{` `}`: the cursor to the folder above or below.
    pub fn select_adjacent_folder(&mut self, by: i64) {
        let folders: Vec<usize> = self
            .sidebar_rows
            .iter()
            .enumerate()
            .filter(|(_, row)| matches!(row.kind, RowKind::Folder(_)))
            .map(|(ix, _)| ix)
            .collect();
        if folders.is_empty() {
            return;
        }
        let here = self
            .sidebar_cursor_row()
            .and_then(|row| self.sidebar_rows.iter().position(|r| r.id == row.id))
            .unwrap_or(0);
        let next = if by > 0 {
            folders.iter().find(|ix| **ix > here).or(folders.first())
        } else {
            folders
                .iter()
                .rev()
                .find(|ix| **ix < here)
                .or(folders.last())
        };
        if let Some(row) = next.and_then(|ix| self.sidebar_rows.get(*ix)).cloned() {
            self.apply_sidebar_cursor(&row);
        }
    }

    /// `⇧I` on the sidebar: the row's icon.
    pub fn begin_icon_pick(&mut self) {
        let id = match self.sidebar_cursor_row().map(|row| row.kind.clone()) {
            Some(RowKind::List(id) | RowKind::Folder(id) | RowKind::NestedList(id)) => Some(id),
            Some(RowKind::FilterList(id)) => Some(id),
            _ => match &self.scope {
                Scope::List { list_id, .. } => Some(list_id.clone()),
                Scope::Folder(id) | Scope::FilterList(id) => Some(id.clone()),
                Scope::Everything => None,
            },
        };
        if let Some(row_id) = id {
            self.open_picker(Picker::Icon { row_id });
        }
    }

    /// `⌘⌥[` `⌘⌥]`: in a folder, which of its lists a new task lands in.
    pub fn step_new_task_destination(&mut self, by: i64) {
        let Scope::Folder(folder) = self.scope.clone() else {
            self.message = Some("Only a folder has a choice of list.".into());
            return;
        };
        let ids = self.scope_list_ids();
        if ids.is_empty() {
            return;
        }
        let current = self
            .prefs
            .folder_destinations
            .get(&folder)
            .and_then(|id| ids.iter().position(|other| other == id))
            .unwrap_or(0);
        let next = (current as i64 + by).rem_euclid(ids.len() as i64) as usize;
        let id = ids[next].clone();
        let name = self.list(&id).map(|l| l.name.clone()).unwrap_or_default();
        self.prefs.folder_destinations.insert(folder, id);
        self.prefs.save();
        self.message = Some(format!("New tasks go in {name}."));
    }

    /// The folder's chosen list for new tasks, when it has one.
    pub fn folder_destination(&self) -> Option<String> {
        let Scope::Folder(folder) = &self.scope else {
            return None;
        };
        self.prefs
            .folder_destinations
            .get(folder)
            .filter(|id| self.list(id).is_some())
            .cloned()
    }

    /// `taskShowProgress`: the inspector, with its keyboard.
    pub fn show_progress(&mut self) {
        if self.selected_task.is_some() {
            self.focus_region(Region::Inspector);
        }
    }
}
