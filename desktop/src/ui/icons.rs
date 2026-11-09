//! The app's icons: line drawings in the style of Lucide, the set Zed draws
//! its chrome with, served to GPUI's `svg()` from memory as
//! `icons/<name>.svg`. GPUI paints an SVG as a mask in the element's text
//! colour, so each is drawn in one opaque stroke.

use gpui::{SharedString, Styled, px, svg};
use std::borrow::Cow;

pub struct Assets;

impl gpui::AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let name = path
            .strip_prefix("icons/")
            .and_then(|p| p.strip_suffix(".svg"));
        Ok(name.and_then(body).map(|body| {
            Cow::Owned(
                format!(
                    r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">{body}</svg>"#
                )
                .into_bytes(),
            )
        }))
    }

    fn list(&self, _path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(NAMES
            .iter()
            .map(|name| SharedString::from(format!("icons/{name}.svg")))
            .collect())
    }
}

const NAMES: &[&str] = &[
    "chevron-left",
    "chevron-right",
    "chevron-down",
    "cloud",
    "server",
    "mail",
    "panel-left",
    "sparkles",
    "search",
    "folder",
    "folder-open",
    "list",
    "inbox",
    "sun",
    "infinity",
    "filter",
    "corner-down-right",
    "info",
    "sliders",
    "chart",
    "circle-check",
    "clock",
    "check",
    "panel-right",
    "panel-bottom",
    "settings",
    "palette",
    "keyboard",
    "timer",
    "bell",
    "party",
    "refresh",
    "loader",
    "calendar",
    "calendar-clock",
    "square-check",
    "link",
    "plug",
    "wrench",
    "chevrons-up-down",
    "x",
    "play",
    "sunrise",
    "hourglass",
    "text",
    "flag",
    "tag",
    "ban",
    "user",
    "users",
    "repeat",
    "flame",
    "target",
    "copy",
    "pin",
    "archive",
    "arrow-left-right",
    "armchair",
    "file-text",
    "file",
    "sheet",
    "presentation",
    "form",
    "image",
    "gem",
    "globe",
];

