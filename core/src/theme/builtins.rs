//! The themes that ship with every app: Takt (the default, stored as
//! Priority), Zed and Zed Dark (stored as Chalk) and Grape. Defined once,
//! here; `shared/themes/` holds them as complete files.

use std::sync::OnceLock;

use super::color::{from_hex, rgba};
use super::file::{CoreThemeFileMicroLabel, CoreThemeFileTypography};
use super::file::{CoreThemeFileStructure, CoreThemeFileTypeScale};
use super::merge::merge_structure;
use super::model::{
    ColorTable, CoreThemeAppearance, CoreThemeBorder, CoreThemeColor, CoreThemeFontFace,
    CoreThemeMicroLabel, CoreThemePalette, CoreThemePlatform, CoreThemeRadius, CoreThemeSpacing,
    CoreThemeSpecification, CoreThemeStructure, CoreThemeTypeScale, CoreThemeTypography,
};
use super::seeds::CoreThemeSeeds;

/// Takt, in the house colours: what a theme file extends unless it says
/// otherwise.
pub const PRIORITY_IDENTIFIER: &str = "native.theme.priority";
/// The Zed look, under its old name.
pub const CHALK_IDENTIFIER: &str = "native.theme.chalk";
pub const CHALK_DARK_IDENTIFIER: &str = "native.theme.chalk.dark";
/// The house design language, whole.
pub const GRAPE_IDENTIFIER: &str = "native.theme.grape";
/// Zed's own default pair, One Light and One Dark: what the Rust Mac app draws.
pub const ONE_IDENTIFIER: &str = "native.theme.one";
/// The default on every platform: what a fresh install shows and what stands
/// in for a theme that will not load. Zed One, as the Mac app draws it.
pub const DEFAULT_IDENTIFIER: &str = ONE_IDENTIFIER;
/// What a theme file extends when it names nothing: Takt, so a file of three
/// seeds keeps the house neutrals it was written against.
pub const EXTENDS_IDENTIFIER: &str = PRIORITY_IDENTIFIER;

/// The built-ins, the base a file extends first.
pub const IDENTIFIERS: [&str; 5] = [
    PRIORITY_IDENTIFIER,
    CHALK_IDENTIFIER,
    CHALK_DARK_IDENTIFIER,
    GRAPE_IDENTIFIER,
    ONE_IDENTIFIER,
];

fn hex(value: &str) -> CoreThemeColor {
    from_hex(value).unwrap_or(super::color::UNRESOLVED)
}

const AZURE: &str = "#007fff";
const EMERALD: &str = "#4cc38e";
const RASPBERRY: &str = "#d62246";
const AMBER: &str = "#ffbf00";
const PURPLE: &str = "#7a4de8";
const PINK: &str = "#ff88dc";
const ORANGE: &str = "#ff6b2b";

fn table(entries: &[(&str, &str)]) -> ColorTable {
    entries
        .iter()
        .map(|(role, value)| (role.to_string(), hex(value)))
        .collect()
}

fn scrim() -> CoreThemeColor {
    let black = hex("#000000");
    rgba(black.red, black.green, black.blue, 0.7)
}

/// Warm paper and grape ink, separated by hairlines. The neutrals run warm
/// at the paper end and cool at the ink end, the way real paper does.
fn chalk_palette() -> CoreThemePalette {
    let mut light = table(&[
        ("paper", "#faf8f4"),
        ("raised", "#ffffff"),
        ("altRow", "#f5f3f1"),
        ("hover", "#f1eff1"),
        ("well", "#edebef"),
        ("border", "#e6e4ea"),
        ("borderMuted", "#efedf2"),
        ("inputBorder", "#d8d5dd"),
        ("ink", "#444054"),
        ("mutedText", "#6e6b7c"),
        ("dimText", "#b6b3bf"),
        ("primary", AZURE),
        ("success", EMERALD),
        ("danger", RASPBERRY),
        ("warning", AMBER),
        ("categoricalPurple", PURPLE),
        ("categoricalPink", PINK),
        ("categoricalOrange", ORANGE),
        ("mediaLetterbox", "#000000"),
        ("mediaScrimInk", "#ffffff"),
    ]);
    light.insert("mediaScrim".into(), scrim());
    // The same grape hue pulled down, never neutral grey; accents keep their hex.
    let dark = table(&[
        ("paper", "#1c1a23"),
        ("raised", "#25232f"),
        ("altRow", "#211f29"),
        ("hover", "#2d2b38"),
        ("well", "#2d2b38"),
        ("border", "#34313f"),
        ("borderMuted", "#2d2b38"),
        ("inputBorder", "#403d4d"),
        ("ink", "#f5f4f7"),
        ("mutedText", "#b6b3bf"),
        ("dimText", "#6e6b7c"),
        ("primary", AZURE),
        ("success", EMERALD),
        ("danger", RASPBERRY),
        ("warning", AMBER),
        ("categoricalPurple", PURPLE),
        ("categoricalPink", PINK),
        ("categoricalOrange", ORANGE),
    ]);
    CoreThemePalette { light, dark }
}

