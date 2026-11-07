//! Quick capture: a field that floats over whatever app is in front, for a
//! task typed and filed without going to the window. Return files it (the
//! trailing words read as the add row reads them), Esc puts it away.
//! `QuickCapturePanelController.swift`.

use super::Takt;
use super::widgets::field_text;
use crate::text_field::{FieldKey, TextField};
use crate::theme::Tokens;
use gpui::{
    App as GpuiApp, Bounds, Context, Entity, EntityInputHandler, FocusHandle, Focusable,
    KeyDownEvent, Pixels, UTF16Selection, WeakEntity, Window, WindowBounds, WindowKind,
    WindowOptions, div, point, prelude::*, px, size,
};
use std::ops::Range;

pub struct QuickCapture {
    takt: Entity<Takt>,
    this: WeakEntity<Self>,
    focus: FocusHandle,
    field: TextField,
    /// Why the last Return didn't file it.
    problem: Option<String>,
}

impl Focusable for QuickCapture {
    fn focus_handle(&self, _: &GpuiApp) -> FocusHandle {
        self.focus.clone()
    }
}

/// Opens the field a third of the way down the main display.
pub fn open(takt: Entity<Takt>, cx: &mut GpuiApp) -> Option<gpui::WindowHandle<QuickCapture>> {
    let width = px(560.);
    let origin = cx
        .primary_display()
        .map(|d| {
            let b = d.bounds();
            point(
                b.origin.x + (b.size.width - width) / 2.,
                b.origin.y + b.size.height / 4.,
            )
        })
        .unwrap_or(point(px(400.), px(200.)));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin,
            size: size(width, px(104.)),
        })),
        titlebar: None,
        kind: WindowKind::PopUp,
        is_movable: true,
        is_resizable: false,
        is_minimizable: false,
        focus: true,
        show: true,
        ..Default::default()
    };
    let handle = cx
        .open_window(options, |window, cx| {
            let capture = cx.new(|cx| QuickCapture {
                takt: takt.clone(),
                this: cx.entity().downgrade(),
                focus: cx.focus_handle(),
                field: TextField::default(),
                problem: None,
            });
            window.focus(&capture.focus_handle(cx));
            capture
        })
        .ok();
    // Over whatever app is in front, which means coming forward ourselves.
    cx.activate(true);
    handle
}

impl QuickCapture {
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let outcome = self.field.handle(&event.keystroke);
        if Takt::clipboard_key(&mut self.field, &outcome, cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        match outcome {
            // Printable: the input method types it.
            FieldKey::Text(_) => return,
            FieldKey::Commit => {
                let text = self.field.text.clone();
                if text.trim().is_empty() {
                    window.remove_window();
                } else {
                    let filed = self.takt.update(cx, |takt, cx| {
                        let filed = takt.app.quick_capture(&text);
                        takt.settle(cx);
                        filed
                    });
                    match filed {
                        Ok(_) => window.remove_window(),
                        Err(problem) => self.problem = Some(problem),
                    }
                }
            }
            FieldKey::Cancel => window.remove_window(),
            FieldKey::Edited => self.problem = None,
            _ => {}
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl Render for QuickCapture {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = &self.takt.read(cx).app;
        let t = Tokens::of(&app.theme(), window.appearance(), &app.prefs.appearance);
        let labels = app.capture_labels(&self.field.text);
        let destination = app
            .quick_capture_destination()
            .map(|(_, name)| name)
            .unwrap_or_else(|| "No list".into());
        let problem = self
            .problem
            .clone()
            .or_else(|| app.capture_parts(&self.field.text).problem);
        let hook = Some((self.this.clone(), self.focus.clone()));
        div()
            .id("quick-capture")
            .key_context("QuickCapture")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .size_full()
            .flex()
            .flex_col()
            .bg(t.surface)
            .border_1()
            .border_color(t.border)
            .rounded(px(10.))
            .text_color(t.text)
            .font_family(super::UI_FONT)
            .child(
                div()
                    .px_4()
                    .pt_3()
                    .pb_2()
                    .text_size(px(16.))
                    .child(field_text(
                        &self.field,
                        "Add a task… 45m #tag @fri !1",
                        t,
                        hook,
                    )),
            )
            .child(
                div()
                    .px_4()
                    .pb_2()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .text_size(px(11.))
                    .child(
                        div()
                            .px_1()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(t.border_variant)
                            .text_color(t.muted)
                            .child(format!("→ {destination}")),
                    )
                    .children(labels.into_iter().map(|label| {
                        div()
                            .px_1()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(t.border_variant)
                            .text_color(t.accent)
                            .child(label)
                    })),
            )
            .child(
                div()
                    .mt_auto()
                    .px_4()
                    .py_1()
                    .border_t_1()
                    .border_color(t.border_variant)
                    .text_size(px(11.))
                    .text_color(if problem.is_some() {
                        t.error
                    } else {
                        t.placeholder
                    })
                    .child(problem.unwrap_or_else(|| "↩ add · esc close".into())),
            )
    }
}

impl EntityInputHandler for QuickCapture {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.field.range_from_utf16(&range);
        actual.replace(self.field.range_to_utf16(&range));
        self.field.text.get(range).map(str::to_string)
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let selection = self.field.selection();
        Some(UTF16Selection {
            range: self.field.range_to_utf16(&selection),
            reversed: self.field.anchor.is_some_and(|a| a > self.field.cursor),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.field
            .marked
            .as_ref()
            .map(|range| self.field.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.field.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match range {
            Some(range) => {
                let range = self.field.range_from_utf16(&range);
                self.field.replace(range, text);
            }
            None => self.field.insert(text),
        }
        self.problem = None;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .map(|range| self.field.range_from_utf16(&range))
            .or_else(|| self.field.marked.clone())
            .unwrap_or_else(|| self.field.selection());
        let selected = selected.map(|selected| TextField::new(text).range_from_utf16(&selected));
        self.field.mark(range, text, selected);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(Bounds::new(bounds.origin, size(px(2.), px(20.))))
    }

    fn character_index_for_point(
        &mut self,
        _: gpui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