fn body(name: &str) -> Option<&'static str> {
    Some(match name {
        "panel-left" => r#"<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M9 3v18"/>"#,
        "panel-right" => r#"<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M15 3v18"/>"#,
        "panel-bottom" => {
            r#"<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 15h18"/>"#
        }
        "sparkles" => {
            r#"<path d="M11 3l1.7 4.6L17.3 9.3l-4.6 1.7L11 15.6 9.3 11 4.7 9.3l4.6-1.7z"/><path d="M19 14v5"/><path d="M21.5 16.5h-5"/>"#
        }
        "search" => r#"<circle cx="11" cy="11" r="7.5"/><path d="m20.5 20.5-4.2-4.2"/>"#,
        "check" => r#"<path d="M20 6 9 17l-5-5"/>"#,
        "folder" => {
            r#"<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>"#
        }
        "folder-open" => {
            r#"<path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2"/>"#
        }
        "list" => {
            r#"<path d="M3.5 6h.01"/><path d="M3.5 12h.01"/><path d="M3.5 18h.01"/><path d="M8 6h13"/><path d="M8 12h13"/><path d="M8 18h13"/>"#
        }
        "inbox" => {
            r#"<path d="M22 12h-6l-2 3h-4l-2-3H2"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/>"#
        }
        "sun" => {
            r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/>"#
        }
        "infinity" => {
            r#"<path d="M12 12c-2-2.67-4-4-6-4a4 4 0 1 0 0 8c2 0 4-1.33 6-4Zm0 0c2 2.67 4 4 6 4a4 4 0 0 0 0-8c-2 0-4 1.33-6 4Z"/>"#
        }
        "filter" => r#"<path d="M22 3H2l8 9.46V19l4 2v-8.54z"/>"#,
        "corner-down-right" => r#"<path d="m15 10 5 5-5 5"/><path d="M4 4v7a4 4 0 0 0 4 4h12"/>"#,
        "info" => r#"<circle cx="12" cy="12" r="9.5"/><path d="M12 16v-4"/><path d="M12 8h.01"/>"#,
        "sliders" => {
            r#"<path d="M21 4h-7"/><path d="M10 4H3"/><path d="M21 12h-9"/><path d="M8 12H3"/><path d="M21 20h-5"/><path d="M12 20H3"/><path d="M14 2v4"/><path d="M8 10v4"/><path d="M16 18v4"/>"#
        }
        "chart" => {
            r#"<path d="M3 3v16a2 2 0 0 0 2 2h16"/><path d="M18 17V9"/><path d="M13 17V5"/><path d="M8 17v-3"/>"#
        }
        "circle-check" => r#"<circle cx="12" cy="12" r="9.5"/><path d="m9 12 2 2 4-4"/>"#,
        "clock" => r#"<circle cx="12" cy="12" r="9.5"/><path d="M12 6.5V12l3.5 2"/>"#,
        "settings" => {
            r#"<circle cx="12" cy="12" r="3"/><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/>"#
        }
        "palette" => {
            r#"<circle cx="13.5" cy="6.5" r=".5"/><circle cx="17.5" cy="10.5" r=".5"/><circle cx="8.5" cy="7.5" r=".5"/><circle cx="6.5" cy="12.5" r=".5"/><path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.93 0 1.65-.75 1.65-1.69 0-.44-.18-.84-.44-1.13-.29-.29-.44-.65-.44-1.13a1.64 1.64 0 0 1 1.67-1.67h2c3.05 0 5.56-2.5 5.56-5.55C21.97 6.01 17.46 2 12 2z"/>"#
        }
        "keyboard" => {
            r#"<rect width="20" height="16" x="2" y="4" rx="2"/><path d="M6 8h.01M10 8h.01M14 8h.01M18 8h.01M8 12h.01M12 12h.01M16 12h.01M7 16h10"/>"#
        }
        "timer" => r#"<path d="M10 2h4"/><path d="m12 14 3-3"/><circle cx="12" cy="14" r="8"/>"#,
        "bell" => {
            r#"<path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9"/><path d="M10.3 21a1.94 1.94 0 0 0 3.4 0"/>"#
        }
        "party" => {
            r#"<path d="M5.8 11.3 2 22l10.7-3.79"/><path d="M4 3h.01M22 8h.01M15 2h.01M22 20h.01"/><path d="m22 2-2.24.75a2.9 2.9 0 0 0-1.96 3.12c.1.86-.57 1.63-1.45 1.63h-.38c-.86 0-1.6.6-1.76 1.44L14 10"/><path d="m11 2 .33.82c.34.86-.2 1.82-1.11 1.98C9.52 4.9 9 5.52 9 6.23V7"/><path d="M11 13c1.93 1.93 2.83 4.17 2 5-.83.83-3.07-.07-5-2-1.93-1.93-2.83-4.17-2-5 .83-.83 3.07.07 5 2Z"/>"#
        }
        "loader" => r#"<path d="M21 12a9 9 0 1 1-6.219-8.56"/>"#,
        "refresh" => {
            r#"<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/>"#
        }
        "calendar" => {
            r#"<rect width="18" height="18" x="3" y="4" rx="2"/><path d="M16 2v4M8 2v4M3 10h18"/>"#
        }
        "calendar-clock" => {
            r#"<path d="M21 7.5V6a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h3.5"/><path d="M16 2v4M8 2v4M3 10h5"/><path d="M17.5 17.5 16 16.3V14"/><circle cx="16" cy="16" r="6"/>"#
        }
        "square-check" => {
            r#"<rect width="18" height="18" x="3" y="3" rx="2"/><path d="m9 12 2 2 4-4"/>"#
        }
        "link" => {
            r#"<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>"#
        }
        "plug" => {
            r#"<path d="M12 22v-5"/><path d="M9 8V2M15 8V2"/><path d="M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8Z"/>"#
        }
        "wrench" => {
            r#"<path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/>"#
        }
        "chevrons-up-down" => r#"<path d="m7 15 5 5 5-5"/><path d="m7 9 5-5 5 5"/>"#,
        "x" => r#"<path d="M18 6 6 18M6 6l12 12"/>"#,
        "play" => r#"<path d="M6 3l14 9-14 9z"/>"#,
        "sunrise" => {
            r#"<path d="M12 2v8"/><path d="m4.93 10.93 1.41 1.41M2 18h2M20 18h2M19.07 10.93l-1.41 1.41M22 22H2"/><path d="m8 6 4-4 4 4"/><path d="M16 18a4 4 0 0 0-8 0"/>"#
        }
        "hourglass" => {
            r#"<path d="M5 22h14M5 2h14"/><path d="M17 22v-4.17a2 2 0 0 0-.59-1.42L12 12l-4.41 4.41A2 2 0 0 0 7 17.83V22"/><path d="M7 2v4.17a2 2 0 0 0 .59 1.42L12 12l4.41-4.41A2 2 0 0 0 17 6.17V2"/>"#
        }
        "text" => r#"<path d="M17 6H3M21 12H3M15.1 18H3"/>"#,
        "flag" => {
            r#"<path d="M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z"/><path d="M4 22v-7"/>"#
        }
        "tag" => {
            r#"<path d="M12.59 2.59A2 2 0 0 0 11.17 2H4a2 2 0 0 0-2 2v7.17a2 2 0 0 0 .59 1.41l8.7 8.71a2.43 2.43 0 0 0 3.42 0l6.58-6.58a2.43 2.43 0 0 0 0-3.42z"/><circle cx="7.5" cy="7.5" r=".5"/>"#
        }
        "ban" => r#"<circle cx="12" cy="12" r="9.5"/><path d="m5.3 5.3 13.4 13.4"/>"#,
        "user" => r#"<circle cx="12" cy="8" r="4.5"/><path d="M20 21a8 8 0 0 0-16 0"/>"#,
        "users" => {
            r#"<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/>"#
        }
        "repeat" => {
            r#"<path d="m17 2 4 4-4 4"/><path d="M3 11v-1a4 4 0 0 1 4-4h14"/><path d="m7 22-4-4 4-4"/><path d="M21 13v1a4 4 0 0 1-4 4H3"/>"#
        }
        "flame" => {
            r#"<path d="M8.5 14.5A2.5 2.5 0 0 0 11 12c0-1.38-.5-2-1-3-1.07-2.14-.22-4.05 2-6 .5 2.5 2 4.9 4 6.5 2 1.6 3 3.5 3 5.5a7 7 0 1 1-14 0c0-1.15.43-2.29 1-3a2.5 2.5 0 0 0 2.5 2.5z"/>"#
        }
        "target" => {
            r#"<circle cx="12" cy="12" r="9.5"/><circle cx="12" cy="12" r="5.5"/><circle cx="12" cy="12" r="1.5"/>"#
        }
        "copy" => {
            r#"<rect width="14" height="14" x="8" y="8" rx="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>"#
        }
        "pin" => {
            r#"<path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/>"#
        }
        "archive" => {
            r#"<rect width="20" height="5" x="2" y="3" rx="1"/><path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8"/><path d="M10 12h4"/>"#
        }
        "arrow-left-right" => {
            r#"<path d="M8 3 4 7l4 4"/><path d="M4 7h16"/><path d="m16 21 4-4-4-4"/><path d="M20 17H4"/>"#
        }
        "armchair" => {
            r#"<path d="M19 9V6a2 2 0 0 0-2-2H7a2 2 0 0 0-2 2v3"/><path d="M3 16a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-5a2 2 0 0 0-4 0v1.5a.5.5 0 0 1-.5.5h-9a.5.5 0 0 1-.5-.5V11a2 2 0 0 0-4 0z"/><path d="M5 18v2M19 18v2"/>"#
        }
        "file-text" => {
            r#"<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/><path d="M10 9H8M16 13H8M16 17H8"/>"#
        }
        "file" => {
            r#"<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/>"#
        }
        "sheet" => {
            r#"<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 9h18M3 15h18M9 9v12M15 9v12"/>"#
        }
        "presentation" => {
            r#"<path d="M2 3h20"/><path d="M21 3v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V3"/><path d="m7 21 5-5 5 5"/>"#
        }
        "form" => {
            r#"<rect width="8" height="4" x="8" y="2" rx="1"/><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/><path d="M12 11h4M12 16h4M8 11h.01M8 16h.01"/>"#
        }
        "image" => {
            r#"<rect width="18" height="18" x="3" y="3" rx="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.09-3.09a2 2 0 0 0-2.82 0L6 21"/>"#
        }
        "gem" => {
            r#"<path d="M6 3h12l4 6-10 13L2 9Z"/><path d="M11 3 8 9l4 13 4-13-3-6"/><path d="M2 9h20"/>"#
        }
        "globe" => {
            r#"<circle cx="12" cy="12" r="9.5"/><path d="M12 2.5a14.5 14.5 0 0 0 0 19 14.5 14.5 0 0 0 0-19"/><path d="M2.5 12h19"/>"#
        }
        "chevron-left" => r#"<path d="m15 18-6-6 6-6"/>"#,
        "chevron-right" => r#"<path d="m9 18 6-6-6-6"/>"#,
        "chevron-down" => r#"<path d="m6 9 6 6 6-6"/>"#,
        "cloud" => r#"<path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z"/>"#,
        "server" => {
            r#"<rect width="20" height="8" x="2" y="2" rx="2"/><rect width="20" height="8" x="2" y="14" rx="2"/><path d="M6 6h.01"/><path d="M6 18h.01"/>"#
        }
        "mail" => {
            r#"<rect width="20" height="16" x="2" y="4" rx="2"/><path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7"/>"#
        }
        _ => return None,
    })
}

/// `name` at `size` points, in the colour the caller gives it.
pub fn icon(name: &'static str, size: f32) -> gpui::Svg {
    svg()
        .path(SharedString::from(format!("icons/{name}.svg")))
        .size(px(size))
        .flex_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AssetSource;

    #[test]
    fn every_listed_icon_loads() {
        for &name in NAMES {
            let path = format!("icons/{name}.svg");
            assert!(Assets.load(&path).unwrap().is_some(), "{name}");
        }
        assert!(Assets.load("icons/nope.svg").unwrap().is_none());
    }

    #[test]
    fn every_icon_parses_as_gpui_will_render_it() {
        for &name in NAMES {
            let bytes = Assets.load(&format!("icons/{name}.svg")).unwrap().unwrap();
            let tree = usvg::Tree::from_data(&bytes, &usvg::Options::default())
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(tree.root().has_children(), "{name} draws nothing");
        }
    }
}
