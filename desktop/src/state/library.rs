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

/// Zed One's identifier, the core's built-in of the same pair: what this
/// app's own Zed theme is stored and synced as.
pub const ONE: &str = "native.theme.one";

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
            Theme::Zed => ONE.into(),
            Theme::Spec(spec) => spec.identifier.clone(),
        }
    }

    pub fn name(&self) -> String {
        match self {
            Theme::Zed => "Zed One (One Dark, One Light)".into(),
            Theme::Spec(spec) => spec.name.clone(),
        }
    }
}

/// What a Zed theme says about Zed's own chrome that Takt's theme format
/// has no role for: kept here, by the identifier its variant loaded as, so
/// the Mac draws it as Zed does.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ZedChrome {
    /// `status_bar.background`.
    pub status_bar: Option<takt_core::theme::model::CoreThemeColor>,
    /// `panel.indent_guide`.
    pub indent_guide: Option<takt_core::theme::model::CoreThemeColor>,
}

/// Each variant in Zed's themes folder by its name, with the chrome colours
/// it states.
fn zed_chrome_by_name(dir: &std::path::Path) -> std::collections::HashMap<String, ZedChrome> {
    let mut found = std::collections::HashMap::new();
    for source in theme_sources(dir) {
        let Ok(root) = serde_json::from_slice::<serde_json::Value>(&source.data) else {
            continue;
        };
        let Some(variants) = root.get("themes").and_then(|t| t.as_array()) else {
            continue;
        };
        for variant in variants {
            let Some(name) = variant.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            let style = variant.get("style");
            let read = |key: &str| {
                style
                    .and_then(|s| s.get(key))
                    .and_then(|v| v.as_str())
                    .and_then(takt_core::theme::color::from_hex)
            };
            found.insert(
                name.to_string(),
                ZedChrome {
                    status_bar: read("status_bar.background"),
                    indent_guide: read("panel.indent_guide"),
                },
            );
        }
    }
    found
}

#[derive(Debug, Clone, Default)]
pub struct Library {
    pub themes: Vec<Theme>,
    /// Zed's chrome colours for the themes read from Zed's folder.
    pub zed_chrome: std::collections::HashMap<String, ZedChrome>,
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

    /// The built-in themes, every theme in Zed's themes folder, and every
    /// `.json` in Takt's own. A Zed theme family gives one theme per
    /// variant (the core reads Zed's format); a theme in Takt's folder wins
    /// over one of the same identifier in Zed's.
    pub fn reload_themes(&mut self) {
        let built_ins = takt_core::theme::theme_builtins(CoreThemePlatform::Macos);
        let load = |sources: Vec<CoreThemeFileSource>| {
            takt_core::theme::theme_load(
                sources,
                CoreThemePlatform::Macos,
                Some(built_ins.clone()),
                built_ins.first().cloned(),
            )
        };
        let zed = zed_themes_dir()
            .map(|dir| load(theme_sources(&dir)))
            .unwrap_or_default();
        let own = load(theme_sources(&themes_dir()));
        // Zed One is the core's too, for the phones; here it is drawn from
        // Zed's own tokens, so the core's copy is not listed twice.
        let mut themes = vec![Theme::Zed];
        themes.extend(
            built_ins
                .iter()
                .filter(|s| s.identifier != ONE)
                .cloned()
                .map(|s| Theme::Spec(Box::new(s))),
        );
        let mut issues = Vec::new();
        for (folder, outcomes) in [("Zed themes/", zed), ("", own)] {
            for outcome in outcomes {
                // Zed's folder is Zed's to keep tidy: only what stops a
                // theme there loading is worth saying.
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
                        themes.push(Theme::Spec(Box::new(spec)));
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
        let by_name = zed_themes_dir()
            .map(|dir| zed_chrome_by_name(&dir))
            .unwrap_or_default();
        self.library.zed_chrome = themes
            .iter()
            .filter_map(|theme| match theme {
                Theme::Spec(spec)
                    if spec
                        .identifier
                        .starts_with(takt_core::theme::zed::IDENTIFIER_PREFIX) =>
                {
                    by_name
                        .get(&spec.name)
                        .map(|chrome| (spec.identifier.clone(), *chrome))
                }
                _ => None,
            })
            .collect();
        self.library.themes = themes;
        self.library.theme_issues = issues;
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

    /// The chrome colours the theme in force states in Zed's format, if any.
    pub fn theme_chrome(&self) -> ZedChrome {
        self.library
            .zed_chrome
            .get(&self.prefs.theme)
            .copied()
            .unwrap_or_default()
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
                    self.share_theme_choice();
                }
                self.message = Some(format!("Exported to {}.", path.display()));
            }
            Err(error) => self.message = Some(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zed_chrome_is_read_by_variant_name() {
        let dir = std::env::temp_dir().join(format!("takt-zed-chrome-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Paper.json"),
            r##"{"name":"Paper","themes":[
                {"name":"Paper - Contrast light","appearance":"light",
                 "style":{"status_bar.background":"#0fbda8","border.variant":"#6547b8"}},
                {"name":"Paper - Guides","style":{"panel.indent_guide":"#d7d2bcff"}}]}"##,
        )
        .unwrap();
        let found = zed_chrome_by_name(&dir);
        let paper = found["Paper - Contrast light"];
        let teal = paper.status_bar.unwrap();
        assert!((teal.green - 0xbd as f64 / 255.).abs() < 0.01);
        assert_eq!(paper.indent_guide, None);
        assert!(found["Paper - Guides"].indent_guide.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
