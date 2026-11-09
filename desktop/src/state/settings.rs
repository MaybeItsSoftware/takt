//! Settings and diagnostics. Settings is a page over the workspace, as
//! Zed's is: its categories down the left with a search above them, and the
//! chosen one's settings on the right, each a title, a line on what it does
//! and its control (a switch, a choice, a button). Return or a click changes
//! one (a switch flips, a choice opens its picker, a text opens a field).
//! `SettingsView.swift`'s panes, and `DiagnosticsView.swift`.

use super::Editing;
use super::pickers::{Choice, Picker};
use super::plugins::{self, Plugin};
use super::{App, Overlay};
use crate::platform::hotkeys::{self, Hotkey};

/// A row of the settings overlay.
pub struct SettingRow {
    pub id: String,
    pub group: &'static str,
    pub title: String,
    pub value: String,
}

/// Where the settings page is: the category shown, the setting under
/// the keyboard, counted in what is shown (a category, or what the search
/// found), and the dropdown open on a setting, if one is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsPage {
    pub category: Option<&'static str>,
    pub cursor: usize,
    pub menu: Option<SettingsMenu>,
}

/// A setting's dropdown: which setting, and the option under the keyboard,
/// counted in what its filter leaves.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsMenu {
    pub row: String,
    pub cursor: usize,
}

/// One option of a dropdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectOption {
    /// What choosing it sets (`choose_setting_option`'s `value`).
    pub value: String,
    pub label: String,
    /// A word on the right ("Zed theme", "Primary").
    pub detail: String,
    /// The value in force, which the dropdown ticks.
    pub current: bool,
}

fn option(value: impl Into<String>, label: impl Into<String>, current: bool) -> SelectOption {
    SelectOption {
        value: value.into(),
        label: label.into(),
        detail: String::new(),
        current,
    }
}

/// The settings whose control is a dropdown of options.
const SELECTS: [&str; 7] = [
    "theme",
    "appearance",
    "quick-capture-list",
    "celebration",
    "google-calendar-choose",
    "remind-due-today-at",
    "remind-dailies-at",
];

/// Past this many options a dropdown shows its filter field; it filters
/// as you type either way.
pub const MENU_FILTER_FROM: usize = 7;

/// The categories, in the order the page lists them; a group no entry here
/// names goes at the end.
pub const CATEGORIES: [(&str, &str); 8] = [
    ("General", "settings"),
    ("Appearance", "palette"),
    ("Keyboard", "keyboard"),
    ("Focus", "timer"),
    ("Sync", "refresh"),
    ("Plugins", "plug"),
    ("AI", "sparkles"),
    ("Advanced", "wrench"),
];

/// The groups too small for a category of their own: each is a headed
/// section of the category it sits under, in this order after the
/// category's own settings. A plugin's settings need no entry: they go
/// under Plugins by the registry (`Plugin::ALL`), beneath the plugin's
/// own switch.
const SECTIONS: &[(&str, &str)] = &[("Reminders", "General"), ("Shared lists", "Sync")];

