//! What the app reads from files of the user's own beside the database:
//! `keymap.json` laid over the catalogue, and the themes folder. Each is
//! read at launch and again on its reload command, and what could not be
//! applied is kept for the diagnostics page and said in the status bar.

use super::App;
use super::store::app_support;
use crate::keys::Bindings;
use std::path::PathBuf;
use takt_core::theme::loader::CoreThemeFileSource;
use takt_core::theme::model::{CoreThemePlatform, CoreThemeSpecification};

pub fn keymap_path() -> PathBuf {
    app_support().join("keymap.json")
}

pub fn themes_dir() -> PathBuf {
    app_support().join("Themes")
}

const KEYMAP_TEMPLATE: &str = r#"// Your own keys, laid over Takt's. The keyboard reference lists every
// command id. A block with a "context" binds on that surface only;
// null unbinds a key. For example:
// [
//   { "bindings": { "cmd+shift+t": "goToday" } },
//   { "context": "Outline", "bindings": { "z": null } }
// ]
[]
"#;

/// A theme the picker offers: Zed's own pair, or a specification the core
/// resolved from a built-in or a file.
#[derive(Debug, Clone)]
pub enum Theme {
    Zed,
    Spec(Box<CoreThemeSpecification>),
}

impl Theme {
    pub fn id(&self) -> String {
        match self {
            Theme::Zed => "zed".into(),
            Theme::Spec(spec) => spec.identifier.clone(),
        }
    }

    pub fn name(&self) -> String {
        match self {
            Theme::Zed => "Zed (One Dark, One Light)".into(),
            Theme::Spec(spec) => spec.name.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Library {
    pub themes: Vec<Theme>,
    pub keymap_issues: Vec<String>,
    pub theme_issues: Vec<String>,
}

impl App {
    /// `keymap.json` read and put in force.
    pub fn reload_keymap(&mut self) {
        let text = std::fs::read_to_string(keymap_path()).unwrap_or_default();
        // The file may carry `//` comments, as Zed's does.
        let text: String = text
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let (bindings, issues) = Bindings::with_user_keymap(&text);
        self.bindings = bindings;
        self.message = Some(if issues.is_empty() {
            "Keymap loaded.".into()
        } else {
            format!("Keymap: {}", issues.join("; "))
        });
        self.library.keymap_issues = issues;
    }

    /// The built-in themes and every `.json` in the themes folder.
    pub fn reload_themes(&mut self) {
        let built_ins = takt_core::theme::theme_builtins(CoreThemePlatform::Macos);
        let sources: Vec<CoreThemeFileSource> = std::fs::read_dir(themes_dir())
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                    .filter_map(|e| {
                        Some(CoreThemeFileSource {
                            name: e.file_name().to_string_lossy().into_owned(),
                            data: std::fs::read(e.path()).ok()?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let outcomes = takt_core::theme::theme_load(
            sources,
            CoreThemePlatform::Macos,
            Some(built_ins.clone()),
            built_ins.first().cloned(),
        );
        let mut themes = vec![Theme::Zed];
        themes.extend(built_ins.into_iter().map(|s| Theme::Spec(Box::new(s))));
        let mut issues = Vec::new();
        for outcome in outcomes {
            for issue in &outcome.issues {
                issues.push(format!("{}: {}", outcome.source, issue.message));
            }
            match outcome.specification {
                Some(spec) => {
                    themes.retain(|t| t.id() != spec.identifier);
                    themes.push(Theme::Spec(Box::new(spec)));
                }
                None => issues.push(format!(
                    "{}: skipped, {}",
                    outcome.source,
                    outcome.skipped_reason.unwrap_or_default()
                )),
            }
        }
        self.library.themes = themes;
        self.library.theme_issues = issues;
    }

    pub fn theme(&self) -> Theme {
        self.library
            .themes
            .iter()
            .find(|t| t.id() == self.prefs.theme)
            .cloned()
            .unwrap_or(Theme::Zed)
    }

    pub fn open_keymap_file(&mut self) {
        let path = keymap_path();
        if !path.exists() {
            let _ = std::fs::create_dir_all(app_support());
            let _ = std::fs::write(&path, KEYMAP_TEMPLATE);
        }
        super::pickers::open_address(&path.to_string_lossy());
    }

    pub fn open_themes_folder(&mut self) {
        let dir = themes_dir();
        let _ = std::fs::create_dir_all(&dir);
        super::pickers::open_address(&dir.to_string_lossy());
    }

    /// A complete, editable copy of the theme in force, in the themes
    /// folder, put in force.
    pub fn export_theme(&mut self) {
        let spec = match self.theme() {
            Theme::Spec(spec) => *spec,
            Theme::Zed => {
                self.message = Some("Zed's themes are built in; pick another to export.".into());
                return;
            }
        };
        let mut copy = spec.clone();
        let stem = format!(
            "{}-copy",
            spec.identifier.rsplit('.').next().unwrap_or("theme")
        );
        copy.identifier = format!("user.theme.{stem}");
        copy.name = format!("{} (copy)", spec.name);
        let file = takt_core::theme::theme_file_from_specification(copy, Vec::new());
        let Some(text) = takt_core::theme::theme_file_encode(file) else {
            self.message = Some("That theme could not be written.".into());
            return;
        };
        let dir = themes_dir();
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{stem}.json"));
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.reload_themes();
                if let Some(theme) = self
                    .library
                    .themes
                    .iter()
                    .find(|t| t.name().ends_with("(copy)") && t.id().ends_with(&stem))
                {
                    self.prefs.theme = theme.id();
                    self.prefs.save();
                }
                self.message = Some(format!("Exported to {}.", path.display()));
            }
            Err(error) => self.message = Some(error.to_string()),
        }
    }
}
