//! Commands typed into the palette: `due fri 4pm`, `start next week`,
//! `repeat every monday`, `tag errand`, `priority 2`, `list groceries`.
//! The core reads them (`command::parse`) and resolves their date words
//! (`command::resolve`); this applies them to the selection.
//!
//! The edit commands (`taskEditDue`, `taskEditTags`, …) open the palette
//! with the word typed for you, so a value is always typed in one place.

use super::store::now_ms;
use super::{App, Overlay, Region};
use takt_core::command::{NamedHours, PaletteCommand, parse, resolve};

/// The word a palette edit starts with, for each edit command.
pub fn prefill(command: &str) -> Option<&'static str> {
    Some(match command {
        "taskEditDue" => "due ",
        "taskEditStart" => "start ",
        "taskEditRecurrence" => "repeat ",
        "taskEditTags" => "tag ",
        _ => return None,
    })
}

/// Whether `input` is a command the palette can run on the selection, and
/// what to call it in the palette's first row.
pub fn describe(input: &str) -> Option<String> {
    let label = match parse(input) {
        PaletteCommand::Unknown { .. } => return None,
        PaletteCommand::Due { raw } => format!("Due {raw}"),
        PaletteCommand::SetStart { raw } => format!("Start {raw}"),
        PaletteCommand::SetRecurrence { raw } => format!("Repeat {raw}"),
        PaletteCommand::Tag { tag } => format!("Tag #{tag}"),
        PaletteCommand::Untag { tag } => format!("Remove tag #{tag}"),
        PaletteCommand::Priority { rank } => format!("Priority {rank}"),
        PaletteCommand::List { query } => format!("Go to list \u{201c}{query}\u{201d}"),
        _ => input.trim().to_string(),
    };
    Some(label)
}

/// A resolved `due`/`start` value: a day, or a moment in milliseconds.
enum When {
    Day(String),
    Moment(i64),
}

fn when(raw: &str, zone: &str) -> Option<When> {
    let tz = takt_core::periodic::zone(zone);
    let resolved = resolve(raw, now_ms(), tz, NamedHours::default());
    if let Ok(day) = chrono::NaiveDate::parse_from_str(&resolved, "%Y-%m-%d") {
        return Some(When::Day(day.format("%Y-%m-%d").to_string()));
    }
    chrono::DateTime::parse_from_str(&resolved, "%Y-%m-%d %H:%M:%S %z")
        .ok()
        .map(|moment| When::Moment(moment.timestamp_millis()))
}

/// A moment typed as `fri 4pm` or `tomorrow`: a day alone is nine in the
/// morning of it, when a follow-up or a reminder is wanted.
pub fn moment(raw: &str, zone: &str) -> Option<i64> {
    match when(raw, zone)? {
        When::Moment(ms) => Some(ms),
        When::Day(day) => day_start_ms(&day).map(|ms| ms + 9 * 3600 * 1000),
    }
}

/// Local midnight of a `yyyy-MM-dd` day, in milliseconds.
fn day_start_ms(day: &str) -> Option<i64> {
    use chrono::{Local, TimeZone};
    let date = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?;
    Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
        .earliest()
        .map(|time| time.timestamp_millis())
}

impl App {
    /// Opens the palette with an edit's word already typed.
    pub fn begin_typed_edit(&mut self, command: &str) {
        if self.selected().is_none() {
            return;
        }
        if let Some(word) = prefill(command) {
            self.palette_prefill = Some(word.to_string());
            self.open_overlay(Overlay::CommandPalette);
        }
    }

