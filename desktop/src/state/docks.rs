//! The docks and their tabs, Zed's way: the right dock helps with the task
//! in front of you (Inspector, Conditions), the bottom dock keeps track
//! (Progress, Done, Timeline). A tab that takes the keyboard is a region;
//! asking for a tab from inside it puts the dock away.
//! `WorkspaceViewModel+Dock.swift` and `+Done.swift`.

use super::{App, Region};

pub const RIGHT_TABS: [&str; 2] = ["inspector", "conditions"];
pub const BOTTOM_TABS: [&str; 3] = ["progress", "done", "timeline"];

/// The region a dock tab is, when it takes the keyboard.
pub fn tab_region(tab: &str) -> Option<Region> {
    match tab {
        "inspector" => Some(Region::Inspector),
        "conditions" => Some(Region::Conditions),
        "done" => Some(Region::Done),
        "timeline" => Some(Region::Timeline),
        _ => None,
    }
}

impl App {
    pub fn toggle_sidebar(&mut self) {
        self.prefs.sidebar_visible = !self.prefs.sidebar_visible;
        if !self.prefs.sidebar_visible && self.region == Region::Sidebar {
            self.region = Region::Tasks;
        }
        self.prefs.save();
    }

    /// `⌘R`: the right dock, on the tab it was on, with the keyboard in it.
    pub fn toggle_right_dock(&mut self) {
        if self.prefs.right_dock_visible {
            self.hide_right_dock();
        } else {
            let tab = self.prefs.right_dock_tab.clone();
            self.show_right_tab(&tab, true);
        }
    }

    pub fn hide_right_dock(&mut self) {
        self.prefs.right_dock_visible = false;
        if matches!(self.region, Region::Inspector | Region::Conditions) {
            self.region = Region::Tasks;
        }
        self.prefs.save();
    }

    pub fn show_right_tab(&mut self, tab: &str, focus: bool) {
        self.prefs.right_dock_visible = true;
        self.prefs.right_dock_tab = tab.into();
        self.prefs.save();
        if tab == "conditions" {
            self.reload_conditions();
        }
        if focus
            && let Some(region) = tab_region(tab)
            && (region != Region::Inspector || self.selected().is_some())
        {
            self.region = region;
        }
    }

    /// A right tab's command: shows it, or from inside it puts the dock away.
    pub fn toggle_right_tab(&mut self, tab: &str) {
        let showing = self.prefs.right_dock_visible && self.prefs.right_dock_tab == tab;
        if showing && tab_region(tab) == Some(self.region) {
            self.hide_right_dock();
        } else {
            self.show_right_tab(tab, true);
        }
    }

    /// `⌘J`: the bottom dock, on the tab it was on.
    pub fn toggle_bottom_dock(&mut self) {
        if self.prefs.bottom_dock_visible {
            self.hide_bottom_dock();
        } else {
            let tab = self.prefs.bottom_dock_tab.clone();
            self.show_bottom_tab(&tab);
        }
    }

    pub fn hide_bottom_dock(&mut self) {
        self.prefs.bottom_dock_visible = false;
        if matches!(self.region, Region::Done | Region::Timeline) {
            self.region = Region::Tasks;
        }
        self.prefs.save();
    }

    pub fn show_bottom_tab(&mut self, tab: &str) {
        self.prefs.bottom_dock_visible = true;
        self.prefs.bottom_dock_tab = tab.into();
        self.prefs.save();
        match tab {
            "done" => {
                self.reload_done();
                if self.done_cursor.is_none() {
                    self.done_cursor = self.done.first().map(|t| t.id.clone());
                }
            }
            "timeline" => self.reload_timeline(),
            "progress" => self.reload_progress(),
            _ => {}
        }
        if let Some(region) = tab_region(tab) {
            self.region = region;
        }
    }

