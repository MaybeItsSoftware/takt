//! A theme as someone writes one: a JSON document, every field optional and
//! every value a partial override of the theme it `extends`.
//!
//! Enumerated values (appearances, designs, weights, roles) are carried as
//! strings, so one typo is one reported issue rather than a file that will
//! not read. Decoding reproduces Swift's `Codable` decode, which defined the
//! format: an absent or `null` optional is nothing, and the first value of
//! the wrong type fails the whole file, named by its dotted path.

use std::collections::HashMap;

use super::json::{self, Json};
use super::model::{CoreThemeIssueSeverity, CoreThemePlatform};
use super::zed;

/// Name → hex per appearance: a role under `palette`, a seed under `seeds`.
#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct CoreThemeFilePalette {
    pub light: Option<HashMap<String, String>>,
    pub dark: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct CoreThemeFileRadius {
    pub panel: Option<f64>,
    pub row: Option<f64>,
    pub control: Option<f64>,
    pub pill: Option<f64>,
    pub shell: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct CoreThemeFileBorder {
    pub hairline: Option<f64>,
    pub emphasis: Option<f64>,
    pub focus_ring: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct CoreThemeFileSpacing {
    pub xxs: Option<f64>,
    pub xs: Option<f64>,
    pub sm: Option<f64>,
    pub md: Option<f64>,
    pub lg: Option<f64>,
    pub xl: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct CoreThemeFileFace {
    pub families: Option<Vec<String>>,
    pub design: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct CoreThemeFileTypeScale {
    pub caption: Option<f64>,
    pub body: Option<f64>,
    pub title: Option<f64>,
    pub display: Option<f64>,
    pub hero: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct CoreThemeFileMicroLabel {
    pub size: Option<f64>,
    pub weight: Option<String>,
    pub tracking: Option<f64>,
    pub uppercase: Option<bool>,
    pub role: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct CoreThemeFileTypography {
    pub display: Option<CoreThemeFileFace>,
    pub body: Option<CoreThemeFileFace>,
    pub mono: Option<CoreThemeFileFace>,
    pub body_size: Option<f64>,
    pub scale: Option<CoreThemeFileTypeScale>,
    pub micro_label: Option<CoreThemeFileMicroLabel>,
}

#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct CoreThemeFileStructure {
    pub radius: Option<CoreThemeFileRadius>,
    pub border: Option<CoreThemeFileBorder>,
    pub spacing: Option<CoreThemeFileSpacing>,
    pub typography: Option<CoreThemeFileTypography>,
    pub touch_target: Option<f64>,
    pub uses_shadows: Option<bool>,
    pub uses_gradients_on_chrome: Option<bool>,
}

/// One platform's entry under `platforms`: structure only.
#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct CoreThemeFilePlatformOverride {
    pub structure: Option<CoreThemeFileStructure>,
}

#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct CoreThemeFilePlatforms {
    pub macos: Option<CoreThemeFilePlatformOverride>,
    pub ios: Option<CoreThemeFilePlatformOverride>,
    pub android: Option<CoreThemeFilePlatformOverride>,
}

impl CoreThemeFilePlatforms {
    pub fn get(&self, platform: CoreThemePlatform) -> Option<&CoreThemeFilePlatformOverride> {
        match platform {
            CoreThemePlatform::Macos => self.macos.as_ref(),
            CoreThemePlatform::Ios => self.ios.as_ref(),
            CoreThemePlatform::Android => self.android.as_ref(),
        }
    }

    pub fn set(
        &mut self,
        platform: CoreThemePlatform,
        value: Option<CoreThemeFilePlatformOverride>,
    ) {
        match platform {
            CoreThemePlatform::Macos => self.macos = value,
            CoreThemePlatform::Ios => self.ios = value,
            CoreThemePlatform::Android => self.android = value,
        }
    }
}

/// What the file inherits every value it does not state from.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum CoreThemeFileBase {
    /// Key absent: the default theme.
    DefaultTheme,
    /// `"extends": "<identifier>"`.
    Theme { identifier: String },
    /// `"extends": null`: no colours inherited.
    Nothing,
}

/// `lockedAppearance`: absent inherits, `null` clears, a string sets.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum CoreThemeFileLock {
    Inherit,
    Unlocked,
    /// Carried raw, so an unknown value is a reported issue.
    Locked {
        raw: String,
    },
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreThemeFile {
    pub identifier: Option<String>,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub locked_appearance: CoreThemeFileLock,
    pub extends: CoreThemeFileBase,
    pub seeds: Option<CoreThemeFilePalette>,
    pub palette: Option<CoreThemeFilePalette>,
    pub structure: Option<CoreThemeFileStructure>,
    pub platforms: Option<CoreThemeFilePlatforms>,
}

impl Default for CoreThemeFile {
    fn default() -> Self {
        CoreThemeFile {
            identifier: None,
            name: None,
            summary: None,
            locked_appearance: CoreThemeFileLock::Inherit,
            extends: CoreThemeFileBase::DefaultTheme,
            seeds: None,
            palette: None,
            structure: None,
            platforms: None,
        }
    }
}

/// Something a theme file got wrong, in a sentence someone editing it can act on.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreThemeFileIssue {
    /// The file name, e.g. `dusk.json`.
    pub source: String,
    pub severity: CoreThemeIssueSeverity,
    pub message: String,
    /// A finding of the audit of the theme the file produced, as opposed to
    /// a problem with what the file says.
    pub is_audit: bool,
}

impl CoreThemeFileIssue {
    pub fn new(source: &str, severity: CoreThemeIssueSeverity, message: impl Into<String>) -> Self {
        CoreThemeFileIssue {
            source: source.to_string(),
            severity,
            message: message.into(),
            is_audit: false,
        }
    }
}

// MARK: - Decoding

/// Decodes one file. A document that is not a theme is `None` plus the
/// reason; keys the format does not have are warnings. A Zed theme family
/// decodes as its first variant (see [`decode_all`] for every one).
pub fn decode(data: &[u8], source: &str) -> (Option<CoreThemeFile>, Vec<CoreThemeFileIssue>) {
    match decode_all(data, source) {
        Ok(mut themes) => {
            let (file, issues) = themes.remove(0);
            (Some(file), issues)
        }
        Err(issues) => (None, issues),
    }
}

/// Decodes one file into every theme it holds: one for a Takt theme, one
/// per variant for a Zed theme family (`zed.rs`), in the family's order.
/// Never an empty list; a file with no theme in it is its issues.
pub fn decode_all(
    data: &[u8],
    source: &str,
) -> Result<Vec<(CoreThemeFile, Vec<CoreThemeFileIssue>)>, Vec<CoreThemeFileIssue>> {
    let Some(root) = json::parse(data) else {
        return Err(vec![CoreThemeFileIssue::new(
            source,
            CoreThemeIssueSeverity::Error,
            "not valid JSON",
        )]);
    };
    if zed::is_family(&root) {
        return zed::variants(&root, source)
            .map(|variants| variants.into_iter().map(|v| (v.file, v.issues)).collect())
            .map_err(|message| {
                vec![CoreThemeFileIssue::new(
                    source,
                    CoreThemeIssueSeverity::Error,
                    message,
                )]
            });
    }
    let (file, issues) = decode_takt(&root, source);
    match file {
        Some(file) => Ok(vec![(file, issues)]),
        None => Err(issues),
    }
}

fn decode_takt(root: &Json, source: &str) -> (Option<CoreThemeFile>, Vec<CoreThemeFileIssue>) {
    let file = match read(root) {
        Ok(file) => file,
        Err(message) => {
            return (
                None,
                vec![CoreThemeFileIssue::new(
                    source,
                    CoreThemeIssueSeverity::Error,
                    message,
                )],
            );
        }
    };
    let issues = unknown_keys(root)
        .into_iter()
        .map(|path| {
            let message = if is_platform_palette(&path) {
                format!("{path} is not allowed: colour is the same on every platform; ignored")
            } else {
                format!("{path} is not a theme setting; ignored")
            };
            CoreThemeFileIssue::new(source, CoreThemeIssueSeverity::Warning, message)
        })
        .collect();
    (Some(file), issues)
}

type Read<T> = Result<T, String>;

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

fn mismatch(path: &str, expected: &str) -> String {
    let path = if path.is_empty() { "the file" } else { path };
    format!("{path} should be {expected}")
}

/// A nested object under `key`, or `None` when absent or `null`.
fn child<'a>(object: &'a Json, key: &str, path: &str) -> Read<Option<(&'a Json, String)>> {
    match object.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(value @ Json::Object(_)) => Ok(Some((value, join(path, key)))),
        Some(_) => Err(mismatch(&join(path, key), "an object")),
    }
}

