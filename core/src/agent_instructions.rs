//! The agent panel's standing instructions: text the user, or the agent with
//! the user's say-so, keeps for the whole account, a folder or a list.
//!
//! One row per scope in `agent_instructions` (`v25_agent_instructions`),
//! keyed `account`, `folder:<id>` or `list:<id>`. The key is the scope itself
//! rather than a random id, so two devices writing the account's
//! instructions write the same row and sync settles them like any other edit
//! instead of keeping two. The table is journalled, so a change is an undo
//! step ("MCP: Edit Agent Instructions" when the CLI makes it), and synced.
//!
//! Blank text is no row: clearing a scope deletes it. A row whose folder or
//! list has gone is left alone (it is one short string) and never read back.
//! See `docs/agent-panel.md`.

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::CoreError;
use crate::journal;
use crate::records::required_ms;
use crate::time::stored;
use crate::workspace::CoreWorkspace;

/// Which scope a piece of instructions is written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum AgentInstructionKind {
    Account,
    Folder,
    List,
}

impl AgentInstructionKind {
    /// `account`, `folder` or `list`: the word the CLI and the key use.
    pub fn word(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Folder => "folder",
            Self::List => "list",
        }
    }

    pub fn from_word(word: &str) -> Option<Self> {
        match word.trim().to_lowercase().as_str() {
            "account" => Some(Self::Account),
            "folder" => Some(Self::Folder),
            "list" => Some(Self::List),
            _ => None,
        }
    }
}

/// One scope's instructions as they are stored.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AgentInstruction {
    pub kind: AgentInstructionKind,
    /// The folder's or list's id; nil for the account.
    pub id: Option<String>,
    /// The folder's or list's name now; nil for the account.
    pub name: Option<String>,
    pub text: String,
    pub updated_at_ms: i64,
}

/// One entry to bring in from somewhere older, by [`import_agent_instructions`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AgentInstructionSeed {
    pub kind: AgentInstructionKind,
    pub id: Option<String>,
    pub text: String,
}

/// The row key for a scope. A folder or list needs its id.
pub fn scope_key(kind: AgentInstructionKind, id: Option<&str>) -> Result<String, CoreError> {
    let id = id.map(str::trim).filter(|id| !id.is_empty());
    match (kind, id) {
        (AgentInstructionKind::Account, _) => Ok("account".to_string()),
        (AgentInstructionKind::Folder, Some(id)) => Ok(format!("folder:{id}")),
        (AgentInstructionKind::List, Some(id)) => Ok(format!("list:{id}")),
        (AgentInstructionKind::Folder, None) => Err(CoreError::MissingFolder { id: String::new() }),
        (AgentInstructionKind::List, None) => Err(CoreError::MissingList { id: String::new() }),
    }
}

fn parse_key(key: &str) -> Option<(AgentInstructionKind, Option<String>)> {
    if key == "account" {
        return Some((AgentInstructionKind::Account, None));
    }
    let (word, id) = key.split_once(':')?;
    let kind = AgentInstructionKind::from_word(word)?;
    (kind != AgentInstructionKind::Account && !id.is_empty()).then(|| (kind, Some(id.to_string())))
}

