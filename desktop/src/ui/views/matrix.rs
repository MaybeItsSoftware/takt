//! The matrix: one plane, urgency across (urgent on the left) and
//! importance down (important at the top), with each placed task a chip
//! whose dot sits at its point (`state/matrix.rs`), and the tasks not yet
//! placed in a tray beneath. A drop anywhere on the plane places the task
//! at that point.

use super::*;
use crate::state::board::MatrixPoint;
use crate::state::matrix;
use gpui::{Bounds, Div, Pixels, canvas, relative};
use std::cell::Cell;

const CHIP_H: f32 = 20.;
const CHIP_MAX_W: f32 = 180.;
const DOT: f32 = 6.;
/// From a chip's outer edge to its dot's centre: the border, the padding
/// and half the dot.
const DOT_INSET: f32 = 1. + 4. + DOT / 2.;
/// The plane's margin round its points, so a chip at an edge stays on it
/// and clear of the quadrant names.
const MARGIN_X: f32 = 16.;
const MARGIN_Y: f32 = 28.;
/// Past this far across, a chip's title runs leftwards from its dot.
const FLIP: f32 = 0.62;

thread_local! {
    /// The points' field as last laid out, in window coordinates: where a
    /// drop lands, and how far apart chips are drawn.
    static FIELD: Cell<Option<Bounds<Pixels>>> = const { Cell::new(None) };
}

/// Where a chip is drawn: its point as fractions of the field, which way
/// its title runs, and how far it was moved down (or up) off another.
struct Drawn {
    ix: usize,
    fx: f32,
    fy: f32,
    leftwards: bool,
    dy: f32,
}

/// A chip's width, near enough to keep titles apart.
fn estimate_width(title: &str) -> f32 {
    (title.chars().count() as f32 * 6.5 + 2. * DOT_INSET + 6.).min(CHIP_MAX_W)
}

/// Each chip at its point, moved a little up or down where it would cover
/// another's title. Layout only: nothing stored moves.
fn lay_out(points: &[MatrixPoint], (width, height): (f32, f32)) -> Vec<Drawn> {
    let fractions: Vec<(f32, f32)> = points
        .iter()
        .map(|p| matrix::to_fraction(p.urgency, p.importance))
        .collect();
    let mut order: Vec<usize> = (0..points.len()).collect();
    order.sort_by(|a, b| {
        fractions[*a]
            .1
            .total_cmp(&fractions[*b].1)
            .then(fractions[*a].0.total_cmp(&fractions[*b].0))
    });
    let mut boxes: Vec<(f32, f32, f32, f32)> = Vec::new();
    let mut drawn = Vec::with_capacity(points.len());
    for ix in order {
        let (fx, fy) = fractions[ix];
        let leftwards = fx > FLIP;
        let w = estimate_width(&points[ix].task.title);
        let (x, y) = (fx * width, fy * height);
        let (x0, x1) = if leftwards {
            (x + DOT_INSET - w, x + DOT_INSET)
        } else {
            (x - DOT_INSET, x - DOT_INSET + w)
        };
        let free = |dy: f32| {
            let (y0, y1) = (y - CHIP_H / 2. + dy, y + CHIP_H / 2. + dy);
            y0 >= -MARGIN_Y
                && y1 <= height + MARGIN_Y
                && !boxes
                    .iter()
                    .any(|b| b.0 < x1 && x0 < b.1 && b.2 < y1 && y0 < b.3)
        };
        let step = CHIP_H / 2. + 1.;
        let dy = (0..40)
            .flat_map(|n| [n as f32 * step, -(n as f32) * step])
            .find(|dy| free(*dy))
            .unwrap_or(0.);
        boxes.push((x0, x1, y - CHIP_H / 2. + dy, y + CHIP_H / 2. + dy));
        drawn.push(Drawn {
            ix,
            fx,
            fy,
            leftwards,
            dy,
        });
    }
    drawn
}

