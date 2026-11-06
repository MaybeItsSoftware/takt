//! The focus panel: a small window that floats over everything, for the
//! block in hand once the main window is put away. The running task and its
//! clock, the queue behind it, and with nothing running the day's ladder to
//! start from. `FocusPanelController.swift` and `FocusPanelView.swift`.
//!
//! It holds no state of its own beyond its cursor: it reads the main
//! window's `App` through the `Takt` entity and redraws when that does, and
//! every key it takes runs on that same `App`.

use super::Takt;
use crate::keys::{Press, spell};
use crate::state::App;
use crate::theme::Tokens;
use gpui::{
    App as GpuiApp, Bounds, Context, Entity, FocusHandle, Focusable, KeyDownEvent, Window,
    WindowBounds, WindowKind, WindowOptions, div, point, prelude::*, px, size,
};

pub struct FocusPanel {
    takt: Entity<Takt>,
    focus: FocusHandle,
    cursor: usize,
}

impl Focusable for FocusPanel {
    fn focus_handle(&self, _: &GpuiApp) -> FocusHandle {
        self.focus.clone()
    }
}

/// Opens the panel at the top right of the main display.
pub fn open(takt: Entity<Takt>, cx: &mut GpuiApp) -> Option<gpui::WindowHandle<FocusPanel>> {
    let display = cx.primary_display();
    let origin = display
        .map(|d| {
            let b = d.bounds();
            point(b.origin.x + b.size.width - px(380.), b.origin.y + px(40.))
        })
        .unwrap_or(point(px(900.), px(40.)));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin,
            size: size(px(340.), px(420.)),
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
    cx.open_window(options, |window, cx| {
        let panel = cx.new(|cx| {
            cx.observe(&takt, |_, _, cx| cx.notify()).detach();
            FocusPanel {
                takt: takt.clone(),
                focus: cx.focus_handle(),
                cursor: 0,
            }
        });
        window.focus(&panel.focus_handle(cx));
        panel
    })
    .ok()
}

/// What the panel lists: the queue while a block runs, the day's ladder
/// when nothing does. (id, title, detail).
fn rows(app: &App) -> Vec<(String, String, String)> {
    if app.focus_session.is_some() {
        return app
            .focus_queue()
            .into_iter()
            .map(|entry| {
                let list = app
                    .list(&entry.task.list_id)
                    .map(|l| l.name.clone())
                    .unwrap_or_default();
                (entry.task.id, entry.task.title, list)
            })
            .collect();
    }
    app.today
        .rows()
        .filter(|row| row.task.status == "open")
        .map(|row| {
            let list = app
                .list(&row.task.list_id)
                .map(|l| l.name.clone())
                .unwrap_or_default();
            (row.task.id.clone(), row.task.title.clone(), list)
        })
        .collect()
}

impl FocusPanel {
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = spell(&Press::from_keystroke(&event.keystroke)) else {
            return;
        };
        cx.stop_propagation();
        if key == "escape" && self.takt.read(cx).app.focus_prompt.is_none() {
            window.remove_window();
            return;
        }
        let cursor = self.cursor;
        let mut close = false;
        self.takt.update(cx, |takt, cx| {
            let app = &mut takt.app;
            if app.focus_prompt.is_some() {
                app.focus_prompt_key(&key);
            } else {
                let listed = rows(app);
                match key.as_str() {
                    "j" | "down" => {}
                    "k" | "up" => {}
                    "enter" => {
                        if let Some((id, _, _)) = listed.get(cursor) {
                            app.selected_task = Some(id.clone());
                            if app.focus_session.is_some() {
                                app.add_selected_to_focus_queue();
                            } else {
                                app.start_focus_on_selected(true);
                            }
                        }
                    }
                    "d" => app.request_focus_completion(true),
                    "p" => app.request_focus_completion(false),
                    "s" => app.stop_focus(),
                    "space" => app.toggle_focus_pause(),
                    "cmd+w" => close = true,
                    _ => {}
                }
            }
            cx.notify();
        });
        let count = rows(&self.takt.read(cx).app).len();
        match key.as_str() {
            "j" | "down" if count > 0 => self.cursor = (self.cursor + 1).min(count - 1),
            "k" | "up" => self.cursor = self.cursor.saturating_sub(1),
            _ => self.cursor = self.cursor.min(count.saturating_sub(1)),
        }
        if close {
            window.remove_window();
        }
        cx.notify();
    }
}

impl Render for FocusPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = &self.takt.read(cx).app;
        let t = Tokens::of(&app.theme(), window.appearance(), &app.prefs.appearance);
        let status = app.focus_status();
        let listed = rows(app);
        let heading = if app.focus_session.is_some() {
            "Up next in this session"
        } else {
            "Start on…"
        };
        let prompt = app.focus_prompt.clone();
        div()
            .id("focus-panel")
            .key_context("FocusPanel")
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
            .text_size(px(13.))
            .child(
                div()
                    .p_3()
                    .border_b_1()
                    .border_color(t.border_variant)
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(match &status {
                        Some((title, _, _)) => div().truncate().child(title.clone()),
                        None => div().text_color(t.muted).child("Nothing running"),
                    })
                    .when_some(status.clone(), |d, (_, clock, over)| {
                        d.child(
                            div()
                                .font_family(super::MONO_FONT)
                                .text_size(px(26.))
                                .text_color(if over { t.warning } else { t.accent })
                                .child(clock),
                        )
                    }),
            )
            .when_some(prompt, |d, prompt| {
                let selected = prompt.selected();
                d.child(
                    div()
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .border_b_1()
                        .border_color(t.border_variant)
                        .child(div().child(format!(
                            "How did it go? ×{:.1} · {:.1} pts",
                            prompt.multiplier(),
                            prompt.points()
                        )))
                        .child(div().flex().flex_wrap().gap_1().children(
                            prompt.choices().into_iter().map(|choice| {
                                div()
                                    .px_2()
                                    .rounded(px(4.))
                                    .border_1()
                                    .border_color(if choice == selected {
                                        t.accent
                                    } else {
                                        t.border_variant
                                    })
                                    .text_size(px(12.))
                                    .child(format!("{} {}", choice.title(&prompt), choice.key()))
                            }),
                        ))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(t.placeholder)
                                .child("↑↓ score · ←→ choose · ↩ takes it"),
                        ),
                )
            })
            .child(
                div()
                    .px_3()
                    .pt_2()
                    .text_size(px(11.))
                    .text_color(t.placeholder)
                    .child(heading),
            )
            .child(
                div()
                    .id("focus-panel-rows")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_1()
                    .children(
                        listed
                            .into_iter()
                            .enumerate()
                            .map(|(ix, (_, title, list))| {
                                let here = ix == self.cursor;
                                div()
                                    .px_2()
                                    .py(px(3.))
                                    .rounded(px(4.))
                                    .border_1()
                                    .border_color(if here {
                                        t.accent
                                    } else {
                                        gpui::transparent_black()
                                    })
                                    .when(here, |d| d.bg(t.selected))
                                    .flex()
                                    .gap_2()
                                    .child(div().flex_1().min_w_0().truncate().child(title))
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(t.placeholder)
                                            .child(list),
                                    )
                            }),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(t.border_variant)
                    .text_size(px(11.))
                    .text_color(t.placeholder)
                    .child("↩ start or queue · d done · p log · space pause · s stop · esc hide"),
            )
    }
}
