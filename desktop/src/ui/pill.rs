//! The focus pill: a small strip that floats over every app while a block
//! runs, with the task's title and its clock (counting up for a timer, down
//! for a pomodoro or a break). It never takes the keyboard: it opens without
//! focus, as a non-activating panel, so the app in front stays in front.
//! Its buttons finish the block, pause or resume it, and grow it into the
//! focus panel, which shrinks back into it when put away: the panel opens
//! over the pill's place and the pill hides while it is up. A drag moves it.
//!
//! Like the panel it holds nothing but its drag: it reads the main window's
//! `App` through the `Takt` entity and redraws when that does. `Takt`
//! opens and closes it (`sync_pill`), always deferred, never inside its own
//! update.

use super::Takt;
use crate::platform::window_move;
use crate::state::focus::reading;
use crate::theme::Tokens;
use gpui::{
    App as GpuiApp, Bounds, Context, DispatchPhase, Entity, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, WeakEntity, Window, WindowBounds, WindowKind,
    WindowOptions, canvas, div, point, prelude::*, px, size,
};

pub const WIDTH: f32 = 300.;
pub const HEIGHT: f32 = 36.;

/// How far the pointer goes before a press is a drag rather than a click.
const DRAG_SLOP: f64 = 3.;

pub struct FocusPill {
    takt: Entity<Takt>,
    this: WeakEntity<Self>,
    /// The `NSWindow`, to move.
    ns_window: Option<usize>,
    /// While the button is down: where the pointer and the window started,
    /// and whether it has moved far enough to be a drag.
    drag: Option<Drag>,
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    pointer: (f64, f64),
    origin: (f64, f64),
    moved: bool,
}

/// Where the pill sits until it is dragged: the top centre of the main
/// display, below the menu bar.
pub fn default_bounds(cx: &GpuiApp) -> Bounds<Pixels> {
    let width = px(WIDTH);
    let origin = cx
        .primary_display()
        .map(|d| {
            let b = d.bounds();
            point(
                b.origin.x + (b.size.width - width) / 2.,
                b.origin.y + px(44.),
            )
        })
        .unwrap_or(point(px(600.), px(44.)));
    Bounds {
        origin,
        size: size(width, px(HEIGHT)),
    }
}

/// Opens the pill where it last was (`at`), else at its default place.
pub fn open(
    takt: Entity<Takt>,
    at: Option<Bounds<Pixels>>,
    cx: &mut GpuiApp,
) -> Option<gpui::WindowHandle<FocusPill>> {
    let bounds = Bounds {
        size: size(px(WIDTH), px(HEIGHT)),
        ..at.unwrap_or_else(|| default_bounds(cx))
    };
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        kind: WindowKind::PopUp,
        // Clear behind its rounded corners, which it draws itself.
        window_background: gpui::WindowBackgroundAppearance::Transparent,
        is_movable: true,
        is_resizable: false,
        is_minimizable: false,
        focus: false,
        show: true,
        ..Default::default()
    };
    cx.open_window(options, |window, cx| {
        let ns_window = window_move::ns_window(window);
        if let Some(ns_window) = ns_window {
            window_move::no_animation(ns_window);
            window_move::no_shadow(ns_window);
        }
        cx.new(|cx| {
            cx.observe(&takt, |_, _, cx| cx.notify()).detach();
            FocusPill {
                takt: takt.clone(),
                this: cx.entity().downgrade(),
                ns_window,
                drag: None,
            }
        })
    })
    .ok()
}

impl FocusPill {
    fn press(&mut self) {
        let Some(ns_window) = self.ns_window else {
            return;
        };
        self.drag = Some(Drag {
            pointer: window_move::mouse_location(),
            origin: window_move::origin(ns_window),
            moved: false,
        });
    }

    fn drag_to(&mut self, cx: &mut Context<Self>) {
        let (Some(ns_window), Some(drag)) = (self.ns_window, self.drag.as_mut()) else {
            return;
        };
        let (x, y) = window_move::mouse_location();
        let (dx, dy) = (x - drag.pointer.0, y - drag.pointer.1);
        if !drag.moved && dx.abs().max(dy.abs()) < DRAG_SLOP {
            return;
        }
        drag.moved = true;
        let (to_x, to_y) = (drag.origin.0 + dx, drag.origin.1 + dy);
        // Moved once this update is over: the move reaches GPUI, which
        // would otherwise find the app already borrowed.
        cx.spawn(async move |this, _| {
            if this.upgrade().is_some() {
                window_move::set_origin(ns_window, to_x, to_y);
            }
        })
        .detach();
    }

    /// A press let go: the drag, if there was one, is over. A click no
    /// longer grows the pill; its expand button does.
    fn release(&mut self, _: &mut Context<Self>) {
        self.drag = None;
    }
}

