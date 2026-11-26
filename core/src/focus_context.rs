//! The context: which conditions hold now, the time available, the goal and
//! when the answer lapses, kept per workspace in `focus_contexts`.
//!
//! It used to live in each app's preferences, where nothing else could see
//! it. As a row, the CLI and the agent read and set it like any other part
//! of the workspace, and the app sees their change the way it sees any other
//! external write. Since v28 the row syncs: it is one answer per account, so
//! saying you are at Campus on the phone tells the Mac. It is pushed whole
//! (`schema::triggers::WHOLE_ROW_TABLES`), so the last device to say what
//! holds wins every column of it, and `conditionsSetAt` travels with it,
//! which is what lets every device call an answer from yesterday stale. It
//! is not journalled: saying where you are is not an edit to undo.
//!
//! Since v34 it also holds the provided conditions switched off for now
//! (`offIdsJSON`): what the conditions on provide holds with them
//! (`conditions::provides`), and an override lasts until whatever provides
//! the condition is unset or replaced, which [`save`] sees to.

use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::CoreError;
use crate::time::{decode_strings, parse_stored, stored, swift_json_strings};

/// What holds now, as stored.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StoredFocusContext {
    /// The conditions on, sorted.
    pub condition_ids: Vec<String>,
    /// When the conditions on were last changed; none with none on.
    pub conditions_set_at_ms: Option<i64>,
    /// When the time available ends; none for no limit.
    pub available_until_ms: Option<i64>,
    /// When the conditions stop being trusted; none for never.
    pub expires_at_ms: Option<i64>,
    /// "progress" or "finish".
    pub mode: String,
    /// The conditions that held before the last expiry, offered back.
    pub suggested_ids: Vec<String>,
    /// Provided conditions switched off for now, sorted. Empty before v34.
    pub off_ids: Vec<String>,
}

impl Default for StoredFocusContext {
    fn default() -> Self {
        Self {
            condition_ids: Vec::new(),
            conditions_set_at_ms: None,
            available_until_ms: None,
            expires_at_ms: None,
            mode: "progress".to_string(),
            suggested_ids: Vec::new(),
            off_ids: Vec::new(),
        }
    }
}

fn sorted(ids: Vec<String>) -> Vec<String> {
    ids.into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn table_exists(connection: &Connection, name: &str) -> Result<bool, CoreError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = ?1)",
        [name],
        |row| row.get(0),
    )?)
}

/// Whether the overrides have a column to live in: not before v34, which
/// the CLI can meet in a database the app has not migrated yet.
fn has_overrides(connection: &Connection) -> Result<bool, CoreError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('focus_contexts') WHERE name = 'offIdsJSON')",
        [],
        |row| row.get(0),
    )?)
}

fn millis(text: Option<String>) -> Option<i64> {
    text.as_deref()
        .and_then(parse_stored)
        .map(|instant| instant.timestamp_millis())
}

/// The workspace's context as last saved, or none if it never was (or the
/// database predates v27).
pub fn load(
    connection: &Connection,
    workspace_id: &str,
) -> Result<Option<StoredFocusContext>, CoreError> {
    if !table_exists(connection, "focus_contexts")? {
        return Ok(None);
    }
    let off = if has_overrides(connection)? {
        "offIdsJSON"
    } else {
        "'[]'"
    };
    Ok(connection
        .query_row(
            &format!(
                "SELECT conditionIdsJSON, conditionsSetAt, availableUntil, expiresAt, mode, suggestedIdsJSON, {off}
                 FROM focus_contexts WHERE workspaceId = ?1"
            ),
            [workspace_id],
            |row| {
                Ok(StoredFocusContext {
                    condition_ids: sorted(decode_strings(&row.get::<_, String>(0)?)),
                    conditions_set_at_ms: millis(row.get(1)?),
                    available_until_ms: millis(row.get(2)?),
                    expires_at_ms: millis(row.get(3)?),
                    mode: row.get(4)?,
                    suggested_ids: sorted(decode_strings(&row.get::<_, String>(5)?)),
                    off_ids: sorted(decode_strings(&row.get::<_, String>(6)?)),
                })
            },
        )
        .optional()?)
}

