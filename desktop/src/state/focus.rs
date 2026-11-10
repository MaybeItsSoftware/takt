//! Focus sessions: starting one on a task, the running clock, finishing the
//! block (the task ticked off and the time logged) or stopping (the time so
//! far logged). `WorkspaceViewModel+Focus.swift`, through the core's focus
//! calls; the core keeps the clock, this reads it.
//!
//! Two ways in: `gf` starts a plain timer that counts up, and `gp` a
//! pomodoro, a block of the suggested length with a break offered after.

use super::store::now_ms;
use super::{App, Editing};
use takt_core::focus::{FocusContext, Unavailable};
use takt_core::rows::SessionRow;
use takt_core::workspace::CoreWorkspace;

/// The Swift app's defaults: 25-minute blocks, 5-minute breaks.
const WORK_SECONDS: i64 = 25 * 60;
const BREAK_SECONDS: i64 = 5 * 60;

/// How a block was started: `gf`, a plain timer counting up; `gp`, a
/// pomodoro counting down to a break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusKind {
    Timer,
    Pomodoro,
}

/// A row of the focus panel's finder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinderHit {
    Task {
        id: String,
        title: String,
        list: String,
    },
    List {
        id: String,
        name: String,
    },
}

/// A timer, counting up with no length, rather than a pomodoro.
pub fn is_timer(session: &SessionRow) -> bool {
    session.open_ended
}

/// The clock as the pill and the status item read it: up for a timer,
/// down for a pomodoro (`+` once it runs over), down for a break. The
/// bool is true once a pomodoro or break has run over.
pub fn reading(session: &SessionRow, now: i64) -> (String, bool) {
    if session.phase == "onBreak" {
        let left = session
            .break_ends_at_ms
            .map(|end| (end - now) / 1000)
            .unwrap_or(0);
        return (clock(left.max(0)), left < 0);
    }
    let elapsed = elapsed_seconds(session, now);
    if is_timer(session) {
        return (clock(elapsed), false);
    }
    let left = session.work_duration_seconds.max(1) - elapsed;
    if left >= 0 {
        (clock(left), false)
    } else {
        (format!("+{}", clock(-left)), true)
    }
}

/// What holds now (the conditions on, the time available), as last saved.
pub fn focus_context(core: &CoreWorkspace, workspace_id: &str) -> FocusContext {
    let now = now_ms();
    let stored = core
        .focus_context(workspace_id.into())
        .ok()
        .flatten()
        .map(|stored| takt_core::focus_context::focus_context_in_force(stored, now))
        .unwrap_or_default();
    FocusContext {
        condition_ids: stored.condition_ids,
        ends_at_ms: stored.available_until_ms,
        mode: stored.mode,
    }
}

/// Seconds worked in the block in hand. `FocusSession.elapsedSeconds`.
pub fn elapsed_seconds(session: &SessionRow, now: i64) -> i64 {
    let running = if session.paused_at_ms.is_none() {
        ((now - session.active_task_started_at_ms) / 1000).max(0)
    } else {
        0
    };
    session.accumulated_seconds.unwrap_or(0).max(0) + running
}

impl App {
    pub fn reload_focus(&mut self) {
        self.focus_session = self
            .store
            .core
            .active_focus_session()
            .ok()
            .flatten()
            .filter(|session| session.phase != "finished");
    }

    pub fn focus_running(&self) -> bool {
        self.focus_session.is_some()
    }

    /// `gf` on a task: a plain timer on it, counting up with no planned
    /// length and no break at the end. A task ruled out now (a condition,
    /// a start in the future) asks first; Return starts it anyway.
    pub fn start_focus_on_selected(&mut self, override_availability: bool) {
        self.start_focus(FocusKind::Timer, override_availability);
    }

    /// `gp` on a task: a pomodoro, a block as long as the core suggests for
    /// it (25 minutes by default) with a 5-minute break offered at its end.
    pub fn start_pomodoro_on_selected(&mut self, override_availability: bool) {
        self.start_focus(FocusKind::Pomodoro, override_availability);
    }

