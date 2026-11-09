//! The inspector: the selected task's fields as a column of stops a cursor
//! walks, each edited in place and saved through the core's editor as one
//! undo step. `WorkspaceInspectorViews.swift` and its `InspectorCursor`.
//!
//! A text stop opens a field (Return or Esc gives the caret back; in the
//! notes Esc or ⌘Return), a switch flips, a choice steps, and an action
//! runs. `x` clears what can be cleared.

use super::inspector_container::Subject;
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
    /// On the links row: each link as the core reads it, for its icon.
    pub links: Vec<takt_core::links::TaskLink>,
}

impl Stop {
    /// The command that does what this row does, for its key hint.
    pub fn command(&self) -> Option<&'static str> {
        Some(match self.id {
            "title" => "taskRenameInsert",
            "focus" => "taskStartFocus",
            "notes" => "taskEditNotes",
            "notes-editor" => "taskMaximiseNotes",
            "estimate" => "taskEditEstimate",
            "tags" => "taskEditTags",
            "conditions" => "taskConditions",
            "move" => "taskMove",
            "blockers" => "taskAddBlocker",
            "waiting" => "taskWaiting",
            "commit" => "taskToggleDaily",
            "links" => "taskAddLink",
            "complete" => "taskComplete",
            "convert" => "taskConvertToList",
            "pin" => "taskPromoteList",
            "assignee" => "taskAssign",
            _ => return None,
        })
    }

    /// What its icon says on hover: its label, or what the row is when it
    /// has none.
    pub fn tip(&self) -> Option<&'static str> {
        if !self.label.is_empty() {
            return Some(self.label);
        }
        Some(match self.id {
            "notes" => "Notes",
            "focus" => "Focus",
            "log" => "Log time",
            "commit" => "Commitment",
            "apply-planning" => "Requirements",
            "pin" => "Sidebar",
            "complete" => "Complete",
            "archive" => "Archive",
            "convert" => "Convert",
            _ => return None,
        })
    }

    pub(super) fn new(id: &'static str, label: &'static str, kind: StopKind) -> Self {
        Stop {
            id,
            label,
            value: String::new(),
            empty: false,
            kind,
            group: false,
            links: Vec::new(),
        }
    }

    /// The icon drawn in place of the label: what the row is, at a glance.
    pub fn icon(&self) -> Option<&'static str> {
        Some(match self.id {
            "focus" => "play",
            "log" => "clock",
            "start" => "calendar-clock",
            "due" => "calendar",
            "due.exact" => "clock",
            "conditions" => "sliders",
            "estimate" => "hourglass",
            "minimum-block" => "timer",
            "single-sitting" => "armchair",
            "notes" => "text",
            "notes-editor" => "file-text",
            "agent-context" => "sparkles",
            "list-context" => "sparkles",
            "priority" => "flag",
            "tags" => "tag",
            "move" | "draft-list" => "list",
            "blockers" => "ban",
            "waiting" => "user",
            "repeat" => "repeat",
            "daily" => "flame",
            "commit" => "target",
            "links" => "link",
            "apply-planning" => "copy",
            "pin" => "pin",
            "complete" => "circle-check",
            "archive" => "archive",
            "convert" => "arrow-left-right",
            "assignee" => "users",
            "authorship" => "user",
            _ => return None,
        })
    }

    pub(super) fn value(mut self, value: Option<String>, add: &str) -> Self {
        match value.filter(|value| !value.trim().is_empty()) {
            Some(value) => self.value = value,
            None => {
                self.value = add.into();
                self.empty = true;
            }
        }
        self
    }

    pub(super) fn grouped(mut self) -> Self {
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

/// The rows of the inspector's When group, in order.
fn when_stops(snapshot: &EditorSnapshot, planning: &takt_core::editor::Planning) -> Vec<Stop> {
    use StopKind::*;
    let estimate = snapshot.estimate_seconds.filter(|s| *s > 0);
    let minutes = |s: i64| format!("{} min", editor_minutes_text(s, true));
    let mut stops = vec![
        Stop::new("start", "Scheduled", Text { multiline: false })
            .value(
                planning
                    .start_at_ms
                    .map(|ms| format!("Scheduled {}", format_slot(ms, estimate))),
                "Schedule a time",
            )
            .grouped(),
        Stop::new("estimate", "Takes", Text { multiline: false }).value(
            estimate.map(|s| format!("Takes {}", minutes(s))),
            "Add estimate",
        ),
    ];
    // How the estimate may be split only means something once there is one.
    if estimate.is_some() || planning.minimum_block_seconds.is_some() {
        stops.push(
            Stop::new("minimum-block", "Min. block", Text { multiline: false }).value(
                planning
                    .minimum_block_seconds
                    .map(|s| format!("In blocks of {} or more", minutes(s))),
                "Add a minimum block",
            ),
        );
    }
    let single = planning.requires_single_sitting == Some(true);
    if estimate.is_some() || single {
        stops.push(Stop {
            value: if single {
                "Must finish in one go".into()
            } else {
                "Can be split".into()
            },
            empty: !single,
            ..Stop::new("single-sitting", "One sitting", Toggle)
        });
    }
    let due = snapshot
        .due_at_ms
        .map(format_when)
        .or_else(|| planning.due_date.as_deref().map(format_day_string));
    stops.push(
        Stop::new("due", "Due", Text { multiline: false })
            .value(due.map(|d| format!("Due {d}")), "Add due date"),
    );
    stops.push(
        Stop::new("repeat", "Repeat", Text { multiline: false }).value(
            snapshot
                .metadata
                .recurrence_rule
                .clone()
                .map(|rule| format!("Repeats {rule}")),
            "Add repeat",
        ),
    );
    stops
}

/// "Mon 3 Nov, 14:00": a day and its time, as the When rows write them.
pub fn format_when(ms: i64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|time| time.format("%a %-d %b, %H:%M").to_string())
        .unwrap_or_default()
}

