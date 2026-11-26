//! Organising conditions: the order they and their headings are shown in,
//! renaming a heading, merging one condition into another, and what each
//! condition is holding back.
//!
//! The order is one `sortOrder` per condition, workspace-wide. A heading's
//! place is its first condition's, so a move renumbers the workspace in the
//! order wanted (writing only the rows whose number changed) and every
//! heading stays one contiguous run.

use std::collections::{BTreeSet, HashMap};

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use super::{
    ConditionSection, Requirement, condition_id_of, condition_sections, effective_category,
    is_location_category, normalized_category,
};
use crate::CoreError;
use crate::planning::Planning;
use crate::rows::{self, ConditionRow};
use crate::time::stored;

/// The workspace's conditions in the order the tab shows them, archived
/// ones under their headings with the rest.
fn display_order(
    connection: &Connection,
    workspace_id: &str,
) -> Result<Vec<ConditionSection>, CoreError> {
    Ok(condition_sections(rows::conditions(
        connection,
        workspace_id,
    )?))
}

/// Numbers the conditions 0, 1, 2… in `order`, writing only those whose
/// number changes. With no `now_ms` the rows' `updatedAt` is left alone, for
/// the migration, which every device runs to the same answer.
fn renumber(
    transaction: &Transaction,
    order: &[ConditionRow],
    now_ms: Option<i64>,
) -> Result<bool, CoreError> {
    let mut changed = false;
    for (index, condition) in order.iter().enumerate() {
        let number = index as i64;
        if condition.sort_order == Some(number) {
            continue;
        }
        changed = true;
        match now_ms {
            Some(now) => transaction.execute(
                "UPDATE task_conditions SET sortOrder = ?1, updatedAt = ?2 WHERE id = ?3",
                params![number, stored(now), condition.id],
            )?,
            None => transaction.execute(
                "UPDATE task_conditions SET sortOrder = ?1 WHERE id = ?2",
                params![number, condition.id],
            )?,
        };
    }
    Ok(changed)
}

fn flatten(sections: Vec<ConditionSection>) -> Vec<ConditionRow> {
    sections
        .into_iter()
        .flat_map(|section| section.conditions)
        .collect()
}

/// v27: every workspace's conditions numbered in the order they were shown
/// before there was an order to keep.
pub(crate) fn number_every_workspace(transaction: &Transaction) -> Result<(), CoreError> {
    let workspaces: Vec<String> = transaction
        .prepare("SELECT DISTINCT workspaceId FROM task_conditions")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    for workspace in workspaces {
        renumber(
            transaction,
            &flatten(display_order(transaction, &workspace)?),
            None,
        )?;
    }
    Ok(())
}

fn workspace_of(transaction: &Transaction, id: &str) -> Result<String, CoreError> {
    transaction
        .query_row(
            "SELECT workspaceId FROM task_conditions WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or(CoreError::InvalidCondition)
}

/// Moves a condition one place up (`offset` below zero) or down (above)
/// among the conditions
/// under its heading that are archived as it is, since the tab shows the
/// archived ones apart. False, writing nothing, at the end of the run.
pub fn move_condition(
    transaction: &Transaction,
    id: &str,
    offset: i64,
    now_ms: i64,
) -> Result<bool, CoreError> {
    let workspace_id = workspace_of(transaction, id)?;
    let mut sections = display_order(transaction, &workspace_id)?;
    let Some(section) = sections
        .iter_mut()
        .find(|section| section.conditions.iter().any(|c| c.id == id))
    else {
        return Ok(false);
    };
    let archived = section
        .conditions
        .iter()
        .find(|c| c.id == id)
        .is_some_and(|c| c.is_archived);
    let siblings: Vec<usize> = section
        .conditions
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_archived == archived)
        .map(|(index, _)| index)
        .collect();
    let Some(position) = siblings
        .iter()
        .position(|&index| section.conditions[index].id == id)
    else {
        return Ok(false);
    };
    let target = position as i64 + offset.signum();
    if offset == 0 || target < 0 || target >= siblings.len() as i64 {
        return Ok(false);
    }
    section
        .conditions
        .swap(siblings[position], siblings[target as usize]);
    renumber(transaction, &flatten(sections), Some(now_ms))
}

/// Whether `section` is the heading `category` names: none for "Other",
/// any spelling of a category for that category.
fn names(section: &ConditionSection, category: Option<&str>) -> bool {
    match (normalized_category(category), &section.category) {
        (None, None) => true,
        (Some(wanted), Some(category)) => wanted.to_lowercase() == category.to_lowercase(),
        _ => false,
    }
}

