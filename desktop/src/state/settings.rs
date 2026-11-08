//! Settings and diagnostics. Settings is a page over the workspace, as
//! Zed's is: its categories down the left with a search above them, and the
//! chosen one's settings on the right, each a title, a line on what it does
//! and its control (a switch, a choice, a button). Return or a click changes
//! one (a switch flips, a choice opens its picker, a text opens a field).
//! `SettingsView.swift`'s panes, and `DiagnosticsView.swift`.

use super::Editing;
use super::pickers::Picker;
use super::{App, Overlay};
use crate::platform::hotkeys::{self, Hotkey};

/// A row of the settings overlay.
pub struct SettingRow {
    pub id: String,
    pub group: &'static str,
    pub title: String,
    pub value: String,
}

/// Where the settings page is: the category shown and the setting under
/// the keyboard, counted in what is shown (a category, or what the search
/// found).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsPage {
    pub category: Option<&'static str>,
    pub cursor: usize,
}

/// The categories, in the order the page lists them; a group no entry here
/// names goes at the end.
pub const CATEGORIES: [(&str, &str); 13] = [
    ("General", "settings"),
    ("Appearance", "palette"),
    ("Keyboard", "keyboard"),
    ("Focus", "timer"),
    ("Reminders", "bell"),
    ("Completion", "party"),
    ("Sync", "refresh"),
    ("Google", "calendar"),
    ("Checkvist", "square-check"),
    ("Links", "link"),
    ("AI", "sparkles"),
    ("MCP", "plug"),
    ("Advanced", "wrench"),
];

/// How a setting is changed on the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    /// On or off, and what else the value says ("At 08:00").
    Switch { on: bool, note: String },
    /// Something done, and what it last said.
    Button { label: &'static str, note: String },
    /// One of several, or a text: the value, opening its picker or field.
    Choice(String),
}

const SWITCHES: [&str; 17] = [
    "ai-off",
    "key-hints",
    "confirm-delete",
    "launch-at-login",
    "focus-score",
    "focus-notify",
    "focus-sound",
    "focus-panel",
    "hotkeys",
    "remind-due-time",
    "remind-due-today",
    "remind-dailies",
    "links-drive",
    "celebration-sound",
    "google-calendar",
    "google-import-events",
    "google-tasks",
];

fn button_label(id: &str) -> Option<&'static str> {
    Some(match id {
        "themes-folder" => "Open folder",
        "themes-reload" | "keymap-reload" => "Reload",
        "theme-export" => "Export",
        "keymap-open" => "Edit keymap.json",
        "diagnostics" => "Open",
        "sync-now" | "google-sync-now" => "Sync now",
        "sync-devices" => "Refresh",
        "sync-new-password" => "Change",
        "sync-sign-out" => "Sign out",
        "sync-delete" => "Delete",
        "sync-google" | "sync-apple" | "sync-email" => "Sign in",
        "sync-create" => "Create",
        "sync-reset" => "Reset",
        "checkvist-import" => "Import",
        "checkvist-connect" | "google-connect" => "Connect",
        "checkvist-disconnect" | "google-disconnect" => "Disconnect",
        "links-vault-add" => "Add…",
        "links-vault-forget" => "Forget",
        mcp if mcp.starts_with("mcp:") => "Add",
        _ => return None,
    })
}

impl SettingRow {
    pub fn control(&self) -> Control {
        if SWITCHES.contains(&self.id.as_str()) {
            let value = self.value.as_str();
            let on = value.starts_with("On") || value.starts_with("At ");
            let note = match value.split_once(" · ") {
                Some((_, rest)) => rest.to_string(),
                None if value.starts_with("At ") => value.to_string(),
                None => String::new(),
            };
            return Control::Switch { on, note };
        }
        if let Some(label) = button_label(&self.id) {
            let note = match self.id.as_str() {
                "links-vault-forget" => format!("{} added here", self.value),
                _ => self.value.clone(),
            };
            return Control::Button { label, note };
        }
        Control::Choice(self.value.clone())
    }

