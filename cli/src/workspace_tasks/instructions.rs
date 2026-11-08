//! The agent panel's standing instructions: read, and replaced or added to.
//!
//! The table, its scopes and every write are the Rust core's
//! (`core/src/agent_instructions.rs`), the same functions the Mac's settings
//! fields call. This adds the CLI's JSON and its refusals. A write is an
//! "MCP: Edit Agent Instructions" step in the app's Undo menu, like any other.

use super::{has_migration, map_core_error};
use crate::error::{Result, ToolError};
use crate::workspace::Workspace;
use chrono::Utc;
use rusqlite::Connection;
use serde_json::{Value, json};
use takt_core::agent_instructions::{self as core, AgentInstruction, AgentInstructionKind};

/// What adds `agent_instructions`. Before it, both tools refuse.
const INSTRUCTIONS_MIGRATION: &str = "v25_agent_instructions";

/// "account", "folder" or "list" and the id a folder or list needs.
pub fn instruction_scope(
    scope: Option<&str>,
    id: Option<&str>,
) -> Result<Option<(AgentInstructionKind, Option<String>)>> {
    let id = id.map(str::trim).filter(|id| !id.is_empty());
    let Some(scope) = scope.map(str::trim).filter(|scope| !scope.is_empty()) else {
        return match id {
            None => Ok(None),
            Some(_) => Err(ToolError::new("Say which scope the id is: folder or list.")),
        };
    };
    let kind = AgentInstructionKind::from_word(&scope.to_lowercase()).ok_or_else(|| {
        ToolError::new(format!(
            "Unknown scope {scope}. Use account, folder or list."
        ))
    })?;
    match (kind, id) {
        (AgentInstructionKind::Account, None) => Ok(Some((kind, None))),
        (AgentInstructionKind::Account, Some(_)) => {
            Err(ToolError::new("The account scope takes no id."))
        }
        (_, None) => Err(ToolError::new(format!(
            "The {} scope needs an id from workspace_tree.",
            kind.word()
        ))),
        (_, Some(id)) => Ok(Some((kind, Some(id.to_string())))),
    }
}

fn require_table(connection: &Connection) -> Result<()> {
    if has_migration(connection, INSTRUCTIONS_MIGRATION)? {
        Ok(())
    } else {
        Err(ToolError::new(format!(
            "This workspace predates stored agent instructions ({INSTRUCTIONS_MIGRATION}). \
             Open an up-to-date Takt once, then try again."
        )))
    }
}

fn instruction_json(instruction: &AgentInstruction) -> Value {
    json!({
        "scope": instruction.kind.word(),
        "id": instruction.id,
        "name": instruction.name,
        "text": instruction.text,
        "updated_at": (instruction.updated_at_ms > 0).then(|| {
            chrono::DateTime::from_timestamp_millis(instruction.updated_at_ms)
                .map(|instant| instant.to_rfc3339())
        }).flatten(),
    })
}

impl Workspace {
    /// One scope's instructions, or every scope that has any.
    pub fn agent_instructions(
        &self,
        scope: Option<(AgentInstructionKind, Option<String>)>,
    ) -> Result<Value> {
        let connection = self.open()?;
        require_table(&connection)?;
        match scope {
            Some((kind, id)) => {
                let instruction = core::agent_instruction(&connection, kind, id.as_deref())
                    .map_err(map_core_error)?;
                Ok(json!({ "instructions": [instruction_json(&instruction)] }))
            }
            None => {
                let all = core::agent_instructions(&connection).map_err(map_core_error)?;
                Ok(json!({
                    "instructions": all.iter().map(instruction_json).collect::<Vec<_>>(),
                }))
            }
        }
    }

    /// Replaces a scope's instructions, or with `append` adds a paragraph,
    /// as one undo step. Blank text with replace clears the scope.
    pub fn set_agent_instructions(
        &self,
        kind: AgentInstructionKind,
        id: Option<&str>,
        text: &str,
        append: bool,
    ) -> Result<Value> {
        self.journalled("Edit Agent Instructions", |tx, _now| {
            require_table(tx)?;
            let before = core::agent_instruction(tx, kind, id).map_err(map_core_error)?;
            core::set_agent_instructions(tx, kind, id, text, append, Utc::now().timestamp_millis())
                .map_err(map_core_error)?;
            let after = core::agent_instruction(tx, kind, id).map_err(map_core_error)?;
            let mut payload = instruction_json(&after);
            payload["previous"] = json!(before.text);
            payload["changed"] = json!(before.text != after.text);
            Ok(payload)
        })
    }

    /// What the agent should know about one task, kept apart from its notes
    /// (`task_metadata.agentContext`, v36).
    pub fn task_agent_context(&self, task_id: &str) -> Result<Value> {
        let connection = self.open()?;
        require_context(&connection)?;
        let text = core::task_agent_context(&connection, task_id).map_err(map_core_error)?;
        Ok(json!({ "scope": "task", "id": task_id, "text": text }))
    }

    /// Replaces a task's agent context, or with `append` adds a paragraph,
    /// as one undo step. Blank text with replace clears it.
    pub fn set_task_agent_context(&self, task_id: &str, text: &str, append: bool) -> Result<Value> {
        self.journalled("Edit Agent Context", |tx, _now| {
            require_context(tx)?;
            let before = core::task_agent_context(tx, task_id).map_err(map_core_error)?;
            let after = core::set_task_agent_context(
                tx,
                task_id,
                text,
                append,
                Utc::now().timestamp_millis(),
            )
            .map_err(map_core_error)?;
            Ok(json!({
                "scope": "task",
                "id": task_id,
                "text": after,
                "previous": before,
                "changed": before != after,
            }))
        })
    }
}

/// What adds a task's agent context. Before it, the task scope refuses.
const CONTEXT_MIGRATION: &str = "v36_task_agent_context";

fn require_context(connection: &Connection) -> Result<()> {
    if has_migration(connection, CONTEXT_MIGRATION)? {
        Ok(())
    } else {
        Err(ToolError::new(format!(
            "This workspace predates a task's agent context ({CONTEXT_MIGRATION}). \
             Open an up-to-date Takt once, then try again."
        )))
    }
}