/// Saves the workspace's context. When the conditions on differ from what
/// was stored, they are stamped as set at `now_ms` (and the stamp goes when
/// none are on); otherwise the stamp stays, whatever `context` carries. An
/// override goes when whatever provided its condition is no longer on, or
/// nothing provides it any more (`conditions::kept_overrides`), so every
/// client's change of place resets the last place's overrides. False,
/// writing nothing, when nothing changed.
pub fn save(
    transaction: &Transaction,
    workspace_id: &str,
    context: &StoredFocusContext,
    now_ms: i64,
) -> Result<bool, CoreError> {
    let current = load(transaction, workspace_id)?;
    // A device condition is never said to hold or switched off: each device
    // adds its own (`conditions::device`), so one said here would follow
    // the answer to every other device.
    let known = crate::rows::conditions(transaction, workspace_id)?;
    let ids = sorted(crate::conditions::without_devices(
        &known,
        &context.condition_ids,
    ));
    let set_at = if ids.is_empty() {
        None
    } else {
        match &current {
            Some(current) if current.condition_ids == ids => current.conditions_set_at_ms,
            _ => Some(now_ms),
        }
    };
    let overrides = has_overrides(transaction)?;
    let off_ids = if overrides {
        crate::conditions::kept_overrides(
            &known,
            current
                .as_ref()
                .map_or(&[][..], |current| &current.condition_ids),
            &ids,
            &crate::conditions::without_devices(&known, &context.off_ids),
        )
    } else {
        Vec::new()
    };
    let next = StoredFocusContext {
        condition_ids: ids,
        off_ids,
        conditions_set_at_ms: set_at,
        suggested_ids: sorted(crate::conditions::without_devices(
            &known,
            &context.suggested_ids,
        )),
        mode: if context.mode == "finish" {
            "finish".to_string()
        } else {
            "progress".to_string()
        },
        ..context.clone()
    };
    if current.as_ref() == Some(&next) {
        return Ok(false);
    }
    // Before v34 there is nowhere to keep an override; `?9` goes unused.
    let (column, value, update) = if overrides {
        (", offIdsJSON", ", ?9", ", offIdsJSON = excluded.offIdsJSON")
    } else {
        ("", "", "")
    };
    transaction.execute(
        &format!(
            "INSERT INTO focus_contexts
               (workspaceId, conditionIdsJSON, conditionsSetAt, availableUntil, expiresAt, mode, suggestedIdsJSON, updatedAt{column})
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8{value})
             ON CONFLICT(workspaceId) DO UPDATE SET
               conditionIdsJSON = excluded.conditionIdsJSON, conditionsSetAt = excluded.conditionsSetAt,
               availableUntil = excluded.availableUntil, expiresAt = excluded.expiresAt,
               mode = excluded.mode, suggestedIdsJSON = excluded.suggestedIdsJSON,
               updatedAt = excluded.updatedAt{update}"
        ),
        rusqlite::params_from_iter(
            [
                Some(workspace_id.to_string()),
                Some(swift_json_strings(&next.condition_ids)),
                next.conditions_set_at_ms.map(stored),
                next.available_until_ms.map(stored),
                next.expires_at_ms.map(stored),
                Some(next.mode.clone()),
                Some(swift_json_strings(&next.suggested_ids)),
                Some(stored(now_ms)),
                Some(swift_json_strings(&next.off_ids)),
            ]
            .into_iter()
            .take(if overrides { 9 } else { 8 }),
        ),
    )?;
    Ok(true)
}

