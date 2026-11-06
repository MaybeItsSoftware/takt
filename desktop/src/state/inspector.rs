//! The inspector: the selected task's fields as a column of stops a cursor
//! walks, each edited in place and saved through the core's editor as one
//! undo step. `WorkspaceInspectorViews.swift` and its `InspectorCursor`.
//!
//! A text stop opens a field (Return or Esc gives the caret back; in the
//! notes Esc or ⌘Return), a switch flips, a choice steps, and an action
//! runs. `x` clears what can be cleared.

use super::store::now_ms;
use super::{App, Editing, Region};
use takt_core::editor::EditorSnapshot;
use takt_core::editor_draft::{editor_minutes_text, editor_parse_minutes};
use takt_core::records::TaskRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopKind {
    Text { multiline: bool },
    Toggle,
    Choice,
    Action,
}

/// One row of the inspector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stop {
    pub id: &'static str,
    /// The label column; empty for a row that spans it (the title, notes,
    /// the actions).
    pub label: &'static str,
    pub value: String,
    /// The value is a stand-in ("Add estimate"), drawn dim.
    pub empty: bool,
    pub kind: StopKind,
    /// A hairline goes above this row: the start of a group.
    pub group: bool,
}

impl Stop {
    fn new(id: &'static str, label: &'static str, kind: StopKind) -> Self {
        Stop {
            id,
            label,
            value: String::new(),
            empty: false,
            kind,
            group: false,
        }
    }

    fn value(mut self, value: Option<String>, add: &str) -> Self {
        match value.filter(|value| !value.trim().is_empty()) {
            Some(value) => self.value = value,
            None => {
                self.value = add.into();
                self.empty = true;
            }
        }
        self
    }

    fn grouped(mut self) -> Self {
        self.group = true;
        self
    }
}

pub const PRIORITY_LEVELS: [i64; 9] = [0, 1, 2, 3, 4, -4, -3, -2, -1];

pub fn priority_name(level: i64) -> &'static str {
    match level {
        1 => "Urgent",
        2 => "High",
        3 => "Medium",
        4 => "Low",
        -4 => "−4 · Later",
        -3 => "−3 · Later still",
        -2 => "−2 · Second to last",
        -1 => "−1 · Last",
        _ => "None",
    }
}

/// "Mon 3 Nov 14:00".
pub fn format_moment(ms: i64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|time| time.format("%a %-d %b %H:%M").to_string())
        .unwrap_or_default()
}

/// "Mon 3 Nov" from `yyyy-MM-dd`.
pub fn format_day_string(day: &str) -> String {
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .map(|date| date.format("%a %-d %b").to_string())
        .unwrap_or_else(|_| day.to_string())
}

fn is_list(task: &TaskRow) -> bool {
    task.item_kind.as_deref() == Some("list")
}

