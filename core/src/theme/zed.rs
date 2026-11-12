//! Zed's theme format, read as Takt themes.
//!
//! A Zed theme file is a *family*: `{"$schema", "name", "author", "themes":
//! [{"name", "appearance", "style": {...}}]}`, one entry per variant. Each
//! variant becomes one Takt theme file in memory — locked to the variant's
//! appearance, extending Zed One (so it has Zed's type and square panels),
//! with every role stated, the same in both tables: those the variant's
//! `style` gives, and the rest grown from its page and text as seeds would
//! grow them. The mapping is the one the Mac app uses to draw Zed's own One
//! Dark and One Light, and is documented in `docs/themes.md` ("Zed themes").
//!
//! Detection is by shape: an object with a `themes` array, or a `$schema`
//! on zed.dev. Nothing else in the theme format has either key.

use std::collections::HashMap;

use super::builtins::ONE_IDENTIFIER;
use super::color::{contrast_ratio, from_hex, hex_string, mix, over, relative_luminance, rgba};
use super::file::{
    CoreThemeFile, CoreThemeFileBase, CoreThemeFileIssue, CoreThemeFileLock, CoreThemeFilePalette,
};
use super::json::Json;
use super::loader::stem;
use super::model::{CoreThemeAppearance, CoreThemeColor, CoreThemeIssueSeverity};
use super::seeds::CoreThemeSeeds;

/// The prefix every Zed variant's identifier takes: `zed.<variant name>`,
/// as Zed itself keys a theme by its name.
pub const IDENTIFIER_PREFIX: &str = "zed.";

/// Whether `root` is a Zed theme family rather than a Takt theme.
pub fn is_family(root: &Json) -> bool {
    let schema = matches!(root.get("$schema"), Some(Json::String(s)) if s.contains("zed.dev"));
    schema || matches!(root.get("themes"), Some(Json::Array(_)))
}

/// One variant, as a Takt file, and what converting it reported.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub file: CoreThemeFile,
    pub issues: Vec<CoreThemeFileIssue>,
}

/// Each role, and the Zed keys it is read from, first stated wins. A role
/// none of them state is grown from the seeds or kept from Zed One.
const ROLE_KEYS: [(&str, &[&str]); 14] = [
    ("paper", &["editor.background", "background"]),
    (
        "raised",
        &[
            "elevated_surface.background",
            "surface.background",
            "panel.background",
        ],
    ),
    ("altRow", &["panel.background", "surface.background"]),
    ("hover", &["element.hover", "ghost_element.hover"]),
    ("well", &["background", "element.background"]),
    ("border", &["border"]),
    ("borderMuted", &["border.variant", "border"]),
    ("inputBorder", &["border"]),
    ("ink", &["text", "editor.foreground"]),
    ("mutedText", &["text.muted"]),
    ("dimText", &["text.placeholder", "text.disabled"]),
    ("success", &["success", "created"]),
    ("danger", &["error", "deleted"]),
    ("warning", &["warning", "modified"]),
];

/// Zed's chrome, each read from these keys, first stated wins. A selection
/// and a guide the variant leaves out are derived from its own page, panel,
/// accent and text (`CoreThemePalette::chrome`) and written down, so they are
/// not inherited from Zed One; the rest stay unstated, which is how the Mac
/// knows whether the variant paints its status bar.
const CHROME_KEYS: [(&str, &[&str]); 5] = [
    ("selected", &["ghost_element.selected", "element.selected"]),
    ("indentGuide", &["panel.indent_guide"]),
    ("statusBar", &["status_bar.background"]),
    ("focus", &["panel.focused_border", "border.focused"]),
    ("icon", &["icon"]),
];

/// Where `primary` is looked for, in order: the first that stands 3:1 off
/// the page (it has to carry the focus ring) wins, else the one that stands
/// furthest off it. `players.0.cursor` and `accents.0` are read too, after
/// `border.focused`, and then every identity colour the variant has.
const PRIMARY_KEYS: [&str; 3] = ["text.accent", "icon.accent", "border.focused"];
const PRIMARY_AFTER: [&str; 3] = ["info", "link_text.hover", "terminal.ansi.blue"];

/// The contrast `primary` is held to, as the audit holds it.
const PRIMARY_CONTRAST: f64 = 3.0;

/// The categorical roles, and the hue (degrees) each is matched to among the
/// variant's accents, players and terminal and syntax colours.
const CATEGORICAL: [(&str, f64); 3] = [
    ("categoricalPurple", 265.0),
    ("categoricalPink", 320.0),
    ("categoricalOrange", 25.0),
];

