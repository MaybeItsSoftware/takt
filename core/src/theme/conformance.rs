//! The resolution cases in `shared/themes/conformance/` and the complete
//! built-in files beside them, written in the canonical form every client is
//! held to (`shared/themes/README.md`).

use super::builtins::{
    self, CHALK_DARK_IDENTIFIER, CHALK_IDENTIFIER, GRAPE_IDENTIFIER, IDENTIFIERS, ONE_IDENTIFIER,
};
use super::color::hex_string;
use super::export::file_from_specification;
use super::file::{self, CoreThemeFile, CoreThemeFileBase};
use super::json::{self, Json};
use super::loader::{CoreThemeFileSource, load};
use super::merge::CoreThemeFileOutcome;
use super::model::{
    CoreThemeAppearance, CoreThemeFontFace, CoreThemePlatform, CoreThemeSpecification,
    CoreThemeStructure, ROLES,
};

/// One input file of a case, as text: it may be invalid JSON on purpose.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreThemeNamedText {
    pub name: String,
    pub json: String,
}

/// A theme fully resolved for one platform and one requested appearance.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreThemeResolved {
    pub identifier: String,
    pub name: String,
    pub locked_appearance: Option<CoreThemeAppearance>,
    /// The appearance actually drawn: the lock, or the one asked for.
    pub appearance: CoreThemeAppearance,
    /// Every role, lowercase `#rrggbb` or `#rrggbbaa`, as drawn.
    pub colors: std::collections::HashMap<String, String>,
    pub structure: CoreThemeStructure,
}

pub fn resolved(
    specification: &CoreThemeSpecification,
    requested: CoreThemeAppearance,
) -> CoreThemeResolved {
    let drawn = specification.locked_appearance.unwrap_or(requested);
    CoreThemeResolved {
        identifier: specification.identifier.clone(),
        name: specification.name.clone(),
        locked_appearance: specification.locked_appearance,
        appearance: drawn,
        colors: ROLES
            .iter()
            .map(|role| {
                (
                    role.to_string(),
                    hex_string(&specification.palette.color(role, drawn)).to_lowercase(),
                )
            })
            .collect(),
        structure: specification.structure.clone(),
    }
}

fn face_json(face: &CoreThemeFontFace) -> Json {
    Json::object([
        (
            "families",
            Some(Json::Array(
                face.families.iter().cloned().map(Json::String).collect(),
            )),
        ),
        ("design", Some(Json::String(face.design.clone()))),
    ])
}

fn numbers(entries: &[(&str, f64)]) -> Json {
    Json::object(entries.iter().map(|(k, v)| (*k, Some(Json::Number(*v)))))
}

fn structure_json(s: &CoreThemeStructure) -> Json {
    let t = &s.typography;
    let m = &t.micro_label;
    Json::object([
        (
            "radius",
            Some(numbers(&[
                ("panel", s.radius.panel),
                ("row", s.radius.row),
                ("control", s.radius.control),
                ("pill", s.radius.pill),
                ("shell", s.radius.shell),
            ])),
        ),
        (
            "border",
            Some(numbers(&[
                ("hairline", s.border.hairline),
                ("emphasis", s.border.emphasis),
                ("focusRing", s.border.focus_ring),
            ])),
        ),
        (
            "spacing",
            Some(numbers(&[
                ("xxs", s.spacing.xxs),
                ("xs", s.spacing.xs),
                ("sm", s.spacing.sm),
                ("md", s.spacing.md),
                ("lg", s.spacing.lg),
                ("xl", s.spacing.xl),
            ])),
        ),
        (
            "typography",
            Some(Json::object([
                ("display", Some(face_json(&t.display))),
                ("body", Some(face_json(&t.body))),
                ("mono", Some(face_json(&t.mono))),
                ("bodySize", Some(Json::Number(t.body_size))),
                (
                    "scale",
                    Some(numbers(&[
                        ("caption", t.scale.caption),
                        ("body", t.scale.body),
                        ("title", t.scale.title),
                        ("display", t.scale.display),
                        ("hero", t.scale.hero),
                    ])),
                ),
                (
                    "microLabel",
                    Some(Json::object([
                        ("size", Some(Json::Number(m.size))),
                        ("weight", Some(Json::String(m.weight.clone()))),
                        ("tracking", Some(Json::Number(m.tracking))),
                        ("uppercase", Some(Json::Bool(m.is_uppercased))),
                        ("role", Some(Json::String(m.role.clone()))),
                    ])),
                ),
            ])),
        ),
        ("touchTarget", Some(Json::Number(s.touch_target))),
        ("usesShadows", Some(Json::Bool(s.uses_shadows))),
        (
            "usesGradientsOnChrome",
            Some(Json::Bool(s.uses_gradients_on_chrome)),
        ),
    ])
}

