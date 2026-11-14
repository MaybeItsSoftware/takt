//! Right-click menus on a task, a sidebar row and a pane's empty space
//! (`WorkspaceContextMenu.swift`). Every entry is a catalogue command run on
//! what was clicked, so a menu shows the same key a reader would press and
//! can do nothing the keys can't. ↑↓ and Return work in it too; Esc or a
//! click elsewhere closes it.

use super::sidebar::RowKind;
use super::{App, Region};

#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub title: String,
    /// A catalogue command id, or `open-link:<n>` for a task's link.
    pub action: String,
    /// The key that does it, drawn as glyphs.
    pub keys: String,
    pub destructive: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuEntry {
    Item(MenuItem),
    Divider,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextMenu {
    /// Where it opens, in window points.
    pub x: f32,
    pub y: f32,
    pub entries: Vec<MenuEntry>,
    /// The highlighted item, by index into `entries`.
    pub index: Option<usize>,
}

impl ContextMenu {
    pub fn items(&self) -> impl Iterator<Item = (usize, &MenuItem)> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(ix, e)| match e {
                MenuEntry::Item(item) => Some((ix, item)),
                MenuEntry::Divider => None,
            })
    }

    /// Moves the highlight to the next or previous item, wrapping.
    pub fn step(&mut self, by: i32) {
        let items: Vec<usize> = self.items().map(|(ix, _)| ix).collect();
        if items.is_empty() {
            return;
        }
        let at = self
            .index
            .and_then(|ix| items.iter().position(|i| *i == ix));
        let next = match at {
            None if by > 0 => 0,
            None => items.len() - 1,
            Some(at) => (at as i32 + by).rem_euclid(items.len() as i32) as usize,
        };
        self.index = Some(items[next]);
    }

    pub fn picked(&self) -> Option<&MenuItem> {
        match self.entries.get(self.index?)? {
            MenuEntry::Item(item) => Some(item),
            MenuEntry::Divider => None,
        }
    }
}

impl App {
    fn item(&self, id: &str, title: &str) -> MenuEntry {
        let keys = self
            .bindings
            .entry(id)
            .and_then(|entry| entry.keys.first())
            .map(|key| crate::keys::Bindings::glyphs(key))
            .unwrap_or_default();
        MenuEntry::Item(MenuItem {
            title: title.into(),
            action: id.into(),
            keys,
            destructive: matches!(id, "taskDelete" | "listDelete"),
        })
    }

    /// The menu for a task: it is selected first, so every entry acts on it.
    pub fn open_task_menu(&mut self, task_id: &str, x: f32, y: f32) {
        self.selected_task = Some(task_id.to_string());
        self.region = Region::Tasks;
        let Some(task) = self.task(task_id).cloned() else {
            return;
        };
        let is_list = task.item_kind.as_deref() == Some("list");
        let open = task.status == "open";
        let mut entries = vec![
            self.item(
                "planEnterTask",
                if is_list {
                    "Open list"
                } else {
                    "Open subtasks"
                },
            ),
            self.item("taskRename", "Rename…"),
            self.item("taskComplete", if open { "Complete" } else { "Reopen" }),
            self.item("taskInvalidate", "Invalidate"),
            MenuEntry::Divider,
            self.item("taskStartFocus", "Focus on it"),
            self.item("taskTogglePlannedToday", "Plan for today"),
            self.item("taskDueToday", "Due today"),
            self.item("taskDueTomorrow", "Due tomorrow"),
            self.item("taskEditDue", "Due…"),
            self.item("taskEditTags", "Tags…"),
            self.item("taskEditEstimate", "Estimate…"),
            self.item("taskEditRecurrence", "Repeat…"),
            self.item("taskConditions", "Conditions…"),
            self.item("taskWaiting", "Waiting on…"),
            self.item("taskAddBlocker", "Blocked by…"),
            self.item("taskToggleDaily", "Commit periodically…"),
            MenuEntry::Divider,
            self.item(
                "taskConvertToList",
                if is_list {
                    "Convert to task"
                } else {
                    "Convert to list"
                },
            ),
            self.item("taskMove", "Move to list or folder…"),
            self.item("taskExtractBranch", "Extract to a list"),
        ];
        if is_list {
            entries.push(self.item("taskPromoteList", "Pin to the sidebar"));
        }
        entries.push(self.item("taskAddLink", "Add a link…"));
        let links = self.task_links(task_id);
        for (ix, link) in links.iter().enumerate().take(5) {
            entries.push(MenuEntry::Item(MenuItem {
                title: format!("Open {}", link_label(link)),
                action: format!("open-link:{ix}"),
                keys: String::new(),
                destructive: false,
            }));
        }
        if !self.prefs.ai_off {
            entries.push(self.item("taskAskAgent", "Ask the agent about it"));
        }
        entries.push(MenuEntry::Divider);
        entries.push(self.item("taskDelete", "Delete"));
        self.context_menu = Some(ContextMenu {
            x,
            y,
            entries,
            index: None,
        });
    }

