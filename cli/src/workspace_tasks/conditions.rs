//! Conditions: the places and circumstances a task can require, what each
//! is holding back, and the context — what holds now.
//!
//! Every rule is the Rust core's (`core/src/conditions.rs`,
//! `core/src/focus_context.rs`), the same functions the apps call: the
//! order the tab shows, the open-task counts, a task's requirements and
//! their checks, one place at a time, the stamp that makes a context stale.
//! This adds the CLI's JSON, names for ids, and its refusals. Requiring a
//! condition is an "MCP: Edit Conditions" undo step, and setting what a
//! condition provides an "MCP: Edit Condition" one; saying what holds now
//! is not an undo step at all, as it is not in the app. What holds is the
//! core's too (`core/src/conditions/provides.rs`): what was said, everything
//! that provides, less what was switched off for now. How each day starts —
//! the hour it changes and the conditions it starts with — is the core's
//! too (`core/src/day_start.rs`): reading or setting what holds starts the
//! day first if it has changed, as opening the app does.

use super::{TaskRow, has_migration, map_core_error, map_write_error, task_row};
use crate::error::{Result, ToolError};
use crate::workspace::{Workspace, local_string, map_query_error};
use chrono::Utc;
use rusqlite::{Connection, TransactionBehavior};
use serde_json::{Value, json};
use std::collections::HashMap;
use takt_core::conditions as core;
use takt_core::day_start::{self, DayStartSettings};
use takt_core::focus_context::{self, StoredFocusContext};
use takt_core::rows::ConditionRow;

/// What adds `focus_contexts` and the conditions' order.
const CONTEXT_MIGRATION: &str = "v27_condition_order_and_context";

/// What lets a condition provide others.
const PROVIDES_MIGRATION: &str = "v34_condition_provides";

/// What adds device conditions and lists that require conditions.
const LIST_CONDITIONS_MIGRATION: &str = "v41_device_and_list_conditions";

/// A new context-set request: every field optional, applied in this order —
/// `set` (or `clear`) replaces the conditions on, then `off` and `on`.
#[derive(Debug, Default)]
pub struct ContextEdit {
    pub set: Option<Vec<String>>,
    pub on: Vec<String>,
    pub off: Vec<String>,
    pub clear: bool,
    pub mode: Option<String>,
    /// Minutes from now the time available ends; 0 for no limit.
    pub available_minutes: Option<i64>,
    /// Minutes from now the conditions stop being trusted; 0 for never.
    pub expires_in_minutes: Option<i64>,
}

/// A new day-start request: each field left out keeps what it was.
#[derive(Debug, Default)]
pub struct DayStartEdit {
    pub resets: Option<bool>,
    pub hour: Option<i64>,
    /// The conditions each day starts with, by name or id; replaces them.
    pub conditions: Option<Vec<String>>,
}

/// This machine's zone, as the app reads the day in.
fn zone() -> String {
    iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into())
}

/// How each day starts, in JSON.
fn day_start_json(settings: &DayStartSettings, all: &[ConditionRow]) -> Value {
    json!({
        "resets": settings.resets,
        "hour": settings.hour,
        "starts_with": names_of(all, &settings.condition_ids),
        "last_reset_day": settings.last_reset_day,
    })
}

fn conditions_of(connection: &Connection, workspace_id: &str) -> Result<Vec<ConditionRow>> {
    takt_core::rows::conditions(connection, workspace_id).map_err(map_core_error)
}

/// A condition by id, or by name however it is cased — a live one before an
/// archived one of the same name.
fn resolve<'a>(all: &'a [ConditionRow], wanted: &str) -> Result<&'a ConditionRow> {
    let wanted = wanted.trim();
    if let Some(found) = all.iter().find(|c| c.id == wanted) {
        return Ok(found);
    }
    let named = |archived: bool| {
        all.iter()
            .find(|c| c.is_archived == archived && c.name.trim().eq_ignore_ascii_case(wanted))
    };
    named(false).or_else(|| named(true)).ok_or_else(|| {
        let known: Vec<&str> = all
            .iter()
            .filter(|c| !c.is_archived)
            .map(|c| c.name.as_str())
            .collect();
        ToolError::new(format!(
            "No condition called \"{wanted}\". Known: {}.",
            if known.is_empty() {
                "none yet".to_string()
            } else {
                known.join(", ")
            }
        ))
    })
}

