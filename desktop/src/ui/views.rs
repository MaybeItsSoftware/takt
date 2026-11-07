use super::*;
use crate::state::columns::ColumnName;

impl Takt {
    pub(super) fn centre(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
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

    pub(super) fn empty(&self, t: Tokens, text: &str) -> AnyElement {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_color(t.placeholder)
            .child(text.to_string())
            .into_any_element()
    }

    pub(super) fn outline(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
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

    pub(super) fn outline_row(
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
            .on_drag(
                drag_task_for(&id, &self.app, t),
                |drag: &DraggedTask, _, _, cx| cx.new(|_| drag.clone()),
            )
            .drag_over::<DraggedTask>(move |style, _, _, _| style.border_color(t.accent))
            .on_drop({
                let id = id.clone();
                cx.listener(move |this, drag: &DraggedTask, _, cx| {
                    this.app.drop_task_before(&drag.id, &id);
                    this.after_change(cx);
                    cx.notify();
                })
            })
            .on_mouse_down(gpui::MouseButton::Right, {
                let id = id.clone();
                cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                    this.app
                        .open_task_menu(&id, f32::from(e.position.x), f32::from(e.position.y));
                    cx.stop_propagation();
                    cx.notify();
                })
            })
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
                title_greyed(&row.task, self.app.is_greyed(&row.task.id), t).into_any_element()
            })
            .children(detail_chips(&self.app, &row.task, t))
            .children(task_chips(&row.task, counted, t))
            .into_any_element()
    }

    pub(super) fn board(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
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
                let renaming = matches!(
                    &self.app.editing,
                    Some(Editing::ColumnName(ColumnName::Rename(id))) if *id == column.id
                );
                let ruled = column.rules.is_some();
                div()
                    .id(("column", column_ix))
                    .drag_over::<DraggedTask>(move |style, _, _, _| style.bg(t.hover))
                    .on_drop(cx.listener(move |this, drag: &DraggedTask, _, cx| {
                        this.app.drop_task_into_column(&drag.id, column_ix);
                        this.after_change(cx);
                        cx.notify();
                    }))
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
                            .child(if renaming {
                                self.field_inline(t, "Column name").into_any_element()
                            } else {
                                div()
                                    .text_color(t.muted)
                                    .child(if ruled {
                                        format!("{} ⚙", column.title)
                                    } else {
                                        column.title.clone()
                                    })
                                    .into_any_element()
                            })
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
            .when(
                matches!(
                    self.app.editing,
                    Some(Editing::ColumnName(ColumnName::New(_)))
                ),
                |d| {
                    d.child(
                        div()
                            .w(px(272.))
                            .flex_none()
                            .p_2()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(t.accent)
                            .bg(t.surface)
                            .child(self.field_inline(t, "New column")),
                    )
                },
            )
            .into_any_element()
    }

    pub(super) fn card(
        &self,
        ix: usize,
        task: &TaskRow,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            .on_drag(
                drag_task_for(&id, &self.app, t),
                |drag: &DraggedTask, _, _, cx| cx.new(|_| drag.clone()),
            )
            .drag_over::<DraggedTask>(move |style, _, _, _| style.border_color(t.accent))
            .on_drop({
                let id = id.clone();
                cx.listener(move |this, drag: &DraggedTask, _, cx| {
                    this.app.drop_task_before(&drag.id, &id);
                    this.after_change(cx);
                    cx.notify();
                })
            })
            .on_mouse_down(gpui::MouseButton::Right, {
                let id = id.clone();
                cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                    this.app
                        .open_task_menu(&id, f32::from(e.position.x), f32::from(e.position.y));
                    cx.stop_propagation();
                    cx.notify();
                })
            })
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
                        title_greyed(task, self.app.is_greyed(&task.id), t).into_any_element()
                    })
                    .children(detail_chips(&self.app, task, t)),
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

    pub(super) fn matrix(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let hover = t.hover;
        let quadrant = |q: usize, this: &Self, cx: &mut Context<Self>| {
            let cards = this.app.board.quadrants.get(q).cloned().unwrap_or_default();
            div()
                .id(("quadrant", q))
                .drag_over::<DraggedTask>(move |style, _, _, _| style.bg(hover))
                .on_drop(cx.listener(move |this, drag: &DraggedTask, _, cx| {
                    this.app.drop_task_into_quadrant(&drag.id, q);
                    this.after_change(cx);
                    cx.notify();
                }))
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

    pub(super) fn today(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
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

    pub(super) fn day_row(
        &self,
        ix: usize,
        row: &DayRow,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            .on_drag(
                drag_task_for(&id, &self.app, t),
                |drag: &DraggedTask, _, _, cx| cx.new(|_| drag.clone()),
            )
            .drag_over::<DraggedTask>(move |style, _, _, _| style.border_color(t.accent))
            .on_drop({
                let id = id.clone();
                cx.listener(move |this, drag: &DraggedTask, _, cx| {
                    this.app.drop_task_before(&drag.id, &id);
                    this.after_change(cx);
                    cx.notify();
                })
            })
            .on_mouse_down(gpui::MouseButton::Right, {
                let id = id.clone();
                cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                    this.app
                        .open_task_menu(&id, f32::from(e.position.x), f32::from(e.position.y));
                    cx.stop_propagation();
                    cx.notify();
                })
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.app.selected_task = Some(id.clone());
                this.app.region = Region::Tasks;
                cx.notify();
            }))
            .child(check(&row.task, t))
            .child(title_greyed(&row.task, self.app.is_greyed(&row.task.id), t))
            .children(detail_chips(&self.app, &row.task, t))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(t.placeholder)
                    .child(format!("{list} · {reason}")),
            )
            .into_any_element()
    }
}

/// The drag for a row's task, by its id.
fn drag_task_for(id: &str, app: &crate::state::App, t: Tokens) -> DraggedTask {
    match app.task(id) {
        Some(task) => drag_task(task, t),
        None => DraggedTask {
            id: id.into(),
            title: String::new(),
            tokens: t,
        },
    }
}
