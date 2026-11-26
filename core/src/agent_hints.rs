//! The keys that would have done what the agent just did.
//!
//! Every change the agent panel's assistant makes is shown as a card, and the
//! card says how to make the same change from the keyboard: `task_update` with
//! a due date is `dd` then the date, a delete is `⌫`, a move is `mm`. The
//! hint is worked out here, from the tool's name and arguments, never written
//! by the model, and its keys are read from the commands passed in — the
//! catalogue with the user's keymap laid over it — so rebinding a command
//! moves its hint with it. A command with no key left is reached through the
//! palette instead; a tool nothing in the app does the same as has no hint.
//! See `docs/agent-panel.md`.

use serde_json::{Map, Value};

use crate::keymap::CommandEntry;

/// One piece of a hint: a key to press, or the words between keys.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum AgentKeyHintPart {
    /// A key in the catalogue's spelling (`dd`, `cmd+shift+m`, `space`).
    Key {
        key: String,
    },
    Text {
        text: String,
    },
}

/// The keystrokes for one tool call, in order.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AgentKeyHint {
    pub parts: Vec<AgentKeyHintPart>,
}

/// The write tools nothing in the desktop app does the same as from the
/// keyboard: the dailies live in their own list on the phones and the menu
/// bar, a blocker is removed from the inspector, and a filter list is
/// deleted from the sidebar's context menu.
pub const TOOLS_WITHOUT_KEYS: &[&str] = &[
    "daily_add",
    "daily_update",
    "daily_tick",
    "workspace_task_blocker_remove",
    "workspace_filter_list_delete",
];

/// The longest a typed value is quoted in a hint before it is cut short.
const QUOTE_LIMIT: usize = 32;

/// The keys that do what `tool` did with `input_json`, or none when nothing
/// does. `tool` may be qualified (`mcp__takt__task_update`); `commands` are
/// the commands with the keys in force, the Mac's `WorkspaceCommandCatalog`.
#[uniffi::export]
pub fn agent_key_hint(
    tool: String,
    input_json: String,
    commands: Vec<CommandEntry>,
) -> Option<AgentKeyHint> {
    let input: Value = serde_json::from_str(&input_json).unwrap_or(Value::Null);
    let empty = Map::new();
    let input = input.as_object().unwrap_or(&empty);
    let bare = tool.rsplit("__").next().unwrap_or(&tool);
    let mut hint = Hint::new(&commands);
    build(bare, input, &mut hint)?;
    hint.finish()
}

/// Whether `bare` (a write tool's unqualified name) can have a hint at all.
pub fn has_key_equivalent(bare: &str) -> bool {
    !TOOLS_WITHOUT_KEYS.contains(&bare)
}