/// A requirement as typed, read as the condition it names and whether it
/// asks for that condition not to hold: "!Home", "!id" or "not Home". A
/// condition whose own name starts "not " is that condition, not a negation.
fn negation<'a>(all: &[ConditionRow], wanted: &'a str) -> (&'a str, bool) {
    let trimmed = wanted.trim();
    if let Some(rest) = trimmed.strip_prefix(core::NEGATION_PREFIX) {
        return (rest.trim(), true);
    }
    let named_so = resolve(all, trimmed).is_ok();
    match trimmed.get(..4) {
        Some(word) if !named_so && word.eq_ignore_ascii_case("not ") => (trimmed[4..].trim(), true),
        _ => (trimmed, false),
    }
}

/// Every condition's name by id, archived ones included.
pub(super) fn condition_names(connection: &Connection) -> Result<HashMap<String, String>> {
    connection
        .prepare("SELECT id, name FROM task_conditions")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect()
        })
        .map_err(map_query_error)
}

/// Requirement groups with condition names for ids, a condition required
/// not to hold as "not Home".
pub(super) fn named_groups(
    names: &HashMap<String, String>,
    groups: Vec<Vec<String>>,
) -> Vec<Vec<String>> {
    groups
        .into_iter()
        .map(|group| {
            group
                .into_iter()
                .map(|member| {
                    let requirement = core::Requirement::parse(&member);
                    match names.get(requirement.condition_id) {
                        Some(name) if requirement.negated => format!("not {name}"),
                        Some(name) => name.clone(),
                        None => member.clone(),
                    }
                })
                .collect()
        })
        .collect()
}

/// Fills in each row's `requires` with condition names, and its
/// `requires_from_list` with what the lists it is in require of it, list
/// by list from the outermost.
pub(super) fn name_requirements(connection: &Connection, rows: &mut [TaskRow]) -> Result<()> {
    let inheriting = core::inherited_groups(connection).map_err(map_core_error)?;
    if rows.iter().all(|row| row.planning_json.is_none()) && inheriting.is_empty() {
        return Ok(());
    }
    let names = condition_names(connection)?;
    for row in rows {
        if inheriting.contains_key(&row.id) {
            row.requires_from_list = core::inherited_requirements(connection, &row.id)
                .map_err(map_core_error)?
                .into_iter()
                .map(|inherited| {
                    json!({
                        "list_id": inherited.list_id,
                        "list": inherited.list_name,
                        "nested": inherited.is_nested,
                        "requires": named_groups(&names, inherited.groups),
                    })
                })
                .collect();
        }
        let Some(planning) = row
            .planning_json
            .as_deref()
            .and_then(|json| takt_core::planning::Planning::from_json(json).ok())
        else {
            continue;
        };
        row.requires = named_groups(&names, planning.requirement_groups.unwrap_or_default());
    }
    Ok(())
}

fn names_of(all: &[ConditionRow], ids: &[String]) -> Vec<Value> {
    // In the tab's order, as the app's status bar names them.
    let order: Vec<&ConditionRow> = core::condition_sections(all.to_vec())
        .iter()
        .flat_map(|section| section.conditions.clone())
        .filter_map(|c| all.iter().find(|row| row.id == c.id))
        .collect();
    order
        .into_iter()
        .filter(|c| ids.contains(&c.id))
        .map(|c| json!({ "id": c.id, "name": c.name, "place": c.is_location }))
        .collect()
}

fn local_ms(ms: Option<i64>) -> Option<String> {
    ms.map(takt_core::time::stored)
        .and_then(|stored| local_string(&Some(stored)))
}

