//! What hovering a task's link shows: its name and kind, and a look inside
//! where one can be had — Drive's own thumbnail of the file, the opening
//! lines of an Obsidian note, a picture on this Mac. Each is fetched once,
//! off the main thread, and kept for the session by address.

use super::{Tokens, UI_FONT};
use crate::ui::zoom::px;
use gpui::{
    AnyView, App as GpuiApp, AppContext, Context, FontWeight, IntoElement, ObjectFit,
    ParentElement, Render, SharedString, Styled, StyledImage, Window, div, img,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use takt_core::links::{LinkKind, TaskLink};

/// How far a preview has got.
#[derive(Clone)]
enum Preview {
    Loading,
    Image(Arc<gpui::Image>),
    Text(String),
    /// Nothing to show beyond the name: a web page, a folder, a failure.
    Nothing,
}

fn previews() -> &'static Mutex<HashMap<String, Preview>> {
    static PREVIEWS: OnceLock<Mutex<HashMap<String, Preview>>> = OnceLock::new();
    PREVIEWS.get_or_init(Default::default)
}

/// Drive's thumbnails are asked for at this size, then drawn smaller.
const THUMBNAIL: u32 = 480;
/// As much of a note as the preview reads.
const NOTE_CHARACTERS: usize = 600;

pub(super) struct LinkPreview {
    link: TaskLink,
    t: Tokens,
}

/// What `.tooltip(…)` takes for a link: the preview, its fetch started on
/// first hover. `client_id` is the Google client Drive is reached with;
/// `vaults` the Obsidian vaults a note is looked for in.
pub(super) fn tip(
    link: TaskLink,
    client_id: String,
    vaults: Vec<PathBuf>,
    t: Tokens,
) -> impl Fn(&mut Window, &mut GpuiApp) -> AnyView + 'static {
    move |_, cx| {
        let link = link.clone();
        let (client_id, vaults) = (client_id.clone(), vaults.clone());
        cx.new(|cx| {
            start(&link, client_id, vaults, cx);
            LinkPreview { link, t }
        })
        .into()
    }
}

/// Begins fetching `link`'s preview unless it has been already.
fn start(link: &TaskLink, client_id: String, vaults: Vec<PathBuf>, cx: &mut Context<LinkPreview>) {
    let url = link.url.clone();
    {
        let mut kept = previews().lock().unwrap_or_else(|e| e.into_inner());
        if kept.contains_key(&url) {
            return;
        }
        kept.insert(url.clone(), Preview::Loading);
    }
    let link = link.clone();
    cx.spawn(async move |this, cx| {
        let preview = cx
            .background_executor()
            .spawn(async move { fetch(&link, &client_id, &vaults) })
            .await;
        previews()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(url, preview);
        let _ = this.update(cx, |_, cx| cx.notify());
    })
    .detach();
}

/// The preview itself. Blocks: run off the main thread.
fn fetch(link: &TaskLink, client_id: &str, vaults: &[PathBuf]) -> Preview {
    match link.kind {
        LinkKind::Drive if !link.is_folder => {
            let Some(id) = crate::google::drive::file_id(&link.url) else {
                return Preview::Nothing;
            };
            if crate::google::drive_token().is_none() {
                return Preview::Nothing;
            }
            crate::google::drive_access_token(client_id)
                .ok()
                .and_then(|token| crate::google::drive::thumbnail(&token, &id, THUMBNAIL).ok())
                .flatten()
                .and_then(image)
                .map_or(Preview::Nothing, Preview::Image)
        }
        LinkKind::Obsidian => crate::state::notes_editor::resolve(&link.url, vaults)
            .and_then(|note| note.path)
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| note_opening(&text))
            .filter(|text| !text.is_empty())
            .map_or(Preview::Nothing, Preview::Text),
        _ => local_file(&link.url)
            .and_then(|path| std::fs::read(path).ok())
            .and_then(image)
            .map_or(Preview::Nothing, Preview::Image),
    }
}

