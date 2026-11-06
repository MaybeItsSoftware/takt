//! The window: Zed's layout (title bar, left dock, centre pane, right dock,
//! status bar, an overlay at the top) drawn from `state::App`.
//!
//! Every key comes through `key_down`, spelled the way the catalogue spells
//! keys and run by `App::handle_key`, so the keymap is the core's, not a
//! second copy in GPUI's binding tables. Rendering reads only what is held.

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
            self.overlay.reset();
            self.overlay.refresh(&self.app);
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
                        d.child(self.inspector_pane(t, snapshot))
                    }),
            )
            .child(self.status_bar(t))
            .when(self.app.overlay.is_some(), |d| {
                d.child(self.overlay_view(t, cx))
            })
    }
}

impl Takt {
    fn title_bar(&self, t: Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let modes = [
            ViewMode::Today,
            ViewMode::Board,
            ViewMode::Outline,
            ViewMode::Matrix,
        ];
        div()
            .h(px(TITLE_BAR_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .pl(px(80.))
            .pr_2()
            .bg(t.chrome)
            .border_b_1()
            .border_color(t.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_color(t.text).child(self.app.scope_title()))
                    .when(self.app.prefs.hide_completed, |d| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(t.placeholder)
                                .child("finished hidden"),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .children(modes.into_iter().enumerate().map(|(ix, mode)| {
                        let current = self.app.view_mode == mode;
                        div()
                            .id(("mode", ix))
                            .px_2()
                            .py(px(2.))
                            .rounded(px(4.))
                            .text_size(px(12.))
                            .text_color(if current { t.text } else { t.muted })
                            .when(current, |d| d.bg(t.selected))
                            .hover(|d| d.bg(t.hover))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.app.select_view_mode(mode);
                                this.after_change(cx);
                                cx.notify();
                            }))
                            .child(mode.title())
                    })),
            )
    }

    fn sidebar(&self, t: Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.app.region == Region::Sidebar;
        let new_item = match &self.app.editing {
            Some(Editing::NewList { .. }) => Some("New list"),
            Some(Editing::NewFolder { .. }) => Some("New folder"),
            _ => None,
        };
        div()
            .w(px(self.app.prefs.sidebar_width.clamp(180., 420.)))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(t.surface)
            .border_r_1()
            .border_color(t.border)
            .when_some(new_item, |d, placeholder| {
                d.child(self.field_row(t, placeholder, 0., true))
            })
            .child(
                uniform_list(
                    "sidebar",
                    self.app.sidebar_rows.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|ix| this.sidebar_row(ix, active, t, cx))
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(self.sidebar_scroll.clone())
                .flex_1()
                .py_1(),
            )
    }

    fn sidebar_row(
        &self,
        ix: usize,
        active: bool,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = self.app.sidebar_rows[ix].clone();
        let current = self.app.row_is_current(&row);
        let cursor = active
            && self
                .app
                .sidebar_cursor_row()
                .is_some_and(|cursor| cursor.id == row.id);
        if let Some(Editing::SidebarName { row_id }) = &self.app.editing
            && *row_id == row.id
        {
            return self
                .field_row(t, "Name", row.depth as f32 * 14., true)
                .into_any_element();
        }
        let (icon, count) = match &row.kind {
            RowKind::Today => ("◎", None),
            RowKind::Everything => ("∞", None),
            RowKind::List(id) => (
                if self.app.inbox().is_some_and(|inbox| &inbox.id == id) {
                    "⇣"
                } else {
                    "≡"
                },
                self.app.task_counts.get(id).copied(),
            ),
            RowKind::NestedList(_) => ("↳", None),
            RowKind::Folder(id) => (
                if self.app.prefs.expanded_folders.contains(id) {
                    "▾"
                } else {
                    "▸"
                },
                None,
            ),
            RowKind::FilterList(_) => ("⌕", None),
        };
        let title = self.row_title(&row.kind);
        let target = row.clone();
        div()
            .id(("sidebar-row", ix))
            .h(px(ROW_HEIGHT))
            .mx_1()
            .pl(px(8. + row.depth as f32 * 14.))
            .pr_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(4.))
            .border_1()
            .border_color(if cursor {
                t.accent
            } else {
                gpui::transparent_black()
            })
            .when(current, |d| d.bg(t.selected))
            .when(!current, |d| d.hover(|d| d.bg(t.hover)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.app.apply_sidebar_cursor(&target);
                this.app.region = Region::Sidebar;
                this.after_change(cx);
                cx.notify();
            }))
            .child(
                div()
                    .w(px(12.))
                    .flex_none()
                    .text_color(t.placeholder)
                    .text_size(px(11.))
                    .child(icon),
            )
            .child(div().flex_1().min_w_0().truncate().child(title))
            .when_some(count.filter(|c| *c > 0), |d, count| {
                d.child(
                    div()
                        .text_size(px(11.))
                        .text_color(t.placeholder)
                        .child(count.to_string()),
                )
            })
            .into_any_element()
    }

    fn centre(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let body = match self.app.view_mode {
            ViewMode::Outline => self.outline(t, cx),
            ViewMode::Board => self.board(t, cx),
            ViewMode::Matrix => self.matrix(t, cx),
            ViewMode::Today => self.today(t, cx),
        };
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(body)
            .into_any_element()
    }

    fn empty(&self, t: Tokens, text: &str) -> AnyElement {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_color(t.placeholder)
            .child(text.to_string())
            .into_any_element()
    }

    fn outline(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let display = self.outline_display();
        if display.is_empty() {
            return self.empty(t, "No tasks here. Press o to add one.");
        }
        let counts = self.app.child_counts();
        let active = self.app.region == Region::Tasks;
        uniform_list(
            "outline",
            display.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|ix| match display[ix] {
                        Display::Draft(depth) => this
                            .field_row(t, "New task", 28. + depth as f32 * 18., true)
                            .into_any_element(),
                        Display::Task(row) => {
                            let row = this.app.rows[row].clone();
                            let counted = counts.get(&row.task.id).copied();
                            this.outline_row(ix, &row, counted, active, t, cx)
                        }
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(self.main_scroll.clone())
        .flex_1()
        .py_1()
        .into_any_element()
    }

    fn outline_row(
        &self,
        ix: usize,
        row: &OutlineRow,
        counted: Option<(usize, usize)>,
        active: bool,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.app.selected_task.as_deref() == Some(row.task.id.as_str());
        let folded = self.app.prefs.folded_tasks.contains(&row.task.id);
        let editing = matches!(&self.app.editing, Some(Editing::TaskTitle { task_id }) if *task_id == row.task.id);
        let disclosure = if row.has_children {
            if folded { "▸" } else { "▾" }
        } else {
            ""
        };
        let id = row.task.id.clone();
        div()
            .id(("outline-row", ix))
            .h(px(ROW_HEIGHT))
            .mx_1()
            .pl(px(8. + row.depth as f32 * 18.))
            .pr_3()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(4.))
            .border_1()
            .border_color(if selected && active {
                t.accent
            } else {
                gpui::transparent_black()
            })
            .when(selected, |d| d.bg(t.selected))
            .when(!selected, |d| d.hover(|d| d.bg(t.hover)))
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                this.app.selected_task = Some(id.clone());
                this.app.region = Region::Tasks;
                if event.click_count() >= 2 {
                    this.app.run("taskRename", None);
                    this.begin_editing();
                }
                cx.notify();
            }))
            .child(
                div()
                    .w(px(10.))
                    .flex_none()
                    .text_size(px(10.))
                    .text_color(t.placeholder)
                    .child(disclosure),
            )
            .child(check(&row.task, t))
            .child(if editing {
                self.field_inline(t, "Task").into_any_element()
            } else {
                title(&row.task, t).into_any_element()
            })
            .children(task_chips(&row.task, counted, t))
            .into_any_element()
    }

    fn board(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let board = &self.app.board;
        if board.columns.is_empty() {
            return self.empty(t, "No columns.");
        }
        let active = self.app.region == Region::Tasks;
        div()
            .id("board")
            .flex_1()
            .min_h_0()
            .flex()
            .gap_2()
            .p_2()
            .overflow_x_scroll()
            .children(board.columns.iter().enumerate().map(|(column_ix, column)| {
                let cards = board.cards.get(column_ix).cloned().unwrap_or_default();
                let focused = column_ix == board.column && active;
                div()
                    .id(("column", column_ix))
                    .w(px(272.))
                    .flex_none()
                    .h_full()
                    .flex()
                    .flex_col()
                    .rounded(px(6.))
                    .bg(t.surface)
                    .border_1()
                    .border_color(if focused { t.accent } else { t.border_variant })
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .flex()
                            .justify_between()
                            .border_b_1()
                            .border_color(t.border_variant)
                            .child(div().text_color(t.muted).child(column.title.clone()))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(t.placeholder)
                                    .child(cards.len().to_string()),
                            ),
                    )
                    .child(
                        div()
                            .id(("cards", column_ix))
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p_1()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .children(cards.iter().enumerate().map(|(ix, task)| {
                                self.card(column_ix * 100_000 + ix, task, t, cx)
                            })),
                    )
            }))
            .into_any_element()
    }

    fn card(&self, ix: usize, task: &TaskRow, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.app.selected_task.as_deref() == Some(task.id.as_str());
        let active = self.app.region == Region::Tasks;
        let editing = matches!(&self.app.editing, Some(Editing::TaskTitle { task_id }) if *task_id == task.id);
        let id = task.id.clone();
        let list = match self.app.scope {
            Scope::List { .. } => None,
            _ => self.app.list(&task.list_id).map(|list| list.name.clone()),
        };
        div()
            .id(("card", ix))
            .px_2()
            .py_1()
            .rounded(px(4.))
            .bg(t.background)
            .border_1()
            .border_color(if selected && active {
                t.accent
            } else {
                t.border_variant
            })
            .when(selected, |d| d.bg(t.selected))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.app.selected_task = Some(id.clone());
                this.app.region = Region::Tasks;
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(check(task, t))
                    .child(if editing {
                        self.field_inline(t, "Task").into_any_element()
                    } else {
                        title(task, t).into_any_element()
                    }),
            )
            .when(list.is_some() || task.due_at_ms.is_some(), |d| {
                d.child(
                    div()
                        .pl(px(20.))
                        .flex()
                        .gap_2()
                        .text_size(px(11.))
                        .text_color(t.placeholder)
                        .when_some(list, |d, list| d.child(list))
                        .children(due_chip(task, t)),
                )
            })
            .into_any_element()
    }

    fn matrix(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let quadrant = |q: usize, this: &Self, cx: &mut Context<Self>| {
            let cards = this.app.board.quadrants.get(q).cloned().unwrap_or_default();
            div()
                .id(("quadrant", q))
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .rounded(px(6.))
                .bg(t.surface)
                .border_1()
                .border_color(t.border_variant)
                .child(
                    div()
                        .px_2()
                        .py_1()
                        .flex()
                        .justify_between()
                        .border_b_1()
                        .border_color(t.border_variant)
                        .child(div().text_color(t.muted).child(QUADRANT_TITLES[q]))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(t.placeholder)
                                .child(format!("⌥{}  {}", q + 1, cards.len())),
                        ),
                )
                .child(
                    div()
                        .id(("quadrant-cards", q))
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .p_1()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(cards.iter().enumerate().map(|(ix, task)| {
                            this.card(1_000_000 + q * 100_000 + ix, task, t, cx)
                        })),
                )
        };
        let unplaced = self.app.board.unplaced.clone();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap_2()
                    .child(quadrant(0, self, cx))
                    .child(quadrant(1, self, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap_2()
                    .child(quadrant(2, self, cx))
                    .child(quadrant(3, self, cx)),
            )
            .child(
                div()
                    .id("unplaced")
                    .h(px(120.))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(t.border_variant)
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .text_color(t.muted)
                            .child(format!("Not placed  {}", unplaced.len())),
                    )
                    .child(
                        div()
                            .id("unplaced-cards")
                            .flex_1()
                            .overflow_y_scroll()
                            .p_1()
                            .flex()
                            .flex_wrap()
                            .gap_1()
                            .children(
                                unplaced
                                    .iter()
                                    .enumerate()
                                    .map(|(ix, task)| self.card(9_000_000 + ix, task, t, cx)),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn today(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let today = &self.app.today;
        if today.day.is_empty() && today.ladder.is_empty() {
            return self.empty(t, "Nothing planned or due today.");
        }
        let section = |label: String| {
            div()
                .px_3()
                .pt_3()
                .pb_1()
                .text_size(px(11.))
                .text_color(t.placeholder)
                .child(label)
        };
        let mut index = 0usize;
        let mut rows: Vec<AnyElement> = Vec::new();
        rows.push(section(format!("Today · {}", today.day.len())).into_any_element());
        for row in &today.day {
            rows.push(self.day_row(index, row, t, cx));
            index += 1;
        }
        if !today.ladder.is_empty() {
            rows.push(
                section(format!("Up next · {} ranked", today.ladder_count)).into_any_element(),
            );
            for row in &today.ladder {
                rows.push(self.day_row(index, row, t, cx));
                index += 1;
            }
        }
        if today.blocked_count > 0 {
            rows.push(
                section(format!("{} ruled out by conditions", today.blocked_count))
                    .into_any_element(),
            );
        }
        div()
            .id("today")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .children(rows)
            .into_any_element()
    }

    fn day_row(&self, ix: usize, row: &DayRow, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.app.selected_task.as_deref() == Some(row.task.id.as_str());
        let active = self.app.region == Region::Tasks;
        let id = row.task.id.clone();
        let list = self
            .app
            .list(&row.task.list_id)
            .map(|list| list.name.clone())
            .unwrap_or_default();
        let reason = row
            .explanation
            .clone()
            .unwrap_or_else(|| reason_title(&row.reason).into());
        div()
            .id(("day-row", ix))
            .h(px(ROW_HEIGHT + 4.))
            .mx_1()
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(4.))
            .border_1()
            .border_color(if selected && active {
                t.accent
            } else {
                gpui::transparent_black()
            })
            .when(selected, |d| d.bg(t.selected))
            .when(!selected, |d| d.hover(|d| d.bg(t.hover)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.app.selected_task = Some(id.clone());
                this.app.region = Region::Tasks;
                cx.notify();
            }))
            .child(check(&row.task, t))
            .child(title(&row.task, t))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(t.placeholder)
                    .child(format!("{list} · {reason}")),
            )
            .into_any_element()
    }

    fn inspector_pane(&self, t: Tokens, snapshot: Option<EditorSnapshot>) -> impl IntoElement {
        let active = self.app.region == Region::Inspector;
        let body: AnyElement = match (snapshot, self.app.selected()) {
            (Some(snapshot), Some(task)) => {
                let list = self
                    .app
                    .list(&task.list_id)
                    .map(|list| list.name.clone())
                    .unwrap_or_default();
                let field = |label: &str, value: String| {
                    div()
                        .flex()
                        .gap_2()
                        .py_1()
                        .border_b_1()
                        .border_color(t.border_variant)
                        .child(
                            div()
                                .w(px(80.))
                                .flex_none()
                                .text_color(t.placeholder)
                                .child(label.to_string()),
                        )
                        .child(div().flex_1().min_w_0().child(value))
                };
                let dash = || "—".to_string();
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::MEDIUM)
                            .pb_2()
                            .child(snapshot.title.clone()),
                    )
                    .child(field("List", list))
                    .child(field("Status", status_title(&task.status).into()))
                    .child(field(
                        "Due",
                        snapshot.due_at_ms.map(format_day).unwrap_or_else(dash),
                    ))
                    .child(field(
                        "Estimate",
                        snapshot
                            .estimate_seconds
                            .map(format_duration)
                            .unwrap_or_else(dash),
                    ))
                    .child(field(
                        "Priority",
                        snapshot
                            .metadata
                            .priority
                            .map(|p| format!("P{p}"))
                            .unwrap_or_else(dash),
                    ))
                    .child(field(
                        "Tags",
                        if snapshot.metadata.tags.is_empty() {
                            dash()
                        } else {
                            snapshot.metadata.tags.join(", ")
                        },
                    ))
                    .child(field(
                        "Repeats",
                        snapshot
                            .metadata
                            .recurrence_rule
                            .clone()
                            .unwrap_or_else(dash),
                    ))
                    .child(
                        div()
                            .pt_2()
                            .text_color(if snapshot.notes.is_empty() {
                                t.placeholder
                            } else {
                                t.text
                            })
                            .child(if snapshot.notes.is_empty() {
                                "No notes".to_string()
                            } else {
                                snapshot.notes.clone()
                            }),
                    )
                    .into_any_element()
            }
            _ => div()
                .text_color(t.placeholder)
                .child("Nothing selected")
                .into_any_element(),
        };
        div()
            .id("inspector")
            .w(px(self.app.prefs.right_dock_width.clamp(240., 520.)))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .p_3()
            .bg(t.surface)
            .border_l_1()
            .border_color(if active { t.accent } else { t.border })
            .child(body)
    }

    fn status_bar(&self, t: Tokens) -> impl IntoElement {
        let prefix = self.app.sequence.prefix().to_string();
        let pending = !prefix.is_empty() || self.app.pending_delete.is_some();
        let left = if !prefix.is_empty() {
            format!("{prefix}…")
        } else if let Some(message) = &self.app.message {
            message.clone()
        } else {
            match self.app.region {
                Region::Sidebar => "Sidebar".into(),
                Region::Tasks => self.app.view_mode.title().into(),
                Region::Inspector => "Inspector".into(),
            }
        };
        let right = [
            self.app
                .undo_label
                .as_ref()
                .map(|label| format!("⌘Z {label}")),
            self.last_key_ms.map(|ms| format!("{ms:.2} ms")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("   ");
        div()
            .h(px(24.))
            .flex_none()
            .px_3()
            .flex()
            .items_center()
            .justify_between()
            .bg(t.chrome)
            .border_t_1()
            .border_color(t.border)
            .text_size(px(11.))
            .text_color(t.muted)
            .child(
                div()
                    .truncate()
                    .when(pending, |d| d.text_color(t.warning))
                    .child(left),
            )
            .child(div().font_family(MONO_FONT).child(right))
    }

    fn overlay_view(&self, t: Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let placeholder = match self.app.overlay {
            Some(Overlay::CommandPalette) => "Run a command…",
            Some(Overlay::ListNavigator) => "Go to a list…",
            Some(Overlay::Search) => "Search tasks…",
            Some(Overlay::KeyboardReference) => "Filter keys…",
            None => "",
        };
        let items = self.overlay.items.clone();
        let selected = self.overlay.index;
        div()
            .absolute()
            .top(px(TITLE_BAR_HEIGHT + 8.))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .id("overlay")
                    .occlude()
                    .w(px(560.))
                    .max_h(px(440.))
                    .flex()
                    .flex_col()
                    .rounded(px(8.))
                    .bg(t.surface)
                    .border_1()
                    .border_color(t.border)
                    .shadow_lg()
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .border_b_1()
                            .border_color(t.border_variant)
                            .child(field_text(&self.overlay.field, placeholder, t)),
                    )
                    .child(
                        uniform_list(
                            "overlay-items",
                            items.len(),
                            cx.processor(move |_, range: std::ops::Range<usize>, _, _| {
                                range
                                    .map(|ix| {
                                        let item = &items[ix];
                                        div()
                                            .id(("overlay-item", ix))
                                            .h(px(ROW_HEIGHT + 2.))
                                            .mx_1()
                                            .px_2()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .rounded(px(4.))
                                            .when(ix == selected, |d| d.bg(t.selected))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .truncate()
                                                    .child(item.title.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(t.placeholder)
                                                    .child(item.detail.clone()),
                                            )
                                            .child(
                                                div()
                                                    .font_family(MONO_FONT)
                                                    .text_size(px(11.))
                                                    .text_color(t.muted)
                                                    .child(item.keys.clone()),
                                            )
                                    })
                                    .collect::<Vec<_>>()
                            }),
                        )
                        .h(px(380.))
                        .py_1(),
                    ),
            )
    }

    /// A text field drawn as a row: the draft task, a new list's name.
    fn field_row(
        &self,
        t: Tokens,
        placeholder: &str,
        indent: f32,
        _focused: bool,
    ) -> impl IntoElement {
        div()
            .h(px(ROW_HEIGHT))
            .mx_1()
            .pl(px(8. + indent))
            .pr_3()
            .flex()
            .items_center()
            .rounded(px(4.))
            .border_1()
            .border_color(t.accent)
            .bg(t.background)
            .child(field_text(&self.field, placeholder, t))
    }

    fn field_inline(&self, t: Tokens, placeholder: &str) -> impl IntoElement {
        div()
            .flex_1()
            .min_w_0()
            .child(field_text(&self.field, placeholder, t))
    }
}

