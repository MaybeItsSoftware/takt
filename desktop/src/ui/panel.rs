//! The focus panel: a small window that floats over everything, for the
//! block in hand once the main window is put away. The running task and its
//! clock, the queue behind it, and with nothing running the day's ladder to
//! start from. `FocusPanelController.swift` and `FocusPanelView.swift`.
//!
//! It is worked from the keyboard, a letter an action: Return starts a
//! timer on the highlighted task (or queues it behind a running block), `p`
//! a pomodoro, space pauses, `d` finishes, `s` stops. `n` opens a field for
//! a new task, filed where quick capture files, and `/` a finder over every
//! list and task; either one leaves its task highlighted, ready to start.
//! The footer spells the letters out.
//!
//! It holds no workspace state of its own, only its cursor, its field and
//! the task it picked: it reads the main window's `App` through the `Takt`
//! entity and redraws when that does, and every action runs on that same
//! `App`.

use super::Takt;
use super::widgets::{field_text, key_hint};
use crate::keys::{Press, spell};
use crate::state::App;
use crate::state::focus::{FinderHit, FocusKind};
use crate::text_field::{FieldKey, TextField};
use crate::theme::Tokens;
use gpui::{
    AnyElement, App as GpuiApp, Bounds, Context, Entity, EntityInputHandler, FocusHandle,
    Focusable, KeyDownEvent, Pixels, UTF16Selection, WeakEntity, Window, WindowBounds, WindowKind,
    WindowOptions, div, point, prelude::*, px, size,
};
use std::ops::Range;

pub struct FocusPanel {
    takt: Entity<Takt>,
    this: WeakEntity<Self>,
    focus: FocusHandle,
    cursor: usize,
    /// A task picked with `n` or `/`, listed first until it is started.
    picked: Option<String>,
    mode: Mode,
}

/// What the keyboard is doing in the panel.
enum Mode {
    /// Moving through the rows; letters are actions.
    Rows,
    /// `n`: typing a new task. Why the last Return didn't file it.
    New {
        field: TextField,
        problem: Option<String>,
    },
    /// `/`: finding a task or a list; with a list chosen, its tasks.
    Find {
        field: TextField,
        list: Option<(String, String)>,
        hits: Vec<FinderHit>,
        cursor: usize,
    },
}

impl Mode {
    fn field(&mut self) -> Option<&mut TextField> {
        match self {
            Mode::Rows => None,
            Mode::New { field, .. } | Mode::Find { field, .. } => Some(field),
        }
    }
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
                this: cx.entity().downgrade(),
                focus: cx.focus_handle(),
                cursor: 0,
                picked: None,
                mode: Mode::Rows,
            }
        });
        window.focus(&panel.focus_handle(cx));
        panel
    })
    .ok()
}

/// What the panel lists: the queue while a block runs, the day's ladder
/// when nothing does, with a task picked by `n` or `/` first when it isn't
/// already among them. (id, title, detail).
fn rows(app: &App, picked: Option<&str>) -> Vec<(String, String, String)> {
    let list_name = |list_id: &str| {
        app.list(list_id)
            .map(|l| l.name.clone())
            .unwrap_or_default()
    };
    let mut listed: Vec<(String, String, String)> = if app.focus_session.is_some() {
        app.focus_queue()
            .into_iter()
            .map(|entry| {
                let list = list_name(&entry.task.list_id);
                (entry.task.id, entry.task.title, list)
            })
            .collect()
    } else {
        app.today
            .rows()
            .filter(|row| row.task.status == "open")
            .map(|row| {
                (
                    row.task.id.clone(),
                    row.task.title.clone(),
                    list_name(&row.task.list_id),
                )
            })
            .collect()
    };
    if let Some(id) = picked
        && !listed.iter().any(|(listed, _, _)| listed == id)
        && let Some(task) = app
            .task(id)
            .cloned()
            .or_else(|| app.store.core.task(id.into()).ok().flatten())
            .filter(|task| task.status == "open")
    {
        let list = list_name(&task.list_id);
        listed.insert(0, (task.id, task.title, list));
    }
    listed
}

impl FocusPanel {
    fn listed(&self, cx: &GpuiApp) -> Vec<(String, String, String)> {
        rows(&self.takt.read(cx).app, self.picked.as_deref())
    }

    /// Highlights `id` among the rows, picking it so it stays listed.
    fn pick(&mut self, id: String, cx: &GpuiApp) {
        self.picked = Some(id.clone());
        self.cursor = self
            .listed(cx)
            .iter()
            .position(|(listed, _, _)| *listed == id)
            .unwrap_or(0);
    }