/// The stops for a task, in the order a task is decided on: what it is,
/// when, what it needs and takes, what it says, where it is filed, how it
/// comes round, and the changes that make it something else.
pub fn stops(
    snapshot: &EditorSnapshot,
    task: &TaskRow,
    list_name: &str,
    focus_running: bool,
) -> Vec<Stop> {
    use StopKind::*;
    let planning = snapshot.planning.clone().unwrap_or_default();
    let list = is_list(task);
    let mut stops = vec![Stop {
        value: snapshot.title.clone(),
        ..Stop::new("title", "", Text { multiline: false })
    }];
    if !list {
        stops.push(Stop {
            value: if focus_running {
                "Add to the focus queue".into()
            } else {
                "Start focus".into()
            },
            ..Stop::new("focus", "", Action)
        });
    }
    stops.push(
        Stop::new("start", "Start", Text { multiline: false })
            .value(planning.start_at_ms.map(format_moment), "Add start")
            .grouped(),
    );
    let due = snapshot
        .due_at_ms
        .map(format_moment)
        .or_else(|| planning.due_date.as_deref().map(format_day_string));
    let has_due = due.is_some();
    stops.push(Stop::new("due", "Due", Text { multiline: false }).value(due, "Add date"));
    if has_due {
        stops.push(Stop {
            value: if snapshot.due_at_ms.is_some() {
                "At a set time".into()
            } else {
                "Any time that day".into()
            },
            empty: snapshot.due_at_ms.is_none(),
            ..Stop::new("due.exact", "Time", Toggle)
        });
    }
    stops.push(Stop::new("conditions", "Needs", Action).value(
        planning.requirement_groups.as_ref().and_then(|groups| {
            (!groups.is_empty()).then(|| format!("{} condition groups", groups.len()))
        }),
        "Add conditions",
    ));
    stops.push(
        Stop::new("estimate", "Estimate", Text { multiline: false })
            .value(
                snapshot
                    .estimate_seconds
                    .map(|s| format!("{} min", editor_minutes_text(s, true))),
                "Add estimate",
            )
            .grouped(),
    );
    stops.push(
        Stop::new("minimum-block", "Min. block", Text { multiline: false }).value(
            planning
                .minimum_block_seconds
                .map(|s| format!("{} min", editor_minutes_text(s, true))),
            "Add minimum",
        ),
    );
    let single = planning.requires_single_sitting == Some(true);
    stops.push(Stop {
        value: if single {
            "Must finish in one go".into()
        } else {
            "Can be split".into()
        },
        empty: !single,
        ..Stop::new("single-sitting", "One sitting", Toggle)
    });
    stops.push(
        Stop::new("notes", "", Text { multiline: true })
            .value(Some(snapshot.notes.clone()), "Add notes")
            .grouped(),
    );
    let priority = snapshot.metadata.priority.unwrap_or(0);
    stops.push(Stop {
        value: if priority == 0 {
            "Add priority".into()
        } else {
            priority_name(priority).into()
        },
        empty: priority == 0,
        group: true,
        ..Stop::new("priority", "Priority", Choice)
    });
    stops.push(
        Stop::new("tags", "Tags", Text { multiline: false })
            .value(Some(snapshot.metadata.tags.join(", ")), "Add tags"),
    );
    stops.push(Stop {
        value: list_name.into(),
        ..Stop::new("move", "List", Action)
    });
    if !list {
        stops.push(Stop::new("blockers", "Blocked by", Action).value(None, "Add blocker"));
        stops.push(Stop::new("waiting", "Waiting on", Action).value(None, "Add waiting"));
    }
    stops.push(
        Stop::new("repeat", "Repeat", Text { multiline: false })
            .value(snapshot.metadata.recurrence_rule.clone(), "Add repeat")
            .grouped(),
    );
    if snapshot.daily_progress {
        stops.push(Stop {
            value: "Make daily progress".into(),
            ..Stop::new("daily", "Daily", Toggle)
        });
    }
    if !list {
        stops.push(Stop {
            value: "Commit periodically…".into(),
            empty: true,
            ..Stop::new("commit", "", Action)
        });
    }
    stops.push(
        Stop::new("links", "Links", Text { multiline: true })
            .value(
                Some(snapshot.metadata.external_links.join("\n")),
                "Add link",
            )
            .grouped(),
    );
    let mut actions = vec![Stop {
        value: "Copy requirements to subtasks".into(),
        group: true,
        ..Stop::new("apply-planning", "", Action)
    }];
    if list {
        actions.push(Stop {
            value: if task.is_promoted == Some(true) {
                "Unpin from sidebar".into()
            } else {
                "Pin to sidebar".into()
            },
            ..Stop::new("pin", "", Action)
        });
        actions.push(Stop {
            value: if task.status == "open" {
                "Complete list".into()
            } else {
                "Reopen list".into()
            },
            ..Stop::new("complete", "", Action)
        });
        actions.push(Stop {
            value: "Archive list".into(),
            ..Stop::new("archive", "", Action)
        });
    }
    actions.push(Stop {
        value: if list {
            "Convert to task".into()
        } else {
            "Convert to list".into()
        },
        ..Stop::new("convert", "", Action)
    });
    stops.extend(actions);
    stops
}

impl App {
    /// The selection's editor snapshot, read again only when the selection
    /// or the data has changed.
    pub fn inspected(&mut self) -> Option<EditorSnapshot> {
        let id = self.selected_task.clone()?;
        let key = (id.clone(), self.generation);
        if let Some((held, snapshot)) = &self.inspected
            && *held == key
        {
            return Some(snapshot.clone());
        }
        let snapshot = self.store.core.editor_snapshot(id).ok()?;
        self.inspected = Some((key, snapshot.clone()));
        Some(snapshot)
    }

