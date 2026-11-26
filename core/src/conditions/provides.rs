//! Conditions that provide others, and what holds once they have.
//!
//! A condition can provide others: being at Home means Floor space, a Desk
//! and Quiet; having the Laptop means Typing and a Big screen. You say the
//! few things that are settings — where you are, what you have with you —
//! and what they provide holds with them, so a task that needs Floor space
//! is offered at Home or the Gym without anyone turning Floor space on.
//!
//! What holds (**effective conditions**) is the conditions said to hold
//! directly, plus everything they provide, through any number of links,
//! less the provided ones switched off for now ("housemates in": Quiet
//! off). Every check of a task against the context — availability, the
//! ranking, its explanation, the "waiting" counts, Next Up — reads that one
//! set, so a negated requirement ("not Quiet") is met by switching Quiet
//! off as much as by never providing it. [`holdings`] is the one place it is
//! worked out.
//!
//! A switched-off condition stays off until whatever provides it is unset
//! or replaced: saying you are at the Library rather than Home brings
//! Home's overrides to an end (`focus_context::save`, through
//! [`kept_overrides`]).
//!
//! Links are stored on the providing condition (`providesJSON`, v34), so
//! they sync, merge and undo with it. Archived conditions provide nothing
//! and are provided by nothing; a link to a condition that has gone is
//! ignored, and dropped the next time the list is written. A condition
//! cannot provide itself, and a cycle (Home provides Quiet, Quiet provides
//! Home) is harmless: each condition is visited once per setting, so the
//! walk ends however the links loop. A client older than v34 drops the
//! column on pull and simply derives nothing.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use rusqlite::{Transaction, params};

use super::{LOCATION_CATEGORY, condition_sections, effective_category};
use crate::CoreError;
use crate::rows::{self, ConditionRow};
use crate::time::{decode_strings, stored, swift_json_strings};