fn string_value(value: &Json, path: &str) -> Read<String> {
    match value {
        Json::String(s) => Ok(s.clone()),
        Json::Null => Err(format!("{path} should be a string, not null")),
        _ => Err(mismatch(path, "a string")),
    }
}

fn string(object: &Json, key: &str, path: &str) -> Read<Option<String>> {
    match object.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(value) => string_value(value, &join(path, key)).map(Some),
    }
}

fn number(object: &Json, key: &str, path: &str) -> Read<Option<f64>> {
    match object.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Number(n)) => Ok(Some(*n)),
        Some(_) => Err(mismatch(&join(path, key), "a number")),
    }
}

fn boolean(object: &Json, key: &str, path: &str) -> Read<Option<bool>> {
    match object.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(mismatch(&join(path, key), "true or false")),
    }
}

fn string_map(object: &Json, key: &str, path: &str) -> Read<Option<HashMap<String, String>>> {
    let Some((Json::Object(entries), table_path)) = child(object, key, path)? else {
        return Ok(None);
    };
    let mut table = HashMap::new();
    // Every entry is read, in document order; a repeated key keeps its first value.
    for (name, value) in entries {
        let value = string_value(value, &join(&table_path, name))?;
        table.entry(name.clone()).or_insert(value);
    }
    Ok(Some(table))
}

