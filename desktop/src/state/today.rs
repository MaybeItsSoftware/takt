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
    /// The dailies still owed today that the day isn't already showing, as
    /// the phones' dailies strip has them.
    pub dailies: Vec<DayRow>,
    pub ladder: Vec<DayRow>,
    /// Each daily due today, by its task: ticking that task logs the day
    /// rather than finishing it.
    pub daily_by_task: HashMap<String, DailyToday>,
    /// The local day this was ranked for (`yyyy-MM-dd`): when the clock
    /// passes into the next, the day is ranked again.
    pub day_key: String,
    pub ladder_count: u64,
    pub blocked_count: usize,
}

/// A daily due today: which it is, and whether today is ticked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyToday {
    pub daily_id: String,
    pub done: bool,
}

impl Today {
    pub fn rows(&self) -> impl Iterator<Item = &DayRow> {
        self.day
            .iter()
            .chain(self.dailies.iter())
            .chain(self.ladder.iter())
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
        .next_up(now, zone.clone(), context, None, Some(LADDER_LIMIT))
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
    let owed = core
        .dailies_on(now, zone.clone())
        .map_err(|e| e.to_string())?;
    let daily_by_task: HashMap<String, DailyToday> = owed
        .iter()
        .map(|daily| {
            (
                daily.task.id.clone(),
                DailyToday {
                    daily_id: daily.daily.id.clone(),
                    done: daily
                        .contribution
                        .as_ref()
                        .is_some_and(|c| c.completed_at_ms.is_some()),
                },
            )
        })
        .collect();
    let done_today = |id: &str| daily_by_task.get(id).is_some_and(|d| d.done);
    // Finished is finished: a daily met today leaves the day, as on the
    // phones.
    let day: Vec<DayRow> = day
        .into_iter()
        .filter(|row| !done_today(&row.task.id))
        .collect();
    let in_day: std::collections::HashSet<String> =
        day.iter().map(|row| row.task.id.clone()).collect();
    let dailies: Vec<DayRow> = owed
        .into_iter()
        .filter(|daily| !done_today(&daily.task.id) && !in_day.contains(&daily.task.id))
        .map(|daily| DayRow {
            task: daily.task,
            reason: "daily".into(),
            explanation: None,
        })
        .collect();
    let shown: std::collections::HashSet<String> = in_day
        .iter()
        .cloned()
        .chain(dailies.iter().map(|row| row.task.id.clone()))
        .collect();
    let ladder = next
        .ranked
        .iter()
        .filter(|scored| !shown.contains(&scored.candidate.id))
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
        dailies,
        ladder,
        daily_by_task,
        day_key: takt_core::dailies::day_key(now, &zone),
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
                let first = today.rows().next().map(|row| row.task.id.clone());
                self.today = today;
                if self.scope == super::Scope::Today {
                    // The board and the matrix draw the same tasks as cards.
                    self.reload_rows();
                    if !keep {
                        self.selected_task = first;
                    }
                }
            }
            Err(error) => self.message = Some(error),
        }
    }

    /// True, and the day marked stale, when midnight has passed since the
    /// day was ranked: what is due, planned and ticked all turn over.
    pub fn check_day_turned(&mut self) -> bool {
        let key = takt_core::dailies::day_key(now_ms(), &self.store.zone);
        if self.today.day_key.is_empty() || self.today.day_key == key || self.today_stale {
            return false;
        }
        self.today.day_key = key;
        self.reload_today();
        true
    }

    /// The rows Today draws, in order: what the keys move through there.
    pub fn today_ids(&self) -> Vec<String> {
        self.today.rows().map(|row| row.task.id.clone()).collect()
    }
}