    fn start_focus(&mut self, kind: FocusKind, override_availability: bool) {
        let Some(task) = self.focus_target() else {
            return;
        };
        if task.item_kind.as_deref() == Some("list") {
            self.message = Some("A list can't be focused on; enter it and pick a task.".into());
            return;
        }
        let now = now_ms();
        let zone = self.store.zone.clone();
        let context = focus_context(&self.store.core, &self.store.workspace_id);
        let candidate = self
            .store
            .core
            .next_up_candidates(now, zone.clone())
            .ok()
            .and_then(|candidates| candidates.into_iter().find(|c| c.id == task.id));
        // A timer has no length, so only what rules the task out itself
        // (a condition, a start later) holds it back, not the time left.
        let available =
            match (kind, &candidate) {
                (FocusKind::Timer, Some(c)) => takt_core::focus::reasons(c, &context, now)
                    .iter()
                    .all(|reason| {
                        matches!(
                            reason,
                            Unavailable::InsufficientTime { .. } | Unavailable::NeedsEstimate
                        )
                    }),
                (FocusKind::Pomodoro, Some(_)) => true,
                (_, None) => false,
            };
        if !available && !override_availability {
            self.pending_focus = Some((task.id.clone(), kind));
            self.message = Some(format!(
                "\u{201c}{}\u{201d} isn't available now. Return starts it anyway.",
                task.title
            ));
            return;
        }
        let planned = candidate
            .as_ref()
            .map(|c| takt_core::focus::suggested_seconds(c, &context, now))
            .unwrap_or(WORK_SECONDS);
        let started = self.write(|store| match kind {
            FocusKind::Timer => store.core.start_focus_timer(
                task.id.clone(),
                BREAK_SECONDS,
                Some(context),
                !available,
                now,
                zone,
            ),
            FocusKind::Pomodoro => store.core.start_focus_session(
                task.id.clone(),
                Some(planned),
                WORK_SECONDS,
                BREAK_SECONDS,
                Some(context),
                !available,
                now,
                zone,
            ),
        });
        if started.is_some() {
            self.reload_focus();
            self.reload_today();
            self.message = Some(match kind {
                FocusKind::Timer => format!("Timing \u{201c}{}\u{201d}.", task.title),
                FocusKind::Pomodoro => format!(
                    "Pomodoro on \u{201c}{}\u{201d} for {} min.",
                    task.title,
                    planned / 60
                ),
            });
        }
    }

    /// Adds the selection to the running session's queue, to come after
    /// the block in hand: timed when it takes over, or as a pomodoro.
    pub fn add_selected_to_focus_queue(&mut self, kind: FocusKind) {
        let (Some(session), Some(task)) = (self.focus_session.clone(), self.focus_target()) else {
            return;
        };
        if self
            .write(|store| {
                if kind == FocusKind::Timer {
                    store
                        .core
                        .add_timer_to_focus_queue(session.id, task.id.clone(), now_ms())
                } else {
                    store
                        .core
                        .add_to_focus_queue(session.id, task.id.clone(), None, now_ms())
                }
            })
            .is_some()
        {
            self.message = Some(format!("Queued \u{201c}{}\u{201d}.", task.title));
        }
    }

    /// The selected task, read from the core when it isn't among the rows
    /// on screen: the focus panel picks from every list.
    fn focus_target(&self) -> Option<takt_core::records::TaskRow> {
        self.selected().cloned().or_else(|| {
            let id = self.selected_task.clone()?;
            self.store.core.task(id).ok().flatten()
        })
    }

    /// Starts (or, behind a running block, queues) a task picked in the
    /// focus panel. The main window's selection moves to it only when it is
    /// on screen there.
    pub fn focus_from_panel(&mut self, task_id: &str, kind: FocusKind) {
        let before = self.selected_task.replace(task_id.to_string());
        if self.focus_running() {
            self.add_selected_to_focus_queue(kind);
        } else {
            self.start_focus(kind, true);
        }
        if self.task(task_id).is_none() {
            self.selected_task = before;
        }
    }

    /// The focus panel's finder: with a list chosen, its open tasks whose
    /// titles match; otherwise the lists whose names match (every list
    /// when nothing is typed) and then the open tasks that match anywhere.
    pub fn focus_finder(&self, query: &str, list_id: Option<&str>) -> Vec<FinderHit> {
        const LIMIT: usize = 60;
        let needle = query.trim().to_lowercase();
        let task_hit = |task: takt_core::records::TaskRow| {
            let list = self
                .list(&task.list_id)
                .map(|l| l.name.clone())
                .unwrap_or_default();
            FinderHit::Task {
                id: task.id,
                title: task.title,
                list,
            }
        };
        let workable = |task: &takt_core::records::TaskRow| {
            task.status == "open" && task.item_kind.as_deref() != Some("list")
        };
        if let Some(list_id) = list_id {
            return self
                .store
                .core
                .tasks_in_lists(vec![list_id.to_string()])
                .unwrap_or_default()
                .into_iter()
                .filter(workable)
                .filter(|t| needle.is_empty() || t.title.to_lowercase().contains(&needle))
                .take(LIMIT)
                .map(task_hit)
                .collect();
        }
        let mut hits: Vec<FinderHit> = self
            .lists
            .iter()
            .filter(|l| !l.is_archived && l.completed_at_ms.is_none())
            .filter(|l| needle.is_empty() || l.name.to_lowercase().contains(&needle))
            .take(if needle.is_empty() { LIMIT } else { 6 })
            .map(|l| FinderHit::List {
                id: l.id.clone(),
                name: l.name.clone(),
            })
            .collect();
        if !needle.is_empty() {
            let tasks = self
                .store
                .core
                .search_tasks(
                    self.store.workspace_id.clone(),
                    query.trim().to_string(),
                    false,
                    false,
                    LIMIT as i64,
                )
                .unwrap_or_default();
            hits.extend(
                tasks
                    .into_iter()
                    .map(|hit| hit.task)
                    .filter(workable)
                    .map(task_hit),
            );
        }
        hits
    }