/// A stored `providesJSON`, read leniently: sorted, once each, and empty for
/// anything that is not an array of strings.
pub fn provided_ids(json: &str) -> Vec<String> {
    decode_strings(json)
        .into_iter()
        .filter(|id| !id.trim().is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// The column's value for `ids`: none when it provides nothing.
fn encoded(ids: &[String]) -> Option<String> {
    (!ids.is_empty()).then(|| swift_json_strings(ids))
}

/// Sets what condition `id` provides, replacing what it did; empty provides
/// nothing. Each must be in its workspace and not archived (unless it
/// already provided it), and none may be the condition itself. A link to a
/// condition that has gone is dropped. False, writing nothing, when it
/// already provided exactly that.
pub fn set_condition_provides(
    transaction: &Transaction,
    id: &str,
    provided: Vec<String>,
    now_ms: i64,
) -> Result<bool, CoreError> {
    let condition = rows::condition(transaction, id)?.ok_or(CoreError::InvalidCondition)?;
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    for member in provided {
        let member = member.trim().to_string();
        if member == id {
            return Err(CoreError::InvalidCondition);
        }
        let target = rows::condition(transaction, &member)?;
        // Nothing said about where you are puts a device in your hand.
        let usable = target.is_some_and(|target| {
            target.workspace_id == condition.workspace_id
                && !super::is_device(&target)
                && (!target.is_archived || condition.provides.contains(&member))
        });
        if !usable {
            return Err(CoreError::InvalidCondition);
        }
        wanted.insert(member);
    }
    let wanted: Vec<String> = wanted.into_iter().collect();
    if wanted == condition.provides {
        return Ok(false);
    }
    transaction.execute(
        "UPDATE task_conditions SET providesJSON = ?1, updatedAt = ?2 WHERE id = ?3",
        params![encoded(&wanted), stored(now_ms), id],
    )?;
    Ok(true)
}

/// Merging `from` into `into`: whatever provided `from` provides `into`
/// instead, `into` provides what `from` did as well, nothing ends up
/// providing itself, and nothing is named twice. Only the rows that change
/// are written.
pub(crate) fn merge_links(
    transaction: &Transaction,
    workspace_id: &str,
    from: &str,
    into: &str,
    now_ms: i64,
) -> Result<(), CoreError> {
    let all = rows::conditions(transaction, workspace_id)?;
    let inherited: Vec<String> = all
        .iter()
        .find(|condition| condition.id == from)
        .map(|condition| condition.provides.clone())
        .unwrap_or_default();
    for condition in &all {
        let mut links: BTreeSet<String> = condition
            .provides
            .iter()
            .map(|id| {
                if id == from {
                    into.to_string()
                } else {
                    id.clone()
                }
            })
            .collect();
        if condition.id == into {
            links.extend(inherited.iter().map(|id| {
                if id == from {
                    into.to_string()
                } else {
                    id.clone()
                }
            }));
        }
        links.remove(&condition.id);
        let links: Vec<String> = links.into_iter().collect();
        if links != condition.provides {
            transaction.execute(
                "UPDATE task_conditions SET providesJSON = ?1, updatedAt = ?2 WHERE id = ?3",
                params![encoded(&links), stored(now_ms), condition.id],
            )?;
        }
    }
    Ok(())
}

/// While sync folds a second workspace into the canonical one: a link in
/// `from` to one of its conditions that is about to give way to the
/// canonical workspace's condition of the same name names that one
/// instead. Runs with recording on, so what it writes syncs.
pub(crate) fn adopt_links(
    transaction: &Transaction,
    from: &str,
    into: &str,
) -> Result<(), CoreError> {
    let has_column: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('task_conditions') WHERE name = 'providesJSON')",
        [],
        |row| row.get(0),
    )?;
    if !has_column {
        return Ok(());
    }
    let mut renamed: HashMap<String, String> = HashMap::new();
    {
        let mut statement = transaction.prepare(
            "SELECT mine.id, canonical.id FROM task_conditions mine
             JOIN task_conditions canonical
               ON canonical.name = mine.name AND canonical.workspaceId = ?2
             WHERE mine.workspaceId = ?1",
        )?;
        for row in statement.query_map(params![from, into], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (mine, canonical) = row?;
            renamed.insert(mine, canonical);
        }
    }
    if renamed.is_empty() {
        return Ok(());
    }
    for condition in rows::conditions(transaction, from)? {
        let links: Vec<String> = condition
            .provides
            .iter()
            .map(|id| renamed.get(id).cloned().unwrap_or_else(|| id.clone()))
            .filter(|id| *id != condition.id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if links != condition.provides {
            transaction.execute(
                "UPDATE task_conditions SET providesJSON = ?1 WHERE id = ?2",
                params![encoded(&links), condition.id],
            )?;
        }
    }
    Ok(())
}

/// One condition's part in what holds now.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ConditionHolding {
    pub condition_id: String,
    /// Said to hold directly: a place chosen, a box ticked.
    pub is_set: bool,
    /// A device condition of this device's kind, which holds here by
    /// itself (`conditions::device`).
    #[uniffi(default = false)]
    pub is_device: bool,
    /// The conditions said to hold that provide it, directly or through
    /// another, in the order the tab shows them: "from Home". Empty when
    /// nothing does.
    pub provided_by: Vec<String>,
    /// Provided, but switched off for now.
    pub is_off: bool,
    /// Whether it holds: set, or provided and not switched off. This is
    /// what every requirement is checked against.
    pub holds: bool,
}

/// The live conditions by id, and each one's place in the tab.
struct Graph<'a> {
    live: HashMap<&'a str, &'a ConditionRow>,
    position: HashMap<String, usize>,
}

impl<'a> Graph<'a> {
    fn new(conditions: &'a [ConditionRow]) -> Self {
        let position = condition_sections(conditions.to_vec())
            .into_iter()
            .flat_map(|section| section.conditions)
            .enumerate()
            .map(|(index, condition)| (condition.id, index))
            .collect();
        Graph {
            live: conditions
                .iter()
                .filter(|condition| !condition.is_archived)
                .map(|condition| (condition.id.as_str(), condition))
                .collect(),
            position,
        }
    }

    fn place(&self, id: &str) -> (usize, String) {
        (
            self.position.get(id).copied().unwrap_or(usize::MAX),
            id.to_string(),
        )
    }

    /// Everything `root` provides, through any number of links, never
    /// passing through a condition in `blocked`, which is reached but goes
    /// no further. Each condition once, so a cycle ends; the root itself is
    /// never among them.
    fn reached(&self, root: &str, blocked: &HashSet<&str>) -> Vec<&'a str> {
        let mut seen: HashSet<&str> = HashSet::from([root]);
        let mut found = Vec::new();
        let mut queue: VecDeque<&str> = VecDeque::new();
        if let Some(condition) = self.live.get(root) {
            queue.extend(condition.provides.iter().map(String::as_str));
        }
        while let Some(id) = queue.pop_front() {
            let Some(condition) = self.live.get(id) else {
                continue;
            };
            // A device condition holds by the device alone.
            if super::is_device(condition) {
                continue;
            }
            if !seen.insert(condition.id.as_str()) {
                continue;
            }
            found.push(condition.id.as_str());
            if !blocked.contains(id) {
                queue.extend(condition.provides.iter().map(String::as_str));
            }
        }
        found
    }

    /// Everything `roots` provide between them, overrides aside.
    fn closure(&self, roots: &[String]) -> HashSet<String> {
        let none = HashSet::new();
        roots
            .iter()
            .flat_map(|root| self.reached(root, &none))
            .map(str::to_string)
            .collect()
    }
}

