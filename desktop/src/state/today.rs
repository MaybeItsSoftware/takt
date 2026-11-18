//! Today: the day's plan and the ladder of what to do next, from every
//! list, ranked by the core (`next_up`).
//!
//! Ranking reads the whole workspace, about 20 ms, so it runs on GPUI's
//! background executor (`compute`) and is swapped in when it lands. The
//! main thread only marks it stale.

use super::App;
use super::store::now_ms;
use std::collections::{HashMap, HashSet};
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
    /// Every daily still owed today, planned or not, unless it is the one
    /// being timed.
    pub dailies: Vec<DayRow>,
    pub ladder: Vec<DayRow>,
    /// Each daily due today, by its task: ticking that task logs the day
    /// rather than finishing it.
    pub daily_by_task: HashMap<String, DailyToday>,
    /// The minute this was ranked in: when the clock moves on, the day is
    /// ranked again, so a start arriving or midnight passing shows.
    pub ranked_minute: i64,
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
    /// Each row of `old` that is lingering and missing from `self`, put
    /// back in its section at the place it had.
    pub fn keep_lingering(&mut self, old: &Today, lingering: &HashSet<String>) {
        if lingering.is_empty() {
            return;
        }
        let present: HashSet<String> = self.rows().map(|row| row.task.id.clone()).collect();
        for (section, before) in [
            (&mut self.day, &old.day),
            (&mut self.dailies, &old.dailies),
            (&mut self.ladder, &old.ladder),
        ] {
            for (at, row) in before.iter().enumerate() {
                if lingering.contains(&row.task.id) && !present.contains(&row.task.id) {
                    section.insert(at.min(section.len()), row.clone());
                }
            }
        }
    }

    /// Drops the finished rows among `ids`, whose lingering is over.
    pub fn let_go(&mut self, ids: &[String]) -> bool {
        let before = self.rows().count();
        for section in [&mut self.day, &mut self.dailies, &mut self.ladder] {
            section.retain(|row| row.task.status == "open" || !ids.contains(&row.task.id));
        }
        self.rows().count() != before
    }

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
    // Every daily due today is drawn in the dailies section, planned or
    // not, so the habits sit together rather than split across the day;
    // only the one being timed stays in the day.
    let day: Vec<DayRow> = day
        .into_iter()
        .filter(|row| row.reason == "running" || !daily_by_task.contains_key(&row.task.id))
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
        ranked_minute: now / 60_000,
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
            Ok(mut today) => {
                // A task just finished is out of the new ranking, but it
                // stays drawn where it was until its seconds are up, so its
                // celebration plays and a slip can be undone.
                today.keep_lingering(&self.today, &self.lingering);
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

    /// True, and the day marked stale, when a minute has passed since the
    /// day was ranked: a start arriving puts its task on the day, and
    /// midnight turns over what is due, planned and ticked. Ranking runs in
    /// the background in about 20 ms, so once a minute costs nothing.
    pub fn check_clock_moved(&mut self) -> bool {
        let minute = now_ms() / 60_000;
        if self.today.ranked_minute == 0 || self.today.ranked_minute == minute || self.today_stale {
            return false;
        }
        self.today.ranked_minute = minute;
        self.reload_today();
        true
    }

    /// The rows Today draws, in order: what the keys move through there.
    pub fn today_ids(&self) -> Vec<String> {
        self.today.rows().map(|row| row.task.id.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, status: &str) -> DayRow {
        DayRow {
            task: TaskRow {
                id: id.into(),
                list_id: "l".into(),
                parent_task_id: None,
                title: id.into(),
                notes: String::new(),
                status: status.into(),
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
            reason: "planned".into(),
            explanation: None,
        }
    }

    fn ids(rows: &[DayRow]) -> Vec<&str> {
        rows.iter().map(|row| row.task.id.as_str()).collect()
    }

    #[test]
    fn a_finished_task_stays_in_its_place_through_a_rerank_until_let_go() {
        let old = Today {
            day: vec![row("a", "open"), row("b", "completed"), row("c", "open")],
            ..Default::default()
        };
        let mut new = Today {
            day: vec![row("a", "open"), row("c", "open")],
            ..Default::default()
        };
        let lingering: HashSet<String> = ["b".to_string()].into();
        new.keep_lingering(&old, &lingering);
        assert_eq!(ids(&new.day), ["a", "b", "c"]);
        // Reranked again while it lingers: still once.
        new.keep_lingering(&new.clone(), &lingering);
        assert_eq!(ids(&new.day), ["a", "b", "c"]);
        assert!(new.let_go(&["b".to_string()]));
        assert_eq!(ids(&new.day), ["a", "c"]);
    }

    #[test]
    fn a_task_opened_again_is_not_let_go() {
        let mut today = Today {
            day: vec![row("b", "open")],
            ..Default::default()
        };
        assert!(!today.let_go(&["b".to_string()]));
    }
}
