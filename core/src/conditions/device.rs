//! Device conditions: a condition that means the device in your hand.
//!
//! What holds is one synced answer per account (`focus_context`), so saying
//! Campus on the phone tells the Mac. That is right for a place and wrong
//! for "Laptop": a task that needs the laptop should wait on the phone and
//! be offered on the Mac at the same moment, whatever was last said. So a
//! condition can be marked with a device kind (`deviceKind`, v41), and then
//! it is never part of what is said to hold or switched off. Instead each
//! client says once what kind of device it is ([`set_this_device_kind`]) and
//! the device conditions of that kind hold there, as if said to, through
//! [`super::holdings`] — the one place every check reads what holds from —
//! so Up next, the focus panel, Today and every explanation see them.
//!
//! The kind is the device's, not the account's, so it lives in the process
//! rather than in the database: the Mac app and the CLI and MCP server
//! (which run on the Mac) are a laptop, an iPhone a phone, and an iPad or an
//! Android tablet a tablet. A process that never says has no kind, and no
//! device condition holds in it.
//!
//! A device condition is shown under its own heading, Device, whatever its
//! category, and is never a place. It can provide others (Laptop: Big
//! screen), but nothing can provide it, since nothing said about where you
//! are can put a laptop in your hand.

use std::sync::RwLock;

use rusqlite::{OptionalExtension, Transaction, params};

use crate::CoreError;
use crate::rows::ConditionRow;
use crate::time::{stored, swift_json_strings};

/// The heading device conditions are shown under.
pub const DEVICE_CATEGORY: &str = "Device";

/// The kinds of device, as stored.
pub const DEVICE_KINDS: [&str; 3] = ["laptop", "phone", "tablet"];

static THIS_DEVICE: RwLock<Option<String>> = RwLock::new(None);

/// A device kind as given, made canonical: one of [`DEVICE_KINDS`] however
/// it is cased or spaced, or none for anything else.
pub fn normalized_device_kind(kind: &str) -> Option<&'static str> {
    let wanted = kind.trim().to_ascii_lowercase();
    DEVICE_KINDS.into_iter().find(|known| *known == wanted)
}

/// Whether a condition is a device condition. One of a kind this build does
/// not know is still one: it is kept out of the context all the same, and
/// simply never holds here.
pub fn is_device(condition: &ConditionRow) -> bool {
    condition
        .device_kind
        .as_deref()
        .is_some_and(|kind| !kind.trim().is_empty())
}

/// Whether `condition` is a live device condition of this device's kind.
pub(crate) fn holds_here(condition: &ConditionRow, here: Option<&str>) -> bool {
    !condition.is_archived
        && here.is_some()
        && condition
            .device_kind
            .as_deref()
            .and_then(normalized_device_kind)
            == here
}

/// Says what kind of device this process runs on — "laptop", "phone" or
/// "tablet" — or none. Every client calls it once as it starts, before
/// anything asks what holds; the device conditions of that kind then hold.
/// An unknown kind is none.
#[uniffi::export]
pub fn set_this_device_kind(kind: Option<String>) {
    let kind = kind
        .as_deref()
        .and_then(normalized_device_kind)
        .map(str::to_string);
    if let Ok(mut this) = THIS_DEVICE.write() {
        *this = kind;
    }
}

/// The kind of device this process said it runs on, if it has.
#[uniffi::export]
pub fn this_device_kind() -> Option<String> {
    THIS_DEVICE.read().ok().and_then(|this| this.clone())
}

/// The ids among `ids` that are not device conditions: what a context may
/// say holds or switch off.
pub(crate) fn without_devices(conditions: &[ConditionRow], ids: &[String]) -> Vec<String> {
    ids.iter()
        .filter(|id| {
            !conditions
                .iter()
                .any(|condition| condition.id == **id && is_device(condition))
        })
        .cloned()
        .collect()
}

/// Marks a condition as the device condition of `kind`, or (none) makes it
/// an ordinary condition again. Marking it takes it out of what is said to
/// hold, switched off or offered back, and out of what other conditions
/// provide, since none of that can apply to it any more; it stops being a
/// place, and a place's Location filing goes with that. Clearing it leaves
/// it where it was filed. False, writing nothing, when it already was that.
pub fn set_condition_device(
    transaction: &Transaction,
    id: &str,
    kind: Option<&str>,
    now_ms: i64,
) -> Result<bool, CoreError> {
    let kind = match kind.map(str::trim).filter(|kind| !kind.is_empty()) {
        None => None,
        Some(kind) => Some(normalized_device_kind(kind).ok_or(CoreError::InvalidCondition)?),
    };
    let (workspace_id, current, category): (String, Option<String>, Option<String>) = transaction
        .query_row(
            "SELECT workspaceId, deviceKind, category FROM task_conditions WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or(CoreError::InvalidCondition)?;
    if current.as_deref() == kind {
        return Ok(false);
    }
    let Some(kind) = kind else {
        transaction.execute(
            "UPDATE task_conditions SET deviceKind = NULL, updatedAt = ?1 WHERE id = ?2",
            params![stored(now_ms), id],
        )?;
        return Ok(true);
    };
    let category = super::normalized_category(category.as_deref())
        .filter(|category| category != super::LOCATION_CATEGORY);
    transaction.execute(
        "UPDATE task_conditions SET deviceKind = ?1, isLocation = 0, category = ?2, updatedAt = ?3
         WHERE id = ?4",
        params![kind, category, stored(now_ms), id],
    )?;
    // What provided it no longer can.
    let providers: Vec<(String, String)> = transaction
        .prepare(
            "SELECT id, providesJSON FROM task_conditions
             WHERE workspaceId = ?1 AND id <> ?2 AND providesJSON IS NOT NULL",
        )?
        .query_map(params![workspace_id, id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?
        .collect::<Result<_, _>>()?;
    for (provider, json) in providers {
        let links = super::provided_ids(&json);
        if !links.iter().any(|link| link == id) {
            continue;
        }
        let kept: Vec<String> = links.into_iter().filter(|link| link != id).collect();
        transaction.execute(
            "UPDATE task_conditions SET providesJSON = ?1, updatedAt = ?2 WHERE id = ?3",
            params![
                (!kept.is_empty()).then(|| swift_json_strings(&kept)),
                stored(now_ms),
                provider
            ],
        )?;
    }
    crate::focus_context::forget_condition(transaction, &workspace_id, id)?;
    crate::day_start::forget_condition(transaction, id, now_ms)?;
    Ok(true)
}

#[cfg(test)]
mod tests;
