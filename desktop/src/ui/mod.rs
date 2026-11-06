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

pub struct Takt {
    pub app: App,
    focus: FocusHandle,
    /// Itself, for the text fields' input handler to reach.
    this: WeakEntity<Self>,
    field: TextField,
    overlay: OverlayState,
    sidebar_scroll: UniformListScrollHandle,
    main_scroll: UniformListScrollHandle,
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
            this: cx.entity().downgrade(),
            field: TextField::default(),
            overlay: OverlayState::default(),
            sidebar_scroll: UniformListScrollHandle::new(),
            main_scroll: UniformListScrollHandle::new(),
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
            self.overlay_key(keystroke, cx)
        } else if self.app.editing.is_some() {
            self.editing_key(keystroke, cx)
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

    /// What the field did with a key that was the clipboard's: true when it
    /// changed.
    fn clipboard_key(field: &mut TextField, key: &FieldKey, cx: &mut Context<Self>) -> bool {
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
                        self.app.create_task(&text);
                        self.field = TextField::default();
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
                    Some(_) => self.app.commit_sidebar_name(&text),
                    None => {}
                }
                true
            }
            FieldKey::Cancel => {
                self.app.editing = None;
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
        let inspector = if self.app.prefs.right_dock_visible {
            Some(self.app.inspector_stops())
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
                    .when_some(inspector, |d, stops| d.child(self.right_dock(t, stops, cx))),
            )
            .child(self.status_bar(t))
            .when(matches!(self.app.editing, Some(Editing::Rules(_))), |d| {
                d.child(self.rules_sheet(t))
            })
            .when(self.app.overlay.is_some(), |d| {
                d.child(self.overlay_view(t, cx))
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
        } else if self.app.editing.is_some() {
            Some(&mut self.field)
        } else {
            None
        }
    }

    /// After the input method changed the field: what typing would do.
    fn typed(&mut self, cx: &mut Context<Self>) {
        if self.app.overlay.is_some() {
            self.overlay.index = 0;
            self.overlay.refresh(&self.app);
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
