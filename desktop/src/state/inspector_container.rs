//! The inspector on a list or a folder: what it is, where it sits and what
//! can be done to it, as the same column of stops a task has. It shows while
//! the keyboard is in the sidebar on a list or folder row, after stepping
//! from there into the inspector, and whenever no task is selected.
//!
//! A list's name, folder, icon, agent context, sharing, completion and
//! archiving; a folder's name and the folder it sits in. Everything else a
//! list has (its root task, its colour) stays on its settings sheet (`oo`),
//! which the last row opens.

use super::inspector::{Stop, StopKind};
use super::store::now_ms;
use super::{App, Region, Scope};
use takt_core::agent_instructions::AgentInstructionKind;

/// What the inspector is showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    Task(String),
    List(String),
    Folder(String),
}

impl App {
    /// The list or folder on screen, if the scope is one (not a nested
    /// list, which is a task and inspected as one).
    fn scope_container(&self) -> Option<Subject> {
        match &self.scope {
            Scope::List {
                list_id,
                entered: None,
            } if self.list(list_id).is_some() => Some(Subject::List(list_id.clone())),
            Scope::Folder(id) if self.folder(id).is_some() => Some(Subject::Folder(id.clone())),
            _ => None,
        }
    }

    /// What the inspector shows: the list or folder while the keyboard is
    /// on it in the sidebar (or came into the inspector from there), or
    /// when no task is selected; else the selected task.
    pub fn inspector_subject(&self) -> Option<Subject> {
        let container = self.scope_container();
        let wants_container = self.region == Region::Sidebar
            || (self.region == Region::Inspector && self.inspecting_container)
            || self.selected_task.is_none();
        if wants_container && container.is_some() {
            return container;
        }
        self.selected_task.clone().map(Subject::Task)
    }

    /// The list or folder the inspector is on, if it isn't on a task.
    pub fn inspected_container(&self) -> Option<Subject> {
        self.inspector_subject()
            .filter(|s| !matches!(s, Subject::Task(_)))
    }

    /// Whether the inspector has anything to take the keyboard to.
    pub fn can_inspect(&self) -> bool {
        self.selected().is_some() || self.scope_container().is_some()
    }

    /// The keyboard goes into the inspector, onto the list or folder if it
    /// comes from the sidebar.
    pub fn enter_inspector_region(&mut self) {
        if self.region != Region::Inspector {
            let from_sidebar = self.region == Region::Sidebar;
            if from_sidebar != self.inspecting_container {
                self.inspector_cursor = 0;
            }
            self.inspecting_container = from_sidebar;
        }
        self.region = Region::Inspector;
    }

    pub(super) fn container_stops(&self, subject: &Subject) -> Vec<Stop> {
        match subject {
            Subject::List(id) => self.list_stops(id),
            Subject::Folder(id) => self.folder_stops(id),
            Subject::Task(_) => Vec::new(),
        }
    }

    fn list_agent_context(&self, list_id: &str) -> String {
        self.store
            .core
            .agent_instructions()
            .unwrap_or_default()
            .into_iter()
            .find(|i| i.kind == AgentInstructionKind::List && i.id.as_deref() == Some(list_id))
            .map(|i| i.text)
            .unwrap_or_default()
    }

    fn folder_name(&self, id: Option<&str>) -> Option<String> {
        id.and_then(|id| self.folder(id)).map(|f| f.name.clone())
    }

