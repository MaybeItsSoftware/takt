//! Conditions a list requires, which every task in it inherits.
//!
//! A list can require conditions as a task can ("Revision" needs Campus or
//! the Library), and then every task in it does, along with whatever it
//! requires itself. So do the tasks in nested lists under it, and a nested
//! list's own requirements pass down to what is under it in the same way.
//! A task's **effective** requirements are its own groups and every group it
//! inherits: all must be met, as any task's groups must.
//!
//! A standalone list keeps its groups on its row (`requirementGroupsJSON`,
//! v41), stored exactly as a task's are in its planning, negation and all.
//! A nested list is a task record (`itemKind` "list"), so its groups are the
//! ones in its planning already, and setting a nested list's conditions
//! sets those.
//!
//! The inherited groups are added where requirements are read, never
//! written into the tasks: [`crate::focus::candidates`] gives each candidate
//! its effective groups, so Up next, Today's held-back count, the focus
//! panel and every explanation check them, and the counts of what a
//! condition holds back (`condition_usage`, `tasks_requiring`) count a task
//! that inherits one. A client from before v41 drops the column on pull and
//! checks each task against its own requirements only.

use std::collections::{BTreeSet, HashMap};

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::CoreError;
use crate::time::stored;

/// A stored group list read leniently: anything that is not an array of
/// arrays of strings is none, and empty groups go.
pub fn stored_groups(json: &str) -> Vec<Vec<String>> {
    serde_json::from_str::<Vec<Vec<String>>>(json)
        .unwrap_or_default()
        .into_iter()
        .filter(|group| !group.is_empty())
        .collect()
}

/// The column's value for `groups`: none when they require nothing.
fn encoded(groups: &[Vec<String>]) -> Option<String> {
    (!groups.is_empty()).then(|| serde_json::Value::from(groups.to_vec()).to_string())
}

/// Whether the lists have somewhere to keep their requirements: not before
/// v41, which the CLI can meet in a database the app has not migrated yet.
fn has_column(connection: &Connection) -> Result<bool, CoreError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('task_lists') WHERE name = 'requirementGroupsJSON')",
        [],
        |row| row.get(0),
    )?)
}

/// What each list that is not archived requires, by list id, for the lists
/// that require anything; every workspace's with none.
fn list_groups(
    connection: &Connection,
    workspace_id: Option<&str>,
) -> Result<HashMap<String, Vec<Vec<String>>>, CoreError> {
    if !has_column(connection)? {
        return Ok(HashMap::new());
    }
    let mut statement = connection.prepare(
        "SELECT id, requirementGroupsJSON FROM task_lists
         WHERE requirementGroupsJSON IS NOT NULL AND isArchived = 0
           AND (?1 IS NULL OR workspaceId = ?1)",
    )?;
    let rows = statement
        .query_map([workspace_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(id, json)| (id, stored_groups(&json)))
        .filter(|(_, groups)| !groups.is_empty())
        .collect())
}

/// What each standalone list in the workspace requires, for those that
/// require anything and are not archived.
pub fn list_requirements(
    connection: &Connection,
    workspace_id: &str,
) -> Result<HashMap<String, Vec<Vec<String>>>, CoreError> {
    list_groups(connection, Some(workspace_id))
}

/// A planning blob's groups, none when it does not read.
fn planning_groups(json: Option<&str>) -> Vec<Vec<String>> {
    json.and_then(|json| crate::planning::Planning::from_json(json).ok())
        .and_then(|planning| planning.requirement_groups)
        .unwrap_or_default()
}

/// `own` with every group of `inherited` the task does not already have
/// (the same members in any order), in that order.
pub fn with_inherited(own: &[Vec<String>], inherited: &[Vec<String>]) -> Vec<Vec<String>> {
    let key = |group: &Vec<String>| group.iter().cloned().collect::<BTreeSet<String>>();
    let mut seen: Vec<BTreeSet<String>> = own.iter().map(key).collect();
    let mut result = own.to_vec();
    for group in inherited {
        let group_key = key(group);
        if !seen.contains(&group_key) {
            seen.push(group_key);
            result.push(group.clone());
        }
    }
    result
}

