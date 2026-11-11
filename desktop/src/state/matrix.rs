//! The matrix as a plane: each task at a point on two axes, urgency across
//! (urgent on the left) and importance down (important at the top).
//!
//! The point is stored in `task_metadata.matrixUrgency` and
//! `matrixImportance`, which every other client reads only by sign (the
//! core's `matrix_quadrant`: above zero is urgent, or important). The Mac
//! keeps a precise scale of -100..=100, a point on an axis included, and
//! stores it moved [`EDGE`] away from zero ([`stored`]): 0 as -10, 1 as 11.
//! So a stored magnitude of [`EDGE`] or more is a precise point, and its
//! sign is still the side the core reads it on (a point on the line counts
//! as not urgent, or not important, as the core's 0 does). Anything smaller
//! was written by quadrant — the old Mac's 1 and 0, the CLI's -9..=9 — and
//! is read as a point inside its quadrant ([`precise`]), spread apart where
//! several land on one ([`arrange`]).

use super::store::now_ms;
use super::{App, Region, ViewMode};

/// The scale's ends.
pub const SCALE: i64 = 100;
/// How far from zero a precise point is stored, so it is never taken for
/// one written by quadrant.
pub const EDGE: i64 = 10;
/// A nudge: `h`/`j`/`k`/`l` and the arrows.
pub const STEP: i64 = 10;
/// A fine nudge: the same with Shift.
pub const FINE: i64 = 2;
/// A quadrant's centre, where `Option+1`–`Option+4` place a task.
pub const CENTRE: i64 = 50;

/// A stored coordinate as a point on the scale. A legacy value (a
/// magnitude of 9 or less) is read by quadrant: 1–9 from the centre of the
/// positive half outwards, 0 at the centre of the negative half, -1–-9 from
/// there outwards. So the old Mac's 1 and 0 sit at their quadrant's centre.
pub fn precise(stored: i64) -> i64 {
    if stored >= EDGE {
        return (stored - EDGE).min(SCALE);
    }
    if stored <= -EDGE {
        return (stored + EDGE).max(-SCALE);
    }
    if stored > 0 {
        CENTRE + (stored - 1) * 6
    } else if stored == 0 {
        -CENTRE
    } else {
        -CENTRE + (stored + 1) * 6
    }
}

/// Whether a stored coordinate was written by quadrant rather than at a
/// point.
pub fn is_legacy(stored: i64) -> bool {
    stored.abs() < EDGE
}

/// A point on the scale as it is stored: [`EDGE`] further from zero, on
/// the side the core reads it on; on the line (0) is the negative side.
pub fn stored(value: i64) -> i64 {
    let value = value.clamp(-SCALE, SCALE);
    if value > 0 {
        value + EDGE
    } else {
        value - EDGE
    }
}

/// A coordinate to write: on the scale. On an axis is allowed.
pub fn snap(value: i64) -> i64 {
    value.clamp(-SCALE, SCALE)
}

/// `value` moved `by`, kept on the scale; it may stop on the axis.
pub fn nudge(value: i64, by: i64) -> i64 {
    (value + by).clamp(-SCALE, SCALE)
}

/// A point on the plane, `0.0..=1.0` from the left and from the top, as
/// stored coordinates: the left edge is most urgent, the top most important.
pub fn from_fraction(x: f32, y: f32) -> (i64, i64) {
    let axis = |f: f32| snap((SCALE as f32 - 2. * SCALE as f32 * f.clamp(0., 1.)).round() as i64);
    (axis(x), axis(y))
}

/// Where a point is drawn, `0.0..=1.0` from the left and from the top.
pub fn to_fraction(urgency: i64, importance: i64) -> (f32, f32) {
    let axis = |v: i64| (SCALE - v.clamp(-SCALE, SCALE)) as f32 / (2 * SCALE) as f32;
    (axis(urgency), axis(importance))
}

/// The quadrant a point is in, as the core reads it: 0 do now, 1 schedule,
/// 2 delegate, 3 eliminate.
fn motion_is_up(motion: &str) -> bool {
    matches!(motion, "k" | "up")
}