    /// Stops the session, logging the time so far without ticking the task.
    pub fn stop_focus(&mut self) {
        self.focus_prompt = None;
        if !self.end_block(false) {
            return;
        }
        if let Some(session) = self.focus_session.clone() {
            self.write(|store| store.core.finish_focus_session(session.id, now_ms()));
        }
        self.reload_focus();
        self.message = Some("Focus stopped. The time so far is logged.".into());
    }

    fn end_block(&mut self, complete: bool) -> bool {
        let Some(session) = self.focus_session.clone() else {
            self.message = Some("No focus block is running.".into());
            return false;
        };
        let now = now_ms();
        if session.active_task_id.is_some() && session.phase != "onBreak" {
            let context = focus_context(&self.store.core, &self.store.workspace_id);
            let zone = self.store.zone.clone();
            let elapsed = elapsed_seconds(&session, now);
            let finished = self.write(|store| {
                store.core.finish_focus_block(
                    session.id.clone(),
                    elapsed,
                    None,
                    complete,
                    session.active_block_id.clone(),
                    context,
                    now,
                    zone,
                )
            });
            if finished.is_none() && complete {
                return false;
            }
        }
        self.reload_focus();
        if let Some(id) = session.active_task_id.clone() {
            self.linger(&id);
        }
        self.reload_rows();
        self.reload_today();
        if complete {
            self.message = Some("Block finished.".into());
        }
        true
    }

    /// The running task's title and the clock, for the status bar.
    pub fn focus_status(&self) -> Option<(String, String, bool)> {
        let session = self.focus_session.as_ref()?;
        let now = now_ms();
        if session.phase == "onBreak" {
            let left = session
                .break_ends_at_ms
                .map(|end| ((end - now) / 1000).max(0))
                .unwrap_or(0);
            return Some(("Break".into(), clock(left), false));
        }
        let title = session
            .active_task_id
            .as_deref()
            .and_then(|id| {
                self.task(id).map(|task| task.title.clone()).or_else(|| {
                    self.store
                        .core
                        .task(id.into())
                        .ok()
                        .flatten()
                        .map(|t| t.title)
                })
            })
            .unwrap_or_else(|| "Focus".into());
        let elapsed = elapsed_seconds(session, now);
        let planned = session.work_duration_seconds.max(1);
        let paused = session.paused_at_ms.is_some();
        let title = if paused {
            format!("{title} (paused)")
        } else {
            title
        };
        if is_timer(session) {
            return Some((title, clock(elapsed), false));
        }
        Some((
            title,
            format!("{} / {}", clock(elapsed), clock(planned)),
            elapsed > planned,
        ))
    }

    /// Return confirms a pending "start anyway"; any other key drops it.
    pub fn confirm_pending_focus(&mut self, key: &str) -> bool {
        let Some((id, kind)) = self.pending_focus.take() else {
            return false;
        };
        if key == "enter" {
            self.selected_task = Some(id);
            self.start_focus(kind, true);
        } else {
            self.message = None;
        }
        if matches!(self.editing, Some(Editing::TaskTitle { .. })) {
            self.editing = None;
        }
        true
    }
}

/// `12:05`, or `1:02:05` past the hour.
pub fn clock(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let (h, m, s) = (seconds / 3600, (seconds / 60) % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(started: i64, accumulated: Option<i64>, paused: Option<i64>) -> SessionRow {
        SessionRow {
            id: "s".into(),
            started_at_ms: started,
            ended_at_ms: None,
            phase: "running".into(),
            active_task_id: Some("t".into()),
            active_task_started_at_ms: started,
            work_duration_seconds: 1500,
            break_duration_seconds: 300,
            break_ends_at_ms: None,
            active_block_id: None,
            accumulated_seconds: accumulated,
            paused_at_ms: paused,
            checkpoint_at_ms: None,
            open_ended: false,
        }
    }

    #[test]
    fn elapsed_counts_the_running_stretch_and_what_was_banked() {
        assert_eq!(elapsed_seconds(&session(0, None, None), 90_000), 90);
        assert_eq!(elapsed_seconds(&session(0, Some(60), None), 30_000), 90);
        assert_eq!(
            elapsed_seconds(&session(0, Some(60), Some(10)), 999_000),
            60
        );
    }

    #[test]
    fn the_clock_reads_minutes_then_hours() {
        assert_eq!(clock(65), "1:05");
        assert_eq!(clock(3725), "1:02:05");
    }
}