/// The field's text with its cursor, or the placeholder.
fn field_text(field: &TextField, placeholder: &str, t: Tokens) -> impl IntoElement {
    let cursor = || div().w(px(1.5)).h(px(15.)).bg(t.accent);
    if field.text.is_empty() {
        return div().flex().items_center().child(cursor()).child(
            div()
                .text_color(t.placeholder)
                .child(placeholder.to_string()),
        );
    }
    if field.all_selected {
        return div()
            .flex()
            .items_center()
            .child(div().bg(t.accent.opacity(0.3)).child(field.text.clone()));
    }
    let (before, after) = field.split();
    div()
        .flex()
        .items_center()
        .whitespace_nowrap()
        .child(SharedString::from(before.to_string()))
        .child(cursor())
        .child(SharedString::from(after.to_string()))
}

fn check(task: &TaskRow, t: Tokens) -> impl IntoElement {
    let (glyph, colour): (&str, Hsla) = match task.status.as_str() {
        "completed" => ("●", t.success),
        "cancelled" => ("✕", t.placeholder),
        _ if task.item_kind.as_deref() == Some("list") => ("≡", t.muted),
        _ => ("○", t.muted),
    };
    div()
        .w(px(12.))
        .flex_none()
        .text_size(px(11.))
        .text_color(colour)
        .child(glyph)
}

