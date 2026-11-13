//! The focus panel: the one small window that floats over everything for
//! starting work and capturing it. The running task and its clock, the queue
//! behind it, and a field to find or add a task. `FocusPanelController.swift`
//! and `FocusPanelView.swift`, with `QuickCapturePanelController.swift`
//! folded in.
//!
//! With nothing running it opens typing: the day's ladder until you type,
//! then every task and list that matches, and last an "Add" row that files
//! what you typed. Return starts a timer on the highlighted task, `⌥↩` a
//! pomodoro; on a list it narrows to that list's tasks, and Add then files
//! there; `Tab` steps the list Add files into. Quick capture's hotkey opens
//! it the same way, and there Add files the task and the panel goes.
//!
//! While a block runs a letter is an action: Return queues the highlighted
//! task, `p` as a pomodoro, space pauses, `d` finishes, `s` stops, and `/`
//! or `n` goes back to typing. The footer spells the keys out. It shows and
//! hides at once, with no fade.
//!
//! It holds no workspace state of its own, only its cursor, its field and
//! the task it picked: it reads the main window's `App` through the `Takt`
//! entity and redraws when that does, and every action runs on that same
//! `App`.

use super::Takt;
use super::widgets::{field_text, key_hint};
use crate::keys::{Press, spell};
use crate::platform::window_move;
use crate::state::App;
use crate::state::focus::{FinderHit, FocusKind};
use crate::text_field::{FieldKey, TextField};
use crate::theme::Tokens;
use gpui::{
    AnyElement, App as GpuiApp, Bounds, Context, Corner, Entity, EntityInputHandler, FocusHandle,
    Focusable, KeyDownEvent, Pixels, UTF16Selection, WeakEntity, Window, WindowBounds, WindowKind,
    WindowOptions, anchored, deferred, div, point, prelude::*, px, size,
};
use std::ops::Range;

pub struct FocusPanel {
    takt: Entity<Takt>,
    this: WeakEntity<Self>,
    focus: FocusHandle,
    cursor: usize,
    /// A task just added, listed first in the ladder until it is started.
    picked: Option<String>,
    /// Opened by quick capture's hotkey: adding a task puts the panel away.
    capturing: bool,
    /// Where Add files, once `Tab` has chosen; else quick capture's list.
    destination: Option<(String, String)>,
    mode: Mode,
    /// The row typing was on when the list picker opened over it.
    type_cursor: usize,
    /// The rows and the picker's, kept scrolled to the keyboard's row.
    scroll: gpui::ScrollHandle,
    pick_scroll: gpui::ScrollHandle,
    /// The row each was last scrolled to, with how many rows there were.
    scrolled: Option<(usize, usize)>,
    pick_scrolled: Option<(usize, usize)>,
    /// Growing out of the pill or shrinking back in: drawn as the pill is.
    morphing: bool,
}

/// What the keyboard is doing in the panel.
enum Mode {
    /// Moving through the rows; letters are actions.
    Rows,
    /// Typing to find a task or a list, or to add a task. With a list
    /// chosen, its tasks, and Add files there. Why the last Add didn't file.
    Type {
        field: TextField,
        list: Option<(String, String)>,
        hits: Vec<FinderHit>,
        problem: Option<String>,
    },
    /// `Tab` while typing: choosing the list Add files into, with the
    /// typing it came from kept to go back to.
    Pick { back: Box<Mode>, field: TextField },
}

impl Mode {
    fn typing() -> Self {
        Mode::Type {
            field: TextField::default(),
            list: None,
            hits: Vec::new(),
            problem: None,
        }
    }

    fn field(&mut self) -> Option<&mut TextField> {
        match self {
            Mode::Rows => None,
            Mode::Type { field, .. } | Mode::Pick { field, .. } => Some(field),
        }
    }

    /// What was typed for the task, from typing or from the picker over it.
    fn typed(&self) -> String {
        match self {
            Mode::Type { field, .. } => field.text.trim().to_string(),
            Mode::Pick { back, .. } => back.typed(),
            Mode::Rows => String::new(),
        }
    }
}

/// What Return leaves the panel to do.
enum Leave {
    Stay,
    /// Put away, back to the app it was summoned over: into the pill
    /// while a block runs, as when one just began.
    Hide,
}

/// What `o` opens in the main window.
pub enum Target {
    Task(String),
    List(String),
}

/// A row of the panel.
#[derive(Clone, Debug, PartialEq)]
enum Entry {
    Task {
        id: String,
        title: String,
        detail: String,
    },
    List {
        id: String,
        name: String,
    },
    /// File what was typed as a new task.
    Add,
    /// In the picker: a list Add can file into, and its folder.
    Into {
        id: String,
        name: String,
        folder: String,
        current: bool,
    },
}

impl Focusable for FocusPanel {
    fn focus_handle(&self, _: &GpuiApp) -> FocusHandle {
        self.focus.clone()
    }
}

pub const WIDTH: f32 = 340.;
pub const HEIGHT: f32 = 420.;