/// An invisible child filling the field that records its frame, and asks
/// for one more frame when it changed so the chips are spaced for it.
fn measure_field() -> impl IntoElement {
    canvas(
        |bounds, window, cx| {
            let was = FIELD.with(|field| field.replace(Some(bounds)));
            if was.is_some_and(|was| was.size != bounds.size) {
                let handle = window.window_handle();
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, _| window.refresh());
                });
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// A zero-sized anchor at a fraction across and down its parent, for what
/// is drawn relative to a point.
fn anchor(fx: f32, fy: f32) -> Div {
    div()
        .absolute()
        .left(relative(fx))
        .top(relative(fy))
        .size(px(0.))
}

impl Takt {
    pub(in crate::ui) fn matrix(&self, t: Tokens, cx: &mut Context<Self>) -> AnyElement {
        let hover = t.hover;
        let points = self.app.board.points.clone();
        let size = FIELD
            .with(Cell::get)
            .map(|b| (f32::from(b.size.width), f32::from(b.size.height)))
            .unwrap_or((800., 480.));
        let drawn = lay_out(&points, size);
        let counts: Vec<usize> = self.app.board.quadrants.iter().map(Vec::len).collect();
        let micro = |text: String| {
            div()
                .text_size(px(10.))
                .font_weight(FontWeight::BOLD)
                .text_color(t.placeholder)
                .whitespace_nowrap()
                .child(text)
        };
        let corner = |q: usize| {
            micro(format!(
                "⌥{}  {}  {}",
                q + 1,
                QUADRANT_TITLES[q].to_uppercase(),
                counts.get(q).copied().unwrap_or(0)
            ))
            .absolute()
            .when(q < 2, |d| d.top(px(6.)))
            .when(q >= 2, |d| d.bottom(px(6.)))
            .when(q.is_multiple_of(2), |d| d.left(px(8.)))
            .when(!q.is_multiple_of(2), |d| d.right(px(8.)))
        };
        let label = |text: &'static str| {
            div()
                .absolute()
                .text_size(px(11.))
                .text_color(t.muted)
                .whitespace_nowrap()
                .child(text)
        };
        let tick = |fx: f32, fy: f32, across: bool| {
            anchor(fx, fy).child(
                div()
                    .absolute()
                    .bg(t.border)
                    .when(across, |d| d.left(px(0.)).top(px(-3.)).w(px(1.)).h(px(7.)))
                    .when(!across, |d| d.top(px(0.)).left(px(-3.)).h(px(1.)).w(px(7.))),
            )
        };
        let draft = (0..4)
            .find(|q| self.drafting(CardPlace::Quadrant(Some(*q))))
            .map(|q| {
                let (u, i) = matrix::centre_of(q);
                let (fx, fy) = matrix::to_fraction(u, i);
                anchor(fx, fy).child(
                    div()
                        .absolute()
                        .left(px(-DOT_INSET))
                        .top(px(-CHIP_H / 2. - 4.))
                        .w(px(CHIP_MAX_W))
                        .child(self.draft_card(t)),
                )
            });
        let plane = div()
            .id("matrix-plane")
            .relative()
            .flex_1()
            .min_h_0()
            .rounded(px(6.))
            .border_1()
            .border_color(t.border_variant)
            .bg(t.background)
            .drag_over::<DraggedTask>(move |style, _, _, _| style.bg(hover))
            .on_drop(cx.listener(move |this, drag: &DraggedTask, window, cx| {
                let Some(field) = FIELD.with(Cell::get) else {
                    return;
                };
                let at = window.mouse_position();
                let x = f32::from(at.x - field.origin.x) / f32::from(field.size.width).max(1.);
                let y = f32::from(at.y - field.origin.y) / f32::from(field.size.height).max(1.);
                this.app.region = Region::Tasks;
                this.app.drop_task_at_point(&drag.id, x, y);
                this.after_change(cx);
                cx.notify();
            }))
            // The axes, through the middle.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(relative(0.5))
                    .w(px(1.))
                    .bg(t.border),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(relative(0.5))
                    .h(px(1.))
                    .bg(t.border),
            )
            // Their ends' names: above the urgency axis at either end, and
            // beside the importance axis at the top and the foot.
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(relative(0.5))
                    .h(px(0.))
                    .child(label("Urgent").bottom(px(3.)).left(px(6.)))
                    .child(label("Not urgent").bottom(px(3.)).right(px(6.))),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(relative(0.5))
                    .w(px(0.))
                    .child(label("Important").top(px(4.)).left(px(6.)))
                    .child(label("Not important").bottom(px(4.)).left(px(6.))),
            )
            .children((0..4).map(corner))
            .child(
                div()
                    .absolute()
                    .top(px(MARGIN_Y))
                    .bottom(px(MARGIN_Y))
                    .left(px(MARGIN_X))
                    .right(px(MARGIN_X))
                    .child(measure_field())
                    // Ticks at the ends and the quadrants' centres.
                    .children(
                        [0., 0.25, 0.75, 1.]
                            .into_iter()
                            .flat_map(|f| [tick(f, 0.5, true), tick(0.5, f, false)]),
                    )
                    .children(
                        drawn
                            .iter()
                            .map(|drawn| self.matrix_chip(&points[drawn.ix], drawn, t, cx)),
                    )
                    .children(draft),
            );
        let unplaced = self.app.board.unplaced.clone();
        let tray = div()
            .id("unplaced")
            .h(px(120.))
            .flex_none()
            .flex()
            .flex_col()
            .rounded(px(6.))
            .border_1()
            .border_color(t.border_variant)
            .drag_over::<DraggedTask>(move |style, _, _, _| style.bg(hover))
            .on_drop(cx.listener(move |this, drag: &DraggedTask, _, cx| {
                this.app.unplace_from_matrix(&drag.id);
                this.after_change(cx);
                cx.notify();
            }))
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
                    )
                    .children(
                        self.drafting(CardPlace::Quadrant(None))
                            .then(|| self.draft_card(t)),
                    ),
            );
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .child(plane)
            .child(tray)
            .into_any_element()
    }

    /// A placed task: a dot at its point and its title beside it, ringed in
    /// the accent while selected. Click selects, a double click renames, and
    /// it drags to another point.
    fn matrix_chip(
        &self,
        point: &MatrixPoint,
        drawn: &Drawn,
        t: Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let task = &point.task;
        let id = task.id.clone();
        let selected = self.app.selected_task.as_deref() == Some(task.id.as_str());
        let active = self.app.region == Region::Tasks;
        let editing = matches!(&self.app.editing, Some(Editing::TaskTitle { task_id }) if *task_id == task.id);
        let ringed = selected && active;
        // In move mode the chip is filled with the accent's tint and its
        // title stays readable, so it is plain which one the keys carry.
        let moving = ringed && self.app.matrix_moving;
        let dot = div()
            .flex_none()
            .size(px(DOT))
            .rounded_full()
            .bg(if ringed {
                t.accent
            } else if task.status == "open" {
                t.muted
            } else {
                t.placeholder
            });
        let chip = div()
            .id(gpui::SharedString::from(format!("matrix-chip-{id}")))
            .absolute()
            .top(px(-CHIP_H / 2. + drawn.dy))
            .when(drawn.leftwards, |d| d.right(px(-DOT_INSET)))
            .when(!drawn.leftwards, |d| d.left(px(-DOT_INSET)))
            .h(px(CHIP_H))
            .max_w(px(CHIP_MAX_W))
            .when(editing, |d| d.w(px(CHIP_MAX_W)))
            .rounded(px(4.))
            .border_1()
            .border_color(if ringed { t.focus } else { t.border_variant })
            .bg(t.background)
            .text_size(px(12.))
            .whitespace_nowrap()
            .on_drag(
                drag_task_for(&id, &self.app, t),
                |drag: &DraggedTask, _, _, cx| cx.new(|_| drag.clone()),
            )
            .on_mouse_down(gpui::MouseButton::Right, {
                let id = id.clone();
                cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                    this.app.open_task_menu(
                        &id,
                        crate::ui::zoom::unzoom(e.position.x),
                        crate::ui::zoom::unzoom(e.position.y),
                    );
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
                    .size_full()
                    .px_1()
                    .rounded(px(3.))
                    .flex()
                    .when(drawn.leftwards, |d| d.flex_row_reverse())
                    .items_center()
                    .gap_1()
                    .when(selected, |d| d.bg(t.selected))
                    .when(moving, |d| d.bg(t.accent.opacity(0.22)))
                    .child(dot)
                    .child(if editing {
                        self.field_inline(t, "Task").into_any_element()
                    } else {
                        title_greyed(task, self.app.is_greyed(&task.id), t).into_any_element()
                    }),
            );
        anchor(drawn.fx, drawn.fy).child(chip).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(title: &str, urgency: i64, importance: i64) -> MatrixPoint {
        MatrixPoint {
            task: TaskRow {
                id: title.into(),
                list_id: "l".into(),
                parent_task_id: None,
                title: title.into(),
                notes: String::new(),
                status: "open".into(),
                sort_order: 0,
                due_at_ms: None,
                estimate_seconds: None,
                source_system: None,
                source_id: None,
                item_kind: None,
                is_promoted: None,
                archived_at_ms: None,
                completed_at_ms: None,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            urgency,
            importance,
        }
    }

    #[test]
    fn chips_on_one_point_are_drawn_apart() {
        let points = [
            point("first", 50, 50),
            point("second", 50, 50),
            point("third", 51, 49),
            point("far away", -80, -80),
        ];
        let drawn = lay_out(&points, (800., 480.));
        let mut ys: Vec<f32> = drawn
            .iter()
            .filter(|d| d.ix < 3)
            .map(|d| d.fy * 480. + d.dy)
            .collect();
        ys.sort_by(f32::total_cmp);
        assert!(ys.windows(2).all(|w| w[1] - w[0] >= CHIP_H), "{ys:?}");
        let far = drawn.iter().find(|d| d.ix == 3).unwrap();
        assert_eq!(far.dy, 0.);
        assert!(far.leftwards);
    }
}