/// The context as the app would show it now, in JSON.
fn context_json(
    connection: &Connection,
    workspace_id: &str,
    all: &[ConditionRow],
) -> Result<Value> {
    if !has_migration(connection, CONTEXT_MIGRATION)? {
        return Ok(json!({
            "note": "This workspace keeps the context in the app's preferences. Open an up-to-date Takt once to move it into the workspace."
        }));
    }
    let now = Utc::now().timestamp_millis();
    let stored = focus_context::load(connection, workspace_id)
        .map_err(map_core_error)?
        .unwrap_or_default();
    let expired = stored.expires_at_ms.is_some_and(|expiry| expiry <= now);
    let context = focus_context::focus_context_in_force(stored, now);
    let stale = focus_context::focus_context_is_stale(context.clone(), now, zone());
    let day = day_start::settings(connection).map_err(map_core_error)?;
    let name = |id: &str| {
        all.iter()
            .find(|c| c.id == id)
            .map_or_else(|| id.to_string(), |c| c.name.clone())
    };
    let holdings = core::holdings(all, &context.condition_ids, &context.off_ids);
    let holds: Vec<Value> = holdings
        .iter()
        .filter(|holding| holding.holds)
        .map(|holding| {
            json!({
                "id": holding.condition_id,
                "name": name(&holding.condition_id),
                "said": holding.is_set,
                // A device condition holds because this is that device.
                "from": holding
                    .is_device
                    .then(|| "device".to_string())
                    .into_iter()
                    .chain(holding.provided_by.iter().map(|id| name(id)))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    let off: Vec<Value> = holdings
        .iter()
        .filter(|holding| holding.is_off)
        .map(|holding| {
            json!({
                "id": holding.condition_id,
                "name": name(&holding.condition_id),
                "from": holding.provided_by.iter().map(|id| name(id)).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({
        "on": names_of(all, &context.condition_ids),
        "holds": holds,
        "off_for_now": off,
        "anywhere": holdings.iter().all(|holding| !holding.holds),
        "set_at": local_ms(context.conditions_set_at_ms),
        "stale": stale,
        "available_until": local_ms(context.available_until_ms),
        "expires_at": local_ms(context.expires_at_ms),
        "expired": expired,
        "mode": context.mode,
        "last_context": names_of(all, &context.suggested_ids),
        "day_start": day_start_json(&day, all),
    }))
}

/// Requirement groups as typed — names or ids, "!Home" or "not Home" for a
/// condition that must not hold — as stored members; with `create_missing`
/// a name nothing has yet becomes a new condition first.
fn resolve_groups(
    tx: &rusqlite::Transaction,
    workspace_id: &str,
    groups: &[Vec<String>],
    create_missing: bool,
    now: i64,
) -> Result<Vec<Vec<String>>> {
    let mut ids: Vec<Vec<String>> = Vec::new();
    for group in groups {
        let mut resolved = Vec::new();
        for wanted in group {
            let all = conditions_of(tx, workspace_id)?;
            let (wanted, negated) = negation(&all, wanted);
            let id = match resolve(&all, wanted) {
                Ok(found) => found.id.clone(),
                Err(_) if create_missing && !wanted.trim().is_empty() => {
                    core::create_condition_in_category(tx, workspace_id, wanted, None, now)
                        .map_err(map_core_error)?
                }
                Err(error) => return Err(error),
            };
            let member = core::member(&id, negated);
            if !resolved.contains(&member) {
                resolved.push(member);
            }
        }
        if !resolved.is_empty() {
            ids.push(resolved);
        }
    }
    Ok(ids)
}

impl Workspace {
    /// Every condition under its heading, in the tab's order, with how many
    /// open tasks need it and how many of those it is holding back; what
    /// holds now; and, for one `condition`, the open tasks that need it.
    pub fn conditions(&self, condition: Option<&str>) -> Result<Value> {
        // A day that has changed starts first, as it would on opening the
        // app; a workspace that cannot be written is read as it stands.
        let _ = self.start_day_if_due();
        let connection = self.open()?;
        let (workspace_id, _) = super::workspace_row(&connection)?;
        let all = conditions_of(&connection, &workspace_id)?;
        let usage: HashMap<String, u32> = core::condition_usage(&connection, &workspace_id)
            .map_err(map_core_error)?
            .into_iter()
            .map(|usage| (usage.condition_id, usage.open_tasks))
            .collect();
        let sections: Vec<Value> = core::condition_sections(all.clone())
            .into_iter()
            .map(|section| {
                json!({
                    "category": section.title,
                    "one_at_a_time": section.is_exclusive,
                    "setting": section.is_setting,
                    "device": section.is_device,
                    "conditions": section.conditions.iter().map(|c| {
                        let mut condition = json!({
                            "id": c.id,
                            "name": c.name,
                            "place": c.is_location,
                            "archived": c.is_archived,
                            "open_tasks": usage.get(&c.id).copied().unwrap_or(0),
                            "provides": names_of(&all, &c.provides),
                        });
                        if let Some(kind) = &c.device_kind {
                            condition["device"] = json!(kind);
                        }
                        condition
                    }).collect::<Vec<_>>(),
                })
            })
            .collect();
        let mut payload = json!({
            "workspace_id": workspace_id,
            // Device conditions hold by themselves on a device of their
            // kind; this server runs on the Mac, a laptop.
            "this_device": core::this_device_kind(),
            "count": all.len(),
            "sections": sections,
            "context": context_json(&connection, &workspace_id, &all)?,
        });
        if let Some(wanted) = condition {
            let found = resolve(&all, wanted)?;
            let tasks = core::tasks_requiring(&connection, &workspace_id, &found.id)
                .map_err(map_core_error)?;
            payload["condition"] = json!({ "id": found.id, "name": found.name });
            payload["tasks"] = json!(
                tasks
                    .into_iter()
                    .map(|task| json!({
                        "id": task.task_id,
                        "title": task.title,
                        "list_id": task.list_id,
                        "list_name": task.list_name,
                    }))
                    .collect::<Vec<_>>()
            );
        }
        Ok(payload)
    }

    /// Sets what a task requires: each group must hold, any one condition in
    /// a group will do. Conditions are named or given by id, and required
    /// not to hold as "!id", "!Name" or "not Name" (a condition actually
    /// called "Not …" wins over that last); with `create_missing` a name
    /// nothing has yet becomes a new condition (with no category) first.
    /// Empty requires nothing.
    pub fn set_task_conditions(
        &self,
        task_id: &str,
        groups: Vec<Vec<String>>,
        create_missing: bool,
    ) -> Result<Value> {
        self.journalled("Edit Conditions", |tx, _now| {
            let now = Utc::now().timestamp_millis();
            let task = task_row(tx, task_id)?;
            let workspace_id: String = tx
                .query_row(
                    "SELECT workspaceId FROM task_lists WHERE id = ?1",
                    [&task.list_id],
                    |row| row.get(0),
                )
                .map_err(map_query_error)?;
            let ids = resolve_groups(tx, &workspace_id, &groups, create_missing, now)?;
            let changed = core::set_task_requirements(tx, task_id, ids, now).map_err(|error| {
                match error {
                    takt_core::CoreError::InvalidCondition => ToolError::new(
                        "A condition is archived, or a group repeats a condition or another group. \
                         workspace_conditions lists them.",
                    ),
                    other => map_core_error(other),
                }
            })?;
            let task = task_row(tx, task_id)?;
            Ok(json!({
                "task": { "id": task.id, "title": task.title },
                "requires": task.requires,
                "changed": changed,
            }))
        })
    }

    /// Sets what a list — standalone, or nested — requires of every task in
    /// it and in the lists nested under it, written as for
    /// [`Self::set_task_conditions`]. Empty requires nothing.
    pub fn set_list_conditions(
        &self,
        list_id: &str,
        groups: Vec<Vec<String>>,
        create_missing: bool,
    ) -> Result<Value> {
        {
            let connection = self.open()?;
            if !has_migration(&connection, LIST_CONDITIONS_MIGRATION)? {
                return Err(ToolError::new(format!(
                    "This workspace predates lists that require conditions \
                     ({LIST_CONDITIONS_MIGRATION}). Open an up-to-date Takt once to migrate it."
                )));
            }
        }
        self.journalled("Edit Conditions", |tx, _now| {
            let now = Utc::now().timestamp_millis();
            let (workspace_id, _) = super::workspace_row(tx)?;
            let name: String = tx
                .query_row(
                    "SELECT name FROM task_lists WHERE id = ?1 AND workspaceId = ?2
                     UNION ALL
                     SELECT t.title FROM tasks t JOIN task_lists l ON l.id = t.listId
                     WHERE t.id = ?1 AND l.workspaceId = ?2 AND t.itemKind = 'list'",
                    [list_id, workspace_id.as_str()],
                    |row| row.get(0),
                )
                .map_err(|_| {
                    ToolError::new(format!(
                        "No list or nested list with id {list_id}. workspace_tree lists them."
                    ))
                })?;
            let ids = resolve_groups(tx, &workspace_id, &groups, create_missing, now)?;
            let changed = core::set_list_requirements(tx, list_id, ids, now).map_err(|error| {
                match error {
                    takt_core::CoreError::InvalidCondition => ToolError::new(
                        "A condition is archived, or a group repeats a condition or another group. \
                         workspace_conditions lists them.",
                    ),
                    other => map_core_error(other),
                }
            })?;
            let saved = core::list_requirement_groups(tx, list_id).map_err(map_core_error)?;
            Ok(json!({
                "list": { "id": list_id, "name": name },
                "requires": named_groups(&condition_names(tx)?, saved),
                "changed": changed,
            }))
        })
    }

    /// Makes a condition a device condition of `kind` — laptop, phone or
    /// tablet — which holds by itself on a device of that kind and nowhere
    /// else, or an ordinary condition again with none.
    pub fn set_condition_device(&self, condition: &str, kind: Option<&str>) -> Result<Value> {
        {
            let connection = self.open()?;
            if !has_migration(&connection, LIST_CONDITIONS_MIGRATION)? {
                return Err(ToolError::new(format!(
                    "This workspace predates device conditions ({LIST_CONDITIONS_MIGRATION}). \
                     Open an up-to-date Takt once to migrate it."
                )));
            }
        }
        let kind = match kind {
            None => None,
            Some(kind) => Some(core::normalized_device_kind(kind).ok_or_else(|| {
                ToolError::new(format!(
                    "device is \"laptop\", \"phone\" or \"tablet\" (or null), not \"{kind}\"."
                ))
            })?),
        };
        self.journalled("Edit Condition", |tx, _now| {
            let now = Utc::now().timestamp_millis();
            let (workspace_id, _) = super::workspace_row(tx)?;
            let all = conditions_of(tx, &workspace_id)?;
            let found = resolve(&all, condition)?.clone();
            let changed =
                core::set_condition_device(tx, &found.id, kind, now).map_err(map_core_error)?;
            let saved = takt_core::rows::condition(tx, &found.id)
                .map_err(map_core_error)?
                .unwrap_or(found);
            Ok(json!({
                "condition": {
                    "id": saved.id,
                    "name": saved.name,
                    "device": saved.device_kind,
                },
                "changed": changed,
            }))
        })
    }

    /// Sets what a condition provides, replacing what it did: while it holds,
    /// they hold too. Conditions by name or id; with `create_missing` a name
    /// nothing has yet becomes a new condition (with no category) first.
    /// Empty provides nothing.
    pub fn set_condition_provides(
        &self,
        condition: &str,
        provides: Vec<String>,
        create_missing: bool,
    ) -> Result<Value> {
        {
            let connection = self.open()?;
            if !has_migration(&connection, PROVIDES_MIGRATION)? {
                return Err(ToolError::new(format!(
                    "This workspace predates conditions that provide others ({PROVIDES_MIGRATION}). \
                     Open an up-to-date Takt once to migrate it."
                )));
            }
        }
        self.journalled("Edit Condition", |tx, _now| {
            let now = Utc::now().timestamp_millis();
            let (workspace_id, _) = super::workspace_row(tx)?;
            let all = conditions_of(tx, &workspace_id)?;
            let provider = resolve(&all, condition)?.clone();
            let mut ids: Vec<String> = Vec::new();
            for wanted in &provides {
                let all = conditions_of(tx, &workspace_id)?;
                let id = match resolve(&all, wanted) {
                    Ok(found) => found.id.clone(),
                    Err(_) if create_missing && !wanted.trim().is_empty() => {
                        core::create_condition_in_category(tx, &workspace_id, wanted, None, now)
                            .map_err(map_core_error)?
                    }
                    Err(error) => return Err(error),
                };
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            let changed = core::set_condition_provides(tx, &provider.id, ids, now).map_err(
                |error| match error {
                    takt_core::CoreError::InvalidCondition => ToolError::new(
                        "A condition cannot provide itself, or an archived condition it did not \
                         provide already. workspace_conditions lists them.",
                    ),
                    other => map_core_error(other),
                },
            )?;
            let all = conditions_of(tx, &workspace_id)?;
            let saved = all
                .iter()
                .find(|c| c.id == provider.id)
                .map(|c| c.provides.clone())
                .unwrap_or_default();
            Ok(json!({
                "condition": { "id": provider.id, "name": provider.name },
                "provides": names_of(&all, &saved),
                "changed": changed,
            }))
        })
    }

    /// Says what holds now. Not an undo step: where you are is not an edit.
    pub fn set_context(&self, edit: ContextEdit) -> Result<Value> {
        let mut connection = self.open_for_writing()?;
        if !has_migration(&connection, CONTEXT_MIGRATION)? {
            return Err(ToolError::new(format!(
                "This workspace predates a shared context ({CONTEXT_MIGRATION}). \
                 Open an up-to-date Takt once to migrate it."
            )));
        }
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_write_error)?;
        let (workspace_id, _) = super::workspace_row(&tx)?;
        let all = conditions_of(&tx, &workspace_id)?;
        let now = Utc::now().timestamp_millis();
        // What you say now is said of today: yesterday's answer goes first.
        day_start::apply(&tx, &workspace_id, now, &zone()).map_err(map_core_error)?;
        let stored = focus_context::load(&tx, &workspace_id)
            .map_err(map_core_error)?
            .unwrap_or_default();
        let mut context: StoredFocusContext = focus_context::focus_context_in_force(stored, now);
        let ids = |names: &[String]| -> Result<Vec<String>> {
            names
                .iter()
                .map(|name| resolve(&all, name).map(|c| c.id.clone()))
                .collect()
        };
        let mut current = context.condition_ids.clone();
        if edit.clear || edit.set.is_some() {
            current.clear();
        }
        let places: Vec<String> = all
            .iter()
            .filter(|c| c.is_location)
            .map(|c| c.id.clone())
            .collect();
        if let Some(set) = &edit.set {
            current = focus_context::focus_context_changed(current, ids(set)?, Vec::new(), places);
        }
        // Off, then on, as the app's switches do: a provided condition
        // turned off stays off until what provides it changes, and one
        // turned on that is not provided is said to hold.
        let mut said = core::ConditionsSaid {
            condition_ids: current,
            off_ids: if edit.clear || edit.set.is_some() {
                Vec::new()
            } else {
                context.off_ids.clone()
            },
        };
        for id in ids(&edit.off)? {
            said = core::switched(&all, &said, &id, Some(false));
        }
        for id in ids(&edit.on)? {
            said = core::switched(&all, &said, &id, Some(true));
        }
        context.condition_ids = said.condition_ids;
        context.off_ids = said.off_ids;
        if edit.set.is_some() || edit.clear || !edit.on.is_empty() {
            context.suggested_ids.clear();
        }
        if let Some(mode) = edit.mode.as_deref() {
            context.mode = match mode {
                "progress" | "finish" => mode.to_string(),
                other => {
                    return Err(ToolError::new(format!(
                        "mode is \"progress\" or \"finish\", not \"{other}\"."
                    )));
                }
            };
        }
        let minutes = |value: i64| (value > 0).then(|| now + value * 60_000);
        if let Some(value) = edit.available_minutes {
            context.available_until_ms = minutes(value);
        }
        if let Some(value) = edit.expires_in_minutes {
            context.expires_at_ms = minutes(value);
        }
        let changed =
            focus_context::save(&tx, &workspace_id, &context, now).map_err(map_core_error)?;
        let mut payload = context_json(&tx, &workspace_id, &all)?;
        tx.commit().map_err(map_write_error)?;
        payload["changed"] = json!(changed);
        Ok(payload)
    }

    /// Starts the day if it has changed since the last reset. Whether the
    /// context was reset.
    fn start_day_if_due(&self) -> Result<bool> {
        let mut connection = self.open_for_writing()?;
        if !has_migration(&connection, CONTEXT_MIGRATION)? {
            return Ok(false);
        }
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_write_error)?;
        let (workspace_id, _) = super::workspace_row(&tx)?;
        let reset = day_start::apply(&tx, &workspace_id, Utc::now().timestamp_millis(), &zone())
            .map_err(map_core_error)?;
        tx.commit().map_err(map_write_error)?;
        Ok(reset)
    }

    /// Sets how each day starts: whether what holds goes back each day to
    /// the conditions it starts with, the hour the day changes, and those
    /// conditions. Naming conditions switches the reset on unless `resets`
    /// says otherwise. Not an undo step, like the rest of what holds now.
    /// Today counts as started: the first reset is at the next change. With
    /// nothing to change, reports how each day starts.
    pub fn set_day_start(&self, edit: DayStartEdit) -> Result<Value> {
        let mut connection = self.open_for_writing()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_write_error)?;
        let (workspace_id, _) = super::workspace_row(&tx)?;
        let all = conditions_of(&tx, &workspace_id)?;
        let mut settings = day_start::settings(&tx).map_err(map_core_error)?;
        if let Some(hour) = edit.hour {
            if !(0..=23).contains(&hour) {
                return Err(ToolError::new(format!(
                    "hour is 0 to 23, the hour the day changes, not {hour}."
                )));
            }
            settings.hour = hour as i32;
        }
        if let Some(names) = &edit.conditions {
            let mut ids = Vec::new();
            for name in names {
                let found = resolve(&all, name)?;
                if found.is_archived {
                    return Err(ToolError::new(format!(
                        "{} is archived: restore it before starting the day with it.",
                        found.name
                    )));
                }
                ids.push(found.id.clone());
            }
            // One place at most, the last named, as saying where you are.
            let places: Vec<String> = all
                .iter()
                .filter(|c| c.is_location)
                .map(|c| c.id.clone())
                .collect();
            settings.condition_ids = ids.iter().fold(Vec::new(), |current, id| {
                focus_context::focus_context_changed(
                    current,
                    vec![id.clone()],
                    Vec::new(),
                    places.clone(),
                )
            });
            settings.resets = true;
        }
        if let Some(resets) = edit.resets {
            settings.resets = resets;
        }
        let now = Utc::now().timestamp_millis();
        let asked = edit.resets.is_some() || edit.hour.is_some() || edit.conditions.is_some();
        let changed =
            asked && day_start::save(&tx, &settings, now, &zone()).map_err(map_core_error)?;
        let saved = day_start::settings(&tx).map_err(map_core_error)?;
        tx.commit().map_err(map_write_error)?;
        let mut payload = day_start_json(&saved, &all);
        payload["changed"] = json!(changed);
        Ok(payload)
    }
}
