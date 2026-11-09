//! The Markdown editor plugin's pane (`state::notes_editor`): over the
//! window below the title bar, the text on the left in the monospaced face
//! and wrapped at words, the Markdown drawn on the right.
//!
//! Its keys are its own while it is up: a chord it doesn't use goes no
//! further, so nothing behind it moves while the text is unsaved.

use super::*;
use crate::state::markdown::{self, Block, Inline, Marks};
use crate::state::notes_editor::{Layout, Source};
use gpui::{FontStyle, HighlightStyle, StrikethroughStyle, StyledText, UnderlineStyle};

/// Where the editor's two sides are scrolled.
#[derive(Default)]
pub struct NotesView {
    text: gpui::ScrollHandle,
    preview: gpui::ScrollHandle,
}

impl Takt {
    /// A key while the editor is up. False only for a printable key, which
    /// the input method types.
    pub(super) fn notes_editor_key(
        &mut self,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        let m = keystroke.modifiers;
        let key = keystroke.key.as_str();
        // The command's own chord puts it away again.
        if (m.platform || m.control)
            && let Some(spelled) = spell(&Press::from_keystroke(keystroke))
            && self
                .app
                .bindings
                .command(&spelled, self.app.surface())
                .is_some_and(|c| c.id == "taskMaximiseNotes")
        {
            self.close_notes_editor(cx);
            return true;
        }
        let changed = self
            .app
            .notes_editor
            .as_ref()
            .is_some_and(|e| matches!(e.source, Source::Changed(_)));
        match (key, m.platform, m.shift, m.control) {
            ("escape", ..) | ("enter", true, false, false) => {
                self.close_notes_editor(cx);
                return true;
            }
            ("s", true, false, false) => {
                let text = self.field.text.clone();
                if self.app.save_notes_editor(&text, changed) {
                    self.app.message = Some("Saved.".into());
                }
                return true;
            }
            ("e", true, false, false) => {
                self.app.cycle_notes_layout();
                return true;
            }
            ("o", true, true, false) => {
                let text = self.field.text.clone();
                if let Some(next) = self.app.switch_notes_source(&text) {
                    self.field = TextField::multiline(&next);
                }
                return true;
            }
            ("o", true, false, false) => {
                self.app.open_notes_in_obsidian();
                return true;
            }
            ("r", true, false, false) if changed => {
                let mine = self.field.text.clone();
                if let Some(theirs) = self.app.take_their_notes() {
                    cx.write_to_clipboard(ClipboardItem::new_string(mine));
                    self.field = TextField::multiline(&theirs);
                }
                return true;
            }
            ("tab", false, false, true) => {
                self.app.toggle_notes_side();
                return true;
            }
            ("tab", false, shift, false) => {
                self.indent_line(!shift);
                self.follow_caret();
                return true;
            }
            _ => {}
        }
        let outcome = self.field.handle(keystroke);
        if Self::clipboard_key(&mut self.field, &outcome, cx) {
            self.follow_caret();
            return true;
        }
        match outcome {
            FieldKey::Text(_) => false,
            FieldKey::Commit => {
                self.close_notes_editor(cx);
                true
            }
            _ => {
                self.follow_caret();
                true
            }
        }
    }

    fn close_notes_editor(&mut self, cx: &mut Context<Self>) {
        let text = self.field.text.clone();
        if let Some(unsaved) = self.app.close_notes_editor(&text) {
            cx.write_to_clipboard(ClipboardItem::new_string(unsaved));
        }
        self.field = TextField::default();
    }

    /// Tab puts two spaces at the start of the caret's line; ⇧Tab takes
    /// up to two away. Lists nest by it.
    fn indent_line(&mut self, deeper: bool) {
        let field = &mut self.field;
        let start = field.text[..field.cursor]
            .rfind('\n')
            .map_or(0, |ix| ix + 1);
        if deeper {
            field.text.insert_str(start, "  ");
            field.cursor += 2;
        } else {
            let spaces = field.text[start..]
                .chars()
                .take(2)
                .take_while(|c| *c == ' ')
                .count();
            field.text.replace_range(start..start + spaces, "");
            field.cursor = field.cursor.saturating_sub(spaces).max(start);
        }
        field.anchor = None;
        field.marked = None;
    }

    /// The caret's line into view.
    fn follow_caret(&self) {
        let line = self.field.text[..self.field.cursor].matches('\n').count();
        self.notes_view.text.scroll_to_item(line);
    }

