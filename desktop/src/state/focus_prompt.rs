//! The end of a block: "how did it go?", a break, and the tick that keeps
//! the clock honest. `WorkspaceFocusQualityPrompt.swift` and the focus half
//! of `WorkspaceViewModel+Focus.swift` and `+Conditions.swift`.
//!
//! Finishing a block pauses the clock and asks: a dial (×0.0 to ×5.0, the
//! block's minutes times it is its score), a note for the task, and the ways
//! out — keep working, log and leave open, done, or a break when the time
//! ran out. Nothing is credited until it is answered, and the seconds that
//! count are those captured when the block ended, not those spent deciding.

use super::focus::{elapsed_seconds, focus_context};
use super::store::now_ms;
use super::{App, Editing};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    KeepWorking,
    Progress,
    Done,
    Break,
}

impl Choice {
    pub fn title(self, prompt: &FocusPrompt) -> String {
        match self {
            Choice::KeepWorking => "Keep working".into(),
            Choice::Progress if prompt.complete_task => "Log, keep open".into(),
            Choice::Progress => "Log progress".into(),
            Choice::Done if prompt.time_is_up || !prompt.complete_task => "Done".into(),
            Choice::Done => "Complete".into(),
            Choice::Break => format!("{}-minute break", prompt.break_minutes),
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Choice::KeepWorking => "esc",
            Choice::Progress => "p",
            Choice::Done => "d",
            Choice::Break => "b",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FocusPrompt {
    pub session_id: String,
    pub task_id: String,
    pub title: String,
    pub seconds: i64,
    pub complete_task: bool,
    pub block_id: Option<String>,
    pub was_paused: bool,
    /// The clock ran past the block's length: a break is offered, and the
    /// task is not assumed done.
    pub time_is_up: bool,
    pub break_minutes: i64,
    /// The dial, in tenths: 10 is ×1.0.
    pub tenths: i64,
    pub chosen: Option<Choice>,
    pub note: String,
    /// The dial is drawn: always, unless the time ran out for someone who
    /// turned scoring off.
    pub scores: bool,
}

impl FocusPrompt {
    pub fn choices(&self) -> Vec<Choice> {
        use Choice::*;
        if self.time_is_up {
            vec![KeepWorking, Done, Break]
        } else if self.complete_task {
            vec![KeepWorking, Progress, Done]
        } else {
            vec![KeepWorking, Done, Progress]
        }
    }

    pub fn selected(&self) -> Choice {
        self.chosen
            .unwrap_or_else(|| *self.choices().last().unwrap_or(&Choice::Done))
    }

    pub fn multiplier(&self) -> f64 {
        if self.scores {
            self.tenths as f64 / 10.
        } else {
            1.
        }
    }

    pub fn points(&self) -> f64 {
        takt_core::progress::focus_score(self.seconds, self.multiplier())
    }
}

/// A note added to the end of a task's notes, stamped with the day.
pub fn appended_note(notes: &str, note: &str) -> String {
    let note = note.trim();
    if note.is_empty() {
        return notes.to_string();
    }
    let stamp = chrono::Local::now().format("%-d %b");
    let entry = format!("{stamp} — {note}");
    let existing = notes.trim_end();
    if existing.is_empty() {
        entry
    } else {
        format!("{existing}\n\n{entry}")
    }
}

impl App {
    /// `focusFinish`: asks how the block went. On a break, back to work.
    pub fn request_focus_completion(&mut self, complete_task: bool) {
        let Some(session) = self.focus_session.clone() else {
            self.message = Some("No focus block is running.".into());
            return;
        };
        if session.phase == "onBreak" {
            self.end_focus_break();
            return;
        }
        if self.focus_prompt.is_some() {
            return;
        }
        let Some(task_id) = session.active_task_id.clone() else {
            return;
        };
        let now = now_ms();
        let seconds = elapsed_seconds(&session, now);
        let time_is_up = seconds >= session.work_duration_seconds;
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
        self.focus_prompt = Some(FocusPrompt {
            session_id: session.id.clone(),
            task_id,
            title,
            seconds,
            complete_task,
            block_id: session.active_block_id.clone(),
            was_paused: session.paused_at_ms.is_some(),
            time_is_up,
            break_minutes: (session.break_duration_seconds / 60).max(1),
            tenths: 10,
            chosen: None,
            note: String::new(),
            scores: !time_is_up || self.prefs.scores_each_block,
        });
        if session.paused_at_ms.is_none() {
            self.write(|s| s.core.pause_focus_session(session.id.clone(), now));
            self.reload_focus();
        }
        if !self.prefs.scores_each_block && !time_is_up {
            self.answer_focus_prompt(Choice::Done, None);
        }
    }

    /// A key while the prompt is up. True when it was the prompt's.
    pub fn focus_prompt_key(&mut self, key: &str) -> bool {
        let Some(prompt) = self.focus_prompt.as_mut() else {
            return false;
        };
        let choices = prompt.choices();
        let here = choices
            .iter()
            .position(|c| *c == prompt.selected())
            .unwrap_or(0);
        match key {
            "up" | "k" | "=" | "+" => prompt.tenths = (prompt.tenths + 1).min(50),
            "down" | "j" | "-" => prompt.tenths = (prompt.tenths - 1).max(0),
            "left" | "h" => prompt.chosen = Some(choices[here.saturating_sub(1)]),
            "right" | "l" => prompt.chosen = Some(choices[(here + 1).min(choices.len() - 1)]),
            "r" => prompt.tenths = 10,
            "enter" => {
                let choice = prompt.selected();
                self.answer_focus_prompt(choice, None);
            }
            "d" => self.answer_focus_prompt(Choice::Done, None),
            "p" if !prompt.time_is_up => self.answer_focus_prompt(Choice::Progress, None),
            "b" if prompt.time_is_up => self.answer_focus_prompt(Choice::Break, None),
            "\\" => {
                let choice = *choices.last().unwrap_or(&Choice::Done);
                self.answer_focus_prompt(choice, Some(0.));
            }
            "n" | "tab" => self.editing = Some(Editing::FocusNote),
            "escape" => self.answer_focus_prompt(Choice::KeepWorking, None),
            _ => {}
        }
        true
    }

    pub fn answer_focus_prompt(&mut self, choice: Choice, multiplier: Option<f64>) {
        let Some(prompt) = self.focus_prompt.take() else {
            return;
        };
        let now = now_ms();
        let zone = self.store.zone.clone();
        if choice == Choice::KeepWorking {
            if !prompt.was_paused {
                self.write(|s| s.core.resume_focus_session(prompt.session_id.clone(), now));
            }
            self.reload_focus();
            return;
        }
        // The note first, so it survives a block that ended elsewhere.
        if !prompt.note.trim().is_empty()
            && let Ok(snapshot) = self.store.core.editor_snapshot(prompt.task_id.clone())
        {
            let notes = appended_note(&snapshot.notes, &prompt.note);
            let task = snapshot.clone();
            self.write(|s| {
                s.core.update_task(
                    task.task_id.clone(),
                    task.title.clone(),
                    notes,
                    task.due_at_ms,
                    task.estimate_seconds,
                    now,
                    zone.clone(),
                )
            });
        }
        let score = multiplier.unwrap_or_else(|| prompt.multiplier());
        let outcome = match choice {
            Choice::Break => self.write(|s| {
                s.core.take_focus_break(
                    prompt.session_id.clone(),
                    prompt.seconds,
                    Some(score),
                    prompt.block_id.clone(),
                    now,
                    zone.clone(),
                )
            }),
            _ => {
                let context = focus_context(&self.store.core, &self.store.workspace_id);
                self.write(|s| {
                    s.core.finish_focus_block(
                        prompt.session_id.clone(),
                        prompt.seconds,
                        Some(score),
                        choice == Choice::Done,
                        prompt.block_id.clone(),
                        context,
                        now,
                        zone.clone(),
                    )
                })
            }
        };
        if outcome.is_none() {
            self.message = Some("That block had already ended.".into());
        } else {
            let points = takt_core::progress::focus_score(prompt.seconds, score);
            self.message = Some(match choice {
                Choice::Break => format!("Logged, {points:.1} pts. On a break."),
                Choice::Done => format!("Done, {points:.1} pts."),
                _ => format!("Logged, {points:.1} pts."),
            });
            if choice == Choice::Done {
                self.celebrate(&prompt.task_id);
            }
        }
        self.reload_focus();
        self.lingering.insert(prompt.task_id.clone());
        self.reload_rows();
        self.reload_today();
        self.reload_done();
    }

    /// Back to work from a break: a fresh block on the task in hand, or the
    /// next queued one.
    pub fn end_focus_break(&mut self) {
        let Some(session) = self.focus_session.clone().filter(|s| s.phase == "onBreak") else {
            return;
        };
        let context = focus_context(&self.store.core, &self.store.workspace_id);
        let zone = self.store.zone.clone();
        if self
            .write(|s| {
                s.core
                    .end_focus_break(session.id.clone(), context, now_ms(), zone)
            })
            .is_some()
        {
            self.message = Some("Back to work.".into());
        }
        self.reload_focus();
        self.reload_today();
    }

    /// Once a second: the checkpoint every half minute, the prompt when a
    /// block reaches its length, and the note when a break ends. True when
    /// anything needs drawing.
    pub fn tick_focus(&mut self) -> bool {
        let Some(session) = self.focus_session.clone() else {
            return false;
        };
        let now = now_ms();
        if session.phase == "onBreak" {
            if let Some(ends) = session.break_ends_at_ms
                && ends <= now
                && self.focus_notified.as_deref() != session.active_block_id.as_deref()
            {
                self.focus_notified = session.active_block_id.clone();
                self.message = Some("Break's over. ⌘. goes back to work.".into());
                if self.prefs.focus_notifications {
                    crate::platform::notify(
                        "Break ended",
                        &self.focus_title(&session),
                        "Ready to resume focus or pick your next task.",
                        self.prefs.focus_sound,
                    );
                }
                return true;
            }
            return false;
        }
        if session.paused_at_ms.is_some() || session.active_task_id.is_none() {
            return false;
        }
        if now - self.focus_checkpointed_at >= 30_000 {
            self.focus_checkpointed_at = now;
            let _ = self
                .store
                .core
                .checkpoint_focus_session(session.id.clone(), now);
        }
        if elapsed_seconds(&session, now) >= session.work_duration_seconds
            && self.focus_notified.as_deref() != session.active_block_id.as_deref()
            && self.focus_prompt.is_none()
        {
            self.focus_notified = session.active_block_id.clone();
            if self.prefs.focus_notifications {
                crate::platform::notify(
                    "Focus block finished",
                    &self.focus_title(&session),
                    "Take a break or review the block.",
                    self.prefs.focus_sound,
                );
            }
            self.request_focus_completion(false);
            return true;
        }
        false
    }

    fn focus_title(&self, session: &takt_core::rows::SessionRow) -> String {
        session
            .active_task_id
            .as_deref()
            .and_then(|id| {
                self.task(id).map(|t| t.title.clone()).or_else(|| {
                    self.store
                        .core
                        .task(id.into())
                        .ok()
                        .flatten()
                        .map(|t| t.title)
                })
            })
            .unwrap_or_default()
    }

    /// Space on the panel: the clock stops, or runs again.
    pub fn toggle_focus_pause(&mut self) {
        let Some(session) = self.focus_session.clone().filter(|s| s.phase != "onBreak") else {
            return;
        };
        let now = now_ms();
        if session.paused_at_ms.is_some() {
            self.write(|s| s.core.resume_focus_session(session.id.clone(), now));
        } else {
            self.write(|s| s.core.pause_focus_session(session.id.clone(), now));
        }
        self.reload_focus();
    }

    /// The queue behind the block in hand.
    pub fn focus_queue(&self) -> Vec<takt_core::rows::QueueEntry> {
        self.focus_session
            .as_ref()
            .and_then(|s| self.store.core.focus_queue(s.id.clone()).ok())
            .unwrap_or_default()
    }

    /// A finished task, marked: the celebration the Swift app plays, here
    /// a line in the status bar saying what it closed.
    pub fn celebrate(&mut self, task_id: &str) {
        let title = self
            .store
            .core
            .task(task_id.into())
            .ok()
            .flatten()
            .map(|t| t.title)
            .unwrap_or_default();
        self.celebration = Some((title, now_ms()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(time_is_up: bool, complete: bool) -> FocusPrompt {
        FocusPrompt {
            session_id: "s".into(),
            task_id: "t".into(),
            title: "T".into(),
            seconds: 1500,
            complete_task: complete,
            block_id: None,
            was_paused: false,
            time_is_up,
            break_minutes: 5,
            tenths: 10,
            chosen: None,
            note: String::new(),
            scores: true,
        }
    }

    #[test]
    fn the_last_choice_is_what_the_key_asked_for() {
        assert_eq!(prompt(false, true).selected(), Choice::Done);
        assert_eq!(prompt(false, false).selected(), Choice::Progress);
        assert_eq!(prompt(true, false).selected(), Choice::Break);
        assert_eq!(prompt(false, true).points(), 25.0);
    }

    #[test]
    fn a_note_is_added_as_its_own_paragraph() {
        let notes = appended_note("First.\n", "went well");
        assert!(notes.starts_with("First.\n\n"));
        assert!(notes.ends_with("— went well"));
        assert_eq!(appended_note("x", "  "), "x");
    }
}
