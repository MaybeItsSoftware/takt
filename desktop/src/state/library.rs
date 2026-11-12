//! What the app reads from files of the user's own beside the database:
//! `keymap.json` laid over the catalogue, and the themes folder. Each is
//! read at launch and again on its reload command, and what could not be
//! applied is kept for the diagnostics page and said in the status bar.

use super::App;
use super::store::app_support;
use crate::keys::Bindings;
use std::path::PathBuf;
use takt_core::theme::loader::CoreThemeFileSource;
use takt_core::theme::model::{CoreThemeIssueSeverity, CoreThemePlatform, CoreThemeSpecification};

pub fn keymap_path() -> PathBuf {
    app_support().join("keymap.json")
}

pub fn themes_dir() -> PathBuf {
    app_support().join("Themes")
}

/// Zed's own themes folder, where Zed's theme extensions and hand-made
/// themes live: `$XDG_CONFIG_HOME/zed/themes`, else `~/.config/zed/themes`.
/// Takt reads it, never writes it. `TAKT_ZED_THEMES` names another folder,
/// or, empty, none.
pub fn zed_themes_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("TAKT_ZED_THEMES") {
        return (!dir.is_empty()).then(|| PathBuf::from(dir));
    }
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(config.join("zed").join("themes"))
}

/// Every `.json` in `dir`, named by its file name; nothing if it is missing.
fn theme_sources(dir: &std::path::Path) -> Vec<CoreThemeFileSource> {
    std::fs::read_dir(dir)
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
        .unwrap_or_default()
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

/// Zed One's identifier: the default, the core's built-in drawn from Zed's
/// own One Dark and One Light.
pub const ONE: &str = takt_core::theme::builtins::DEFAULT_IDENTIFIER;

/// A theme the picker offers: a specification the core resolved from a
/// built-in, a synced row or a file.
#[derive(Debug, Clone)]
pub struct Theme(pub Box<CoreThemeSpecification>);

impl Theme {
    pub fn id(&self) -> String {
        self.0.identifier.clone()
    }

    pub fn name(&self) -> String {
        self.0.name.clone()
    }

    pub fn spec(&self) -> &CoreThemeSpecification {
        &self.0
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

    /// The built-in themes, the synced ones, every theme in Zed's themes
    /// folder, and every `.json` in Takt's own. A Zed theme family gives one
    /// theme per variant (the core reads Zed's format); a later source wins
    /// over an earlier one of the same identifier. Each file that loads from
    /// either folder is stored as a synced row, so the phones list it too.
    pub fn reload_themes(&mut self) {
        let built_ins = takt_core::theme::theme_builtins(CoreThemePlatform::Macos);
        let load = |sources: Vec<CoreThemeFileSource>| {
            takt_core::theme::theme_load(
                sources,
                CoreThemePlatform::Macos,
                Some(built_ins.clone()),
                None,
            )
        };
        let rows = self.store.core.themes().unwrap_or_default();
        let synced = load(
            rows.iter()
                .map(|row| CoreThemeFileSource {
                    name: format!("{}.json", row.id),
                    data: row.json.clone().into_bytes(),
                })
                .collect(),
        );
        let zed_sources = zed_themes_dir()
            .map(|dir| theme_sources(&dir))
            .unwrap_or_default();
        let own_sources = theme_sources(&themes_dir());
        let zed = load(zed_sources.clone());
        let own = load(own_sources.clone());
        self.share_theme_files(&zed, &zed_sources, &built_ins);
        self.share_theme_files(&own, &own_sources, &built_ins);

        let mut themes: Vec<Theme> = built_ins
            .iter()
            .cloned()
            .map(|s| Theme(Box::new(s)))
            .collect();
        // Zed One, the default, leads the list.
        if let Some(at) = themes.iter().position(|t| t.id() == ONE) {
            let one = themes.remove(at);
            themes.insert(0, one);
        }
        let mut issues = Vec::new();
        for (folder, outcomes) in [("synced/", synced), ("Zed themes/", zed), ("", own)] {
            for outcome in outcomes {
                // Zed's folder is Zed's to keep tidy, and a synced row was
                // checked where it was made: only what stops a theme there
                // loading is worth saying.
                let errors_only = !folder.is_empty();
                for issue in &outcome.issues {
                    if errors_only && issue.severity != CoreThemeIssueSeverity::Error {
                        continue;
                    }
                    issues.push(format!("{folder}{}: {}", outcome.source, issue.message));
                }
                match outcome.specification {
                    Some(spec) => {
                        themes.retain(|t| t.id() != spec.identifier);
                        themes.push(Theme(Box::new(spec)));
                    }
                    None if errors_only => {}
                    None => issues.push(format!(
                        "{}: skipped, {}",
                        outcome.source,
                        outcome.skipped_reason.unwrap_or_default()
                    )),
                }
            }
        }
        self.library.themes = themes;
        self.library.theme_issues = issues;
    }

    /// Stores each file that loaded as a synced row under the identifier a
    /// phone would give it on import, unless it already says the same. A
    /// file that leaves the folder keeps its row: a phone may be using it.
    fn share_theme_files(
        &mut self,
        outcomes: &[takt_core::theme::merge::CoreThemeFileOutcome],
        sources: &[CoreThemeFileSource],
        built_ins: &[CoreThemeSpecification],
    ) {
        if std::env::var_os("TAKT_DESKTOP_PREFS").is_some() {
            return;
        }
        let loaded: std::collections::HashSet<&str> = outcomes
            .iter()
            .filter(|o| o.specification.is_some())
            .map(|o| o.source.as_str())
            .collect();
        let now = super::store::now_ms();
        for source in sources.iter().filter(|s| loaded.contains(s.name.as_str())) {
            let Ok(json) = String::from_utf8(source.data.clone()) else {
                continue;
            };
            let decoded = takt_core::theme::theme_decode(source.data.clone(), source.name.clone());
            let Some(file) = decoded.file else { continue };
            let id = takt_core::theme::theme_identifier(file, source.name.clone());
            if built_ins.iter().any(|b| b.identifier == id) {
                continue;
            }
            if let Err(error) = self.store.core.upsert_theme(id, json, now) {
                self.message = Some(format!("A theme didn't sync: {error}"));
            }
        }
    }

    /// Writes the theme in force to the synced `theme.selected`, so the
    /// phones and any other Mac follow it (`docs/themes.md`).
    pub fn share_theme_choice(&mut self) {
        let id = self.theme().id();
        let now = super::store::now_ms();
        if let Err(error) = self
            .store
            .core
            .set_preference("theme.selected".into(), Some(id), now)
        {
            self.message = Some(format!("The theme choice didn't sync: {error}"));
        }
    }

    /// Once: this Mac's theme becomes the shared choice, so the phones take
    /// the look this app has.
    pub fn adopt_shared_theme(&mut self) {
        if self.prefs.theme_shared || std::env::var_os("TAKT_DESKTOP_PREFS").is_some() {
            return;
        }
        self.share_theme_choice();
        self.prefs.theme_shared = true;
        self.prefs.save();
    }

    pub fn theme(&self) -> Theme {
        self.library
            .themes
            .iter()
            .find(|t| t.id() == self.prefs.theme)
            .or_else(|| self.library.themes.iter().find(|t| t.id() == ONE))
            .cloned()
            .unwrap_or_else(|| {
                Theme(Box::new(
                    takt_core::theme::builtins::default_theme(CoreThemePlatform::Macos).clone(),
                ))
            })
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
        let spec = *self.theme().0;
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
                    self.share_theme_choice();
                }
                self.message = Some(format!("Exported to {}.", path.display()));
            }
            Err(error) => self.message = Some(error.to_string()),
        }
    }
}