    /// Runs a typed palette command on the selection.
    pub fn apply_typed(&mut self, input: &str) {
        let zone = self.store.zone.clone();
        match parse(input) {
            PaletteCommand::Done => self.with_selected(|app, id| app.set_status(id, "completed")),
            PaletteCommand::Undone => self.with_selected(|app, id| app.set_status(id, "open")),
            PaletteCommand::Invalidate => {
                self.with_selected(|app, id| app.set_status(id, "cancelled"))
            }
            PaletteCommand::Due { raw } => match when(&raw, &zone) {
                Some(When::Day(day)) => self.edit_selected(|edit| {
                    edit.due_at_ms = None;
                    let mut planning = edit.planning.clone().unwrap_or_default();
                    planning.due_date = Some(day);
                    edit.planning = planning.normalized();
                }),
                Some(When::Moment(ms)) => self.edit_selected(|edit| {
                    edit.due_at_ms = Some(ms);
                    if let Some(planning) = edit.planning.as_mut() {
                        planning.due_date = None;
                    }
                }),
                None => {
                    self.message = Some(format!("Couldn't read \u{201c}{raw}\u{201d} as a date."))
                }
            },
            PaletteCommand::ClearDue => self.edit_selected(|edit| {
                edit.due_at_ms = None;
                if let Some(planning) = edit.planning.as_mut() {
                    planning.due_date = None;
                }
            }),
            PaletteCommand::SetStart { raw } => {
                let start = match when(&raw, &zone) {
                    Some(When::Day(day)) => day_start_ms(&day),
                    Some(When::Moment(ms)) => Some(ms),
                    None => None,
                };
                match start {
                    Some(start) => self.schedule_selected(Some(start)),
                    None => {
                        self.message =
                            Some(format!("Couldn't read \u{201c}{raw}\u{201d} as a date."))
                    }
                }
            }
            PaletteCommand::ClearStart => self.schedule_selected(None),
            PaletteCommand::SetRecurrence { raw } => {
                self.edit_selected(|edit| edit.metadata.recurrence_rule = Some(raw))
            }
            PaletteCommand::ClearRecurrence => {
                self.edit_selected(|edit| edit.metadata.recurrence_rule = None)
            }
            PaletteCommand::Tag { tag } => self.edit_selected(|edit| {
                if !edit.metadata.tags.contains(&tag) {
                    edit.metadata.tags.push(tag);
                }
            }),
            PaletteCommand::Untag { tag } => {
                self.edit_selected(|edit| edit.metadata.tags.retain(|other| *other != tag))
            }
            PaletteCommand::Priority { rank } => {
                self.edit_selected(|edit| edit.metadata.priority = Some(rank))
            }
            PaletteCommand::PriorityBack => {
                self.edit_selected(|edit| edit.metadata.priority = Some(-1))
            }
            PaletteCommand::ClearPriority => {
                self.edit_selected(|edit| edit.metadata.priority = None)
            }
            PaletteCommand::Delete => self.run("taskDelete", None),
            PaletteCommand::MoveUp => self.run("taskMoveUp", None),
            PaletteCommand::MoveDown => self.run("taskMoveDown", None),
            PaletteCommand::Undo => self.run("windowUndo", None),
            PaletteCommand::EnterChildren => self.run("planEnterTask", None),
            PaletteCommand::ExitParent => self.run("planLeaveTask", None),
            PaletteCommand::ExpandTask | PaletteCommand::CollapseTask => {
                self.run("planToggleFold", None)
            }
            PaletteCommand::ExpandAll => self.run("planUnfoldAll", None),
            PaletteCommand::CollapseAll => self.run("planFoldAll", None),
            PaletteCommand::AddSibling => self.run("taskNew", None),
            PaletteCommand::AddChild => self.run("taskNewChild", None),
            PaletteCommand::Edit => self.run("taskRename", None),
            PaletteCommand::EditAtStart => self.run("taskRenameInsert", None),
            PaletteCommand::Search => self.open_overlay(Overlay::Search),
            PaletteCommand::OpenCommandPalette => self.open_overlay(Overlay::CommandPalette),
            PaletteCommand::ToggleTimer => {
                if self.focus_session.is_some() {
                    self.stop_focus();
                } else {
                    self.start_focus_on_selected(false);
                }
            }
            PaletteCommand::ToggleHideFuture => self.run("planHideCompleted", None),
            PaletteCommand::List { query } => self.go_to_list_named(&query),
            other => {
                self.message = Some(format!("{other:?} isn't in the Rust app yet."));
            }
        }
    }

    fn with_selected(&mut self, act: impl FnOnce(&mut App, &str)) {
        if let Some(id) = self.selected_task.clone() {
            act(self, &id);
        }
    }

    fn go_to_list_named(&mut self, query: &str) {
        let candidates = self
            .lists
            .iter()
            .map(|list| takt_core::palette::PaletteCandidate {
                title: list.name.clone(),
                detail: String::new(),
                keys: Vec::new(),
                boost: 0,
            })
            .collect();
        let best = takt_core::palette::palette_rank(query.into(), candidates)
            .first()
            .and_then(|ix| self.lists.get(*ix as usize))
            .map(|list| list.id.clone());
        match best {
            Some(id) => {
                self.select_list(&id);
                self.region = Region::Tasks;
            }
            None => self.message = Some(format!("No list matches \u{201c}{query}\u{201d}.")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_open_with_their_word() {
        assert_eq!(prefill("taskEditDue"), Some("due "));
        assert_eq!(prefill("taskComplete"), None);
    }

    #[test]
    fn typed_commands_are_named_and_unknown_ones_are_not() {
        assert_eq!(describe("due tomorrow").as_deref(), Some("Due tomorrow"));
        assert_eq!(describe("tag errand").as_deref(), Some("Tag #errand"));
        assert_eq!(describe("make tea"), None);
    }

    #[test]
    fn a_day_resolves_to_a_day_and_a_time_to_a_moment() {
        assert!(matches!(
            when("tomorrow", "Europe/London"),
            Some(When::Day(_))
        ));
        assert!(matches!(
            when("tomorrow 4pm", "Europe/London"),
            Some(When::Moment(_))
        ));
        assert!(when("whenever", "Europe/London").is_none());
    }
}