fn string_list(object: &Json, key: &str, path: &str) -> Read<Option<Vec<String>>> {
    let list_path = join(path, key);
    match object.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(index, item)| string_value(item, &format!("{list_path}.{index}")))
            .collect::<Read<Vec<_>>>()
            .map(Some),
        // Swift's decoder names the expectation `[Any]`, which reads as "an object".
        Some(_) => Err(mismatch(&list_path, "an object")),
    }
}

/// Reads a file out of parsed JSON, in the order Swift's decoder read it, so
/// the first bad value reported is the same one.
fn read(root: &Json) -> Read<CoreThemeFile> {
    match root {
        Json::Object(_) => {}
        Json::Null => return Err("the file should be an object, not null".to_string()),
        _ => return Err(mismatch("", "an object")),
    }
    let palette = |key: &str| -> Read<Option<CoreThemeFilePalette>> {
        child(root, key, "")?
            .map(|(p, path)| {
                Ok(CoreThemeFilePalette {
                    light: string_map(p, "light", &path)?,
                    dark: string_map(p, "dark", &path)?,
                })
            })
            .transpose()
    };
    let identifier = string(root, "identifier", "")?;
    let name = string(root, "name", "")?;
    let summary = string(root, "summary", "")?;
    let seeds = palette("seeds")?;
    let palette_table = palette("palette")?;
    let structure = child(root, "structure", "")?
        .map(|(s, path)| read_structure(s, &path))
        .transpose()?;
    let platforms = child(root, "platforms", "")?
        .map(|(p, path)| -> Read<CoreThemeFilePlatforms> {
            let one = |key: &str| -> Read<Option<CoreThemeFilePlatformOverride>> {
                child(p, key, &path)?
                    .map(|(entry, entry_path)| {
                        Ok(CoreThemeFilePlatformOverride {
                            structure: child(entry, "structure", &entry_path)?
                                .map(|(s, sp)| read_structure(s, &sp))
                                .transpose()?,
                        })
                    })
                    .transpose()
            };
            Ok(CoreThemeFilePlatforms {
                macos: one("macos")?,
                ios: one("ios")?,
                android: one("android")?,
            })
        })
        .transpose()?;
    let extends = match root.get("extends") {
        None => CoreThemeFileBase::DefaultTheme,
        Some(Json::Null) => CoreThemeFileBase::Nothing,
        Some(value) => CoreThemeFileBase::Theme {
            identifier: string_value(value, "extends")?,
        },
    };
    let locked_appearance = match root.get("lockedAppearance") {
        None => CoreThemeFileLock::Inherit,
        Some(Json::Null) => CoreThemeFileLock::Unlocked,
        Some(value) => CoreThemeFileLock::Locked {
            raw: string_value(value, "lockedAppearance")?,
        },
    };
    Ok(CoreThemeFile {
        identifier,
        name,
        summary,
        locked_appearance,
        extends,
        seeds,
        palette: palette_table,
        structure,
        platforms,
    })
}