fn title(task: &TaskRow, t: Tokens) -> impl IntoElement {
    let done = task.status != "open";
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .when(done, |d| d.text_color(t.placeholder).line_through())
        .child(task.title.clone())
}

fn task_chips(task: &TaskRow, counted: Option<(usize, usize)>, t: Tokens) -> Vec<AnyElement> {
    let mut chips: Vec<AnyElement> = due_chip(task, t).into_iter().collect();
    if let Some((done, total)) = counted.filter(|(_, total)| *total > 0) {
        chips.push(
            div()
                .text_size(px(11.))
                .text_color(t.placeholder)
                .font_family(MONO_FONT)
                .child(format!("{done}/{total}"))
                .into_any_element(),
        );
    }
    if !task.notes.trim().is_empty() {
        chips.push(
            div()
                .text_size(px(11.))
                .text_color(t.placeholder)
                .child("¶")
                .into_any_element(),
        );
    }
    chips
}

fn due_chip(task: &TaskRow, t: Tokens) -> Option<AnyElement> {
    let due = task.due_at_ms?;
    let overdue = task.status == "open" && due < start_of_today_ms();
    Some(
        div()
            .text_size(px(11.))
            .text_color(if overdue { t.error } else { t.placeholder })
            .child(format_day(due))
            .into_any_element(),
    )
}

