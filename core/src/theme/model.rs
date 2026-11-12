//! The resolved theme: what a file becomes once it has been merged over its
//! base. Every value is stated; the enumerated ones (a font's design and
//! weight, a micro-label's role) are carried as their raw names, which the
//! merge only ever sets to a name it recognised.

use std::collections::HashMap;

/// Which of a palette's two tables is in force.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum CoreThemeAppearance {
    Light,
    Dark,
}

impl CoreThemeAppearance {
    pub const ALL: [CoreThemeAppearance; 2] =
        [CoreThemeAppearance::Light, CoreThemeAppearance::Dark];

    pub fn raw(self) -> &'static str {
        match self {
            CoreThemeAppearance::Light => "light",
            CoreThemeAppearance::Dark => "dark",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "light" => Some(CoreThemeAppearance::Light),
            "dark" => Some(CoreThemeAppearance::Dark),
            _ => None,
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            CoreThemeAppearance::Light => CoreThemeAppearance::Dark,
            CoreThemeAppearance::Dark => CoreThemeAppearance::Light,
        }
    }
}

/// The app a theme is resolved for. The palette is the same on all three;
/// the structure is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum CoreThemePlatform {
    Macos,
    Ios,
    Android,
}

impl CoreThemePlatform {
    pub const ALL: [CoreThemePlatform; 3] = [
        CoreThemePlatform::Macos,
        CoreThemePlatform::Ios,
        CoreThemePlatform::Android,
    ];

