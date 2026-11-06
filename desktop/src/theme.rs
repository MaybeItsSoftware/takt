//! Colours as role tokens that flip with the window's appearance, taken from
//! Zed's default themes, One Dark and One Light, so the app reads as Zed
//! does. Components only touch these roles.

use gpui::{Hsla, WindowAppearance, rgb, rgba};

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
    pub fn for_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::one_dark(),
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::one_light(),
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