pub fn quadrant_of(urgency: i64, importance: i64) -> usize {
    takt_core::board_rules::matrix_quadrant(Some(urgency as i32), Some(importance as i32))
        .unwrap_or(3) as usize
}

/// A quadrant's centre.
pub fn centre_of(quadrant: usize) -> (i64, i64) {
    match quadrant {
        0 => (CENTRE, CENTRE),
        1 => (-CENTRE, CENTRE),
        2 => (CENTRE, -CENTRE),
        _ => (-CENTRE, -CENTRE),
    }
}

/// `value` kept in `side`'s quadrant half: above the axis, or on or below
/// it, as the core counts them.
fn keep_side(value: i64, side: i64) -> i64 {
    if side > 0 {
        value.clamp(1, SCALE)
    } else {
        value.clamp(-SCALE, 0)
    }
}

/// Every placed task's point, from its stored coordinates and its old
/// in-quadrant rank, in the order given. Precise points are as stored;
/// legacy ones that land on the same point are spread on a small grid
/// around it, by rank and then by order, so none hide another. Each stays
/// in its quadrant.
pub fn arrange(stored: &[(i64, i64, Option<i64>)]) -> Vec<(i64, i64)> {
    let mut points: Vec<(i64, i64)> = stored
        .iter()
        .map(|(u, i, _)| (precise(*u), precise(*i)))
        .collect();
    let mut groups: std::collections::BTreeMap<(i64, i64), Vec<usize>> = Default::default();
    for (ix, (u, i, _)) in stored.iter().enumerate() {
        if is_legacy(*u) || is_legacy(*i) {
            groups.entry(points[ix]).or_default().push(ix);
        }
    }
    for ((u, i), mut members) in groups {
        if members.len() < 2 {
            continue;
        }
        members.sort_by_key(|ix| (stored[*ix].2.unwrap_or(i64::MAX), *ix));
        let side = (members.len() as f64).sqrt().ceil() as i64;
        let spacing = (80 / side).clamp(2, 8);
        let start = -(side - 1) * spacing / 2;
        for (k, ix) in members.into_iter().enumerate() {
            let (col, row) = (k as i64 % side, k as i64 / side);
            // Reading order: the first across the top, from the urgent end.
            points[ix] = (
                keep_side(u - (start + col * spacing), u),
                keep_side(i - (start + row * spacing), i),
            );
        }
    }
    points
}

/// A free point near `want`, in its quadrant: `want` itself unless another
/// point is within a few units of it, else the nearest free one on rings of
/// [`STEP`] around it.
pub fn free_point(want: (i64, i64), taken: &[(i64, i64)]) -> (i64, i64) {
    let clear = |p: (i64, i64)| {
        taken
            .iter()
            .all(|q| (q.0 - p.0).abs() >= 6 || (q.1 - p.1).abs() >= 6)
    };
    if clear(want) {
        return want;
    }
    for ring in 1..=4i64 {
        for dy in -ring..=ring {
            for dx in -ring..=ring {
                if dx.abs() != ring && dy.abs() != ring {
                    continue;
                }
                let p = (
                    keep_side(want.0 - dx * STEP, want.0),
                    keep_side(want.1 - dy * STEP, want.1),
                );
                if clear(p) {
                    return p;
                }
            }
        }
    }
    want
}

impl App {
    /// The selected task's point on the plane, when it has one.
    pub fn selected_matrix_point(&self) -> Option<(i64, i64)> {
        let id = self.selected_task.as_deref()?;
        self.board
            .points
            .iter()
            .find(|point| point.task.id == id)
            .map(|point| (point.urgency, point.importance))
    }

    /// Places a task at a point, snapped to what may be written: one undo
    /// step through the core, patched into what is held.
    pub fn set_matrix_point(&mut self, id: &str, urgency: i64, importance: i64) {
        let (urgency, importance) = (snap(urgency), snap(importance));
        let task = id.to_string();
        if self
            .write(|store| {
                store.core.set_matrix_position(
                    task.clone(),
                    Some(stored(urgency)),
                    Some(stored(importance)),
                    now_ms(),
                )
            })
            .is_some()
        {
            let placement = self.placements.entry(task.clone()).or_default();
            placement.urgency = Some(stored(urgency));
            placement.importance = Some(stored(importance));
            placement.rank = None;
            self.selected_task = Some(task);
            self.layout_board();
        }
    }

