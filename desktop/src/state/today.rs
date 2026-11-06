//! Today: the day's plan and the ladder of what to do next, from every
//! list, ranked by the core (`next_up`).
//!
//! Ranking reads the whole workspace, about 20 ms, so it runs on GPUI's
//! background executor (`compute`) and is swapped in when it lands. The
//! main thread only marks it stale.

use super::App;
use super::store::now_ms;
use std::collections::HashMap;
use std::sync::Arc;
use takt_core::focus::FocusContext;
use takt_core::records::TaskRow;
use takt_core::workspace::CoreWorkspace;

/// How many of the ladder's head the Mac shows below the day.
const LADDER_LIMIT: u32 = 8;

#[derive(Debug, Clone)]
pub struct DayRow {
    pub task: TaskRow,
    /// Why it is in the day ("planned", "overdue", "dueToday", …) or on the
    /// ladder ("priority", "dueSoon", …).
    pub reason: String,
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Today {
    pub day: Vec<DayRow>,
    pub ladder: Vec<DayRow>,
    pub ladder_count: u64,
    pub blocked_count: usize,
}

impl Today {
    pub fn rows(&self) -> impl Iterator<Item = &DayRow> {
        self.day.iter().chain(self.ladder.iter())
    }
}

/// The day and the ladder, read and ranked. Safe off the main thread: the
/// core serialises on its own handles.
pub fn compute(
    core: Arc<CoreWorkspace>,
    workspace_id: String,
    zone: String,
) -> Result<Today, String> {
    let now = now_ms();
    let stored = core
        .focus_context(workspace_id)
        .map_err(|e| e.to_string())?
        .map(|stored| takt_core::focus_context::focus_context_in_force(stored, now))
        .unwrap_or_default();
    let context = FocusContext {
        condition_ids: stored.condition_ids,
        ends_at_ms: stored.available_until_ms,
        mode: stored.mode,
    };
    let next = core
        .next_up(now, zone, context, None, Some(LADDER_LIMIT))
        .map_err(|e| e.to_string())?;
    let mut ids: Vec<String> = next.day_plan.iter().map(|entry| entry.id.clone()).collect();
    ids.extend(next.ranked.iter().map(|scored| scored.candidate.id.clone()));
    let tasks: HashMap<String, TaskRow> = core
        .tasks_by_id(ids)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|task| (task.id.clone(), task))
        .collect();
    let day: Vec<DayRow> = next
        .day_plan
        .iter()
        .filter_map(|entry| {
            Some(DayRow {
                task: tasks.get(&entry.id)?.clone(),
                reason: entry.reason.clone(),
                explanation: None,
            })
        })
        .collect();
    let in_day: std::collections::HashSet<&str> =
        day.iter().map(|row| row.task.id.as_str()).collect();
    let ladder = next
        .ranked
        .iter()
        .filter(|scored| !in_day.contains(scored.candidate.id.as_str()))
        .take(LADDER_LIMIT as usize)
        .filter_map(|scored| {
            Some(DayRow {
                task: tasks.get(&scored.candidate.id)?.clone(),
                reason: scored.reason.clone(),
                explanation: scored.explanation.clone(),
            })
        })
        .collect();
    Ok(Today {
        day,
        ladder,
        ladder_count: next.ranked_count,
        blocked_count: next.blocked.len(),
    })
}

impl App {
    /// Asks for the day to be ranked again; the window runs it in the
    /// background and hands it to `apply_today`.
    pub fn reload_today(&mut self) {
        self.today_stale = true;
    }

    pub fn apply_today(&mut self, today: Result<Today, String>) {
        match today {
            Ok(today) => {
                let keep = self
                    .selected_task
                    .as_deref()
                    .is_some_and(|id| today.rows().any(|row| row.task.id == id));
                if !keep && self.view_mode == super::ViewMode::Today {
                    self.selected_task = today.rows().next().map(|row| row.task.id.clone());
                }
                self.today = today;
            }
            Err(error) => self.message = Some(error),
        }
    }

    /// The rows Today draws, in order: what the keys move through there.
    pub fn today_ids(&self) -> Vec<String> {
        self.today.rows().map(|row| row.task.id.clone()).collect()
    }
}