    fn list_stops(&self, id: &str) -> Vec<Stop> {
        let Some(list) = self.list(id).cloned() else {
            return Vec::new();
        };
        let inbox = list.system_role.as_deref() == Some("inbox");
        let mut stops = Vec::new();
        stops.push(Stop::new(
            "title",
            "",
            if inbox {
                StopKind::Action
            } else {
                StopKind::Text { multiline: false }
            },
        ));
        stops[0].value = list.name.clone();

        let shared = self.shared_list(id);
        let mut said = vec![if inbox { "The inbox" } else { "List" }.to_string()];
        let count = self.task_counts.get(id).copied().unwrap_or(0);
        said.push(match count {
            0 => "no tasks".into(),
            1 => "1 task".into(),
            n => format!("{n} tasks"),
        });
        if list.completed_at_ms.is_some() {
            said.push("completed".into());
        }
        if list.is_archived {
            said.push("archived".into());
        }
        if shared.is_some() {
            said.push("shared".into());
        }
        stops.push(
            Stop::new("container-summary", "", StopKind::Action).value(Some(said.join(" · ")), ""),
        );

        if !inbox {
            stops.push(
                Stop::new("container-folder", "Folder", StopKind::Choice)
                    .value(
                        self.folder_name(list.folder_id.as_deref()),
                        "None: top level",
                    )
                    .grouped(),
            );
        }
        stops.push(
            Stop::new("container-icon", "Icon", StopKind::Action)
                .value(self.prefs.icons.get(id).cloned(), "Choose an icon"),
        );
        if !self.prefs.ai_off {
            stops.push(
                Stop::new(
                    "list-context",
                    "Agent context",
                    StopKind::Text { multiline: true },
                )
                .value(
                    Some(self.list_agent_context(id)),
                    "What the agent should know about this list",
                ),
            );
        }
        if !inbox {
            let sharing = match &shared {
                Some(s) => {
                    let others = s.members.iter().filter(|m| !m.you).count();
                    Some(match others {
                        0 => "Shared: nobody has joined yet".to_string(),
                        1 => "Shared with 1 person".to_string(),
                        n => format!("Shared with {n} people"),
                    })
                }
                None => None,
            };
            stops.push(
                Stop::new("list-sharing", "Sharing", StopKind::Action)
                    .value(sharing, "Share with another Takt account"),
            );
            stops.push(
                Stop::new("list-complete", "", StopKind::Action)
                    .value(
                        Some(
                            if list.completed_at_ms.is_some() {
                                "Reopen the list"
                            } else {
                                "Complete the list"
                            }
                            .into(),
                        ),
                        "",
                    )
                    .grouped(),
            );
            stops.push(
                Stop::new("list-archive", "", StopKind::Action).value(
                    Some(
                        if list.is_archived {
                            "Restore the list"
                        } else {
                            "Archive the list"
                        }
                        .into(),
                    ),
                    "",
                ),
            );
        }
        stops.push(
            Stop::new("container-settings", "", StopKind::Action)
                .value(Some("More settings: root task, colour".into()), "")
                .grouped(),
        );
        stops
    }

    fn folder_stops(&self, id: &str) -> Vec<Stop> {
        let Some(folder) = self.folder(id).cloned() else {
            return Vec::new();
        };
        let mut title = Stop::new("title", "", StopKind::Text { multiline: false });
        title.value = folder.name.clone();
        let lists = self
            .lists
            .iter()
            .filter(|l| l.folder_id.as_deref() == Some(id) && !l.is_archived)
            .count();
        let folders = self
            .folders
            .iter()
            .filter(|f| f.parent_folder_id.as_deref() == Some(id))
            .count();
        let mut said = vec!["Folder".to_string()];
        said.push(match lists {
            1 => "1 list".into(),
            n => format!("{n} lists"),
        });
        if folders > 0 {
            said.push(match folders {
                1 => "1 folder".into(),
                n => format!("{n} folders"),
            });
        }
        vec![
            title,
            Stop::new("container-summary", "", StopKind::Action).value(Some(said.join(" · ")), ""),
            Stop::new("container-folder", "Inside", StopKind::Choice)
                .value(
                    self.folder_name(folder.parent_folder_id.as_deref()),
                    "None: top level",
                )
                .grouped(),
            Stop::new("container-icon", "Icon", StopKind::Action)
                .value(self.prefs.icons.get(id).cloned(), "Choose an icon"),
        ]
    }

    /// The folders a list or folder could be moved into, in sidebar order,
    /// after "none": a folder can't go inside itself or what it holds.
    fn folder_choices(&self, subject: &Subject) -> Vec<Option<String>> {
        let moving = match subject {
            Subject::Folder(id) => Some(id.as_str()),
            _ => None,
        };
        // Whether `id` is the folder being moved or inside it.
        let inside_moving = |id: &str| {
            let mut at = Some(id.to_string());
            let mut seen = 0;
            while let Some(id) = at {
                if Some(id.as_str()) == moving || seen > self.folders.len() {
                    return true;
                }
                seen += 1;
                at = self.folder(&id).and_then(|f| f.parent_folder_id.clone());
            }
            false
        };
        let mut ordered: Vec<Option<String>> = vec![None];
        for row in &self.sidebar_rows_expanding(true) {
            if let super::sidebar::RowKind::Folder(id) = &row.kind
                && !inside_moving(id)
            {
                ordered.push(Some(id.clone()));
            }
        }
        ordered
    }

    /// ←/→ (and Return) on the folder row: the next folder it could sit in.
    pub(super) fn step_container_folder(&mut self, subject: &Subject, by: i64) {
        let choices = self.folder_choices(subject);
        let current = match subject {
            Subject::List(id) => self.list(id).and_then(|l| l.folder_id.clone()),
            Subject::Folder(id) => self.folder(id).and_then(|f| f.parent_folder_id.clone()),
            Subject::Task(_) => return,
        };
        let at = choices.iter().position(|c| *c == current).unwrap_or(0) as i64;
        let next = choices[(at + by).rem_euclid(choices.len() as i64) as usize].clone();
        self.set_container_folder(subject, next);
    }

