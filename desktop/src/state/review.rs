//! The bottom dock's Timeline and Progress tabs: a day of focus against the
//! clock, and the last fortnight's closing, adding and focus. Shaped by the
//! core (`review_timeline`, `review_progress`), as the phones' Review screen
//! is. `WorkspaceTimelineScreen.swift` and the progress graph.

use super::store::now_ms;
use super::{App, Region};
use takt_core::review::{ReviewAwardPoints, ReviewProgress, ReviewTimeline, ReviewTimelineBlock};
use takt_core::rows::WorkBlockRow;

const DAY_MS: i64 = 24 * 3600 * 1000;
pub const PROGRESS_DAYS: u32 = 14;

#[derive(Debug, Clone, Default)]
pub struct Timeline {
    /// Any moment of the day shown.
    pub day_ms: i64,
    /// The day's blocks, the running one last when it is today's.
    pub blocks: Vec<Block>,
    pub shaped: Option<ReviewTimeline>,
    /// The summary (a task of the day) with the keyboard.
    pub cursor: usize,
}

/// A block as the timeline draws it.
#[derive(Debug, Clone, Default)]
pub struct Block {
    pub task_id: Option<String>,
    pub title: String,
}

impl Timeline {
    /// The day's tasks, most time first: (title, task id, seconds, blocks).
    pub fn summaries(&self) -> Vec<(String, Option<String>, i64, u32)> {
        let Some(shaped) = &self.shaped else {
            return Vec::new();
        };
        shaped
            .summaries
            .iter()
            .filter_map(|summary| {
                let block = self.blocks.get(summary.latest as usize)?;
                Some((
                    block.title.clone(),
                    block.task_id.clone(),
                    summary.seconds,
                    summary.blocks,
                ))
            })
            .collect()
    }
}

impl App {
    /// Reads the day the timeline shows.
    pub fn reload_timeline(&mut self) {
        let now = now_ms();
        if self.timeline.day_ms == 0 {
            self.timeline.day_ms = now;
        }
        let day_start = local_midnight(self.timeline.day_ms);
        let rows: Vec<WorkBlockRow> = self
            .store
            .core
            .work_blocks_between(day_start, day_start + DAY_MS + 3600 * 1000)
            .unwrap_or_default();
        let mut blocks: Vec<Block> = rows
            .iter()
            .map(|row| Block {
                task_id: row.task_id.clone(),
                title: row.task_title.clone(),
            })
            .collect();
        let mut inputs: Vec<ReviewTimelineBlock> = rows
            .iter()
            .map(|row| ReviewTimelineBlock {
                id: row.id.clone(),
                task_key: row
                    .original_task_id
                    .clone()
                    .or_else(|| row.task_id.clone())
                    .unwrap_or_else(|| row.task_title.clone()),
                seconds: row.seconds,
                ended_at_ms: row.recorded_at_ms,
                is_live: false,
            })
            .collect();
        if let Some(session) = self.focus_session.clone()
            && let Some(task_id) = session.active_task_id.clone()
        {
            let seconds = super::focus::elapsed_seconds(&session, now);
            let title = self
                .task(&task_id)
                .map(|t| t.title.clone())
                .or_else(|| {
                    self.store
                        .core
                        .task(task_id.clone())
                        .ok()
                        .flatten()
                        .map(|t| t.title)
                })
                .unwrap_or_default();
            blocks.push(Block {
                task_id: Some(task_id.clone()),
                title,
            });
            inputs.push(ReviewTimelineBlock {
                id: session
                    .active_block_id
                    .clone()
                    .unwrap_or_else(|| session.id.clone()),
                task_key: task_id,
                seconds,
                ended_at_ms: now,
                is_live: true,
            });
        }
        let awards: Vec<ReviewAwardPoints> = self
            .store
            .core
            .recent_focus_awards(400)
            .unwrap_or_default()
            .into_iter()
            .map(|award| ReviewAwardPoints {
                id: award.id,
                points: award.points,
            })
            .collect();
        let shaped = takt_core::review::review_timeline(
            inputs,
            awards,
            self.timeline.day_ms,
            now,
            self.store.zone.clone(),
        );
        self.timeline.blocks = blocks;
        self.timeline.shaped = Some(shaped);
        let count = self.timeline.summaries().len();
        self.timeline.cursor = self.timeline.cursor.min(count.saturating_sub(1));
    }

    pub fn reload_progress(&mut self) {
        self.progress = self
            .store
            .core
            .review_progress(PROGRESS_DAYS, now_ms(), self.store.zone.clone())
            .ok();
    }

    /// Whether the timeline shows today.
    pub fn timeline_is_today(&self) -> bool {
        local_midnight(self.timeline.day_ms) == local_midnight(now_ms())
    }

    pub fn step_timeline_day(&mut self, by: i64) {
        let next = self.timeline.day_ms + by * DAY_MS;
        self.timeline.day_ms = next.min(now_ms());
        self.timeline.cursor = 0;
        self.reload_timeline();
    }

    pub fn timeline_today(&mut self) {
        self.timeline.day_ms = now_ms();
        self.reload_timeline();
    }

    pub fn move_timeline_cursor(&mut self, key: &str) {
        let count = self.timeline.summaries().len();
        if count == 0 {
            return;
        }
        let last = count - 1;
        let here = self.timeline.cursor;
        self.timeline.cursor = match key {
            "j" | "down" => (here + 1).min(last),
            "k" | "up" => here.saturating_sub(1),
            "gg" | "home" | "cmd+up" => 0,
            "shift+g" | "end" | "cmd+down" => last,
            _ => here,
        };
    }

    pub fn reveal_timeline(&mut self) {
        let Some((_, Some(id), _, _)) =
            self.timeline.summaries().get(self.timeline.cursor).cloned()
        else {
            return;
        };
        if let Some(task) = self.store.core.task(id.clone()).ok().flatten() {
            self.reveal_task(&task.id, &task.list_id);
        } else {
            self.message = Some("That task is gone.".into());
        }
    }

    pub fn leave_timeline(&mut self) {
        if self.region == Region::Timeline {
            self.region = Region::Tasks;
        }
    }

    /// `⌘9`: the timeline tab, or away again from inside it.
    pub fn go_timeline(&mut self) {
        self.toggle_bottom_tab("timeline");
    }
}

/// Local midnight of the day containing `ms`.
pub fn local_midnight(ms: i64) -> i64 {
    use chrono::{Local, TimeZone};
    let Some(time) = Local.timestamp_millis_opt(ms).single() else {
        return ms;
    };
    let midnight = time.date_naive().and_hms_opt(0, 0, 0).unwrap_or_default();
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map_or(ms, |t| t.timestamp_millis())
}

/// The progress summary held for the Progress tab.
pub type Progress = Option<ReviewProgress>;
