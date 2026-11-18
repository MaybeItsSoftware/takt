use super::board_links::{self, End};
use super::*;
use crate::state::board::{CardPlace, CardRow};
use crate::state::columns::ColumnName;

mod matrix;

impl Takt {
    pub(super) fn centre(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let body = match self.app.view_mode {
            ViewMode::Outline if self.app.scope == Scope::Today => self.today(t, cx),
            ViewMode::Outline => self.outline(t, cx),
            ViewMode::Board => self.board(t, cx),
            ViewMode::Matrix => self.matrix(t, cx),
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
        // What the rows are, over them, as the Today view titles its own:
        // the list's name and how many are open.
        let open = self
            .app
            .rows
            .iter()
            .filter(|row| row.task.status == "open")
            .count();
        let heading = div()
            .px_3()
            .pt_2()
            .pb_1()
            .text_size(px(11.))
            .text_color(t.placeholder)
            .child(format!("{} · {open}", self.app.scope_title()));
        let list = uniform_list(
            "outline",
            display.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .map(|ix| match display[ix] {
                        Display::Draft(depth) => this.draft_row(depth, t),
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
        .pb_1();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(heading)
            .child(list)
            .into_any_element()
    }

    /// The task being drafted, laid out as `outline_row` lays a task: the
    /// indent, fold-arrow and status-mark gutters outside the box, so the
    /// box lines up with its siblings' and its text with their titles.
    fn draft_row(&self, depth: i64, t: Tokens) -> AnyElement {
        div()
            .h(px(ROW_HEIGHT))
            .mx_1()
            .pl(px(2. + depth as f32 * 18.))
            .flex()
            .items_center()
            .child(div().w(px(10. + 4. + 12.)).flex_none())
            // The same box as a task row's, so it stands clear of a
            // selected row above or below rather than touching it.
            .child(
                div()
                    .h(px(ROW_BOX_HEIGHT))
                    .flex_1()
                    .min_w_0()
                    .pl(px(6.))
                    .pr_3()
                    .flex()
                    .items_center()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(t.accent)
                    .bg(t.background)
                    .child(self.field_inline(t, "New task")),
            )
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
        let celebrating = self.app.celebrating(&row.task.id);
        let element = div()
            .id(("outline-row", ix))
            .h(px(ROW_BOX_HEIGHT))
            .flex_1()
            .min_w_0()
            .pl(px(6.))
            .pr_3()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(4.))
            .border_1()
            .border_color(if selected && active {
                t.focus
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
            .child(if editing {
                self.field_inline(t, "Task").into_any_element()
            } else {
                title_greyed(&row.task, self.app.is_greyed(&row.task.id), t).into_any_element()
            })
            .children(detail_chips(&self.app, &row.task, t))
            .children(task_chips(&row.task, counted, t));
        // The indent, the fold arrow and the status mark are outside the
        // row, so the selection and hover box starts at the title rather
        // than around the blank space before it.
        div()
            .h(px(ROW_HEIGHT))
            .mx_1()
            .pl(px(2. + row.depth as f32 * 18.))
            .flex()
            .items_center()
            .child(
                div()
                    .w(px(10.))
                    .flex_none()
                    .text_size(px(10.))
                    .text_color(t.placeholder)
                    .child(disclosure),
            )
            .child(div().w(px(4.)).flex_none())
            .child(check(&row.task, t))
            .child(super::celebrate::celebrate(
                element,
                &row.task.id,
                celebrating,
                t,
            ))
            .into_any_element()
    }

    pub(super) fn board(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let board = &self.app.board;
        if board.columns.is_empty() {
            return self.empty(t, "No columns.");
        }
        let active = self.app.region == Region::Tasks;
        self.board_links.borrow_mut().clear();
        let focus: Vec<String> = [
            self.app.selected_task.clone(),
            self.hover_card.clone(),
            self.hover_row.clone(),
        ]
        .into_iter()
        .flatten()
        .collect();
        {
            let mut scrolls = self.card_scrolls.borrow_mut();
            if scrolls.len() < board.columns.len() {
                scrolls.resize_with(board.columns.len(), gpui::ScrollHandle::new);
            }
        }
        let columns = div()
            .id("board")
            .relative()
            .size_full()
            .flex()
            .gap(px(board_links::GUTTER))
            .p_2()
            .overflow_x_scroll()
            .track_scroll(&self.board_scroll)
            // Sideways only from a sideways swipe: gpui would otherwise turn
            // scrolling down into scrolling across.
            .map(|mut d| {
                d.style().restrict_scroll_to_axis = Some(true);
                d
            })
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
                    .relative()
                    .flex()
                    .flex_col()
                    .rounded(px(6.))
                    .bg(t.surface)
                    .border_1()
                    .border_color(if focused { t.focus } else { t.border_variant })
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
                            .flex_1()
                            .min_h_0()
                            .relative()
                            .child(board_links::measure(
                                &self.board_links,
                                End::Band(column_ix),
                            ))
                            .child(
                                div()
                                    .id(("cards", column_ix))
                                    .size_full()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.card_scrolls.borrow()[column_ix])
                                    .p_1()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .children({
                                        let place = CardPlace::Column(column_ix);
                                        let after = self.draft_after(place);
                                        let under = self.subtask_draft_card(column_ix);
                                        let mut drawn: Vec<AnyElement> = Vec::new();
                                        for (ix, task) in cards.iter().enumerate() {
                                            drawn.push(self.card(
                                                column_ix * 100_000 + ix,
                                                task,
                                                t,
                                                cx,
                                            ));
                                            if after.as_deref() == Some(task.id.as_str()) {
                                                drawn.push(self.draft_card(t));
                                            }
                                            if under.as_deref() == Some(task.id.as_str()) {
                                                drawn.push(self.draft_subtask(t));
                                            }
                                        }
                                        let below = after.as_deref().is_some_and(|id| {
                                            cards.iter().any(|task| task.id == id)
                                        });
                                        if self.drafting(place) && !below {
                                            drawn.push(self.draft_card(t));
                                        }
                                        drawn
                                    }),
                            ),
                    )
                    .child(board_links::measure(
                        &self.board_links,
                        End::Column(column_ix),
                    ))
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
            );
        // The links' lines over the columns sit outside the scrolling row:
        // inside it, a layer the row's size counts as content, and gpui
        // adds the row's padding to that, so a board that fits could still
        // be scrolled sideways by the padding.
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .child(columns)
            .child(board_links::overlay(&self.board_links, focus, t.accent))
            .into_any_element()
    }

    fn drafting(&self, place: CardPlace) -> bool {
        matches!(&self.app.editing, Some(Editing::NewCard { place: p, .. }) if *p == place)
    }

    /// The card in `column` a subtask being drafted on the board belongs
    /// to: the draft is drawn under it.
    fn subtask_draft_card(&self, column: usize) -> Option<String> {
        if self.app.view_mode != ViewMode::Board {
            return None;
        }
        match &self.app.editing {
            Some(Editing::NewTask {
                parent_id: Some(parent),
                ..
            }) => self.app.card_in_column(parent, column),
            _ => None,
        }
    }

    /// A subtask being typed on the board, set in under its card.
    fn draft_subtask(&self, t: Tokens) -> AnyElement {
        div()
            .ml(px(16.))
            .px_2()
            .py_1()
            .rounded(px(4.))
            .bg(t.background)
            .border_1()
            .border_color(t.accent)
            .flex()
            .items_center()
            .child(self.field_inline(t, "New subtask"))
            .into_any_element()
    }

    /// The card a draft in `place` is drawn below, if any.
    fn draft_after(&self, place: CardPlace) -> Option<String> {
        match &self.app.editing {
            Some(Editing::NewCard {
                place: p, after, ..
            }) if *p == place => after.clone(),
            _ => None,
        }
    }

    /// The new card being typed, below the selected card or at the foot of
    /// its column or quadrant.
    fn draft_card(&self, t: Tokens) -> AnyElement {
        div()
            .px_2()
            .py_1()
            .rounded(px(4.))
            .bg(t.background)
            .border_1()
            .border_color(t.accent)
            .flex()
            .items_center()
            .child(self.field_inline(t, "New card"))
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
        // A board card's column, for its link ends; not the matrix's.
        let column = (ix < 1_000_000).then_some(ix / 100_000);
        let celebrating = self.app.celebrating(&task.id);
        // The status mark takes a slot only when it draws something: an open
        // task's is empty, and reserving it left a gap before every title.
        let marked = task.status != "open" || task.item_kind.as_deref() == Some("list");
        let inset = if marked { px(20.) } else { px(0.) };
        let (rows, total) = self.app.card_subtree(task);
        // The cursor on one of its subtask rows: the card is still the one
        // the keyboard is in.
        let holds_cursor = self
            .app
            .selected_task
            .as_deref()
            .is_some_and(|id| rows.iter().any(|row| row.task.id == id));
        let element = div()
            .id(("card", ix))
            .on_hover({
                let id = id.clone();
                cx.listener(move |this, hovered: &bool, _, cx| {
                    let was = this.hover_card.clone();
                    if *hovered {
                        this.hover_card = Some(id.clone());
                    } else if this.hover_card.as_deref() == Some(id.as_str()) {
                        this.hover_card = None;
                    }
                    if was != this.hover_card {
                        cx.notify();
                    }
                })
            })
            .px_2()
            .py_1()
            .rounded(px(4.))
            .bg(t.background)
            .border_1()
            .border_color(if (selected || holds_cursor) && active {
                t.focus
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
                if let Some(column) = column {
                    this.app.board.column = column;
                }
                cx.notify();
            }))
            .child(
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when_some(
                        column.filter(|_| task.parent_task_id.is_some()),
                        |d, column| {
                            d.child(board_links::measure(
                                &self.board_links,
                                End::Target {
                                    child: task.id.clone(),
                                    column,
                                },
                            ))
                        },
                    )
                    .when(marked, |d| d.child(check(task, t)))
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
                        .pl(inset)
                        .flex()
                        .gap_2()
                        .text_size(px(11.))
                        .text_color(t.placeholder)
                        .when_some(list, |d, list| d.child(list))
                        .children(due_chip(task, t)),
                )
            })
            .when_some(self.app.card_parent_hint(task), |d, hint| {
                d.child(
                    div()
                        .pl(inset)
                        .text_size(px(11.))
                        .text_color(t.placeholder)
                        .truncate()
                        .child(hint),
                )
            })
            .child(self.card_subtasks(task, rows, total, column, t, cx));
        super::celebrate::celebrate(element, &task.id, celebrating, t)
    }

    /// The card's subtree as compact rows: `└` marks, flush with the card's
    /// title; a subtask filed in another column muted and naming it; a
    /// folded card or row says what it keeps out of sight.
    fn card_subtasks(
        &self,
        card: &TaskRow,
        rows: Vec<CardRow>,
        total: usize,
        column: Option<usize>,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hidden = self.app.card_folded_count(card);
        if rows.is_empty() {
            if hidden == 0 {
                return div().into_any_element();
            }
            return div()
                .pt_1()
                .text_size(px(11.))
                .text_color(t.placeholder)
                .child(format!(
                    "▸ {hidden} subtask{}",
                    if hidden == 1 { "" } else { "s" }
                ))
                .into_any_element();
        }
        let more = total.saturating_sub(rows.len());
        div()
            .pt_1()
            .flex()
            .flex_col()
            .children(rows.into_iter().map(|row| {
                let id = row.task.id.clone();
                let selected = self.app.selected_task.as_deref() == Some(id.as_str());
                let editing = matches!(&self.app.editing, Some(Editing::TaskTitle { task_id }) if *task_id == id);
                let open = row.task.status == "open";
                let status = match row.task.status.as_str() {
                    "completed" => "●",
                    "cancelled" => "✕",
                    _ => "",
                };
                let source = column
                    .filter(|_| row.elsewhere.is_some())
                    .map(|column| End::Source {
                        child: id.clone(),
                        column,
                        card: card.id.clone(),
                    });
                div()
                    .id(gpui::SharedString::from(format!("card-row-{id}")))
                    .relative()
                    .when_some(source, |d, end| {
                        d.child(board_links::measure(&self.board_links, end))
                    })
                    .on_hover({
                        let id = id.clone();
                        cx.listener(move |this, hovered: &bool, _, cx| {
                            let was = this.hover_row.clone();
                            if *hovered {
                                this.hover_row = Some(id.clone());
                            } else if this.hover_row.as_deref() == Some(id.as_str()) {
                                this.hover_row = None;
                            }
                            if was != this.hover_row {
                                cx.notify();
                            }
                        })
                    })
                    .pl(px(row.depth as f32 * 10.))
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .gap_1()
                    .rounded(px(3.))
                    .text_size(px(12.))
                    .when(selected, |d| d.bg(t.selected))
                    .hover(|d| d.bg(t.hover))
                    .text_color(if row.elsewhere.is_some() || !open {
                        t.placeholder
                    } else {
                        t.muted
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.app.selected_task = Some(id.clone());
                        this.app.region = Region::Tasks;
                        if let Some(column) = column {
                            this.app.board.column = column;
                        }
                        cx.notify();
                    }))
                    .child(div().flex_none().text_color(t.placeholder).child("└"))
                    .when(!status.is_empty(), |d| {
                        d.child(div().flex_none().child(status))
                    })
                    .child(if editing {
                        self.field_inline(t, "Task").into_any_element()
                    } else {
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .when(!open, |d| d.line_through())
                            .child(row.task.title.clone())
                            .into_any_element()
                    })
                    .when(row.folded, |d| {
                        d.child(div().flex_none().text_color(t.placeholder).child("▸"))
                    })
                    .when_some(row.elsewhere, |d, column| {
                        d.child(
                            div()
                                .flex_none()
                                .text_size(px(11.))
                                .text_color(t.accent)
                                .child(format!("→ {column}")),
                        )
                    })
            }))
            .when(more > 0, |d| {
                d.child(
                    div()
                        .text_size(px(11.))
                        .text_color(t.placeholder)
                        .child(format!("+{more} more")),
                )
            })
            .into_any_element()
    }

    pub(super) fn today(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let today = &self.app.today;
        // A task being typed here: under the task it will be a subtask of,
        // else at the foot of the day, where a planned task joins it.
        let draft = match &self.app.editing {
            Some(Editing::NewTask { parent_id, .. }) => Some(parent_id.clone()),
            _ => None,
        };
        let draft_under = |id: &str| matches!(&draft, Some(Some(parent)) if parent == id);
        let parent_drawn = matches!(&draft, Some(Some(parent))
            if today.rows().any(|row| &row.task.id == parent));
        let draft_at_foot = draft.is_some() && !parent_drawn;
        if today.day.is_empty()
            && today.dailies.is_empty()
            && today.ladder.is_empty()
            && draft.is_none()
        {
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
        // An empty day drops its heading, unless a task is being typed into it.
        if !today.day.is_empty() || draft_at_foot {
            rows.push(section(format!("Today · {}", today.day.len())).into_any_element());
        }
        for row in &today.day {
            rows.push(self.day_row(index, row, t, cx));
            if draft_under(&row.task.id) {
                rows.push(self.draft_row(1, t));
            }
            index += 1;
        }
        if draft_at_foot {
            rows.push(self.draft_row(0, t));
        }
        if !today.dailies.is_empty() {
            rows.push(
                section(format!("Dailies · {} left", today.dailies.len())).into_any_element(),
            );
            for row in &today.dailies {
                rows.push(self.day_row(index, row, t, cx));
                index += 1;
            }
        }
        if !today.ladder.is_empty() {
            rows.push(
                section(format!("Up next · {} ranked", today.ladder_count)).into_any_element(),
            );
            for row in &today.ladder {
                rows.push(self.day_row(index, row, t, cx));
                if draft_under(&row.task.id) {
                    rows.push(self.draft_row(1, t));
                }
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
        let element = div()
            .id(("day-row", ix))
            .h(px(ROW_HEIGHT + 4.))
            .flex_1()
            .min_w_0()
            .pl(px(6.))
            .pr_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(4.))
            .border_1()
            .border_color(if selected && active {
                t.focus
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
            .child(title_greyed(&row.task, self.app.is_greyed(&row.task.id), t))
            .children(detail_chips(&self.app, &row.task, t))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(t.placeholder)
                    .child(format!("{list} · {reason}")),
            );
        // As an outline row at the top level: the status mark sits outside
        // the box, so the selection starts at the title, level with a list's.
        div()
            .h(px(ROW_HEIGHT + 4.))
            .mx_1()
            .pl(px(2. + 10. + 4.))
            .flex()
            .items_center()
            .child(check(&row.task, t))
            .child(element)
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
