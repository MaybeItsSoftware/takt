//! What the menu bar item says and what its menu holds, from the state
//! alone; `platform::status_item` draws it. `MenuBarController.swift`.
//!
//! The item names the next thing on today, or the running block and its
//! clock. The menu is the block's controls, the day (each starts a block),
//! and the ways into the app.

use super::App;
use super::focus::{clock, elapsed_seconds};
use super::store::now_ms;
use crate::platform::hotkeys;
use crate::platform::status_item::{Entry, Snapshot, clip};

/// Past this the menu is a list you scan, and the focus panel is better.
const DAY_MENU_LIMIT: usize = 8;

fn item(title: impl Into<String>, action: &str) -> Entry {
    Entry::Item {
        title: title.into(),
        action: action.into(),
        indent: 0,
        checked: false,
        tooltip: None,
    }
}

fn indented(title: impl Into<String>, action: &str, tooltip: Option<&str>) -> Entry {
    Entry::Item {
        title: title.into(),
        action: action.into(),
        indent: 1,
        checked: false,
        tooltip: tooltip.map(str::to_string),
    }
}

impl App {
    /// The running block's task, wherever it is.
    fn running_title(&self) -> Option<String> {
        let id = self.focus_session.as_ref()?.active_task_id.clone()?;
        self.task(&id).map(|task| task.title.clone()).or_else(|| {
            self.store
                .core
                .task(id)
                .ok()
                .flatten()
                .map(|task| task.title)
        })
    }

    /// The item and its menu as things stand.
    pub fn status_snapshot(&self) -> Snapshot {
        let mut snapshot = Snapshot {
            visible: true,
            tooltip: "Open Takt".into(),
            ..Default::default()
        };
        if let Some(session) = &self.focus_session {
            let now = now_ms();
            let title = self.running_title().unwrap_or_else(|| "Focus".into());
            let (reading, over) = if session.phase == "onBreak" {
                let left = session
                    .break_ends_at_ms
                    .map(|end| (end - now) / 1000)
                    .unwrap_or(0);
                (format!("Break {}", clock(left.max(0))), left < 0)
            } else {
                let elapsed = elapsed_seconds(session, now);
                let planned = session.work_duration_seconds.max(1);
                let paused = session.paused_at_ms.is_some();
                let left = planned - elapsed;
                let text = if left >= 0 {
                    clock(left)
                } else {
                    format!("+{}", clock(-left))
                };
                (if paused { format!("⏸ {text}") } else { text }, left < 0)
            };
            snapshot.title = format!("{}  {reading}", clip(&title, 28));
            snapshot.monospaced = true;
            snapshot.warn = over;
            snapshot.tooltip = if self.focus_prompt.is_some() {
                format!("Waiting on how that block went: {title}")
            } else if session.phase == "onBreak" {
                format!("On a break from: {title}")
            } else if session.paused_at_ms.is_some() {
                format!("Paused: {title}")
            } else {
                format!("Focusing: {title}")
            };
        } else if let Some(next) = self.today.day.first() {
            let more = self.today.day.len() - 1;
            snapshot.title = if more > 0 {
                format!("{}  +{more}", clip(&next.task.title, 24))
            } else {
                clip(&next.task.title, 28)
            };
            snapshot.warn = next.reason == "overdue";
            snapshot.tooltip = if self.today.day.len() == 1 {
                "1 task today — click for the list".into()
            } else {
                format!("{} tasks today — click for the list", self.today.day.len())
            };
        }
        snapshot.menu = self.status_menu();
        snapshot
    }