fn read_structure(s: &Json, path: &str) -> Read<CoreThemeFileStructure> {
    let radius = child(s, "radius", path)?
        .map(|(r, p)| -> Read<_> {
            Ok(CoreThemeFileRadius {
                panel: number(r, "panel", &p)?,
                row: number(r, "row", &p)?,
                control: number(r, "control", &p)?,
                pill: number(r, "pill", &p)?,
                shell: number(r, "shell", &p)?,
            })
        })
        .transpose()?;
    let border = child(s, "border", path)?
        .map(|(b, p)| -> Read<_> {
            Ok(CoreThemeFileBorder {
                hairline: number(b, "hairline", &p)?,
                emphasis: number(b, "emphasis", &p)?,
                focus_ring: number(b, "focusRing", &p)?,
            })
        })
        .transpose()?;
    let spacing = child(s, "spacing", path)?
        .map(|(sp, p)| -> Read<_> {
            Ok(CoreThemeFileSpacing {
                xxs: number(sp, "xxs", &p)?,
                xs: number(sp, "xs", &p)?,
                sm: number(sp, "sm", &p)?,
                md: number(sp, "md", &p)?,
                lg: number(sp, "lg", &p)?,
                xl: number(sp, "xl", &p)?,
            })
        })
        .transpose()?;
    let typography = child(s, "typography", path)?
        .map(|(t, p)| -> Read<_> {
            let face = |key: &str| -> Read<Option<CoreThemeFileFace>> {
                child(t, key, &p)?
                    .map(|(f, fp)| {
                        Ok(CoreThemeFileFace {
                            families: string_list(f, "families", &fp)?,
                            design: string(f, "design", &fp)?,
                        })
                    })
                    .transpose()
            };
            Ok(CoreThemeFileTypography {
                display: face("display")?,
                body: face("body")?,
                mono: face("mono")?,
                body_size: number(t, "bodySize", &p)?,
                scale: child(t, "scale", &p)?
                    .map(|(sc, scp)| -> Read<_> {
                        Ok(CoreThemeFileTypeScale {
                            caption: number(sc, "caption", &scp)?,
                            body: number(sc, "body", &scp)?,
                            title: number(sc, "title", &scp)?,
                            display: number(sc, "display", &scp)?,
                            hero: number(sc, "hero", &scp)?,
                        })
                    })
                    .transpose()?,
                micro_label: child(t, "microLabel", &p)?
                    .map(|(m, mp)| -> Read<_> {
                        Ok(CoreThemeFileMicroLabel {
                            size: number(m, "size", &mp)?,
                            weight: string(m, "weight", &mp)?,
                            tracking: number(m, "tracking", &mp)?,
                            uppercase: boolean(m, "uppercase", &mp)?,
                            role: string(m, "role", &mp)?,
                        })
                    })
                    .transpose()?,
            })
        })
        .transpose()?;
    Ok(CoreThemeFileStructure {
        radius,
        border,
        spacing,
        typography,
        touch_target: number(s, "touchTarget", path)?,
        uses_shadows: boolean(s, "usesShadows", path)?,
        uses_gradients_on_chrome: boolean(s, "usesGradientsOnChrome", path)?,
    })
}

// MARK: - Unknown keys