    /// The menu for a sidebar row, which the cursor moves to first.
    pub fn open_sidebar_menu(&mut self, row_id: &str, x: f32, y: f32) {
        let Some(row) = self.sidebar_rows.iter().find(|r| r.id == row_id).cloned() else {
            return;
        };
        self.sidebar_cursor = Some(row.id.clone());
        self.apply_sidebar_cursor(&row);
        self.region = Region::Sidebar;
        let mut entries = vec![
            self.item("listNew", "New list"),
            self.item("folderNew", "New folder"),
            self.item("filterListNew", "New filter list…"),
            MenuEntry::Divider,
        ];
        match row.kind {
            RowKind::List(_) | RowKind::NestedList(_) => {
                entries.push(self.item("listRename", "Rename…"));
                entries.push(self.item("listIcon", "Icon…"));
                entries.push(self.item("listSettings", "List settings…"));
                entries.push(self.item("listMoveUp", "Move up"));
                entries.push(self.item("listMoveDown", "Move down"));
                entries.push(self.item("listComplete", "Complete the list"));
                entries.push(self.item("listArchive", "Archive"));
                entries.push(MenuEntry::Divider);
                entries.push(self.item("listDelete", "Delete"));
            }
            RowKind::Folder(_) => {
                entries.push(self.item("listRename", "Rename…"));
                entries.push(self.item("listIcon", "Icon…"));
                entries.push(self.item("listSettings", "Folder settings…"));
                entries.push(self.item("listNewTaskDestination", "New tasks land in…"));
                entries.push(self.item("folderMoveUp", "Move up"));
                entries.push(self.item("folderMoveDown", "Move down"));
                entries.push(self.item("folderCollapseAll", "Collapse all"));
                entries.push(self.item("folderExpandAll", "Expand all"));
                entries.push(MenuEntry::Divider);
                entries.push(self.item("listDelete", "Delete"));
            }
            RowKind::FilterList(_) => {
                entries.push(self.item("filterListEdit", "Edit the rules…"));
                entries.push(self.item("listRename", "Rename…"));
                entries.push(self.item("listIcon", "Icon…"));
                entries.push(MenuEntry::Divider);
                entries.push(self.item("listDelete", "Delete"));
            }
            _ => {
                entries.pop();
            }
        }
        self.context_menu = Some(ContextMenu {
            x,
            y,
            entries,
            index: None,
        });
    }

    /// The menu for a pane's empty space: what to add, how to look.
    pub fn open_pane_menu(&mut self, x: f32, y: f32) {
        self.region = Region::Tasks;
        let entries = vec![
            self.item("taskNew", "New task"),
            MenuEntry::Divider,
            self.item("planHideCompleted", "Hide or show completed"),
            self.item("folderCollapseAll", "Collapse all"),
            self.item("folderExpandAll", "Expand all"),
            self.item("goCommandPalette", "Command palette…"),
        ];
        let entries = entries
            .into_iter()
            .filter(|entry| match entry {
                MenuEntry::Item(item) => self.bindings.entry(&item.action).is_some(),
                MenuEntry::Divider => true,
            })
            .collect();
        self.context_menu = Some(ContextMenu {
            x,
            y,
            entries,
            index: None,
        });
    }