    /// Takes a task off the plane, back to the unplaced ones.
    pub fn unplace_from_matrix(&mut self, id: &str) {
        let task = id.to_string();
        if self
            .write(|store| {
                store
                    .core
                    .set_matrix_position(task.clone(), None, None, now_ms())
            })
            .is_some()
        {
            let placement = self.placements.entry(task.clone()).or_default();
            placement.urgency = None;
            placement.importance = None;
            placement.rank = None;
            self.selected_task = Some(task);
            self.layout_board();
        }
    }

    /// A drop on the plane at a fraction across and down it.
    pub fn drop_task_at_point(&mut self, dragged: &str, x: f32, y: f32) {
        let (urgency, importance) = from_fraction(x, y);
        self.set_matrix_point(dragged, urgency, importance);
    }

    /// `Option+1`–`Option+4`: the selected task at a quadrant's centre, or
    /// beside it when another task is there.
    pub fn place_in_quadrant(&mut self, quadrant: usize) {
        let Some(id) = self.selected_task.clone() else {
            return;
        };
        let taken: Vec<(i64, i64)> = self
            .board
            .points
            .iter()
            .filter(|point| point.task.id != id)
            .map(|point| (point.urgency, point.importance))
            .collect();
        let (urgency, importance) = free_point(centre_of(quadrant), &taken);
        self.set_matrix_point(&id, urgency, importance);
    }

    /// The matrix's own keys, ahead of the catalogue: on a placed task the
    /// motions nudge it rather than move the selection, and Tab steps the
    /// selection instead. False for anything else, which the catalogue
    /// then runs as usual.
    pub fn matrix_key(&mut self, key: &str) -> bool {
        if self.view_mode != ViewMode::Matrix || self.region != Region::Tasks {
            self.matrix_moving = false;
            return false;
        }
        match key {
            "tab" => {
                self.matrix_moving = false;
                self.step_matrix_selection(1);
                return true;
            }
            "shift+tab" => {
                self.matrix_moving = false;
                self.step_matrix_selection(-1);
                return true;
            }
            // Move mode: on a placed task, and off again.
            "v" => {
                if self.matrix_moving {
                    self.stop_matrix_move();
                } else if self.selected_matrix_point().is_some() {
                    self.matrix_moving = true;
                    self.message = Some(
                        "Moving: the arrows or h j k l nudge it (Shift for small steps); v, Return or Esc when it is there"
                            .into(),
                    );
                }
                return true;
            }
            "enter" | "escape" if self.matrix_moving => {
                self.stop_matrix_move();
                return true;
            }
            _ => {}
        }
        let (fine, motion) = match key.strip_prefix("shift+") {
            Some(rest) => (true, rest),
            None => (false, key),
        };
        let (du, di) = match motion {
            "h" | "left" => (1, 0),
            "l" | "right" => (-1, 0),
            "k" | "up" => (0, 1),
            "j" | "down" => (0, -1),
            _ => return false,
        };
        let Some((urgency, importance)) = self.selected_matrix_point() else {
            // In the tray: up comes back onto the plane, the rest move
            // along the tray as everywhere else.
            if !fine && motion_is_up(motion) {
                return self.select_nearest_point(None, (0, 1));
            }
            return false;
        };
        if !self.matrix_moving {
            if fine {
                return false;
            }
            if self.select_nearest_point(Some((urgency, importance)), (du, di)) {
                return true;
            }
            // Below the lowest: into the tray.
            if di < 0
                && let Some(first) = self.board.unplaced.first()
            {
                self.selected_task = Some(first.id.clone());
                return true;
            }
            // Nothing that way: left and right fall to what they do
            // anywhere else.
            return di != 0;
        }
        let Some(id) = self.selected_task.clone() else {
            return false;
        };
        let step = if fine { FINE } else { STEP };
        let urgency = if du == 0 {
            urgency
        } else {
            nudge(urgency, du * step)
        };
        let importance = if di == 0 {
            importance
        } else {
            nudge(importance, di * step)
        };
        self.set_matrix_point(&id, urgency, importance);
        true
    }

