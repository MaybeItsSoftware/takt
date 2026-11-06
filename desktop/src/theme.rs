//! Colours as role tokens that flip with the window's appearance, taken from
//! Zed's default themes, One Dark and One Light, so the app reads as Zed
//! does. Components only touch these roles.

use crate::state::library::Theme;
use gpui::{Hsla, Rgba, WindowAppearance, rgb, rgba};
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
        match theme {
            Theme::Zed => {
                if dark {
                    Self::one_dark()
                } else {
                    Self::one_light()
                }
            }
            Theme::Spec(spec) => {
                let appearance = match spec.locked_appearance {
                    Some(locked) => locked,
                    None if dark => CoreThemeAppearance::Dark,
                    None => CoreThemeAppearance::Light,
                };
                Self::from_spec(spec, appearance)
            }
        }
    }

    fn from_spec(spec: &CoreThemeSpecification, appearance: CoreThemeAppearance) -> Self {
        let role = |name: &str| colour(spec.palette.color(name, appearance));
        let paper = spec.palette.color("paper", appearance);
        let primary = spec.palette.color("primary", appearance);
        let selected = takt_core::theme::color::mix(&paper, &primary, 0.16);
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
            selected: colour(selected),
            accent: role("primary"),
            error: role("danger"),
            success: role("success"),
            warning: role("warning"),
        }
    }

    fn one_dark() -> Self {
        Tokens {
            background: rgb(0x282c33).into(),
            surface: rgb(0x2f343e).into(),
            chrome: rgb(0x3b414d).into(),
            border: rgb(0x464b57).into(),
            border_variant: rgb(0x363c46).into(),
            text: rgb(0xdce0e5).into(),
            muted: rgb(0xa9afbc).into(),
            placeholder: rgb(0x878a98).into(),
            hover: rgba(0x363c46ff).into(),
            selected: rgb(0x454a56).into(),
            accent: rgb(0x74ade8).into(),
            error: rgb(0xd07277).into(),
            success: rgb(0xa1c181).into(),
            warning: rgb(0xdec184).into(),
        }
    }

    fn one_light() -> Self {
        Tokens {
            background: rgb(0xfafafa).into(),
            surface: rgb(0xebebec).into(),
            chrome: rgb(0xdcdcdd).into(),
            border: rgb(0xc9c9ca).into(),
            border_variant: rgb(0xdfdfe0).into(),
            text: rgb(0x242529).into(),
            muted: rgb(0x58585a).into(),
            placeholder: rgb(0x7e8086).into(),
            hover: rgba(0xdfdfe0ff).into(),
            selected: rgb(0xcacaca).into(),
            accent: rgb(0x5c78e2).into(),
            error: rgb(0xd36151).into(),
            success: rgb(0x669f59).into(),
            warning: rgb(0xa48819).into(),
        }
    }
}