fn face(family: &str, design: &str) -> CoreThemeFontFace {
    CoreThemeFontFace {
        families: vec![family.to_string()],
        design: design.to_string(),
    }
}

fn micro_label(size: f64, weight: &str, tracking: f64, is_uppercased: bool) -> CoreThemeMicroLabel {
    CoreThemeMicroLabel {
        size,
        weight: weight.to_string(),
        tracking,
        is_uppercased,
        role: "mutedText".to_string(),
    }
}

const EDITOR_SPACING: CoreThemeSpacing = CoreThemeSpacing {
    xxs: 2.0,
    xs: 4.0,
    sm: 8.0,
    md: 12.0,
    lg: 16.0,
    xl: 24.0,
};

const HAIRLINES: CoreThemeBorder = CoreThemeBorder {
    hairline: 1.0,
    emphasis: 2.0,
    focus_ring: 2.0,
};

/// Zed: IBM Plex Sans and Lilex, square panels, only what you press keeps a corner.
fn chalk() -> CoreThemeSpecification {
    CoreThemeSpecification {
        identifier: CHALK_IDENTIFIER.into(),
        name: "Zed".into(),
        summary: "The Zed look: IBM Plex Sans and Lilex, square panels, hairlines, warm paper and grape ink."
            .into(),
        locked_appearance: None,
        palette: chalk_palette(),
        structure: CoreThemeStructure {
            radius: CoreThemeRadius {
                panel: 0.0,
                row: 0.0,
                control: 4.0,
                pill: 9999.0,
                shell: 20.0,
            },
            border: HAIRLINES,
            spacing: EDITOR_SPACING,
            typography: CoreThemeTypography {
                display: face("IBM Plex Sans", "sans"),
                body: face("IBM Plex Sans", "sans"),
                mono: face("Lilex", "monospaced"),
                body_size: 13.0,
                scale: CoreThemeTypeScale {
                    caption: 12.0,
                    body: 13.0,
                    title: 15.0,
                    display: 28.0,
                    hero: 64.0,
                },
                micro_label: micro_label(12.0, "regular", 0.0, false),
            },
            touch_target: 0.0,
            uses_shadows: false,
            uses_gradients_on_chrome: false,
        },
    }
}

/// Zed's One Light and One Dark, role for role as the Rust Mac app draws
/// them (`desktop/src/theme.rs`): the editor's background as the page, the
/// panels' as the raised surface, the status colours as Zed's.
fn one_palette() -> CoreThemePalette {
    let mut light = table(&[
        ("paper", "#fafafa"),
        ("raised", "#ebebec"),
        ("altRow", "#ebebec"),
        ("hover", "#dfdfe0"),
        ("well", "#dcdcdd"),
        ("border", "#c9c9ca"),
        ("borderMuted", "#dfdfe0"),
        ("inputBorder", "#c9c9ca"),
        ("ink", "#242529"),
        ("mutedText", "#58585a"),
        ("dimText", "#7e8086"),
        ("primary", "#5c78e2"),
        ("selected", "#cacaca"),
        ("indentGuide", "#dfdfe0"),
        ("success", "#669f59"),
        ("danger", "#d36151"),
        ("warning", "#a48819"),
        ("categoricalPurple", PURPLE),
        ("categoricalPink", PINK),
        ("categoricalOrange", ORANGE),
        ("mediaLetterbox", "#000000"),
        ("mediaScrimInk", "#ffffff"),
    ]);
    light.insert("mediaScrim".into(), scrim());
    let dark = table(&[
        ("paper", "#282c33"),
        ("raised", "#2f343e"),
        ("altRow", "#2f343e"),
        ("hover", "#363c46"),
        ("well", "#3b414d"),
        ("border", "#464b57"),
        ("borderMuted", "#363c46"),
        ("inputBorder", "#464b57"),
        ("ink", "#dce0e5"),
        ("mutedText", "#a9afbc"),
        ("dimText", "#878a98"),
        ("primary", "#74ade8"),
        ("selected", "#454a56"),
        ("indentGuide", "#363c46"),
        ("success", "#a1c181"),
        ("danger", "#d07277"),
        ("warning", "#dec184"),
        ("categoricalPurple", PURPLE),
        ("categoricalPink", PINK),
        ("categoricalOrange", ORANGE),
    ]);
    CoreThemePalette { light, dark }
}