    /// Each second while it is up: the source looked at again.
    pub(super) fn poll_notes_editor(&mut self) -> bool {
        if !matches!(self.app.editing, Some(Editing::NotesEditor)) {
            return false;
        }
        let text = self.field.text.clone();
        if let Some(next) = self.app.poll_notes_editor(&text) {
            let cursor = self.field.cursor.min(next.len());
            self.field = TextField::multiline(&next);
            // On a char boundary at or before where it was.
            self.field.cursor = (0..=cursor)
                .rev()
                .find(|ix| next.is_char_boundary(*ix))
                .unwrap_or(0);
        }
        true
    }

    pub(super) fn notes_editor_view(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let Some(editor) = self.app.notes_editor.clone() else {
            return div().into_any_element();
        };
        let dirty = self.field.text != editor.saved;
        let button = |id: &'static str, label: String, keys: &str| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .py(px(2.))
                .rounded(px(6.))
                .border_1()
                .border_color(t.border_variant)
                .text_size(px(12.))
                .text_color(t.muted)
                .hover(|d| d.bg(t.hover).text_color(t.text))
                .child(label)
                .child(key_hint(keys.to_string(), t))
        };
        let has_note = editor.note.is_some();
        let header = div()
            .flex_none()
            .px_4()
            .py_2()
            .flex()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(t.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(micro_label(&editor.editing_what(), t))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.text)
                            .truncate()
                            .child(editor.title.clone()),
                    ),
            )
            .child(
                button("notes-layout", editor.layout.name().into(), "⌘E").on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.app.cycle_notes_layout();
                        cx.notify();
                    },
                )),
            )
            .when(has_note, |d| {
                let label = if editor.same {
                    "Same as the Obsidian file"
                } else {
                    "Additional to the Obsidian file"
                };
                d.child(
                    button("notes-source", label.into(), "⌘⇧O")
                        .when(editor.same, |b| b.border_color(t.accent.opacity(0.5)))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            let text = this.field.text.clone();
                            if let Some(next) = this.app.switch_notes_source(&text) {
                                this.field = TextField::multiline(&next);
                            }
                            cx.notify();
                        })),
                )
                .child(
                    button("notes-obsidian", "Obsidian".into(), "⌘O").on_click(cx.listener(
                        |this, _: &ClickEvent, _, cx| {
                            this.app.open_notes_in_obsidian();
                            cx.notify();
                        },
                    )),
                )
            })
            .child(
                button("notes-done", "Done".into(), "esc").on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.close_notes_editor(cx);
                        cx.notify();
                    },
                )),
            );
        let name = editor
            .note
            .as_ref()
            .map(|n| format!("{}.md", n.name))
            .unwrap_or_default();
        let banner = match &editor.source {
            Source::Changed(_) => Some((
                t.warning,
                format!(
                    "{} changed elsewhere while you were typing. ⌘S keeps yours; ⌘R takes theirs and puts yours on the clipboard.",
                    if editor.same {
                        name.as_str()
                    } else {
                        "The notes"
                    }
                ),
            )),
            Source::Missing if editor.same => Some((
                t.warning,
                format!("{name} isn't in the vault. Type and ⌘S (or Esc) to make it."),
            )),
            _ => None,
        };
        let banner = banner.map(|(colour, text)| {
            div()
                .flex_none()
                .px_4()
                .py_1()
                .border_b_1()
                .border_color(colour.opacity(0.4))
                .bg(colour.opacity(0.1))
                .text_color(t.text)
                .text_size(px(12.))
                .child(text)
        });
        let show_text = editor.layout != Layout::Preview;
        let show_preview = editor.layout != Layout::Edit;
        let (preview_label, preview_text) = if editor.showing_note && !editor.same {
            (
                format!("{name} · read only"),
                editor
                    .note_text
                    .clone()
                    .unwrap_or_else(|| format!("{name} isn't on this Mac.")),
            )
        } else {
            ("Preview".to_string(), self.field.text.clone())
        };
        let preview_label = if has_note && !editor.same {
            format!(
                "{preview_label}  ·  ⌃⇥ {}",
                if editor.showing_note {
                    "preview"
                } else {
                    &name
                }
            )
        } else {
            preview_label
        };
        let text_side = div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .when(show_preview, |d| d.border_r_1().border_color(t.border))
            .child(div().px_4().pt_2().child(micro_label("Markdown", t)))
            .child(
                div()
                    .id("notes-editor-text")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.notes_view.text)
                    .px_4()
                    .py_2()
                    .font_family(MONO_FONT)
                    .text_size(px(13.))
                    .child(wrapped_field(
                        &self.field,
                        "Write in Markdown…",
                        t,
                        self.input_hook(),
                    )),
            );
        let preview_side = div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(div().px_5().pt_2().child(micro_label(&preview_label, t)))
            .child(
                div()
                    .id("notes-editor-preview")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.notes_view.preview)
                    .px_5()
                    .py_2()
                    .flex()
                    .flex_col()
                    .children(rendered(&preview_text, t)),
            );
        let words = self.field.text.split_whitespace().count();
        let footer = div()
            .flex_none()
            .px_4()
            .py_1()
            .flex()
            .gap_3()
            .border_t_1()
            .border_color(t.border)
            .font_family(MONO_FONT)
            .text_size(px(11.))
            .text_color(t.muted)
            .child(format!("{words} words"))
            .child(if dirty { "unsaved" } else { "saved" })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(self.app.message.clone().unwrap_or_default()),
            )
            .child("⌘S save · ⇥ indent · esc done");
        div()
            .id("notes-editor")
            .occlude()
            .absolute()
            .top(px(TITLE_BAR_HEIGHT))
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .flex_col()
            .bg(t.background)
            .border_t_1()
            .border_color(t.border)
            .child(header)
            .children(banner)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when(show_text, |d| d.child(text_side))
                    .when(show_preview, |d| d.child(preview_side)),
            )
            .child(footer)
            .into_any_element()
    }
}