/// Where the panel goes when it grows out of the pill at `pill`: the same
/// top-left corner, kept on its display. With no pill, where the
/// pill would be, at the top centre of the main display.
fn bounds_from(pill: Option<Bounds<Pixels>>, cx: &GpuiApp) -> Bounds<Pixels> {
    let panel = size(px(WIDTH), px(HEIGHT));
    let pill = pill.unwrap_or_else(|| super::pill::default_bounds(cx));
    let display = cx
        .displays()
        .into_iter()
        .map(|d| d.bounds())
        .find(|b| b.contains(&pill.origin))
        .or_else(|| cx.primary_display().map(|d| d.bounds()));
    let mut origin = pill.origin;
    if let Some(d) = display {
        let right = d.origin.x + d.size.width - panel.width;
        let bottom = d.origin.y + d.size.height - panel.height;
        origin.x = origin.x.max(d.origin.x).min(right);
        origin.y = origin.y.max(d.origin.y).min(bottom);
    }
    Bounds {
        origin,
        size: panel,
    }
}

/// How long the panel takes to grow out of the pill or shrink back into it.
const MORPH_SECONDS: f64 = 0.18;

/// Opens the panel at the pill at `pill` (or where the pill would be), in
/// front of whatever app is: on its rows, or typing when `capturing`. With
/// `grow` (the pill is up) it grows out of the pill rather than appearing.
pub fn open(
    takt: Entity<Takt>,
    capturing: bool,
    pill: Option<Bounds<Pixels>>,
    grow: bool,
    cx: &mut GpuiApp,
) -> Option<gpui::WindowHandle<FocusPanel>> {
    let full = bounds_from(pill, cx);
    let grow = grow && !window_move::reduce_motion();
    let first = if grow {
        Bounds {
            origin: full.origin,
            size: size(px(super::pill::WIDTH), px(super::pill::HEIGHT)),
        }
    } else {
        full
    };
    let mut grown = None;
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(first)),
        titlebar: None,
        kind: WindowKind::PopUp,
        // Clear behind its rounded corners, which it draws itself.
        window_background: gpui::WindowBackgroundAppearance::Transparent,
        is_movable: true,
        is_resizable: false,
        is_minimizable: false,
        focus: true,
        show: true,
        ..Default::default()
    };
    let handle = cx
        .open_window(options, |window, cx| {
            if let Some(ns_window) = window_move::ns_window(window) {
                window_move::no_animation(ns_window);
                window_move::no_shadow(ns_window);
                grown = Some(ns_window);
            }
            let panel = cx.new(|cx| {
                cx.observe(&takt, |_, _, cx| cx.notify()).detach();
                FocusPanel {
                    takt: takt.clone(),
                    this: cx.entity().downgrade(),
                    focus: cx.focus_handle(),
                    cursor: 0,
                    picked: None,
                    capturing,
                    destination: None,
                    type_cursor: 0,
                    scroll: gpui::ScrollHandle::new(),
                    pick_scroll: gpui::ScrollHandle::new(),
                    scrolled: None,
                    pick_scrolled: None,
                    morphing: grow,
                    mode: if capturing {
                        Mode::typing()
                    } else {
                        Mode::Rows
                    },
                }
            });
            window.focus(&panel.focus_handle(cx));
            panel
        })
        .ok();
    cx.activate(true);
    if let (true, Some(ns_window)) = (grow, grown) {
        // Down and to the right from the pill's top-left corner: AppKit's
        // origin is the bottom left, so the top edge stays where it is.
        let (x, y, _, h) = window_move::frame(ns_window);
        let (fw, fh) = (f64::from(WIDTH), f64::from(HEIGHT));
        let to = (x, y + h - fh, fw, fh);
        let panel = handle;
        cx.spawn(async move |cx| {
            window_move::animate_frame(ns_window, to, MORPH_SECONDS);
            cx.background_executor()
                .timer(std::time::Duration::from_secs_f64(MORPH_SECONDS))
                .await;
            if let Some(panel) = panel {
                let _ = panel.update(cx, |panel, _, cx| {
                    panel.morphing = false;
                    cx.notify();
                });
            }
        })
        .detach();
    }
    handle
}

/// The day's open tasks to start from, with a task just added first when
/// it isn't already among them.
fn ladder(app: &App, picked: Option<&str>) -> Vec<Entry> {
    let list_name = |list_id: &str| {
        app.list(list_id)
            .map(|l| l.name.clone())
            .unwrap_or_default()
    };
    let mut listed: Vec<Entry> = app
        .today
        .rows()
        .filter(|row| row.task.status == "open")
        .map(|row| Entry::Task {
            id: row.task.id.clone(),
            title: row.task.title.clone(),
            detail: list_name(&row.task.list_id),
        })
        .collect();
    if let Some(id) = picked
        && !listed
            .iter()
            .any(|e| matches!(e, Entry::Task { id: listed, .. } if listed == id))
        && let Some(task) = app
            .task(id)
            .cloned()
            .or_else(|| app.store.core.task(id.into()).ok().flatten())
            .filter(|task| task.status == "open")
    {
        let detail = list_name(&task.list_id);
        listed.insert(
            0,
            Entry::Task {
                id: task.id,
                title: task.title,
                detail,
            },
        );
    }
    listed
}