/// Zed One: the Zed look in Zed's own colours, the Rust Mac app's theme,
/// so the phones can wear it too.
fn one() -> CoreThemeSpecification {
    CoreThemeSpecification {
        identifier: ONE_IDENTIFIER.into(),
        name: "Zed One".into(),
        summary: "Zed's own One Light and One Dark: IBM Plex Sans and Lilex, square panels, hairlines, as the Mac app draws them."
            .into(),
        palette: one_palette(),
        ..chalk()
    }
}

/// Zed with the lights off: the same palette, the appearance fixed to dark.
fn chalk_dark() -> CoreThemeSpecification {
    let chalk = chalk();
    CoreThemeSpecification {
        identifier: CHALK_DARK_IDENTIFIER.into(),
        name: "Zed Dark".into(),
        summary:
            "The Zed look with the lights off. The same grape hue pulled down, never neutral grey."
                .into(),
        locked_appearance: Some(CoreThemeAppearance::Dark),
        ..chalk
    }
}

/// A table grown from `seeds`, with `overrides` named over it, plus the
/// roles seeds do not reach: the categorical hues and, in the light, the
/// media surfaces.
fn seeded(
    seeds: &CoreThemeSeeds,
    appearance: CoreThemeAppearance,
    overrides: &ColorTable,
) -> ColorTable {
    let mut table = seeds.roles(appearance).unwrap_or_default();
    table.extend(overrides.iter().map(|(k, v)| (k.clone(), *v)));
    table.insert("categoricalPurple".into(), hex(PURPLE));
    table.insert("categoricalPink".into(), hex(PINK));
    table.insert("categoricalOrange".into(), hex(ORANGE));
    if appearance == CoreThemeAppearance::Light {
        table.insert("mediaLetterbox".into(), hex("#000000"));
        table.insert("mediaScrim".into(), scrim());
        table.insert("mediaScrimInk".into(), hex("#ffffff"));
    }
    table
}

/// Takt, the default: the Mac app's structure (IBM Plex Sans and Lilex,
/// square panels, hairlines, Zed's spacing) in the house colours. Grown from
/// seeds like a theme file, with the house palette's hand-named neutrals over
/// them, so a phone left on the default is set as the Mac is.
fn priority() -> CoreThemeSpecification {
    let house = chalk_palette();
    let seeds = |background: &str, foreground: &str| CoreThemeSeeds {
        background: Some(hex(background)),
        foreground: Some(hex(foreground)),
        accent: Some(hex(AZURE)),
        success: Some(hex(EMERALD)),
        danger: Some(hex(RASPBERRY)),
        warning: Some(hex(AMBER)),
    };
    CoreThemeSpecification {
        identifier: PRIORITY_IDENTIFIER.into(),
        name: "Takt".into(),
        summary: "The default. Set as the Mac app is, in IBM Plex Sans with square panels and hairlines, in the house colours: chalk paper, grape ink and an azure accent."
            .into(),
        locked_appearance: None,
        palette: CoreThemePalette {
            light: seeded(&seeds("#faf8f4", "#444054"), CoreThemeAppearance::Light, &house.light),
            dark: seeded(&seeds("#1c1a23", "#f5f4f7"), CoreThemeAppearance::Dark, &house.dark),
        },
        structure: chalk().structure,
    }
}

/// Grape: Zed's palette set as the house style states it — Arvo, Geist Mono,
/// 8 and 6 radii, and 10pt bold capitals tracked 0.15em for every label.
fn grape() -> CoreThemeSpecification {
    CoreThemeSpecification {
        identifier: GRAPE_IDENTIFIER.into(),
        name: "Grape".into(),
        summary: "The house style: warm paper, grape ink, Arvo and Geist Mono, hairlines and small capitals.".into(),
        locked_appearance: None,
        palette: chalk_palette(),
        structure: CoreThemeStructure {
            radius: CoreThemeRadius {
                panel: 8.0,
                row: 6.0,
                control: 6.0,
                pill: 9999.0,
                shell: 20.0,
            },
            border: HAIRLINES,
            spacing: EDITOR_SPACING,
            typography: CoreThemeTypography {
                // The fallback is a serif too, so a failed Arvo still reads as Grape.
                display: face("Arvo", "serif"),
                body: face("Arvo", "serif"),
                mono: face("Geist Mono", "monospaced"),
                body_size: 13.0,
                scale: CoreThemeTypeScale {
                    caption: 11.0,
                    body: 13.0,
                    title: 16.0,
                    display: 28.0,
                    hero: 64.0,
                },
                micro_label: micro_label(10.0, "bold", 0.15, true),
            },
            touch_target: 0.0,
            uses_shadows: false,
            uses_gradients_on_chrome: false,
        },
    }
}

