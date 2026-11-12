//! Colours as role tokens that flip with the window's appearance, taken from
//! the theme in force — by default Zed One, Zed's own One Dark and One Light,
//! so the app reads as Zed does. Components only touch these roles; the
//! phones read the same roles from the same core.

use crate::state::library::Theme;
use gpui::{Hsla, Rgba, WindowAppearance};
use takt_core::theme::model::{CoreThemeAppearance, CoreThemeColor, CoreThemeSpecification};

fn colour(c: CoreThemeColor) -> Hsla {
    Rgba {
        r: c.red as f32,
        g: c.green as f32,
        b: c.blue as f32,
        a: c.alpha as f32,
    }
    .into()
}

#[derive(Clone, Copy)]
pub struct Tokens {
    /// The pane a list is drawn on (Zed's `editor.background`).
    pub background: Hsla,
    /// Docks, the title bar and the status bar (Zed's `panel.background`,
    /// `title_bar.background`).
    pub surface: Hsla,
    pub chrome: Hsla,
    pub border: Hsla,
    pub border_variant: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub placeholder: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
    pub accent: Hsla,
    /// Zed's status colours: `error`, `success`, `warning`.
    pub error: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
    /// The status bar along the foot (Zed's `status_bar.background`).
    pub status_bar: Hsla,
    /// The hairlines under a tree's levels (Zed's `panel.indent_guide`).
    pub indent_guide: Hsla,
    /// The outline round the row the keyboard is on (Zed's
    /// `panel.focused_border`); the accent where a theme names none.
    pub focus: Hsla,
    /// Words and resting icons on the status bar: the text colour where the
    /// theme paints the bar, as Zed does, else muted.
    pub status_text: Hsla,
    pub status_icon: Hsla,
}

impl Tokens {
    /// The tokens of the theme in force, in the appearance asked for
    /// (`system` follows the window's).
    pub fn of(theme: &Theme, appearance: WindowAppearance, wanted: &str) -> Self {
        let dark = match wanted {
            "light" => false,
            "dark" => true,
            _ => matches!(
                appearance,
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ),
        };
        let spec = theme.spec();
        let appearance = match spec.locked_appearance {
            Some(locked) => locked,
            None if dark => CoreThemeAppearance::Dark,
            None => CoreThemeAppearance::Light,
        };
        Self::from_spec(spec, appearance)
    }

    fn from_spec(spec: &CoreThemeSpecification, appearance: CoreThemeAppearance) -> Self {
        let palette = &spec.palette;
        let role = |name: &str| colour(palette.color(name, appearance));
        let chrome = |name: &str| colour(palette.chrome(name, appearance));
        // A theme that paints its status bar sets its words in its text
        // colour and its icons in its icon colour, as Zed does; else both
        // are muted.
        let painted = palette.stated("statusBar", appearance).is_some();
        let status_text = if painted {
            role("ink")
        } else {
            role("mutedText")
        };
        let status_icon = if painted { chrome("icon") } else { status_text };
        Tokens {
            background: role("paper"),
            surface: role("altRow"),
            chrome: role("well"),
            border: role("border"),
            border_variant: role("borderMuted"),
            text: role("ink"),
            muted: role("mutedText"),
            placeholder: role("dimText"),
            hover: role("hover"),
            selected: chrome("selected"),
            accent: role("primary"),
            error: role("danger"),
            success: role("success"),
            warning: role("warning"),
            status_bar: chrome("statusBar"),
            indent_guide: chrome("indentGuide"),
            focus: chrome("focus"),
            status_text,
            status_icon,
        }
    }
}