    /// The inspector's stops for the selection.
    pub fn inspector_stops(&mut self) -> Vec<Stop> {
        let Some(snapshot) = self.inspected() else {
            return Vec::new();
        };
        let Some(task) = self.selected().cloned().or_else(|| {
            self.store
                .core
                .task(snapshot.task_id.clone())
                .ok()
                .flatten()
        }) else {
            return Vec::new();
        };
        let list = self
            .list(&task.list_id)
            .map(|list| list.name.clone())
            .unwrap_or_default();
        stops(&snapshot, &task, &list, self.focus_session.is_some())
    }

    pub fn inspector_stop(&mut self) -> Option<Stop> {
        let stops = self.inspector_stops();
        let ix = self.inspector_cursor.min(stops.len().saturating_sub(1));
        stops.into_iter().nth(ix)
    }

    pub fn step_inspector(&mut self, by: i64) {
        let count = self.inspector_stops().len() as i64;
        if count == 0 {
            return;
        }
        self.inspector_cursor = (self.inspector_cursor as i64 + by).clamp(0, count - 1) as usize;
    }

    pub fn inspector_to_end(&mut self, last: bool) {
        let count = self.inspector_stops().len();
        self.inspector_cursor = if last { count.saturating_sub(1) } else { 0 };
    }

    /// Return on a stop: a field opens, a switch flips, a choice steps, an
    /// action runs.
    pub fn activate_inspector(&mut self) {
        let Some(stop) = self.inspector_stop() else {
            return;
        };
        let Some(task_id) = self.selected_task.clone() else {
            return;
        };
        match stop.kind {
            StopKind::Text { .. } => {
                self.editing = Some(Editing::InspectorField {
                    task_id,
                    stop: stop.id,
                });
            }
            StopKind::Toggle => self.toggle_inspector_stop(stop.id),
            StopKind::Choice => self.step_inspector_choice(1),
            StopKind::Action => self.press_inspector_action(stop.id),
        }
    }

    /// `⌘⇧N`-style edits that land in the inspector: its dock opens on
    /// the field, with the caret in it.
    pub fn edit_in_inspector(&mut self, stop: &'static str) {
        let Some(task_id) = self.selected_task.clone() else {
            return;
        };
        self.prefs.right_dock_visible = true;
        self.prefs.right_dock_tab = "inspector".into();
        self.prefs.save();
        self.region = Region::Inspector;
        if let Some(ix) = self.inspector_stops().iter().position(|s| s.id == stop) {
            self.inspector_cursor = ix;
        }
        self.editing = Some(Editing::InspectorField { task_id, stop });
    }

    /// ← on a choice steps it back; anywhere else it leaves.
    pub fn inspector_left(&mut self, by_arrow: bool) {
        match self.inspector_stop() {
            Some(stop) if stop.kind == StopKind::Choice && by_arrow => {
                self.step_inspector_choice(-1)
            }
            _ => self.region = Region::Tasks,
        }
    }

    fn step_inspector_choice(&mut self, by: i64) {
        let Some(snapshot) = self.inspected() else {
            return;
        };
        let current = snapshot.metadata.priority.unwrap_or(0);
        let ix = PRIORITY_LEVELS
            .iter()
            .position(|level| *level == current)
            .unwrap_or(0) as i64;
        let next = PRIORITY_LEVELS[(ix + by).rem_euclid(PRIORITY_LEVELS.len() as i64) as usize];
        self.edit_selected(|edit| edit.metadata.priority = (next != 0).then_some(next));
    }

    fn toggle_inspector_stop(&mut self, id: &str) {
        match id {
            "due.exact" => self.edit_selected(|edit| {
                let mut planning = edit.planning.clone().unwrap_or_default();
                if let Some(at) = edit.due_at_ms.take() {
                    planning.due_date = Some(day_of(at));
                } else if let Some(day) = planning.due_date.take() {
                    edit.due_at_ms = day_noon_ms(&day);
                }
                edit.planning = planning.normalized();
            }),
            "single-sitting" => self.edit_selected(|edit| {
                let mut planning = edit.planning.clone().unwrap_or_default();
                planning.requires_single_sitting =
                    (planning.requires_single_sitting != Some(true)).then_some(true);
                edit.planning = planning.normalized();
            }),
            "daily" => self.edit_selected(|edit| edit.daily_progress = !edit.daily_progress),
            _ => {}
        }
    }