/// The shape of the format: an object with exactly these keys, or anything.
enum Node {
    Object(Vec<(&'static str, Node)>),
    Any,
}

fn schema() -> Node {
    use Node::{Any, Object};
    let face = || Object(vec![("families", Any), ("design", Any)]);
    let structure = || {
        Object(vec![
            (
                "radius",
                Object(vec![
                    ("panel", Any),
                    ("row", Any),
                    ("control", Any),
                    ("pill", Any),
                    ("shell", Any),
                ]),
            ),
            (
                "border",
                Object(vec![
                    ("hairline", Any),
                    ("emphasis", Any),
                    ("focusRing", Any),
                ]),
            ),
            (
                "spacing",
                Object(vec![
                    ("xxs", Any),
                    ("xs", Any),
                    ("sm", Any),
                    ("md", Any),
                    ("lg", Any),
                    ("xl", Any),
                ]),
            ),
            (
                "typography",
                Object(vec![
                    ("display", face()),
                    ("body", face()),
                    ("mono", face()),
                    ("bodySize", Any),
                    (
                        "scale",
                        Object(vec![
                            ("caption", Any),
                            ("body", Any),
                            ("title", Any),
                            ("display", Any),
                            ("hero", Any),
                        ]),
                    ),
                    (
                        "microLabel",
                        Object(vec![
                            ("size", Any),
                            ("weight", Any),
                            ("tracking", Any),
                            ("uppercase", Any),
                            ("role", Any),
                        ]),
                    ),
                ]),
            ),
            ("touchTarget", Any),
            ("usesShadows", Any),
            ("usesGradientsOnChrome", Any),
        ])
    };
    // A platform's entry is structure only; a `palette` there is reported.
    let platform = || Object(vec![("structure", structure())]);
    Object(vec![
        ("identifier", Any),
        ("name", Any),
        ("summary", Any),
        ("lockedAppearance", Any),
        ("extends", Any),
        // Role and seed names are checked by the merge, which can say which.
        ("palette", Object(vec![("light", Any), ("dark", Any)])),
        ("seeds", Object(vec![("light", Any), ("dark", Any)])),
        ("structure", structure()),
        (
            "platforms",
            Object(
                CoreThemePlatform::ALL
                    .iter()
                    .map(|p| (p.raw(), platform()))
                    .collect(),
            ),
        ),
    ])
}

/// `platforms.ios.palette`: not a typo, a thing the format refuses.
pub fn is_platform_palette(path: &str) -> bool {
    let parts: Vec<&str> = path.split('.').collect();
    parts.len() == 3
        && parts[0] == "platforms"
        && parts[2] == "palette"
        && CoreThemePlatform::parse(parts[1]).is_some()
}

/// Dotted paths of keys the format does not have, sorted.
pub fn unknown_keys(root: &Json) -> Vec<String> {
    fn walk(value: &Json, node: &Node, path: &str, found: &mut Vec<String>) {
        let (Node::Object(children), Json::Object(entries)) = (node, value) else {
            return;
        };
        let mut seen: Vec<&str> = Vec::new();
        for (key, child) in entries {
            // A key stated twice is one key, holding its first value.
            if seen.contains(&key.as_str()) {
                continue;
            }
            seen.push(key);
            let child_path = join(path, key);
            match children.iter().find(|(name, _)| name == key) {
                Some((_, schema)) => walk(child, schema, &child_path, found),
                None => found.push(child_path),
            }
        }
    }
    let mut found = Vec::new();
    walk(root, &schema(), "", &mut found);
    found.sort();
    found
}

// MARK: - Encoding

fn num(value: Option<f64>) -> Option<Json> {
    value.map(Json::Number)
}

fn text(value: &Option<String>) -> Option<Json> {
    value.clone().map(Json::String)
}

fn string_table(table: &Option<HashMap<String, String>>) -> Option<Json> {
    table.as_ref().map(|t| {
        Json::Object(
            t.iter()
                .map(|(k, v)| (k.clone(), Json::String(v.clone())))
                .collect(),
        )
    })
}

fn palette_json(palette: &Option<CoreThemeFilePalette>) -> Option<Json> {
    palette.as_ref().map(|p| {
        Json::object([
            ("light", string_table(&p.light)),
            ("dark", string_table(&p.dark)),
        ])
    })
}

pub fn structure_json(s: &CoreThemeFileStructure) -> Json {
    let face = |f: &Option<CoreThemeFileFace>| {
        f.as_ref().map(|f| {
            Json::object([
                (
                    "families",
                    f.families
                        .as_ref()
                        .map(|list| Json::Array(list.iter().cloned().map(Json::String).collect())),
                ),
                ("design", text(&f.design)),
            ])
        })
    };
    Json::object([
        (
            "radius",
            s.radius.as_ref().map(|r| {
                Json::object([
                    ("panel", num(r.panel)),
                    ("row", num(r.row)),
                    ("control", num(r.control)),
                    ("pill", num(r.pill)),
                    ("shell", num(r.shell)),
                ])
            }),
        ),
        (
            "border",
            s.border.as_ref().map(|b| {
                Json::object([
                    ("hairline", num(b.hairline)),
                    ("emphasis", num(b.emphasis)),
                    ("focusRing", num(b.focus_ring)),
                ])
            }),
        ),
        (
            "spacing",
            s.spacing.as_ref().map(|sp| {
                Json::object([
                    ("xxs", num(sp.xxs)),
                    ("xs", num(sp.xs)),
                    ("sm", num(sp.sm)),
                    ("md", num(sp.md)),
                    ("lg", num(sp.lg)),
                    ("xl", num(sp.xl)),
                ])
            }),
        ),
        (
            "typography",
            s.typography.as_ref().map(|t| {
                Json::object([
                    ("display", face(&t.display)),
                    ("body", face(&t.body)),
                    ("mono", face(&t.mono)),
                    ("bodySize", num(t.body_size)),
                    (
                        "scale",
                        t.scale.as_ref().map(|sc| {
                            Json::object([
                                ("caption", num(sc.caption)),
                                ("body", num(sc.body)),
                                ("title", num(sc.title)),
                                ("display", num(sc.display)),
                                ("hero", num(sc.hero)),
                            ])
                        }),
                    ),
                    (
                        "microLabel",
                        t.micro_label.as_ref().map(|m| {
                            Json::object([
                                ("size", num(m.size)),
                                ("weight", text(&m.weight)),
                                ("tracking", num(m.tracking)),
                                ("uppercase", m.uppercase.map(Json::Bool)),
                                ("role", text(&m.role)),
                            ])
                        }),
                    ),
                ])
            }),
        ),
        ("touchTarget", num(s.touch_target)),
        ("usesShadows", s.uses_shadows.map(Json::Bool)),
        (
            "usesGradientsOnChrome",
            s.uses_gradients_on_chrome.map(Json::Bool),
        ),
    ])
}

/// The file as JSON: absent fields left out, `extends` and
/// `lockedAppearance` written as `null` when that is what they say.
pub fn file_json(file: &CoreThemeFile) -> Json {
    let platforms = file.platforms.as_ref().map(|p| {
        let one = |o: &Option<CoreThemeFilePlatformOverride>| {
            o.as_ref()
                .map(|o| Json::object([("structure", o.structure.as_ref().map(structure_json))]))
        };
        Json::object([
            ("macos", one(&p.macos)),
            ("ios", one(&p.ios)),
            ("android", one(&p.android)),
        ])
    });
    Json::object([
        ("identifier", text(&file.identifier)),
        ("name", text(&file.name)),
        ("summary", text(&file.summary)),
        ("seeds", palette_json(&file.seeds)),
        ("palette", palette_json(&file.palette)),
        ("structure", file.structure.as_ref().map(structure_json)),
        ("platforms", platforms),
        (
            "extends",
            match &file.extends {
                CoreThemeFileBase::DefaultTheme => None,
                CoreThemeFileBase::Theme { identifier } => Some(Json::String(identifier.clone())),
                CoreThemeFileBase::Nothing => Some(Json::Null),
            },
        ),
        (
            "lockedAppearance",
            match &file.locked_appearance {
                CoreThemeFileLock::Inherit => None,
                CoreThemeFileLock::Unlocked => Some(Json::Null),
                CoreThemeFileLock::Locked { raw } => Some(Json::String(raw.clone())),
            },
        ),
    ])
}

/// Pretty-printed, keys sorted, so an export diffs cleanly. `None` when a
/// value is not a finite number.
pub fn encode(file: &CoreThemeFile) -> Option<String> {
    json::pretty(&file_json(file))
}
