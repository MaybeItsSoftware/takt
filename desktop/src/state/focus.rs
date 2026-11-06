//! Focus sessions: starting one on a task, the running clock, finishing the
//! block (the task ticked off and the time logged) or stopping (the time so
//! far logged). `WorkspaceViewModel+Focus.swift`, through the core's focus
//! calls; the core keeps the clock, this reads it.
//!
//! Not yet: the "how did it go?" prompt after a block, breaks, the queue
//! and the floating focus panel.

use super::store::now_ms;
use super::{App, Editing};
use takt_core::focus::FocusContext;
use takt_core::rows::SessionRow;
use takt_core::workspace::CoreWorkspace;

/// The Swift app's defaults: 25-minute blocks, 5-minute breaks.
const WORK_SECONDS: i64 = 25 * 60;
const BREAK_SECONDS: i64 = 5 * 60;

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

    /// `⌘⇧F` on a task: a block on it, as long as the core suggests for
    /// it. A task ruled out now (a condition, a start in the future) asks
    /// first; Return starts it anyway.
    pub fn start_focus_on_selected(&mut self, override_availability: bool) {
        let Some(task) = self.selected().cloned() else {
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
        let available = candidate.is_some();
        if !available && !override_availability {
            self.pending_focus = Some(task.id.clone());
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
        let started = self.write(|store| {
            store.core.start_focus_session(
                task.id.clone(),
                Some(planned),
                WORK_SECONDS,
                BREAK_SECONDS,
                Some(context),
                !available,
                now,
                zone,
            )
        });
        if started.is_some() {
            self.reload_focus();
            self.reload_today();
            self.message = Some(format!(
                "Focusing on \u{201c}{}\u{201d} for {} min.",
                task.title,
                planned / 60
            ));
        }
    }

    /// Finishes the block: the task ticked off and the time logged.
    pub fn finish_focus_block(&mut self) {
        self.end_block(true);
    }

    /// Stops the session, logging the time so far without ticking the task.
    pub fn stop_focus(&mut self) {
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
        self.lingering.extend(session.active_task_id.clone());
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
        Some((
            if paused {
                format!("{title} (paused)")
            } else {
                title
            },
            format!("{} / {}", clock(elapsed), clock(planned)),
            elapsed > planned,
        ))
    }

    /// Return confirms a pending "start anyway"; any other key drops it.
    pub fn confirm_pending_focus(&mut self, key: &str) -> bool {
        let Some(id) = self.pending_focus.take() else {
            return false;
        };
        if key == "enter" {
            self.selected_task = Some(id);
            self.start_focus_on_selected(true);
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