    fn press_inspector_action(&mut self, id: &str) {
        let Some(task) = self.selected().cloned().or_else(|| {
            let id = self.selected_task.clone()?;
            self.store.core.task(id).ok().flatten()
        }) else {
            return;
        };
        let now = now_ms();
        match id {
            "focus" => {
                if self.focus_session.is_some() {
                    self.add_selected_to_focus_queue();
                } else {
                    self.start_focus_on_selected(false);
                }
            }
            "conditions" => self.run("taskConditions", None),
            "move" => self.run("taskMove", None),
            "blockers" => self.run("taskAddBlocker", None),
            "waiting" => self.run("taskWaiting", None),
            "commit" => self.run("taskToggleDaily", None),
            "apply-planning" => {
                let zone = self.store.zone.clone();
                if self
                    .write(|store| {
                        store
                            .core
                            .apply_planning_to_descendants(task.id.clone(), now, zone)
                    })
                    .is_some()
                {
                    self.message = Some("Copied the requirements to the subtasks.".into());
                    self.reload_rows();
                }
            }
            "pin" => {
                let promoted = task.is_promoted != Some(true);
                if self
                    .write(|store| {
                        store
                            .core
                            .set_nested_list_promoted(task.id.clone(), promoted, now)
                    })
                    .is_some()
                {
                    self.reload_sidebar();
                    self.reload_rows();
                }
            }
            "complete" => self.toggle_complete(),
            "archive" => {
                if self
                    .write(|store| {
                        store
                            .core
                            .set_nested_list_archived(task.id.clone(), true, now)
                    })
                    .is_some()
                {
                    self.reload_sidebar();
                    self.reload_rows();
                }
            }
            "convert" => self.run("taskConvertToList", None),
            _ => {}
        }
    }

    /// `x` on a stop: empties a field, takes a date away, sets the
    /// priority back to none, turns a switch off. The title is left: a task
    /// needs one.
    pub fn clear_inspector_stop(&mut self) {
        let Some(stop) = self.inspector_stop() else {
            return;
        };
        match stop.id {
            "start" => self.schedule_selected(None),
            "due" => self.apply_typed("clear due"),
            "estimate" => self.edit_selected(|edit| edit.estimate_seconds = None),
            "minimum-block" => self.edit_planning(|planning| planning.minimum_block_seconds = None),
            "single-sitting" => {
                self.edit_planning(|planning| planning.requires_single_sitting = None)
            }
            "notes" => self.edit_selected(|edit| edit.notes.clear()),
            "priority" => self.edit_selected(|edit| edit.metadata.priority = None),
            "tags" => self.edit_selected(|edit| edit.metadata.tags.clear()),
            "repeat" => self.edit_selected(|edit| edit.metadata.recurrence_rule = None),
            "links" => self.edit_selected(|edit| edit.metadata.external_links.clear()),
            "daily" => self.edit_selected(|edit| edit.daily_progress = false),
            "conditions" => self.edit_planning(|planning| planning.requirement_groups = None),
            "due.exact" if self.inspected().is_some_and(|s| s.due_at_ms.is_some()) => {
                self.toggle_inspector_stop("due.exact");
            }
            _ => {}
        }
    }

    fn edit_planning(&mut self, change: impl FnOnce(&mut takt_core::editor::Planning)) {
        self.edit_selected(|edit| {
            let mut planning = edit.planning.clone().unwrap_or_default();
            change(&mut planning);
            edit.planning = planning.normalized();
        });
    }

