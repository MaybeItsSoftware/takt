//! Takt's lists mirrored into Google Tasks (`GoogleTasksMirrorService.swift`,
//! `NativeGoogleTasksIntegrationPlugin.swift`). The core plans each pass
//! (`google_tasks::google_tasks_plan`); this reads both sides and carries
//! the plan out.
//!
//! The ledger (what was pushed, and which Google list mirrors which list)
//! is `google-tasks-mirror.json`, the Swift app's file in its own shape, so
//! the two never disagree about what they made. It is saved even when a
//! pass fails halfway: every Google write lands in it as it happens, and
//! the next pass must not create those tasks a second time.

use super::{GoogleError, TASKS_SCOPES, access_token, failure, send};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use takt_core::google_tasks::{
    GoogleTasksLedgerEntry, GoogleTasksListMapping, GoogleTasksLocalList, GoogleTasksLocalTask,
    GoogleTasksOperation, GoogleTasksPayload, GoogleTasksRemoteList, GoogleTasksRemoteTask,
    google_tasks_plan,
};
use takt_core::workspace::CoreWorkspace;

const SERVICE: &str = "Google Tasks";
const ROOT: &str = "https://tasks.googleapis.com/tasks/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    #[serde(rename = "remoteID")]
    pub remote_id: String,
    #[serde(rename = "remoteListID")]
    pub remote_list_id: String,
    pub pushed_title: String,
    pub pushed_notes: String,
    pub pushed_due: Option<String>,
    pub pushed_completed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Ledger {
    pub tasks: BTreeMap<String, Entry>,
    pub lists: BTreeMap<String, String>,
}

fn ledger_path() -> PathBuf {
    std::env::var_os("TAKT_DESKTOP_PREFS")
        .map(PathBuf::from)
        .map(|p| p.with_file_name("google-tasks-mirror.json"))
        .unwrap_or_else(|| crate::state::store::app_support().join("google-tasks-mirror.json"))
}

