//! The built-in plugins: each integration as something switched on or off
//! in Settings, each a page under the sidebar's Plugins heading (MCP sits
//! in AI and the celebration in Appearance). A plugin is an id, a name, a line on what it does
//! and its settings rows; switched off, its rows leave the page and what it
//! does in the background or offers in the link picker stops.
//!
//! The integrations themselves stay where they were (`google`, `links`,
//! `checkvist`, `mcp_clients`, `celebration`): the switch is read where
//! their work is started or offered, not inside them. Which plugins are on
//! is `Prefs::plugins`; one the file doesn't name yet is seeded at launch
//! from whether it is already set up, so nothing in use goes on upgrade.

use super::App;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plugin {
    GoogleCalendar,
    GoogleTasks,
    GoogleDrive,
    Checkvist,
    Obsidian,
    MarkdownEditor,
    Mcp,
    Celebration,
}

/// The Google page: the sign-in every Google plugin shares, and the
/// plugins themselves, which sit under it in the sidebar.
pub const GOOGLE_ACCOUNT: &str = "Google";

/// Google's own switch in `Prefs::plugins`, beside the plugins' ids, and
/// its row's id is `plugin:` and this.
pub const GOOGLE_SWITCH: &str = "google";

impl Plugin {
    /// In the order the page lists them.
    pub const ALL: [Plugin; 8] = [
        Plugin::GoogleCalendar,
        Plugin::GoogleTasks,
        Plugin::GoogleDrive,
        Plugin::Checkvist,
        Plugin::Obsidian,
        Plugin::MarkdownEditor,
        Plugin::Mcp,
        Plugin::Celebration,
    ];

    /// The key in the prefs file; permanent.
    pub fn id(self) -> &'static str {
        match self {
            Plugin::GoogleCalendar => "google-calendar",
            Plugin::GoogleTasks => "google-tasks",
            Plugin::GoogleDrive => "google-drive",
            Plugin::Checkvist => "checkvist",
            Plugin::Obsidian => "obsidian",
            Plugin::MarkdownEditor => "markdown-editor",
            Plugin::Mcp => "mcp",
            Plugin::Celebration => "celebration",
        }
    }

    /// Its name, which is also the heading its settings sit under.
    pub fn name(self) -> &'static str {
        match self {
            Plugin::GoogleCalendar => "Google Calendar",
            Plugin::GoogleTasks => "Google Tasks",
            Plugin::GoogleDrive => "Google Drive",
            Plugin::Checkvist => "Checkvist",
            Plugin::Obsidian => "Obsidian",
            Plugin::MarkdownEditor => "Markdown editor",
            Plugin::Mcp => "MCP",
            Plugin::Celebration => "Celebration",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Plugin::GoogleCalendar => {
                "Timed tasks mirrored to a calendar, and its events brought back as tasks."
            }
            Plugin::GoogleTasks => "Each list mirrored as a Google Tasks list.",
            Plugin::GoogleDrive => "Link Drive files you choose in Google's file picker.",
            Plugin::Checkvist => "Import your Checkvist lists into the workspace.",
            Plugin::Obsidian => "Notes and folders in your Obsidian vaults, offered as links.",
            Plugin::MarkdownEditor => {
                "A task's notes maximised (nm), drawn as Markdown; or its linked Obsidian note, edited in place."
            }
            Plugin::Mcp => "Add Takt's MCP server to the assistants on this Mac.",
            Plugin::Celebration => "A row's flourish and a quiet sound as a task is finished.",
        }
    }

    /// The settings category a plugin lives in when it isn't a page of its
    /// own under the sidebar's Plugins heading: MCP with the rest of AI,
    /// the celebration with how the app looks.
    pub fn home(self) -> Option<&'static str> {
        match self {
            Plugin::Mcp => Some("AI"),
            Plugin::Celebration => Some("Appearance"),
            _ => None,
        }
    }

    pub fn from_id(id: &str) -> Option<Plugin> {
        Plugin::ALL.into_iter().find(|p| p.id() == id)
    }

    pub fn is_google(self) -> bool {
        matches!(
            self,
            Plugin::GoogleCalendar | Plugin::GoogleTasks | Plugin::GoogleDrive
        )
    }

    /// The plugin a settings row belongs to, by the row's id.
    pub fn of_row(id: &str) -> Option<Plugin> {
        Some(match id {
            "google-calendar" | "google-calendar-choose" | "google-import-events" => {
                Plugin::GoogleCalendar
            }
            "google-tasks" => Plugin::GoogleTasks,
            "links-drive" => Plugin::GoogleDrive,
            "links-vault-add" | "links-vault-forget" => Plugin::Obsidian,
            "celebration" | "celebration-sound" => Plugin::Celebration,
            checkvist if checkvist.starts_with("checkvist-") => Plugin::Checkvist,
            mcp if mcp.starts_with("mcp:") => Plugin::Mcp,
            _ => return None,
        })
    }
}

/// The Google sign-in rows, which every Google plugin needs.
pub fn is_google_account_row(id: &str) -> bool {
    matches!(
        id,
        "google-own-client"
            | "google-client"
            | "google-secret"
            | "google-connect"
            | "google-disconnect"
            | "google-sync-now"
    )
}

impl App {
    /// On by its own switch and, for a Google plugin, Google's too.
    pub fn plugin_on(&self, plugin: Plugin) -> bool {
        self.plugin_switched_on(plugin) && (!plugin.is_google() || self.google_on())
    }

    /// Its own switch, whatever Google's says.
    fn plugin_switched_on(&self, plugin: Plugin) -> bool {
        self.prefs.plugins.get(plugin.id()).copied().unwrap_or(true)
    }