    fn stop_matrix_move(&mut self) {
        self.matrix_moving = false;
        self.message = None;
    }

    /// The nearest placed task from `from` in direction `(du, di)` (urgency
    /// up is left, importance up is up): along that way first, then off to
    /// the side, counted double. From the tray (`None`), the lowest.
    fn select_nearest_point(&mut self, from: Option<(i64, i64)>, (du, di): (i64, i64)) -> bool {
        let selected = self.selected_task.clone();
        let best = self
            .board
            .points
            .iter()
            .filter(|p| Some(&p.task.id) != selected.as_ref())
            .filter_map(|p| {
                let Some((u, i)) = from else {
                    return Some((p.importance, p.task.id.clone()));
                };
                let (along, aside) = if du != 0 {
                    ((p.urgency - u) * du, (p.importance - i).abs())
                } else {
                    ((p.importance - i) * di, (p.urgency - u).abs())
                };
                (along > 0).then(|| (along + 2 * aside, p.task.id.clone()))
            })
            .min_by_key(|(score, _)| *score);
        match best {
            Some((_, id)) => {
                self.selected_task = Some(id);
                true
            }
            None => false,
        }
    }

    /// Tab / Shift+Tab: the next or previous task in the matrix's order,
    /// the plane's quadrant by quadrant and then the unplaced, wrapping.
    pub fn step_matrix_selection(&mut self, by: i64) {
        let ids = self.board_ids();
        if ids.is_empty() {
            return;
        }
        let count = ids.len() as i64;
        let next = match self
            .selected_task
            .as_ref()
            .and_then(|id| ids.iter().position(|other| other == id))
        {
            Some(at) => (at as i64 + by).rem_euclid(count),
            None if by > 0 => 0,
            None => count - 1,
        };
        self.selected_task = Some(ids[next as usize].clone());
    }