/// The section eyebrow: small, bold, spaced capitals, muted.
fn micro_label(text: &str, t: Tokens) -> impl IntoElement {
    div()
        .text_size(px(10.))
        .font_weight(FontWeight::BOLD)
        .text_color(t.muted)
        .child(text.to_uppercase())
}

/// The field as lines that wrap at words, each a child of the element
/// that scrolls them, so the caret's line can be scrolled to.
fn wrapped_field(
    field: &TextField,
    placeholder: &str,
    t: Tokens,
    hook: Option<InputHook>,
) -> AnyElement {
    let caret = || div().w(px(1.5)).h(px(16.)).flex_none().bg(t.accent);
    let body = if field.text.is_empty() {
        div().flex().items_center().child(caret()).child(
            div()
                .text_color(t.placeholder)
                .child(placeholder.to_string()),
        )
    } else {
        let selection = field.selection();
        let marked = field.marked.clone().unwrap_or(0..0);
        let text = &field.text;
        let mut lines = Vec::new();
        let mut line_start = 0;
        for line in text.split('\n') {
            let line_end = line_start + line.len();
            let mut cuts = vec![
                line_start,
                line_end,
                selection.start,
                selection.end,
                marked.start,
                marked.end,
                field.cursor,
            ];
            // Each word starts a piece, so the line can wrap between them.
            let mut previous_space = false;
            for (ix, c) in line.char_indices() {
                if previous_space && !c.is_whitespace() {
                    cuts.push(line_start + ix);
                }
                previous_space = c.is_whitespace();
            }
            cuts.retain(|cut| (line_start..=line_end).contains(cut));
            cuts.sort_unstable();
            cuts.dedup();
            let mut pieces: Vec<AnyElement> = Vec::new();
            for pair in cuts.windows(2) {
                let (from, to) = (pair[0], pair[1]);
                if from == field.cursor && selection.is_empty() {
                    pieces.push(caret().into_any_element());
                }
                let selected = from >= selection.start && to <= selection.end && from < to;
                let composing = from >= marked.start && to <= marked.end && from < to;
                pieces.push(
                    div()
                        .whitespace_nowrap()
                        .when(selected, |d| d.bg(t.accent.opacity(0.3)))
                        .when(composing, |d| d.underline())
                        .child(SharedString::from(text[from..to].to_string()))
                        .into_any_element(),
                );
            }
            if field.cursor == line_end && selection.is_empty() {
                pieces.push(caret().into_any_element());
            }
            lines.push(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .min_h(px(19.))
                    .children(pieces)
                    .into_any_element(),
            );
            line_start = line_end + 1;
        }
        div().flex().flex_col().children(lines)
    };
    let Some((view, focus)) = hook else {
        return body.into_any_element();
    };
    div()
        .relative()
        .child(body)
        .child(
            gpui::canvas(
                |bounds, _, _| bounds,
                move |bounds, _, window, cx| {
                    if let Some(view) = view.upgrade() {
                        window.handle_input(
                            &focus,
                            gpui::ElementInputHandler::new(bounds, view),
                            cx,
                        );
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .into_any_element()
}

/// One line of inline Markdown as styled text.
fn styled(inline: &Inline, t: Tokens) -> StyledText {
    let highlights: Vec<(Range<usize>, HighlightStyle)> = inline
        .spans
        .iter()
        .map(|(range, marks)| (range.clone(), highlight(marks, t)))
        .collect();
    StyledText::new(SharedString::from(inline.text.clone())).with_highlights(highlights)
}

fn highlight(marks: &Marks, t: Tokens) -> HighlightStyle {
    let mut style = HighlightStyle::default();
    if marks.bold {
        style.font_weight = Some(FontWeight::BOLD);
    }
    if marks.italic {
        style.font_style = Some(FontStyle::Italic);
    }
    if marks.code {
        style.background_color = Some(t.hover);
        style.color = Some(t.text);
    }
    if marks.strike {
        style.strikethrough = Some(StrikethroughStyle {
            thickness: px(1.),
            color: Some(t.muted),
        });
        style.color = Some(t.muted);
    }
    if marks.highlight {
        style.background_color = Some(t.warning.opacity(0.3));
    }
    if marks.link || marks.tag {
        style.color = Some(t.accent);
    }
    if marks.link {
        style.underline = Some(UnderlineStyle {
            thickness: px(1.),
            color: Some(t.accent.opacity(0.5)),
            wavy: false,
        });
    }
    style
}

/// `text`'s Markdown, drawn.
fn rendered(text: &str, t: Tokens) -> Vec<AnyElement> {
    if text.trim().is_empty() {
        return vec![
            div()
                .text_color(t.placeholder)
                .child("Nothing written yet.")
                .into_any_element(),
        ];
    }
    markdown::blocks(text)
        .into_iter()
        .map(|block| match block {
            Block::Heading { level, text } => div()
                .pt(px(if level <= 2 { 10. } else { 6. }))
                .pb_1()
                .text_size(px(match level {
                    1 => 24.,
                    2 => 20.,
                    3 => 17.,
                    _ => 15.,
                }))
                .font_weight(FontWeight::BOLD)
                .text_color(t.text)
                .when(level <= 2, |d| {
                    d.border_b_1().border_color(t.border_variant)
                })
                .child(styled(&text, t))
                .into_any_element(),
            Block::Line(text) => div()
                .py(px(1.))
                .text_color(t.text)
                .child(styled(&text, t))
                .into_any_element(),
            Block::Item {
                depth,
                marker,
                task,
                text,
            } => {
                let done = task == Some(true);
                let marker = match task {
                    Some(true) => "☑".to_string(),
                    Some(false) => "☐".to_string(),
                    None => marker,
                };
                div()
                    .flex()
                    .gap_2()
                    .py(px(1.))
                    .pl(px(4. + 18. * depth as f32))
                    .child(
                        div()
                            .flex_none()
                            .min_w(px(14.))
                            .text_color(if task.is_some() { t.accent } else { t.muted })
                            .child(marker),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(if done { t.muted } else { t.text })
                            .when(done, |d| d.line_through())
                            .child(styled(&text, t)),
                    )
                    .into_any_element()
            }
            Block::Quote(text) => div()
                .my_1()
                .pl_3()
                .border_l_2()
                .border_color(t.border)
                .text_color(t.muted)
                .child(styled(&text, t))
                .into_any_element(),
            Block::Code { lang, text } => div()
                .my_1()
                .px_3()
                .py_2()
                .rounded(px(6.))
                .border_1()
                .border_color(t.border_variant)
                .bg(t.surface)
                .font_family(MONO_FONT)
                .text_size(px(12.))
                .flex()
                .flex_col()
                .overflow_hidden()
                .when(!lang.is_empty(), |d| d.child(micro_label(&lang, t)))
                .children(text.split('\n').map(|line| {
                    div()
                        .whitespace_nowrap()
                        .min_h(px(16.))
                        .child(line.to_string())
                }))
                .into_any_element(),
            Block::Properties(text) => div()
                .mb_2()
                .px_3()
                .py_2()
                .rounded(px(6.))
                .border_1()
                .border_color(t.border_variant)
                .font_family(MONO_FONT)
                .text_size(px(11.))
                .text_color(t.muted)
                .flex()
                .flex_col()
                .child(micro_label("Properties", t))
                .children(text.split('\n').map(|line| div().child(line.to_string())))
                .into_any_element(),
            Block::Rule => div().my_2().h(px(1.)).bg(t.border).into_any_element(),
            Block::Blank => div().h(px(10.)).into_any_element(),
        })
        .collect()
}