    fn set_container_folder(&mut self, subject: &Subject, folder: Option<String>) {
        let now = now_ms();
        let saved = match subject {
            Subject::List(id) => {
                let Some(list) = self.list(id).cloned() else {
                    return;
                };
                let settings = takt_core::lists::ListSettings {
                    name: list.name,
                    colour_hex: list.color_hex,
                    folder_id: folder.clone(),
                    is_archived: list.is_archived,
                    visible_root_task_id: list.visible_root_task_id,
                };
                let id = id.clone();
                self.write(|s| s.core.save_list_settings(id, settings, now))
                    .is_some()
            }
            Subject::Folder(id) => {
                let Some(name) = self.folder(id).map(|f| f.name.clone()) else {
                    return;
                };
                let id = id.clone();
                self.write(|s| s.core.save_folder_settings(id, name, folder.clone(), now))
                    .is_some()
            }
            Subject::Task(_) => false,
        };
        if saved {
            // Open the way down to it, so its row can still be seen.
            if let Some(id) = &folder {
                self.prefs.expanded_folders.insert(id.clone());
                self.prefs.save();
            }
            self.reload_sidebar();
            self.reload_rows();
        }
    }

    fn rename_container(&mut self, subject: &Subject, name: &str) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let now = now_ms();
        let saved = match subject {
            Subject::List(id) => {
                let Some(list) = self.list(id).cloned() else {
                    return;
                };
                let settings = takt_core::lists::ListSettings {
                    name,
                    colour_hex: list.color_hex,
                    folder_id: list.folder_id,
                    is_archived: list.is_archived,
                    visible_root_task_id: list.visible_root_task_id,
                };
                let id = id.clone();
                self.write(|s| s.core.save_list_settings(id, settings, now))
            }
            Subject::Folder(id) => {
                let Some(parent) = self.folder(id).map(|f| f.parent_folder_id.clone()) else {
                    return;
                };
                let id = id.clone();
                self.write(|s| s.core.save_folder_settings(id, name, parent, now))
            }
            Subject::Task(_) => None,
        };
        if saved.is_some() {
            self.reload_sidebar();
            self.reload_rows();
        }
    }

    fn set_list_context(&mut self, list_id: &str, text: &str) {
        let text = text.trim().to_string();
        let list = list_id.to_string();
        let now = now_ms();
        if self
            .write(|s| {
                s.core.set_agent_instructions(
                    AgentInstructionKind::List,
                    Some(list),
                    text,
                    false,
                    now,
                )
            })
            .is_some()
        {
            self.message = Some("Agent context saved.".into());
        }
    }

    /// Return on an action row of a list or folder.
    pub(super) fn press_container_action(&mut self, subject: &Subject, stop: &str) {
        match (stop, subject) {
            ("container-icon", _) => self.begin_icon_pick(),
            ("container-settings", _) => self.begin_list_settings(),
            ("list-sharing", Subject::List(id)) => {
                if self.shared_list(id).is_some() {
                    self.run("listMembers", None);
                } else {
                    self.run("listShare", None);
                }
            }
            ("list-complete", Subject::List(_)) => self.toggle_list_completed(),
            ("list-archive", Subject::List(id)) => {
                let Some(archived) = self.list(id).map(|l| l.is_archived) else {
                    return;
                };
                if archived {
                    let id = id.clone();
                    if self
                        .write(|s| s.core.set_list_archived(id, false, now_ms()))
                        .is_some()
                    {
                        self.reload_sidebar();
                        self.message = Some("Restored.".into());
                    }
                } else {
                    self.run("listArchive", None);
                    self.inspecting_container = false;
                    self.region = Region::Tasks;
                }
            }
            _ => {}
        }
    }

    /// What a text row's field opens with.
    pub(super) fn container_field_text(&self, subject: &Subject, stop: &str) -> String {
        match (stop, subject) {
            ("title", Subject::List(id)) => self.list(id).map(|l| l.name.clone()),
            ("title", Subject::Folder(id)) => self.folder(id).map(|f| f.name.clone()),
            ("list-context", Subject::List(id)) => Some(self.list_agent_context(id)),
            _ => None,
        }
        .unwrap_or_default()
    }

    /// A text row's field closed with Return.
    pub(super) fn commit_container_field(&mut self, subject: &Subject, stop: &str, text: &str) {
        match (stop, subject) {
            ("title", _) => self.rename_container(subject, text),
            ("list-context", Subject::List(id)) => {
                let id = id.clone();
                self.set_list_context(&id, text);
            }
            _ => {}
        }
    }

    /// `x` on a row of a list or folder.
    pub(super) fn clear_container_stop(&mut self, subject: &Subject, stop: &str) {
        match (stop, subject) {
            ("container-folder", _) => self.set_container_folder(subject, None),
            ("list-context", Subject::List(id)) => {
                let id = id.clone();
                self.set_list_context(&id, "");
            }
            ("container-icon", Subject::List(id) | Subject::Folder(id))
                if self.prefs.icons.remove(id).is_some() =>
            {
                self.prefs.save();
            }
            _ => {}
        }
    }
}