/// A `file://` address or a path, as the file it names.
fn local_file(url: &str) -> Option<PathBuf> {
    url.strip_prefix("file://")
        .map(|path| PathBuf::from(crate::state::notes_editor::decode(path)))
        .or_else(|| url.starts_with('/').then(|| PathBuf::from(url)))
}

/// Bytes as an image GPUI can draw, by what they start with.
fn image(bytes: Vec<u8>) -> Option<Arc<gpui::Image>> {
    use gpui::ImageFormat::*;
    let format = match bytes.as_slice() {
        [0x89, b'P', b'N', b'G', ..] => Png,
        [0xFF, 0xD8, ..] => Jpeg,
        [b'G', b'I', b'F', ..] => Gif,
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Webp,
        [b'B', b'M', ..] => Bmp,
        _ => return None,
    };
    Some(Arc::new(gpui::Image::from_bytes(format, bytes)))
}

/// A note's opening, as the preview shows it: its front matter left off,
/// blank lines run together, cut at a line near `NOTE_CHARACTERS`.
fn note_opening(text: &str) -> String {
    let body = match text.strip_prefix("---\n") {
        Some(rest) => rest.split_once("\n---").map_or(rest, |(_, after)| after),
        None => text,
    };
    let mut out = String::new();
    let mut blank = false;
    for line in body.lines().map(str::trim_end) {
        if line.trim().is_empty() {
            blank = !out.is_empty();
            continue;
        }
        if out.len() + line.len() > NOTE_CHARACTERS {
            out.push('…');
            break;
        }
        if !out.is_empty() {
            out.push_str(if blank { "\n\n" } else { "\n" });
        }
        blank = false;
        out.push_str(line);
    }
    out
}

impl Render for LinkPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.t;
        let preview = previews()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&self.link.url)
            .cloned()
            .unwrap_or(Preview::Nothing);
        let savable = self.link.kind == LinkKind::Drive && !self.link.is_folder;
        let look: Option<gpui::AnyElement> = match preview {
            Preview::Image(picture) => Some(
                div()
                    .mt_1()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(t.border_variant)
                    .overflow_hidden()
                    .child(
                        img(picture)
                            .max_w(px(300.))
                            .max_h(px(220.))
                            .object_fit(ObjectFit::Contain),
                    )
                    .into_any_element(),
            ),
            Preview::Text(text) => Some(
                div()
                    .mt_1()
                    .pt_1()
                    .border_t_1()
                    .border_color(t.border_variant)
                    .text_size(px(11.))
                    .text_color(t.muted)
                    .children(
                        text.lines()
                            .map(|line| div().child(SharedString::from(line.to_string())))
                            .collect::<Vec<_>>(),
                    )
                    .into_any_element(),
            ),
            Preview::Loading => Some(
                div()
                    .mt_1()
                    .text_size(px(11.))
                    .text_color(t.placeholder)
                    .child("Loading a preview…")
                    .into_any_element(),
            ),
            Preview::Nothing => None,
        };
        div()
            .font_family(UI_FONT)
            .max_w(px(320.))
            .px_2()
            .py(px(6.))
            .rounded(px(6.))
            .bg(t.surface)
            .border_1()
            .border_color(t.border)
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.text)
                    .child(self.link.title.clone()),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(t.muted)
                    .child(self.link.subtitle.clone()),
            )
            .children(look)
            .child(
                div()
                    .mt_1()
                    .text_size(px(10.))
                    .text_color(t.placeholder)
                    .child(if savable {
                        "Click to open · right-click to save or remove"
                    } else {
                        "Click to open · right-click for more"
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::note_opening;

    #[test]
    fn a_note_opens_after_its_front_matter_with_blank_lines_run_together() {
        let note = "---\ntags: [x]\n---\n# Plan\n\n\n\nFirst point\nSecond\n";
        assert_eq!(note_opening(note), "# Plan\n\nFirst point\nSecond");
    }

    #[test]
    fn a_long_note_is_cut_at_a_line() {
        let long = "word ".repeat(200);
        let note = format!("short\n{long}\nmore");
        assert_eq!(note_opening(&note), "short…");
    }
}