pub fn load_ledger() -> Ledger {
    std::fs::read(ledger_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_ledger(ledger: &Ledger) -> Result<(), String> {
    let path = ledger_path();
    let bytes = serde_json::to_vec_pretty(ledger).map_err(|e| e.to_string())?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, &path).map_err(|e| e.to_string())
}

/// A day as Google Tasks writes one: that local day at midnight UTC.
pub fn due_string(due_ms: i64, zone: &str) -> String {
    use chrono::TimeZone;
    let zone = takt_core::periodic::zone(zone);
    let day = zone
        .timestamp_millis_opt(due_ms)
        .single()
        .map(|t| t.date_naive())
        .unwrap_or_default();
    format!("{}T00:00:00.000Z", day.format("%Y-%m-%d"))
}

/// Google's due string back as that day, at noon local time.
pub fn parse_due(text: &str, zone: &str) -> Option<i64> {
    use chrono::TimeZone;
    let day = chrono::NaiveDate::parse_from_str(text.get(..10)?, "%Y-%m-%d").ok()?;
    let noon = day.and_hms_opt(12, 0, 0)?;
    takt_core::periodic::zone(zone)
        .from_local_datetime(&noon)
        .single()
        .map(|t| t.timestamp_millis())
}

struct Api {
    token: String,
}

impl Api {
    fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<Option<Value>, GoogleError> {
        let url = format!("{ROOT}{path}");
        let body = body.map(|b| b.to_string());
        let reply = send(method, &url, &self.token, body.as_deref())?;
        // Deleted between the read and the write: already done.
        if matches!(reply.status, 404 | 410) {
            return Ok(None);
        }
        if !reply.ok() {
            return Err(failure(SERVICE, &reply));
        }
        Ok(Some(
            serde_json::from_str(&reply.body).unwrap_or(Value::Null),
        ))
    }

    fn pages(&self, path: &str) -> Result<Vec<Value>, GoogleError> {
        let mut items = Vec::new();
        let mut page: Option<String> = None;
        loop {
            let join = if path.contains('?') { '&' } else { '?' };
            let mut url = format!("{path}{join}maxResults=100");
            if let Some(token) = &page {
                url.push_str(&format!("&pageToken={}", crate::sync::http::escape(token)));
            }
            let value = self.call("GET", &url, None)?.unwrap_or(Value::Null);
            if let Some(found) = value["items"].as_array() {
                items.extend(found.iter().cloned());
            }
            match value["nextPageToken"].as_str() {
                Some(next) => page = Some(next.to_string()),
                None => return Ok(items),
            }
        }
    }
}

fn escape(id: &str) -> String {
    crate::sync::http::escape(id)
}

fn task_body(payload: &GoogleTasksPayload) -> Value {
    json!({
        "title": payload.title,
        "notes": payload.notes,
        "due": payload.due,
        "status": if payload.is_completed { "completed" } else { "needsAction" },
    })
}

fn entry_for(remote_id: String, remote_list_id: String, payload: &GoogleTasksPayload) -> Entry {
    Entry {
        remote_id,
        remote_list_id,
        pushed_title: payload.title.clone(),
        pushed_notes: payload.notes.clone(),
        pushed_due: payload.due.clone(),
        pushed_completed: payload.is_completed,
    }
}

/// Everything Takt holds, in the planner's words. Archived lists are left
/// out: to the mirror, archiving a list is its ceasing to exist.
fn snapshot(
    core: &CoreWorkspace,
    workspace_id: &str,
    zone: &str,
) -> Result<(Vec<GoogleTasksLocalList>, Vec<GoogleTasksLocalTask>), GoogleError> {
    let err = |e: takt_core::CoreError| GoogleError::Other(e.to_string());
    let lists = core.lists(workspace_id.into(), false).map_err(err)?;
    let rows = core
        .tasks_in_lists(lists.iter().map(|l| l.id.clone()).collect())
        .map_err(err)?;
    let local_lists = lists
        .iter()
        .map(|l| GoogleTasksLocalList {
            id: l.id.clone(),
            name: l.name.clone(),
        })
        .collect();
    let depth = {
        let parents: HashMap<&str, Option<&str>> = rows
            .iter()
            .map(|r| (r.id.as_str(), r.parent_task_id.as_deref()))
            .collect();
        move |id: &str| {
            let mut depth = 0;
            let mut at = parents.get(id).copied().flatten();
            while let Some(parent) = at {
                depth += 1;
                at = parents.get(parent).copied().flatten();
                if depth > 64 {
                    break;
                }
            }
            depth
        }
    };
    let mut tasks: Vec<(usize, i64, GoogleTasksLocalTask)> = rows
        .iter()
        .filter(|r| {
            r.item_kind.as_deref() != Some("list")
                && r.status != "cancelled"
                && r.archived_at_ms.is_none()
        })
        .map(|r| {
            (
                depth(&r.id),
                r.sort_order,
                GoogleTasksLocalTask {
                    id: r.id.clone(),
                    list_id: r.list_id.clone(),
                    parent_id: r.parent_task_id.clone(),
                    title: r.title.clone(),
                    notes: r.notes.clone(),
                    due: r.due_at_ms.map(|ms| due_string(ms, zone)),
                    is_completed: r.status == "completed",
                },
            )
        })
        .collect();
    tasks.sort_by_key(|(depth, order, _)| (*depth, *order));
    Ok((local_lists, tasks.into_iter().map(|(_, _, t)| t).collect()))
}

#[derive(Debug, Default)]
pub struct Summary {
    pub mirrored: usize,
    pub wrote_locally: bool,
    pub conflicts: usize,
}

/// One pass of the mirror.
pub fn run_pass(
    core: &CoreWorkspace,
    workspace_id: &str,
    client_id: &str,
    zone: &str,
) -> Result<Summary, GoogleError> {
    let api = Api {
        token: access_token(client_id, &TASKS_SCOPES, SERVICE)?,
    };
    let mut ledger = load_ledger();
    let (local_lists, local_tasks) = snapshot(core, workspace_id, zone)?;
    let remote_lists: Vec<GoogleTasksRemoteList> = api
        .pages("/users/@me/lists")?
        .iter()
        .filter_map(|l| {
            Some(GoogleTasksRemoteList {
                id: l["id"].as_str()?.into(),
                title: l["title"].as_str().unwrap_or_default().into(),
            })
        })
        .collect();
    // Only the lists this mirror owns are read.
    let mut remote_tasks = Vec::new();
    for list_id in ledger.lists.values() {
        if !remote_lists.iter().any(|l| &l.id == list_id) {
            continue;
        }
        let path = format!(
            "/lists/{}/tasks?showCompleted=true&showDeleted=true&showHidden=true",
            escape(list_id)
        );
        for t in api.pages(&path)? {
            let Some(id) = t["id"].as_str() else { continue };
            remote_tasks.push(GoogleTasksRemoteTask {
                id: id.into(),
                list_id: list_id.clone(),
                parent_id: t["parent"].as_str().map(str::to_string),
                title: t["title"].as_str().unwrap_or_default().into(),
                notes: t["notes"].as_str().unwrap_or_default().into(),
                due: t["due"].as_str().map(str::to_string),
                is_completed: t["status"].as_str() == Some("completed"),
                is_deleted: t["deleted"].as_bool().unwrap_or(false),
            });
        }
    }
    let plan = google_tasks_plan(
        local_lists,
        local_tasks,
        remote_lists,
        remote_tasks,
        ledger
            .tasks
            .iter()
            .map(|(local, e)| GoogleTasksLedgerEntry {
                local_id: local.clone(),
                remote_id: e.remote_id.clone(),
                remote_list_id: e.remote_list_id.clone(),
                pushed_title: e.pushed_title.clone(),
                pushed_notes: e.pushed_notes.clone(),
                pushed_due: e.pushed_due.clone(),
                pushed_completed: e.pushed_completed,
            })
            .collect(),
        ledger
            .lists
            .iter()
            .map(|(local, remote)| GoogleTasksListMapping {
                local_list_id: local.clone(),
                remote_list_id: remote.clone(),
            })
            .collect(),
    );
    let mut summary = Summary {
        conflicts: plan.conflicts.len(),
        ..Default::default()
    };
    let result = apply(
        core,
        zone,
        &api,
        &plan.operations,
        &mut ledger,
        &mut summary,
    );
    let saved = save_ledger(&ledger);
    summary.mirrored = ledger.tasks.len();
    result?;
    saved.map_err(GoogleError::Other)?;
    Ok(summary)
}

fn apply(
    core: &CoreWorkspace,
    zone: &str,
    api: &Api,
    operations: &[GoogleTasksOperation],
    ledger: &mut Ledger,
    summary: &mut Summary,
) -> Result<(), GoogleError> {
    use GoogleTasksOperation as Op;
    let now = crate::state::store::now_ms;
    // Lists first: their ids are what the task operations need.
    for operation in operations {
        match operation {
            Op::CreateList {
                local_list_id,
                title,
            } => {
                let made = api.call("POST", "/users/@me/lists", Some(json!({ "title": title })))?;
                if let Some(id) = made.as_ref().and_then(|v| v["id"].as_str()) {
                    ledger.lists.insert(local_list_id.clone(), id.into());
                }
            }
            Op::AdoptRemoteList {
                local_list_id,
                remote_list_id,
            } => {
                ledger
                    .lists
                    .insert(local_list_id.clone(), remote_list_id.clone());
            }
            Op::RenameList {
                remote_list_id,
                title,
            } => {
                api.call(
                    "PATCH",
                    &format!("/users/@me/lists/{}", escape(remote_list_id)),
                    Some(json!({ "title": title })),
                )?;
            }
            Op::DeleteList {
                remote_list_id,
                local_list_id,
            } => {
                api.call(
                    "DELETE",
                    &format!("/users/@me/lists/{}", escape(remote_list_id)),
                    None,
                )?;
                ledger.lists.remove(local_list_id);
                ledger
                    .tasks
                    .retain(|_, e| &e.remote_list_id != remote_list_id);
            }
            _ => {}
        }
    }
    for operation in operations {
        match operation {
            Op::CreateTask {
                local_id,
                remote_list_id,
                payload,
            } => {
                let mut path = format!("/lists/{}/tasks", escape(remote_list_id));
                if let Some(parent) = &payload.parent_remote_id {
                    path.push_str(&format!("?parent={}", escape(parent)));
                }
                let made = api.call("POST", &path, Some(task_body(payload)))?;
                if let Some(id) = made.as_ref().and_then(|v| v["id"].as_str()) {
                    ledger.tasks.insert(
                        local_id.clone(),
                        entry_for(id.into(), remote_list_id.clone(), payload),
                    );
                }
            }
            Op::UpdateTask {
                local_id,
                remote_id,
                remote_list_id,
                payload,
            } => {
                api.call(
                    "PATCH",
                    &format!(
                        "/lists/{}/tasks/{}",
                        escape(remote_list_id),
                        escape(remote_id)
                    ),
                    Some(task_body(payload)),
                )?;
                ledger.tasks.insert(
                    local_id.clone(),
                    entry_for(remote_id.clone(), remote_list_id.clone(), payload),
                );
            }
            Op::DeleteTask {
                remote_id,
                remote_list_id,
                local_id,
            } => {
                api.call(
                    "DELETE",
                    &format!(
                        "/lists/{}/tasks/{}",
                        escape(remote_list_id),
                        escape(remote_id)
                    ),
                    None,
                )?;
                ledger.tasks.remove(local_id);
            }
            // Local writes are per row: one the store refuses must not stop
            // the rest, and the ledger records only what landed.
            Op::CompleteLocalTask { local_id } => {
                if core
                    .set_status(local_id.clone(), "completed".into(), now(), zone.into())
                    .is_ok()
                {
                    if let Some(entry) = ledger.tasks.get_mut(local_id) {
                        entry.pushed_completed = true;
                    }
                    summary.wrote_locally = true;
                }
            }
            Op::MergeNotesIntoLocalTask { local_id, notes } => {
                let landed = core
                    .editor_snapshot(local_id.clone())
                    .and_then(|s| {
                        core.update_task(
                            local_id.clone(),
                            s.title,
                            notes.clone(),
                            s.due_at_ms,
                            s.estimate_seconds,
                            now(),
                            zone.into(),
                        )
                    })
                    .is_ok();
                if landed {
                    if let Some(entry) = ledger.tasks.get_mut(local_id) {
                        entry.pushed_notes = notes.clone();
                    }
                    summary.wrote_locally = true;
                }
            }
            Op::AdoptRemoteTask {
                remote_id,
                remote_list_id,
                local_list_id,
                payload,
            } => {
                let created = core.create_task(
                    takt_core::tasks::NewTask {
                        list_id: local_list_id.clone(),
                        title: payload.title.clone(),
                        kind: "task".into(),
                        ..Default::default()
                    },
                    now(),
                );
                let Ok(id) = created else { continue };
                summary.wrote_locally = true;
                let mut landed = GoogleTasksPayload {
                    title: payload.title.clone(),
                    notes: String::new(),
                    due: None,
                    is_completed: false,
                    parent_remote_id: None,
                };
                if (!payload.notes.is_empty() || payload.due.is_some())
                    && core
                        .update_task(
                            id.clone(),
                            payload.title.clone(),
                            payload.notes.clone(),
                            payload.due.as_deref().and_then(|d| parse_due(d, zone)),
                            None,
                            now(),
                            zone.into(),
                        )
                        .is_ok()
                {
                    landed = payload.clone();
                }
                ledger.tasks.insert(
                    id,
                    entry_for(remote_id.clone(), remote_list_id.clone(), &landed),
                );
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_due_day_survives_the_trip_through_google() {
        let zone = "America/Los_Angeles";
        // 2026-03-05 18:00 in Los Angeles.
        let ms = parse_due("2026-03-05T00:00:00.000Z", zone).unwrap();
        assert_eq!(due_string(ms, zone), "2026-03-05T00:00:00.000Z");
        assert_eq!(
            due_string(ms + 5 * 3_600_000, zone),
            "2026-03-05T00:00:00.000Z"
        );
    }

    #[test]
    fn the_ledger_reads_the_swift_apps_file() {
        let text = r#"{"tasks":{"t1":{"remoteID":"r1","remoteListID":"l1","pushedTitle":"A","pushedNotes":"","pushedDue":null,"pushedCompleted":false}},"lists":{"L":"l1"}}"#;
        let ledger: Ledger = serde_json::from_str(text).unwrap();
        assert_eq!(ledger.tasks["t1"].remote_id, "r1");
        assert_eq!(ledger.lists["L"], "l1");
    }
}