/// What every task inherits from the lists it is in — its list's groups and
/// those of each nested list above it — by task id, for the tasks that
/// inherit anything. Every workspace's. Cheap when no list requires
/// anything, which is most of the time.
pub fn inherited_groups(
    connection: &Connection,
) -> Result<HashMap<String, Vec<Vec<String>>>, CoreError> {
    let lists = list_groups(connection, None)?;
    let mut nested: HashMap<String, Vec<Vec<String>>> = HashMap::new();
    {
        let mut statement = connection.prepare(
            "SELECT t.id, m.planningJSON FROM tasks t JOIN task_metadata m ON m.taskId = t.id
             WHERE t.itemKind = 'list' AND m.planningJSON IS NOT NULL
               AND instr(m.planningJSON, 'requirementGroups') > 0",
        )?;
        for row in statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })? {
            let (id, json) = row?;
            let groups = planning_groups(json.as_deref());
            if !groups.is_empty() {
                nested.insert(id, groups);
            }
        }
    }
    if lists.is_empty() && nested.is_empty() {
        return Ok(HashMap::new());
    }
    let mut parents: HashMap<String, (Option<String>, String)> = HashMap::new();
    {
        let mut statement = connection.prepare("SELECT id, parentTaskId, listId FROM tasks")?;
        for row in statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            let (id, parent, list) = row?;
            parents.insert(id, (parent, list));
        }
    }
    let mut memo: HashMap<String, Vec<Vec<String>>> = HashMap::new();
    let ids: Vec<String> = parents.keys().cloned().collect();
    for id in ids {
        inherited_of(&id, &parents, &lists, &nested, &mut memo, 0);
    }
    memo.retain(|_, groups| !groups.is_empty());
    Ok(memo)
}

/// What `id` inherits: its parent's inheritance and, when the parent is a
/// nested list, the parent's own groups; at the top, its list's. Outermost
/// first. A loop in the tree (which a merge could in principle leave) stops
/// at a depth no real tree reaches.
fn inherited_of(
    id: &str,
    parents: &HashMap<String, (Option<String>, String)>,
    lists: &HashMap<String, Vec<Vec<String>>>,
    nested: &HashMap<String, Vec<Vec<String>>>,
    memo: &mut HashMap<String, Vec<Vec<String>>>,
    depth: usize,
) -> Vec<Vec<String>> {
    if let Some(known) = memo.get(id) {
        return known.clone();
    }
    let Some((parent, list)) = parents.get(id) else {
        return Vec::new();
    };
    let groups = match parent {
        Some(parent) if depth < 256 && parents.contains_key(parent) => {
            let above = inherited_of(parent, parents, lists, nested, memo, depth + 1);
            match nested.get(parent) {
                Some(own) => with_inherited(&above, own),
                None => above,
            }
        }
        _ => lists.get(list).cloned().unwrap_or_default(),
    };
    memo.insert(id.to_string(), groups.clone());
    groups
}

/// A task's parent, title, kind and planning, read walking up the tree.
type Ancestor = (Option<String>, String, Option<String>, Option<String>);

/// One list a task inherits requirements from, and what they are.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct InheritedRequirement {
    /// The standalone list's id, or the nested list's task id.
    pub list_id: String,
    pub list_name: String,
    /// Whether it is a nested list (a task record) rather than a standalone
    /// list.
    pub is_nested: bool,
    /// Its groups, as a task's are kept.
    pub groups: Vec<Vec<String>>,
}