/// What comes after the running task: the session's queue, then the rest
/// of the day's open tasks, the running one in neither.
fn up_next(app: &App) -> Vec<Entry> {
    let running = app
        .focus_session
        .as_ref()
        .and_then(|s| s.active_task_id.clone());
    let mut seen: std::collections::HashSet<String> = running.into_iter().collect();
    let mut next: Vec<Entry> = app
        .focus_queue()
        .into_iter()
        .map(|entry| Entry::Task {
            detail: app
                .list(&entry.task.list_id)
                .map(|l| l.name.clone())
                .unwrap_or_default(),
            id: entry.task.id,
            title: entry.task.title,
        })
        .collect();
    next.extend(ladder(app, None));
    next.retain(|entry| match entry {
        Entry::Task { id, .. } => seen.insert(id.clone()),
        _ => true,
    });
    next
}

/// The lists Add can file into, matching `query` (names starting with it
/// first), with each one's folder. (id, name, folder).
fn destinations(app: &App, query: &str) -> Vec<(String, String, String)> {
    let needle = query.trim().to_lowercase();
    let mut found: Vec<(bool, String, String, String)> = app
        .capture_lists()
        .into_iter()
        .filter_map(|(id, name)| {
            let folder = app
                .list(&id)
                .and_then(|l| l.folder_id.as_deref())
                .and_then(|f| app.folder(f))
                .map(|f| f.name.clone())
                .unwrap_or_default();
            let lower = name.to_lowercase();
            let hit = needle.is_empty()
                || lower.contains(&needle)
                || folder.to_lowercase().contains(&needle);
            hit.then(|| (!lower.starts_with(&needle), id, name, folder))
        })
        .collect();
    // Stable: sidebar order within each group.
    found.sort_by_key(|(later, ..)| *later);
    found
        .into_iter()
        .map(|(_, id, name, folder)| (id, name, folder))
        .collect()
}

impl FocusPanel {
    /// The rows as the mode lists them.
    fn entries(&self, app: &App) -> Vec<Entry> {
        self.entries_of(&self.mode, app)
    }

    /// The rows `mode` lists: in the picker, the lists it offers.
    fn entries_of(&self, mode: &Mode, app: &App) -> Vec<Entry> {
        match mode {
            Mode::Rows if app.focus_session.is_some() => up_next(app),
            Mode::Rows => ladder(app, self.picked.as_deref()),
            Mode::Pick { field, .. } => {
                let current = self.filing_into(app).map(|(id, _)| id);
                destinations(app, &field.text)
                    .into_iter()
                    .map(|(id, name, folder)| Entry::Into {
                        current: current.as_deref() == Some(id.as_str()),
                        id,
                        name,
                        folder,
                    })
                    .collect()
            }
            Mode::Type {
                field, list, hits, ..
            } => {
                if field.text.trim().is_empty() && list.is_none() {
                    return ladder(app, self.picked.as_deref());
                }
                let mut entries: Vec<Entry> = hits
                    .iter()
                    .map(|hit| match hit.clone() {
                        FinderHit::Task { id, title, list } => Entry::Task {
                            id,
                            title,
                            detail: list,
                        },
                        FinderHit::List { id, name } => Entry::List { id, name },
                    })
                    .collect();
                if !field.text.trim().is_empty() {
                    entries.push(Entry::Add);
                }
                entries
            }
        }
    }

    /// Where Add files: the list last chosen, by the picker or by
    /// narrowing to it, else quick capture's.
    fn filing_into(&self, app: &App) -> Option<(String, String)> {
        self.destination
            .clone()
            .or_else(|| app.quick_capture_destination())
    }

    /// `Tab` or a click on "Adds to": the picker, on the current list.
    fn open_picker(&mut self, cx: &GpuiApp) {
        if !matches!(self.mode, Mode::Type { .. }) {
            return;
        }
        let back = std::mem::replace(&mut self.mode, Mode::Rows);
        self.type_cursor = self.cursor;
        self.pick_scrolled = None;
        self.mode = Mode::Pick {
            back: Box::new(back),
            field: TextField::default(),
        };
        self.cursor = self
            .entries(&self.takt.read(cx).app)
            .iter()
            .position(|e| matches!(e, Entry::Into { current: true, .. }))
            .unwrap_or(0);
    }

    /// Leaves the picker, filing into `chosen` from now on when there is one.
    fn close_picker(&mut self, chosen: Option<(String, String)>, cx: &GpuiApp) {
        let chosen_none = chosen.is_none();
        if let Mode::Pick { back, .. } = std::mem::replace(&mut self.mode, Mode::Rows) {
            self.mode = *back;
        }
        if chosen.is_some() {
            self.destination = chosen;
        }
        // Back on Add, ready to file there, when something was typed;
        // left without a choice, where it was.
        let count = self.entries(&self.takt.read(cx).app).len();
        self.cursor = if chosen_none {
            self.type_cursor.min(count.saturating_sub(1))
        } else if self.mode.typed().is_empty() {
            0
        } else {
            count.saturating_sub(1)
        };
    }

    /// After the field changed: find again, or in the picker, start over
    /// at the top.
    fn after_edit(&mut self, cx: &GpuiApp) {
        if matches!(self.mode, Mode::Pick { .. }) {
            self.cursor = 0;
        } else {
            self.refind(cx);
        }
    }

    /// Quick capture's hotkey on an open panel: back to typing, to add.
    pub fn begin_capture(&mut self) {
        self.capturing = true;
        if matches!(self.mode, Mode::Rows) {
            self.mode = Mode::typing();
            self.cursor = 0;
        }
    }