fn resolved_json(r: &CoreThemeResolved) -> Json {
    Json::object([
        ("identifier", Some(Json::String(r.identifier.clone()))),
        ("name", Some(Json::String(r.name.clone()))),
        // Always present, so `null` is stated rather than implied.
        (
            "lockedAppearance",
            Some(
                r.locked_appearance
                    .map(|a| Json::String(a.raw().to_string()))
                    .unwrap_or(Json::Null),
            ),
        ),
        (
            "appearance",
            Some(Json::String(r.appearance.raw().to_string())),
        ),
        (
            "colors",
            Some(Json::Object(
                r.colors
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::String(v.clone())))
                    .collect(),
            )),
        ),
        ("structure", Some(structure_json(&r.structure))),
    ])
}

/// What a device shows for `identifier`: the user theme of that identifier
/// if it loaded, else the built-in, else the default standing in.
pub fn selected(
    identifier: &str,
    outcomes: &[CoreThemeFileOutcome],
    platform: CoreThemePlatform,
) -> CoreThemeSpecification {
    outcomes
        .iter()
        .filter_map(|o| o.specification.as_ref())
        .find(|s| s.identifier == identifier)
        .or_else(|| builtins::specification(identifier, platform))
        .unwrap_or_else(|| builtins::default_theme(platform))
        .clone()
}

/// A whole case, encoded: the inputs, the theme picked, what every platform
/// shows in each appearance, and what loading reported without the audit.
/// Pretty-printed with sorted keys and a trailing newline.
pub fn case(files: &[CoreThemeNamedText], selected_identifier: &str) -> String {
    let sources: Vec<CoreThemeFileSource> = files
        .iter()
        .map(|f| CoreThemeFileSource {
            name: f.name.clone(),
            data: f.json.as_bytes().to_vec(),
        })
        .collect();
    let mut expected = Vec::new();
    let mut issues = Vec::new();
    for platform in CoreThemePlatform::ALL {
        let outcomes = load(&sources, platform, None, None);
        let theme = selected(selected_identifier, &outcomes, platform);
        expected.push((
            platform.raw().to_string(),
            Json::Object(
                CoreThemeAppearance::ALL
                    .iter()
                    .map(|a| (a.raw().to_string(), resolved_json(&resolved(&theme, *a))))
                    .collect(),
            ),
        ));
        // Only the audit differs by platform; the rest is kept once, as the Mac reports it.
        if platform == CoreThemePlatform::Macos {
            issues = outcomes
                .iter()
                .flat_map(|o| o.issues.iter())
                .filter(|i| !i.is_audit)
                .map(|i| {
                    Json::object([
                        ("source", Some(Json::String(i.source.clone()))),
                        (
                            "severity",
                            Some(Json::String(i.severity.name().to_string())),
                        ),
                        ("message", Some(Json::String(i.message.clone()))),
                    ])
                })
                .collect();
        }
    }
    let value = Json::object([
        (
            "files",
            Some(Json::Array(
                files
                    .iter()
                    .map(|f| {
                        Json::object([
                            ("name", Some(Json::String(f.name.clone()))),
                            ("json", Some(Json::String(f.json.clone()))),
                        ])
                    })
                    .collect(),
            )),
        ),
        (
            "selected",
            Some(Json::String(selected_identifier.to_string())),
        ),
        ("expected", Some(Json::Object(expected))),
        ("issues", Some(Json::Array(issues))),
    ]);
    json::pretty(&value).unwrap_or_default() + "\n"
}

/// A built-in as the complete file `shared/themes/` holds: every value, the
/// Mac's structure as `structure`, the phones' differences under `platforms`,
/// extending nothing.
pub fn shared_file(identifier: &str) -> Option<CoreThemeFile> {
    let mac = builtins::specification(identifier, CoreThemePlatform::Macos)?;
    let variants: Vec<(CoreThemePlatform, &CoreThemeSpecification)> = CoreThemePlatform::ALL
        .iter()
        .filter_map(|p| builtins::specification(identifier, *p).map(|s| (*p, s)))
        .collect();
    let mut file = file_from_specification(mac, &variants);
    file.extends = CoreThemeFileBase::Nothing;
    Some(file)
}

/// The stem a built-in's shared file goes under; Zed keeps its old name.
pub fn shared_file_name(identifier: &str) -> String {
    match identifier {
        CHALK_IDENTIFIER => "chalk",
        CHALK_DARK_IDENTIFIER => "chalk-dark",
        GRAPE_IDENTIFIER => "grape",
        ONE_IDENTIFIER => "one",
        _ => "priority",
    }
    .to_string()
}

/// `<name>.json` → the text it should hold, for each built-in.
pub fn shared_files() -> Vec<CoreThemeNamedText> {
    IDENTIFIERS
        .iter()
        .filter_map(|identifier| {
            let file = shared_file(identifier)?;
            Some(CoreThemeNamedText {
                name: format!("{}.json", shared_file_name(identifier)),
                json: file::encode(&file)? + "\n",
            })
        })
        .collect()
}