/// A merged condition's place in the context taken by the one it was merged
/// into, on and offered back alike.
pub(crate) fn replace_condition(
    transaction: &Transaction,
    workspace_id: &str,
    from: &str,
    into: &str,
    now_ms: i64,
) -> Result<(), CoreError> {
    let Some(mut context) = load(transaction, workspace_id)? else {
        return Ok(());
    };
    let swap = |ids: &mut Vec<String>| {
        for id in ids.iter_mut() {
            if id == from {
                *id = into.to_string();
            }
        }
    };
    let set_at = context.conditions_set_at_ms;
    swap(&mut context.condition_ids);
    swap(&mut context.suggested_ids);
    swap(&mut context.off_ids);
    context.condition_ids = sorted(context.condition_ids);
    context.off_ids = sorted(context.off_ids);
    save(transaction, workspace_id, &context, now_ms)?;
    // The same answer, renamed: not a new one, so it keeps its stamp.
    if set_at.is_some() {
        transaction.execute(
            "UPDATE focus_contexts SET conditionsSetAt = ?1 WHERE workspaceId = ?2",
            params![set_at.map(stored), workspace_id],
        )?;
    }
    Ok(())
}

/// A condition taken out of the context — on, switched off and offered
/// back alike — when it becomes a device condition, which no context can
/// hold. The answer is otherwise the same one, so it keeps its stamp.
pub(crate) fn forget_condition(
    transaction: &Transaction,
    workspace_id: &str,
    id: &str,
) -> Result<(), CoreError> {
    let Some(context) = load(transaction, workspace_id)? else {
        return Ok(());
    };
    let kept = |ids: &[String]| -> Vec<String> {
        ids.iter().filter(|known| *known != id).cloned().collect()
    };
    let (on, off, offered) = (
        kept(&context.condition_ids),
        kept(&context.off_ids),
        kept(&context.suggested_ids),
    );
    if on == context.condition_ids && off == context.off_ids && offered == context.suggested_ids {
        return Ok(());
    }
    let off_column = if has_overrides(transaction)? {
        ", offIdsJSON = ?4"
    } else {
        ""
    };
    transaction.execute(
        &format!(
            "UPDATE focus_contexts SET conditionIdsJSON = ?1, suggestedIdsJSON = ?2,
               conditionsSetAt = CASE WHEN ?1 = '[]' THEN NULL ELSE conditionsSetAt END{off_column}
             WHERE workspaceId = ?3"
        ),
        rusqlite::params_from_iter(
            [
                swift_json_strings(&on),
                swift_json_strings(&offered),
                workspace_id.to_string(),
                swift_json_strings(&off),
            ]
            .into_iter()
            .take(if off_column.is_empty() { 3 } else { 4 }),
        ),
    )?;
    Ok(())
}

