//! Takt's desktop app, on GPUI. The first slice of `docs/rust-desktop-plan.md`:
//! the sidebar's lists and a list's outline, driven from the keyboard, read
//! and written through `takt-core` as a Rust crate (no FFI). A write patches
//! the rows held in memory rather than reading the list again.
//!
//! `TAKT_DESKTOP_DB` names the database; without it the app's own is used.

mod model;
mod theme;

use gpui::{
    App, Application, Bounds, Context, FocusHandle, Focusable, KeyBinding, ScrollStrategy,
    SharedString, TitlebarOptions, UniformListScrollHandle, Window, WindowBounds, WindowOptions,
    actions, div, point, prelude::*, px, size, uniform_list,
};
use model::{Model, Pane};
use std::borrow::Cow;
use std::time::Instant;
use theme::Tokens;

actions!(
    takt,
    [Down, Up, FocusSidebar, FocusOutline, ToggleDone, Quit]
);

struct Desktop {
    model: Model,
    focus: FocusHandle,
    sidebar_scroll: UniformListScrollHandle,
    outline_scroll: UniformListScrollHandle,
    last_action_us: Option<u128>,
}

impl Desktop {
    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.timed(cx, |this| this.model.step(1));
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.timed(cx, |this| this.model.step(-1));
    }

    fn focus_sidebar(&mut self, _: &FocusSidebar, _: &mut Window, cx: &mut Context<Self>) {
        self.timed(cx, |this| this.model.pane = Pane::Sidebar);
    }

    fn focus_outline(&mut self, _: &FocusOutline, _: &mut Window, cx: &mut Context<Self>) {
        self.timed(cx, |this| this.model.pane = Pane::Outline);
    }

    fn toggle_done(&mut self, _: &ToggleDone, _: &mut Window, cx: &mut Context<Self>) {
        self.timed(cx, |this| this.model.toggle_selected_done());
    }

    /// Runs one keystroke's work, keeps the selection in view and asks for a
    /// frame. The time shown in the status bar is the model's, not GPUI's.
    fn timed(&mut self, cx: &mut Context<Self>, work: impl FnOnce(&mut Self)) {
        let started = Instant::now();
        work(self);
        self.last_action_us = Some(started.elapsed().as_micros());
        self.sidebar_scroll
            .scroll_to_item(self.model.list_index, ScrollStrategy::Top);
        self.outline_scroll
            .scroll_to_item(self.model.row_index, ScrollStrategy::Top);
        cx.notify();
    }

    /// Zed's title bar: the window's own controls on the left, the scope's
    /// name, nothing else.
    fn title_bar(&self, tokens: Tokens) -> impl IntoElement {
        div()
            .h(px(34.))
            .pl(px(80.))
            .flex()
            .items_center()
            .bg(tokens.chrome)
            .border_b_1()
            .border_color(tokens.border)
            .text_color(tokens.muted)
            .child(self.scope_title())
    }

    fn scope_title(&self) -> SharedString {
        self.model
            .lists
            .get(self.model.list_index)
            .map(|list| list.name.clone())
            .unwrap_or_default()
            .into()
    }

    /// The left dock, as Zed's project panel.
    fn sidebar(&self, tokens: Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.model.pane == Pane::Sidebar;
        div()
            .w(px(240.))
            .h_full()
            .flex()
            .flex_col()
            .pt_1()
            .bg(tokens.surface)
            .border_r_1()
            .border_color(tokens.border)
            .child(
                uniform_list(
                    "lists",
                    self.model.lists.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, _| {
                        range
                            .map(|ix| {
                                let list = &this.model.lists[ix];
                                let selected = ix == this.model.list_index;
                                row(
                                    ix,
                                    list.name.clone().into(),
                                    0,
                                    selected,
                                    active,
                                    false,
                                    tokens,
                                )
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(self.sidebar_scroll.clone())
                .flex_1(),
            )
    }

    /// The centre pane: a tab bar holding the one scope shown, and its
    /// outline beneath.
    fn outline(&self, tokens: Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.model.pane == Pane::Outline;
        div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .bg(tokens.background)
            .child(
                div()
                    .h(px(32.))
                    .flex()
                    .bg(tokens.surface)
                    .border_b_1()
                    .border_color(tokens.border)
                    .child(
                        div()
                            .h_full()
                            .px_3()
                            .flex()
                            .items_center()
                            .bg(tokens.background)
                            .border_r_1()
                            .border_color(tokens.border)
                            .mb(px(-1.))
                            .child(self.scope_title()),
                    ),
            )
            .child(
                uniform_list(
                    "outline",
                    self.model.rows.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, _| {
                        range
                            .map(|ix| {
                                let item = &this.model.rows[ix];
                                let selected = ix == this.model.row_index;
                                let done = item.task.status != "open";
                                row(
                                    ix,
                                    item.task.title.clone().into(),
                                    item.depth,
                                    selected,
                                    active,
                                    done,
                                    tokens,
                                )
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(self.outline_scroll.clone())
                .pt_1()
                .flex_1(),
            )
    }

    fn status_bar(&self, tokens: Tokens) -> impl IntoElement {
        let timing = self
            .last_action_us
            .map(|us| format!("last key {:.2} ms", us as f64 / 1000.0))
            .unwrap_or_default();
        let message = self.model.message.clone().unwrap_or_default();
        div()
            .h(px(26.))
            .px_2()
            .flex()
            .items_center()
            .justify_between()
            .bg(tokens.chrome)
            .border_t_1()
            .border_color(tokens.border_variant)
            .text_size(px(12.))
            .text_color(tokens.muted)
            .child(message)
            .child(timing)
    }
}

impl Focusable for Desktop {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Desktop {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::for_appearance(window.appearance());
        div()
            .key_context("Workspace")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::focus_sidebar))
            .on_action(cx.listener(Self::focus_outline))
            .on_action(cx.listener(Self::toggle_done))
            .size_full()
            .flex()
            .flex_col()
            .bg(tokens.background)
            .text_color(tokens.text)
            .font_family(UI_FONT)
            .text_size(px(14.))
            .child(self.title_bar(tokens))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .min_h_0()
                    .child(self.sidebar(tokens, cx))
                    .child(self.outline(tokens, cx)),
            )
            .child(self.status_bar(tokens))
    }
}

/// Zed's interface face is IBM Plex Sans (as "Zed Plex Sans") and its
/// default monospace is Lilex; both are bundled.
const UI_FONT: &str = "IBM Plex Sans";

/// A row as Zed's project panel draws one: flat, the selection a fill, and
/// an accent border while its pane has the keyboard.
fn row(
    ix: usize,
    text: SharedString,
    depth: usize,
    selected: bool,
    pane_active: bool,
    done: bool,
    tokens: Tokens,
) -> impl IntoElement {
    let mut element = div()
        .id(ix)
        .h(px(26.))
        .pl(px(12. + depth as f32 * 20.))
        .pr_3()
        .flex()
        .items_center()
        .border_1()
        .border_color(gpui::transparent_black())
        .hover(|style| style.bg(tokens.hover))
        .child(text);
    if done {
        element = element.text_color(tokens.placeholder).line_through();
    }
    if selected {
        element = element.bg(tokens.selected);
        if pane_active {
            element = element.border_color(tokens.accent);
        }
    }
    element
}

fn load_fonts(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../../Takt/Fonts/IBMPlexSans-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../../Takt/Fonts/IBMPlexSans-Medium.ttf")),
        Cow::Borrowed(include_bytes!("../../Takt/Fonts/IBMPlexSans-SemiBold.ttf")),
        Cow::Borrowed(include_bytes!("../../Takt/Fonts/IBMPlexSans-Bold.ttf")),
        Cow::Borrowed(include_bytes!("../../Takt/Fonts/IBMPlexSans-Italic.ttf")),
        Cow::Borrowed(include_bytes!("../../Takt/Fonts/Lilex-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../../Takt/Fonts/Lilex-Medium.ttf")),
    ];
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        eprintln!("takt-desktop: could not load the bundled fonts: {error}");
    }
}

fn main() {
    let launched = Instant::now();
    let model = match Model::open() {
        Ok(model) => model,
        Err(error) => {
            eprintln!("takt-desktop: {error}");
            std::process::exit(1);
        }
    };
    Application::new().run(move |cx: &mut App| {
        load_fonts(cx);
        cx.bind_keys([
            KeyBinding::new("j", Down, Some("Workspace")),
            KeyBinding::new("down", Down, Some("Workspace")),
            KeyBinding::new("k", Up, Some("Workspace")),
            KeyBinding::new("up", Up, Some("Workspace")),
            KeyBinding::new("h", FocusSidebar, Some("Workspace")),
            KeyBinding::new("left", FocusSidebar, Some("Workspace")),
            KeyBinding::new("l", FocusOutline, Some("Workspace")),
            KeyBinding::new("right", FocusOutline, Some("Workspace")),
            KeyBinding::new("enter", FocusOutline, Some("Workspace")),
            KeyBinding::new("x", ToggleDone, Some("Workspace")),
            KeyBinding::new("cmd-q", Quit, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        let bounds = Bounds::centered(None, size(px(1100.), px(720.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Takt".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(12.), px(11.))),
                    }),
                    ..Default::default()
                },
                |_, cx| {
                    cx.new(|cx| Desktop {
                        model,
                        focus: cx.focus_handle(),
                        sidebar_scroll: UniformListScrollHandle::new(),
                        outline_scroll: UniformListScrollHandle::new(),
                        last_action_us: None,
                    })
                },
            )
            .expect("open the window");
        window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx));
                cx.activate(true);
                if std::env::var_os("TAKT_DESKTOP_SMOKE").is_some() {
                    smoke(view, window, launched);
                }
            })
            .ok();
    });
}

