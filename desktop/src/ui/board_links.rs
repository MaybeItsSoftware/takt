//! The line the board draws between a subtask's row on its parent's card and
//! the subtask's own card in another column (`BoardLinkRoute.swift`,
//! `WorkspaceBoardLinkLines.swift`). At rest there is no line: the row names
//! the column in words. While the selection or the pointer is on either end
//! — the subtask, or the card its row is drawn on — the line is drawn in the
//! accent, out of the row sideways, down the gutter beside the parent's
//! column in a lane of its own, and into the card:
//!
//! ```text
//! row ──╮
//!       │
//!       ╰──── card
//! ```
//!
//! Each end measures itself as it is laid out (`measure`), and the overlay
//! (`overlay`), the board's last child, draws from those frames when it
//! paints, which is after every element has been laid out. Nothing here
//! moves a card.

use crate::ui::zoom::px;
use gpui::{
    Bounds, ContentMask, Hsla, IntoElement, PathBuilder, Pixels, Point, Styled, canvas, point,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// How many lines the last frame drew, for the smoke run's report.
static PAINTED: AtomicUsize = AtomicUsize::new(0);

pub fn painted() -> usize {
    PAINTED.load(Ordering::Relaxed)
}

/// Rises or falls smaller than this are drawn straight.
const STRAIGHT: f32 = 1.;
const INSET: f32 = 2.;
const RADIUS: f32 = 6.;
const LANE_SPACING: f32 = 3.;
const LANE_WIDTH: f32 = 12.;
/// The space between two columns (`gap_2`).
pub const GUTTER: f32 = 8.;
const WIDTH: f32 = 1.5;

/// Where the ends were in the frame being drawn, in window coordinates.
#[derive(Default)]
pub struct Geometry {
    /// By the subtask's id: its row on the parent's card, the parent card's
    /// column and the parent card's id.
    sources: HashMap<String, (Bounds<Pixels>, usize, String)>,
    /// By the subtask's id: its own card's heading and column.
    targets: HashMap<String, (Bounds<Pixels>, usize)>,
    columns: HashMap<usize, Bounds<Pixels>>,
    /// The part of each column its cards scroll in: a line is clipped to it.
    bands: HashMap<usize, Bounds<Pixels>>,
}

pub type Shared = Rc<RefCell<Geometry>>;

pub enum End {
    Source {
        child: String,
        column: usize,
        card: String,
    },
    Target {
        child: String,
        column: usize,
    },
    Column(usize),
    Band(usize),
}

impl Geometry {
    pub fn clear(&mut self) {
        self.sources.clear();
        self.targets.clear();
        self.columns.clear();
        self.bands.clear();
    }
}

/// An invisible child filling its parent (which must be `relative`) that
/// records the parent's frame as `end`.
pub fn measure(geometry: &Shared, end: End) -> impl IntoElement {
    let geometry = geometry.clone();
    canvas(
        move |bounds, _, _| {
            let mut g = geometry.borrow_mut();
            match end {
                End::Source {
                    child,
                    column,
                    card,
                } => {
                    g.sources.insert(child, (bounds, column, card));
                }
                End::Target { child, column } => {
                    g.targets.insert(child, (bounds, column));
                }
                End::Column(ix) => {
                    g.columns.insert(ix, bounds);
                }
                End::Band(ix) => {
                    g.bands.insert(ix, bounds);
                }
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// The lit links' lines, over the columns. `focus` is what the selection
/// and the pointer are on.
pub fn overlay(geometry: &Shared, focus: Vec<String>, color: Hsla) -> impl IntoElement {
    let geometry = geometry.clone();
    canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            PAINTED.store(0, Ordering::Relaxed);
            if focus.is_empty() {
                return;
            }
            let g = geometry.borrow();
            let lines = lines(&g, &focus);
            if lines.is_empty() {
                return;
            }
            let Some(band) = band(&g) else { return };
            window.with_content_mask(Some(ContentMask { bounds: band }), |window| {
                for line in lines {
                    if let Some(path) = stroke(&line) {
                        window.paint_path(path, color);
                        PAINTED.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

/// The band every column's cards scroll in, from the leftmost column to
/// the rightmost.
fn band(g: &Geometry) -> Option<Bounds<Pixels>> {
    let bands = g.bands.values();
    let top = bands
        .clone()
        .map(|b| f32::from(b.origin.y))
        .reduce(f32::min)?;
    let bottom = bands
        .clone()
        .map(|b| f32::from(b.origin.y + b.size.height))
        .reduce(f32::max)?;
    let left = bands
        .clone()
        .map(|b| f32::from(b.origin.x))
        .reduce(f32::min)?;
    let right = bands
        .map(|b| f32::from(b.origin.x + b.size.width))
        .reduce(f32::max)?;
    (bottom > top)
        .then(|| Bounds::from_corners(point(px(left), px(top)), point(px(right), px(bottom))))
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl Rect {
    fn of(b: Bounds<Pixels>) -> Self {
        Rect {
            min_x: f32::from(b.origin.x),
            min_y: f32::from(b.origin.y),
            max_x: f32::from(b.origin.x + b.size.width),
            max_y: f32::from(b.origin.y + b.size.height),
        }
    }

    fn mid_y(&self) -> f32 {
        (self.min_y + self.max_y) / 2.
    }
}

/// A route's corners and the radius each is rounded to.
#[derive(Debug, Clone, PartialEq)]
struct Line {
    points: Vec<(f32, f32)>,
    radii: Vec<f32>,
}

fn lines(g: &Geometry, focus: &[String]) -> Vec<Line> {
    struct Measured {
        id: String,
        source: Rect,
        target: Rect,
        column: Rect,
        source_column: usize,
        forward: bool,
    }
    let mut ends: Vec<Measured> = g
        .sources
        .iter()
        .filter(|(child, (_, _, card))| focus.iter().any(|f| f == *child || f == card))
        .filter_map(|(child, (source, source_column, _))| {
            let (target, child_column) = g.targets.get(child)?;
            let column = g.columns.get(source_column)?;
            (child_column != source_column).then(|| Measured {
                id: child.clone(),
                source: Rect::of(*source),
                target: Rect::of(*target),
                column: Rect::of(*column),
                source_column: *source_column,
                forward: child_column > source_column,
            })
        })
        .collect();
    ends.sort_by(|a, b| a.id.cmp(&b.id));
    let lanes = lanes(
        &ends
            .iter()
            .map(|e| Lane {
                id: e.id.clone(),
                source_column: e.source_column,
                forward: e.forward,
                source_y: e.source.mid_y(),
                target_y: e.target.mid_y(),
            })
            .collect::<Vec<_>>(),
        LANE_SPACING,
        LANE_WIDTH,
    );
    ends.iter()
        .map(|e| {
            let rail = GUTTER / 2. + lanes.get(&e.id).copied().unwrap_or(0.);
            let points = points(e.source, e.target, e.column, e.forward, INSET, rail);
            let radii = radii(&points, RADIUS);
            Line { points, radii }
        })
        .collect()
}

/// Out of the source sideways, along the rule beside its column, and into
/// the target at its height. Never doubling back.
fn points(
    source: Rect,
    target: Rect,
    column: Rect,
    forward: bool,
    inset: f32,
    rail: f32,
) -> Vec<(f32, f32)> {
    let sign = if forward { 1. } else { -1. };
    let start_x = if forward {
        source.max_x + inset
    } else {
        source.min_x - inset
    };
    let edge_x = if forward {
        column.max_x + rail
    } else {
        column.min_x - rail
    };
    let end_x = if forward {
        target.min_x - inset
    } else {
        target.max_x + inset
    };
    let rail_x = sign * (sign * edge_x).max(sign * start_x);
    let finish_x = sign * (sign * end_x).max(sign * rail_x);
    let start = (start_x, source.mid_y());
    if (target.mid_y() - source.mid_y()).abs() < STRAIGHT {
        return vec![start, (finish_x, source.mid_y())];
    }
    vec![
        start,
        (rail_x, source.mid_y()),
        (rail_x, target.mid_y()),
        (finish_x, target.mid_y()),
    ]
}

/// Each corner's radius, shrunk to fit: all of a leg to an end, half of
/// one shared with another corner.
fn radii(points: &[(f32, f32)], radius: f32) -> Vec<f32> {
    let mut out = vec![0.; points.len()];
    if points.len() <= 2 {
        return out;
    }
    let length =
        |a: usize, b: usize| (points[b].0 - points[a].0).abs() + (points[b].1 - points[a].1).abs();
    let last = points.len() - 1;
    for (ix, slot) in out.iter_mut().enumerate().take(last).skip(1) {
        let before = length(ix - 1, ix) / if ix - 1 == 0 { 1. } else { 2. };
        let after = length(ix, ix + 1) / if ix + 1 == last { 1. } else { 2. };
        *slot = radius.min(before).min(after).max(0.);
    }
    out
}

struct Lane {
    id: String,
    source_column: usize,
    forward: bool,
    source_y: f32,
    target_y: f32,
}

/// How far each link's turn sits from the middle of its gutter, so links
/// turning in one gutter each take a lane: falling links outermost, the
/// one leaving higher further out; rising ones inside, the one leaving
/// lower further out. A link missing from the answer turns on the middle.
fn lanes(requests: &[Lane], spacing: f32, width: f32) -> HashMap<String, f32> {
    let mut groups: HashMap<(usize, bool), Vec<&Lane>> = HashMap::new();
    for r in requests {
        if (r.target_y - r.source_y).abs() >= STRAIGHT {
            groups
                .entry((r.source_column, r.forward))
                .or_default()
                .push(r);
        }
    }
    let mut out = HashMap::new();
    for group in groups.values().filter(|g| g.len() > 1) {
        let mut falling: Vec<&&Lane> = group.iter().filter(|r| r.target_y > r.source_y).collect();
        falling.sort_by(|a, b| a.source_y.total_cmp(&b.source_y).then(a.id.cmp(&b.id)));
        let mut rising: Vec<&&Lane> = group.iter().filter(|r| r.target_y < r.source_y).collect();
        rising.sort_by(|a, b| b.source_y.total_cmp(&a.source_y).then(a.id.cmp(&b.id)));
        let ordered: Vec<&&Lane> = falling.into_iter().chain(rising).collect();
        let gap = spacing.min(width.max(0.) / (ordered.len() - 1) as f32);
        let middle = (ordered.len() - 1) as f32 / 2.;
        for (slot, r) in ordered.iter().enumerate() {
            out.insert(r.id.clone(), (middle - slot as f32) * gap);
        }
    }
    out
}

/// The route as a stroked path, its corners rounded with a curve on each.
fn stroke(line: &Line) -> Option<gpui::Path<Pixels>> {
    let p = |(x, y): (f32, f32)| -> Point<Pixels> { point(px(x), px(y)) };
    let unit = |from: (f32, f32), to: (f32, f32)| {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let len = dx.abs() + dy.abs();
        if len == 0. {
            (0., 0.)
        } else {
            (dx / len, dy / len)
        }
    };
    let points = &line.points;
    let mut builder = PathBuilder::stroke(px(WIDTH));
    builder.move_to(p(*points.first()?));
    for ix in 1..points.len().saturating_sub(1) {
        let corner = points[ix];
        let r = line.radii[ix];
        let before = unit(points[ix - 1], corner);
        let after = unit(corner, points[ix + 1]);
        builder.line_to(p((corner.0 - before.0 * r, corner.1 - before.1 * r)));
        builder.curve_to(
            p((corner.0 + after.0 * r, corner.1 + after.1 * r)),
            p(corner),
        );
    }
    builder.line_to(p(*points.last()?));
    builder.build().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Rect {
        Rect {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    #[test]
    fn a_link_turns_beside_its_column_and_runs_into_the_card() {
        let column = rect(0., 0., 272., 600.);
        let source = rect(10., 100., 260., 120.);
        let target = rect(290., 300., 550., 320.);
        let points = points(source, target, column, true, 2., 4.);
        assert_eq!(
            points,
            vec![(262., 110.), (276., 110.), (276., 310.), (288., 310.)]
        );
        assert_eq!(radii(&points, 6.), vec![0., 6., 6., 0.]);
    }

    #[test]
    fn a_level_link_runs_straight_and_one_pointing_back_mirrors() {
        let column = rect(280., 0., 552., 600.);
        let source = rect(290., 100., 540., 120.);
        let straight = points(source, rect(10., 100.5, 260., 120.5), column, false, 2., 4.);
        assert_eq!(straight, vec![(288., 110.), (262., 110.)]);
        let back = points(source, rect(10., 200., 260., 220.), column, false, 2., 4.);
        assert_eq!(back[1], (276., 110.));
        assert_eq!(back[3], (262., 210.));
    }

    #[test]
    fn corners_shrink_to_fit_a_short_leg() {
        let points = vec![(0., 0.), (4., 0.), (4., 3.), (10., 3.)];
        assert_eq!(radii(&points, 6.), vec![0., 1.5, 1.5, 0.]);
    }

    #[test]
    fn links_turning_in_one_gutter_take_lanes_of_their_own() {
        let lane = |id: &str, source_y: f32, target_y: f32| Lane {
            id: id.into(),
            source_column: 0,
            forward: true,
            source_y,
            target_y,
        };
        let lanes = lanes(
            &[
                lane("a", 10., 100.),
                lane("b", 20., 200.),
                lane("c", 300., 50.),
                lane("flat", 40., 40.),
            ],
            3.,
            12.,
        );
        assert_eq!(lanes["a"], 3.);
        assert_eq!(lanes["b"], 0.);
        assert_eq!(lanes["c"], -3.);
        assert!(!lanes.contains_key("flat"));
    }
}