    /// The line under the title.
    pub fn description(&self) -> &'static str {
        match self.id.as_str() {
            "theme" => "The colours the window is drawn in.",
            "appearance" => "Follow the system, or keep to light or dark.",
            "themes-folder" => "Theme files of your own, read at launch and on reload.",
            "themes-reload" => "Read the themes folder again without relaunching.",
            "theme-export" => {
                "Write the current theme out as a JSON file to start one of your own."
            }
            "confirm-delete" => {
                "Ask before a task is deleted; off deletes at once, and undo still brings it back."
            }
            "quick-capture-list" => "The list a task typed into quick capture is filed in.",
            "launch-at-login" => "Start Takt when you log in, in the menu bar.",
            "focus-score" => "When a focus block ends, ask how it went.",
            "focus-notify" => "A notification as a focus block or break ends.",
            "focus-sound" => "A sound with that notification.",
            "focus-panel" => "Float a small panel with the running block over other windows.",
            "hotkeys" => "Shortcuts that reach Takt from any app.",
            "hotkey-capture" => "Open quick capture from anywhere.",
            "hotkey-panel" => "Show or hide the focus panel from anywhere.",
            "hotkey-window" => "Bring the main window forward from anywhere.",
            "keymap-open" => "Your own bindings, laid over the built-in ones.",
            "keymap-reload" => "Read keymap.json again without relaunching.",
            "diagnostics" => "What the app is reading, and anything that went wrong.",
            "ai-off" => {
                "Hide the agent panel, quick ask, the agent's context on tasks and the MCP setup. Nothing is sent to an AI while it is on."
            }
            "key-hints" => "A small marking with the key beside buttons and rows that have one.",
            "remind-due-time" => "A notification when a task's due time comes.",
            "remind-due-today" => "Each morning, a notification listing what is due that day.",
            "remind-due-today-at" => "When the morning reminder comes.",
            "remind-dailies" => "Each evening, a notification if dailies are still left.",
            "remind-dailies-at" => "When the evening reminder comes.",
            "celebration" => "What a row does as its task is finished.",
            "celebration-sound" => "A quiet system sound with it.",
            "sync-now" => "Send and fetch changes now rather than waiting.",
            "sync-devices" => "The devices signed in to this account.",
            "sync-new-password" => "The password that unlocks the workspace on a new device.",
            "sync-sign-out" => "Stop syncing this Mac; its workspace stays here.",
            "sync-delete" => "Remove the account and everything the server holds for it.",
            "sync-google" => "Sync this workspace with your phone and other Macs.",
            "sync-apple" => "Sync with your Apple ID.",
            "sync-email" => "Sign in to an account made with an email address.",
            "sync-create" => "Make an account with an email address.",
            "sync-reset" => "Send a link to choose a new password.",
            "sync-server" => "The server sync talks to.",
            "checkvist-connect" => "Copy your Checkvist lists into the workspace.",
            "checkvist-import" => "Copy in the Checkvist lists the workspace lacks.",
            "checkvist-disconnect" => "Forget the Checkvist account; imported lists stay.",
            "google-client" => "The OAuth client Takt signs in to Google with.",
            "google-connect" => "Sign in to Google for Calendar, Tasks and Drive.",
            "google-disconnect" => "Sign out of Google on this Mac.",
            "google-calendar" => "Tasks with a time appear in your calendar.",
            "google-calendar-choose" => "The calendar that changes come back from.",
            "google-import-events" => "Events added in the calendar become tasks.",
            "google-tasks" => "Each list mirrored as a Google Tasks list.",
            "google-sync-now" => "Pass the calendar and tasks mirrors now.",
            "links-vault-add" => "Notes in Obsidian vaults are offered when you link a task.",
            "links-vault-forget" => "Drop the vault folders added by hand; Obsidian's own stay.",
            "links-drive" => "Offer Google Drive files when you link a task.",
            mcp if mcp.starts_with("mcp:") => "Let this assistant read and change your tasks.",
            _ => "",
        }
    }
}

fn shortcut(spelled: &Option<String>) -> String {
    spelled
        .as_deref()
        .map(crate::platform::hotkeys::display)
        .unwrap_or_else(|| "Off".into())
}

fn on(flag: bool) -> String {
    if flag { "On".into() } else { "Off".into() }
}

