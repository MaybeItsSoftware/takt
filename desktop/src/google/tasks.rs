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
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use takt_core::google_tasks::{
    GoogleTasksLedgerEntry, GoogleTasksListMapping, GoogleTasksLocalList, GoogleTasksLocalTask,
    GoogleTasksOperation, GoogleTasksPayload, GoogleTasksRemoteList, GoogleTasksRemoteTask,
    google_tasks_plan,
};
use takt_core::workspace::CoreWorkspace;

const SERVICE: &str = "Google Tasks";

/// One pass or removal at a time: each reads and saves the ledger whole.
static WORKING: std::sync::Mutex<()> = std::sync::Mutex::new(());
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
    // A task with an event on the two-way calendar already shows in Google
    // Calendar; a Google Task as well drew it twice. Left out here, the
    // mirror deletes its Google Task, and it comes back if the event goes.
    let on_calendar: HashSet<String> = core
        .google_calendar_linked_task_ids()
        .map_err(err)?
        .into_iter()
        .collect();
    let mut tasks: Vec<(usize, i64, GoogleTasksLocalTask)> = rows
        .iter()
        .filter(|r| {
            !on_calendar.contains(&r.id)
                && r.item_kind.as_deref() != Some("list")
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
    title_times: bool,
) -> Result<Summary, GoogleError> {
    let _working = WORKING.lock().unwrap_or_else(|p| p.into_inner());
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
        title_times,
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
    title_times: bool,
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
                if land_adopted(core, &id, payload, zone, title_times, now()) {
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

/// Gives a task adopted from Google its notes and due day, and with
/// `title_times` the start and length its title names. True when the
/// notes and day landed, which is what the ledger then records.
fn land_adopted(
    core: &CoreWorkspace,
    id: &str,
    payload: &GoogleTasksPayload,
    zone: &str,
    title_times: bool,
    now: i64,
) -> bool {
    let due = payload.due.as_deref().and_then(|d| parse_due(d, zone));
    // Google keeps no time on a task, so with the setting on a time in its
    // title is the start, and a range its length.
    let times = due
        .filter(|_| title_times)
        .and_then(|due| title_times_on(&payload.title, due, zone));
    if payload.notes.is_empty() && due.is_none() {
        return false;
    }
    // Google's day comes in at noon, before an afternoon start, which the
    // store refuses: a timed task is due by the end of its day instead.
    let due = match (due, times) {
        (Some(due), Some(_)) => end_of_day(due, zone).or(Some(due)),
        (due, _) => due,
    };
    let landed = core
        .update_task(
            id.into(),
            payload.title.clone(),
            payload.notes.clone(),
            due,
            times.and_then(|(start, end)| end.map(|end| (end - start) / 1000)),
            now,
            zone.into(),
        )
        .is_ok();
    if landed && let Some((start, _)) = times {
        let _ = core.schedule_task(id.into(), Some(start), now, zone.into());
    }
    landed
}

/// The last minute of the day `ms` falls on in `zone`.
fn end_of_day(ms: i64, zone: &str) -> Option<i64> {
    use chrono::TimeZone;
    let zone = takt_core::periodic::zone(zone);
    let day = zone.timestamp_millis_opt(ms).single()?.date_naive();
    zone.from_local_datetime(&day.and_hms_opt(23, 59, 0)?)
        .latest()
        .map(|t| t.timestamp_millis())
}

/// Deletes every Google Tasks list the mirror made or adopted, and forgets
/// the mirror, so the next pass with Google Tasks on starts afresh. Lists
/// it never mirrored are left alone; Takt's own lists and tasks too.
pub fn remove_mirror(client_id: &str) -> Result<usize, GoogleError> {
    let _working = WORKING.lock().unwrap_or_else(|p| p.into_inner());
    let api = Api {
        token: access_token(client_id, &TASKS_SCOPES, SERVICE)?,
    };
    let mut ledger = load_ledger();
    let lists: Vec<(String, String)> = ledger
        .lists
        .iter()
        .map(|(local, remote)| (local.clone(), remote.clone()))
        .collect();
    let mut removed = 0;
    let mut outcome = Ok(());
    for (local, remote) in lists {
        if let Err(error) = api.call(
            "DELETE",
            &format!("/users/@me/lists/{}", escape(&remote)),
            None,
        ) {
            outcome = Err(error);
            break;
        }
        // Gone from Google: its tasks went with it.
        ledger.lists.remove(&local);
        ledger
            .tasks
            .retain(|_, entry| entry.remote_list_id != remote);
        removed += 1;
    }
    if outcome.is_ok() {
        ledger = Ledger::default();
    }
    save_ledger(&ledger).map_err(GoogleError::Other)?;
    outcome.map(|()| removed)
}

/// Whether the mirror has made any Google Tasks lists.
pub fn has_mirror() -> bool {
    !load_ledger().lists.is_empty()
}

/// A time or time range in a title, on the day `due_ms` falls on in
/// `zone`: "14:00", "2pm", "14:00–15:00", "2-3pm", "9.30am to 11". The
/// start, and the end when it is a range that ends after it starts. Only
/// written times count, not words like "tonight".
pub fn title_times_on(title: &str, due_ms: i64, zone: &str) -> Option<(i64, Option<i64>)> {
    use chrono::TimeZone;
    let (start, end) = title_times(title)?;
    let zone = takt_core::periodic::zone(zone);
    let day = zone.timestamp_millis_opt(due_ms).single()?.date_naive();
    let at = |(hour, minute): (u32, u32)| {
        zone.from_local_datetime(&day.and_hms_opt(hour, minute, 0)?)
            .earliest()
            .map(|t| t.timestamp_millis())
    };
    let start_ms = at(start)?;
    let end_ms = end.and_then(at).filter(|end| *end > start_ms);
    Some((start_ms, end_ms))
}

/// An hour and minute.
type Clock = (u32, u32);

fn title_times(title: &str) -> Option<(Clock, Option<Clock>)> {
    static RANGE: std::sync::LazyLock<regex_lite::Regex> = std::sync::LazyLock::new(|| {
        regex_lite::Regex::new(
            r"(?i)(?:^|[\s(@])(\d{1,2}(?:[:.]\d{2})?)\s*(am|pm|a|p)?\s*(?:-|–|—|to)\s*(\d{1,2}(?:[:.]\d{2})?)\s*(am|pm|a|p)?(?:$|[\s),.])",
        )
        .expect("range pattern")
    });
    static SINGLE: std::sync::LazyLock<regex_lite::Regex> = std::sync::LazyLock::new(|| {
        regex_lite::Regex::new(
            r"(?i)(?:^|[\s(@])(\d{1,2}[:.]\d{2}|\d{1,2}(?:[:.]\d{2})?\s*(?:am|pm|a|p))(?:$|[\s),.])",
        )
        .expect("time pattern")
    });
    let read = |digits: &str, suffix: Option<&str>| {
        let word = format!("{digits}{}", suffix.unwrap_or_default()).to_lowercase();
        takt_core::capture::time_of_day(&word).or_else(|| {
            // A bare hour only as the half of a range with no suffix.
            let hour: u32 = digits.parse().ok()?;
            (hour < 24).then_some((hour, 0))
        })
    };
    if let Some(c) = RANGE.captures(title) {
        let first_suffix = c.get(2).map(|m| m.as_str());
        let second_suffix = c.get(4).map(|m| m.as_str());
        let has_clock =
            |m: Option<regex_lite::Match>| m.is_some_and(|m| m.as_str().contains([':', '.']));
        // "2-3" alone is likely not a time; one side needs a suffix or minutes.
        if first_suffix.is_some()
            || second_suffix.is_some()
            || has_clock(c.get(1))
            || has_clock(c.get(3))
        {
            let start = read(&c[1], first_suffix.or(second_suffix))?;
            let end = read(&c[3], second_suffix.or(first_suffix))?;
            return Some((start, Some(end)));
        }
    }
    let c = SINGLE.captures(title)?;
    let word = c[1].replace(' ', "").to_lowercase();
    Some((takt_core::capture::time_of_day(&word)?, None))
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
    fn times_are_read_from_titles() {
        assert_eq!(
            title_times("Call Sam 14:00–15:00"),
            Some(((14, 0), Some((15, 0))))
        );
        assert_eq!(
            title_times("Call Sam 14:00 - 15:30"),
            Some(((14, 0), Some((15, 30))))
        );
        assert_eq!(title_times("Gym 2-3pm"), Some(((14, 0), Some((15, 0)))));
        assert_eq!(
            title_times("Lecture 9.30am to 11"),
            Some(((9, 30), Some((11, 0))))
        );
        assert_eq!(title_times("Dentist 2pm"), Some(((14, 0), None)));
        assert_eq!(title_times("Standup at 09:15."), Some(((9, 15), None)));
        assert_eq!(title_times("Read chapters 2-3"), None);
        assert_eq!(title_times("Pay £20 tonight"), None);
        assert_eq!(title_times("Buy milk"), None);
        let zone = "Europe/London";
        let due = parse_due("2026-11-12T00:00:00.000Z", zone).unwrap();
        let (start, end) = title_times_on("Call 14:00–15:00", due, zone).unwrap();
        assert_eq!(end.unwrap() - start, 3_600_000);
        assert_eq!(due_string(start, zone), "2026-11-12T00:00:00.000Z");
    }

    #[test]
    fn an_adopted_task_takes_its_titles_times() {
        let path = std::env::temp_dir().join(format!("takt-adopt-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let path_text = path.to_string_lossy().into_owned();
        takt_core::schema::migrate_workspace(path_text.clone()).unwrap();
        let core = CoreWorkspace::open(path_text).unwrap();
        let now = crate::state::store::now_ms();
        let workspace = core.bootstrap(now).unwrap();
        let list = core
            .create_list(workspace, "L".into(), None, now)
            .unwrap()
            .id;
        let zone = "Europe/London";
        let adopt = |title: &str, on: bool| {
            let id = core
                .create_task(
                    takt_core::tasks::NewTask {
                        list_id: list.clone(),
                        title: title.into(),
                        kind: "task".into(),
                        ..Default::default()
                    },
                    now,
                )
                .unwrap();
            let payload = GoogleTasksPayload {
                title: title.into(),
                notes: String::new(),
                due: Some("2030-03-14T00:00:00.000Z".into()),
                is_completed: false,
                parent_remote_id: None,
            };
            assert!(land_adopted(&core, &id, &payload, zone, on, now));
            core.editor_snapshot(id).unwrap()
        };
        let timed = adopt("Call Sam 14:00\u{2013}15:30", true);
        let start = timed.planning.and_then(|p| p.start_at_ms).expect("a start");
        let expected = title_times_on("14:00", parse_due("2030-03-14", zone).unwrap(), zone)
            .unwrap()
            .0;
        assert_eq!(start, expected);
        assert_eq!(timed.estimate_seconds, Some(90 * 60));
        let due = timed.due_at_ms.expect("still due");
        assert_eq!(due_string(due, zone), "2030-03-14T00:00:00.000Z");
        let plain = adopt("Call Sam 14:00\u{2013}15:30", false);
        assert_eq!(plain.planning.and_then(|p| p.start_at_ms), None);
        assert_eq!(plain.estimate_seconds, None);
        drop(core);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_ledger_reads_the_swift_apps_file() {
        let text = r#"{"tasks":{"t1":{"remoteID":"r1","remoteListID":"l1","pushedTitle":"A","pushedNotes":"","pushedDue":null,"pushedCompleted":false}},"lists":{"L":"l1"}}"#;
        let ledger: Ledger = serde_json::from_str(text).unwrap();
        assert_eq!(ledger.tasks["t1"].remote_id, "r1");
        assert_eq!(ledger.lists["L"], "l1");
    }
}