// MARK: - Per platform

/// The phones' type: the Mac's scale in its proportions, set at 15 rather
/// than 13 so it reads in the hand (caption 12→13 is kept at the Mac's
/// caption-to-body step, the title 15→17, the micro-label the caption's).
fn phone_type(micro_label: bool) -> Option<CoreThemeFileTypography> {
    let (body, caption, title, display) = (15.0, 13.0, 17.0, 32.0);
    Some(CoreThemeFileTypography {
        body_size: Some(body),
        scale: Some(CoreThemeFileTypeScale {
            caption: Some(caption),
            body: Some(body),
            title: Some(title),
            display: Some(display),
            hero: Some(72.0),
        }),
        micro_label: micro_label.then(|| CoreThemeFileMicroLabel {
            size: Some(caption),
            ..Default::default()
        }),
        ..Default::default()
    })
}

fn touch_target(platform: CoreThemePlatform) -> Option<f64> {
    Some(if platform == CoreThemePlatform::Ios {
        44.0
    } else {
        48.0
    })
}

/// What a built-in lays over its own structure on `platform`. The Mac has
/// none: each built-in's structure already is the Mac's.
pub fn platform_structure(
    identifier: &str,
    platform: CoreThemePlatform,
) -> Option<CoreThemeFileStructure> {
    if platform == CoreThemePlatform::Macos {
        return None;
    }
    match identifier {
        // The Mac's own look in the hand: its square panels, 4pt controls and
        // spacing kept, the type at the Mac's proportions set a step larger,
        // and hit areas grown to the platform's minimum (invisibly: a control
        // keeps its painted size).
        PRIORITY_IDENTIFIER | CHALK_IDENTIFIER | CHALK_DARK_IDENTIFIER | ONE_IDENTIFIER => {
            Some(CoreThemeFileStructure {
                typography: phone_type(true),
                touch_target: touch_target(platform),
                ..Default::default()
            })
        }
        // The Mac's radius scale; the micro-label stays 10pt.
        GRAPE_IDENTIFIER => Some(CoreThemeFileStructure {
            typography: phone_type(false),
            touch_target: touch_target(platform),
            ..Default::default()
        }),
        _ => None,
    }
}

fn for_platform(
    specification: CoreThemeSpecification,
    platform: CoreThemePlatform,
) -> CoreThemeSpecification {
    let overlay = platform_structure(&specification.identifier, platform);
    let structure = merge_structure(
        overlay.as_ref(),
        &specification.structure,
        "structure",
        &mut |_, _| {},
    );
    CoreThemeSpecification {
        structure,
        ..specification
    }
}

/// The built-ins as `platform` resolves them, the base a file extends first.
pub fn all(platform: CoreThemePlatform) -> &'static [CoreThemeSpecification] {
    static CACHE: OnceLock<[Vec<CoreThemeSpecification>; 3]> = OnceLock::new();
    let cache = CACHE.get_or_init(|| {
        CoreThemePlatform::ALL.map(|platform| {
            [priority(), chalk(), chalk_dark(), grape(), one()]
                .into_iter()
                .map(|specification| for_platform(specification, platform))
                .collect()
        })
    });
    &cache[platform.index()]
}

/// The default, Zed One, as `platform` resolves it.
pub fn default_theme(platform: CoreThemePlatform) -> &'static CoreThemeSpecification {
    specification(DEFAULT_IDENTIFIER, platform).unwrap_or(&all(platform)[0])
}

/// What a file that names no base extends: Takt, as `platform` resolves it.
pub fn extends_base(platform: CoreThemePlatform) -> &'static CoreThemeSpecification {
    &all(platform)[0]
}

pub fn specification(
    identifier: &str,
    platform: CoreThemePlatform,
) -> Option<&'static CoreThemeSpecification> {
    all(platform).iter().find(|s| s.identifier == identifier)
}