/// `TAKT_DESKTOP_SMOKE=1`: exercises the model the way the keys would, waits
/// for the first frame, prints what it saw and quits. A check that needs no
/// one at the screen, and never types into whatever is in front.
fn smoke(view: &mut Desktop, window: &mut Window, launched: Instant) {
    let model = &mut view.model;
    let lists = model.lists.len();
    let mut steps = Vec::new();
    for _ in 0..lists.min(20) {
        let started = Instant::now();
        model.step(1);
        steps.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    steps.sort_by(f64::total_cmp);
    let median = steps.get(steps.len() / 2).copied().unwrap_or_default();
    let worst = steps.last().copied().unwrap_or_default();
    for _ in 0..lists {
        if !model.rows.is_empty() {
            break;
        }
        model.step(-1);
    }
    let rows = model.rows.len();
    model.pane = model::Pane::Outline;
    let mut write_ms = None;
    if std::env::var_os("TAKT_DESKTOP_SMOKE_WRITE").is_some() && rows > 0 {
        let started = Instant::now();
        model.toggle_selected_done();
        model.toggle_selected_done();
        write_ms = Some(started.elapsed().as_secs_f64() * 1000.0 / 2.0);
    }
    let message = model.message.clone();
    window.on_next_frame(move |_, cx| {
        println!(
            "smoke: lists {lists}, rows in the list ticked in {rows}, list switch median {median:.2} ms worst {worst:.2} ms, \
             tick {}, first frame {:.0} ms after launch{}",
            write_ms.map(|ms| format!("{ms:.2} ms")).unwrap_or_else(|| "not run".into()),
            launched.elapsed().as_secs_f64() * 1000.0,
            message.map(|m| format!(", error: {m}")).unwrap_or_default(),
        );
        cx.quit();
    });
}