/// The pill's line: what runs on the left, its clock, and its buttons —
/// finish, pause or resume, and expand (or, `expanded`, in the panel,
/// collapse). The panel's own top line is this same line, so the two meet
/// without a jump as one grows out of the other.
pub fn line(app: &crate::state::App, t: Tokens, takt: &Entity<Takt>, expanded: bool) -> gpui::Div {
    let now = crate::state::store::now_ms();
    let session = app.focus_session.clone();
    let (clock, over) = session
        .as_ref()
        .map(|s| reading(s, now))
        .unwrap_or_default();
    let on_break = session.as_ref().is_some_and(|s| s.phase == "onBreak");
    let paused = session.as_ref().is_some_and(|s| s.paused_at_ms.is_some());
    let asking = app.focus_prompt.is_some();
    let title = if on_break {
        "Break".to_string()
    } else {
        app.running_title().unwrap_or_else(|| "Focus".into())
    };
    let clock_colour = if over {
        t.warning
    } else if paused || asking {
        t.muted
    } else {
        t.accent
    };
    // Each button acts once the click is over: this window is out of
    // GPUI's list while it handles one, and `Takt` would think it closed.
    let button = |id: &'static str, name: &'static str, act: fn(&mut Takt)| {
        let takt = takt.clone();
        div()
            .id(id)
            .flex_none()
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .text_color(t.muted)
            .hover(|d| d.bg(t.hover).text_color(t.text))
            .cursor_pointer()
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                let takt = takt.clone();
                cx.defer(move |cx| {
                    takt.update(cx, |takt, cx| {
                        act(takt);
                        takt.settle(cx);
                    })
                });
            })
            .child(super::icons::icon(name, 14.).text_color(t.muted))
    };
    div()
        .flex_none()
        .h(px(HEIGHT - 2.))
        .flex()
        .items_center()
        .gap_2()
        .pl_3()
        .pr(px(6.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .when(paused || asking, |d| d.text_color(t.muted))
                .child(title),
        )
        .child(
            div()
                .flex_none()
                .font_family(super::MONO_FONT)
                .text_color(clock_colour)
                .child(clock),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .child(button("pill-finish", "check", |takt| {
                    // Answered in the panel: "how did it go?"
                    takt.app.request_focus_completion(true);
                    takt.app.wants_panel = Some(true);
                }))
                .child(button(
                    "pill-pause",
                    if paused || on_break { "play" } else { "pause" },
                    |takt| {
                        let app = &mut takt.app;
                        if app
                            .focus_session
                            .as_ref()
                            .is_some_and(|s| s.phase == "onBreak")
                        {
                            app.end_focus_break();
                        } else {
                            app.toggle_focus_pause();
                        }
                    },
                ))
                .child(if expanded {
                    button("pill-collapse", "minimize-2", |takt| {
                        takt.app.wants_panel = Some(false)
                    })
                } else {
                    button("pill-expand", "maximize-2", |takt| {
                        takt.app.wants_panel = Some(true)
                    })
                }),
        )
}

/// The pill as drawn: its line, in its box. The panel wears it while it
/// grows out of the pill or shrinks back in.
pub fn face(app: &crate::state::App, t: Tokens, takt: &Entity<Takt>) -> gpui::Div {
    framed(
        t,
        div()
            .flex()
            .flex_col()
            .text_color(t.text)
            .font_family(super::UI_FONT)
            .text_size(px(13.))
            .child(line(app, t, takt, false)),
    )
}

/// A floating window's box: its edge drawn as a fill one pixel larger
/// than the surface inside it, rather than as a border over the surface,
/// so the surface's anti-aliased corners can't show past the edge.
pub fn framed(t: Tokens, inside: gpui::Div) -> gpui::Div {
    div()
        .size_full()
        .p(px(1.))
        .rounded(px(8.))
        .bg(t.border_variant)
        .child(
            inside
                .size_full()
                .overflow_hidden()
                .rounded(px(7.))
                .bg(t.surface),
        )
}

impl Render for FocusPill {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = &self.takt.read(cx).app;
        let t = Tokens::of(&app.theme(), window.appearance(), &app.prefs.appearance);
        let this = self.this.clone();
        face(app, t, &self.takt)
            .id("focus-pill")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|pill, _: &MouseDownEvent, _, _| pill.press()),
            )
            // The drag and the release are heard window-wide: the pointer
            // can outrun the window while it follows.
            .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        let moving = this.clone();
                        window.on_mouse_event(move |_: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble {
                                let _ = moving.update(cx, |pill, cx| pill.drag_to(cx));
                            }
                        });
                        let releasing = this.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                                let _ = releasing.update(cx, |pill, cx| pill.release(cx));
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}