    /// A bottom tab's command: shows it with the keyboard, or from inside it
    /// puts the dock away.
    pub fn toggle_bottom_tab(&mut self, tab: &str) {
        let showing = self.prefs.bottom_dock_visible && self.prefs.bottom_dock_tab == tab;
        let inside = tab_region(tab).is_none_or(|region| region == self.region);
        if showing && inside {
            self.hide_bottom_dock();
        } else {
            self.show_bottom_tab(tab);
        }
    }

    /// `]` `[`: the next tab of the dock with the keyboard.
    pub fn cycle_dock_tab(&mut self, by: i64) {
        let bottom = matches!(self.region, Region::Done | Region::Timeline);
        let tabs: &[&str] = if bottom { &BOTTOM_TABS } else { &RIGHT_TABS };
        let current = if bottom {
            &self.prefs.bottom_dock_tab
        } else {
            &self.prefs.right_dock_tab
        };
        let ix = tabs.iter().position(|t| t == current).unwrap_or(0) as i64;
        let next = tabs[(ix + by).rem_euclid(tabs.len() as i64) as usize];
        if bottom {
            self.show_bottom_tab(next);
        } else {
            self.show_right_tab(next, true);
        }
    }

    pub fn close_all_docks(&mut self) {
        self.prefs.sidebar_visible = false;
        self.prefs.right_dock_visible = false;
        self.prefs.bottom_dock_visible = false;
        self.region = Region::Tasks;
        self.prefs.save();
    }

    // MARK: The done rail

    pub fn move_done_cursor(&mut self, key: &str) {
        let ids: Vec<String> = self.done.iter().map(|t| t.id.clone()).collect();
        if ids.is_empty() {
            return;
        }
        let here = self
            .done_cursor
            .as_ref()
            .and_then(|id| ids.iter().position(|other| other == id));
        let last = ids.len() - 1;
        let next = match key {
            "j" | "down" => here.map_or(0, |ix| (ix + 1).min(last)),
            "k" | "up" => here.map_or(0, |ix| ix.saturating_sub(1)),
            "gg" | "home" | "cmd+up" => 0,
            "shift+g" | "end" | "cmd+down" => last,
            "ctrl+d" | "pagedown" => here.map_or(0, |ix| (ix + 10).min(last)),
            "ctrl+u" | "pageup" => here.map_or(0, |ix| ix.saturating_sub(10)),
            _ => here.unwrap_or(0),
        };
        self.done_cursor = Some(ids[next].clone());
    }

    /// `o` on the done rail: the task where it lives, selected.
    pub fn reveal_done(&mut self) {
        let Some(task) = self
            .done_cursor
            .as_ref()
            .and_then(|id| self.done.iter().find(|t| &t.id == id))
            .cloned()
        else {
            return;
        };
        self.reveal_task(&task.id, &task.list_id);
    }

    /// A task from a dock tab shown where it lives, with the keyboard on it.
    pub fn reveal_task(&mut self, id: &str, list_id: &str) {
        if self.view_mode == super::ViewMode::Today {
            self.select_view_mode(super::ViewMode::Outline);
        }
        self.select_list(list_id);
        if !self.prefs.hide_completed {
            self.selected_task = Some(id.to_string());
        } else {
            self.lingering.insert(id.to_string());
            self.layout_rows();
            self.selected_task = Some(id.to_string());
        }
        self.region = Region::Tasks;
    }

    /// `r` on the done rail: open again, the cursor to the next.
    pub fn reopen_done(&mut self) {
        let Some(id) = self.done_cursor.clone() else {
            return;
        };
        let successor = self
            .done
            .iter()
            .position(|t| t.id == id)
            .and_then(|ix| {
                self.done
                    .get(ix + 1)
                    .or_else(|| self.done.get(ix.wrapping_sub(1)))
            })
            .map(|t| t.id.clone());
        let title = self
            .done
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.title.clone())
            .unwrap_or_default();
        self.set_status(&id, "open");
        self.reload_done();
        self.done_cursor = successor;
        self.message = Some(format!("Reopened \u{201c}{title}\u{201d}."));
    }
}
