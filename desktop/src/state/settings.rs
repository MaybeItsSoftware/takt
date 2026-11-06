//! Settings and diagnostics, as overlays of rows: each setting a row whose
//! value is shown beside it, Return to change it (a switch flips, a choice
//! opens its picker, a text opens a field). `SettingsView.swift`'s panes,
//! flattened into one ranked list, and `DiagnosticsView.swift`.

use super::pickers::Picker;
use super::{App, Overlay};

/// A row of the settings overlay.
pub struct SettingRow {
    pub id: &'static str,
    pub group: &'static str,
    pub title: String,
    pub value: String,
}

fn on(flag: bool) -> String {
    if flag { "On".into() } else { "Off".into() }
}

impl App {
    pub fn settings_rows(&self) -> Vec<SettingRow> {
        let p = &self.prefs;
        let row = |id, group, title: &str, value: String| SettingRow {
            id,
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
        ];
        rows.extend(self.integration_settings());
        rows
    }

    /// Rows the integrations add: sync, Google, Checkvist, the agent, MCP,
    /// the daily log. Each is the platform module's to describe.
    fn integration_settings(&self) -> Vec<SettingRow> {
        self.platform_settings()
            .into_iter()
            .map(|(id, group, title, value)| SettingRow {
                id,
                group,
                title: title.into(),
                value,
            })
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
            "launch-at-login" => {
                p.launch_at_login = !p.launch_at_login;
                let enabled = p.launch_at_login;
                p.save();
                crate::platform::set_launch_at_login(enabled);
                return self.open_overlay(Overlay::Settings);
            }
            "focus-score" => p.scores_each_block = !p.scores_each_block,
            "focus-notify" => p.focus_notifications = !p.focus_notifications,
            "focus-sound" => p.focus_sound = !p.focus_sound,
            "focus-panel" => p.focus_panel = !p.focus_panel,
            "hotkeys" => p.hotkeys_enabled = !p.hotkeys_enabled,
            other => {
                self.change_platform_setting(other);
                return;
            }
        }
        self.prefs.save();
        self.open_overlay(Overlay::Settings);
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
        for issue in &self.library.keymap_issues {
            rows.push(("Keymap problem".into(), issue.clone()));
        }
        for issue in &self.library.theme_issues {
            rows.push(("Theme problem".into(), issue.clone()));
        }
        rows
    }
}