/// The name of the folder or list a scope names, or an error when it does
/// not exist. The account has no name.
fn target_name(
    connection: &Connection,
    kind: AgentInstructionKind,
    id: Option<&str>,
) -> Result<Option<String>, CoreError> {
    let (table, missing): (&str, fn(String) -> CoreError) = match kind {
        AgentInstructionKind::Account => return Ok(None),
        AgentInstructionKind::Folder => ("list_folders", |id| CoreError::MissingFolder { id }),
        AgentInstructionKind::List => ("task_lists", |id| CoreError::MissingList { id }),
    };
    let id = id.map(str::trim).unwrap_or_default();
    connection
        .query_row(
            &format!("SELECT name FROM {table} WHERE id = ?1"),
            [id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(Some)
        .ok_or_else(|| missing(id.to_string()))
}

/// Every scope with instructions whose folder or list still exists: the
/// account first, then folders and lists, each by name.
pub fn agent_instructions(connection: &Connection) -> Result<Vec<AgentInstruction>, CoreError> {
    let mut statement =
        connection.prepare("SELECT scope, text, updatedAt FROM agent_instructions")?;
    let rows: Vec<(String, String, i64)> = statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, required_ms(row, "updatedAt")?))
        })?
        .collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for (key, text, updated_at_ms) in rows {
        let Some((kind, id)) = parse_key(&key) else {
            continue;
        };
        let name = match target_name(connection, kind, id.as_deref()) {
            Ok(name) => name,
            Err(CoreError::MissingFolder { .. } | CoreError::MissingList { .. }) => continue,
            Err(error) => return Err(error),
        };
        out.push(AgentInstruction {
            kind,
            id,
            name,
            text,
            updated_at_ms,
        });
    }
    let rank = |kind: AgentInstructionKind| match kind {
        AgentInstructionKind::Account => 0,
        AgentInstructionKind::Folder => 1,
        AgentInstructionKind::List => 2,
    };
    out.sort_by(|a, b| {
        (
            rank(a.kind),
            a.name.as_deref().map(str::to_lowercase),
            &a.id,
        )
            .cmp(&(
                rank(b.kind),
                b.name.as_deref().map(str::to_lowercase),
                &b.id,
            ))
    });
    Ok(out)
}

/// One scope's instructions, empty when it has none. A folder or list that
/// does not exist is an error, so a mistyped id is not read as "nothing
/// written".
pub fn agent_instruction(
    connection: &Connection,
    kind: AgentInstructionKind,
    id: Option<&str>,
) -> Result<AgentInstruction, CoreError> {
    let key = scope_key(kind, id)?;
    let name = target_name(connection, kind, id)?;
    let row: Option<(String, i64)> = connection
        .query_row(
            "SELECT text, updatedAt FROM agent_instructions WHERE scope = ?1",
            [&key],
            |row| Ok((row.get(0)?, required_ms(row, "updatedAt")?)),
        )
        .optional()?;
    let (text, updated_at_ms) = row.unwrap_or_default();
    Ok(AgentInstruction {
        kind,
        id: parse_key(&key).and_then(|(_, id)| id),
        name,
        text,
        updated_at_ms,
    })
}

/// Replaces a scope's instructions, or with `append` adds `text` as a new
/// paragraph after what is there. Surrounding whitespace is trimmed, and
/// blank text clears the scope. Returns the text the scope holds now. Writes
/// nothing when that is what it held already, so saving an untouched field is
/// not an undo step.
pub fn set_agent_instructions(
    transaction: &Transaction,
    kind: AgentInstructionKind,
    id: Option<&str>,
    text: &str,
    append: bool,
    now_ms: i64,
) -> Result<String, CoreError> {
    let current = agent_instruction(transaction, kind, id)?;
    let key = scope_key(kind, id)?;
    let addition = text.trim();
    let next = if append && !current.text.is_empty() {
        if addition.is_empty() {
            current.text.clone()
        } else {
            format!("{}\n\n{addition}", current.text)
        }
    } else {
        addition.to_string()
    };
    if next == current.text {
        return Ok(next);
    }
    if next.is_empty() {
        transaction.execute("DELETE FROM agent_instructions WHERE scope = ?1", [&key])?;
    } else {
        // An UPDATE when the row is there, rather than INSERT OR REPLACE,
        // which the journal and the outbox would record as a delete and an
        // insert.
        let updated = transaction.execute(
            "UPDATE agent_instructions SET text = ?1, updatedAt = ?2 WHERE scope = ?3",
            params![next, stored(now_ms), key],
        )?;
        if updated == 0 {
            transaction.execute(
                "INSERT INTO agent_instructions (scope, text, updatedAt) VALUES (?1, ?2, ?3)",
                params![key, next, stored(now_ms)],
            )?;
        }
    }
    Ok(next)
}