/// The lists `task_id` inherits requirements from, outermost first: its
/// standalone list, then each nested list above it that requires anything.
/// A nested list's own requirements are not among its own inheritance. Empty
/// for a task that inherits nothing, or does not exist.
pub fn inherited_requirements(
    connection: &Connection,
    task_id: &str,
) -> Result<Vec<InheritedRequirement>, CoreError> {
    let mut chain: Vec<InheritedRequirement> = Vec::new();
    let start: Option<(Option<String>, String)> = connection
        .query_row(
            "SELECT parentTaskId, listId FROM tasks WHERE id = ?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((mut parent, list_id)) = start else {
        return Ok(Vec::new());
    };
    let mut depth = 0;
    while let Some(id) = parent.take() {
        depth += 1;
        if depth > 256 {
            break;
        }
        let row: Option<Ancestor> = connection
            .query_row(
                "SELECT t.parentTaskId, t.title, t.itemKind, m.planningJSON
                 FROM tasks t LEFT JOIN task_metadata m ON m.taskId = t.id WHERE t.id = ?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some((above, title, kind, json)) = row else {
            break;
        };
        if kind.as_deref() == Some("list") {
            let groups = planning_groups(json.as_deref());
            if !groups.is_empty() {
                chain.push(InheritedRequirement {
                    list_id: id,
                    list_name: title,
                    is_nested: true,
                    groups,
                });
            }
        }
        parent = above;
    }
    if has_column(connection)? {
        let list: Option<(String, Option<String>, bool)> = connection
            .query_row(
                "SELECT name, requirementGroupsJSON, isArchived FROM task_lists WHERE id = ?1",
                [&list_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if let Some((name, Some(json), false)) = list {
            let groups = stored_groups(&json);
            if !groups.is_empty() {
                chain.push(InheritedRequirement {
                    list_id,
                    list_name: name,
                    is_nested: false,
                    groups,
                });
            }
        }
    }
    chain.reverse();
    Ok(chain)
}

/// What a list requires: a standalone list's groups, or a nested list's own
/// (its planning's). Empty for one that requires nothing.
pub fn list_requirement_groups(
    connection: &Connection,
    list_id: &str,
) -> Result<Vec<Vec<String>>, CoreError> {
    if has_column(connection)? {
        let stored: Option<Option<String>> = connection
            .query_row(
                "SELECT requirementGroupsJSON FROM task_lists WHERE id = ?1",
                [list_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(json) = stored {
            return Ok(json.as_deref().map(stored_groups).unwrap_or_default());
        }
    }
    let nested: Option<Option<String>> = connection
        .query_row(
            "SELECT m.planningJSON FROM tasks t LEFT JOIN task_metadata m ON m.taskId = t.id
             WHERE t.id = ?1 AND t.itemKind = 'list'",
            [list_id],
            |row| row.get(0),
        )
        .optional()?;
    match nested {
        Some(json) => Ok(planning_groups(json.as_deref())),
        None => Err(CoreError::MissingList {
            id: list_id.to_string(),
        }),
    }
}

/// Sets what a list requires — every task in it, and in the nested lists
/// under it, inherits it — replacing what it did; empty requires nothing.
/// `list_id` is a standalone list, or a nested list, whose own requirements
/// (its planning's) are set. The rules are a task's: each condition in the
/// list's workspace and not archived unless the list already required it,
/// no group repeating a condition or another group. False, writing nothing,
/// when it already required exactly that.
pub fn set_list_requirements(
    transaction: &Transaction,
    list_id: &str,
    groups: Vec<Vec<String>>,
    now_ms: i64,
) -> Result<bool, CoreError> {
    let standalone: Option<(String, Option<String>)> = if has_column(transaction)? {
        transaction
            .query_row(
                "SELECT workspaceId, requirementGroupsJSON FROM task_lists WHERE id = ?1",
                [list_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
    } else {
        None
    };
    let Some((workspace_id, current)) = standalone else {
        let is_nested: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id = ?1 AND itemKind = 'list')",
            [list_id],
            |row| row.get(0),
        )?;
        if !is_nested {
            return Err(CoreError::MissingList {
                id: list_id.to_string(),
            });
        }
        return super::set_task_requirements(transaction, list_id, groups, now_ms);
    };
    let current = current.as_deref().map(stored_groups).unwrap_or_default();
    let previous: Vec<&str> = current
        .iter()
        .flatten()
        .map(|member| super::condition_id_of(member))
        .collect();
    super::check_groups(transaction, &workspace_id, &groups, &previous)?;
    if groups == current {
        return Ok(false);
    }
    transaction.execute(
        "UPDATE task_lists SET requirementGroupsJSON = ?1, updatedAt = ?2 WHERE id = ?3",
        params![encoded(&groups), stored(now_ms), list_id],
    )?;
    Ok(true)
}

/// Merging condition `from` into `into` in what the lists require, as
/// `merge_conditions` does in what tasks require. Returns how many lists
/// changed.
pub(crate) fn merge_list_groups(
    transaction: &Transaction,
    workspace_id: &str,
    from: &str,
    into: &str,
    now_ms: i64,
) -> Result<usize, CoreError> {
    if !has_column(transaction)? {
        return Ok(0);
    }
    let rows: Vec<(String, String)> = transaction
        .prepare(
            "SELECT id, requirementGroupsJSON FROM task_lists
             WHERE workspaceId = ?1 AND requirementGroupsJSON IS NOT NULL
               AND instr(requirementGroupsJSON, ?2) > 0",
        )?
        .query_map(params![workspace_id, from], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?
        .collect::<Result<_, _>>()?;
    let mut changed = 0;
    for (id, json) in rows {
        let groups = stored_groups(&json);
        if !groups
            .iter()
            .flatten()
            .any(|member| super::condition_id_of(member) == from)
        {
            continue;
        }
        let merged = super::merged_groups(&groups, from, into);
        transaction.execute(
            "UPDATE task_lists SET requirementGroupsJSON = ?1, updatedAt = ?2 WHERE id = ?3",
            params![encoded(&merged), stored(now_ms), id],
        )?;
        changed += 1;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests;