impl App {
    pub fn settings_rows(&self) -> Vec<SettingRow> {
        let p = &self.prefs;
        let row = |id: &str, group, title: &str, value: String| SettingRow {
            id: id.into(),
            group,
            title: title.into(),
            value,
        };
        let capture = p
            .quick_capture_list
            .as_deref()
            .and_then(|id| self.list(id))
            .map(|l| l.name.clone())
            .unwrap_or_else(|| "Inbox".into());
        let mut rows = vec![
            row("theme", "Appearance", "Theme", self.theme().name()),
            row(
                "appearance",
                "Appearance",
                "Light or dark",
                match p.appearance.as_str() {
                    "light" => "Light".into(),
                    "dark" => "Dark".into(),
                    _ => "As the system is".into(),
                },
            ),
            row(
                "themes-folder",
                "Appearance",
                "Open the themes folder",
                String::new(),
            ),
            row(
                "themes-reload",
                "Appearance",
                "Reload themes",
                format!("{}", self.library.themes.len()),
            ),
            row(
                "theme-export",
                "Appearance",
                "Export the theme as JSON",
                String::new(),
            ),
            row(
                "confirm-delete",
                "General",
                "Ask before deleting a task",
                on(p.confirm_delete),
            ),
            row(
                "quick-capture-list",
                "General",
                "Quick capture lands in",
                capture,
            ),
            row(
                "launch-at-login",
                "General",
                "Open at login",
                on(p.launch_at_login),
            ),
            row(
                "focus-score",
                "Focus",
                "Score each focus block",
                on(p.scores_each_block),
            ),
            row(
                "focus-notify",
                "Focus",
                "Notify when a block ends",
                on(p.focus_notifications),
            ),
            row(
                "focus-sound",
                "Focus",
                "Play a sound with it",
                on(p.focus_sound),
            ),
            row(
                "focus-panel",
                "Focus",
                "Show the floating focus panel",
                on(p.focus_panel),
            ),
            row(
                "hotkeys",
                "Keyboard",
                "Global hotkeys",
                on(p.hotkeys_enabled),
            ),
            row(
                "hotkey-capture",
                "Keyboard",
                "Quick capture from any app",
                shortcut(&p.hotkey_quick_capture),
            ),
            row(
                "hotkey-panel",
                "Keyboard",
                "The focus panel from any app",
                shortcut(&p.hotkey_focus_panel),
            ),
            row(
                "hotkey-window",
                "Keyboard",
                "The main window from any app",
                shortcut(&p.hotkey_show_window),
            ),
            row("keymap-open", "Keyboard", "Open keymap.json", String::new()),
            row(
                "keymap-reload",
                "Keyboard",
                "Reload the keymap",
                if self.library.keymap_issues.is_empty() {
                    String::new()
                } else {
                    format!("{} problems", self.library.keymap_issues.len())
                },
            ),
            row("diagnostics", "Advanced", "Diagnostics", String::new()),
            row("ai-off", "AI", "Switch off all AI", on(p.ai_off)),
            row(
                "key-hints",
                "Keyboard",
                "Show key hints",
                on(!p.hide_key_hints),
            ),
        ];
        rows.extend(self.integration_settings());
        rows
    }

    /// The categories that have settings, in the page's order, with their
    /// icons.
    pub fn settings_categories(&self) -> Vec<(&'static str, &'static str)> {
        let rows = self.settings_rows();
        let mut shown: Vec<(&'static str, &'static str)> = CATEGORIES
            .into_iter()
            .filter(|(name, _)| rows.iter().any(|row| row.group == *name))
            .collect();
        for row in &rows {
            if !shown.iter().any(|(name, _)| *name == row.group) {
                shown.push((row.group, "settings"));
            }
        }
        shown
    }