/// The category a row's group is shown under.
pub fn category_of(group: &'static str) -> &'static str {
    if group == plugins::GOOGLE_ACCOUNT || Plugin::ALL.iter().any(|p| p.name() == group) {
        return "Plugins";
    }
    SECTIONS
        .iter()
        .find(|(section, _)| *section == group)
        .map_or(group, |(_, category)| category)
}

/// A plugin's own setting, drawn beneath the plugin's switch rather than
/// as a row of the page.
pub fn is_plugin_setting(group: &'static str) -> bool {
    group != "Plugins" && category_of(group) == "Plugins"
}

/// Where a group sorts within its category: the category's own settings
/// first, then its sections in [`SECTIONS`] order.
fn section_rank(group: &str) -> usize {
    SECTIONS
        .iter()
        .position(|(section, _)| *section == group)
        .map_or(0, |at| at + 1)
}

/// The Plugins category in order: each plugin's switch, and beneath it, if
/// it is on, its settings. The Google sign-in, which every Google plugin
/// needs, goes beneath the first Google plugin that is on.
fn plugins_in_order(rows: Vec<SettingRow>) -> Vec<SettingRow> {
    let mut rest: Vec<Option<SettingRow>> = rows.into_iter().map(Some).collect();
    let mut take = |keep: &dyn Fn(&SettingRow) -> bool| -> Vec<SettingRow> {
        rest.iter_mut()
            .filter(|slot| slot.as_ref().is_some_and(keep))
            .filter_map(Option::take)
            .collect()
    };
    let switches = take(&|row| row.id.starts_with("plugin:"));
    let mut ordered = Vec::new();
    for switch in switches {
        let plugin = Plugin::from_id(&switch.id["plugin:".len()..]);
        ordered.push(switch);
        let Some(plugin) = plugin else { continue };
        if plugin.is_google() {
            ordered.extend(take(&|row| row.group == plugins::GOOGLE_ACCOUNT));
        }
        ordered.extend(take(&|row| row.group == plugin.name()));
    }
    ordered.extend(rest.into_iter().flatten());
    ordered
}

/// `options` narrowed to those with every word of `query` in their label
/// or detail, those whose label starts with it first; an empty query keeps
/// them all, in order.
pub fn filter_options(options: Vec<SelectOption>, query: &str) -> Vec<SelectOption> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return options;
    }
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut found: Vec<(bool, SelectOption)> = options
        .into_iter()
        .filter_map(|option| {
            let label = option.label.to_lowercase();
            let all = format!("{label} {}", option.detail.to_lowercase());
            words
                .iter()
                .all(|w| all.contains(w))
                .then(|| (label.starts_with(&query), option))
        })
        .collect();
    found.sort_by_key(|(starts, _)| !*starts);
    found.into_iter().map(|(_, option)| option).collect()
}

/// A day's times, every half hour, with `current` (minutes after midnight)
/// ticked, and among them if it falls between.
fn time_options(current: i32) -> Vec<SelectOption> {
    let mut minutes: Vec<i32> = (0..48).map(|half| half * 30).collect();
    if !minutes.contains(&current) {
        minutes.push(current);
        minutes.sort_unstable();
    }
    minutes
        .into_iter()
        .map(|m| {
            let clock = super::reminders::clock(m);
            option(clock.clone(), clock, m == current)
        })
        .collect()
}

/// How a setting is changed on the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    /// On or off, and what else the value says ("At 08:00").
    Switch { on: bool, note: String },
    /// Something done, and what it last said.
    Button { label: &'static str, note: String },
    /// One of several: the value, opening a dropdown of the others.
    Select(String),
    /// A text: the value, opening a field to type it.
    Text(String),
    /// A global shortcut: the keys, recorded from the next press.
    Shortcut(String),
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
        "sync-devices" | "sync-subscription" => "Refresh",
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
        "share-join" => "Join…",
        share if share.starts_with("share:") => "Invite",
        unshare if unshare.starts_with("unshare:") => "Stop",
        leave if leave.starts_with("leave:") => "Leave",
        member if member.starts_with("member:") => "Remove",
        _ => return None,
    })
}