    /// The account button at the top right: who is signed in, and what
    /// can be done about it.
    pub fn open_account_menu(&mut self, x: f32, y: f32) {
        let item = |title: &str, action: &str, keys: &str, destructive| {
            MenuEntry::Item(MenuItem {
                title: title.into(),
                action: action.into(),
                keys: keys.into(),
                destructive,
            })
        };
        let mut entries = Vec::new();
        if self.sync.signed_in() {
            let who = self
                .sync
                .status()
                .email
                .unwrap_or_else(|| "Signed in".into());
            entries.push(item(&who, "account:settings", "", false));
            entries.push(MenuEntry::Divider);
            entries.push(item("Sync now", "account:sync-now", "", false));
        } else {
            entries.push(item("Sign in to sync…", "account:settings", "", false));
            entries.push(MenuEntry::Divider);
        }
        entries.push(item("Settings…", "account:settings", "⌘,", false));
        if self.sync.signed_in() {
            entries.push(MenuEntry::Divider);
            entries.push(item("Sign out", "account:sign-out", "", true));
        }
        self.context_menu = Some(ContextMenu {
            x,
            y,
            entries,
            index: None,
        });
    }

    /// The status bar's history clicked: the journal's steps, newest first.
    /// Choosing one undoes what came after it, or redoes up to it when it
    /// was undone.
    pub fn open_history_menu(&mut self, x: f32, y: f32) {
        let steps = self.store.core.undo_history(30).unwrap_or_default();
        let mut entries = vec![MenuEntry::Item(MenuItem {
            title: if steps.is_empty() {
                "Nothing to undo yet".into()
            } else {
                "History — choose a step to go back to it".into()
            },
            action: "history:none".into(),
            keys: String::new(),
            destructive: false,
        })];
        if !steps.is_empty() {
            entries.push(MenuEntry::Divider);
        }
        let newest_done = steps.iter().position(|s| !s.is_undone);
        for (ix, step) in steps.iter().enumerate() {
            entries.push(MenuEntry::Item(MenuItem {
                title: if step.is_undone {
                    format!("{}  (undone)", step.label)
                } else {
                    step.label.clone()
                },
                action: format!("history:{ix}"),
                keys: if Some(ix) == newest_done {
                    "⌘Z".into()
                } else {
                    String::new()
                },
                destructive: false,
            }));
        }
        self.context_menu = Some(ContextMenu {
            x,
            y,
            entries,
            index: None,
        });
    }

    /// Back or forward through the journal to step `ix` of the history as
    /// the menu listed it (newest first).
    fn go_to_history_step(&mut self, ix: usize) {
        let steps = self.store.core.undo_history(30).unwrap_or_default();
        let Some(step) = steps.get(ix) else { return };
        if step.is_undone {
            // Redo the undone steps from the oldest up to and including it.
            let count = steps[..=ix].iter().filter(|s| s.is_undone).count();
            for _ in 0..count {
                self.undo(true);
            }
        } else {
            // Undo the done steps newer than it.
            let count = steps[..ix].iter().filter(|s| !s.is_undone).count();
            for _ in 0..count {
                self.undo(false);
            }
        }
    }

    fn task_links(&self, task_id: &str) -> Vec<String> {
        self.store
            .core
            .editor_snapshot(task_id.into())
            .map(|snapshot| snapshot.metadata.external_links)
            .unwrap_or_default()
    }

    /// The menu for one of a task's links: open it, save a Drive file (to
    /// Downloads, or where the person chooses), show a file or note in
    /// Finder, or take the link off.
    pub fn open_link_menu(&mut self, link: &takt_core::links::TaskLink, x: f32, y: f32) {
        use takt_core::links::LinkKind;
        let url = &link.url;
        let item = |title: &str, action: String, destructive: bool| {
            MenuEntry::Item(MenuItem {
                title: title.into(),
                action,
                keys: String::new(),
                destructive,
            })
        };
        let mut entries = vec![item("Open", format!("link-open:{url}"), false)];
        if link.kind == LinkKind::Drive && !link.is_folder {
            entries.push(item("Save to Downloads", format!("link-save:{url}"), false));
            entries.push(item("Save to…", format!("link-save-as:{url}"), false));
        }
        if self.local_path_of(url).is_some() {
            entries.push(item("Show in Finder", format!("link-reveal:{url}"), false));
        }
        entries.push(MenuEntry::Divider);
        entries.push(item("Remove link", format!("link-remove:{url}"), true));
        self.context_menu = Some(ContextMenu {
            x,
            y,
            entries,
            index: None,
        });
    }