/// What the agent should know about one task (`task_metadata.agentContext`),
/// empty when nothing is written. A task that does not exist is an error.
pub fn task_agent_context(connection: &Connection, task_id: &str) -> Result<String, CoreError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE id = ?1)",
        [task_id],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(CoreError::MissingTask {
            id: task_id.to_string(),
        });
    }
    let text: Option<String> = connection
        .query_row(
            "SELECT agentContext FROM task_metadata WHERE taskId = ?1",
            [task_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(text.unwrap_or_default())
}

/// Replaces a task's agent context, or with `append` adds a paragraph, as
/// [`set_agent_instructions`] does a scope's; blank text clears it. Returns
/// the text it holds now, and writes nothing when that is unchanged.
pub fn set_task_agent_context(
    transaction: &Transaction,
    task_id: &str,
    text: &str,
    append: bool,
    now_ms: i64,
) -> Result<String, CoreError> {
    let current = task_agent_context(transaction, task_id)?;
    let addition = text.trim();
    let next = if append && !current.is_empty() {
        if addition.is_empty() {
            current.clone()
        } else {
            format!("{current}\n\n{addition}")
        }
    } else {
        addition.to_string()
    };
    if next == current {
        return Ok(next);
    }
    let value = (!next.is_empty()).then_some(next.as_str());
    let updated = transaction.execute(
        "UPDATE task_metadata SET agentContext = ?1, updatedAt = ?2 WHERE taskId = ?3",
        params![value, stored(now_ms), task_id],
    )?;
    if updated == 0 {
        transaction.execute(
            "INSERT INTO task_metadata (taskId, agentContext, updatedAt) VALUES (?1, ?2, ?3)",
            params![task_id, value, stored(now_ms)],
        )?;
    }
    Ok(next)
}

/// Brings in instructions kept somewhere older (the Mac's preferences) where
/// the table has none for that scope yet, outside the journal: it is a move,
/// not an edit to take back. Blank text, and an entry for a folder or list
/// that is gone, are skipped. Returns how many it wrote.
pub fn import_agent_instructions(
    transaction: &Transaction,
    seeds: &[AgentInstructionSeed],
    now_ms: i64,
) -> Result<u32, CoreError> {
    let mut written = 0;
    for seed in seeds {
        let text = seed.text.trim();
        if text.is_empty() {
            continue;
        }
        match target_name(transaction, seed.kind, seed.id.as_deref()) {
            Ok(_) => {}
            Err(CoreError::MissingFolder { .. } | CoreError::MissingList { .. }) => continue,
            Err(error) => return Err(error),
        }
        let key = scope_key(seed.kind, seed.id.as_deref())?;
        written += transaction.execute(
            "INSERT OR IGNORE INTO agent_instructions (scope, text, updatedAt) VALUES (?1, ?2, ?3)",
            params![key, text, stored(now_ms)],
        )? as u32;
    }
    Ok(written)
}

// MARK: - The handle

#[uniffi::export]
impl CoreWorkspace {
    /// Every scope with instructions; see [`agent_instructions`].
    pub fn agent_instructions(&self) -> Result<Vec<AgentInstruction>, CoreError> {
        agent_instructions(&self.lock())
    }

    /// Sets a scope's instructions as one "Edit Agent Instructions" step and
    /// returns the text it holds now.
    pub fn set_agent_instructions(
        &self,
        kind: AgentInstructionKind,
        id: Option<String>,
        text: String,
        append: bool,
        now_ms: i64,
    ) -> Result<String, CoreError> {
        journal::journalled(&mut self.lock(), "Edit Agent Instructions", |tx| {
            set_agent_instructions(tx, kind, id.as_deref(), &text, append, now_ms)
        })
    }

    /// A task's agent context; see [`task_agent_context`].
    pub fn task_agent_context(&self, task_id: String) -> Result<String, CoreError> {
        task_agent_context(&self.lock(), &task_id)
    }

    /// Sets a task's agent context as one "Edit Agent Context" step and
    /// returns the text it holds now.
    pub fn set_task_agent_context(
        &self,
        task_id: String,
        text: String,
        append: bool,
        now_ms: i64,
    ) -> Result<String, CoreError> {
        journal::journalled(&mut self.lock(), "Edit Agent Context", |tx| {
            set_task_agent_context(tx, &task_id, &text, append, now_ms)
        })
    }

    /// Brings in older instructions, outside the journal; see
    /// [`import_agent_instructions`].
    pub fn import_agent_instructions(
        &self,
        seeds: Vec<AgentInstructionSeed>,
        now_ms: i64,
    ) -> Result<u32, CoreError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let written = import_agent_instructions(&transaction, &seeds, now_ms)?;
        transaction.commit()?;
        Ok(written)
    }
}

#[cfg(test)]
mod tests;