/// Moves a whole heading — a category, or none for "Other" — `offset`
/// places among the headings. Settings stay above properties, so a heading
/// moves only among its own kind. False, writing nothing, at either end,
/// across that line, or for a heading nothing is under.
pub fn move_condition_category(
    transaction: &Transaction,
    workspace_id: &str,
    category: Option<&str>,
    offset: i64,
    now_ms: i64,
) -> Result<bool, CoreError> {
    let mut sections = display_order(transaction, workspace_id)?;
    let Some(position) = sections.iter().position(|section| names(section, category)) else {
        return Ok(false);
    };
    let target = position as i64 + offset;
    if offset == 0
        || target < 0
        || target >= sections.len() as i64
        || sections[target as usize].is_setting != sections[position].is_setting
        || sections[target as usize].is_device != sections[position].is_device
    {
        return Ok(false);
    }
    let section = sections.remove(position);
    sections.insert(target as usize, section);
    renumber(transaction, &flatten(sections), Some(now_ms))
}

/// Renames a heading: every condition under `from` is filed under `to`,
/// keeping its place. Renaming onto a category already in use merges the
/// two, spelled as the workspace spells it. Location cannot be renamed away,
/// since being filed there is what makes a condition a place; renaming
/// another category to Location makes its conditions places. Returns how
/// many conditions were refiled.
pub fn rename_condition_category(
    transaction: &Transaction,
    workspace_id: &str,
    from: Option<&str>,
    to: Option<&str>,
    now_ms: i64,
) -> Result<usize, CoreError> {
    let from_normalized = normalized_category(from);
    if is_location_category(from_normalized.as_deref())
        && !is_location_category(normalized_category(to).as_deref())
    {
        return Err(CoreError::InvalidCondition);
    }
    // Device is a device kind's heading, not a category to rename.
    let is_device_heading = |category: Option<&str>| {
        normalized_category(category)
            .is_some_and(|c| c.eq_ignore_ascii_case(super::DEVICE_CATEGORY))
    };
    if is_device_heading(from) || is_device_heading(to) {
        return Err(CoreError::InvalidCondition);
    }
    let members: Vec<ConditionRow> = rows::conditions(transaction, workspace_id)?
        .into_iter()
        .filter(|condition| {
            effective_category(condition).map(|c| c.to_lowercase())
                == from_normalized.as_ref().map(|c| c.to_lowercase())
        })
        .collect();
    // Spelled as a category already in use outside this heading, if one is.
    let others: Vec<String> = members.iter().map(|c| c.id.clone()).collect();
    let target = match normalized_category(to) {
        None => None,
        Some(to) => {
            let existing = rows::conditions(transaction, workspace_id)?
                .into_iter()
                .filter(|c| !others.contains(&c.id))
                .filter_map(|c| effective_category(&c))
                .find(|c| c.to_lowercase() == to.to_lowercase());
            Some(existing.unwrap_or(to))
        }
    };
    let place = is_location_category(target.as_deref());
    let mut changed = 0;
    for member in members {
        if member.category == target && member.is_location == place {
            continue;
        }
        transaction.execute(
            "UPDATE task_conditions SET category = ?1, isLocation = ?2, updatedAt = ?3 WHERE id = ?4",
            params![target, place, stored(now_ms), member.id],
        )?;
        changed += 1;
    }
    Ok(changed)
}

/// `groups` with `from` read as `into`, negated where it was ("!from" becomes
/// "!into"): a group naming both the same way names it once, and a group that
/// comes out the same as an earlier one goes.
pub(crate) fn merged_groups(groups: &[Vec<String>], from: &str, into: &str) -> Vec<Vec<String>> {
    let mut result: Vec<Vec<String>> = Vec::new();
    let mut seen: BTreeSet<Vec<String>> = BTreeSet::new();
    for group in groups {
        let mut merged: Vec<String> = Vec::new();
        for member in group {
            let requirement = Requirement::parse(member);
            let member = if requirement.condition_id == from {
                super::member(into, requirement.negated)
            } else {
                member.clone()
            };
            if !merged.contains(&member) {
                merged.push(member);
            }
        }
        let mut key = merged.clone();
        key.sort();
        if !merged.is_empty() && seen.insert(key) {
            result.push(merged);
        }
    }
    result
}

