//! Two-way Google Calendar, without the network (docs/google-calendar.md).
//!
//! A timed task becomes an event in the calendar the user chose; moving or
//! renaming the event moves or renames the task, and editing, completing or
//! deleting the task updates or removes the event. Everything that decides
//! anything is here so the Mac, the iPhone and Android agree:
//!
//! * [`google_calendar_request`] builds the reads (the calendar list, a
//!   calendar's change feed, a day's events) and the readers turn Google's
//!   answers into rows;
//! * [`CoreWorkspace::google_calendar_begin_pass`] takes the one-device lease;
//! * [`CoreWorkspace::google_calendar_reconcile`] merges a pass's changed
//!   events with the workspace, applies the local half as one undo step and
//!   returns the writes to send;
//! * [`CoreWorkspace::google_calendar_record_push`] files each write's answer;
//! * [`CoreWorkspace::google_calendar_agenda`] interleaves a day's events with
//!   its timed tasks for the phones' Today.
//!
//! Sending stays with each client, as Checkvist's does: OAuth, the token and
//! the HTTP stack are the platform's.

use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::CoreError;
use crate::editor;
use crate::journal;
use crate::tasks::{self, NewTask};
use crate::time::{parse_stored, stored};
use crate::workspace::CoreWorkspace;

const API: &str = "https://www.googleapis.com/calendar/v3";
/// The private extended property naming the task an event stands for.
pub const TASK_PROPERTY: &str = "taktTaskId";
/// What a completed task's event title starts with.
pub const DONE_PREFIX: &str = "✓ ";
/// An event's length when its task has no estimate.
pub const DEFAULT_DURATION_MS: i64 = 30 * 60 * 1000;
/// How far back a task or event may end and still be created on the other side.
const CREATE_HORIZON_MS: i64 = 24 * 60 * 60 * 1000;
/// How long a device's claim to push lasts.
pub const LEASE_MS: i64 = 60 * 60 * 1000;

pub const PREF_CALENDAR: &str = "googleCalendar.calendarId";
pub const PREF_IMPORT: &str = "googleCalendar.importEvents";
pub const PREF_AGENDA: &str = "googleCalendar.agendaCalendars";
pub const PREF_LEASE: &str = "googleCalendar.lease";

// MARK: - Requests and answers

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GoogleCalendarHeader {
    pub name: String,
    pub value: String,
}

/// One request, ready for a client's HTTP stack.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GoogleCalendarHttpRequest {
    /// `GET`, `POST`, `PATCH` or `DELETE`.
    pub method: String,
    pub url: String,
    pub headers: Vec<GoogleCalendarHeader>,
    /// JSON, when the call has a body.
    pub body: Option<String>,
}

