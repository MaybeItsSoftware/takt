//! The window: Zed's layout (title bar, left dock, centre pane, right dock,
//! status bar, an overlay at the top) drawn from `state::App`.
//!
//! Every key comes through `key_down`, spelled the way the catalogue spells
//! keys and run by `App::handle_key`, so the keymap is the core's, not a
//! second copy in GPUI's binding tables. Rendering reads only what is held.

pub mod board_links;
mod calendar;
mod celebrate;
mod chrome;
mod commitment;
mod dock;
pub mod icons;
mod log_time;
pub mod menus;
mod notes_editor;
pub mod panel;
pub mod pill;
mod repeat;
mod settings;
mod sync_page;
mod views;
mod widgets;

use crate::keys::{Press, spell};
use crate::overlay::{self, OverlayState};
use crate::state::board::QUADRANT_TITLES;
use crate::state::commands::{EditCursor, KeyOutcome};
use crate::state::inspector::{Stop, StopKind};
use crate::state::outline::OutlineRow;
use crate::state::sidebar::RowKind;
use crate::state::today::DayRow;
use crate::state::{App, Editing, Overlay, Region, Scope, ViewMode};
use crate::text_field::{FieldKey, TextField};
use crate::theme::Tokens;
use gpui::{
    AnyElement, App as GpuiApp, Bounds, ClickEvent, ClipboardItem, Context, EntityInputHandler,
    FocusHandle, Focusable, FontWeight, Hsla, KeyDownEvent, Pixels, ScrollStrategy, SharedString,
    UTF16Selection, UniformListScrollHandle, WeakEntity, Window, div, prelude::*, px, uniform_list,
};
use std::ops::Range;
use std::time::{Duration, Instant};
use takt_core::records::TaskRow;
use widgets::*;

const UI_FONT: &str = "IBM Plex Sans";
const MONO_FONT: &str = "Lilex";
const ROW_HEIGHT: f32 = 26.;
const TITLE_BAR_HEIGHT: f32 = 34.;
/// How narrow and how wide the side docks may be dragged.
const SIDEBAR_MIN: f32 = 180.;
const SIDEBAR_MAX: f32 = 520.;
const RIGHT_DOCK_MIN: f32 = 240.;
const RIGHT_DOCK_MAX: f32 = 640.;

/// A side dock's inner edge being dragged; nothing is drawn under the
/// pointer.
#[derive(Clone, Copy)]
pub(super) enum DockEdge {
    Left,
    Right,
}

impl gpui::Render for DockEdge {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
/// The sidebar's rows, as Zed's project panel spaces them.
const SIDEBAR_INSET: f32 = 10.;
const SIDEBAR_INDENT: f32 = 20.;

pub struct Takt {
    pub app: App,
    focus: FocusHandle,
    /// Itself, for the text fields' input handler to reach.
    this: WeakEntity<Self>,
    field: TextField,
    overlay: OverlayState,
    /// The settings page's search, and where its list is scrolled.
    settings_field: TextField,
    settings_scroll: gpui::ScrollHandle,
    /// The Google plugins folded away under Google in the settings sidebar.
    google_folded: bool,
    /// The open dropdown's filter and scroll on the settings page.
    settings_menu: settings::MenuView,
    /// The board's columns side to side, and each column's cards.
    board_scroll: gpui::ScrollHandle,
    card_scrolls: std::cell::RefCell<Vec<gpui::ScrollHandle>>,
    sidebar_scroll: UniformListScrollHandle,
    main_scroll: UniformListScrollHandle,
    /// The Markdown editor's two sides' scrolling.
    notes_view: notes_editor::NotesView,
    last_key_ms: Option<f64>,
    /// The floating focus panel, while it is open.
    panel: Option<gpui::WindowHandle<panel::FocusPanel>>,
    /// The app that was in front when the panel was summoned over it: put
    /// away, the panel hands the keyboard back there rather than to the
    /// main window.
    panel_return_to: Option<i32>,
    /// The focus pill, while a block runs, and whether one is on its way.
    pill: Option<gpui::WindowHandle<pill::FocusPill>>,
    pill_opening: bool,
    /// Where the pill last was, so the panel grows out of it and it comes
    /// back there.
    pill_at: Option<gpui::Bounds<gpui::Pixels>>,
    /// The main window, while it is open: closing it leaves the app in the
    /// menu bar, and this entity lives on for the next one.
    main: Option<gpui::WindowHandle<Takt>>,
    /// The settings window, while it is open: a second window onto this
    /// same entity, drawing only the settings page.
    settings_window: Option<gpui::WindowHandle<Takt>>,
    /// The settings window closed but not yet gone: it keeps drawing the
    /// settings page, never the main window's, until it is removed.
    settings_closing: Option<gpui::WindowId>,
    /// A settings window is on its way: asked for, not yet open. ⌘, comes
    /// by two routes (the menu's key and the window's), and a second must
    /// not open a second window, which would draw the main view.
    settings_opening: bool,
    /// The settings window is the one in front: sheets and pickers come
    /// up there rather than in the main window.
    settings_active: bool,
    /// ⌘, asked for the settings window to come forward.
    settings_wanted_front: bool,
    /// The status item and hotkeys are up (not in a smoke run).
    platform_live: bool,
    /// Where the board's link ends were drawn, for the lines over them.
    board_links: board_links::Shared,
    /// The board card and the card's subtask row under the pointer.
    hover_card: Option<String>,
    hover_row: Option<String>,
}

/// Keeps the one `Takt` alive while no window shows it.
pub struct Shell(pub gpui::Entity<Takt>);

impl gpui::Global for Shell {}

/// Opens the main window onto `takt`.
pub fn open_main(takt: &gpui::Entity<Takt>, cx: &mut GpuiApp) -> Option<gpui::WindowHandle<Takt>> {
    let bounds = Bounds::centered(None, gpui::size(px(1200.), px(780.)), cx);
    let root = takt.clone();
    let handle = cx
        .open_window(
            gpui::WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Takt".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(gpui::point(px(12.), px(11.))),
                }),
                ..Default::default()
            },
            move |_, _| root,
        )
        .ok()?;
    let focus = takt.read(cx).focus.clone();
    let _ = handle.update(cx, |_, window, _| window.focus(&focus));
    takt.update(cx, |this, _| this.main = Some(handle));
    crate::platform::set_regular(true);
    cx.activate(true);
    Some(handle)
}

/// After any window closes: with the main one gone the app keeps to the
/// menu bar, out of the Dock.
pub fn window_closed(cx: &mut GpuiApp) {
    let Some(takt) = cx.try_global::<Shell>().map(|shell| shell.0.clone()) else {
        return;
    };
    let main = takt.read(cx).main;
    let open = main.is_some_and(|handle| {
        cx.windows()
            .iter()
            .any(|w| w.window_id() == handle.window_id())
    });
    if !open {
        takt.update(cx, |this, _| this.main = None);
        // Out of the Dock only with the menu bar item there to come back by.
        if takt.read(cx).app.prefs.show_menu_bar_item {
            crate::platform::set_regular(false);
        }
    }
    // The panel put away shrinks back to the pill.
    takt.update(cx, |this, cx| this.sync_pill(cx));
    let settings = takt.read(cx).settings_window;
    if settings.is_some_and(|handle| {
        !cx.windows()
            .iter()
            .any(|w| w.window_id() == handle.window_id())
    }) {
        takt.update(cx, |this, cx| {
            this.settings_window = None;
            this.settings_active = false;
            this.app.close_settings_page();
            cx.notify();
        });
    }
}

impl Focusable for Takt {
    fn focus_handle(&self, _: &GpuiApp) -> FocusHandle {
        self.focus.clone()
    }
}

