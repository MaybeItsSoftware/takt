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

const NAMES: [&str; 19] = [
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
        for name in NAMES {
            let path = format!("icons/{name}.svg");
            assert!(Assets.load(&path).unwrap().is_some(), "{name}");
        }
        assert!(Assets.load("icons/nope.svg").unwrap().is_none());
    }

    #[test]
    fn every_icon_parses_as_gpui_will_render_it() {
        for name in NAMES {
            let bytes = Assets.load(&format!("icons/{name}.svg")).unwrap().unwrap();
            let tree = usvg::Tree::from_data(&bytes, &usvg::Options::default())
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(tree.root().has_children(), "{name} draws nothing");
        }
    }
}