    /// Google's switch, over its plugins: off, they all stop and their own
    /// switches are kept for when it is on again.
    pub fn google_on(&self) -> bool {
        self.prefs
            .plugins
            .get(GOOGLE_SWITCH)
            .copied()
            .unwrap_or(true)
    }

    pub fn toggle_google(&mut self) {
        let on = !self.google_on();
        self.prefs.plugins.insert(GOOGLE_SWITCH.into(), on);
        // On with nothing under it would do nothing: all of them, then.
        if on
            && !Plugin::ALL
                .into_iter()
                .any(|p| p.is_google() && self.plugin_switched_on(p))
        {
            for plugin in Plugin::ALL.into_iter().filter(|p| p.is_google()) {
                self.prefs.plugins.insert(plugin.id().into(), true);
            }
        }
        self.prefs.save();
        self.google_now();
    }

    pub fn any_google_plugin_on(&self) -> bool {
        Plugin::ALL
            .into_iter()
            .any(|p| p.is_google() && self.plugin_on(p))
    }

    /// The plugins shown on the page: MCP goes with the rest of AI.
    pub fn plugins_shown(&self) -> Vec<Plugin> {
        Plugin::ALL
            .into_iter()
            .filter(|p| *p != Plugin::Mcp || !self.prefs.ai_off)
            .filter(|p| !p.is_google() || self.google_on())
            .collect()
    }

    /// At launch, after the Swift app's settings are adopted: each plugin
    /// the prefs don't name yet is on if it is already in use.
    pub fn seed_plugins(&mut self) {
        let missing: Vec<Plugin> = Plugin::ALL
            .into_iter()
            .filter(|p| !self.prefs.plugins.contains_key(p.id()))
            .collect();
        if missing.is_empty() {
            return;
        }
        for plugin in missing {
            let on = self.plugin_in_use(plugin);
            self.prefs.plugins.insert(plugin.id().into(), on);
        }
        self.prefs.save();
    }

    fn plugin_in_use(&self, plugin: Plugin) -> bool {
        let p = &self.prefs;
        match plugin {
            Plugin::GoogleCalendar => {
                let settings = self.store.core.google_calendar_settings().ok();
                p.google_calendar
                    || settings.is_some_and(|s| s.calendar_id.is_some() || s.import_events)
                    // Signed in to Google with nothing switched on: keep
                    // the sign-in on the page.
                    || (crate::google::connected() && !p.google_tasks && !p.links_drive_search)
            }
            Plugin::GoogleTasks => p.google_tasks,
            Plugin::GoogleDrive => p.links_drive_search,
            Plugin::Checkvist => super::checkvist::credentials().is_some(),
            Plugin::Obsidian => {
                !p.links_obsidian_vaults.is_empty()
                    || !super::links::configured_vaults(&obsidian_config()).is_empty()
            }
            // Its setup rows were on the page for everyone with AI on, and
            // an assistant already added may be one this Mac can't see.
            // Nothing to set up, and nothing changes until it is asked for.
            Plugin::MarkdownEditor => true,
            Plugin::Mcp => !p.ai_off,
            Plugin::Celebration => self.celebration_style() != super::celebration::Style::None,
        }
    }

    /// Return on a plugin's switch.
    pub fn toggle_plugin(&mut self, plugin: Plugin) {
        let on = !self.plugin_switched_on(plugin);
        self.prefs.plugins.insert(plugin.id().into(), on);
        self.prefs.save();
        match plugin {
            // The worker reads the mirrors' switches on its next pass.
            Plugin::GoogleCalendar | Plugin::GoogleTasks => self.google_now(),
            Plugin::Obsidian if !on => {
                self.links.vaults.clear();
                self.links.entries.clear();
            }
            Plugin::MarkdownEditor if !on => {
                if matches!(self.editing, Some(super::Editing::NotesEditor)) {
                    self.editing = None;
                }
                self.notes_editor = None;
            }
            Plugin::Celebration if !on => {
                self.celebrating.clear();
                self.celebration = None;
            }
            _ => {}
        }
    }

    /// The link picker's vault index, or none with Obsidian off.
    pub fn index_link_sources(&mut self) {
        if self.plugin_on(Plugin::Obsidian) {
            self.index_obsidian();
        } else {
            self.links.vaults.clear();
            self.links.entries.clear();
        }
    }
}

/// Obsidian's list of vaults (`links::obsidian_config`, which is private
/// to the picker).
fn obsidian_config() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
        .join("Library/Application Support/obsidian/obsidian.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_plugin_has_its_own_id_and_comes_back_from_it() {
        for plugin in Plugin::ALL {
            assert_eq!(Plugin::from_id(plugin.id()), Some(plugin));
            assert!(!plugin.description().is_empty());
        }
        let mut ids: Vec<_> = Plugin::ALL.iter().map(|p| p.id()).collect();
        ids.push(GOOGLE_SWITCH);
        ids.sort();
        ids.dedup();
        // Google's switch shares the prefs map, so no plugin may take its id.
        assert_eq!(ids.len(), Plugin::ALL.len() + 1);
    }

    #[test]
    fn rows_belong_to_their_plugin() {
        assert_eq!(
            Plugin::of_row("google-import-events"),
            Some(Plugin::GoogleCalendar)
        );
        assert_eq!(Plugin::of_row("links-drive"), Some(Plugin::GoogleDrive));
        assert_eq!(Plugin::of_row("checkvist-import"), Some(Plugin::Checkvist));
        assert_eq!(Plugin::of_row("mcp:claude-code"), Some(Plugin::Mcp));
        assert_eq!(Plugin::of_row("google-connect"), None);
        assert!(is_google_account_row("google-connect"));
        assert!(is_google_account_row("google-own-client"));
        assert_eq!(Plugin::of_row("sync-now"), None);
    }
}
