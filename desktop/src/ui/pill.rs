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
    /// Its hotkey gives it the keyboard: the button the keyboard is on,
    /// and the app to hand the keyboard back to.
    focus: gpui::FocusHandle,
    cursor: usize,
    return_to: Option<i32>,
    /// The flash a redraw is due for when it ends.
    flash_due: Option<i64>,
}

/// The pill's buttons, in order: pause or resume, finish in the middle,
/// and expand (in the panel, collapse).
const PAUSE: usize = 0;
const FINISH: usize = 1;
const EXPAND: usize = 2;

/// What a button does to the app: `which` is its place, `expanded` whether
/// it is drawn in the panel.
fn action(which: usize, expanded: bool) -> fn(&mut Takt) {
    match which {
        FINISH => |takt| {
            // Answered in the panel: "how did it go?"
            takt.app.request_focus_completion(true);
            takt.app.wants_panel = Some(true);
        },
        PAUSE => |takt| {
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
        _ if expanded => |takt| takt.app.wants_panel = Some(false),
        _ => |takt| takt.app.wants_panel = Some(true),
    }
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
            // The keyboard's button is lit only while it has the keyboard.
            cx.observe_window_activation(window, |_, _, cx| cx.notify())
                .detach();
            FocusPill {
                takt: takt.clone(),
                this: cx.entity().downgrade(),
                ns_window,
                drag: None,
                focus: cx.focus_handle(),
                cursor: FINISH,
                return_to: None,
                flash_due: None,
            }
        })
    })
    .ok()
}

impl FocusPill {
    /// The hotkey with the pill up: it takes the keyboard, on finish,
    /// while the app in front stays in front.
    pub fn take_keyboard(&mut self, return_to: Option<i32>, window: &mut Window) {
        self.return_to = return_to;
        self.cursor = FINISH;
        window.activate_window();
        window.focus(&self.focus);
    }

    pub fn has_keyboard(&self, window: &Window) -> bool {
        window.is_window_active() && self.focus.is_focused(window)
    }

    /// Hands the keyboard back to the app it came from: Esc, or the hotkey
    /// again.
    pub fn give_back(&mut self) {
        if let Some(ns_window) = self.ns_window {
            window_move::resign_key(ns_window);
        }
        if let Some(pid) = self.return_to.take() {
            crate::platform::activate_app(pid);
        }
    }

    fn key_down(&mut self, event: &gpui::KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = crate::keys::spell(&crate::keys::Press::from_keystroke(&event.keystroke))
        else {
            return;
        };
        cx.stop_propagation();
        let which = match key.as_str() {
            "left" | "h" | "shift+tab" => {
                self.cursor = self.cursor.saturating_sub(1);
                None
            }
            "right" | "l" | "tab" => {
                self.cursor = (self.cursor + 1).min(EXPAND);
                None
            }
            "enter" => Some(self.cursor),
            "space" | "p" => Some(PAUSE),
            "d" => Some(FINISH),
            "e" | "o" | "up" | "down" => Some(EXPAND),
            "escape" => {
                self.give_back();
                None
            }
            _ => None,
        };
        if let Some(which) = which {
            let act = action(which, false);
            let takt = self.takt.clone();
            cx.defer(move |cx| {
                takt.update(cx, |takt, cx| {
                    act(takt);
                    takt.settle(cx);
                })
            });
        }
        cx.notify();
    }

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

/// A redraw once a flash has been said long enough to go.
pub fn redraw_after_flash<V: 'static>(cx: &mut Context<V>) {
    cx.spawn(async move |this, cx| {
        cx.background_executor()
            .timer(std::time::Duration::from_millis(3600))
            .await;
        let _ = this.update(cx, |_, cx| cx.notify());
    })
    .detach();
}

/// The pill's line: what runs on the left, its clock, and its buttons —
/// finish, pause or resume, and expand (or, `expanded`, in the panel,
/// collapse). The panel's own top line is this same line, so the two meet
/// without a jump as one grows out of the other.
pub fn line(
    app: &crate::state::App,
    t: Tokens,
    takt: &Entity<Takt>,
    expanded: bool,
    chosen: Option<usize>,
) -> gpui::Div {
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
    let button = |id: &'static str, name: &'static str, which: usize| {
        let takt = takt.clone();
        let act = action(which, expanded);
        let on = chosen == Some(which);
        div()
            .id(id)
            .flex_none()
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .border_1()
            .border_color(if on {
                t.focus
            } else {
                gpui::transparent_black()
            })
            .when(on, |d| d.bg(t.selected))
            .hover(|d| d.bg(t.hover))
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
            .child(super::icons::icon(name, 14.).text_color(if on { t.text } else { t.muted }))
    };
    div()
        .flex_none()
        .h(px(HEIGHT - 2. - 2. * INSET))
        .flex()
        .items_center()
        .gap_2()
        .pl_3()
        .pr(px(6.))
        // What was just done is said where the title is, for a moment.
        .child(match app.current_flash() {
            Some(flash) => div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(t.accent)
                .child(flash.to_string()),
            None => div()
                .flex_1()
                .min_w_0()
                .truncate()
                .when(paused || asking, |d| d.text_color(t.muted))
                .child(title),
        })
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
                .child(button(
                    "pill-pause",
                    if paused || on_break { "play" } else { "pause" },
                    PAUSE,
                ))
                .child(button("pill-finish", "check", FINISH))
                .child(if expanded {
                    button("pill-collapse", "minimize-2", EXPAND)
                } else {
                    button("pill-expand", "maximize-2", EXPAND)
                }),
        )
}

/// The pill as drawn: its line, in its box. The panel wears it while it
/// grows out of the pill or shrinks back in.
pub fn face(
    app: &crate::state::App,
    t: Tokens,
    takt: &Entity<Takt>,
    chosen: Option<usize>,
) -> gpui::Div {
    framed(
        t,
        div()
            .flex()
            .flex_col()
            .text_color(t.text)
            .font_family(super::UI_FONT)
            .text_size(px(13.))
            .child(line(app, t, takt, false, chosen)),
    )
}

/// A floating window's box: its edge drawn as a fill one pixel larger
/// than the surface inside it, rather than as a border over the surface,
/// so the surface's anti-aliased corners can't show past the edge.
pub fn framed(t: Tokens, inside: gpui::Div) -> gpui::Div {
    // A clear pixel all round, so the edge never lies on the window's own
    // edge, where AppKit trims it.
    div().size_full().p(px(INSET)).child(
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
            ),
    )
}

/// The clear margin round a floating window's box.
pub const INSET: f32 = 1.;

impl Render for FocusPill {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let flash_at = self
            .takt
            .read(cx)
            .app
            .focus_flash
            .as_ref()
            .map(|(_, at)| *at);
        if flash_at.is_some() && flash_at != self.flash_due {
            self.flash_due = flash_at;
            redraw_after_flash(cx);
        }
        let app = &self.takt.read(cx).app;
        let t = Tokens::of(&app.theme(), window.appearance(), &app.prefs.appearance);
        let this = self.this.clone();
        let chosen = self.has_keyboard(window).then_some(self.cursor);

        face(app, t, &self.takt, chosen)
            .id("focus-pill")
            .key_context("FocusPill")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
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