/// How far (degrees) a candidate may sit from a categorical hue.
const HUE_TOLERANCE: f64 = 30.0;

/// Lowercase letters and digits, every other run one hyphen.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

fn text(value: Option<&Json>) -> Option<String> {
    match value {
        Some(Json::String(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    }
}

/// Hue in degrees and saturation (HSL), for matching the categorical roles.
fn hue_saturation(c: &CoreThemeColor) -> (f64, f64) {
    let max = c.red.max(c.green).max(c.blue);
    let min = c.red.min(c.green).min(c.blue);
    let delta = max - min;
    if delta <= f64::EPSILON {
        return (0.0, 0.0);
    }
    let lightness = (max + min) / 2.0;
    let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs()).max(f64::EPSILON);
    let hue = if max == c.red {
        ((c.green - c.blue) / delta).rem_euclid(6.0)
    } else if max == c.green {
        (c.blue - c.red) / delta + 2.0
    } else {
        (c.red - c.green) / delta + 4.0
    } * 60.0;
    (hue, saturation)
}

fn hue_distance(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

struct Reader<'a> {
    source: &'a str,
    path: String,
    style: &'a Json,
    issues: Vec<CoreThemeFileIssue>,
}

/// Zed's colours are `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`; Takt reads
/// all but the second, so that one is spelt out first.
fn zed_hex(raw: &str) -> Option<CoreThemeColor> {
    let trimmed = raw.trim();
    let digits = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if digits.chars().count() == 4 && digits.chars().all(|c| c.is_ascii_hexdigit()) {
        let long: String = digits.chars().flat_map(|c| [c, c]).collect();
        return from_hex(&long);
    }
    from_hex(trimmed)
}

impl Reader<'_> {
    fn warn(&mut self, message: String) {
        // A key read for two roles is reported once.
        if self.issues.iter().any(|i| i.message == message) {
            return;
        }
        self.issues.push(CoreThemeFileIssue::new(
            self.source,
            CoreThemeIssueSeverity::Warning,
            message,
        ));
    }

    /// The colour under `key` in `style`; a value that is not a hex colour
    /// is reported and treated as absent.
    fn colour(&mut self, key: &str) -> Option<CoreThemeColor> {
        let value = self.style.get(key)?;
        self.parse(value, &format!("{}.{key}", self.path))
    }

    fn parse(&mut self, value: &Json, path: &str) -> Option<CoreThemeColor> {
        match value {
            Json::Null => None,
            Json::String(raw) => {
                let parsed = zed_hex(raw);
                if parsed.is_none() {
                    self.warn(format!("{path} \"{raw}\" is not a hex colour; ignored"));
                }
                parsed
            }
            _ => {
                self.warn(format!("{path} should be a hex colour; ignored"));
                None
            }
        }
    }

    fn first(&mut self, keys: &[&str]) -> Option<CoreThemeColor> {
        keys.iter().find_map(|key| self.colour(key))
    }

    /// Every saturated colour the variant gives for identity: `accents`,
    /// each player's cursor, the terminal's ANSI colours and the syntax
    /// colours, in that order.
    fn candidates(&mut self) -> Vec<CoreThemeColor> {
        let mut found = Vec::new();
        let style = self.style;
        if let Some(Json::Array(accents)) = style.get("accents") {
            for (i, accent) in accents.iter().enumerate() {
                let path = format!("{}.accents.{i}", self.path);
                found.extend(self.parse(accent, &path));
            }
        }
        if let Some(Json::Array(players)) = style.get("players") {
            for (i, player) in players.iter().enumerate() {
                if let Some(cursor) = player.get("cursor") {
                    let path = format!("{}.players.{i}.cursor", self.path);
                    found.extend(self.parse(cursor, &path));
                }
            }
        }
        if let Json::Object(entries) = style {
            for (key, value) in entries {
                if key.starts_with("terminal.ansi.") && !key.contains("dim") {
                    let path = format!("{}.{key}", self.path);
                    found.extend(self.parse(value, &path));
                }
            }
        }
        if let Some(Json::Object(syntax)) = style.get("syntax") {
            for (key, entry) in syntax {
                if let Some(color) = entry.get("color") {
                    let path = format!("{}.syntax.{key}.color", self.path);
                    found.extend(self.parse(color, &path));
                }
            }
        }
        found
    }
}