impl SettingRow {
    pub fn control(&self) -> Control {
        if SWITCHES.contains(&self.id.as_str()) || self.id.starts_with("plugin:") {
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
        let value = self.value.clone();
        if SELECTS.contains(&self.id.as_str()) {
            Control::Select(value)
        } else if self.id.starts_with("hotkey-") {
            Control::Shortcut(value)
        } else {
            // The server address, the Google client ID.
            Control::Text(value)
        }
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
            "sync-subscription" => {
                "Hosted sync costs £1 a year, bought in the iPhone or Android app; one purchase covers every device. A server you host yourself is free."
            }
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
            "share-join" => "Open an invite someone sent you.",
            share if share.starts_with("share:") => {
                "Copy an invite link for one more person. Each works once, for seven days."
            }
            unshare if unshare.starts_with("unshare:") => {
                "It stays yours and goes from everyone else's devices."
            }
            leave if leave.starts_with("leave:") => {
                "Shared with you. Leaving takes it off your devices; the others keep it."
            }
            member if member.starts_with("member:") => "Take them out of the list.",
            plugin if plugin.starts_with("plugin:") => {
                Plugin::from_id(&plugin["plugin:".len()..]).map_or("", Plugin::description)
            }
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
            .filter(|(name, _)| rows.iter().any(|row| category_of(row.group) == *name))
            .collect();
        for row in &rows {
            let category = category_of(row.group);
            if !shown.iter().any(|(name, _)| *name == category) {
                shown.push((category, "settings"));
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
            let mut shown: Vec<SettingRow> = rows
                .into_iter()
                .filter(|r| category_of(r.group) == category)
                .collect();
            if category == "Plugins" {
                return plugins_in_order(shown);
            }
            // Stable, so each section keeps its own order.
            shown.sort_by_key(|r| section_rank(r.group));
            return shown;
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

    // MARK: Dropdowns

    /// A dropdown's options, before any filter; none for a setting that
    /// isn't one.
    pub fn setting_options(&self, id: &str) -> Option<Vec<SelectOption>> {
        let p = &self.prefs;
        Some(match id {
            "theme" => {
                let current = self.theme().id();
                self.library
                    .themes
                    .iter()
                    .map(|theme| SelectOption {
                        detail: if theme
                            .id()
                            .starts_with(takt_core::theme::zed::IDENTIFIER_PREFIX)
                        {
                            "Zed theme".into()
                        } else {
                            String::new()
                        },
                        current: theme.id() == current,
                        value: theme.id(),
                        label: theme.name(),
                    })
                    .collect()
            }
            "appearance" => [
                ("system", "As the system is"),
                ("light", "Light"),
                ("dark", "Dark"),
            ]
            .into_iter()
            .map(|(mode, label)| {
                let current = p.appearance == mode
                    || (mode == "system" && !matches!(p.appearance.as_str(), "light" | "dark"));
                option(mode, label, current)
            })
            .collect(),
            "quick-capture-list" => {
                let current = p.quick_capture_list.clone().unwrap_or_default();
                std::iter::once(option("", "Inbox", current.is_empty()))
                    .chain(self.lists_in_sidebar_order().into_iter().map(|id| {
                        let name = self.list(&id).map(|l| l.name.clone()).unwrap_or_default();
                        let here = id == current;
                        option(id, name, here)
                    }))
                    .collect()
            }
            "celebration" => {
                let current = self.celebration_style();
                super::celebration::Style::ALL
                    .into_iter()
                    .map(|style| option(style.id(), style.name(), style == current))
                    .collect()
            }
            "google-calendar-choose" => {
                let current = (self.store.core.google_calendar_settings().ok())
                    .and_then(|s| s.calendar_id)
                    .unwrap_or_default();
                std::iter::once(option("", "None", current.is_empty()))
                    .chain(
                        self.google_calendar_choices()
                            .into_iter()
                            .map(|(id, name, detail)| SelectOption {
                                current: id == current,
                                value: id,
                                label: name,
                                detail,
                            }),
                    )
                    .collect()
            }
            "remind-due-today-at" => time_options(p.remind_due_today_minute),
            "remind-dailies-at" => time_options(p.remind_dailies_left_minute),
            _ => return None,
        })
    }

    /// The open dropdown's options with `query` typed into its filter. A
    /// time typed ("7:45", "9pm") is offered first, as itself.
    pub fn setting_menu_options(&self, query: &str) -> Vec<SelectOption> {
        let Some(menu) = self.settings_page.as_ref().and_then(|p| p.menu.as_ref()) else {
            return Vec::new();
        };
        let options = self.setting_options(&menu.row).unwrap_or_default();
        let mut shown = filter_options(options, query);
        if menu.row.ends_with("-at")
            && let Some((hour, minute)) = takt_core::capture::time_of_day(query.trim())
        {
            let clock = super::reminders::clock((hour * 60 + minute) as i32);
            if shown.first().is_none_or(|first| first.value != clock) {
                shown.insert(
                    0,
                    SelectOption {
                        detail: "Typed".into(),
                        ..option(clock.clone(), clock, false)
                    },
                );
            }
        }
        shown
    }

    pub fn setting_menu(&self) -> Option<&SettingsMenu> {
        self.settings_page.as_ref().and_then(|p| p.menu.as_ref())
    }

    /// Opens `id`'s dropdown with the value in force under the keyboard;
    /// false when it has none.
    pub fn open_setting_menu(&mut self, id: &str) -> bool {
        let Some(options) = self.setting_options(id) else {
            return false;
        };
        if id == "google-calendar-choose" {
            // Fetches the calendars; the dropdown shows them as they come.
            self.change_google_setting(id);
        }
        let cursor = options.iter().position(|o| o.current).unwrap_or(0);
        let page = self.settings_page.get_or_insert_with(Default::default);
        page.menu = Some(SettingsMenu {
            row: id.into(),
            cursor,
        });
        true
    }

    pub fn close_setting_menu(&mut self) {
        if let Some(page) = self.settings_page.as_mut() {
            page.menu = None;
        }
    }

    /// ↑ and ↓ in the open dropdown, wrapping as a menu does.
    pub fn step_setting_menu(&mut self, by: isize, query: &str) {
        let len = self.setting_menu_options(query).len() as isize;
        if let Some(menu) = self.settings_page.as_mut().and_then(|p| p.menu.as_mut())
            && len > 0
        {
            menu.cursor = (menu.cursor as isize + by).rem_euclid(len) as usize;
        }
    }

    /// The filter changed: its first option is the one under the keyboard.
    pub fn setting_menu_filtered(&mut self) {
        if let Some(menu) = self.settings_page.as_mut().and_then(|p| p.menu.as_mut()) {
            menu.cursor = 0;
        }
    }

    /// Return in the open dropdown: the option under the keyboard.
    pub fn choose_setting_menu(&mut self, query: &str) {
        let Some(menu) = self.setting_menu().cloned() else {
            return;
        };
        let options = self.setting_menu_options(query);
        match options.get(menu.cursor.min(options.len().saturating_sub(1))) {
            Some(option) => {
                let value = option.value.clone();
                self.choose_setting_option(&menu.row, &value);
            }
            None => self.close_setting_menu(),
        }
    }

    /// Sets `id` to one of its options, and closes its dropdown.
    pub fn choose_setting_option(&mut self, id: &str, value: &str) {
        self.close_setting_menu();
        let some = || (!value.is_empty()).then(|| value.to_string());
        match id {
            "theme" => self.apply_choice(Picker::Theme, Choice::Theme(value.into())),
            "appearance" => self.apply_choice(Picker::Appearance, Choice::Appearance(value.into())),
            "quick-capture-list" => {
                self.apply_choice(Picker::CaptureList, Choice::CaptureList(some()))
            }
            "google-calendar-choose" => self.choose_google_calendar(some()),
            "celebration" => {
                self.prefs.celebration = Some(value.into());
                self.prefs.save();
            }
            "remind-due-today-at" => self.set_reminder_time(false, value),
            "remind-dailies-at" => self.set_reminder_time(true, value),
            _ => {}
        }
        self.open_overlay(Overlay::Settings);
    }

    /// Rows the integrations add: sync, then each plugin's switch and,
    /// for those switched on, their own settings under their name. Each is
    /// the platform module's to describe; the page only files them.
    fn integration_settings(&self) -> Vec<SettingRow> {
        let google = self.any_google_plugin_on();
        let plugin_rows = self.plugins_shown().into_iter().map(|plugin| SettingRow {
            id: format!("plugin:{}", plugin.id()),
            group: "Plugins",
            title: plugin.name().into(),
            value: on(self.plugin_on(plugin)),
        });
        self.platform_settings()
            .into_iter()
            .map(|(id, group, title, value)| SettingRow {
                id: id.into(),
                group,
                title: title.into(),
                value,
            })
            .chain(plugin_rows)
            .chain(
                self.shared_list_rows()
                    .into_iter()
                    .map(|(id, group, title, value)| SettingRow {
                        id,
                        group,
                        title,
                        value,
                    }),
            )
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
            .filter_map(|mut row| {
                if let Some(plugin) = Plugin::of_row(&row.id) {
                    if !self.plugin_on(plugin) {
                        return None;
                    }
                    row.group = plugin.name();
                } else if plugins::is_google_account_row(&row.id) {
                    if !google {
                        return None;
                    }
                    row.group = plugins::GOOGLE_ACCOUNT;
                }
                Some(row)
            })
            .collect()
    }

    /// Return on a settings row.
    pub fn change_setting(&mut self, id: &str) {
        // On the page a choice opens its dropdown; from the palette, the
        // picker or field it always has.
        if self.settings_page.is_some() && self.open_setting_menu(id) {
            return;
        }
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
            plugin if plugin.starts_with("plugin:") => {
                if let Some(plugin) = Plugin::from_id(&plugin["plugin:".len()..]) {
                    self.toggle_plugin(plugin);
                }
                return self.open_overlay(Overlay::Settings);
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, group: &'static str) -> SettingRow {
        SettingRow {
            id: id.into(),
            group,
            title: id.into(),
            value: String::new(),
        }
    }

    #[test]
    fn every_plugin_files_under_plugins_by_the_registry() {
        for plugin in Plugin::ALL {
            assert_eq!(category_of(plugin.name()), "Plugins", "{plugin:?}");
            assert!(is_plugin_setting(plugin.name()));
        }
        assert_eq!(category_of(plugins::GOOGLE_ACCOUNT), "Plugins");
        assert!(!is_plugin_setting("Plugins"));
        assert_eq!(category_of("Reminders"), "General");
        assert_eq!(category_of("Sync"), "Sync");
    }

    #[test]
    fn each_plugins_settings_sit_beneath_its_switch() {
        let rows = vec![
            row("checkvist-import", "Checkvist"),
            row("google-connect", plugins::GOOGLE_ACCOUNT),
            row("links-drive", "Google Drive"),
            row("plugin:google-calendar", "Plugins"),
            row("plugin:google-drive", "Plugins"),
            row("plugin:checkvist", "Plugins"),
            row("plugin:celebration", "Plugins"),
            row("celebration", "Celebration"),
        ];
        let ids: Vec<String> = plugins_in_order(rows).into_iter().map(|r| r.id).collect();
        assert_eq!(
            ids,
            [
                "plugin:google-calendar",
                "google-connect",
                "plugin:google-drive",
                "links-drive",
                "plugin:checkvist",
                "checkvist-import",
                "plugin:celebration",
                "celebration",
            ]
        );
    }

    #[test]
    fn controls_follow_what_the_setting_is() {
        assert!(matches!(
            row("plugin:obsidian", "Plugins").control(),
            Control::Switch { .. }
        ));
        assert!(matches!(
            row("theme", "Appearance").control(),
            Control::Select(_)
        ));
        assert!(matches!(
            row("celebration", "Celebration").control(),
            Control::Select(_)
        ));
        assert!(matches!(
            row("hotkey-capture", "Keyboard").control(),
            Control::Shortcut(_)
        ));
        assert!(matches!(
            row("sync-server", "Sync").control(),
            Control::Text(_)
        ));
        assert!(matches!(
            row("sync-now", "Sync").control(),
            Control::Button { .. }
        ));
    }

    #[test]
    fn a_filter_keeps_what_has_every_word_starts_first() {
        let options = vec![
            option("a", "One Dark", false),
            option("b", "Ayu Dark", false),
            option("c", "Dark Matter", true),
            option("d", "Solarized Light", false),
        ];
        let labels = |q: &str| -> Vec<String> {
            filter_options(options.clone(), q)
                .into_iter()
                .map(|o| o.label)
                .collect()
        };
        assert_eq!(labels("dark"), ["Dark Matter", "One Dark", "Ayu Dark"]);
        assert_eq!(labels("ayu dark"), ["Ayu Dark"]);
        assert_eq!(
            labels("  "),
            ["One Dark", "Ayu Dark", "Dark Matter", "Solarized Light"]
        );
        assert!(labels("zzz").is_empty());
    }

    #[test]
    fn a_time_between_the_half_hours_is_among_them_and_ticked() {
        let times = time_options(7 * 60 + 45);
        assert_eq!(times.len(), 49);
        let ticked: Vec<&str> = times
            .iter()
            .filter(|o| o.current)
            .map(|o| o.label.as_str())
            .collect();
        assert_eq!(ticked, ["07:45"]);
        let at = times.iter().position(|o| o.current).unwrap();
        assert_eq!(times[at - 1].label, "07:30");
        assert_eq!(times[at + 1].label, "08:00");
        assert_eq!(time_options(9 * 60).len(), 48);
    }
}