/// Folds `from`'s context into `into`'s while sync folds a second
/// workspace into the canonical one: the more recently saved of the two
/// stays, under `into`, with `from`'s conditions renamed to the canonical
/// workspace's conditions of the same name (which are about to replace
/// them). Runs with recording on, so what it writes syncs.
pub(crate) fn adopt(transaction: &Transaction, from: &str, into: &str) -> Result<(), CoreError> {
    if !table_exists(transaction, "focus_contexts")? {
        return Ok(());
    }
    let Some(mut theirs) = load(transaction, from)? else {
        return Ok(());
    };
    let updated = |workspace: &str| -> Result<Option<String>, CoreError> {
        Ok(transaction
            .query_row(
                "SELECT updatedAt FROM focus_contexts WHERE workspaceId = ?1",
                [workspace],
                |row| row.get(0),
            )
            .optional()?)
    };
    let from_updated = updated(from)?;
    let keep_theirs = match updated(into)? {
        None => true,
        Some(canonical) => from_updated.as_ref().is_some_and(|from| *from > canonical),
    };
    if keep_theirs {
        let rename = |ids: &mut Vec<String>| -> Result<(), CoreError> {
            for id in ids.iter_mut() {
                let same: Option<String> = transaction
                    .query_row(
                        "SELECT canonical.id FROM task_conditions mine
                         JOIN task_conditions canonical
                           ON canonical.name = mine.name AND canonical.workspaceId = ?2
                         WHERE mine.id = ?1 AND mine.workspaceId = ?3",
                        params![id.as_str(), into, from],
                        |row| row.get(0),
                    )
                    .optional()?;
                if let Some(same) = same {
                    *id = same;
                }
            }
            *ids = sorted(std::mem::take(ids));
            Ok(())
        };
        rename(&mut theirs.condition_ids)?;
        rename(&mut theirs.suggested_ids)?;
        rename(&mut theirs.off_ids)?;
        transaction.execute("DELETE FROM focus_contexts WHERE workspaceId = ?1", [into])?;
        transaction.execute(
            "INSERT INTO focus_contexts
               (workspaceId, conditionIdsJSON, conditionsSetAt, availableUntil, expiresAt, mode, suggestedIdsJSON, updatedAt)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                into,
                swift_json_strings(&theirs.condition_ids),
                theirs.conditions_set_at_ms.map(stored),
                theirs.available_until_ms.map(stored),
                theirs.expires_at_ms.map(stored),
                theirs.mode,
                swift_json_strings(&theirs.suggested_ids),
                from_updated,
            ],
        )?;
        if has_overrides(transaction)? {
            transaction.execute(
                "UPDATE focus_contexts SET offIdsJSON = ?1 WHERE workspaceId = ?2",
                params![swift_json_strings(&theirs.off_ids), into],
            )?;
        }
    }
    transaction.execute("DELETE FROM focus_contexts WHERE workspaceId = ?1", [from])?;
    Ok(())
}

/// The context as it stands at `now_ms`: once it has expired its conditions
/// are set aside — offered back, so "Use again" can restore them — and the
/// expiry, stamp and overrides go with them. Otherwise `context` unchanged.
#[uniffi::export]
pub fn focus_context_in_force(context: StoredFocusContext, now_ms: i64) -> StoredFocusContext {
    match context.expires_at_ms {
        Some(expiry) if expiry <= now_ms => StoredFocusContext {
            suggested_ids: if context.condition_ids.is_empty() {
                context.suggested_ids
            } else {
                context.condition_ids
            },
            condition_ids: Vec::new(),
            conditions_set_at_ms: None,
            expires_at_ms: None,
            off_ids: Vec::new(),
            ..context
        },
        _ => context,
    }
}

/// Whether the conditions on were said on an earlier day than `now_ms`, in
/// `zone`: a place left on overnight is probably not where you are. Never
/// with nothing on.
#[uniffi::export]
pub fn focus_context_is_stale(context: StoredFocusContext, now_ms: i64, zone: String) -> bool {
    use chrono::TimeZone;
    let (false, Some(set_at)) = (
        context.condition_ids.is_empty(),
        context.conditions_set_at_ms,
    ) else {
        return false;
    };
    let zone = crate::periodic::zone(&zone);
    let day = |ms: i64| {
        zone.timestamp_millis_opt(ms)
            .single()
            .map(|instant| instant.date_naive())
    };
    matches!((day(set_at), day(now_ms)), (Some(then), Some(now)) if then < now)
}

/// `current` with `on` turned on and `off` turned off. Turning a place on
/// turns every other place (`location_ids`) off, since you are in one place
/// at a time; whatever else combines. Sorted.
#[uniffi::export]
pub fn focus_context_changed(
    current: Vec<String>,
    on: Vec<String>,
    off: Vec<String>,
    location_ids: Vec<String>,
) -> Vec<String> {
    let mut next: BTreeSet<String> = current.into_iter().collect();
    for id in &off {
        next.remove(id);
    }
    // The last place named wins, as saying two in a row would.
    for id in on {
        if location_ids.contains(&id) {
            for place in &location_ids {
                next.remove(place);
            }
        }
        next.insert(id);
    }
    next.into_iter().collect()
}

#[cfg(test)]
mod tests;