/// "Mon 3 Nov, 14:00–14:45": a start and, with a length, its end.
pub fn format_slot(start_ms: i64, length_seconds: Option<i64>) -> String {
    use chrono::{Local, TimeZone};
    let from = format_when(start_ms);
    match length_seconds.and_then(|s| Local.timestamp_millis_opt(start_ms + s * 1000).single()) {
        Some(end) => format!("{from}\u{2013}{}", end.format("%H:%M")),
        None => from,
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
    work: takt_core::task_work::TaskWorkFacts,
) -> Vec<Stop> {
    use StopKind::*;
    let planning = snapshot.planning.clone().unwrap_or_default();
    let list = is_list(task);
    let mut stops = vec![Stop {
        value: snapshot.title.clone(),
        ..Stop::new("title", "", Text { multiline: false })
    }];

    // Progress: working on it, and how far it has got, before anything
    // about when it is due. A task is worked on many times before it is
    // finished, if it ever is.
    if !list {
        let action = if focus_running {
            "Add to the focus queue"
        } else {
            "Start focus"
        };
        stops.push(Stop {
            value: match progress_summary(&work, snapshot.estimate_seconds) {
                Some(progress) => format!("{action} \u{b7} {progress}"),
                None => action.into(),
            },
            ..Stop::new("focus", "", Action)
        });
        // Work done away from the timer counts as much as work timed.
        stops.push(Stop {
            value: "Log time worked".into(),
            empty: true,
            ..Stop::new("log", "", Text { multiline: false })
        });
    }
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

    // When, and for how long: one group, each row a short sentence that
    // starts with what it is. Scheduled is the slot it is to be done in
    // (its start, buried until then, and the event Google Calendar shows),
    // as long as it takes; the splitting rows say how that time may be cut;
    // then the deadline, and how it comes round. Each calendar row opens the
    // calendar, whose time is typed beside its days.
    stops.extend(when_stops(snapshot, &planning));

    // What it is about.
    stops.push(
        Stop::new("notes", "", Text { multiline: true })
            .value(Some(snapshot.notes.clone()), "Add notes")
            .grouped(),
    );

    // Where it is filed and how it ranks.
    stops.push(Stop {
        value: list_name.into(),
        group: true,
        ..Stop::new("move", "List", Action)
    });
    let priority = snapshot.metadata.priority.unwrap_or(0);
    stops.push(Stop {
        value: if priority == 0 {
            "Add priority".into()
        } else {
            priority_name(priority).into()
        },
        empty: priority == 0,
        ..Stop::new("priority", "Priority", Choice)
    });
    stops.push(
        Stop::new("tags", "Tags", Text { multiline: false })
            .value(Some(snapshot.metadata.tags.join(", ")), "Add tags"),
    );

    // What it waits on.
    stops.push(
        Stop::new("conditions", "Needs", Action)
            .value(
                planning.requirement_groups.as_ref().and_then(|groups| {
                    (!groups.is_empty()).then(|| format!("{} condition groups", groups.len()))
                }),
                "Add conditions",
            )
            .grouped(),
    );
    if !list {
        stops.push(Stop::new("blockers", "Blocked by", Action).value(None, "Add blocker"));
        stops.push(Stop::new("waiting", "Waiting on", Action).value(None, "Add waiting"));
    }

    let has_requirements = planning
        .requirement_groups
        .as_ref()
        .is_some_and(|groups| !groups.is_empty())
        || planning.minimum_block_seconds.is_some()
        || planning.requires_single_sitting == Some(true);
    let mut actions = Vec::new();
    // Only a task with subtasks has anyone to copy its requirements to.
    if work.subtasks > 0 && has_requirements {
        actions.push(Stop {
            value: "Copy requirements to subtasks".into(),
            ..Stop::new("apply-planning", "", Action)
        });
    }
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
    if let Some(first) = actions.first_mut() {
        first.group = true;
    }
    stops.extend(actions);
    stops
}

/// "1h 20m", "45m", "2h".
pub(crate) fn span(seconds: i64) -> String {
    let minutes = (seconds + 30) / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// A typed length of time in seconds: "25" and "25m" are minutes, "1h",
/// "1.5h" and "1h 20m" hours and minutes. Empty is zero; nonsense is none.
pub fn parse_span(text: &str) -> Option<i64> {
    let text = text.trim().to_lowercase().replace(' ', "");
    if text.is_empty() {
        return Some(0);
    }
    if let Ok(minutes) = text.parse::<f64>() {
        return (minutes >= 0.0).then(|| (minutes * 60.0).round() as i64);
    }
    let mut seconds = 0.0;
    let mut number = String::new();
    let mut any = false;
    for c in text.chars() {
        if c.is_ascii_digit() || c == '.' {
            number.push(c);
            continue;
        }
        let value: f64 = number.parse().ok()?;
        number.clear();
        seconds += value
            * match c {
                'h' => 3600.0,
                'm' => 60.0,
                _ => return None,
            };
        any = true;
    }
    if !number.is_empty() {
        // "1h20": the trailing number is minutes.
        seconds += number.parse::<f64>().ok()? * 60.0;
    }
    any.then(|| seconds.round() as i64)
}

/// "1h 20m of 2h logged · 2 of 5 subtasks", or nothing when it has neither.
fn progress_summary(
    work: &takt_core::task_work::TaskWorkFacts,
    estimate_seconds: Option<i64>,
) -> Option<String> {
    let mut parts = Vec::new();
    if work.logged_seconds > 0 {
        let logged = span(work.logged_seconds);
        parts.push(match estimate_seconds.filter(|s| *s > 0) {
            Some(estimate) => format!("{logged} of {} logged", span(estimate)),
            None => format!("{logged} logged"),
        });
    }
    if work.subtasks > 0 {
        parts.push(format!(
            "{} of {} subtasks",
            work.subtasks_done, work.subtasks
        ));
    }
    (!parts.is_empty()).then(|| parts.join(" \u{b7} "))
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

    /// While a new task or card is being typed, the inspector shows that
    /// task rather than the selection it was opened from: its title as
    /// typed so far and the list it will land in. None when no draft is
    /// open.
    pub fn draft_inspector_stops(&self, title: &str) -> Option<Vec<Stop>> {
        let list_id = match &self.editing {
            Some(Editing::NewTask { list_id, .. } | Editing::NewCard { list_id, .. }) => list_id,
            _ => return None,
        };
        let list = self.list(list_id).map(|list| list.name.clone());
        Some(vec![
            Stop::new("title", "", StopKind::Text { multiline: false })
                .value(Some(title.trim().to_string()), "New task"),
            Stop::new("draft-list", "List", StopKind::Action)
                .value(list, "")
                .grouped(),
        ])
    }

    /// The inspector's stops for the selection.
    pub fn inspector_stops(&mut self) -> Vec<Stop> {
        if let Some(subject) = self.inspected_container() {
            return self.container_stops(&subject);
        }
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
        let work = self
            .store
            .core
            .task_work_facts(task.id.clone())
            .unwrap_or_default();
        let mut stops = stops(&snapshot, &task, &list, self.focus_session.is_some(), work);
        // What the agent is told about the task, and about its list: each
        // its own field, apart from the notes, and none while AI is off.
        if !self.prefs.ai_off
            && let Some(at) = stops.iter().position(|s| s.id == "notes")
        {
            let core = &self.store.core;
            let own = core.task_agent_context(task.id.clone()).unwrap_or_default();
            let of_list = core
                .agent_instructions()
                .unwrap_or_default()
                .into_iter()
                .find(|i| {
                    i.kind == takt_core::agent_instructions::AgentInstructionKind::List
                        && i.id.as_deref() == Some(task.list_id.as_str())
                })
                .map(|i| i.text)
                .unwrap_or_default();
            stops.insert(
                at + 1,
                Stop::new(
                    "agent-context",
                    "Agent context",
                    StopKind::Text { multiline: true },
                )
                .value(Some(own), "Add agent context"),
            );
            stops.insert(
                at + 2,
                Stop::new(
                    "list-context",
                    "List agent context",
                    StopKind::Text { multiline: true },
                )
                .value(Some(of_list), &format!("Add agent context for {list}")),
            );
        }
        self.add_notes_editor_stop(&mut stops, &task.id);
        // The links sit beside the name, with the button that adds one.
        if let Some(stop) = stops.iter_mut().find(|s| s.id == "title") {
            stop.links = self
                .store
                .core
                .task_links(task.id.clone())
                .unwrap_or_default();
        }
        // In a shared list: who holds it, and who added and finished it.
        if let Some(sharing) = self.task_sharing(&task.id) {
            let at = stops
                .iter()
                .position(|s| s.id == "move")
                .map_or(stops.len(), |ix| ix + 1);
            let name = takt_core::assignment::shared_member_name;
            let mut assignee = Stop::new("assignee", "Assigned to", StopKind::Choice).value(
                sharing.assignee.clone().map(|m| capitalise(&name(m))),
                "Nobody: anyone in the list",
            );
            assignee.group = true;
            let mut said = Vec::new();
            if let Some(by) = sharing.created_by.clone() {
                said.push(format!("Added by {}", name(by)));
            }
            if let Some(by) = sharing.completed_by.clone() {
                said.push(format!("completed by {}", name(by)));
            }
            stops.insert(at, assignee);
            if !said.is_empty() {
                stops.insert(
                    at + 1,
                    Stop::new("authorship", "", StopKind::Action)
                        .value(Some(capitalise(&said.join(", "))), ""),
                );
            }
        }
        // The commitment it has, or carries, in place of the offer to make one.
        let commitment = self.commitment_of(&task.id);
        if let Some(stop) = stops.iter_mut().find(|s| s.id == "commit")
            && let Some(c) = commitment
        {
            stop.label = "Commitment";
            stop.empty = false;
            stop.value = format!("{} · {} · x stops it", c.title, c.rule);
        }
        stops
    }

    /// The commitment a task carries, or its first.
    pub fn commitment_of(&self, task_id: &str) -> Option<takt_core::commitments::Commitment> {
        let core = &self.store.core;
        core.commitment(task_id.into()).ok().flatten().or_else(|| {
            core.commitments(task_id.into())
                .ok()
                .and_then(|c| c.into_iter().next())
        })
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
        // A draft's inspector is a preview: the task has no row to edit yet.
        if matches!(
            self.editing,
            Some(Editing::NewTask { .. } | Editing::NewCard { .. })
        ) {
            return;
        }
        let Some(stop) = self.inspector_stop() else {
            return;
        };
        if let Some(subject) = self.inspected_container() {
            let (Subject::List(id) | Subject::Folder(id)) = &subject else {
                return;
            };
            match stop.kind {
                StopKind::Text { .. } => {
                    self.editing = Some(Editing::InspectorField {
                        task_id: id.clone(),
                        stop: stop.id,
                    });
                }
                StopKind::Choice => self.step_container_folder(&subject, 1),
                StopKind::Action => self.press_container_action(&subject, stop.id),
                StopKind::Toggle => {}
            }
            return;
        }
        let Some(task_id) = self.selected_task.clone() else {
            return;
        };
        match stop.id {
            "start" | "due" => return self.open_date_edit(stop.id),
            "repeat" => return self.open_repeat_edit(),
            _ => {}
        }
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
        self.inspecting_container = false;
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
            _ if self.inspecting_container => self.region = Region::Sidebar,
            _ => self.region = Region::Tasks,
        }
    }

    fn step_inspector_choice(&mut self, by: i64) {
        if let Some(subject) = self.inspected_container() {
            return self.step_container_folder(&subject, by);
        }
        if self
            .inspector_stop()
            .is_some_and(|stop| stop.id == "assignee")
        {
            return self.step_assignee(by);
        }
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
            "notes-editor" => self.open_notes_editor(),
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
        if let Some(subject) = self.inspected_container() {
            return self.clear_container_stop(&subject, stop.id);
        }
        match stop.id {
            "start" => self.schedule_selected(None),
            "due" => self.apply_typed("clear due"),
            "estimate" => self.edit_selected(|edit| edit.estimate_seconds = None),
            "minimum-block" => self.edit_planning(|planning| planning.minimum_block_seconds = None),
            "single-sitting" => {
                self.edit_planning(|planning| planning.requires_single_sitting = None)
            }
            "notes" => self.edit_selected(|edit| edit.notes.clear()),
            "agent-context" | "list-context" => self.set_agent_context(stop.id, ""),
            "commit" => {
                if let Some(id) = self.selected_task.clone()
                    && let Some(c) = self.commitment_of(&id)
                {
                    self.stop_commitment(&c.task_id);
                    self.message = Some(format!(
                        "Stopped \u{201c}{}\u{201d}; what was logged is kept.",
                        c.title
                    ));
                }
            }
            "priority" => self.edit_selected(|edit| edit.metadata.priority = None),
            "assignee" => self.assign_selected(None),
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
        if let Some(subject) = self.inspected_container() {
            return self.container_field_text(&subject, stop);
        }
        let Some(snapshot) = self.inspected() else {
            return String::new();
        };
        let planning = snapshot.planning.clone().unwrap_or_default();
        match stop {
            "title" => snapshot.title,
            "notes" => snapshot.notes,
            "agent-context" => self
                .store
                .core
                .task_agent_context(snapshot.task_id.clone())
                .unwrap_or_default(),
            "list-context" => self
                .selected()
                .map(|t| t.list_id.clone())
                .and_then(|list_id| {
                    self.store
                        .core
                        .agent_instructions()
                        .unwrap_or_default()
                        .into_iter()
                        .find(|i| {
                            i.kind == takt_core::agent_instructions::AgentInstructionKind::List
                                && i.id.as_deref() == Some(list_id.as_str())
                        })
                })
                .map(|i| i.text)
                .unwrap_or_default(),
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

    /// Records time worked on the selected task without a focus session.
    fn log_selected_work(&mut self, seconds: i64) {
        let Some(id) = self.selected_task.clone() else {
            return;
        };
        let zone = self.store.zone.clone();
        let now = now_ms();
        if self
            .write(|s| s.core.log_task_work(id, seconds, now, zone))
            .is_some()
        {
            self.message = Some(format!("Logged {}.", span(seconds)));
        }
    }

    /// Saves the task's agent context, or its list's, as one undo step.
    fn set_agent_context(&mut self, stop: &str, text: &str) {
        let Some(task) = self.selected().cloned() else {
            return;
        };
        let text = text.trim().to_string();
        let now = now_ms();
        let saved = if stop == "list-context" {
            let list = task.list_id.clone();
            self.write(|s| {
                s.core.set_agent_instructions(
                    takt_core::agent_instructions::AgentInstructionKind::List,
                    Some(list),
                    text,
                    false,
                    now,
                )
            })
        } else {
            let id = task.id.clone();
            self.write(|s| s.core.set_task_agent_context(id, text, false, now))
        };
        if saved.is_some() {
            self.message = Some("Agent context saved.".into());
        }
    }

    /// The field closed with Return: save what was typed.
    pub fn commit_inspector_field(&mut self, stop: &str, text: &str) {
        if let Some(subject) = self.inspected_container() {
            return self.commit_container_field(&subject, stop, text);
        }
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
            "agent-context" => self.set_agent_context(stop, text),
            "list-context" => self.set_agent_context(stop, text),
            "log" => match parse_span(trimmed) {
                Some(seconds) if seconds > 0 => self.log_selected_work(seconds),
                Some(_) => {}
                None => {
                    self.message = Some(format!(
                        "\u{201c}{trimmed}\u{201d} isn't a length of time: try 25, 25m or 1h 20m."
                    ))
                }
            },
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

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

impl App {
    /// The selected task's part in a shared list, when it is in one.
    pub fn task_sharing(&self, task_id: &str) -> Option<takt_core::assignment::TaskSharing> {
        self.store.core.task_sharing(task_id.into()).ok().flatten()
    }

    /// Assigns the selected task to a member of its shared list, or to
    /// nobody, as one undo step.
    pub fn assign_selected(&mut self, account_id: Option<String>) {
        let Some(id) = self.selected_task.clone() else {
            return;
        };
        if self.task_sharing(&id).is_none() {
            self.message = Some("Only a task in a shared list can be assigned.".into());
            return;
        }
        let now = now_ms();
        if self
            .write(|s| s.core.assign_task(id, account_id, now))
            .is_some()
        {
            self.reload_all();
        }
    }

    /// Steps who holds the selected task through nobody and each member.
    pub fn step_assignee(&mut self, by: i64) {
        let Some(id) = self.selected_task.clone() else {
            return;
        };
        let Some(sharing) = self.task_sharing(&id) else {
            self.message = Some("Only a task in a shared list can be assigned.".into());
            return;
        };
        let mut choices: Vec<Option<String>> = vec![None];
        choices.extend(sharing.members.iter().map(|m| Some(m.account_id.clone())));
        let current = sharing.assignee.map(|m| m.account_id);
        let ix = choices.iter().position(|c| *c == current).unwrap_or(0) as i64;
        let next = choices[(ix + by).rem_euclid(choices.len() as i64) as usize].clone();
        self.assign_selected(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // The title shows no key: `ei` begins with the inspector's own `e`.
    fn every_key_the_inspector_shows_runs_its_row_from_there() {
        let bindings = crate::keys::Bindings::shipped();
        let ids = [
            "focus",
            "notes",
            "notes-editor",
            "estimate",
            "tags",
            "conditions",
            "move",
            "blockers",
            "waiting",
            "commit",
            "links",
            "complete",
            "convert",
            "pin",
            "assignee",
        ];
        let mut wrong = Vec::new();
        for id in ids {
            let command = Stop::new(id, "", StopKind::Action).command().unwrap();
            let Some(key) = bindings
                .entry(command)
                .and_then(|e| e.keys.first())
                .cloned()
            else {
                continue;
            };
            let ran = bindings
                .command(&key, takt_core::keymap::CommandSurface::Inspector)
                .map(|c| c.id.clone());
            if ran.as_deref() != Some(command) {
                wrong.push(format!("{id}: {key} runs {ran:?}, not {command}"));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

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
        let stops = stops(&snapshot(), &task(None), "Inbox", false, Default::default());
        assert_eq!(stops[0].id, "title");
        let estimate = stops.iter().find(|s| s.id == "estimate").unwrap();
        assert_eq!(estimate.value, "Takes 30 min");
        let due = stops.iter().find(|s| s.id == "due").unwrap();
        assert!(due.empty);
        assert!(!stops.iter().any(|s| s.id == "due.exact"));
        assert!(stops.iter().any(|s| s.id == "blockers"));
    }

    #[test]
    fn spans_read_as_people_type_them() {
        assert_eq!(parse_span("25"), Some(1500));
        assert_eq!(parse_span("25m"), Some(1500));
        assert_eq!(parse_span("1h 20m"), Some(4800));
        assert_eq!(parse_span("1h20"), Some(4800));
        assert_eq!(parse_span("1.5h"), Some(5400));
        assert_eq!(parse_span(""), Some(0));
        assert_eq!(parse_span("soon"), None);
    }

    #[test]
    fn a_list_offers_pinning_and_no_focus() {
        let stops = stops(
            &snapshot(),
            &task(Some("list")),
            "Inbox",
            false,
            Default::default(),
        );
        assert!(stops.iter().any(|s| s.id == "pin"));
        assert!(!stops.iter().any(|s| s.id == "focus" || s.id == "blockers"));
    }

    #[test]
    fn progress_comes_before_dates_and_says_how_far_it_has_got() {
        use takt_core::task_work::TaskWorkFacts;
        let work = TaskWorkFacts {
            subtasks: 5,
            subtasks_done: 2,
            logged_seconds: 4800,
            work_blocks: 3,
        };
        let stops = stops(&snapshot(), &task(None), "Inbox", false, work);
        let order: Vec<&str> = stops.iter().map(|s| s.id).collect();
        let at = |id: &str| order.iter().position(|s| *s == id).unwrap();
        assert!(at("focus") < at("estimate") && at("estimate") < at("due"));
        assert!(at("due") < at("repeat") && at("repeat") < at("notes"));
        assert!(at("notes") < at("move") && at("move") < at("conditions"));
        assert_eq!(
            stops[at("focus")].value,
            "Start focus \u{b7} 1h 20m of 30m logged \u{b7} 2 of 5 subtasks"
        );
        assert!(
            !order.contains(&"apply-planning"),
            "nothing to copy without requirements"
        );
    }

    #[test]
    fn splitting_rows_wait_for_an_estimate() {
        let mut bare = snapshot();
        bare.estimate_seconds = None;
        let stops = stops(&bare, &task(None), "Inbox", false, Default::default());
        assert!(
            !stops
                .iter()
                .any(|s| s.id == "minimum-block" || s.id == "single-sitting")
        );
        assert_eq!(
            stops.iter().find(|s| s.id == "focus").unwrap().value,
            "Start focus"
        );
    }

    #[test]
    fn the_when_rows_are_one_group_of_sentences() {
        use chrono::{Local, TimeZone};
        let start = Local
            .with_ymd_and_hms(2026, 11, 9, 14, 0, 0)
            .unwrap()
            .timestamp_millis();
        let mut scheduled = snapshot();
        scheduled.planning = Some(takt_core::editor::Planning {
            start_at_ms: Some(start),
            due_date: Some("2026-11-13".into()),
            ..Default::default()
        });
        scheduled.metadata.recurrence_rule = Some("weekly".into());
        let stops = stops(&scheduled, &task(None), "Inbox", false, Default::default());
        let ids: Vec<&str> = stops.iter().map(|s| s.id).collect();
        let from = ids.iter().position(|id| *id == "start").unwrap();
        assert_eq!(
            &ids[from..from + 6],
            [
                "start",
                "estimate",
                "minimum-block",
                "single-sitting",
                "due",
                "repeat"
            ]
        );
        // One hairline, above the first of them.
        assert!(stops[from].group);
        assert!(stops[from + 1..from + 6].iter().all(|s| !s.group));
        let value = |id: &str| stops.iter().find(|s| s.id == id).unwrap().value.clone();
        assert_eq!(value("start"), "Scheduled Mon 9 Nov, 14:00\u{2013}14:30");
        assert_eq!(value("due"), "Due Fri 13 Nov");
        assert_eq!(value("repeat"), "Repeats weekly");
        assert_eq!(format_slot(start, None), "Mon 9 Nov, 14:00");
    }

    #[test]
    fn priorities_cycle_through_none_and_the_later_ones() {
        assert_eq!(priority_name(2), "High");
        assert_eq!(PRIORITY_LEVELS.len(), 9);
    }
}