    fn refind(&mut self, cx: &GpuiApp) {
        if let Mode::Find {
            field,
            list,
            hits,
            cursor,
        } = &mut self.mode
        {
            *hits = self
                .takt
                .read(cx)
                .app
                .focus_finder(&field.text, list.as_ref().map(|(id, _)| id.as_str()));
            *cursor = 0;
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.mode, Mode::Rows) {
            self.field_key(event, cx);
            return;
        }
        let Some(key) = spell(&Press::from_keystroke(&event.keystroke)) else {
            return;
        };
        cx.stop_propagation();
        if key == "escape" && self.takt.read(cx).app.focus_prompt.is_none() {
            window.remove_window();
            return;
        }
        if self.takt.read(cx).app.focus_prompt.is_none() {
            match key.as_str() {
                "n" => {
                    self.mode = Mode::New {
                        field: TextField::default(),
                        problem: None,
                    };
                    cx.notify();
                    return;
                }
                "/" => {
                    self.mode = Mode::Find {
                        field: TextField::default(),
                        list: None,
                        hits: Vec::new(),
                        cursor: 0,
                    };
                    self.refind(cx);
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }
        let listed = self.listed(cx);
        let highlighted = listed.get(self.cursor).map(|(id, _, _)| id.clone());
        let mut close = false;
        let mut started = false;
        self.takt.update(cx, |takt, _| {
            let app = &mut takt.app;
            if app.focus_prompt.is_some() {
                app.focus_prompt_key(&key);
                return;
            }
            match key.as_str() {
                "enter" | "p" => {
                    if let Some(id) = &highlighted {
                        let kind = if key == "p" {
                            FocusKind::Pomodoro
                        } else {
                            FocusKind::Timer
                        };
                        app.focus_from_panel(id, kind);
                        started = true;
                    }
                }
                "d" => app.request_focus_completion(true),
                "s" => app.stop_focus(),
                "space" => {
                    if app
                        .focus_session
                        .as_ref()
                        .is_some_and(|s| s.phase == "onBreak")
                    {
                        app.end_focus_break();
                    } else {
                        app.toggle_focus_pause();
                    }
                }
                "cmd+w" => close = true,
                _ => {}
            }
        });
        if started && highlighted == self.picked {
            self.picked = None;
        }
        // The pill, the day and the status item catch up once this window's
        // update is over: `Takt` can't see this window while it updates.
        let takt = self.takt.clone();
        cx.defer(move |cx| takt.update(cx, |takt, cx| takt.settle(cx)));
        let count = self.listed(cx).len();
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

    /// A key while the new-task field or the finder is open.
    fn field_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
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
        let step = match (event.keystroke.key.as_str(), &outcome) {
            (_, FieldKey::Down) | ("n", _) if ctrl || outcome == FieldKey::Down => Some(1),
            (_, FieldKey::Up) | ("p", _) if ctrl || outcome == FieldKey::Up => Some(-1),
            _ => None,
        };
        match outcome {
            // Printable: the input method types it.
            FieldKey::Text(_) if step.is_none() => return,
            FieldKey::Edited => self.after_edit(cx),
            FieldKey::Commit => self.commit(cx),
            FieldKey::Cancel => self.cancel(cx),
            _ => {}
        }
        if let (Some(step), Mode::Find { hits, cursor, .. }) = (step, &mut self.mode) {
            let last = hits.len().saturating_sub(1);
            *cursor = if step > 0 {
                (*cursor + 1).min(last)
            } else {
                cursor.saturating_sub(1)
            };
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn after_edit(&mut self, cx: &GpuiApp) {
        match &mut self.mode {
            Mode::New { problem, .. } => *problem = None,
            Mode::Find { .. } => self.refind(cx),
            Mode::Rows => {}
        }
    }

    fn commit(&mut self, cx: &mut Context<Self>) {
        match &mut self.mode {
            Mode::New { field, problem } => {
                let text = field.text.clone();
                if text.trim().is_empty() {
                    self.mode = Mode::Rows;
                    return;
                }
                let filed = self.takt.update(cx, |takt, _| {
                    let filed = takt.app.quick_capture_task(&text);
                    takt.app.today_stale = true;
                    filed
                });
                match filed {
                    Ok((id, _)) => {
                        self.mode = Mode::Rows;
                        self.pick(id, cx);
                        let takt = self.takt.clone();
                        cx.defer(move |cx| takt.update(cx, |takt, cx| takt.settle(cx)));
                    }
                    Err(why) => *problem = Some(why),
                }
            }
            Mode::Find {
                field,
                list,
                hits,
                cursor,
            } => match hits.get(*cursor).cloned() {
                Some(FinderHit::Task { id, .. }) => {
                    self.mode = Mode::Rows;
                    self.pick(id, cx);
                }
                Some(FinderHit::List { id, name }) => {
                    *list = Some((id, name));
                    *field = TextField::default();
                    self.refind(cx);
                }
                None => {}
            },
            Mode::Rows => {}
        }
    }

    /// Esc: out of a chosen list back to everything, else back to the rows.
    fn cancel(&mut self, cx: &GpuiApp) {
        if let Mode::Find { list, field, .. } = &mut self.mode
            && list.is_some()
        {
            *list = None;
            *field = TextField::default();
            self.refind(cx);
            return;
        }
        self.mode = Mode::Rows;
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
        let status = app.focus_status();
        let running = app.focus_session.is_some();
        let prompt = app.focus_prompt.clone();
        let destination = app
            .quick_capture_destination()
            .map(|(_, name)| name)
            .unwrap_or_else(|| "No list".into());
        let listed = rows(app, self.picked.as_deref());
        let hook = Some((self.this.clone(), self.focus.clone()));
        let row = |here: bool, title: String, detail: String| {
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
                        .flex_none()
                        .text_size(px(11.))
                        .text_color(t.placeholder)
                        .child(detail),
                )
        };
        let heading = |text: String| {
            div()
                .px_3()
                .pt_2()
                .text_size(px(11.))
                .text_color(t.placeholder)
                .child(text)
        };
        let field_row = |field: &TextField, placeholder: &str| {
            div()
                .mx_2()
                .mt_2()
                .px_2()
                .py_1()
                .rounded(px(4.))
                .border_1()
                .border_color(t.accent)
                .child(field_text(field, placeholder, t, hook.clone()))
        };
        let (body, hints): (Vec<AnyElement>, Vec<(&str, &str)>) = match &self.mode {
            Mode::Rows => (
                vec![
                    heading(
                        if running {
                            "Up next in this session"
                        } else {
                            "Start on…"
                        }
                        .into(),
                    )
                    .into_any_element(),
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
                                .map(|(ix, (_, title, list))| row(ix == self.cursor, title, list)),
                        )
                        .into_any_element(),
                ],
                if prompt.is_some() {
                    vec![]
                } else if running {
                    vec![
                        ("↩", "queue"),
                        ("p", "queue"),
                        ("␣", "pause"),
                        ("d", "done"),
                        ("s", "stop"),
                        ("n", "new"),
                        ("/", "find"),
                        ("esc", "hide"),
                    ]
                } else {
                    vec![
                        ("↩", "timer"),
                        ("p", "pomodoro"),
                        ("j k", "move"),
                        ("n", "new"),
                        ("/", "find"),
                        ("esc", "hide"),
                    ]
                },
            ),
            Mode::New { field, problem } => (
                vec![
                    heading(format!("New task → {destination}")).into_any_element(),
                    field_row(field, "Add a task… 45m #tag @fri !1").into_any_element(),
                    div()
                        .px_3()
                        .pt_1()
                        .text_size(px(11.))
                        .text_color(t.error)
                        .when_some(problem.clone(), |d, why| d.child(why))
                        .into_any_element(),
                    div().flex_1().into_any_element(),
                ],
                vec![("↩", "add"), ("esc", "back")],
            ),
            Mode::Find {
                field,
                list,
                hits,
                cursor,
            } => (
                vec![
                    heading(match list {
                        Some((_, name)) => format!("Tasks in {name}"),
                        None => "Find a task, or a list".into(),
                    })
                    .into_any_element(),
                    field_row(
                        field,
                        if list.is_some() {
                            "Filter…"
                        } else {
                            "Search tasks and lists…"
                        },
                    )
                    .into_any_element(),
                    div()
                        .id("focus-panel-finds")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .p_1()
                        .children(hits.iter().enumerate().map(|(ix, hit)| match hit {
                            FinderHit::Task { title, list, .. } => {
                                row(ix == *cursor, title.clone(), list.clone())
                            }
                            FinderHit::List { name, .. } => {
                                row(ix == *cursor, name.clone(), "list".into())
                            }
                        }))
                        .into_any_element(),
                ],
                vec![
                    ("↑↓", "move"),
                    ("↩", "pick"),
                    ("esc", if list.is_some() { "all lists" } else { "back" }),
                ],
            ),
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
                    })
                    .when(running && prompt.is_none(), |d| {
                        d.child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .text_size(px(11.))
                                .text_color(t.placeholder)
                                .child(hint("␣", "pause", t))
                                .child(hint("d", "done", t))
                                .child(hint("s", "stop", t)),
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
            Mode::New { field, .. } | Mode::Find { field, .. } => field
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
