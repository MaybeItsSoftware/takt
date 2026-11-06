//! The window: Zed's layout (title bar, left dock, centre pane, right dock,
//! status bar, an overlay at the top) drawn from `state::App`.
//!
//! Every key comes through `key_down`, spelled the way the catalogue spells
//! keys and run by `App::handle_key`, so the keymap is the core's, not a
//! second copy in GPUI's binding tables. Rendering reads only what is held.

mod chrome;
mod dock;
mod views;
mod widgets;

use crate::keys::{Press, spell};
use crate::overlay::{self, OverlayState};
use crate::state::board::QUADRANT_TITLES;
use crate::state::commands::{EditCursor, KeyOutcome};
use crate::state::outline::OutlineRow;
use crate::state::sidebar::RowKind;
use crate::state::today::DayRow;
use crate::state::{App, Editing, Overlay, Region, Scope, ViewMode};
use crate::text_field::{FieldKey, TextField};
use crate::theme::Tokens;
use gpui::{
    AnyElement, App as GpuiApp, ClickEvent, Context, FocusHandle, Focusable, FontWeight, Hsla,
    KeyDownEvent, ScrollStrategy, SharedString, UniformListScrollHandle, Window, div, prelude::*,
    px, uniform_list,
};
use std::time::{Duration, Instant};
use takt_core::editor::EditorSnapshot;
use takt_core::records::TaskRow;
use widgets::*;

const UI_FONT: &str = "IBM Plex Sans";
const MONO_FONT: &str = "Lilex";
const ROW_HEIGHT: f32 = 26.;
const TITLE_BAR_HEIGHT: f32 = 34.;

pub struct Takt {
    pub app: App,
    focus: FocusHandle,
    field: TextField,
    overlay: OverlayState,
    sidebar_scroll: UniformListScrollHandle,
    main_scroll: UniformListScrollHandle,
    /// The inspector's read of the selection, kept until it or the data
    /// changes.
    inspector: Option<(String, i64, EditorSnapshot)>,
    last_key_ms: Option<f64>,
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
            field: TextField::default(),
            overlay: OverlayState::default(),
            sidebar_scroll: UniformListScrollHandle::new(),
            main_scroll: UniformListScrollHandle::new(),
            inspector: None,
            last_key_ms: None,
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
                    if this.app.check_external_writes() {
                        this.app.reload_today();
                        this.rank_today(cx);
                        cx.notify();
                    } else if this.app.focus_running() {
                        // The clock in the status bar.
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
        if keystroke.modifiers.platform && keystroke.key == "w" {
            window.remove_window();
            return;
        }
        let editing_before = self.app.editing.clone();
        let handled = if self.app.overlay.is_some() {
            self.overlay_key(keystroke);
            true
        } else if self.app.editing.is_some() {
            self.editing_key(keystroke)
        } else {
            self.command_key(keystroke, cx)
        };
        if !handled {
            return;
        }
        cx.stop_propagation();
        if self.app.editing != editing_before {
            self.begin_editing();
        }
        self.after_change(cx);
        self.last_key_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
        cx.notify();
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

    fn overlay_key(&mut self, keystroke: &gpui::Keystroke) {
        let ctrl = keystroke.modifiers.control;
        match (keystroke.key.as_str(), ctrl) {
            ("n", true) | ("j", true) => return self.overlay.step(1),
            ("p", true) | ("k", true) => return self.overlay.step(-1),
            _ => {}
        }
        match self.overlay.field.handle(keystroke) {
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
    }

    fn editing_key(&mut self, keystroke: &gpui::Keystroke) -> bool {
        match self.field.handle(keystroke) {
            FieldKey::Commit => {
                let text = self.field.text.clone();
                match self.app.editing.clone() {
                    Some(Editing::TaskTitle { task_id }) => {
                        self.app.rename_task(&task_id, &text);
                        self.app.editing = None;
                    }
                    Some(Editing::NewTask { .. }) => {
                        self.app.create_task(&text);
                        self.field = TextField::default();
                    }
                    Some(_) => self.app.commit_sidebar_name(&text),
                    None => {}
                }
                true
            }
            FieldKey::Cancel => {
                self.app.editing = None;
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
            _ => TextField::default(),
        };
    }

    /// After anything changed: rank the day if it went stale, and keep the
    /// selection and the sidebar cursor on screen.
    fn after_change(&mut self, cx: &mut Context<Self>) {
        self.rank_today(cx);
        if let Some(index) = self.main_row_index() {
            self.main_scroll
                .scroll_to_item(index, ScrollStrategy::Center);
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

    // MARK: Inspector data

    fn inspector_snapshot(&mut self) -> Option<EditorSnapshot> {
        let id = self.app.selected_task.clone()?;
        let version = self.app.seen_version;
        if let Some((held, at, snapshot)) = &self.inspector
            && *held == id
            && *at == version
            && self.app.undo_label.is_some() == self.app.undo_label.is_some()
        {
            return Some(snapshot.clone());
        }
        let snapshot = self.app.store.core.editor_snapshot(id.clone()).ok()?;
        self.inspector = Some((id, version, snapshot.clone()));
        Some(snapshot)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Display {
    Task(usize),
    Draft(i64),
}

// MARK: Rendering

impl Render for Takt {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Tokens::for_appearance(window.appearance());
        // Our own writes move the version too; reading the inspector fresh
        // after any key keeps it right without a second cache.
        if self.last_key_ms.is_some() {
            self.inspector = None;
        }
        let inspector = if self.app.prefs.right_dock_visible {
            Some(self.inspector_snapshot())
        } else {
            None
        };
        div()
            .id("takt")
            .key_context("Takt")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .bg(t.background)
            .text_color(t.text)
            .font_family(UI_FONT)
            .text_size(px(13.))
            .child(self.title_bar(t, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when(self.app.prefs.sidebar_visible, |d| {
                        d.child(self.sidebar(t, cx))
                    })
                    .child(self.centre(t, cx))
                    .when_some(inspector, |d, snapshot| {
                        d.child(self.right_dock(t, snapshot, cx))
                    }),
            )
            .child(self.status_bar(t))
            .when(self.app.overlay.is_some(), |d| {
                d.child(self.overlay_view(t, cx))
            })
    }
}