/// A read to make. The writes come out of
/// [`CoreWorkspace::google_calendar_reconcile`] already built.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum GoogleCalendarCall {
    /// The calendars the account can see.
    Calendars { page_token: Option<String> },
    /// What changed in a calendar since `sync_token`; everything without one.
    Changes {
        calendar_id: String,
        sync_token: Option<String>,
        page_token: Option<String>,
    },
    /// The events overlapping a window, recurring ones expanded, in order.
    Window {
        calendar_id: String,
        from_ms: i64,
        to_ms: i64,
        page_token: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GoogleCalendarInfo {
    pub id: String,
    pub summary: String,
    pub is_primary: bool,
    /// Whether events can be written there (owner or writer).
    pub can_write: bool,
    /// `#rrggbb`, as Google colours it for this account.
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GoogleCalendarCalendarsPage {
    pub calendars: Vec<GoogleCalendarInfo>,
    pub next_page_token: Option<String>,
}

/// An event as the core needs it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GoogleCalendarEvent {
    pub id: String,
    pub calendar_id: String,
    /// `confirmed`, `tentative` or `cancelled`.
    pub status: String,
    pub summary: String,
    /// A timed event's start and end.
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    /// An all-day event's first day and the day after its last, `yyyy-MM-dd`.
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub updated_ms: Option<i64>,
    /// `extendedProperties.private.taktTaskId`.
    pub task_id: Option<String>,
    pub html_link: Option<String>,
}

impl GoogleCalendarEvent {
    fn is_cancelled(&self) -> bool {
        self.status == "cancelled"
    }

    fn timed(&self) -> Option<(i64, i64)> {
        Some((self.start_ms?, self.end_ms?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GoogleCalendarEventsPage {
    pub events: Vec<GoogleCalendarEvent>,
    pub next_page_token: Option<String>,
    /// On the last page of a change feed: where the next pass starts.
    pub next_sync_token: Option<String>,
}

fn invalid(detail: impl Into<String>) -> CoreError {
    CoreError::GoogleCalendar {
        detail: detail.into(),
    }
}

/// Percent-encodes everything but RFC 3986's unreserved characters.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn rfc3339(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms)
        .single()
        .unwrap_or_default()
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn parse_instant(text: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|at| at.timestamp_millis())
}

fn headers(access_token: &str, has_body: bool) -> Vec<GoogleCalendarHeader> {
    let mut out = vec![GoogleCalendarHeader {
        name: "Authorization".into(),
        value: format!("Bearer {access_token}"),
    }];
    if has_body {
        out.push(GoogleCalendarHeader {
            name: "Content-Type".into(),
            value: "application/json".into(),
        });
    }
    out
}

fn query(base: String, pairs: &[(&str, Option<String>)]) -> String {
    let parts: Vec<String> = pairs
        .iter()
        .filter_map(|(key, value)| value.as_ref().map(|v| format!("{key}={}", encode(v))))
        .collect();
    if parts.is_empty() {
        base
    } else {
        format!("{base}?{}", parts.join("&"))
    }
}

fn events_url(calendar_id: &str) -> String {
    format!("{API}/calendars/{}/events", encode(calendar_id))
}

fn event_url(calendar_id: &str, event_id: &str) -> String {
    format!("{}/{}", events_url(calendar_id), encode(event_id))
}

/// The request for a read.
#[uniffi::export]
pub fn google_calendar_request(
    call: GoogleCalendarCall,
    access_token: String,
) -> GoogleCalendarHttpRequest {
    let url = match call {
        GoogleCalendarCall::Calendars { page_token } => query(
            format!("{API}/users/me/calendarList"),
            &[
                ("maxResults", Some("250".into())),
                ("pageToken", page_token),
            ],
        ),
        GoogleCalendarCall::Changes {
            calendar_id,
            sync_token,
            page_token,
        } => query(
            events_url(&calendar_id),
            &[
                ("showDeleted", Some("true".into())),
                ("maxResults", Some("250".into())),
                ("syncToken", sync_token),
                ("pageToken", page_token),
            ],
        ),
        GoogleCalendarCall::Window {
            calendar_id,
            from_ms,
            to_ms,
            page_token,
        } => query(
            events_url(&calendar_id),
            &[
                ("singleEvents", Some("true".into())),
                ("orderBy", Some("startTime".into())),
                ("maxResults", Some("250".into())),
                ("timeMin", Some(rfc3339(from_ms))),
                ("timeMax", Some(rfc3339(to_ms))),
                ("pageToken", page_token),
            ],
        ),
    };
    GoogleCalendarHttpRequest {
        method: "GET".into(),
        url,
        headers: headers(&access_token, false),
        body: None,
    }
}

fn parse_json(body: &str) -> Result<Value, CoreError> {
    serde_json::from_str(body)
        .map_err(|error| invalid(format!("Unreadable Google answer: {error}")))
}

fn string_at(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

/// A page of `calendarList`.
#[uniffi::export]
pub fn google_calendar_calendars(body: String) -> Result<GoogleCalendarCalendarsPage, CoreError> {
    let value = parse_json(&body)?;
    let calendars = value
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let role = string_at(item, "accessRole").unwrap_or_default();
                    Some(GoogleCalendarInfo {
                        id: string_at(item, "id")?,
                        summary: string_at(item, "summaryOverride")
                            .or_else(|| string_at(item, "summary"))
                            .unwrap_or_default(),
                        is_primary: item.get("primary").and_then(Value::as_bool) == Some(true),
                        can_write: role == "owner" || role == "writer",
                        color: string_at(item, "backgroundColor"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(GoogleCalendarCalendarsPage {
        calendars,
        next_page_token: string_at(&value, "nextPageToken"),
    })
}

fn event_from(value: &Value, calendar_id: &str) -> Option<GoogleCalendarEvent> {
    let start = value.get("start");
    let end = value.get("end");
    let instant = |side: Option<&Value>| {
        side.and_then(|s| s.get("dateTime"))
            .and_then(Value::as_str)
            .and_then(parse_instant)
    };
    let day = |side: Option<&Value>| {
        side.and_then(|s| s.get("date"))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    Some(GoogleCalendarEvent {
        id: string_at(value, "id")?,
        calendar_id: calendar_id.to_string(),
        status: string_at(value, "status").unwrap_or_else(|| "confirmed".into()),
        summary: string_at(value, "summary").unwrap_or_default(),
        start_ms: instant(start),
        end_ms: instant(end),
        start_date: day(start),
        end_date: day(end),
        updated_ms: string_at(value, "updated")
            .as_deref()
            .and_then(parse_instant),
        task_id: value
            .pointer("/extendedProperties/private")
            .and_then(|p| p.get(TASK_PROPERTY))
            .and_then(Value::as_str)
            .map(str::to_string),
        html_link: string_at(value, "htmlLink"),
    })
}

/// A page of a calendar's events, from a change feed or a window.
#[uniffi::export]
pub fn google_calendar_events_page(
    calendar_id: String,
    body: String,
) -> Result<GoogleCalendarEventsPage, CoreError> {
    let value = parse_json(&body)?;
    let events = value
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| event_from(item, &calendar_id))
                .collect()
        })
        .unwrap_or_default();
    Ok(GoogleCalendarEventsPage {
        events,
        next_page_token: string_at(&value, "nextPageToken"),
        next_sync_token: string_at(&value, "nextSyncToken"),
    })
}

/// The id Takt gives a task's event: `takt` and 32 hex digits of
/// `SHA-256("takt.calendar:<taskId>")`, inside Google's base32hex alphabet.
/// Every device derives the same one, so two of them cannot make two events.
#[uniffi::export]
pub fn google_calendar_event_id(task_id: String) -> String {
    let digest = Sha256::digest(format!("takt.calendar:{task_id}").as_bytes());
    let hex: String = digest[..16].iter().map(|b| format!("{b:02x}")).collect();
    format!("takt{hex}")
}

// MARK: - Settings and the lease

/// The account's choices, kept as synced preferences.
#[derive(Debug, Clone, PartialEq, Eq, Default, uniffi::Record)]
pub struct GoogleCalendarSettings {
    /// The two-way calendar; none switches two-way sync off.
    pub calendar_id: Option<String>,
    /// Whether events made in Google become tasks.
    pub import_events: bool,
    /// The calendars Today shows.
    pub agenda_calendar_ids: Vec<String>,
}

/// What a pass works on, once this device holds the lease.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GoogleCalendarPass {
    pub calendar_id: String,
    /// None: list everything.
    pub sync_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Lease {
    device: String,
    until: i64,
}

fn preference(connection: &Connection, key: &str) -> Result<Option<String>, CoreError> {
    Ok(connection
        .query_row(
            "SELECT value FROM preferences WHERE key = ?1",
            [key],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten())
}

fn settings(connection: &Connection) -> Result<GoogleCalendarSettings, CoreError> {
    Ok(GoogleCalendarSettings {
        calendar_id: preference(connection, PREF_CALENDAR)?.filter(|id| !id.trim().is_empty()),
        import_events: preference(connection, PREF_IMPORT)?.as_deref() == Some("true"),
        agenda_calendar_ids: preference(connection, PREF_AGENDA)?
            .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
            .unwrap_or_default(),
    })
}

fn device_id(connection: &Connection) -> Result<String, CoreError> {
    Ok(connection
        .query_row("SELECT deviceId FROM sync_state WHERE id = 0", [], |row| {
            row.get::<_, Option<String>>(0)
        })
        .optional()?
        .flatten()
        .unwrap_or_else(|| "local".into()))
}

/// Whether `device` may push, and the lease to store if it should be
/// written: taken when free or expired, renewed once half of it has gone.
pub fn decide_lease(current: Option<&str>, device: &str, now_ms: i64) -> (bool, Option<String>) {
    let held = current.and_then(|json| serde_json::from_str::<Lease>(json).ok());
    let fresh = || {
        serde_json::to_string(&Lease {
            device: device.to_string(),
            until: now_ms + LEASE_MS,
        })
        .ok()
    };
    match held {
        Some(lease) if lease.device != device && lease.until > now_ms => (false, None),
        Some(lease) if lease.device == device && lease.until - now_ms > LEASE_MS / 2 => {
            (true, None)
        }
        _ => (true, fresh()),
    }
}

// MARK: - The merge

/// What was last agreed with Google for one task: `pushedJSON`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agreed {
    pub title: String,
    #[serde(rename = "startMs")]
    pub start: i64,
    #[serde(rename = "endMs")]
    pub end: i64,
    pub done: bool,
}

impl Agreed {
    fn summary(&self) -> String {
        if self.done {
            format!("{DONE_PREFIX}{}", self.title)
        } else {
            self.title.clone()
        }
    }

    fn json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// The event starts at the task's start.
    Start,
    /// The event ends at the task's due time.
    Due,
}

/// A task as the merge sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalTask {
    pub id: String,
    pub title: String,
    /// The task's event, if it should have one now.
    pub shape: Option<(Anchor, i64, i64)>,
    pub is_open: bool,
    pub done: bool,
    /// The task's last write.
    pub changed_ms: i64,
}

impl LocalTask {
    fn desired(&self) -> Option<Agreed> {
        let (_, start, end) = self.shape?;
        Some(Agreed {
            title: self.title.clone(),
            start,
            end,
            done: self.done,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub task_id: String,
    pub event_id: String,
    pub detached: bool,
    pub agreed: Option<Agreed>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Insert the event under its derived id (or bring it back).
    Create { task_id: String, values: Agreed },
    /// Write `values` over the event.
    Update {
        task_id: String,
        event_id: String,
        values: Agreed,
    },
    /// Delete the event, then the link.
    Delete { task_id: String, event_id: String },
    /// Forget the link; the event is already gone.
    DropLink { task_id: String },
    /// The event was deleted in Google: keep the task, stop following it.
    Detach {
        task_id: String,
        event_id: String,
        values: Agreed,
    },
    /// Google's side won: write it onto the task.
    WriteTask {
        task_id: String,
        title: String,
        start: i64,
        end: i64,
    },
    /// Record agreement without sending anything.
    Agree {
        task_id: String,
        event_id: String,
        values: Agreed,
        remote_updated_ms: Option<i64>,
    },
    /// An event made in Google becomes a task.
    Import { event: GoogleCalendarEvent },
}

fn strip_done(summary: &str) -> &str {
    summary.strip_prefix(DONE_PREFIX).unwrap_or(summary)
}

/// Settles one field group: whichever side moved from `base`, or, when both
/// did, the later writer, Takt on a tie.
fn settle<T: PartialEq + Clone>(
    base: Option<&T>,
    local: &T,
    remote: &T,
    local_ms: i64,
    remote_ms: Option<i64>,
) -> T {
    if local == remote {
        return local.clone();
    }
    let local_moved = base != Some(local);
    let remote_moved = base != Some(remote);
    match (local_moved, remote_moved) {
        (true, false) => local.clone(),
        (false, true) => remote.clone(),
        _ => {
            if remote_ms.is_some_and(|remote_ms| remote_ms > local_ms) {
                remote.clone()
            } else {
                local.clone()
            }
        }
    }
}

fn merge(
    task: &LocalTask,
    local: &Agreed,
    base: Option<&Agreed>,
    event: &GoogleCalendarEvent,
    out: &mut Vec<Decision>,
) {
    let remote_title = strip_done(&event.summary).to_string();
    let local_time = (local.start, local.end);
    // An all-day event says nothing about the time: as if unchanged.
    let remote_time = event
        .timed()
        .or(base.map(|b| (b.start, b.end)))
        .unwrap_or(local_time);
    let title = settle(
        base.map(|b| &b.title),
        &local.title,
        &remote_title,
        task.changed_ms,
        event.updated_ms,
    );
    let base_time = base.map(|b| (b.start, b.end));
    let time = settle(
        base_time.as_ref(),
        &local_time,
        &remote_time,
        task.changed_ms,
        event.updated_ms,
    );
    let result = Agreed {
        title,
        start: time.0,
        end: time.1,
        done: local.done,
    };
    if result.title != local.title || time != local_time {
        out.push(Decision::WriteTask {
            task_id: task.id.clone(),
            title: result.title.clone(),
            start: result.start,
            end: result.end,
        });
    }
    if result.summary() != event.summary || event.timed() != Some(time) {
        out.push(Decision::Update {
            task_id: task.id.clone(),
            event_id: event.id.clone(),
            values: result,
        });
    } else {
        out.push(Decision::Agree {
            task_id: task.id.clone(),
            event_id: event.id.clone(),
            values: result,
            remote_updated_ms: event.updated_ms,
        });
    }
}

/// Works out one pass. `events` are the changed events (every event, when
/// `full`); an event missing from an incremental pass is unchanged.
pub fn decide(
    tasks: &[LocalTask],
    links: &[Link],
    events: &[GoogleCalendarEvent],
    full: bool,
    import: bool,
    now_ms: i64,
) -> Vec<Decision> {
    let tasks_by_id: HashMap<&str, &LocalTask> = tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let links_by_task: HashMap<&str, &Link> =
        links.iter().map(|l| (l.task_id.as_str(), l)).collect();
    let task_by_event: HashMap<&str, &str> = links
        .iter()
        .map(|l| (l.event_id.as_str(), l.task_id.as_str()))
        .collect();

    let mut remote_by_task: HashMap<String, &GoogleCalendarEvent> = HashMap::new();
    let mut unclaimed = Vec::new();
    for event in events {
        let owner = task_by_event
            .get(event.id.as_str())
            .map(|id| id.to_string())
            .or_else(|| {
                event.task_id.clone().filter(|id| {
                    tasks_by_id.contains_key(id.as_str()) || links_by_task.contains_key(id.as_str())
                })
            });
        match owner {
            Some(task_id) => {
                remote_by_task.insert(task_id, event);
            }
            None if event.task_id.is_none() => unclaimed.push(event),
            // Ours, for a task this device has not seen yet: leave it.
            None => {}
        }
    }

    let mut ids: BTreeSet<String> = links_by_task.keys().map(|k| k.to_string()).collect();
    ids.extend(
        tasks
            .iter()
            .filter(|t| t.shape.is_some())
            .map(|t| t.id.clone()),
    );
    ids.extend(remote_by_task.keys().cloned());

    let mut out = Vec::new();
    for id in ids {
        let task = tasks_by_id.get(id.as_str()).copied();
        let link = links_by_task.get(id.as_str()).copied();
        let remote = remote_by_task.get(&id).copied();
        let desired = task.and_then(LocalTask::desired);
        let fresh = |values: &Agreed| values.end >= now_ms - CREATE_HORIZON_MS;

        match (link, desired) {
            // No link yet.
            (None, Some(values)) => {
                let task = task.expect("desired comes from a task");
                match remote {
                    Some(event) if !event.is_cancelled() => {
                        merge(task, &values, None, event, &mut out)
                    }
                    _ if task.is_open && fresh(&values) => out.push(Decision::Create {
                        task_id: id.clone(),
                        values,
                    }),
                    _ => {}
                }
            }
            (None, None) => {
                // An event of ours whose task exists but should not have one.
                if let (Some(_), Some(event)) = (task, remote)
                    && !event.is_cancelled()
                {
                    out.push(Decision::Delete {
                        task_id: id.clone(),
                        event_id: event.id.clone(),
                    });
                }
            }
            // The task no longer wants an event.
            (Some(link), None) => {
                if link.detached || remote.is_some_and(GoogleCalendarEvent::is_cancelled) {
                    out.push(Decision::DropLink {
                        task_id: id.clone(),
                    });
                } else {
                    out.push(Decision::Delete {
                        task_id: id.clone(),
                        event_id: link.event_id.clone(),
                    });
                }
            }
            (Some(link), Some(values)) => {
                let task = task.expect("desired comes from a task");
                let base = link.agreed.as_ref();
                match remote {
                    Some(event) if !event.is_cancelled() => {
                        merge(task, &values, base, event, &mut out)
                    }
                    _ if link.detached => {
                        let moved =
                            base.is_none_or(|b| (b.start, b.end) != (values.start, values.end));
                        if moved && task.is_open {
                            out.push(Decision::Create {
                                task_id: id.clone(),
                                values,
                            });
                        }
                    }
                    Some(_) => out.push(Decision::Detach {
                        task_id: id.clone(),
                        event_id: link.event_id.clone(),
                        values,
                    }),
                    None if full => out.push(Decision::Detach {
                        task_id: id.clone(),
                        event_id: link.event_id.clone(),
                        values,
                    }),
                    None => {
                        if base != Some(&values) {
                            out.push(Decision::Update {
                                task_id: id.clone(),
                                event_id: link.event_id.clone(),
                                values,
                            });
                        }
                    }
                }
            }
        }
    }

    if import {
        for event in unclaimed {
            if !event.is_cancelled()
                && let Some((_, end)) = event.timed()
                && end >= now_ms - CREATE_HORIZON_MS
                && !strip_done(&event.summary).trim().is_empty()
            {
                out.push(Decision::Import {
                    event: event.clone(),
                });
            }
        }
    }
    out
}

// MARK: - Writes to send

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GoogleCalendarPushKind {
    Create,
    Update,
    Delete,
    /// Writes the task id onto an imported event.
    Tag,
}

/// One write for the client to send, then hand back with its answer.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GoogleCalendarPush {
    pub task_id: String,
    pub calendar_id: String,
    pub event_id: String,
    pub kind: GoogleCalendarPushKind,
    pub request: GoogleCalendarHttpRequest,
    /// Sent instead when `request` answers `409`: the derived id is taken,
    /// by another device's insert or an event deleted earlier, so it is
    /// patched back to life.
    pub on_conflict: Option<GoogleCalendarHttpRequest>,
    /// What the link records once it has been sent (`pushedJSON`).
    pub agreed_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, uniffi::Record)]
pub struct GoogleCalendarReconcile {
    pub pushes: Vec<GoogleCalendarPush>,
    /// Tasks moved or renamed from Google.
    pub tasks_updated: u32,
    /// Tasks made from events (only with import on).
    pub tasks_imported: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum GoogleCalendarPushOutcome {
    Done,
    /// The grant was refused: sign in again.
    Unauthorized,
    /// Left as it was; the next pass tries again.
    Failed {
        detail: String,
    },
}

fn event_body(values: &Agreed, zone: &str) -> Value {
    json!({
        "summary": values.summary(),
        "start": {"dateTime": rfc3339(values.start), "timeZone": zone},
        "end": {"dateTime": rfc3339(values.end), "timeZone": zone},
    })
}

fn tag(task_id: &str) -> Value {
    json!({"private": {TASK_PROPERTY: task_id}})
}

fn write(method: &str, url: String, body: Option<Value>, token: &str) -> GoogleCalendarHttpRequest {
    GoogleCalendarHttpRequest {
        method: method.into(),
        url,
        headers: headers(token, body.is_some()),
        body: body.map(|b| b.to_string()),
    }
}

fn create_push(
    task_id: &str,
    calendar_id: &str,
    values: &Agreed,
    zone: &str,
    token: &str,
) -> GoogleCalendarPush {
    let event_id = google_calendar_event_id(task_id.to_string());
    let mut body = event_body(values, zone);
    body["id"] = json!(event_id);
    body["extendedProperties"] = tag(task_id);
    let mut revive = event_body(values, zone);
    revive["status"] = json!("confirmed");
    revive["extendedProperties"] = tag(task_id);
    GoogleCalendarPush {
        task_id: task_id.into(),
        calendar_id: calendar_id.into(),
        event_id: event_id.clone(),
        kind: GoogleCalendarPushKind::Create,
        request: write("POST", events_url(calendar_id), Some(body), token),
        on_conflict: Some(write(
            "PATCH",
            event_url(calendar_id, &event_id),
            Some(revive),
            token,
        )),
        agreed_json: Some(values.json()),
    }
}

// MARK: - The workspace

fn local_tasks(
    connection: &Connection,
    only: Option<&[String]>,
) -> Result<Vec<LocalTask>, CoreError> {
    let mut statement = connection.prepare(
        "SELECT t.id, t.status, t.archivedAt IS NOT NULL OR l.isArchived = 1,
                COALESCE(t.itemKind, 'task'), t.updatedAt, m.updatedAt
         FROM tasks t
         JOIN task_lists l ON l.id = t.listId
         LEFT JOIN task_metadata m ON m.taskId = t.id
         WHERE m.startAt IS NOT NULL OR t.dueAt IS NOT NULL
            OR t.id IN (SELECT taskId FROM calendar_event_links)",
    )?;
    type Row = (String, String, bool, String, String, Option<String>);
    let rows: Vec<Row> = statement
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for (id, status, archived, kind, task_updated, metadata_updated) in rows {
        if only.is_some_and(|ids| !ids.contains(&id)) {
            continue;
        }
        let snapshot = editor::snapshot(connection, &id)?;
        let duration = snapshot
            .estimate_seconds
            .filter(|e| *e > 0)
            .map(|e| e * 1000)
            .unwrap_or(DEFAULT_DURATION_MS);
        let planning = snapshot.planning.as_ref();
        let due_is_time = planning.is_none_or(|p| p.due_date.is_none());
        let shape = match (planning.and_then(|p| p.start_at_ms), snapshot.due_at_ms) {
            (Some(start), _) => Some((Anchor::Start, start, start + duration)),
            (None, Some(due)) if due_is_time => Some((Anchor::Due, due - duration, due)),
            _ => None,
        };
        let eligible = !archived && kind == "task" && status != "cancelled";
        let changed_ms = [Some(task_updated), metadata_updated]
            .into_iter()
            .flatten()
            .filter_map(|text| parse_stored(&text))
            .map(|at| at.timestamp_millis())
            .max()
            .unwrap_or(0);
        out.push(LocalTask {
            id,
            title: snapshot.title,
            shape: shape.filter(|_| eligible),
            is_open: status == "open",
            done: status == "completed",
            changed_ms,
        });
    }
    Ok(out)
}

fn links(connection: &Connection, calendar_id: &str) -> Result<Vec<Link>, CoreError> {
    let mut statement = connection.prepare(
        "SELECT taskId, eventId, state, pushedJSON FROM calendar_event_links WHERE calendarId = ?1",
    )?;
    let rows = statement
        .query_map([calendar_id], |row| {
            let json: Option<String> = row.get(3)?;
            Ok(Link {
                task_id: row.get(0)?,
                event_id: row.get(1)?,
                detached: row.get::<_, String>(2)? == "detached",
                agreed: json.and_then(|j| serde_json::from_str(&j).ok()),
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

#[allow(clippy::too_many_arguments)]
fn save_link(
    transaction: &Transaction,
    task_id: &str,
    calendar_id: &str,
    event_id: &str,
    state: &str,
    agreed_json: Option<&str>,
    remote_updated_ms: Option<i64>,
    now_ms: i64,
) -> Result<(), CoreError> {
    transaction.execute(
        "INSERT INTO calendar_event_links
           (taskId, calendarId, eventId, state, pushedJSON, remoteUpdatedAt, createdAt, updatedAt)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
         ON CONFLICT(taskId) DO UPDATE SET calendarId = excluded.calendarId,
           eventId = excluded.eventId, state = excluded.state, pushedJSON = excluded.pushedJSON,
           remoteUpdatedAt = COALESCE(excluded.remoteUpdatedAt, remoteUpdatedAt),
           updatedAt = excluded.updatedAt
         WHERE calendarId IS NOT excluded.calendarId OR eventId IS NOT excluded.eventId
            OR state IS NOT excluded.state OR pushedJSON IS NOT excluded.pushedJSON",
        params![
            task_id,
            calendar_id,
            event_id,
            state,
            agreed_json,
            remote_updated_ms.map(stored),
            stored(now_ms),
        ],
    )?;
    Ok(())
}

fn drop_link(transaction: &Transaction, task_id: &str) -> Result<(), CoreError> {
    transaction.execute(
        "DELETE FROM calendar_event_links WHERE taskId = ?1",
        [task_id],
    )?;
    Ok(())
}

/// Writes Google's title and time onto a task through the editor, so every
/// rule a person's edit meets applies. False when the editor refuses it.
fn write_task(
    transaction: &Transaction,
    task: &LocalTask,
    title: &str,
    start: i64,
    end: i64,
    now_ms: i64,
    zone: &str,
) -> Result<bool, CoreError> {
    let Some((anchor, old_start, old_end)) = task.shape else {
        return Ok(false);
    };
    let baseline = editor::snapshot(transaction, &task.id)?;
    let mut edit = baseline.clone();
    edit.title = title.to_string();
    if (start, end) != (old_start, old_end) {
        let length = end - start;
        let had_estimate = baseline.estimate_seconds.is_some_and(|e| e > 0);
        if had_estimate || length != DEFAULT_DURATION_MS {
            edit.estimate_seconds = Some((length / 1000).max(60));
        }
        match anchor {
            Anchor::Start => {
                let mut planning = edit.planning.clone().unwrap_or_default();
                planning.start_at_ms = Some(start);
                edit.planning = planning.normalized();
            }
            Anchor::Due => edit.due_at_ms = Some(end),
        }
    }
    // A refused edit must leave nothing half-written in the pass's step.
    transaction.execute_batch("SAVEPOINT google_calendar_write")?;
    match editor::save_editor(transaction, &edit, &baseline, now_ms, zone) {
        Ok(_) => {
            transaction.execute_batch("RELEASE google_calendar_write")?;
            Ok(true)
        }
        Err(error) => {
            transaction.execute_batch(
                "ROLLBACK TO google_calendar_write; RELEASE google_calendar_write",
            )?;
            match error {
                CoreError::Database { detail } => Err(CoreError::Database { detail }),
                _ => Ok(false),
            }
        }
    }
}

fn import_workspace(transaction: &Transaction) -> Result<Option<String>, CoreError> {
    let canonical: Option<String> = transaction
        .query_row(
            "SELECT canonicalWorkspaceId FROM sync_state WHERE id = 0",
            [],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    if canonical.is_some() {
        return Ok(canonical);
    }
    Ok(transaction
        .query_row(
            "SELECT id FROM workspaces ORDER BY createdAt, id LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GoogleCalendarAgendaKind {
    Event,
    Task,
}

/// One line of a day's schedule.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GoogleCalendarAgendaEntry {
    pub kind: GoogleCalendarAgendaKind,
    /// The event's or the task's id.
    pub id: String,
    pub title: String,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub all_day: bool,
    pub calendar_id: Option<String>,
    pub is_done: bool,
    pub html_link: Option<String>,
}

#[uniffi::export]
impl CoreWorkspace {
    /// The account's calendar choices.
    pub fn google_calendar_settings(&self) -> Result<GoogleCalendarSettings, CoreError> {
        settings(&self.lock())
    }

    /// Stores the account's calendar choices. Choosing another two-way
    /// calendar forgets the links to the old one's events, which stay there.
    pub fn set_google_calendar_settings(
        &self,
        settings_: GoogleCalendarSettings,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        let mut connection = self.lock();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous = settings(&transaction)?;
        let calendar = settings_.calendar_id.filter(|id| !id.trim().is_empty());
        if previous.calendar_id != calendar {
            transaction.execute(
                "DELETE FROM calendar_event_links WHERE calendarId IS NOT ?1",
                [&calendar],
            )?;
        }
        let agenda = serde_json::to_string(&settings_.agenda_calendar_ids).unwrap_or_default();
        crate::setup::set_preference(&transaction, PREF_CALENDAR, calendar.as_deref(), now_ms)?;
        crate::setup::set_preference(
            &transaction,
            PREF_IMPORT,
            Some(if settings_.import_events {
                "true"
            } else {
                "false"
            }),
            now_ms,
        )?;
        crate::setup::set_preference(&transaction, PREF_AGENDA, Some(&agenda), now_ms)?;
        transaction.commit()?;
        Ok(())
    }

    /// Starts a pass: None when two-way sync is off or another device holds
    /// the lease, otherwise the calendar and where its feed was left.
    pub fn google_calendar_begin_pass(
        &self,
        now_ms: i64,
    ) -> Result<Option<GoogleCalendarPass>, CoreError> {
        let mut connection = self.lock();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(calendar_id) = settings(&transaction)?.calendar_id else {
            return Ok(None);
        };
        let device = device_id(&transaction)?;
        let current = preference(&transaction, PREF_LEASE)?;
        let (may_push, renewed) = decide_lease(current.as_deref(), &device, now_ms);
        if !may_push {
            return Ok(None);
        }
        if let Some(lease) = renewed {
            crate::setup::set_preference(&transaction, PREF_LEASE, Some(&lease), now_ms)?;
        }
        let sync_token = transaction
            .query_row(
                "SELECT syncToken FROM calendar_sync_tokens WHERE calendarId = ?1",
                [&calendar_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();
        transaction.commit()?;
        Ok(Some(GoogleCalendarPass {
            calendar_id,
            sync_token,
        }))
    }

    /// Where a calendar's feed carries on from; None starts it again.
    pub fn set_google_calendar_sync_token(
        &self,
        calendar_id: String,
        sync_token: Option<String>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        self.lock().execute(
            "INSERT INTO calendar_sync_tokens (calendarId, syncToken, updatedAt) VALUES (?1, ?2, ?3)
             ON CONFLICT(calendarId) DO UPDATE SET syncToken = excluded.syncToken,
               updatedAt = excluded.updatedAt",
            params![calendar_id, sync_token, stored(now_ms)],
        )?;
        Ok(())
    }

    /// Merges a pass's changed events (every event when `full`) with the
    /// workspace. Changes to tasks are one "Google Calendar" undo step;
    /// the writes Google needs come back to be sent, each then handed to
    /// [`Self::google_calendar_record_push`].
    pub fn google_calendar_reconcile(
        &self,
        calendar_id: String,
        events: Vec<GoogleCalendarEvent>,
        full: bool,
        access_token: String,
        now_ms: i64,
        zone: String,
    ) -> Result<GoogleCalendarReconcile, CoreError> {
        journal::journalled(&mut self.lock(), "Google Calendar", |tx| {
            let import = settings(tx)?.import_events;
            let tasks = local_tasks(tx, None)?;
            let links = links(tx, &calendar_id)?;
            let decisions = decide(&tasks, &links, &events, full, import, now_ms);
            let by_id: HashMap<&str, &LocalTask> =
                tasks.iter().map(|t| (t.id.as_str(), t)).collect();
            let mut result = GoogleCalendarReconcile::default();
            let token = access_token.as_str();
            let cal = calendar_id.as_str();
            // Tasks whose write from Google the editor refused: their own
            // values have gone back over the event instead.
            let mut refused: Vec<String> = Vec::new();
            for decision in decisions {
                match decision {
                    Decision::Create { task_id, values } => {
                        result
                            .pushes
                            .push(create_push(&task_id, cal, &values, &zone, token));
                    }
                    Decision::Update {
                        task_id,
                        event_id,
                        values,
                    } => {
                        if refused.contains(&task_id) {
                            continue;
                        }
                        result.pushes.push(GoogleCalendarPush {
                            task_id,
                            calendar_id: cal.into(),
                            event_id: event_id.clone(),
                            kind: GoogleCalendarPushKind::Update,
                            request: write(
                                "PATCH",
                                event_url(cal, &event_id),
                                Some(event_body(&values, &zone)),
                                token,
                            ),
                            on_conflict: None,
                            agreed_json: Some(values.json()),
                        });
                    }
                    Decision::Delete { task_id, event_id } => {
                        result.pushes.push(GoogleCalendarPush {
                            task_id,
                            calendar_id: cal.into(),
                            event_id: event_id.clone(),
                            kind: GoogleCalendarPushKind::Delete,
                            request: write("DELETE", event_url(cal, &event_id), None, token),
                            on_conflict: None,
                            agreed_json: None,
                        });
                    }
                    Decision::DropLink { task_id } => drop_link(tx, &task_id)?,
                    Decision::Detach {
                        task_id,
                        event_id,
                        values,
                    } => save_link(
                        tx,
                        &task_id,
                        cal,
                        &event_id,
                        "detached",
                        Some(&values.json()),
                        None,
                        now_ms,
                    )?,
                    Decision::WriteTask {
                        task_id,
                        title,
                        start,
                        end,
                    } => {
                        let Some(task) = by_id.get(task_id.as_str()) else {
                            continue;
                        };
                        if write_task(tx, task, &title, start, end, now_ms, &zone)? {
                            result.tasks_updated += 1;
                        } else if let Some(values) = task.desired() {
                            // Refused (a start past its due date): Takt's
                            // values go back over Google's.
                            let event_id = links
                                .iter()
                                .find(|l| l.task_id == task_id)
                                .map(|l| l.event_id.clone())
                                .unwrap_or_else(|| google_calendar_event_id(task_id.clone()));
                            refused.push(task_id.clone());
                            result.pushes.push(GoogleCalendarPush {
                                task_id,
                                calendar_id: cal.into(),
                                event_id: event_id.clone(),
                                kind: GoogleCalendarPushKind::Update,
                                request: write(
                                    "PATCH",
                                    event_url(cal, &event_id),
                                    Some(event_body(&values, &zone)),
                                    token,
                                ),
                                on_conflict: None,
                                agreed_json: Some(values.json()),
                            });
                        }
                    }
                    Decision::Agree { task_id, .. } if refused.contains(&task_id) => {}
                    Decision::Agree {
                        task_id,
                        event_id,
                        values,
                        remote_updated_ms,
                    } => save_link(
                        tx,
                        &task_id,
                        cal,
                        &event_id,
                        "linked",
                        Some(&values.json()),
                        remote_updated_ms,
                        now_ms,
                    )?,
                    Decision::Import { event } => {
                        let Some(workspace) = import_workspace(tx)? else {
                            continue;
                        };
                        let Some(inbox) = crate::records::inbox(tx, &workspace)? else {
                            continue;
                        };
                        let Some((start, end)) = event.timed() else {
                            continue;
                        };
                        let title = strip_done(&event.summary).trim().to_string();
                        let length = end - start;
                        let task_id = tasks::create_task(
                            tx,
                            &NewTask {
                                list_id: inbox.id,
                                title: title.clone(),
                                parent_task_id: None,
                                kind: "task".into(),
                                notes: String::new(),
                                kanban_column: None,
                                start_at_ms: Some(start),
                                due_at_ms: None,
                                estimate_seconds: (length != DEFAULT_DURATION_MS)
                                    .then_some((length / 1000).max(60)),
                                tags: vec![],
                                priority: None,
                                waiting_on: None,
                                external_links: event.html_link.clone().into_iter().collect(),
                                at_top: false,
                                adjacent_task_id: None,
                                above: false,
                            },
                            now_ms,
                        )?;
                        let values = Agreed {
                            title,
                            start,
                            end,
                            done: false,
                        };
                        result.tasks_imported += 1;
                        result.pushes.push(GoogleCalendarPush {
                            task_id: task_id.clone(),
                            calendar_id: cal.into(),
                            event_id: event.id.clone(),
                            kind: GoogleCalendarPushKind::Tag,
                            request: write(
                                "PATCH",
                                event_url(cal, &event.id),
                                Some(json!({"extendedProperties": tag(&task_id)})),
                                token,
                            ),
                            on_conflict: None,
                            agreed_json: Some(values.json()),
                        });
                    }
                }
            }
            Ok(result)
        })
    }

    /// Files a sent write's answer: `status` is the HTTP status of the last
    /// request sent for it (`on_conflict`'s, when that was sent), `body`
    /// what came back.
    pub fn google_calendar_record_push(
        &self,
        push: GoogleCalendarPush,
        status: u16,
        body: String,
        now_ms: i64,
    ) -> Result<GoogleCalendarPushOutcome, CoreError> {
        if status == 401 {
            return Ok(GoogleCalendarPushOutcome::Unauthorized);
        }
        let gone = status == 404 || status == 410;
        let ok = (200..300).contains(&status);
        if !ok && !gone {
            let detail = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v.pointer("/error/message")?.as_str().map(str::to_string))
                .unwrap_or_else(|| format!("Google Calendar answered {status}."));
            return Ok(GoogleCalendarPushOutcome::Failed { detail });
        }
        let mut connection = self.lock();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let remote_updated = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|v| v.get("updated")?.as_str().and_then(parse_instant));
        let task = &push.task_id;
        let event_id = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|v| string_at(&v, "id"))
            .unwrap_or_else(|| push.event_id.clone());
        let outcome = match (push.kind, gone) {
            (GoogleCalendarPushKind::Delete, _) => {
                drop_link(&transaction, task)?;
                GoogleCalendarPushOutcome::Done
            }
            (GoogleCalendarPushKind::Create, true) => GoogleCalendarPushOutcome::Failed {
                detail: "The calendar is gone. Choose another in settings.".into(),
            },
            (_, true) => {
                save_link(
                    &transaction,
                    task,
                    &push.calendar_id,
                    &push.event_id,
                    "detached",
                    push.agreed_json.as_deref(),
                    None,
                    now_ms,
                )?;
                GoogleCalendarPushOutcome::Done
            }
            (_, false) => {
                save_link(
                    &transaction,
                    task,
                    &push.calendar_id,
                    &event_id,
                    "linked",
                    push.agreed_json.as_deref(),
                    remote_updated,
                    now_ms,
                )?;
                GoogleCalendarPushOutcome::Done
            }
        };
        transaction.commit()?;
        Ok(outcome)
    }

    /// A window's schedule: `events` (from the agenda calendars) and the
    /// timed tasks overlapping `[from_ms, to_ms)`, all-day events first,
    /// then by start. An event that stands for a task is shown as the task.
    pub fn google_calendar_agenda(
        &self,
        events: Vec<GoogleCalendarEvent>,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<GoogleCalendarAgendaEntry>, CoreError> {
        let connection = self.lock();
        let tasks = local_tasks(&connection, None)?;
        let linked: std::collections::HashSet<String> = connection
            .prepare("SELECT eventId FROM calendar_event_links")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        drop(connection);
        let mut out: Vec<GoogleCalendarAgendaEntry> = events
            .into_iter()
            .filter(|e| !e.is_cancelled() && e.task_id.is_none() && !linked.contains(&e.id))
            .map(|e| GoogleCalendarAgendaEntry {
                kind: GoogleCalendarAgendaKind::Event,
                all_day: e.start_ms.is_none(),
                title: e.summary,
                start_ms: e.start_ms,
                end_ms: e.end_ms,
                calendar_id: Some(e.calendar_id),
                is_done: false,
                html_link: e.html_link,
                id: e.id,
            })
            .collect();
        for task in tasks {
            let Some((_, start, end)) = task.shape else {
                continue;
            };
            if end <= from_ms || start >= to_ms {
                continue;
            }
            out.push(GoogleCalendarAgendaEntry {
                kind: GoogleCalendarAgendaKind::Task,
                id: task.id,
                title: task.title,
                start_ms: Some(start),
                end_ms: Some(end),
                all_day: false,
                calendar_id: None,
                is_done: task.done,
                html_link: None,
            });
        }
        out.sort_by(|a, b| {
            b.all_day
                .cmp(&a.all_day)
                .then(a.start_ms.cmp(&b.start_ms))
                .then(
                    (a.kind == GoogleCalendarAgendaKind::Task)
                        .cmp(&(b.kind == GoogleCalendarAgendaKind::Task)),
                )
                .then(a.title.cmp(&b.title))
        });
        Ok(out)
    }
}

impl CoreWorkspace {
    /// The tasks that have an event on the two-way calendar now (linked,
    /// not detached). The Mac's Google Tasks mirror leaves them out, so a
    /// timed task shows once in Google Calendar, as its event, rather than
    /// a second time as a Google Task. Rust only: no phone mirrors Tasks.
    pub fn google_calendar_linked_task_ids(&self) -> Result<Vec<String>, CoreError> {
        crate::records::all(
            &self.read(),
            "SELECT taskId FROM calendar_event_links WHERE state = 'linked' ORDER BY taskId",
            [],
            |row| row.get(0),
        )
    }
}

#[cfg(test)]
mod tests;