impl Takt {
    pub fn new(app: App, cx: &mut Context<Self>) -> Self {
        let mut this = Takt {
            app,
            focus: cx.focus_handle(),
            this: cx.entity().downgrade(),
            field: TextField::default(),
            overlay: OverlayState::default(),
            settings_field: TextField::default(),
            settings_scroll: gpui::ScrollHandle::new(),
            google_folded: false,
            settings_menu: Default::default(),
            board_scroll: gpui::ScrollHandle::new(),
            card_scrolls: Default::default(),
            sidebar_scroll: UniformListScrollHandle::new(),
            main_scroll: UniformListScrollHandle::new(),
            notes_view: Default::default(),
            last_key_ms: None,
            panel: None,
            panel_return_to: None,
            pill: None,
            pill_opening: false,
            pill_at: None,
            main: None,
            settings_window: None,
            settings_closing: None,
            settings_opening: false,
            settings_active: false,
            settings_wanted_front: false,
            platform_live: false,
            board_links: Default::default(),
            hover_card: None,
            hover_row: None,
        };
        this.rank_today(cx);
        this.poll_external_writes(cx);
        this
    }

    /// Another process's writes (the CLI, the MCP server, the Swift app,
    /// sync) are seen within a second.
    fn poll_external_writes(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let alive = this.update(cx, |this, cx| {
                    this.refresh_status();
                    this.app.sync.set_foreground(this.main.is_some());
                    this.app.sync.note_outbox();
                    this.app.google_note_change();
                    this.app.prune_celebrations();
                    if !this.app.lingering_since.is_empty() {
                        this.app.expire_lingering();
                    }
                    this.app.tick_reminders();
                    // A session ended or begun anywhere (a click, the CLI,
                    // another device) shows or puts away the pill.
                    this.sync_pill(cx);
                    if this.poll_notes_editor() {
                        cx.notify();
                    }
                    if this.app.check_external_writes() {
                        this.app.reload_today();
                        this.rank_today(cx);
                        cx.notify();
                    } else if this.app.tick_focus() {
                        cx.notify();
                    } else if this.app.focus_running() {
                        // The clock in the status bar, and the running
                        // block on the timeline once a minute.
                        if this.app.prefs.bottom_dock_visible
                            && this.app.prefs.bottom_dock_tab == "timeline"
                            && crate::state::store::now_ms() / 1000 % 60 == 0
                        {
                            this.app.reload_timeline();
                        }
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// The status item, the hotkeys, and the queue AppKit's callbacks
    /// leave their events in.
    pub fn start_platform(&mut self, cx: &mut Context<Self>) {
        self.platform_live = true;
        self.register_hotkeys();
        self.refresh_status();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(40))
                    .await;
                let events = crate::platform::drain();
                if events.is_empty() {
                    continue;
                }
                let alive = this.update(cx, |this, cx| {
                    for event in events {
                        this.platform_event(event, cx);
                    }
                    this.settle(cx);
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn register_hotkeys(&mut self) {
        // Not while one is being recorded: the old one would take the key.
        let wanted = if matches!(self.app.editing, Some(Editing::Hotkey(_))) {
            Vec::new()
        } else {
            self.app.wanted_hotkeys()
        };
        let refused = crate::platform::hotkeys::register(&wanted);
        self.app.hotkey_problems = refused;
    }

    fn refresh_status(&self) {
        if self.platform_live {
            crate::platform::status_item::update(self.app.status_snapshot());
        }
    }

    /// After something outside the main window's keys changed the state:
    /// the day reranked, windows asked for opened, the status item redrawn.
    pub fn settle(&mut self, cx: &mut Context<Self>) {
        self.rank_today(cx);
        self.open_requested_windows(cx);
        self.refresh_status();
        cx.notify();
    }

    fn platform_event(&mut self, event: crate::platform::Event, cx: &mut Context<Self>) {
        use crate::platform::Event;
        match event {
            Event::ShowWindow => self.show_main_window(cx),
            // The one hotkey shows the panel, or puts it away when it is up.
            // Up but behind another app's window, it is brought forward
            // and given the keyboard instead.
            // With only the pill up, it takes the keyboard, and a second
            // press hands the keyboard back; its expand button grows it.
            Event::FocusPanel => {
                if !self.panel_open(cx)
                    && let Some(pill) = self.pill.filter(|_| self.pill_open(cx))
                {
                    let keyed = pill
                        .update(cx, |pill, window, _| pill.has_keyboard(window))
                        .unwrap_or(false);
                    if keyed {
                        // Again: the keyboard goes back where it was.
                        cx.defer(move |cx| {
                            let _ = pill.update(cx, |pill, _, cx| {
                                pill.give_back();
                                cx.notify();
                            });
                        });
                    } else {
                        let back = crate::platform::frontmost_other_app();
                        cx.defer(move |cx| {
                            let _ = pill.update(cx, |pill, window, cx| {
                                pill.take_keyboard(back, window);
                                cx.notify();
                            });
                        });
                    }
                    return;
                }
                let active = self.panel.as_ref().and_then(|handle| {
                    handle
                        .update(cx, |_, window, _| window.is_window_active())
                        .ok()
                });
                self.app.wants_panel = Some(active != Some(true));
            }
            Event::Sync { workspace } => {
                if workspace {
                    self.app.reload_all();
                    self.app.reload_today();
                    self.rank_today(cx);
                }
                if let Some(notice) = self.app.sync.take_notice() {
                    self.app.message = Some(notice);
                }
                if matches!(
                    self.app.overlay,
                    Some(
                        Overlay::Settings
                            | Overlay::Picker(crate::state::pickers::Picker::Link { .. })
                    )
                ) {
                    self.overlay.refresh(&self.app);
                }
            }
            Event::Url(url) => {
                if url.starts_with(crate::sync::auth::CALLBACK) {
                    self.app.sync.callback(&url);
                } else if url.starts_with("takt://join/") {
                    self.app.join_from_link(&url);
                }
            }
            Event::Menu(action) => match action.as_str() {
                "show-window" => self.show_main_window(cx),
                "quick-capture" => self.app.wants_capture = true,
                "focus-panel" => self.app.wants_panel = Some(true),
                "settings" => {
                    self.open_settings_page();
                    self.open_requested_windows(cx);
                }
                "quit" => cx.quit(),
                crate::state::links::DRIVE_PICKED => self.app.take_drive_picks(),
                // The calendars fetched: the settings dropdown open on them
                // redraws with them; otherwise their picker opens.
                "pick-google-calendar"
                    if self
                        .app
                        .setting_menu()
                        .is_some_and(|m| m.row == "google-calendar-choose") => {}
                "pick-google-calendar" => {
                    self.app
                        .open_picker(crate::state::pickers::Picker::GoogleCalendar);
                    self.open_overlay_field();
                }
                // A reminder clicked: the task, in the window.
                open if open.starts_with("open:") => {
                    let id = &open["open:".len()..];
                    self.show_main_window(cx);
                    let list = self.app.store.core.task(id.into()).ok().flatten();
                    if let Some(task) = list {
                        self.app.reveal_task(id, &task.list_id);
                    }
                }
                other => {
                    self.app.status_action(other);
                }
            },
        }
    }

    /// ↑↓ move through an open context menu, Return runs, Esc closes; any
    /// other key closes it and goes on as usual (false).
    fn context_menu_key(&mut self, keystroke: &gpui::Keystroke) -> bool {
        let Some(menu) = self.app.context_menu.as_mut() else {
            return false;
        };
        match keystroke.key.as_str() {
            "down" => menu.step(1),
            "up" => menu.step(-1),
            "enter" => {
                if let Some(action) = menu.picked().map(|item| item.action.clone()) {
                    self.app.pick_from_menu(&action);
                    if self.app.overlay.is_some() {
                        self.open_overlay_field();
                    }
                    self.begin_editing();
                } else {
                    self.app.context_menu = None;
                }
            }
            "escape" => self.app.context_menu = None,
            _ => {
                self.app.context_menu = None;
                return false;
            }
        }
        true
    }

    /// What the platform smoke check prints.
    pub fn platform_report(&self, cx: &mut Context<Self>) -> String {
        let snapshot = self.app.status_snapshot();
        format!(
            "status \"{}\" ({} menu rows); hotkeys {:?}, refused {:?}; panel open {}; {}",
            snapshot.title,
            snapshot.menu.len(),
            self.app
                .wanted_hotkeys()
                .into_iter()
                .filter_map(|(_, s)| s)
                .collect::<Vec<_>>(),
            self.app.hotkey_problems,
            self.panel_open(cx),
            self.app.message.clone().unwrap_or_default(),
        )
    }

    fn panel_open(&self, cx: &mut Context<Self>) -> bool {
        self.panel
            .as_ref()
            .is_some_and(|handle| handle.update(cx, |_, _, _| ()).is_ok())
    }

    /// Brings the main window forward, opening a new one onto this same
    /// state if it was closed.
    ///
    /// Always once this update ends: the main window's root is this entity,
    /// so reaching the window (or drawing a new one) from inside one of its
    /// updates would lease it twice and abort.
    pub fn show_main_window(&mut self, cx: &mut Context<Self>) {
        let main = self.main;
        let takt = cx.entity();
        cx.defer(move |cx| {
            if let Some(handle) = main
                && handle
                    .update(cx, |_, window, _| window.activate_window())
                    .is_ok()
            {
                cx.activate(true);
                return;
            }
            takt.update(cx, |this, _| this.main = None);
            open_main(&takt, cx);
        });
    }

    /// What the state asked for that only the window can do: the focus
    /// panel opened or put away.
    pub fn open_requested_windows(&mut self, cx: &mut Context<Self>) {
        self.sync_settings_window(cx);
        if self.app.hotkeys_changed {
            self.app.hotkeys_changed = false;
            if self.platform_live {
                self.register_hotkeys();
            }
        }
        // Quick capture is the focus panel, typing to add: opened so, or
        // turned to it when it is up already.
        let capturing = std::mem::take(&mut self.app.wants_capture);
        if capturing {
            self.app.wants_panel = Some(true);
        }
        if let Some(wanted) = self.app.wants_panel.take() {
            let open = self
                .panel
                .as_ref()
                .is_some_and(|handle| handle.update(cx, |_, _, _| ()).is_ok());
            if open && !wanted {
                // Kept until its window closes, so the pill stays away
                // while the panel shrinks into its place.
                if let Some(handle) = self.panel {
                    let back = self.panel_return_to.take();
                    cx.defer(move |cx| {
                        let _ =
                            handle.update(cx, |panel, window, cx| panel.leave(back, window, cx));
                    });
                }
            } else if !open && wanted {
                self.panel_return_to = crate::platform::frontmost_other_app();
                // Opened once this update ends: the panel's first draw reads
                // this entity, which is leased while it updates.
                if let Some(at) = self
                    .pill
                    .and_then(|h| h.update(cx, |_, window, _| window.bounds()).ok())
                {
                    self.pill_at = Some(at);
                }
                let pill = self.pill_at;
                let grow = self.pill_open(cx);
                let takt = cx.entity();
                cx.defer(move |cx| {
                    let handle = panel::open(takt.clone(), capturing, pill, grow, cx);
                    // The pill hides while the panel grown out of it is up.
                    takt.update(cx, |this, cx| {
                        this.panel = handle;
                        this.sync_pill(cx);
                    });
                });
            } else if open && let Some(handle) = &self.panel {
                let _ = handle.update(cx, |panel, window, cx| {
                    if capturing {
                        panel.begin_capture();
                        cx.notify();
                    }
                    window.activate_window();
                });
                cx.activate(true);
            }
        }
        self.sync_pill(cx);
    }

    /// The pill up while a block runs (unless turned off), and away once
    /// the session ends. Both deferred: opening draws it, which reads this
    /// entity, and closing runs `window_closed`, which does too.
    pub fn sync_pill(&mut self, cx: &mut Context<Self>) {
        // The panel is the pill grown larger: one or the other, not both.
        let wanted =
            self.app.focus_session.is_some() && self.app.prefs.focus_panel && !self.panel_open(cx);
        let open = self.pill.is_some_and(|handle| {
            cx.windows()
                .iter()
                .any(|w| w.window_id() == handle.window_id())
        });
        if !open {
            self.pill = None;
        }
        if wanted && !open && !self.pill_opening {
            self.pill_opening = true;
            let takt = cx.entity();
            cx.defer(move |cx| {
                let at = takt.read(cx).pill_at;
                let handle = pill::open(takt.clone(), at, cx);
                takt.update(cx, |this, _| {
                    this.pill = handle;
                    this.pill_opening = false;
                });
            });
        } else if !wanted && let Some(handle) = self.pill.take() {
            if let Ok(at) = handle.update(cx, |_, window, _| window.bounds()) {
                self.pill_at = Some(at);
            }
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
    }

    fn pill_open(&self, cx: &mut Context<Self>) -> bool {
        self.pill.is_some_and(|handle| {
            cx.windows()
                .iter()
                .any(|w| w.window_id() == handle.window_id())
        })
    }

    /// `TAKT_DESKTOP_SMOKE_WRITE`: a timer started on the selection brings
    /// the pill up, and stopping it puts the pill away, each through the
    /// deferred open and close a real start and stop take.
    pub fn smoke_pill(&mut self, cx: &mut Context<Self>) {
        if self.app.selected().is_none() || self.app.focus_running() {
            println!("smoke pill: skipped (nothing selected, or a block already running)");
            return;
        }
        let pref = self.app.prefs.focus_panel;
        self.app.prefs.focus_panel = true;
        self.app.start_focus_on_selected(true);
        let timer = self
            .app
            .focus_session
            .as_ref()
            .is_some_and(crate::state::focus::is_timer);
        self.open_requested_windows(cx);
        let takt = cx.entity();
        cx.defer(move |cx| {
            let opened = takt.update(cx, |this, cx| this.pill_open(cx));
            takt.update(cx, |this, cx| {
                this.app.stop_focus();
                this.open_requested_windows(cx);
            });
            let takt = takt.clone();
            cx.defer(move |cx| {
                let (closed, stopped) = takt.update(cx, |this, cx| {
                    this.app.prefs.focus_panel = pref;
                    (!this.pill_open(cx), !this.app.focus_running())
                });
                println!(
                    "smoke pill: timer {timer}, pill opened {opened}, stopped {stopped}, pill closed {closed}"
                );
                // The panel's `n` then `p`: a task filed as quick capture
                // files it, found by the finder, started as a pomodoro.
                let line = takt.update(cx, |this, cx| {
                    let app = &mut this.app;
                    let Ok((id, title)) = app.quick_capture_task("Smoke panel task") else {
                        return "skipped (nowhere to file)".to_string();
                    };
                    let found = app.focus_finder(&title, None).iter().any(|hit| {
                        matches!(hit, crate::state::focus::FinderHit::Task { id: found, .. } if *found == id)
                    });
                    app.focus_from_panel(&id, crate::state::focus::FocusKind::Pomodoro);
                    let pomodoro = app
                        .focus_session
                        .as_ref()
                        .is_some_and(|s| !crate::state::focus::is_timer(s));
                    app.stop_focus();
                    this.open_requested_windows(cx);
                    format!(
                        "found {found}, pomodoro {pomodoro}, stopped {}",
                        !this.app.focus_running()
                    )
                });
                println!("smoke panel: {line}");
                // The panel's `o`: the panel goes and the main window shows
                // the task.
                let id = takt.read(cx).app.selected_task.clone();
                takt.update(cx, |this, cx| {
                    this.app.wants_panel = Some(true);
                    this.open_requested_windows(cx);
                });
                let takt = takt.clone();
                cx.defer(move |cx| {
                    let handle = takt.read(cx).panel;
                    let (Some(handle), Some(id)) = (handle, id) else {
                        println!("smoke panel open in takt: skipped");
                        return;
                    };
                    let _ = handle.update(cx, |panel, window, cx| {
                        panel.open_in_main(panel::Target::Task(id.clone()), window, cx)
                    });
                    let takt = takt.clone();
                    cx.defer(move |cx| {
                        let shown = takt.read(cx).app.selected_task.as_deref() == Some(id.as_str());
                        println!("smoke panel open in takt: selected {shown}");
                    });
                });
            });
        });
    }

    /// While the agent works, its output is read ten times a second.
    fn poll_agent(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let working = this.update(cx, |this, cx| {
                    if this.app.drain_agent() {
                        this.app.reload_today();
                        this.rank_today(cx);
                        cx.notify();
                    }
                    this.app.agents_working()
                });
                if !matches!(working, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    /// Ranks the day off the main thread when it has been marked stale.
    fn rank_today(&mut self, cx: &mut Context<Self>) {
        if !self.app.today_stale {
            return;
        }
        self.app.today_stale = false;
        let core = self.app.store.core.clone();
        let workspace = self.app.store.workspace_id.clone();
        let zone = self.app.store.zone.clone();
        let ranking = cx
            .background_executor()
            .spawn(async move { crate::state::today::compute(core, workspace, zone) });
        cx.spawn(async move |this, cx| {
            let today = ranking.await;
            this.update(cx, |this, cx| {
                this.app.apply_today(today);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn arm_sequence_release(&mut self, held_at: Instant, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(crate::keys::Sequence::TIMEOUT)
                .await;
            this.update(cx, |this, cx| {
                if this.app.release_held_key(held_at) {
                    this.after_change(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // MARK: Keys

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let started = Instant::now();
        let keystroke = &event.keystroke;
        if keystroke.modifiers.platform && keystroke.key == "q" {
            cx.quit();
            return;
        }
        if keystroke.modifiers.platform && keystroke.key == "," && self.app.editing.is_none() {
            self.open_settings_page();
            self.open_requested_windows(cx);
            cx.notify();
            return;
        }
        if keystroke.modifiers.platform && keystroke.key == "w" {
            let handle = window.window_handle();
            // Not through the root: that is this entity, which may be
            // mid-update when the deferral runs.
            cx.defer(move |cx| {
                let _ = cx.update_window(handle, |_, window, _| window.remove_window());
            });
            return;
        }
        if matches!(self.app.editing, Some(Editing::Hotkey(_))) {
            cx.stop_propagation();
            let spelled = crate::platform::hotkeys::spell(&Press::from_keystroke(keystroke));
            if self.app.record_hotkey_key(&spelled) && self.app.overlay.is_some() {
                self.open_overlay_field();
            }
            self.open_requested_windows(cx);
            cx.notify();
            return;
        }
        if self.app.context_menu.is_some() && self.context_menu_key(keystroke) {
            cx.stop_propagation();
            self.after_change(cx);
            self.open_requested_windows(cx);
            cx.notify();
            return;
        }
        let editing_before = self.app.editing.clone();
        let handled = if self.app.overlay.is_some() {
            self.overlay_key(keystroke, cx)
        } else if self.app.editing.is_some() {
            self.editing_key(keystroke, cx)
        } else if self.is_settings_window(window) {
            self.settings_key(keystroke, cx)
        } else {
            self.command_key(keystroke, cx)
        };
        if !handled {
            return;
        }
        cx.stop_propagation();
        // A draft moved by Tab or ⇧Tab is the same draft: what is typed
        // in it stays.
        let draft_moved = matches!(
            (&editing_before, &self.app.editing),
            (Some(Editing::NewTask { .. }), Some(Editing::NewTask { .. }))
        );
        if self.app.editing != editing_before && !draft_moved {
            self.begin_editing();
        }
        self.after_change(cx);
        self.open_requested_windows(cx);
        self.refresh_status();
        self.last_key_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
        cx.notify();
    }

    /// `TAKT_DESKTOP_SMOKE_ENTER`: Enter, a title and Enter, three times,
    /// through the window's own key handler (in process; no key reaches the
    /// system). Each step's time, in ms.
    pub fn smoke_new_tasks(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<String> {
        let mut said = Vec::new();
        let press = |this: &mut Self, key: &str, window: &mut Window, cx: &mut Context<Self>| {
            let event = KeyDownEvent {
                keystroke: gpui::Keystroke::parse(key).expect("a key"),
                is_held: false,
            };
            let started = Instant::now();
            this.key_down(&event, window, cx);
            started.elapsed().as_secs_f64() * 1000.0
        };
        // In a list's outline: Return on a task drafts below it, and Tab
        // while typing makes the draft a subtask of the task above.
        self.app.overlay = None;
        self.app.editing = None;
        self.app.region = crate::state::Region::Tasks;
        let list = self.app.lists_in_sidebar_order().into_iter().find(|id| {
            self.app.select_list(id);
            self.app.rows.iter().filter(|r| r.depth == 0).count() >= 2
        });
        if list.is_some() {
            self.app.select_view_mode(crate::state::ViewMode::Outline);
            self.app.select_row(0);
            let first = self.app.selected_task.clone();
            press(self, "enter", window, cx);
            self.field = TextField::new("smoke below");
            press(self, "enter", window, cx);
            let below = self.app.selected_task.clone();
            let ix = |app: &crate::state::App, id: &Option<String>| {
                app.rows
                    .iter()
                    .position(|r| Some(&r.task.id) == id.as_ref())
            };
            let first_ix = ix(&self.app, &first);
            let first_end = first_ix.map(|i| {
                let depth = self.app.rows[i].depth;
                i + 1
                    + self.app.rows[i + 1..]
                        .iter()
                        .take_while(|r| r.depth > depth)
                        .count()
            });
            said.push(format!(
                "outline below: right after {}",
                first_end.is_some() && ix(&self.app, &below) == first_end
            ));
            press(self, "enter", window, cx);
            self.field = TextField::new("smoke child");
            press(self, "tab", window, cx);
            let kept = self.field.text == "smoke child";
            press(self, "enter", window, cx);
            let child = self.app.selected_task.clone();
            let parent = child
                .as_ref()
                .and_then(|id| self.app.task(id))
                .and_then(|t| t.parent_task_id.clone());
            said.push(format!(
                "tab makes a subtask {}, keeping the typing {kept}",
                parent.is_some() && parent == below
            ));
            press(self, "escape", window, cx);
        }
        // On the board: a card in the focused column, three times over.
        self.app.overlay = None;
        self.app.editing = None;
        self.app.region = crate::state::Region::Tasks;
        self.app.select_everything();
        self.app.run("goBoard", None);
        let column = 1.min(self.app.board.columns.len().saturating_sub(1));
        self.app.board.column = column;
        self.app.selected_task = self
            .app
            .board
            .cards
            .get(column)
            .and_then(|cards| cards.first())
            .or_else(|| self.app.board.cards.iter().flatten().next())
            .map(|card| card.id.clone());
        let open = press(self, "enter", window, cx);
        said.push(format!(
            "open {open:.1} ({:?}, {:?})",
            self.app.view_mode, self.app.editing
        ));
        // The draft stays open below each new card for the next.
        for n in 0..3 {
            self.field = TextField::new(&format!("smoke card {n}"));
            let enter = press(self, "enter", window, cx);
            let landed = self.app.selected_task.as_ref().and_then(|id| {
                self.app
                    .board
                    .cards
                    .iter()
                    .position(|cards| cards.iter().any(|card| &card.id == id))
            });
            said.push(format!(
                "enter {enter:.1} (card in column {landed:?} of wanted {column}, draft open: {})",
                self.app.editing.is_some()
            ));
        }
        let escape = press(self, "escape", window, cx);
        // On the matrix: a card in the selected card's quadrant.
        self.app.run("goMatrix", None);
        if self.app.board.quadrants.iter().all(Vec::is_empty) {
            self.app.selected_task = self.app.board.unplaced.first().map(|c| c.id.clone());
            self.app.place_in_quadrant(1);
        }
        let quadrant = self
            .app
            .board
            .quadrants
            .iter()
            .position(|cards| !cards.is_empty());
        self.app.selected_task = quadrant
            .and_then(|q| self.app.board.quadrants[q].first())
            .map(|card| card.id.clone());
        press(self, "enter", window, cx);
        self.field = TextField::new("smoke matrix card");
        let enter = press(self, "enter", window, cx);
        let landed = self.app.selected_task.as_ref().and_then(|id| {
            self.app
                .board
                .quadrants
                .iter()
                .position(|cards| cards.iter().any(|card| &card.id == id))
        });
        said.push(format!(
            "matrix enter {enter:.1} (card in quadrant {landed:?} of wanted {quadrant:?})"
        ));
        press(self, "escape", window, cx);
        said.push(format!("escape {escape:.1}"));
        said
    }

    fn command_key(&mut self, keystroke: &gpui::Keystroke, cx: &mut Context<Self>) -> bool {
        let Some(key) = spell(&Press::from_keystroke(keystroke)) else {
            return false;
        };
        let overlay_before = self.app.overlay.clone();
        let outcome = self.app.handle_key(&key, Instant::now());
        if self.app.overlay.is_some() && self.app.overlay != overlay_before {
            self.open_overlay_field();
        }
        match outcome {
            // The timeline owns the keyboard: what it does not use is
            // swallowed rather than reaching the panes behind it.
            KeyOutcome::Unhandled => self
                .app
                .bindings
                .swallows_unhandled(&key, self.app.surface()),
            KeyOutcome::Handled => true,
            KeyOutcome::Held(at) => {
                self.arm_sequence_release(at, cx);
                true
            }
        }
    }

    /// A fresh overlay: its field empty, or holding an edit's word.
    fn open_overlay_field(&mut self) {
        self.overlay.reset();
        if let Some(prefill) = self.app.palette_prefill.take() {
            self.overlay.field = TextField::new(&prefill);
        }
        self.overlay.refresh(&self.app);
    }

    /// What the field did with a key that was the clipboard's: true when it
    /// changed.
    pub(super) fn clipboard_key(field: &mut TextField, key: &FieldKey, cx: &mut GpuiApp) -> bool {
        match key {
            FieldKey::Copy | FieldKey::Cut => {
                let selected = field.selected_text().to_string();
                if !selected.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(selected));
                    if *key == FieldKey::Cut {
                        field.insert("");
                        return true;
                    }
                }
                false
            }
            FieldKey::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    field.insert(&text);
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Returns false for a printable key, which the input method types.
    fn overlay_key(&mut self, keystroke: &gpui::Keystroke, cx: &mut Context<Self>) -> bool {
        let ctrl = keystroke.modifiers.control;
        match (keystroke.key.as_str(), ctrl) {
            ("n", true) | ("j", true) => {
                self.overlay.step(1);
                return true;
            }
            ("p", true) | ("k", true) => {
                self.overlay.step(-1);
                return true;
            }
            _ => {}
        }
        let outcome = self.overlay.field.handle(keystroke);
        if Self::clipboard_key(&mut self.overlay.field, &outcome, cx) {
            self.overlay.index = 0;
            self.overlay.refresh(&self.app);
            return true;
        }
        match outcome {
            FieldKey::Text(_) => return false,
            FieldKey::Copy | FieldKey::Cut | FieldKey::Paste => {}
            FieldKey::Commit => {
                let pick = self.overlay.picked().unwrap_or(overlay::Pick::Nothing);
                self.overlay.reset();
                overlay::apply(&mut self.app, pick);
                if self.app.overlay.is_some() {
                    self.overlay.refresh(&self.app);
                }
            }
            FieldKey::Cancel => {
                self.overlay.reset();
                self.app.close_overlay();
            }
            FieldKey::Down | FieldKey::Tab { shift: false } => self.overlay.step(1),
            FieldKey::Up | FieldKey::Tab { shift: true } => self.overlay.step(-1),
            FieldKey::Edited => {
                self.overlay.index = 0;
                self.overlay.refresh(&self.app);
            }
            FieldKey::Ignored => {
                // ⌘K from search opens the palette over it, as Zed's does.
                if let Some(key) = spell(&Press::from_keystroke(keystroke))
                    && let Some(command) = self.app.bindings.command(&key, self.app.surface())
                    && command.id.starts_with("go")
                    && matches!(
                        command.id.as_str(),
                        "goCommandPalette" | "goSearch" | "goListNavigator" | "goKeyboardReference"
                    )
                {
                    let id = command.id.clone();
                    self.app.run(&id, Some(&key));
                    self.overlay.reset();
                    self.overlay.refresh(&self.app);
                }
            }
        }
        true
    }

    fn editing_key(&mut self, keystroke: &gpui::Keystroke, cx: &mut Context<Self>) -> bool {
        if matches!(self.app.editing, Some(Editing::NotesEditor)) {
            return self.notes_editor_key(keystroke, cx);
        }
        // From the inspector's notes field, the Markdown editor's chord
        // keeps what was typed and maximises it.
        if let Some(Editing::InspectorField { stop: "notes", .. }) = self.app.editing
            && (keystroke.modifiers.platform || keystroke.modifiers.control)
            && let Some(key) = spell(&Press::from_keystroke(keystroke))
            && self
                .app
                .bindings
                .command(&key, self.app.surface())
                .is_some_and(|c| c.id == "taskMaximiseNotes")
            && self.app.notes_editor_on()
        {
            let text = self.field.text.clone();
            self.app.editing = None;
            self.app.commit_inspector_field("notes", &text);
            self.app.open_notes_editor();
            return true;
        }
        // The inspector's calendar takes its keys whole: nothing is typed.
        if matches!(self.app.editing, Some(Editing::Date(_))) {
            if let Some(key) = spell(&Press::from_keystroke(keystroke)) {
                self.app.date_key(&key);
            }
            return true;
        }
        if matches!(self.app.editing, Some(Editing::Commitment)) {
            return self.commitment_key(keystroke, cx);
        }
        // So does the repeat selector: only the count's digits are typed.
        if matches!(self.app.editing, Some(Editing::Repeat(_))) {
            if let Some(key) = spell(&Press::from_keystroke(keystroke)) {
                self.app.repeat_key(&key);
            }
            return true;
        }
        // And the log-time selector: its digits are the minutes.
        if matches!(self.app.editing, Some(Editing::Log(_))) {
            if let Some(key) = spell(&Press::from_keystroke(keystroke)) {
                self.app.log_key(&key);
            }
            return true;
        }
        if matches!(self.app.editing, Some(Editing::AgentMessage)) {
            let m = &keystroke.modifiers;
            match keystroke.key.as_str() {
                "enter" if !m.shift && !m.alt => {
                    let text = std::mem::take(&mut self.field.text);
                    self.field = TextField::multiline("");
                    self.app.send_to_agent(&text);
                    self.poll_agent(cx);
                    return true;
                }
                "enter" => {
                    self.field.insert("\n");
                    return true;
                }
                "escape" => {
                    self.app.editing = None;
                    self.app.region = Region::Tasks;
                    return true;
                }
                "n" if m.platform => {
                    self.app.agent.reset();
                    self.field = TextField::multiline("");
                    return true;
                }
                "c" if m.control && self.app.agent.working => {
                    self.app.agent.end();
                    self.app
                        .agent
                        .entries
                        .push(crate::state::agent::Entry::Notice {
                            text: "Stopped.".into(),
                            error: false,
                        });
                    return true;
                }
                _ => {}
            }
        }
        // The rules editor's Esc leaves it unsaved: rules that don't read
        // yet would otherwise hold it open.
        if keystroke.key == "escape" && matches!(self.app.editing, Some(Editing::Rules(_))) {
            self.app.editing = None;
            self.app.message = None;
            return true;
        }
        let outcome = self.field.handle(keystroke);
        if Self::clipboard_key(&mut self.field, &outcome, cx) {
            return true;
        }
        match outcome.clone() {
            FieldKey::Text(_) => false,
            FieldKey::Copy | FieldKey::Cut | FieldKey::Paste => true,
            FieldKey::Commit => {
                let text = self.field.text.clone();
                match self.app.editing.clone() {
                    Some(Editing::TaskTitle { task_id }) => {
                        self.app.rename_task(&task_id, &text);
                        self.app.editing = None;
                    }
                    Some(Editing::NewTask { .. }) => {
                        if self.app.create_task(&text) {
                            self.field = TextField::default();
                        }
                    }
                    Some(Editing::NewCard { .. }) => {
                        if self.app.create_card(&text) {
                            self.field = TextField::default();
                        }
                    }
                    Some(Editing::InspectorField { stop, .. }) => {
                        self.app.editing = None;
                        self.app.commit_inspector_field(stop, &text);
                    }
                    Some(Editing::ColumnName(target)) => {
                        self.app.editing = None;
                        self.app.commit_column_name(target, &text);
                    }
                    Some(Editing::Rules(target)) => {
                        if self.app.commit_rules(target, &text) {
                            self.app.editing = None;
                        }
                    }
                    Some(Editing::ConditionTime) => {
                        self.app.editing = None;
                        self.app.commit_typed_time(&text);
                    }
                    Some(Editing::ConditionName(id)) => {
                        self.app.editing = None;
                        self.app.commit_condition_name(id, &text);
                    }
                    Some(Editing::Ask(ask)) => {
                        self.app.answer(ask, &text);
                        if self.app.overlay.is_some() {
                            self.open_overlay_field();
                        }
                        if self.app.agents_working() {
                            self.poll_agent(cx);
                        }
                    }
                    Some(Editing::FocusNote) => {
                        self.app.editing = None;
                        if let Some(prompt) = self.app.focus_prompt.as_mut() {
                            prompt.note = text;
                        }
                    }
                    Some(Editing::AgentMessage) => {
                        self.app.send_to_agent(&text);
                        self.field = TextField::multiline("");
                        self.poll_agent(cx);
                    }
                    Some(Editing::CategoryName(from)) => {
                        self.app.editing = None;
                        self.app.commit_category_name(from, &text);
                    }
                    Some(_) => self.app.commit_sidebar_name(&text),
                    None => {}
                }
                true
            }
            FieldKey::Cancel => {
                if matches!(self.app.editing, Some(Editing::Ask(_))) {
                    self.app.leave_ask();
                    self.open_overlay_field();
                } else {
                    self.app.editing = None;
                }
                true
            }
            FieldKey::Tab { .. } | FieldKey::Up | FieldKey::Down
                if matches!(self.app.editing, Some(Editing::InspectorField { .. })) =>
            {
                // In the inspector Tab, and ↑↓ in a one-line field, save
                // and move on.
                let by = match outcome {
                    FieldKey::Tab { shift } => {
                        if shift {
                            -1
                        } else {
                            1
                        }
                    }
                    FieldKey::Up => -1,
                    _ => 1,
                };
                if let Some(Editing::InspectorField { stop, .. }) = self.app.editing.take() {
                    let text = self.field.text.clone();
                    self.app.commit_inspector_field(stop, &text);
                }
                self.app.step_inspector(by);
                true
            }
            FieldKey::Tab { shift } => {
                // In a draft: Tab files it under the task above, ⇧Tab
                // brings it back out a level.
                if let Some(Editing::NewTask {
                    list_id,
                    parent_id,
                    after,
                }) = self.app.editing.clone()
                {
                    let next = if shift {
                        parent_id.clone().map(|parent| {
                            let grand = self
                                .app
                                .task(&parent)
                                .and_then(|task| task.parent_task_id.clone());
                            (grand, Some(parent))
                        })
                    } else {
                        after.clone().map(|above| (Some(above), None))
                    };
                    if let Some((parent_id, after)) = next {
                        self.app.editing = Some(Editing::NewTask {
                            list_id,
                            parent_id,
                            after,
                        });
                    }
                }
                true
            }
            FieldKey::Edited | FieldKey::Up | FieldKey::Down => true,
            FieldKey::Ignored => {
                // Chords that reach out of a text field (the catalogue
                // says which) still run.
                let Some(key) = spell(&Press::from_keystroke(keystroke)) else {
                    return false;
                };
                let surface = self.app.surface();
                if self.app.bindings.reaches_into_text_field(&key, surface) {
                    self.app.editing = None;
                    self.app.dispatch(&key);
                    if self.app.overlay.is_some() {
                        self.overlay.reset();
                        self.overlay.refresh(&self.app);
                    }
                    return true;
                }
                false
            }
        }
    }

    /// A text field opened: fill it with what it edits.
    fn begin_editing(&mut self) {
        self.field = match &self.app.editing {
            Some(Editing::TaskTitle { task_id }) => {
                let title = self
                    .app
                    .task(task_id)
                    .map(|task| task.title.clone())
                    .unwrap_or_default();
                match self.app.edit_cursor {
                    EditCursor::SelectAll => TextField::selecting_all(&title),
                    EditCursor::Start => TextField::at_start(&title),
                    EditCursor::End => TextField::new(&title),
                    EditCursor::Empty => TextField::new(""),
                }
            }
            Some(Editing::SidebarName { row_id }) => {
                let name = self
                    .app
                    .sidebar_rows
                    .iter()
                    .find(|row| &row.id == row_id)
                    .map(|row| self.row_title(&row.kind))
                    .unwrap_or_default();
                TextField::selecting_all(&name)
            }
            Some(Editing::ColumnName(crate::state::columns::ColumnName::Rename(id))) => {
                let title = self
                    .app
                    .board
                    .layout
                    .iter()
                    .find(|c| c.id == *id)
                    .map(|c| c.title.clone())
                    .unwrap_or_default();
                TextField::selecting_all(&title)
            }
            Some(Editing::Rules(target)) => TextField::multiline(&self.app.rules_text(target)),
            Some(Editing::Commitment) => TextField::new(
                self.app
                    .commitment_form
                    .as_ref()
                    .map_or("", |f| f.title.as_str()),
            ),
            Some(Editing::ConditionTime) => TextField::new(
                self.app
                    .conditions
                    .time_entry
                    .as_deref()
                    .unwrap_or_default(),
            ),
            Some(Editing::ConditionName(Some(id))) => TextField::selecting_all(
                &self
                    .app
                    .condition(id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default(),
            ),
            Some(Editing::CategoryName(Some(category))) => TextField::selecting_all(category),
            Some(Editing::Ask(ask)) => TextField::selecting_all(&self.app.ask_prefill(ask)),
            Some(Editing::FocusNote) => TextField::new(
                &self
                    .app
                    .focus_prompt
                    .as_ref()
                    .map(|p| p.note.clone())
                    .unwrap_or_default(),
            ),
            Some(Editing::AgentMessage) => {
                TextField::multiline(&self.app.agent_draft.take().unwrap_or_default())
            }
            Some(Editing::NotesEditor) => TextField::multiline(
                &self
                    .app
                    .notes_editor
                    .as_ref()
                    .map(|e| e.saved.clone())
                    .unwrap_or_default(),
            ),
            Some(Editing::InspectorField { stop, .. }) => {
                let stop = *stop;
                let text = self.app.inspector_field_text(stop);
                if matches!(stop, "notes" | "links") {
                    TextField::multiline(&text)
                } else {
                    TextField::new(&text)
                }
            }
            _ => TextField::default(),
        };
    }

    /// After anything changed: rank the day if it went stale, and keep the
    /// selection and the sidebar cursor on screen.
    fn after_change(&mut self, cx: &mut Context<Self>) {
        // A click away from a sidebar opened for a visit closes it.
        self.app.end_sidebar_peek();
        self.rank_today(cx);
        if let Some(index) = self.main_row_index() {
            self.main_scroll
                .scroll_to_item(index, ScrollStrategy::Center);
        }
        // The board: the focused column into view side to side, and the
        // selected card within it.
        if self.app.view_mode == crate::state::ViewMode::Board {
            let column = self.app.board.column;
            self.board_scroll.scroll_to_item(column);
            let card = self.app.selected_task.as_ref().and_then(|id| {
                self.app
                    .board
                    .cards
                    .get(column)?
                    .iter()
                    .position(|task| &task.id == id)
            });
            if let (Some(card), Some(handle)) = (card, self.card_scrolls.borrow().get(column)) {
                handle.scroll_to_item(card);
            }
        }
        if let Some(index) = self
            .app
            .sidebar_cursor_row()
            .and_then(|row| self.app.sidebar_rows.iter().position(|r| r.id == row.id))
        {
            self.sidebar_scroll
                .scroll_to_item(index, ScrollStrategy::Center);
        }
    }

    fn main_row_index(&self) -> Option<usize> {
        if self.app.view_mode != ViewMode::Outline {
            return None;
        }
        let id = self.app.selected_task.as_deref()?;
        self.outline_display()
            .iter()
            .position(|row| matches!(row, Display::Task(ix) if self.app.rows[*ix].task.id == id))
    }

    fn row_title(&self, kind: &RowKind) -> String {
        match kind {
            RowKind::Today => "Today".into(),
            RowKind::Everything => "Everything".into(),
            RowKind::List(id) => self
                .app
                .list(id)
                .map(|l| l.name.clone())
                .unwrap_or_default(),
            RowKind::NestedList(id) => self
                .app
                .nested_lists
                .iter()
                .find(|item| &item.task.id == id)
                .map(|item| item.task.title.clone())
                .unwrap_or_default(),
            RowKind::Folder(id) => self
                .app
                .folder(id)
                .map(|f| f.name.clone())
                .unwrap_or_default(),
            RowKind::FilterList(id) => self
                .app
                .filter_lists
                .iter()
                .find(|f| &f.id == id)
                .map(|f| f.name.clone())
                .unwrap_or_default(),
        }
    }

    /// The outline's rows with the draft row, when one is open, in place.
    fn outline_display(&self) -> Vec<Display> {
        let mut display: Vec<Display> = (0..self.app.rows.len()).map(Display::Task).collect();
        if let Some(Editing::NewTask {
            parent_id, after, ..
        }) = &self.app.editing
        {
            let rows = &self.app.rows;
            let at = match after {
                Some(after) => rows
                    .iter()
                    .position(|row| &row.task.id == after)
                    .map(|ix| {
                        let depth = rows[ix].depth;
                        ix + 1
                            + rows[ix + 1..]
                                .iter()
                                .take_while(|r| r.depth > depth)
                                .count()
                    })
                    .unwrap_or(rows.len()),
                None => parent_id
                    .as_ref()
                    .and_then(|parent| rows.iter().position(|row| &row.task.id == parent))
                    .map_or(0, |ix| ix + 1),
            };
            let depth = parent_id
                .as_ref()
                .and_then(|parent| rows.iter().find(|row| &row.task.id == parent))
                .map_or(
                    after
                        .as_ref()
                        .and_then(|a| rows.iter().find(|row| &row.task.id == a))
                        .map_or(0, |row| row.depth),
                    |parent| parent.depth + 1,
                );
            display.insert(at.min(display.len()), Display::Draft(depth));
        }
        display
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Display {
    Task(usize),
    Draft(i64),
}

/// The window's panes, for a click to give one the keyboard.
#[derive(Clone, Copy)]
enum Pane {
    Left,
    Centre,
    Right,
    Bottom,
}

impl Takt {
    /// `content` with a click anywhere on it giving it the keyboard; what
    /// the click lands on still gets it too.
    fn pane(
        &self,
        pane: Pane,
        content: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex_none()
            .flex()
            // The bottom dock spans the centre; the side docks keep their
            // width and run the window's height.
            .when(matches!(pane, Pane::Bottom), |d| d.w_full().flex_col())
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _: &gpui::MouseDownEvent, _, cx| this.focus_pane(pane, cx)),
            )
            .child(content)
    }

    /// The region a pane is, given what it is showing.
    fn focus_pane(&mut self, pane: Pane, cx: &mut Context<Self>) {
        let p = &self.app.prefs;
        let region = match pane {
            Pane::Left if p.left_dock_tab == "agent" => Some(Region::Agent),
            Pane::Left => Some(Region::Sidebar),
            Pane::Centre => Some(Region::Tasks),
            Pane::Right => crate::state::docks::tab_region(&p.right_dock_tab)
                .filter(|r| *r != Region::Inspector || self.app.can_inspect()),
            Pane::Bottom => crate::state::docks::tab_region(&p.bottom_dock_tab),
        };
        let Some(region) = region else { return };
        if region == self.app.region {
            return;
        }
        if self.app.region == Region::Agent
            && matches!(self.app.editing, Some(Editing::AgentMessage))
        {
            self.app.editing = None;
        }
        if region == Region::Inspector {
            self.app.enter_inspector_region();
        } else {
            self.app.region = region;
        }
        if region == Region::Agent && self.app.editing.is_none() {
            self.app.editing = Some(Editing::AgentMessage);
            self.begin_editing();
        }
        cx.notify();
    }
}

// MARK: Rendering

impl Render for Takt {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Tokens::of(
            &self.app.theme(),
            window.appearance(),
            &self.app.prefs.appearance,
        );
        let settings = self.is_settings_window(window);
        // Sheets and pickers come up in the window in front.
        let popups = if settings {
            self.settings_active
        } else {
            self.settings_window.is_none() || !self.settings_active
        };
        let inspector = if self.app.prefs.right_dock_visible && !settings {
            Some(
                self.app
                    .draft_inspector_stops(&self.field.text)
                    .unwrap_or_else(|| self.app.inspector_stops()),
            )
        } else {
            None
        };
        div()
            .id("takt")
            .key_context("Takt")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            // A side dock's edge, dragged: the width follows the pointer
            // wherever it goes, and is kept once it is let go.
            .on_drag_move(
                cx.listener(|this, e: &gpui::DragMoveEvent<DockEdge>, window, cx| {
                    let x = f32::from(e.event.position.x);
                    let edge = *e.drag(cx);
                    let p = &mut this.app.prefs;
                    let (width, wanted) = match edge {
                        DockEdge::Left => (&mut p.sidebar_width, x.clamp(SIDEBAR_MIN, SIDEBAR_MAX)),
                        DockEdge::Right => (
                            &mut p.right_dock_width,
                            (f32::from(window.viewport_size().width) - x)
                                .clamp(RIGHT_DOCK_MIN, RIGHT_DOCK_MAX),
                        ),
                    };
                    if (*width - wanted).abs() >= 0.5 {
                        *width = wanted;
                        cx.notify();
                    }
                }),
            )
            .on_drop(cx.listener(|this, _: &DockEdge, _, cx| {
                this.app.prefs.save();
                cx.notify();
            }))
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .bg(t.background)
            .text_color(t.text)
            .font_family(UI_FONT)
            .text_size(px(13.))
            .when(settings, |d| d.child(self.settings_page(t, cx)))
            .when(!settings, |d| {
                let bottom = (self.app.prefs.bottom_dock_visible)
                    .then(|| self.bottom_dock(t, cx).into_any_element());
                d.child(self.title_bar(t, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .when(self.app.prefs.sidebar_visible, |d| {
                                let dock = self.left_dock(t, cx);
                                d.child(self.pane(Pane::Left, dock, cx))
                            })
                            // The bottom dock sits under the centre, between the
                            // side docks, as Zed's does: they run the full height.
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .id("centre")
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .on_mouse_down(
                                                gpui::MouseButton::Left,
                                                cx.listener(
                                                    |this, _: &gpui::MouseDownEvent, _, cx| {
                                                        this.focus_pane(Pane::Centre, cx);
                                                    },
                                                ),
                                            )
                                            .on_mouse_down(
                                                gpui::MouseButton::Right,
                                                cx.listener(
                                                    |this, e: &gpui::MouseDownEvent, _, cx| {
                                                        this.app.open_pane_menu(
                                                            f32::from(e.position.x),
                                                            f32::from(e.position.y),
                                                        );
                                                        cx.notify();
                                                    },
                                                ),
                                            )
                                            .min_h_0()
                                            .child(self.centre(t, cx)),
                                    )
                                    .when_some(bottom, |d, dock| {
                                        d.child(self.pane(Pane::Bottom, dock, cx))
                                    }),
                            )
                            .when_some(inspector, |d, stops| {
                                let dock = self.right_dock(t, stops, cx).into_any_element();
                                d.child(self.pane(Pane::Right, dock, cx))
                            }),
                    )
                    .child(self.status_bar(t, cx))
            })
            .when(
                !settings && matches!(self.app.editing, Some(Editing::Rules(_))),
                |d| d.child(self.rules_sheet(t)),
            )
            .when(
                !settings && matches!(self.app.editing, Some(Editing::Commitment)),
                |d| d.child(self.commitment_sheet(t, cx)),
            )
            .when(
                !settings && matches!(self.app.editing, Some(Editing::NotesEditor)),
                |d| d.child(self.notes_editor_view(t, cx)),
            )
            .when(!settings && self.app.focus_prompt.is_some(), |d| {
                d.child(self.focus_prompt_sheet(t))
            })
            .when(
                popups && matches!(self.app.editing, Some(Editing::Ask(_))),
                |d| d.child(self.ask_sheet(t)),
            )
            .when(popups && self.app.overlay.is_some(), |d| {
                d.child(self.overlay_view(t, cx))
            })
            .when(!settings && self.app.context_menu.is_some(), |d| {
                d.child(self.context_menu_view(t, window.viewport_size(), cx))
            })
    }
}

// MARK: The input method

impl Takt {
    /// The field the keyboard types into: the overlay's, or the one open in
    /// a row or the inspector.
    fn active_field(&mut self) -> Option<&mut TextField> {
        if self.app.overlay.is_some() {
            Some(&mut self.overlay.field)
        } else if self.app.editing.is_some()
            && !matches!(
                self.app.editing,
                Some(Editing::Date(_) | Editing::Repeat(_) | Editing::Log(_))
            )
            && !(matches!(self.app.editing, Some(Editing::Commitment))
                && self.app.commitment_form.as_ref().is_some_and(|f| {
                    f.field != crate::state::commitment_form::CommitmentField::Title
                }))
        {
            Some(&mut self.field)
        } else if self.app.setting_menu().is_some() {
            Some(&mut self.settings_menu.field)
        } else if self.app.settings_page.is_some() {
            Some(&mut self.settings_field)
        } else {
            None
        }
    }

    /// After the input method changed the field: what typing would do.
    fn typed(&mut self, cx: &mut Context<Self>) {
        if self.app.overlay.is_some() {
            self.overlay.index = 0;
            self.overlay.refresh(&self.app);
        } else if self.app.editing.is_none() && self.app.setting_menu().is_some() {
            self.settings_menu_filtered();
        } else if self.app.editing.is_none() && self.app.settings_page.is_some() {
            self.settings_searched();
        }
        cx.notify();
    }
}

impl Takt {
    pub(super) fn input_hook(&self) -> Option<InputHook> {
        Some((self.this.clone(), self.focus.clone()))
    }
}

impl EntityInputHandler for Takt {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let field = self.active_field()?;
        let range = field.range_from_utf16(&range);
        actual.replace(field.range_to_utf16(&range));
        field.text.get(range).map(str::to_string)
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let field = self.active_field()?;
        let selection = field.selection();
        Some(UTF16Selection {
            range: field.range_to_utf16(&selection),
            reversed: field.anchor.is_some_and(|anchor| anchor > field.cursor),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let field = if self.app.overlay.is_some() {
            &self.overlay.field
        } else if self.app.editing.is_some() {
            &self.field
        } else if self.app.setting_menu().is_some() {
            &self.settings_menu.field
        } else if self.app.settings_page.is_some() {
            &self.settings_field
        } else {
            return None;
        };
        field
            .marked
            .as_ref()
            .map(|range| field.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        if let Some(field) = self.active_field() {
            field.marked = None;
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(field) = self.active_field() else {
            return;
        };
        match range {
            Some(range) => {
                let range = field.range_from_utf16(&range);
                field.replace(range, text);
            }
            None => field.insert(text),
        }
        self.typed(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(field) = self.active_field() else {
            return;
        };
        let range = range
            .map(|range| field.range_from_utf16(&range))
            .or_else(|| field.marked.clone())
            .unwrap_or_else(|| field.selection());
        // The selection inside the new text is counted in its UTF-16.
        let selected = selected.map(|selected| {
            let probe = TextField::new(text);
            probe.range_from_utf16(&selected)
        });
        field.mark(range, text, selected);
        self.typed(cx);
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let field = self.active_field()?;
        let start = field.offset_from_utf16(range.start);
        let line_start = field.text[..start].rfind('\n').map_or(0, |ix| ix + 1);
        let line = field.text[..start].matches('\n').count();
        let before = field.text[line_start..start].to_string();
        let style = window.text_style();
        let run = gpui::TextRun {
            len: before.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let x = window
            .text_system()
            .shape_line(before.into(), px(13.), &[run], None)
            .width;
        let line_height = px(18.);
        let top = bounds.top() + line_height * line as f32;
        Some(Bounds::new(
            gpui::point(bounds.left() + x, top),
            gpui::size(px(2.), line_height),
        ))
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