/// Merges condition `from` into `into`: every task that required `from`
/// requires `into` instead (and one that required it not to hold, `into` not
/// to), a context that had `from` on has `into` on, what provided `from`
/// provides `into` and `into` provides what `from` did, and
/// `from` is archived, so it can still be restored. Both must be in the same
/// workspace and differ. Returns how many tasks changed.
pub fn merge_conditions(
    transaction: &Transaction,
    from: &str,
    into: &str,
    now_ms: i64,
) -> Result<usize, CoreError> {
    let workspace_id = workspace_of(transaction, from)?;
    if from == into || workspace_of(transaction, into)? != workspace_id {
        return Err(CoreError::InvalidCondition);
    }
    let plans: Vec<(String, String)> = transaction
        .prepare(
            "SELECT m.taskId, m.planningJSON FROM task_metadata m
             JOIN tasks t ON t.id = m.taskId
             JOIN task_lists l ON l.id = t.listId
             WHERE l.workspaceId = ?1 AND m.planningJSON IS NOT NULL AND instr(m.planningJSON, ?2) > 0",
        )?
        .query_map(params![workspace_id, from], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut changed = 0;
    for (task_id, json) in plans {
        let Ok(mut planning) = Planning::from_json(&json) else {
            continue;
        };
        let Some(groups) = planning.requirement_groups.clone() else {
            continue;
        };
        if !groups
            .iter()
            .flatten()
            .any(|id| condition_id_of(id) == from)
        {
            continue;
        }
        planning.requirement_groups = Some(merged_groups(&groups, from, into));
        transaction.execute(
            "UPDATE task_metadata SET planningJSON = ?1, updatedAt = ?2 WHERE taskId = ?3",
            params![planning.to_json(), stored(now_ms), task_id],
        )?;
        changed += 1;
    }
    super::merge_list_groups(transaction, &workspace_id, from, into, now_ms)?;
    super::merge_links(transaction, &workspace_id, from, into, now_ms)?;
    crate::focus_context::replace_condition(transaction, &workspace_id, from, into, now_ms)?;
    transaction.execute(
        "UPDATE task_conditions SET isArchived = 1, updatedAt = ?1 WHERE id = ?2 AND isArchived = 0",
        params![stored(now_ms), from],
    )?;
    Ok(changed)
}

/// How many open tasks require one condition.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ConditionUsage {
    pub condition_id: String,
    pub open_tasks: u32,
}