    fn status_menu(&self) -> Vec<Entry> {
        let mut menu = Vec::new();
        if let Some(session) = &self.focus_session {
            let title = self
                .running_title()
                .map(|title| clip(&title, 44))
                .unwrap_or_else(|| "Focus session".into());
            menu.push(Entry::Label { title, indent: 0 });
            if self.focus_prompt.is_some() {
                menu.push(indented("How did that go?", "focus-panel", None));
            } else if let Some((_, reading, _)) = self.focus_status() {
                let on_break = session.phase == "onBreak";
                menu.push(Entry::Label {
                    title: reading,
                    indent: 1,
                });
                let pause = if on_break {
                    "Back to work"
                } else if session.paused_at_ms.is_some() {
                    "Resume"
                } else {
                    "Pause"
                };
                menu.push(indented(pause, "focus-pause", None));
                if !on_break && session.active_task_id.is_some() {
                    menu.push(indented(
                        "Done",
                        "focus-done",
                        Some("Complete the task and close the block"),
                    ));
                    menu.push(indented(
                        "End Block",
                        "focus-end",
                        Some("Close the block and keep the task"),
                    ));
                }
            }
            menu.push(indented(
                "Stop Focusing",
                "focus-stop",
                Some("Log the time so far, keep the task open and drop the queue"),
            ));
            menu.push(Entry::Separator);
        }
        if !self.today.day.is_empty() {
            menu.push(Entry::Label {
                title: "Today".into(),
                indent: 0,
            });
            let running = self
                .focus_session
                .as_ref()
                .and_then(|s| s.active_task_id.clone());
            for row in self.today.day.iter().take(DAY_MENU_LIMIT) {
                menu.push(Entry::Item {
                    title: clip(&row.task.title, 44),
                    action: format!("start:{}", row.task.id),
                    indent: 1,
                    checked: running.as_deref() == Some(row.task.id.as_str()),
                    tooltip: row.explanation.clone(),
                });
            }
            if self.today.day.len() > DAY_MENU_LIMIT {
                menu.push(indented(
                    format!("{} more…", self.today.day.len() - DAY_MENU_LIMIT),
                    "focus-panel",
                    None,
                ));
            }
            menu.push(Entry::Separator);
        }
        let tip = |spelled: &Option<String>| {
            spelled
                .as_deref()
                .filter(|_| self.prefs.hotkeys_enabled)
                .map(|s| format!("{} from any app", hotkeys::display(s)))
        };
        let with_tip = |title: &str, action: &str, spelled: &Option<String>| Entry::Item {
            title: title.into(),
            action: action.into(),
            indent: 0,
            checked: false,
            tooltip: tip(spelled),
        };
        menu.push(with_tip(
            "Quick Add…",
            "quick-capture",
            &self.prefs.hotkey_quick_capture,
        ));
        menu.push(with_tip(
            "Focus Panel",
            "focus-panel",
            &self.prefs.hotkey_focus_panel,
        ));
        menu.push(with_tip(
            "Open Main Window",
            "show-window",
            &self.prefs.hotkey_show_window,
        ));
        menu.push(Entry::Separator);
        menu.push(item("Settings…", "settings"));
        menu.push(Entry::Separator);
        menu.push(item("Quit Takt", "quit"));
        menu
    }

    /// A menu action that is the state's alone; false for the ones the
    /// window does (windows, quitting).
    pub fn status_action(&mut self, action: &str) -> bool {
        match action {
            "focus-pause" => {
                if self
                    .focus_session
                    .as_ref()
                    .is_some_and(|s| s.phase == "onBreak")
                {
                    self.end_focus_break();
                } else {
                    self.toggle_focus_pause();
                }
            }
            // Closing a block asks how it went, so the panel comes up for
            // the question to have somewhere to be.
            "focus-done" | "focus-end" => {
                self.request_focus_completion(action == "focus-done");
                self.wants_panel = Some(true);
            }
            "focus-stop" => self.stop_focus(),
            _ => {
                let Some(id) = action.strip_prefix("start:") else {
                    return false;
                };
                // Picking from the day is a decision to work on that
                // instead, not to queue it.
                self.selected_task = Some(id.to_string());
                self.start_focus_on_selected(true);
            }
        }
        true
    }
}