    fn refind(&mut self, cx: &GpuiApp) {
        if let Mode::Type {
            field,
            list,
            hits,
            problem,
        } = &mut self.mode
        {
            *problem = None;
            *hits = self
                .takt
                .read(cx)
                .app
                .focus_finder(&field.text, list.as_ref().map(|(id, _)| id.as_str()));
        }
        // On what was typed, the best match; on nothing, the top.
        self.cursor = 0;
    }

    /// `o` (`⌘O` while typing): the task, or list, in the main window,
    /// which takes the keyboard; the panel goes back to the pill.
    pub fn open_in_main(&mut self, target: Target, window: &mut Window, cx: &mut Context<Self>) {
        let takt = self.takt.clone();
        takt.update(cx, |takt, _| takt.panel_return_to = None);
        window.remove_window();
        cx.defer(move |cx| {
            takt.update(cx, |takt, cx| {
                let app = &mut takt.app;
                match target {
                    Target::Task(id) => {
                        let list = app
                            .task(&id)
                            .cloned()
                            .or_else(|| app.store.core.task(id.clone()).ok().flatten())
                            .map(|t| t.list_id);
                        if let Some(list) = list {
                            app.reveal_task(&id, &list);
                        }
                    }
                    Target::List(id) => app.select_list(&id),
                }
                takt.show_main_window(cx);
                takt.settle(cx);
            })
        });
    }

    /// Puts the panel away, handing the keyboard back to the app it was
    /// summoned over rather than letting the main window take it.
    fn put_away(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let back = self.takt.update(cx, |takt, _| takt.panel_return_to.take());
        self.leave(back, window, cx);
    }

    /// Goes, handing the keyboard to `back`. While a block runs the pill
    /// takes its place: the panel shrinks into its top-left corner, where
    /// the pill then stands. Otherwise, or with reduced motion, it simply
    /// goes.
    pub fn leave(&mut self, back: Option<i32>, window: &mut Window, cx: &mut Context<Self>) {
        let pill = {
            let app = &self.takt.read(cx).app;
            app.focus_session.is_some() && app.prefs.focus_panel
        };
        let ns_window = window_move::ns_window(window);
        let (Some(ns_window), true, false) = (ns_window, pill, window_move::reduce_motion()) else {
            if let Some(pid) = back {
                crate::platform::activate_app(pid);
            }
            return window.remove_window();
        };
        let at = window.bounds();
        let pill_size = size(px(super::pill::WIDTH), px(super::pill::HEIGHT));
        self.takt.update(cx, |takt, _| {
            takt.pill_at = Some(Bounds {
                origin: at.origin,
                size: pill_size,
            });
        });
        // Wears the pill from the first frame of the shrink.
        self.morphing = true;
        cx.notify();
        let handle = window.window_handle();
        let (x, y, _, h) = window_move::frame(ns_window);
        let (pw, ph) = (
            f64::from(super::pill::WIDTH),
            f64::from(super::pill::HEIGHT),
        );
        let to = (x, y + h - ph, pw, ph);
        cx.spawn(async move |_, cx| {
            window_move::animate_frame(ns_window, to, MORPH_SECONDS);
            cx.background_executor()
                .timer(std::time::Duration::from_secs_f64(MORPH_SECONDS))
                .await;
            if let Some(pid) = back {
                crate::platform::activate_app(pid);
            }
            let _ = cx.update_window(handle, |_, window, _| window.remove_window());
        })
        .detach();
    }

    /// Settles the main window once this window's update is over: `Takt`
    /// can't see this window while it updates.
    fn settle(&self, cx: &mut Context<Self>) {
        let takt = self.takt.clone();
        cx.defer(move |cx| takt.update(cx, |takt, cx| takt.settle(cx)));
    }

