//! What the window remembers between launches: its own file, beside the
//! database, so the Swift app's preferences are left as they are.

use super::store::app_support;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// `everything`, `today`, `list:<id>`, `folder:<id>`, `filter:<id>`.
    pub scope: String,
    pub view_mode: String,
    pub expanded_folders: HashSet<String>,
    pub folded_tasks: HashSet<String>,
    pub sidebar_visible: bool,
    pub sidebar_width: f32,
    pub right_dock_visible: bool,
    pub right_dock_width: f32,
    pub right_dock_tab: String,
    pub bottom_dock_visible: bool,
    pub bottom_dock_tab: String,
    pub hide_completed: bool,
    /// Tasks whose start is still to come, left out.
    pub hide_later: bool,
    /// A sidebar row's icon, by list, nested list or folder id.
    pub icons: HashMap<String, String>,
    /// In a folder, the list a new task lands in.
    pub folder_destinations: HashMap<String, String>,
    /// The Conditions tab's folded headings.
    pub folded_conditions: HashSet<String>,
    pub recent_lists: Vec<String>,
    pub recent_commands: Vec<String>,
    pub theme: String,
    /// `system`, `light` or `dark`.
    pub appearance: String,
    pub confirm_delete: bool,
    /// The list quick capture lands in; the inbox when none.
    pub quick_capture_list: Option<String>,
    pub launch_at_login: bool,
    pub scores_each_block: bool,
    pub focus_notifications: bool,
    pub focus_sound: bool,
    /// The floating panel while a block runs.
    pub focus_panel: bool,
    pub hotkeys_enabled: bool,
    /// The global shortcuts, spelled as the keymap spells keys; none is off.
    pub hotkey_show_window: Option<String>,
    pub hotkey_quick_capture: Option<String>,
    pub hotkey_focus_panel: Option<String>,
    /// `lists` or `agent`.
    pub left_dock_tab: String,
    /// `claude` or `codex`.
    pub agent_client: String,
    /// Where closed days are written as Obsidian notes; none, not written.
    pub daily_notes_folder: Option<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            scope: "today".into(),
            view_mode: "today".into(),
            expanded_folders: HashSet::new(),
            folded_tasks: HashSet::new(),
            sidebar_visible: true,
            sidebar_width: 240.0,
            right_dock_visible: false,
            right_dock_width: 320.0,
            right_dock_tab: "inspector".into(),
            bottom_dock_visible: false,
            bottom_dock_tab: "done".into(),
            hide_completed: false,
            hide_later: false,
            icons: HashMap::new(),
            folder_destinations: HashMap::new(),
            folded_conditions: HashSet::new(),
            recent_lists: Vec::new(),
            recent_commands: Vec::new(),
            theme: "zed".into(),
            appearance: "system".into(),
            confirm_delete: true,
            quick_capture_list: None,
            launch_at_login: false,
            scores_each_block: true,
            focus_notifications: true,
            focus_sound: true,
            focus_panel: true,
            hotkeys_enabled: true,
            hotkey_show_window: None,
            hotkey_quick_capture: Some("cmd+ctrl+option+shift+d".into()),
            hotkey_focus_panel: Some("cmd+ctrl+option+shift+f".into()),
            left_dock_tab: "lists".into(),
            agent_client: "claude".into(),
            daily_notes_folder: None,
        }
    }
}

fn path() -> PathBuf {
    std::env::var_os("TAKT_DESKTOP_PREFS")
        .map(PathBuf::from)
        .unwrap_or_else(|| app_support().join("desktop.json"))
}

impl Prefs {
    pub fn load() -> Self {
        std::fs::read(path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let path = path();
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, bytes);
        }
    }
}