/// The plans of the workspace's open tasks — not completed, not archived, in
/// a list that is not archived — with each task's id, title and list. The
/// requirement groups are the effective ones: a task's own and what it
/// inherits from its lists (`conditions::inherited`), so a task with no plan
/// of its own that inherits one is here too.
fn open_plans(
    connection: &Connection,
    workspace_id: &str,
) -> Result<Vec<(RequiringTask, Planning)>, CoreError> {
    let inherited = super::inherited_groups(connection)?;
    let mut statement = connection.prepare(
        "SELECT t.id, t.title, l.id, l.name, m.planningJSON FROM tasks t
         JOIN task_lists l ON l.id = t.listId
         LEFT JOIN task_metadata m ON m.taskId = t.id
         WHERE l.workspaceId = ?1 AND l.isArchived = 0
           AND t.status = 'open' AND t.archivedAt IS NULL
           AND (m.planningJSON IS NOT NULL OR ?2)
         ORDER BY l.sortOrder, t.sortOrder, t.id",
    )?;
    let rows = statement
        .query_map(params![workspace_id, !inherited.is_empty()], |row| {
            Ok((
                RequiringTask {
                    task_id: row.get(0)?,
                    title: row.get(1)?,
                    list_id: row.get(2)?,
                    list_name: row.get(3)?,
                },
                row.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    // One unreadable plan should not hide every condition.
    Ok(rows
        .into_iter()
        .filter_map(|(task, json)| {
            let mut plan = match json {
                Some(json) => Planning::from_json(&json).ok()?,
                None => Planning::default(),
            };
            if let Some(more) = inherited.get(&task.task_id) {
                let own = plan.requirement_groups.unwrap_or_default();
                plan.requirement_groups = Some(super::with_inherited(&own, more));
            }
            Some((task, plan))
        })
        .collect())
}

/// How many open tasks require each condition, for every condition at least
/// one does, by condition id. Requiring it not to hold counts, and a task
/// requiring a condition twice (in two groups, or both ways) counts once.
pub fn condition_usage(
    connection: &Connection,
    workspace_id: &str,
) -> Result<Vec<ConditionUsage>, CoreError> {
    let mut counts: std::collections::BTreeMap<String, u32> = Default::default();
    for (_, planning) in open_plans(connection, workspace_id)? {
        let ids: BTreeSet<String> = planning
            .requirement_groups
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(|member| condition_id_of(&member).to_string())
            .collect();
        for id in ids {
            *counts.entry(id).or_default() += 1;
        }
    }
    Ok(counts
        .into_iter()
        .map(|(condition_id, open_tasks)| ConditionUsage {
            condition_id,
            open_tasks,
        })
        .collect())
}

/// An open task that requires a condition.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RequiringTask {
    pub task_id: String,
    pub title: String,
    pub list_id: String,
    pub list_name: String,
}

/// The open tasks that require `condition_id`, or require it not to hold,
/// list by list in sidebar order.
pub fn tasks_requiring(
    connection: &Connection,
    workspace_id: &str,
    condition_id: &str,
) -> Result<Vec<RequiringTask>, CoreError> {
    Ok(open_plans(connection, workspace_id)?
        .into_iter()
        .filter(|(_, planning)| {
            planning
                .requirement_groups
                .iter()
                .flatten()
                .flatten()
                .any(|id| condition_id_of(id) == condition_id)
        })
        .map(|(task, _)| task)
        .collect())
}

/// How many held-back tasks wait on each condition: `missing` is, for each
/// task the ranking held back for conditions, the requirement groups it
/// found unmet. A task waits on every condition in those groups — to hold,
/// or, negated, to stop holding — once however many groups name it.
/// Most-waited-on first, then by id.
#[uniffi::export]
pub fn condition_waiting_counts(missing: Vec<Vec<Vec<String>>>) -> Vec<ConditionUsage> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    for groups in missing {
        let ids: BTreeSet<String> = groups
            .into_iter()
            .flatten()
            .map(|member| condition_id_of(&member).to_string())
            .collect();
        for id in ids {
            *counts.entry(id).or_default() += 1;
        }
    }
    let mut result: Vec<ConditionUsage> = counts
        .into_iter()
        .map(|(condition_id, open_tasks)| ConditionUsage {
            condition_id,
            open_tasks,
        })
        .collect();
    result.sort_by(|a, b| {
        b.open_tasks
            .cmp(&a.open_tasks)
            .then_with(|| a.condition_id.cmp(&b.condition_id))
    });
    result
}

/// Refuses requirement groups a task or list cannot be given: an empty
/// group, one repeating a condition or another group, or a condition outside
/// `workspace_id`, or archived unless it is among `previous` (what was
/// required already).
pub(crate) fn check_groups(
    transaction: &Transaction,
    workspace_id: &str,
    groups: &[Vec<String>],
    previous: &[&str],
) -> Result<(), CoreError> {
    let mut distinct: BTreeSet<Vec<String>> = BTreeSet::new();
    for group in groups {
        let unique: BTreeSet<&String> = group.iter().collect();
        if group.is_empty() || unique.len() != group.len() {
            return Err(CoreError::InvalidCondition);
        }
        let mut key = group.clone();
        key.sort();
        if !distinct.insert(key) {
            return Err(CoreError::InvalidCondition);
        }
        for member in group {
            let id = condition_id_of(member);
            let condition: Option<(String, bool)> = transaction
                .query_row(
                    "SELECT workspaceId, isArchived FROM task_conditions WHERE id = ?1",
                    [id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let usable = condition.is_some_and(|(workspace, archived)| {
                workspace == workspace_id && (!archived || previous.contains(&id))
            });
            if !usable {
                return Err(CoreError::InvalidCondition);
            }
        }
    }
    Ok(())
}

/// Sets what a task requires — every group must hold, any one condition in a
/// group will do, "!id" for a condition not holding — leaving the rest of
/// its plan alone. Empty requires nothing. Each condition must be in the task's workspace and not archived
/// unless the task already required it; groups may not repeat a condition or
/// one another. False, writing nothing, when it already required exactly
/// that.
pub fn set_task_requirements(
    transaction: &Transaction,
    task_id: &str,
    groups: Vec<Vec<String>>,
    now_ms: i64,
) -> Result<bool, CoreError> {
    let workspace_id: String = transaction
        .query_row(
            "SELECT l.workspaceId FROM tasks t JOIN task_lists l ON l.id = t.listId WHERE t.id = ?1",
            [task_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::MissingTask { id: task_id.to_string() })?;
    let stored_json: Option<Option<String>> = transaction
        .query_row(
            "SELECT planningJSON FROM task_metadata WHERE taskId = ?1",
            [task_id],
            |row| row.get(0),
        )
        .optional()?;
    let mut planning = match stored_json.clone().flatten() {
        Some(json) => Planning::from_json(&json)?,
        None => Planning::default(),
    };
    let stored_groups = planning.requirement_groups.clone().unwrap_or_default();
    let previous: Vec<&str> = stored_groups
        .iter()
        .flatten()
        .map(|member| condition_id_of(member))
        .collect();
    check_groups(transaction, &workspace_id, &groups, &previous)?;
    let wanted = (!groups.is_empty()).then_some(groups);
    if planning
        .requirement_groups
        .clone()
        .filter(|g| !g.is_empty())
        == wanted
    {
        return Ok(false);
    }
    planning.requirement_groups = wanted;
    let json = planning.normalized().map(|planning| planning.to_json());
    match stored_json {
        Some(_) => transaction.execute(
            "UPDATE task_metadata SET planningJSON = ?1, updatedAt = ?2 WHERE taskId = ?3",
            params![json, stored(now_ms), task_id],
        )?,
        None => transaction.execute(
            "INSERT INTO task_metadata (taskId, planningJSON, updatedAt) VALUES (?1, ?2, ?3)",
            params![task_id, json, stored(now_ms)],
        )?,
    };
    Ok(true)
}

#[cfg(test)]
mod tests;