fn start_of_today_ms() -> i64 {
    use chrono::{Local, TimeZone};
    let midnight = Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap_or_default();
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map_or(0, |time| time.timestamp_millis())
}

/// "Today", "Tomorrow", "Mon 3 Nov".
fn format_day(ms: i64) -> String {
    use chrono::{Local, TimeZone};
    let Some(time) = Local.timestamp_millis_opt(ms).single() else {
        return String::new();
    };
    let today = Local::now().date_naive();
    let day = time.date_naive();
    match (day - today).num_days() {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        -1 => "Yesterday".into(),
        _ => day.format("%a %-d %b").to_string(),
    }
}

fn format_duration(seconds: i64) -> String {
    let minutes = seconds / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

fn status_title(status: &str) -> &'static str {
    match status {
        "completed" => "Done",
        "cancelled" => "Invalidated",
        _ => "Open",
    }
}

fn reason_title(reason: &str) -> &'static str {
    match reason {
        "running" => "running",
        "planned" => "planned",
        "overdue" => "overdue",
        "dueToday" => "due today",
        "startsToday" => "starts today",
        "daily" => "daily",
        "dueSoon" => "due soon",
        "today" => "today",
        "importance" => "important",
        "priority" => "priority",
        "order" => "next in order",
        "condition" => "fits now",
        "started" => "started",
        "deadlineRisk" => "deadline at risk",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_as_hours_and_minutes() {
        assert_eq!(format_duration(25 * 60), "25m");
        assert_eq!(format_duration(2 * 3600), "2h");
        assert_eq!(format_duration(90 * 60), "1h 30m");
    }
}