    /// What a text stop's field opens with: the value as typed, not as
    /// shown.
    pub fn inspector_field_text(&mut self, stop: &str) -> String {
        let Some(snapshot) = self.inspected() else {
            return String::new();
        };
        let planning = snapshot.planning.clone().unwrap_or_default();
        match stop {
            "title" => snapshot.title,
            "notes" => snapshot.notes,
            "estimate" => snapshot
                .estimate_seconds
                .map(|s| editor_minutes_text(s, true))
                .unwrap_or_default(),
            "minimum-block" => planning
                .minimum_block_seconds
                .map(|s| editor_minutes_text(s, true))
                .unwrap_or_default(),
            "tags" => snapshot.metadata.tags.join(", "),
            "repeat" => snapshot.metadata.recurrence_rule.unwrap_or_default(),
            "links" => snapshot.metadata.external_links.join("\n"),
            "start" => planning.start_at_ms.map(format_moment).unwrap_or_default(),
            "due" => snapshot
                .due_at_ms
                .map(format_moment)
                .or_else(|| planning.due_date.as_deref().map(format_day_string))
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// The field closed with Return: save what was typed.
    pub fn commit_inspector_field(&mut self, stop: &str, text: &str) {
        let trimmed = text.trim();
        match stop {
            "title" => {
                if let Some(id) = self.selected_task.clone() {
                    self.rename_task(&id, trimmed);
                }
            }
            "notes" => {
                let notes = text.trim_end().to_string();
                self.edit_selected(|edit| edit.notes = notes);
            }
            "estimate" | "minimum-block" => match editor_parse_minutes(trimmed.into()) {
                Ok(seconds) => {
                    if stop == "estimate" {
                        self.edit_selected(|edit| edit.estimate_seconds = seconds)
                    } else {
                        self.edit_planning(|planning| planning.minimum_block_seconds = seconds)
                    }
                }
                Err(_) => {
                    self.message = Some(format!(
                        "\u{201c}{trimmed}\u{201d} isn't a number of minutes."
                    ))
                }
            },
            "tags" => {
                let tags: Vec<String> = trimmed
                    .split(',')
                    .map(|tag| tag.trim().trim_start_matches('#').to_string())
                    .filter(|tag| !tag.is_empty())
                    .collect();
                self.edit_selected(|edit| edit.metadata.tags = tags);
            }
            "repeat" => {
                let rule = (!trimmed.is_empty()).then(|| trimmed.to_string());
                self.edit_selected(|edit| edit.metadata.recurrence_rule = rule);
            }
            "links" => {
                let links: Vec<String> = text
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect();
                self.edit_selected(|edit| edit.metadata.external_links = links);
            }
            "start" => {
                if trimmed.is_empty() {
                    self.schedule_selected(None);
                } else {
                    self.apply_typed(&format!("start {trimmed}"));
                }
            }
            "due" => {
                if trimmed.is_empty() {
                    self.apply_typed("clear due");
                } else {
                    self.apply_typed(&format!("due {trimmed}"));
                }
            }
            _ => {}
        }
    }
}

/// `yyyy-MM-dd` of a moment, locally.
fn day_of(ms: i64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|time| time.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// Noon on a `yyyy-MM-dd` day, locally: a set time to start from.
fn day_noon_ms(day: &str) -> Option<i64> {
    use chrono::{Local, TimeZone};
    let date = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?;
    Local
        .from_local_datetime(&date.and_hms_opt(12, 0, 0)?)
        .earliest()
        .map(|time| time.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> EditorSnapshot {
        EditorSnapshot {
            workspace_id: "w".into(),
            task_id: "t".into(),
            title: "Write".into(),
            notes: String::new(),
            due_at_ms: None,
            estimate_seconds: Some(1800),
            metadata: Default::default(),
            daily_progress: false,
            planning: None,
        }
    }

    fn task(kind: Option<&str>) -> TaskRow {
        TaskRow {
            id: "t".into(),
            list_id: "l".into(),
            parent_task_id: None,
            title: "Write".into(),
            notes: String::new(),
            status: "open".into(),
            sort_order: 0,
            due_at_ms: None,
            estimate_seconds: None,
            source_system: None,
            source_id: None,
            item_kind: kind.map(str::to_string),
            is_promoted: None,
            archived_at_ms: None,
            completed_at_ms: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    #[test]
    fn a_task_reads_title_first_and_its_empty_fields_as_adds() {
        let stops = stops(&snapshot(), &task(None), "Inbox", false);
        assert_eq!(stops[0].id, "title");
        let estimate = stops.iter().find(|s| s.id == "estimate").unwrap();
        assert_eq!(estimate.value, "30 min");
        let due = stops.iter().find(|s| s.id == "due").unwrap();
        assert!(due.empty);
        assert!(!stops.iter().any(|s| s.id == "due.exact"));
        assert!(stops.iter().any(|s| s.id == "blockers"));
    }

    #[test]
    fn a_list_offers_pinning_and_no_focus() {
        let stops = stops(&snapshot(), &task(Some("list")), "Inbox", false);
        assert!(stops.iter().any(|s| s.id == "pin"));
        assert!(!stops.iter().any(|s| s.id == "focus" || s.id == "blockers"));
    }

    #[test]
    fn priorities_cycle_through_none_and_the_later_ones() {
        assert_eq!(priority_name(2), "High");
        assert_eq!(PRIORITY_LEVELS.len(), 9);
    }
}