    /// The category the page shows: the one chosen, or the first.
    pub fn settings_category(&self) -> &'static str {
        let categories = self.settings_categories();
        self.settings_page
            .as_ref()
            .and_then(|page| page.category)
            .filter(|name| categories.iter().any(|(c, _)| c == name))
            .or_else(|| categories.first().map(|(c, _)| *c))
            .unwrap_or("General")
    }

    /// What the page shows: the category's settings, or with a search
    /// typed, the settings it finds in every category, best first.
    pub fn settings_shown(&self, query: &str) -> Vec<SettingRow> {
        let rows = self.settings_rows();
        if query.trim().is_empty() {
            let category = self.settings_category();
            return rows.into_iter().filter(|r| r.group == category).collect();
        }
        // Every word somewhere in the setting; those whose title has them
        // all come first. Fuzzy matching across the descriptions finds
        // nearly everything.
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let mut found: Vec<(bool, SettingRow)> = rows
            .into_iter()
            .filter_map(|row| {
                let title = row.title.to_lowercase();
                let all = format!("{title} {} {}", row.group, row.description()).to_lowercase();
                words
                    .iter()
                    .all(|w| all.contains(w.as_str()))
                    .then(|| (words.iter().all(|w| title.contains(w.as_str())), row))
            })
            .collect();
        found.sort_by_key(|(in_title, _)| !*in_title);
        found.into_iter().map(|(_, row)| row).collect()
    }

    pub fn close_settings_page(&mut self) {
        self.settings_page = None;
    }

    /// Shows `category`, from its top.
    pub fn choose_settings_category(&mut self, category: &'static str) {
        let page = self.settings_page.get_or_insert_with(Default::default);
        page.category = Some(category);
        page.cursor = 0;
    }

    /// Tab and ⇧Tab: the next or previous category.
    pub fn step_settings_category(&mut self, by: isize) {
        let categories = self.settings_categories();
        let now = self.settings_category();
        let at = categories.iter().position(|(c, _)| *c == now).unwrap_or(0) as isize;
        let len = categories.len() as isize;
        if len == 0 {
            return;
        }
        let next = categories[((at + by).rem_euclid(len)) as usize].0;
        self.choose_settings_category(next);
    }

    /// ↑ and ↓ through what is shown.
    pub fn step_settings_cursor(&mut self, by: isize, query: &str) {
        let len = self.settings_shown(query).len();
        if let Some(page) = self.settings_page.as_mut()
            && len > 0
        {
            page.cursor = (page.cursor as isize + by).clamp(0, len as isize - 1) as usize;
        }
    }

    /// Return on the page: changes the setting under the keyboard.
    pub fn change_settings_cursor(&mut self, query: &str) {
        let cursor = self.settings_page.as_ref().map_or(0, |p| p.cursor);
        if let Some(row) = self.settings_shown(query).get(cursor) {
            let id = row.id.clone();
            self.change_setting(&id);
        }
    }

    /// Rows the integrations add: sync, Google, Checkvist, the agent, MCP,
    /// the daily log. Each is the platform module's to describe.
    fn integration_settings(&self) -> Vec<SettingRow> {
        self.platform_settings()
            .into_iter()
            .map(|(id, group, title, value)| SettingRow {
                id: id.into(),
                group,
                title: title.into(),
                value,
            })
            .chain(
                self.mcp_client_rows()
                    .into_iter()
                    .filter(|_| !self.prefs.ai_off)
                    .map(|(id, title, value)| SettingRow {
                        id,
                        group: "MCP",
                        title,
                        value,
                    }),
            )
            .collect()
    }

    /// Return on a settings row.
    pub fn change_setting(&mut self, id: &str) {
        let p = &mut self.prefs;
        match id {
            "theme" => return self.open_picker(Picker::Theme),
            "appearance" => return self.open_picker(Picker::Appearance),
            "themes-folder" => return self.open_themes_folder(),
            "themes-reload" => return self.run("windowReloadThemes", None),
            "theme-export" => return self.export_theme(),
            "keymap-open" => return self.open_keymap_file(),
            "keymap-reload" => return self.reload_keymap(),
            "diagnostics" => return self.open_overlay(Overlay::Diagnostics),
            "quick-capture-list" => return self.open_picker(Picker::CaptureList),
            "confirm-delete" => p.confirm_delete = !p.confirm_delete,
            "key-hints" => p.hide_key_hints = !p.hide_key_hints,
            "ai-off" => {
                let off = !p.ai_off;
                self.set_ai_off(off);
            }
            "launch-at-login" => {
                let enabled = !p.launch_at_login;
                match crate::platform::set_launch_at_login(enabled) {
                    Ok(()) => {
                        self.prefs.launch_at_login = enabled;
                        self.prefs.save();
                    }
                    Err(error) => self.message = Some(error),
                }
                return self.open_overlay(Overlay::Settings);
            }
            "focus-score" => p.scores_each_block = !p.scores_each_block,
            "focus-notify" => p.focus_notifications = !p.focus_notifications,
            "focus-sound" => p.focus_sound = !p.focus_sound,
            "focus-panel" => p.focus_panel = !p.focus_panel,
            "hotkeys" => {
                p.hotkeys_enabled = !p.hotkeys_enabled;
                self.hotkeys_changed = true;
            }
            "hotkey-capture" => return self.record_hotkey(Hotkey::QuickCapture),
            "hotkey-panel" => return self.record_hotkey(Hotkey::FocusPanel),
            "hotkey-window" => return self.record_hotkey(Hotkey::ShowWindow),
            other => {
                self.change_platform_setting(other);
                return;
            }
        }
        self.prefs.save();
        self.open_overlay(Overlay::Settings);
    }

    /// Waits for the next key pressed to be the shortcut.
    fn record_hotkey(&mut self, which: Hotkey) {
        self.overlay = None;
        self.editing = Some(Editing::Hotkey(which));
        self.hotkeys_changed = true;
        self.message =
            Some("Press the new shortcut. Delete turns it off, Esc leaves it as it was.".into());
    }

    /// A key pressed while a shortcut is being recorded. `spelled` is the
    /// raw press (`hotkeys::spell`); true once the recording is over.
    pub fn record_hotkey_key(&mut self, spelled: &str) -> bool {
        let Some(Editing::Hotkey(which)) = self.editing.clone() else {
            return false;
        };
        let value = match spelled {
            "escape" => {
                self.editing = None;
                self.hotkeys_changed = true;
                self.message = None;
                self.open_overlay(Overlay::Settings);
                return true;
            }
            "delete" => None,
            _ if hotkeys::parse(spelled).is_none() => {
                // A modifier on its own is the start of a chord, not a miss.
                if !["cmd", "ctrl", "option", "shift", "alt", "fn", "capslock"]
                    .iter()
                    .any(|m| spelled.ends_with(m))
                {
                    self.message =
                        Some("A shortcut needs ⌘, ⌃ or ⌥ with a key. Try again, or Esc.".into());
                }
                return false;
            }
            _ => Some(spelled.to_string()),
        };
        let p = &mut self.prefs;
        match which {
            Hotkey::QuickCapture => p.hotkey_quick_capture = value,
            Hotkey::FocusPanel => p.hotkey_focus_panel = value,
            Hotkey::ShowWindow => p.hotkey_show_window = value,
        }
        p.save();
        self.hotkeys_changed = true;
        self.editing = None;
        self.message = None;
        self.open_overlay(Overlay::Settings);
        true
    }

    /// The shortcuts to register: none while they are switched off.
    pub fn wanted_hotkeys(&self) -> Vec<(Hotkey, Option<String>)> {
        let p = &self.prefs;
        let on = |s: &Option<String>| s.clone().filter(|_| p.hotkeys_enabled);
        vec![
            (Hotkey::ShowWindow, on(&p.hotkey_show_window)),
            (Hotkey::QuickCapture, on(&p.hotkey_quick_capture)),
            (Hotkey::FocusPanel, on(&p.hotkey_focus_panel)),
        ]
    }

    /// The diagnostics page: what the app is reading and what went wrong.
    pub fn diagnostics_rows(&self) -> Vec<(String, String)> {
        let core = &self.store.core;
        let mut rows = vec![
            ("Database".into(), self.store.path.clone()),
            ("Workspace".into(), self.store.workspace_id.clone()),
            ("Time zone".into(), self.store.zone.clone()),
            (
                "Data version".into(),
                core.data_version()
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
            ),
            ("Lists".into(), self.lists.len().to_string()),
            ("Rows in this scope".into(), self.all_rows.len().to_string()),
            ("Themes".into(), self.library.themes.len().to_string()),
            (
                "Keymap".into(),
                if super::library::keymap_path().exists() {
                    super::library::keymap_path().display().to_string()
                } else {
                    "none".into()
                },
            ),
            ("Version".into(), env!("CARGO_PKG_VERSION").into()),
        ];
        rows.extend(self.platform_diagnostics());
        for (hotkey, spelled) in self.wanted_hotkeys() {
            if let Some(spelled) = spelled {
                rows.push((format!("Hotkey: {hotkey:?}"), hotkeys::display(&spelled)));
            }
        }
        for problem in &self.hotkey_problems {
            rows.push(("Hotkey problem".into(), problem.clone()));
        }
        for issue in &self.library.keymap_issues {
            rows.push(("Keymap problem".into(), issue.clone()));
        }
        for issue in &self.library.theme_issues {
            rows.push(("Theme problem".into(), issue.clone()));
        }
        rows
    }
}