    pub fn raw(self) -> &'static str {
        match self {
            CoreThemePlatform::Macos => "macos",
            CoreThemePlatform::Ios => "ios",
            CoreThemePlatform::Android => "android",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|platform| platform.raw() == raw)
    }

    pub(crate) fn index(self) -> usize {
        match self {
            CoreThemePlatform::Macos => 0,
            CoreThemePlatform::Ios => 1,
            CoreThemePlatform::Android => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum CoreThemeIssueSeverity {
    /// Something will render wrong.
    Error,
    /// It renders, but a rule of the house style is being bent.
    Warning,
    /// True, worth knowing, and intended.
    Note,
}

impl CoreThemeIssueSeverity {
    pub fn rank(self) -> u8 {
        match self {
            CoreThemeIssueSeverity::Error => 2,
            CoreThemeIssueSeverity::Warning => 1,
            CoreThemeIssueSeverity::Note => 0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            CoreThemeIssueSeverity::Error => "error",
            CoreThemeIssueSeverity::Warning => "warning",
            CoreThemeIssueSeverity::Note => "note",
        }
    }
}

/// Every colour role, in the order the format declares them. A view names a
/// role, never a hex value.
pub const ROLES: [&str; 21] = [
    "paper",
    "raised",
    "altRow",
    "hover",
    "well",
    "border",
    "borderMuted",
    "inputBorder",
    "ink",
    "mutedText",
    "dimText",
    "primary",
    "success",
    "danger",
    "warning",
    "categoricalPurple",
    "categoricalPink",
    "categoricalOrange",
    "mediaLetterbox",
    "mediaScrim",
    "mediaScrimInk",
];

/// Zed's chrome, which a theme may state and need not: the selected row,
/// the hairlines under a tree's levels, the status bar, the outline round the
/// keyboard's row and an icon at rest. A role left out is derived from the
/// others ([`CoreThemePalette::chrome`]), as Zed draws a theme that names none.
pub const CHROME_ROLES: [&str; 5] = ["selected", "indentGuide", "statusBar", "focus", "icon"];

/// Roles that do not flip with the appearance: media surfaces, whose content
/// is not ours to theme. They always answer from the light table.
pub const INVARIANT_ROLES: [&str; 3] = ["mediaLetterbox", "mediaScrim", "mediaScrimInk"];

/// Roles running text may be set in, held to 4.5:1 on the paper.
pub const BODY_TEXT_ROLES: [&str; 2] = ["ink", "mutedText"];

pub const DESIGNS: [&str; 4] = ["serif", "sans", "monospaced", "rounded"];
pub const WEIGHTS: [&str; 5] = ["regular", "medium", "semibold", "bold", "black"];

pub fn is_role(raw: &str) -> bool {
    ROLES.contains(&raw) || CHROME_ROLES.contains(&raw)
}

pub fn is_invariant(role: &str) -> bool {
    INVARIANT_ROLES.contains(&role)
}

/// A colour: four channels in 0…1.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct CoreThemeColor {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

pub type ColorTable = HashMap<String, CoreThemeColor>;

/// Two tables of literal colour, one per appearance.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreThemePalette {
    pub light: ColorTable,
    pub dark: ColorTable,
}

impl CoreThemePalette {
    pub fn table(&self, appearance: CoreThemeAppearance) -> &ColorTable {
        match appearance {
            CoreThemeAppearance::Light => &self.light,
            CoreThemeAppearance::Dark => &self.dark,
        }
    }

    /// Total, in this order: an invariant role from the light table; else the
    /// appearance's own table; else the other one; else debug magenta.
    pub fn color(&self, role: &str, appearance: CoreThemeAppearance) -> CoreThemeColor {
        if is_invariant(role) {
            return self
                .light
                .get(role)
                .or_else(|| self.dark.get(role))
                .copied()
                .unwrap_or(super::color::UNRESOLVED);
        }
        self.table(appearance)
            .get(role)
            .or_else(|| self.table(appearance.opposite()).get(role))
            .copied()
            .unwrap_or(super::color::UNRESOLVED)
    }

    /// A role as the theme states it for `appearance`, if it does: its own
    /// table, else the other.
    pub fn stated(&self, role: &str, appearance: CoreThemeAppearance) -> Option<CoreThemeColor> {
        self.table(appearance)
            .get(role)
            .or_else(|| self.table(appearance.opposite()).get(role))
            .copied()
    }

    /// One of the [`CHROME_ROLES`], opaque: as stated, else derived as Zed
    /// draws a theme that names none. A selection is a sixth of the accent in
    /// the page; a guide a seventh of the text in the panel; the status bar
    /// the well; the keyboard's outline the accent; an icon the muted text.
    pub fn chrome(&self, role: &str, appearance: CoreThemeAppearance) -> CoreThemeColor {
        use super::color::{mix, over};
        let at = |r: &str| self.color(r, appearance);
        let panel = at("altRow");
        if let Some(stated) = self.stated(role, appearance) {
            let below = match role {
                "indentGuide" | "statusBar" => panel,
                _ => at("paper"),
            };
            return over(stated, below);
        }
        match role {
            "selected" => mix(&at("paper"), &at("primary"), 0.16),
            "indentGuide" => mix(&panel, &at("ink"), 0.14),
            "statusBar" => at("well"),
            "focus" => at("primary"),
            "icon" => at("mutedText"),
            _ => self.color(role, appearance),
        }
    }

    /// Roles absent from `appearance`'s own table, by name. Invariant roles
    /// are only expected in the light one.
    pub fn missing_roles(&self, appearance: CoreThemeAppearance) -> Vec<&'static str> {
        let mut missing: Vec<&'static str> = ROLES
            .into_iter()
            .filter(|role| {
                if is_invariant(role) {
                    appearance == CoreThemeAppearance::Light && !self.light.contains_key(*role)
                } else {
                    !self.table(appearance).contains_key(*role)
                }
            })
            .collect();
        missing.sort_unstable();
        missing
    }
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct CoreThemeRadius {
    pub panel: f64,
    pub row: f64,
    pub control: f64,
    pub pill: f64,
    pub shell: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct CoreThemeBorder {
    pub hairline: f64,
    pub emphasis: f64,
    pub focus_ring: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct CoreThemeSpacing {
    pub xxs: f64,
    pub xs: f64,
    pub sm: f64,
    pub md: f64,
    pub lg: f64,
    pub xl: f64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreThemeFontFace {
    /// Most-wanted first. Empty means "use the design".
    pub families: Vec<String>,
    /// `serif`, `sans`, `monospaced` or `rounded`.
    pub design: String,
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct CoreThemeTypeScale {
    pub caption: f64,
    pub body: f64,
    pub title: f64,
    pub display: f64,
    pub hero: f64,
}

impl CoreThemeTypeScale {
    /// A scale proportioned from a body size, for a theme that names only that.
    pub fn proportioned(body: f64) -> Self {
        CoreThemeTypeScale {
            caption: (body * 0.85).round(),
            body,
            title: (body * 1.25).round(),
            display: (body * 2.2).round(),
            hero: (body * 5.0).round(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreThemeMicroLabel {
    pub size: f64,
    /// `regular`, `medium`, `semibold`, `bold` or `black`.
    pub weight: String,
    /// In em.
    pub tracking: f64,
    pub is_uppercased: bool,
    /// A colour role.
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreThemeTypography {
    pub display: CoreThemeFontFace,
    pub body: CoreThemeFontFace,
    pub mono: CoreThemeFontFace,
    pub body_size: f64,
    pub scale: CoreThemeTypeScale,
    pub micro_label: CoreThemeMicroLabel,
}

/// Everything about a theme that is not colour.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreThemeStructure {
    pub radius: CoreThemeRadius,
    pub border: CoreThemeBorder,
    pub spacing: CoreThemeSpacing,
    pub typography: CoreThemeTypography,
    /// The minimum hit area, in points; 0 for a pointer platform.
    pub touch_target: f64,
    pub uses_shadows: bool,
    pub uses_gradients_on_chrome: bool,
}

/// A whole theme.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreThemeSpecification {
    pub identifier: String,
    pub name: String,
    pub summary: String,
    /// A theme that exists in one appearance only.
    pub locked_appearance: Option<CoreThemeAppearance>,
    pub palette: CoreThemePalette,
    pub structure: CoreThemeStructure,
}