    /// Starts (or queues) a timer or a pomodoro on `id`. True when it
    /// began a block, rather than queueing one behind the running block.
    fn start(&mut self, id: String, kind: FocusKind, cx: &mut Context<Self>) -> bool {
        let began = self.takt.update(cx, |takt, _| {
            let idle = takt.app.focus_session.is_none();
            takt.app.focus_from_panel(&id, kind);
            idle && takt.app.focus_session.is_some()
        });
        if self.picked.as_deref() == Some(id.as_str()) {
            self.picked = None;
        }
        self.mode = Mode::Rows;
        self.cursor = 0;
        self.settle(cx);
        began
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.mode, Mode::Rows) {
            self.field_key(event, window, cx);
            return;
        }
        let Some(key) = spell(&Press::from_keystroke(&event.keystroke)) else {
            return;
        };
        cx.stop_propagation();
        let prompt = self.takt.read(cx).app.focus_prompt.is_some();
        if prompt {
            self.takt
                .update(cx, |takt, _| takt.app.focus_prompt_key(&key));
            self.settle(cx);
            cx.notify();
            return;
        }
        let entries = self.entries(&self.takt.read(cx).app);
        let highlighted = match entries.get(self.cursor) {
            Some(Entry::Task { id, .. }) => Some(id.clone()),
            _ => None,
        };
        match key.as_str() {
            "escape" | "cmd+w" => return self.put_away(window, cx),
            "n" | "/" => {
                self.mode = Mode::typing();
                self.cursor = 0;
            }
            "o" => {
                let running = self
                    .takt
                    .read(cx)
                    .app
                    .focus_session
                    .as_ref()
                    .and_then(|s| s.active_task_id.clone());
                if let Some(id) = highlighted.clone().or(running) {
                    return self.open_in_main(Target::Task(id), window, cx);
                }
            }
            "enter" | "p" => {
                if let Some(id) = highlighted {
                    let kind = if key == "p" {
                        FocusKind::Pomodoro
                    } else {
                        FocusKind::Timer
                    };
                    if self.start(id, kind, cx) {
                        return self.put_away(window, cx);
                    }
                }
            }
            "d" | "s" | "space" => {
                self.takt.update(cx, |takt, _| {
                    let app = &mut takt.app;
                    match key.as_str() {
                        "d" => app.request_focus_completion(true),
                        "s" => app.stop_focus(),
                        _ if app
                            .focus_session
                            .as_ref()
                            .is_some_and(|s| s.phase == "onBreak") =>
                        {
                            app.end_focus_break()
                        }
                        _ => app.toggle_focus_pause(),
                    }
                });
                self.settle(cx);
            }
            "j" | "down" => self.cursor = (self.cursor + 1).min(entries.len().saturating_sub(1)),
            "k" | "up" => self.cursor = self.cursor.saturating_sub(1),
            _ => {}
        }
        cx.notify();
    }

    /// A key while typing.
    fn field_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = &event.keystroke.modifiers;
        if m.platform
            && !m.shift
            && event.keystroke.key == "o"
            && matches!(self.mode, Mode::Type { .. })
        {
            let entries = self.entries(&self.takt.read(cx).app);
            let target = match entries.get(self.cursor) {
                Some(Entry::Task { id, .. }) => Some(Target::Task(id.clone())),
                Some(Entry::List { id, .. }) => Some(Target::List(id.clone())),
                _ => None,
            };
            if let Some(target) = target {
                cx.stop_propagation();
                return self.open_in_main(target, window, cx);
            }
        }
        let Some(field) = self.mode.field() else {
            return;
        };
        let outcome = field.handle(&event.keystroke);
        if Takt::clipboard_key(field, &outcome, cx) {
            self.after_edit(cx);
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let ctrl = event.keystroke.modifiers.control;
        let key = event.keystroke.key.as_str();
        let picking = matches!(self.mode, Mode::Pick { .. });
        let step = match outcome {
            FieldKey::Down => Some(1),
            FieldKey::Up => Some(-1),
            FieldKey::Tab { shift } if picking => Some(if shift { -1 } else { 1 }),
            _ if ctrl && key == "n" => Some(1),
            _ if ctrl && key == "p" => Some(-1),
            _ => None,
        };
        match outcome {
            // Printable: the input method types it.
            FieldKey::Text(_) if step.is_none() => return,
            FieldKey::Edited => self.after_edit(cx),
            FieldKey::Commit if picking => {
                let entries = self.entries(&self.takt.read(cx).app);
                if let Some(Entry::Into { id, name, .. }) = entries.get(self.cursor).cloned() {
                    self.close_picker(Some((id, name)), cx);
                }
            }
            FieldKey::Cancel if picking => self.close_picker(None, cx),
            FieldKey::Commit => {
                let kind = if event.keystroke.modifiers.alt {
                    FocusKind::Pomodoro
                } else {
                    FocusKind::Timer
                };
                let hide = event.keystroke.modifiers.platform;
                match self.commit(kind, hide, cx) {
                    Leave::Stay => {}
                    Leave::Hide => return self.put_away(window, cx),
                }
            }
            FieldKey::Cancel => {
                if self.cancel(cx) {
                    return self.put_away(window, cx);
                }
            }
            FieldKey::Tab { .. } => self.open_picker(cx),
            _ => {}
        }
        if let Some(step) = step {
            let count = self.entries(&self.takt.read(cx).app).len();
            self.cursor = if step > 0 {
                (self.cursor + 1).min(count.saturating_sub(1))
            } else {
                self.cursor.saturating_sub(1)
            };
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Return on a row; `hide` (⌘Return) puts the panel away after an
    /// Add. Whether, and how, the panel should go.
    fn commit(&mut self, kind: FocusKind, hide: bool, cx: &mut Context<Self>) -> Leave {
        let entries = self.entries(&self.takt.read(cx).app);
        match entries.get(self.cursor).cloned() {
            Some(Entry::Task { id, .. }) => {
                if self.start(id, kind, cx) {
                    return Leave::Hide;
                }
            }
            Some(Entry::List { id, name }) => {
                if let Mode::Type { field, list, .. } = &mut self.mode {
                    *list = Some((id.clone(), name.clone()));
                    *field = TextField::default();
                }
                // Narrowed to a list, Add files there.
                self.destination = Some((id, name));
                self.refind(cx);
            }
            Some(Entry::Add) => {
                let Mode::Type { field, .. } = &self.mode else {
                    return Leave::Stay;
                };
                let text = field.text.clone();
                let into = self.filing_into(&self.takt.read(cx).app);
                let filed = self.takt.update(cx, |takt, _| {
                    let filed = takt.app.capture_into(&text, into);
                    takt.app.today_stale = true;
                    filed
                });
                match filed {
                    Ok(_) if hide || self.capturing => {
                        self.settle(cx);
                        return Leave::Hide;
                    }
                    Ok((id, _)) => {
                        // Back to the rows with it highlighted, to start.
                        self.mode = Mode::Rows;
                        self.picked = Some(id.clone());
                        self.cursor = self
                            .entries(&self.takt.read(cx).app)
                            .iter()
                            .position(|e| matches!(e, Entry::Task { id: t, .. } if *t == id))
                            .unwrap_or(0);
                        self.settle(cx);
                    }
                    Err(why) => {
                        if let Mode::Type { problem, .. } = &mut self.mode {
                            *problem = Some(why);
                        }
                    }
                }
            }
            Some(Entry::Into { .. }) | None => {}
        }
        Leave::Stay
    }

    /// Esc: clears what was typed, then leaves a chosen list, then goes back
    /// to the running block's keys. True when the panel should go.
    fn cancel(&mut self, cx: &GpuiApp) -> bool {
        if let Mode::Type { field, list, .. } = &mut self.mode {
            if !field.text.is_empty() {
                *field = TextField::default();
            } else if let Some(left) = list.take() {
                // Leaving it, Add goes back to where it was going.
                if self.destination.as_ref() == Some(&left) {
                    self.destination = None;
                }
            } else if !self.capturing {
                self.mode = Mode::Rows;
            } else {
                return true;
            }
        }
        self.refind(cx);
        false
    }
}

/// A key and what it does, for the footer.
fn hint(keys: &str, label: &str, t: Tokens) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(4.))
        .child(key_hint(keys.to_string(), t))
        .child(div().child(label.to_string()))
        .into_any_element()
}