fn build(tool: &str, input: &Map<String, Value>, hint: &mut Hint) -> Option<()> {
    match tool {
        "task_add" => {
            let child = number(input, "parent_task_id").is_some_and(|id| id != 0);
            hint.key(if child { "taskNewChild" } else { "taskNew" });
            hint.typed(text(input, "content"));
        }
        "task_update" => {
            if let Some(content) = text(input, "content") {
                hint.step("taskRename").typed(Some(content));
            }
            match text(input, "due") {
                Some(due) => hint.step("taskEditDue").typed(Some(due)),
                None if input.contains_key("due") => hint.step("taskClearDue"),
                None => hint,
            };
            match text(input, "tags") {
                Some(tags) => hint.step("taskEditTags").typed(Some(tags)),
                None if input.contains_key("tags") => hint.step("taskClearTags"),
                None => hint,
            };
        }
        "task_note_add" => {
            hint.key("taskEditNotes").typed(text(input, "note"));
        }
        "task_move" => {
            hint.either("taskMoveUp", "taskMoveDown");
        }
        "task_reparent" => {
            hint.key("taskMove").words("then choose where it goes");
        }
        "project_move" => {
            hint.key("taskMove").words("then choose the list");
        }
        "task_complete" | "task_reopen" => {
            hint.key("taskComplete");
        }
        "task_invalidate" => {
            hint.key("taskInvalidate");
        }
        "task_delete" | "workspace_task_delete" => {
            hint.key("taskDelete");
        }
        "list_create" | "workspace_list_create" => {
            hint.key("listNew").typed(text(input, "name"));
        }
        "task_matrix_set" => {
            let placements = input.get("placements")?.as_array()?;
            let [placement] = placements.as_slice() else {
                return None;
            };
            let urgency = placement.get("urgency")?.as_f64()?;
            let importance = placement.get("importance")?.as_f64()?;
            let quadrant = match (urgency > 0.0, importance > 0.0) {
                _ if urgency == 0.0 && importance == 0.0 => return None,
                (true, true) => 0,
                (false, true) => 1,
                (true, false) => 2,
                (false, false) => 3,
            };
            hint.key("goMatrix").words("then");
            hint.nth_key("planMatrixPlace", quadrant);
        }
        "workspace_task_add" => {
            let child = text(input, "parent_task_id").is_some();
            hint.key(if child { "taskNewChild" } else { "taskNew" });
            hint.typed(text(input, "title"));
            if input.get("kind").and_then(Value::as_str) == Some("list") {
                hint.step("taskConvertToList");
            }
            if let Some(notes) = text(input, "notes") {
                hint.step("taskEditNotes").typed(Some(notes));
            }
        }
        "workspace_task_update" => {
            if let Some(title) = text(input, "title") {
                hint.step("taskRename").typed(Some(title));
            }
            match text(input, "notes") {
                Some(notes) => hint.step("taskEditNotes").typed(Some(notes)),
                None if input.contains_key("notes") => hint.step("taskClearNotes"),
                None => hint,
            };
            match input.get("status").and_then(Value::as_str) {
                Some("completed") | Some("open") => hint.step("taskComplete"),
                Some("cancelled") => hint.step("taskInvalidate"),
                _ => hint,
            };
            if input.contains_key("kanban_column") {
                hint.step("goBoard").words("then");
                hint.either("planBoardMoveCardLeft", "planBoardMoveCardRight");
            }
            if input.contains_key("kind") {
                hint.step("taskConvertToList");
            }
            if input.contains_key("pinned") {
                hint.step("taskPromoteList");
            }
            if input.contains_key("waiting_on") || input.contains_key("follow_up_at") {
                hint.step("taskWaiting").typed(text(input, "waiting_on"));
            }
            match text(input, "due") {
                Some(due) => hint.step("taskEditDue").typed(Some(due)),
                None if input.contains_key("due") => hint.step("taskClearDue"),
                None => hint,
            };
            match text(input, "start") {
                Some(start) => hint.step("taskEditStart").typed(Some(start)),
                None if input.contains_key("start") => {
                    hint.step("taskEditStart").words("then clear it")
                }
                None => hint,
            };
            // The digit is the priority: `1` urgent to `4` low, `0` none,
            // `-` the back, and `-2` to `-4` just ahead of it.
            match number(input, "priority") {
                Some(level @ 1..=4) => hint.step_nth("motionSetPriority", level as usize - 1),
                Some(-1) => hint.step("taskSendToBack"),
                Some(level @ -4..=-2) => hint.step_nth("motionSetPriority", (3 - level) as usize),
                _ if input.contains_key("priority") => hint.step("taskClearPriority"),
                _ => hint,
            };
            match number(input, "estimate_minutes").filter(|minutes| *minutes > 0) {
                Some(minutes) => hint
                    .step("taskEditEstimate")
                    .typed(Some(minutes.to_string())),
                None if input.contains_key("estimate_minutes") => {
                    hint.step("taskEditEstimate").words("then clear it")
                }
                None => hint,
            };
        }
        "workspace_task_move" => {
            if text(input, "parent_task_id").is_some() {
                hint.key("taskMove").words("then choose the new parent");
            } else if text(input, "list_id").is_some() {
                hint.key("taskMove").words("then choose the list");
            } else {
                hint.either("taskMoveUp", "taskMoveDown");
            }
        }
        "workspace_task_to_list" => {
            hint.key("taskExtractBranch");
        }
        "workspace_task_blocker_add" => {
            hint.key("taskAddBlocker")
                .words("then find the task it waits on");
        }
        "workspace_task_commit" => {
            hint.key("taskToggleDaily").words("then choose the days");
        }
        "workspace_filter_list_create" => {
            hint.key("filterListNew");
        }
        "workspace_filter_list_update" => {
            hint.key("filterListEdit");
        }
        "workspace_folder_create" => {
            hint.key("folderNew").typed(text(input, "name"));
        }
        "workspace_list_move" => {
            hint.words("in the sidebar,");
            hint.key("taskMove").words("then choose the folder");
        }
        "workspace_list_delete" => {
            hint.key("listDelete");
        }
        "workspace_list_priority_set" => {
            hint.words("in the sidebar,");
            if input
                .get("position")
                .is_some_and(|position| !position.is_null())
            {
                hint.key("listPrioritise");
            } else {
                hint.key("listUnprioritise");
            }
        }
        "workspace_board_column_set" => {
            if text(input, "column_id").is_none() {
                hint.key("planBoardNewColumn").typed(text(input, "title"));
            } else {
                if let Some(title) = text(input, "title") {
                    hint.step("planBoardRenameColumn").typed(Some(title));
                }
                if input.contains_key("rules") || input.contains_key("match") {
                    hint.step("planBoardEditColumnRules");
                }
            }
        }
        "workspace_board_column_delete" => {
            hint.key("planBoardRemoveColumn");
        }
        "workspace_task_conditions_set" => {
            hint.key("taskConditions").words("then tick what it needs");
        }
        "workspace_list_conditions_set" => {
            hint.key("listSettings")
                .words("on the list, then tick what its tasks need");
        }
        "workspace_condition_device_set" => {
            hint.key("focusSetAvailable")
                .words("then set the device on the condition");
        }
        "workspace_condition_provides_set" => {
            hint.key("conditionsProvides")
                .words("on the condition, then tick what it provides");
        }
        "workspace_context_set" => {
            hint.key("focusSetAvailable")
                .words("then Space on each condition that holds");
        }
        "workspace_day_start_set" => {
            hint.key("focusSetAvailable")
                .words("then set it under Start each day");
        }
        "agent_instructions_set" => {
            if input.get("scope").and_then(Value::as_str) != Some("list") {
                return None;
            }
            hint.key("listSettings");
        }
        _ => return None,
    }
    Some(())
}