/// What holds when `set_ids` are said to hold and `off_ids` are switched off
/// for now, on this device: every condition set, provided or switched off,
/// and the device conditions of the kind this process said it is
/// ([`super::set_this_device_kind`]), in the tab's order (one the
/// conditions do not include, after them). An override on a condition
/// nothing provides any more is no part of it.
pub fn holdings(
    conditions: &[ConditionRow],
    set_ids: &[String],
    off_ids: &[String],
) -> Vec<ConditionHolding> {
    holdings_on(
        conditions,
        set_ids,
        off_ids,
        super::this_device_kind().as_deref(),
    )
}

/// [`holdings`] on a device of kind `device` (none for no kind). A device
/// condition in `set_ids` or `off_ids` is ignored: only the device decides
/// whether one holds.
pub fn holdings_on(
    conditions: &[ConditionRow],
    set_ids: &[String],
    off_ids: &[String],
    device: Option<&str>,
) -> Vec<ConditionHolding> {
    let graph = Graph::new(conditions);
    let set_ids = super::without_devices(conditions, set_ids);
    let off_ids = super::without_devices(conditions, off_ids);
    let here: BTreeSet<&str> = conditions
        .iter()
        .filter(|condition| super::holds_here(condition, device))
        .map(|condition| condition.id.as_str())
        .collect();
    let set: BTreeSet<&str> = set_ids.iter().map(String::as_str).collect();
    // A condition said to hold is not switched off, whatever provides it.
    let off: HashSet<&str> = off_ids
        .iter()
        .map(String::as_str)
        .filter(|id| !set.contains(id))
        .collect();
    let mut roots: Vec<&str> = set.union(&here).copied().collect();
    roots.sort_by_cached_key(|id| graph.place(id));
    let mut providers: HashMap<&str, Vec<String>> = HashMap::new();
    for root in &roots {
        for id in graph.reached(root, &off) {
            providers.entry(id).or_default().push(root.to_string());
        }
    }
    let mut ids: Vec<&str> = set
        .iter()
        .copied()
        .chain(here.iter().copied())
        .chain(providers.keys().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    ids.sort_by_cached_key(|id| graph.place(id));
    ids.into_iter()
        .map(|id| {
            let is_set = set.contains(id);
            let is_device = here.contains(id);
            let provided_by: Vec<String> = providers
                .remove(id)
                .unwrap_or_default()
                .into_iter()
                .filter(|root| root != id)
                .collect();
            let is_off = !is_set && !is_device && off.contains(id);
            ConditionHolding {
                condition_id: id.to_string(),
                is_set,
                is_device,
                holds: is_set || is_device || !is_off,
                provided_by,
                is_off,
            }
        })
        .collect()
}

/// The ids of the conditions that hold, sorted: [`holdings`] that hold.
pub fn effective_ids(
    conditions: &[ConditionRow],
    set_ids: &[String],
    off_ids: &[String],
) -> Vec<String> {
    let mut ids: Vec<String> = holdings(conditions, set_ids, off_ids)
        .into_iter()
        .filter(|holding| holding.holds)
        .map(|holding| holding.condition_id)
        .collect();
    ids.sort();
    ids
}

/// The overrides that survive the conditions said to hold going from
/// `previous_set` to `next_set`: one on a condition that something unset or
/// replaced provided goes (saying Library rather than Home brings back what
/// Home provides), and so does one on a condition nothing provides any
/// more, or one now said to hold directly. Sorted.
pub fn kept_overrides(
    conditions: &[ConditionRow],
    previous_set: &[String],
    next_set: &[String],
    off_ids: &[String],
) -> Vec<String> {
    let graph = Graph::new(conditions);
    let removed: Vec<String> = previous_set
        .iter()
        .filter(|id| !next_set.contains(id))
        .cloned()
        .collect();
    let reset = graph.closure(&removed);
    let provided = graph.closure(next_set);
    off_ids
        .iter()
        .filter(|id| provided.contains(*id) && !reset.contains(*id) && !next_set.contains(id))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// What is said to hold, and what is switched off for now: the two halves
/// of a context that [`holdings`] reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct ConditionsSaid {
    /// Said to hold directly, sorted.
    pub condition_ids: Vec<String>,
    /// Provided but switched off for now, sorted.
    pub off_ids: Vec<String>,
}

/// One condition switched on (`on`), off, or — with none — the other way
/// from how it stands.
///
/// Off, a condition said to hold is no longer said to; if something still
/// provides it, it is switched off for now as well. On, an override goes,
/// and a condition nothing provides is said to hold. A place is always said
/// to hold, and replaces the last one, since you are in one place at a
/// time; whatever the place it replaced provided comes back on.
pub fn switched(
    conditions: &[ConditionRow],
    said: &ConditionsSaid,
    condition_id: &str,
    on: Option<bool>,
) -> ConditionsSaid {
    // Only the device in hand decides whether a device condition holds.
    if conditions
        .iter()
        .any(|condition| condition.id == condition_id && super::is_device(condition))
    {
        return said.clone();
    }
    let holds = holdings(conditions, &said.condition_ids, &said.off_ids)
        .into_iter()
        .any(|holding| holding.condition_id == condition_id && holding.holds);
    let on = on.unwrap_or(!holds);
    let is_place = |id: &str| {
        conditions.iter().any(|condition| {
            condition.id == id
                && effective_category(condition).as_deref() == Some(LOCATION_CATEGORY)
        })
    };
    let mut set: BTreeSet<String> = said.condition_ids.iter().cloned().collect();
    let mut off: BTreeSet<String> = said.off_ids.iter().cloned().collect();
    if on {
        off.remove(condition_id);
        if is_place(condition_id) {
            set.retain(|id| !is_place(id));
            set.insert(condition_id.to_string());
        } else {
            let set_now: Vec<String> = set.iter().cloned().collect();
            let off_now: Vec<String> = off.iter().cloned().collect();
            let provided = effective_ids(conditions, &set_now, &off_now)
                .iter()
                .any(|id| id == condition_id);
            if !provided {
                set.insert(condition_id.to_string());
            }
        }
    } else {
        set.remove(condition_id);
        let set_now: Vec<String> = set.iter().cloned().collect();
        if Graph::new(conditions)
            .closure(&set_now)
            .contains(condition_id)
        {
            off.insert(condition_id.to_string());
        }
    }
    let condition_ids: Vec<String> = set.into_iter().collect();
    let off_ids = kept_overrides(
        conditions,
        &said.condition_ids,
        &condition_ids,
        &off.into_iter().collect::<Vec<_>>(),
    );
    ConditionsSaid {
        condition_ids,
        off_ids,
    }
}

/// Whether a condition is a setting: something about your situation (a
/// place, what you have with you, the device in hand) rather than a
/// property a task needs. A place is one, a device condition is one, and so
/// is anything that provides something.
pub fn is_setting(condition: &ConditionRow) -> bool {
    effective_category(condition).as_deref() == Some(LOCATION_CATEGORY)
        || super::is_device(condition)
        || (!condition.is_archived && !condition.provides.is_empty())
}

/// What holds when `set_ids` are said to hold and `off_ids` are switched
/// off for now, condition by condition: the core's one answer to "what
/// holds", which every client reads rather than working it out itself.
#[uniffi::export]
pub fn condition_holdings(
    conditions: Vec<ConditionRow>,
    set_ids: Vec<String>,
    off_ids: Vec<String>,
) -> Vec<ConditionHolding> {
    holdings(&conditions, &set_ids, &off_ids)
}

/// The ids of the conditions that hold, sorted: what a context's
/// `condition_ids` should be when a task is checked against it.
#[uniffi::export]
pub fn effective_condition_ids(
    conditions: Vec<ConditionRow>,
    set_ids: Vec<String>,
    off_ids: Vec<String>,
) -> Vec<String> {
    effective_ids(&conditions, &set_ids, &off_ids)
}

/// A context's conditions with one switched on, off or (`on` none) the
/// other way: see [`switched`].
#[uniffi::export]
pub fn focus_context_switch(
    conditions: Vec<ConditionRow>,
    said: ConditionsSaid,
    condition_id: String,
    on: Option<bool>,
) -> ConditionsSaid {
    switched(&conditions, &said, &condition_id, on)
}

/// The overrides that survive the conditions said to hold changing: see
/// [`kept_overrides`]. Saving a context applies it too.
#[uniffi::export]
pub fn focus_context_kept_overrides(
    conditions: Vec<ConditionRow>,
    previous_ids: Vec<String>,
    next_ids: Vec<String>,
    off_ids: Vec<String>,
) -> Vec<String> {
    kept_overrides(&conditions, &previous_ids, &next_ids, &off_ids)
}

#[cfg(test)]
mod tests;