    /// The file on this Mac a link names: a `file://` address or path, or
    /// an Obsidian note found in its vault.
    fn local_path_of(&self, url: &str) -> Option<std::path::PathBuf> {
        if let Some(path) = url.strip_prefix("file://") {
            return Some(std::path::PathBuf::from(super::notes_editor::decode(path)));
        }
        if url.starts_with('/') {
            return Some(std::path::PathBuf::from(url));
        }
        super::notes_editor::resolve(url, &self.obsidian_vaults()).and_then(|note| note.path)
    }

    /// A link's menu item, run: false when the action wasn't one.
    fn run_link_action(&mut self, action: &str) -> bool {
        let Some((verb, url)) = action.split_once(':') else {
            return false;
        };
        let url = url.to_string();
        match verb {
            "link-open" => super::pickers::open_address(&url),
            "link-save" => {
                let client = self.google_client_id();
                self.message = Some("Saving from Drive…".into());
                std::thread::spawn(move || {
                    let said = super::links::save_drive_file(&client, &url, None);
                    crate::sync::announce(&said, false);
                });
            }
            "link-save-as" => {
                let name = self
                    .selected_task
                    .clone()
                    .and_then(|id| self.store.core.task_links(id).ok())
                    .unwrap_or_default()
                    .into_iter()
                    .find(|link| link.url == url)
                    .map(|link| link.title)
                    .unwrap_or_default();
                self.wants_save_as = Some((url, name));
            }
            "link-reveal" => {
                if let Some(path) = self.local_path_of(&url) {
                    let _ = std::process::Command::new("/usr/bin/open")
                        .arg("-R")
                        .arg(path)
                        .spawn();
                }
            }
            "link-remove" => self.edit_selected(|edit| {
                edit.metadata.external_links.retain(|link| link != &url);
            }),
            _ => return false,
        }
        true
    }

    /// Runs what was picked and closes the menu.
    pub fn pick_from_menu(&mut self, action: &str) {
        self.context_menu = None;
        match action {
            "account:settings" => return self.open_overlay(super::Overlay::Settings),
            "account:sync-now" => {
                self.sync.sync_now();
                return;
            }
            "account:sign-out" => {
                self.sync.sign_out();
                return;
            }
            "history:none" => return,
            _ => {}
        }
        if let Some(ix) = action.strip_prefix("history:") {
            if let Ok(ix) = ix.parse::<usize>() {
                self.go_to_history_step(ix);
            }
            return;
        }
        if action.starts_with("link-") && self.run_link_action(action) {
            return;
        }
        if let Some(ix) = action.strip_prefix("open-link:") {
            let links = self
                .selected_task
                .clone()
                .map(|id| self.task_links(&id))
                .unwrap_or_default();
            if let Some(link) = ix.parse::<usize>().ok().and_then(|ix| links.get(ix)) {
                let _ = std::process::Command::new("/usr/bin/open")
                    .arg(link)
                    .spawn();
            }
            return;
        }
        self.run(action, None);
    }
}

/// A link as a menu names it: its host, or the file's name.
fn link_label(link: &str) -> String {
    let trimmed = link.trim_end_matches('/');
    let after_scheme = trimmed.split_once("://").map_or(trimmed, |(_, rest)| rest);
    let label = if link.starts_with("file://") || link.starts_with('/') {
        after_scheme.rsplit('/').next().unwrap_or(after_scheme)
    } else {
        after_scheme.split('/').next().unwrap_or(after_scheme)
    };
    let label: String = label.chars().take(40).collect();
    format!("“{label}”")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str) -> MenuEntry {
        MenuEntry::Item(MenuItem {
            title: title.into(),
            action: title.into(),
            keys: String::new(),
            destructive: false,
        })
    }

    #[test]
    fn the_highlight_skips_dividers_and_wraps() {
        let mut menu = ContextMenu {
            x: 0.,
            y: 0.,
            entries: vec![item("a"), MenuEntry::Divider, item("b")],
            index: None,
        };
        menu.step(1);
        assert_eq!(menu.picked().map(|i| i.title.as_str()), Some("a"));
        menu.step(1);
        assert_eq!(menu.picked().map(|i| i.title.as_str()), Some("b"));
        menu.step(1);
        assert_eq!(menu.picked().map(|i| i.title.as_str()), Some("a"));
        menu.step(-1);
        assert_eq!(menu.picked().map(|i| i.title.as_str()), Some("b"));
    }
}