    /// The smoke run's matrix checks: the legacy reading, and with `write`
    /// a task dropped at a point, nudged, and read back from the database,
    /// then put back where it was.
    pub fn matrix_smoke(&mut self, write: bool) -> Vec<String> {
        let mut said = Vec::new();
        let legacy = precise(1) == CENTRE
            && precise(0) == -CENTRE
            && precise(9) == 98
            && precise(-9) == -98
            && (-9..=9).all(|v| quadrant_of(precise(v), CENTRE) == quadrant_of(v, 1));
        let (placed, unplaced) = (self.board.points.len(), self.board.unplaced.len());
        let legacy_points = self
            .board
            .points
            .iter()
            .filter(|point| {
                let p = self.placements.get(&point.task.id);
                p.and_then(|p| p.urgency).is_some_and(is_legacy)
                    || p.and_then(|p| p.importance).is_some_and(is_legacy)
            })
            .count();
        let mut seen = std::collections::HashSet::new();
        let stacked = self
            .board
            .points
            .iter()
            .filter(|point| !seen.insert((point.urgency, point.importance)))
            .count();
        let in_quadrant = self.board.points.iter().all(|point| {
            let p = self
                .placements
                .get(&point.task.id)
                .cloned()
                .unwrap_or_default();
            takt_core::board_rules::matrix_quadrant(
                p.urgency.map(|v| v as i32),
                p.importance.map(|v| v as i32),
            ) == Some(quadrant_of(point.urgency, point.importance) as u32)
        });
        said.push(format!(
            "matrix plane {placed} placed ({legacy_points} legacy, {stacked} on another's point), {unplaced} not placed; legacy mapping {legacy}, every point in its stored quadrant {in_quadrant}"
        ));
        if !write {
            return said;
        }
        let Some(id) = self
            .board
            .points
            .first()
            .map(|p| p.task.id.clone())
            .or_else(|| self.board.unplaced.first().map(|t| t.id.clone()))
        else {
            return said;
        };
        let before = self.placements.get(&id).cloned().unwrap_or_default();
        self.region = Region::Tasks;
        self.selected_task = Some(id.clone());
        // 31.5% across, 81.5% down: urgency 37, importance -63.
        self.drop_task_at_point(&id, 0.315, 0.815);
        let dropped = self.selected_matrix_point();
        // Move mode carries the task; out of it the same keys move the
        // selection.
        self.matrix_key("v");
        let moving = self.matrix_moving;
        self.matrix_key("l");
        self.matrix_key("shift+k");
        self.matrix_key("enter");
        let held = self.selected_matrix_point();
        let mut navigated = None;
        let other = self
            .board
            .points
            .iter()
            .map(|p| p.task.id.clone())
            .chain(self.board.unplaced.iter().map(|t| t.id.clone()))
            .find(|other| *other != id);
        let other_before = other
            .as_ref()
            .map(|o| self.placements.get(o).cloned().unwrap_or_default());
        if let Some(other) = &other {
            // 70% across, 20% down: less urgent, more important.
            self.drop_task_at_point(other, 0.7, 0.2);
            self.selected_task = Some(id.clone());
            let here = self.selected_matrix_point();
            self.matrix_key("l");
            navigated = Some(
                self.selected_task.as_deref() == Some(other.as_str())
                    && self.selected_task.as_deref() != Some(id.as_str())
                    && {
                        self.selected_task = Some(id.clone());
                        self.selected_matrix_point() == here
                    },
            );
        }
        self.read_scope_into_rows();
        self.layout_board();
        let stored = self.placements.get(&id).map(|p| (p.urgency, p.importance));
        let read_back = stored == Some((Some(37), Some(-71)))
            && self.selected_matrix_point() == Some((27, -61));
        let quadrant = self
            .board
            .quadrants
            .iter()
            .position(|tasks| tasks.iter().any(|t| t.id == id));
        // The centre of the plane is on both axes, and stays there.
        self.drop_task_at_point(&id, 0.5, 0.5);
        let snapped = self.selected_matrix_point() == Some((0, 0));
        said.push(format!(
            "matrix point: dropped at {dropped:?}, move mode {moving}, nudged to {held:?}, read back {read_back} in quadrant {quadrant:?}; centre sits on the axes {snapped}; l selects the task to the right {navigated:?}"
        ));
        if let (Some(other), Some(before)) = (other, other_before) {
            let (u, i) = (before.urgency, before.importance);
            if self
                .write(|store| {
                    store
                        .core
                        .set_matrix_position(other.clone(), u, i, now_ms())
                })
                .is_some()
            {
                self.placements.insert(other, before);
            }
        }
        let (u, i) = (before.urgency, before.importance);
        let restored =
            self.write(|store| store.core.set_matrix_position(id.clone(), u, i, now_ms()));
        if restored.is_some() {
            self.placements.insert(id, before);
            self.layout_board();
        }
        said
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_values_land_inside_their_quadrant() {
        assert_eq!(precise(1), 50);
        assert_eq!(precise(0), -50);
        assert_eq!(precise(9), 98);
        assert_eq!(precise(-1), -50);
        assert_eq!(precise(-9), -98);
        assert_eq!(precise(5), 74);
        for v in -9..=9 {
            let p = precise(v);
            assert!(p != 0 && p.abs() <= SCALE, "{v} -> {p}");
            assert_eq!(p > 0, v > 0, "{v} keeps its side");
        }
    }

    #[test]
    fn precise_points_survive_being_stored() {
        for v in -100..=100 {
            let s = stored(v);
            assert!(!is_legacy(s), "{v} is stored as a precise value");
            assert_eq!(precise(s), v);
            // The core reads the side by sign: the line is the negative side.
            assert_eq!(s > 0, v > 0);
        }
        assert_eq!(stored(0), -10);
        assert_eq!(stored(1), 11);
        assert_eq!(precise(250), 100);
        assert_eq!(precise(-250), -100);
    }

    #[test]
    fn a_point_may_sit_on_an_axis() {
        assert_eq!(snap(0), 0);
        assert_eq!(snap(140), 100);
        assert_eq!(snap(-140), -100);
        assert_eq!(nudge(10, -10), 0);
        assert_eq!(nudge(0, 2), 2);
        assert_eq!(nudge(95, 10), 100);
        assert_eq!(nudge(-95, -10), -100);
        assert_eq!(from_fraction(0.5, 0.5), (0, 0));
    }

    #[test]
    fn quadrants_match_the_cores_reading() {
        for u in -100..=100 {
            for i in [-100, -1, 0, 1, 100] {
                // What the phones and the CLI read from what is stored.
                let core = takt_core::board_rules::matrix_quadrant(
                    Some(stored(u) as i32),
                    Some(stored(i) as i32),
                );
                assert_eq!(core, Some(quadrant_of(u, i) as u32));
            }
        }
        // The orientation: urgent left, important top.
        assert_eq!(quadrant_of(centre_of(0).0, centre_of(0).1), 0);
        assert_eq!(quadrant_of(centre_of(1).0, centre_of(1).1), 1);
        assert_eq!(quadrant_of(centre_of(2).0, centre_of(2).1), 2);
        assert_eq!(quadrant_of(centre_of(3).0, centre_of(3).1), 3);
        assert_eq!(from_fraction(0.25, 0.25), (50, 50));
        assert_eq!(from_fraction(0.75, 0.25), (-50, 50));
        assert_eq!(from_fraction(0.25, 0.75), (50, -50));
        assert_eq!(from_fraction(0.0, 1.0), (100, -100));
        assert_eq!(from_fraction(0.5, 0.5), (0, 0));
        assert_eq!(to_fraction(50, 50), (0.25, 0.25));
        assert_eq!(to_fraction(-100, -100), (1.0, 1.0));
    }

    #[test]
    fn legacy_points_on_one_spot_are_spread_within_their_quadrant() {
        // The old Mac's (1, 1) four times, ranked out of order, and one
        // precise point that happens to be at the same place.
        let stored = [
            (1, 1, Some(2)),
            (1, 1, None),
            (1, 1, Some(0)),
            (1, 1, Some(1)),
            (stored(50), stored(50), None),
            (0, 0, None),
        ];
        let points = arrange(&stored);
        let legacy: std::collections::HashSet<_> = points[..4].iter().collect();
        assert_eq!(legacy.len(), 4, "no two legacy points stack: {points:?}");
        for p in &points[..4] {
            assert_eq!(quadrant_of(p.0, p.1), 0);
            assert!((p.0 - 50).abs() <= 10 && (p.1 - 50).abs() <= 10);
        }
        // Rank 0 first: the top-left of the grid.
        assert!(points[2].1 >= points[0].1 && points[2].0 >= points[3].0);
        assert_eq!(points[4], (50, 50));
        assert_eq!(points[5], (-50, -50));
        // The same input, the same layout.
        assert_eq!(arrange(&stored), points);
    }

    #[test]
    fn a_crowded_legacy_spot_still_keeps_its_side() {
        let stored: Vec<_> = (0..200).map(|n| (9, -9, Some(n))).collect();
        for (u, i) in arrange(&stored) {
            assert!((1..=SCALE).contains(&u));
            assert!((-SCALE..=0).contains(&i));
        }
    }

    #[test]
    fn a_quadrant_placement_steps_aside_from_one_taken() {
        assert_eq!(free_point((50, 50), &[]), (50, 50));
        let next = free_point((50, 50), &[(50, 50)]);
        assert_ne!(next, (50, 50));
        assert_eq!(quadrant_of(next.0, next.1), 0);
        let third = free_point((-50, -50), &[(-50, -50), (-40, -40), (-60, -60)]);
        assert_eq!(quadrant_of(third.0, third.1), 3);
        assert!(![(-50, -50), (-40, -40), (-60, -60)].contains(&third));
    }
}