/// The nearest saturated candidate to `hue`, within the tolerance.
fn nearest(candidates: &[CoreThemeColor], hue: f64) -> Option<CoreThemeColor> {
    candidates
        .iter()
        .filter_map(|c| {
            let (h, s) = hue_saturation(c);
            let d = hue_distance(h, hue);
            (s >= 0.3 && d <= HUE_TOLERANCE).then_some((d, *c))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, c)| c)
}

fn variant(
    entry: &Json,
    index: usize,
    family: Option<&str>,
    author: Option<&str>,
    source: &str,
) -> Variant {
    let path = format!("themes.{index}");
    let empty = Json::Object(Vec::new());
    let style = match entry.get("style") {
        Some(style @ Json::Object(_)) => style,
        _ => &empty,
    };
    let mut reader = Reader {
        source,
        path: format!("{path}.style"),
        style,
        issues: Vec::new(),
    };
    if !matches!(entry, Json::Object(_)) {
        reader.warn(format!("{path} should be an object"));
    }
    let name = text(entry.get("name"));
    let mut roles: HashMap<&str, CoreThemeColor> = HashMap::new();
    for (role, keys) in ROLE_KEYS {
        if let Some(colour) = reader.first(keys) {
            roles.insert(role, colour);
        }
    }
    let candidates = reader.candidates();
    let mut accents: Vec<CoreThemeColor> = PRIMARY_KEYS
        .iter()
        .filter_map(|key| reader.colour(key))
        .collect();
    let first_player = match style.get("players") {
        Some(Json::Array(players)) => players.first().and_then(|p| p.get("cursor")),
        _ => None,
    };
    let first_accent = match style.get("accents") {
        Some(Json::Array(list)) => list.first(),
        _ => None,
    };
    for (value, at) in [
        (first_player, "players.0.cursor"),
        (first_accent, "accents.0"),
    ] {
        if let Some(value) = value {
            let at = format!("{}.{at}", reader.path);
            accents.extend(reader.parse(value, &at));
        }
    }
    accents.extend(PRIMARY_AFTER.iter().filter_map(|key| reader.colour(key)));
    accents.extend(candidates.iter().copied());
    for (role, hue) in CATEGORICAL {
        if let Some(colour) = nearest(&candidates, hue) {
            roles.insert(role, colour);
        }
    }

    // The page itself is made opaque over the window background (or the
    // appearance's own white or black); every other role over the page, as
    // Takt draws it on the page.
    let stated = text(entry.get("appearance"));
    let appearance = match stated.as_deref().map(CoreThemeAppearance::parse) {
        Some(Some(appearance)) => appearance,
        other => {
            let guessed = match roles.get("paper") {
                Some(paper) if relative_luminance(paper) > 0.4 => CoreThemeAppearance::Light,
                _ => CoreThemeAppearance::Dark,
            };
            reader.warn(match other {
                Some(None) => format!(
                    "{path}.appearance \"{}\" is not light or dark; read as {}",
                    stated.unwrap_or_default(),
                    guessed.raw()
                ),
                _ => format!("{path}.appearance is missing; read as {}", guessed.raw()),
            });
            guessed
        }
    };
    let canvas = match appearance {
        CoreThemeAppearance::Light => rgba(1.0, 1.0, 1.0, 1.0),
        CoreThemeAppearance::Dark => rgba(0.0, 0.0, 0.0, 1.0),
    };
    let window = reader.colour("background").map(|b| over(b, canvas));
    if let Some(paper) = roles.get_mut("paper") {
        *paper = over(*paper, window.unwrap_or(canvas));
    }
    let paper = roles.get("paper").copied();
    let flat = |c: CoreThemeColor| paper.map_or(c, |p| over(c, p));
    for (role, colour) in roles.iter_mut() {
        if *role != "paper" {
            *colour = flat(*colour);
        }
    }
    let accents: Vec<CoreThemeColor> = accents.into_iter().map(flat).collect();
    let primary = match paper {
        Some(paper) => accents
            .iter()
            .find(|c| contrast_ratio(c, &paper) >= PRIMARY_CONTRAST)
            .or_else(|| {
                accents
                    .iter()
                    .max_by(|a, b| contrast_ratio(a, &paper).total_cmp(&contrast_ratio(b, &paper)))
            })
            .copied(),
        None => accents.first().copied(),
    };
    if let Some(primary) = primary {
        roles.insert("primary", primary);
    }

    // What the variant leaves out is grown from its own page and text, the
    // way a seeded Takt theme is, rather than kept from Zed One.
    let mut table: HashMap<String, String> = HashMap::new();
    match (roles.get("paper"), roles.get("ink")) {
        (Some(paper), Some(ink)) => {
            let seeds = CoreThemeSeeds {
                background: Some(*paper),
                foreground: Some(*ink),
                ..Default::default()
            };
            for (role, colour) in seeds.roles(appearance).unwrap_or_default() {
                table.insert(role, hex_string(&colour));
            }
        }
        _ => reader.warn(format!(
            "{path}.style has no background or no text colour; the rest comes from Zed One"
        )),
    }
    for (role, colour) in &roles {
        table.insert(role.to_string(), hex_string(colour));
    }
    // Zed's chrome, opaque: a guide and the status bar over the panel, as
    // Zed draws them, the rest over the page.
    let at = |role: &str| {
        table
            .get(role)
            .and_then(|hex| from_hex(hex))
            .unwrap_or(canvas)
    };
    let (page, panel, accent, ink) = (at("paper"), at("altRow"), at("primary"), at("ink"));
    let mut chrome = Vec::new();
    for (role, keys) in CHROME_KEYS {
        let below = if matches!(role, "indentGuide" | "statusBar") {
            panel
        } else {
            page
        };
        let colour = match reader.first(keys) {
            Some(stated) => Some(over(stated, below)),
            None => match role {
                "selected" => Some(mix(&page, &accent, 0.16)),
                "indentGuide" => Some(mix(&panel, &ink, 0.14)),
                _ => None,
            },
        };
        if let Some(colour) = colour {
            chrome.push((role, hex_string(&colour)));
        }
    }
    for (role, hex) in chrome {
        table.insert(role.to_string(), hex);
    }
    // The same table both ways: the theme is locked to its appearance, and
    // a reader that ignored the lock would still see the variant.
    let palette = (!table.is_empty()).then(|| CoreThemeFilePalette {
        light: Some(table.clone()),
        dark: Some(table),
    });

    let fallback = format!("{}-{}", slug(family.unwrap_or(&stem(source))), index + 1);
    let identifier = match name.as_deref().map(slug) {
        Some(s) if !s.is_empty() => s,
        _ => fallback,
    };
    let summary = match (family, author) {
        (Some(family), Some(author)) => format!("From the Zed theme {family}, by {author}."),
        (Some(family), None) => format!("From the Zed theme {family}."),
        (None, Some(author)) => format!("A Zed theme, by {author}."),
        (None, None) => "A Zed theme.".to_string(),
    };
    Variant {
        file: CoreThemeFile {
            identifier: Some(format!("{IDENTIFIER_PREFIX}{identifier}")),
            name: Some(
                name.unwrap_or_else(|| format!("{} {}", family.unwrap_or("Zed theme"), index + 1)),
            ),
            summary: Some(summary),
            locked_appearance: CoreThemeFileLock::Locked {
                raw: appearance.raw().to_string(),
            },
            extends: CoreThemeFileBase::Theme {
                identifier: ONE_IDENTIFIER.to_string(),
            },
            seeds: None,
            palette,
            structure: None,
            platforms: None,
        },
        issues: reader.issues,
    }
}