impl Render for FocusPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = &self.takt.read(cx).app;
        let t = Tokens::of(&app.theme(), window.appearance(), &app.prefs.appearance);
        if self.morphing {
            return super::pill::face(app, t)
                .id("focus-panel")
                .key_context("FocusPanel")
                .track_focus(&self.focus)
                .on_key_down(cx.listener(Self::key_down))
                .into_any_element();
        }
        let status = app.focus_status();
        let running = app.focus_session.is_some();
        let prompt = app.focus_prompt.clone();
        let entries = self.entries(app);
        let filing_into = self
            .filing_into(app)
            .map(|(_, name)| name)
            .unwrap_or_else(|| "No list".into());
        let filing_into = filing_into.as_str();
        let picking = matches!(self.mode, Mode::Pick { .. });
        // Under the picker, the rows typing had, as they were.
        let (rows_of, rows_cursor, options) = match &self.mode {
            Mode::Pick { back, .. } => {
                (self.entries_of(back, app), self.type_cursor, Some(entries))
            }
            _ => (entries, self.cursor, None),
        };
        let cursor = rows_cursor.min(rows_of.len().saturating_sub(1));
        let hook = Some((self.this.clone(), self.focus.clone()));
        let typed = self.mode.typed();
        let labels = app.capture_labels(&typed);
        // Kept scrolled to the keyboard's row, once per move, so the wheel
        // still scrolls.
        let at = (cursor, rows_of.len());
        if self.scrolled != Some(at) {
            self.scrolled = Some(at);
            self.scroll.scroll_to_item(cursor);
        }
        if let Some(options) = &options {
            let at = (
                self.cursor.min(options.len().saturating_sub(1)),
                options.len(),
            );
            if self.pick_scrolled != Some(at) {
                self.pick_scrolled = Some(at);
                self.pick_scroll.scroll_to_item(at.0);
            }
        }
        let row = move |here: bool| {
            div()
                .px_2()
                .py(px(3.))
                .rounded(px(4.))
                .border_1()
                .border_color(if here {
                    t.focus
                } else {
                    gpui::transparent_black()
                })
                .when(here, |d| d.bg(t.selected))
                .flex()
                .items_center()
                .gap_2()
        };
        let detail = move |text: String| {
            div()
                .flex_none()
                .text_size(px(11.))
                .text_color(t.placeholder)
                .child(text)
        };
        let draw = |entries: Vec<Entry>, cursor: usize, cx: &mut Context<Self>| {
            entries
                .into_iter()
                .enumerate()
                .map(|(ix, entry)| {
                    let here = ix == cursor;
                    match entry {
                        Entry::Task {
                            title, detail: d, ..
                        } => row(here)
                            .child(div().flex_1().min_w_0().truncate().child(title))
                            .child(detail(d))
                            .into_any_element(),
                        Entry::List { name, .. } => row(here)
                            .child(div().flex_1().min_w_0().truncate().child(name))
                            .child(detail("list".into()))
                            .into_any_element(),
                        Entry::Add => row(here)
                            .child(div().flex_none().text_color(t.accent).child("+"))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(format!("Add \u{201c}{typed}\u{201d}")),
                            )
                            .children(labels.iter().map(|l| detail(l.clone())))
                            .child(detail(format!("\u{2192} {filing_into}")))
                            .into_any_element(),
                        Entry::Into {
                            id,
                            name,
                            folder,
                            current,
                        } => {
                            let shown = name.clone();
                            row(here)
                                .id(gpui::ElementId::Name(format!("into-{id}").into()))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                                    this.close_picker(Some((id.clone(), name.clone())), cx);
                                    cx.notify();
                                }))
                                .child(
                                    div()
                                        .flex_none()
                                        .w(px(12.))
                                        .text_color(t.accent)
                                        .child(if current { "\u{2713}" } else { "" }),
                                )
                                .child(div().flex_1().min_w_0().truncate().child(shown))
                                .child(detail(folder))
                                .into_any_element()
                        }
                    }
                })
                .collect::<Vec<_>>()
        };
        let rows = draw(rows_of, cursor, cx);
        let picker_rows = options.map(|options| {
            let cursor = self.cursor.min(options.len().saturating_sub(1));
            draw(options, cursor, cx)
        });
        let list_area = div()
            .id("focus-panel-rows")
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_1()
            .children(rows);
        let heading = |text: String| {
            div()
                .px_3()
                .pt_2()
                .text_size(px(11.))
                .text_color(t.placeholder)
                .child(text)
        };
        // The list Add files into, beside the field; `Tab` or a click drops
        // its picker down under it, over the rows.
        let destination = {
            let chip = div()
                .id("focus-panel-destination")
                .flex_none()
                .cursor_pointer()
                .px_1()
                .h(px(22.))
                .flex()
                .items_center()
                .rounded(px(4.))
                .border_1()
                .border_color(if picking { t.focus } else { t.border_variant })
                .text_size(px(11.))
                .text_color(if picking { t.text } else { t.placeholder })
                .hover(|d| d.text_color(t.text))
                .on_click(cx.listener(|this, _: &gpui::ClickEvent, _, cx| {
                    if matches!(this.mode, Mode::Pick { .. }) {
                        this.close_picker(None, cx);
                    } else {
                        this.open_picker(cx);
                    }
                    cx.notify();
                }))
                .child(format!("\u{2192} {filing_into} \u{25be}"));
            let menu = match (&self.mode, picker_rows) {
                (Mode::Pick { field, .. }, Some(items)) => Some(
                    div()
                        .w(px(230.))
                        .flex()
                        .flex_col()
                        .bg(t.surface)
                        .border_1()
                        .border_color(t.border)
                        .rounded(px(6.))
                        .text_size(px(13.))
                        .child(
                            div()
                                .m_1()
                                .px_2()
                                .py(px(2.))
                                .rounded(px(4.))
                                .border_1()
                                .border_color(t.border_variant)
                                .child(field_text(field, "Find a list\u{2026}", t, hook.clone())),
                        )
                        .child(
                            div()
                                .id("focus-panel-picker")
                                .track_scroll(&self.pick_scroll)
                                .max_h(px(200.))
                                .overflow_y_scroll()
                                .px_1()
                                .pb_1()
                                .children(items),
                        ),
                ),
                _ => None,
            };
            div()
                .relative()
                .flex_none()
                .child(chip)
                .when_some(menu, |d, menu| {
                    d.child(
                        div().absolute().right_0().bottom_0().child(
                            deferred(
                                anchored()
                                    .anchor(Corner::TopRight)
                                    .offset(point(px(0.), px(4.)))
                                    .snap_to_window_with_margin(px(6.))
                                    .child(menu),
                            )
                            .with_priority(1),
                        ),
                    )
                })
        };
        let (body, hints): (Vec<AnyElement>, Vec<(&str, &str)>) = match &self.mode {
            Mode::Rows => (
                vec![
                    heading(
                        if running {
                            "Up next"
                        } else {
                            "Start on\u{2026}"
                        }
                        .into(),
                    )
                    .into_any_element(),
                    list_area.into_any_element(),
                ],
                if prompt.is_some() {
                    vec![]
                } else if running {
                    vec![
                        ("\u{21a9}", "queue"),
                        ("p", "queue pomodoro"),
                        ("j k", "move"),
                        ("o", "open in Takt"),
                        ("/", "find or add"),
                        ("esc", "hide"),
                    ]
                } else {
                    vec![
                        ("\u{21a9}", "timer"),
                        ("p", "pomodoro"),
                        ("j k", "move"),
                        ("o", "open in Takt"),
                        ("/", "find or add"),
                        ("esc", "hide"),
                    ]
                },
            ),
            Mode::Type { .. } | Mode::Pick { .. } => {
                let (field, list, problem) = match &self.mode {
                    Mode::Type {
                        field,
                        list,
                        problem,
                        ..
                    } => (field, list, problem),
                    Mode::Pick { back, .. } => match back.as_ref() {
                        Mode::Type {
                            field,
                            list,
                            problem,
                            ..
                        } => (field, list, problem),
                        _ => unreachable!("the picker opens over typing"),
                    },
                    Mode::Rows => unreachable!(),
                };
                let placeholder = match list {
                    Some((_, name)) => format!("Find or add in {name}\u{2026}"),
                    None if self.capturing => "Add a task\u{2026} 45m #tag @fri !1".into(),
                    None => "Find a task or list, or add one\u{2026}".into(),
                };
                // Under the picker the field shows what was typed, but the
                // keyboard is the picker's.
                let field_hook = if picking { None } else { hook.clone() };
                (
                    vec![
                        div()
                            .mx_2()
                            .mt_2()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.))
                                    .border_1()
                                    .border_color(if picking { t.border_variant } else { t.accent })
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .when_some(list.clone(), |d, (_, name)| {
                                        d.child(
                                            div()
                                                .flex_none()
                                                .px_1()
                                                .rounded(px(4.))
                                                .bg(t.selected)
                                                .text_size(px(11.))
                                                .child(name),
                                        )
                                    })
                                    .child(div().flex_1().min_w_0().child(field_text(
                                        field,
                                        &placeholder,
                                        t,
                                        field_hook,
                                    ))),
                            )
                            .child(destination)
                            .into_any_element(),
                        match problem {
                            Some(problem) => div()
                                .px_3()
                                .pt_1()
                                .text_size(px(11.))
                                .text_color(t.error)
                                .child(problem.clone())
                                .into_any_element(),
                            None => div().into_any_element(),
                        },
                        if typed.is_empty() && list.is_none() {
                            heading("Start on\u{2026}".into()).into_any_element()
                        } else {
                            div().into_any_element()
                        },
                        list_area.into_any_element(),
                    ],
                    if picking {
                        vec![
                            ("\u{2191}\u{2193}", "move"),
                            ("\u{21a9}", "choose list"),
                            ("esc", "back"),
                        ]
                    } else {
                        vec![
                            ("\u{21a9}", "timer / add"),
                            ("\u{2325}\u{21a9}", "pomodoro"),
                            ("\u{2191}\u{2193}", "move"),
                            ("\u{2318}\u{21a9}", "add & hide"),
                            ("\u{21e5}", "choose list"),
                            ("\u{2318}O", "open in Takt"),
                            (
                                "esc",
                                if !typed.is_empty() {
                                    "clear"
                                } else if list.is_some() {
                                    "all lists"
                                } else if !self.capturing {
                                    "back"
                                } else {
                                    "hide"
                                },
                            ),
                        ]
                    },
                )
            }
        };
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
            .border_color(t.border_variant)
            .rounded(px(8.))
            .text_color(t.text)
            .font_family(super::UI_FONT)
            .text_size(px(13.))
            .when(status.is_some(), |d| {
                d.child(
                    div()
                        .p_3()
                        .border_b_1()
                        .border_color(t.border_variant)
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(
                            status
                                .clone()
                                .map(|(title, _, _)| div().truncate().child(title)),
                        )
                        .when_some(status.clone(), |d, (_, clock, over)| {
                            d.child(
                                div()
                                    .font_family(super::MONO_FONT)
                                    .text_size(px(26.))
                                    .text_color(if over { t.warning } else { t.accent })
                                    .child(clock),
                            )
                        })
                        .when(
                            running && prompt.is_none() && matches!(self.mode, Mode::Rows),
                            |d| {
                                d.child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .gap_2()
                                        .text_size(px(11.))
                                        .text_color(t.placeholder)
                                        .child(hint("\u{2423}", "pause", t))
                                        .child(hint("d", "finish task", t))
                                        .child(hint("s", "stop, task stays open", t)),
                                )
                            },
                        ),
                )
            })
            .when_some(prompt, |d, prompt| {
                d.child(
                    div()
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .border_b_1()
                        .border_color(t.border_variant)
                        .child(div().text_size(px(13.)).child(if prompt.time_is_up {
                            "Time's up. How did it go?"
                        } else {
                            "How did it go?"
                        }))
                        .when(prompt.scores, |d| {
                            d.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .font_family(super::MONO_FONT)
                                            .text_size(px(20.))
                                            .text_color(t.accent)
                                            .child(format!("\u{d7}{:.1}", prompt.multiplier())),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(11.))
                                            .text_color(t.muted)
                                            .child(format!("{:.1} pts", prompt.points())),
                                    )
                                    .child(hint("\u{2191}\u{2193}", "score", t)),
                            )
                        })
                        .child(super::widgets::prompt_choices(&prompt, t))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_x_3()
                                .gap_y_1()
                                .text_size(px(11.))
                                .text_color(t.placeholder)
                                .child(hint("\u{2190}\u{2192}", "choose", t))
                                .child(hint("\u{21a9}", "take it", t)),
                        ),
                )
            })
            .children(body)
            .when(!hints.is_empty(), |d| {
                d.child(
                    div()
                        .px_3()
                        .py_2()
                        .border_t_1()
                        .border_color(t.border_variant)
                        .flex()
                        .flex_wrap()
                        .gap_x_3()
                        .gap_y_1()
                        .text_size(px(11.))
                        .text_color(t.placeholder)
                        .children(hints.into_iter().map(|(keys, label)| hint(keys, label, t))),
                )
            })
            .into_any_element()
    }
}

impl EntityInputHandler for FocusPanel {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let field = self.mode.field()?;
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
        let field = self.mode.field()?;
        let selection = field.selection();
        Some(UTF16Selection {
            range: field.range_to_utf16(&selection),
            reversed: field.anchor.is_some_and(|a| a > field.cursor),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        match &self.mode {
            Mode::Type { field, .. } | Mode::Pick { field, .. } => field
                .marked
                .as_ref()
                .map(|range| field.range_to_utf16(range)),
            Mode::Rows => None,
        }
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        if let Some(field) = self.mode.field() {
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
        let Some(field) = self.mode.field() else {
            return;
        };
        match range {
            Some(range) => {
                let range = field.range_from_utf16(&range);
                field.replace(range, text);
            }
            None => field.insert(text),
        }
        self.after_edit(cx);
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
        let Some(field) = self.mode.field() else {
            return;
        };
        let range = range
            .map(|range| field.range_from_utf16(&range))
            .or_else(|| field.marked.clone())
            .unwrap_or_else(|| field.selection());
        let selected = selected.map(|selected| TextField::new(text).range_from_utf16(&selected));
        field.mark(range, text, selected);
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