/// A string argument, trimmed, or none when it is missing or blank.
fn text(input: &Map<String, Value>, key: &str) -> Option<String> {
    let value = input.get(key)?.as_str()?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn number(input: &Map<String, Value>, key: &str) -> Option<i64> {
    input.get(key)?.as_i64()
}

/// Cut to [`QUOTE_LIMIT`] characters, with an ellipsis where it was cut, on
/// one line.
fn quoted(value: &str) -> String {
    let line = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut characters = line.chars();
    let mut cut: String = characters.by_ref().take(QUOTE_LIMIT).collect();
    if characters.next().is_some() {
        cut = cut.trim_end().to_string();
        cut.push('…');
    }
    format!("“{cut}”")
}

struct Hint<'a> {
    commands: &'a [CommandEntry],
    parts: Vec<AgentKeyHintPart>,
    /// A command the hint needed had no key and the palette had none either.
    unreachable: bool,
}

impl<'a> Hint<'a> {
    fn new(commands: &'a [CommandEntry]) -> Self {
        Hint {
            commands,
            parts: Vec::new(),
            unreachable: false,
        }
    }

    fn command(&self, id: &str) -> Option<&'a CommandEntry> {
        self.commands.iter().find(|entry| entry.id == id)
    }

    /// The command's first key in force, which the catalogue lists most
    /// memorable first (`dd` before `⌘D`). A command left with no key is
    /// reached through the palette, by its title.
    fn key(&mut self, id: &str) -> &mut Self {
        let Some(entry) = self.command(id) else {
            self.unreachable = true;
            return self;
        };
        if let Some(key) = entry.keys.first() {
            self.parts.push(AgentKeyHintPart::Key { key: key.clone() });
            return self;
        }
        match self
            .command("goCommandPalette")
            .and_then(|palette| palette.keys.first())
        {
            Some(palette) => {
                self.parts.push(AgentKeyHintPart::Key {
                    key: palette.clone(),
                });
                self.words(&format!("then “{}”", entry.title));
            }
            None => self.unreachable = true,
        }
        self
    }

    /// The command's `index`th key, for one that reads which key ran it (the
    /// matrix's ⌥1–⌥4), whose keys cannot be rebound.
    fn nth_key(&mut self, id: &str, index: usize) -> &mut Self {
        match self.command(id).and_then(|entry| entry.keys.get(index)) {
            Some(key) => self.parts.push(AgentKeyHintPart::Key { key: key.clone() }),
            None => self.unreachable = true,
        }
        self
    }

    /// A further change in the same call, after a separator.
    fn step(&mut self, id: &str) -> &mut Self {
        self.separate();
        self.key(id)
    }

    /// [`Self::step`] with the command's `index`th key, for the priority
    /// digits.
    fn step_nth(&mut self, id: &str, index: usize) -> &mut Self {
        self.separate();
        self.nth_key(id, index)
    }

    fn separate(&mut self) {
        if !self.parts.is_empty() {
            self.words("·");
        }
    }

    /// One of two keys, for a move whose direction the arguments do not say.
    fn either(&mut self, first: &str, second: &str) -> &mut Self {
        self.key(first).words("or").key(second)
    }

    fn words(&mut self, text: &str) -> &mut Self {
        self.parts.push(AgentKeyHintPart::Text {
            text: text.to_string(),
        });
        self
    }

    /// "then type “friday”", when there is something to type.
    fn typed(&mut self, value: Option<String>) -> &mut Self {
        if let Some(value) = value {
            self.words(&format!("then type {}", quoted(&value)));
        }
        self
    }

    fn finish(self) -> Option<AgentKeyHint> {
        let has_key = self
            .parts
            .iter()
            .any(|part| matches!(part, AgentKeyHintPart::Key { .. }));
        (!self.unreachable && has_key).then_some(AgentKeyHint { parts: self.parts })
    }
}

#[cfg(test)]
mod tests;