/// Every variant of a family, in the family's order, or why the family
/// gives none.
pub fn variants(root: &Json, source: &str) -> Result<Vec<Variant>, String> {
    let Some(Json::Array(themes)) = root.get("themes") else {
        return Err("a Zed theme should have a themes list".to_string());
    };
    if themes.is_empty() {
        return Err("a Zed theme with no themes in its list".to_string());
    }
    let family = text(root.get("name"));
    let author = text(root.get("author"));
    Ok(themes
        .iter()
        .enumerate()
        .map(|(i, entry)| variant(entry, i, family.as_deref(), author.as_deref(), source))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_lowercase_and_hyphenated() {
        assert_eq!(slug("Meadow Dark (Hi-Con)"), "meadow-dark-hi-con");
        assert_eq!(slug("  "), "");
    }

    #[test]
    fn translucent_colours_are_flattened_over_what_is_below() {
        let white = rgba(1.0, 1.0, 1.0, 1.0);
        let half_black = rgba(0.0, 0.0, 0.0, 0.5);
        assert_eq!(hex_string(&over(half_black, white)), "#808080");
    }

    #[test]
    fn hue_matching_ignores_greys_and_distant_hues() {
        let grey = from_hex("#808080").unwrap();
        let orange = from_hex("#ff7a1a").unwrap();
        let blue = from_hex("#3070ff").unwrap();
        assert_eq!(nearest(&[grey, blue, orange], 25.0), Some(orange));
        assert_eq!(nearest(&[grey, blue], 25.0), None);
    }
}
