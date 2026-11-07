//! Timed tasks and the chosen Google calendar kept in step
//! (`docs/google-calendar.md`, `GoogleCalendarClient.swift`). The core
//! builds every request, merges what Google sent and says what to write
//! back; this sends it. Which device writes is the core's lease, so a Mac
//! and a phone on the same account don't both push.

use super::{CALENDAR_SCOPES, GoogleError, access_token, failure, send};
use takt_core::google_calendar::{
    self, GoogleCalendarCall, GoogleCalendarHttpRequest, GoogleCalendarInfo,
    GoogleCalendarPushOutcome,
};
use takt_core::workspace::CoreWorkspace;

const SERVICE: &str = "Google Calendar";

#[derive(Debug, Default, Clone)]
pub struct Summary {
    pub tasks_updated: u32,
    pub tasks_imported: u32,
    pub events_written: u32,
    pub failures: Vec<String>,
}

fn now_ms() -> i64 {
    crate::state::store::now_ms()
}

fn core_error(error: takt_core::CoreError) -> GoogleError {
    GoogleError::Other(error.to_string())
}

fn send_request(
    request: &GoogleCalendarHttpRequest,
) -> Result<crate::sync::http::Reply, GoogleError> {
    let headers: Vec<(&str, &str)> = request
        .headers
        .iter()
        .map(|h| (h.name.as_str(), h.value.as_str()))
        .collect();
    crate::sync::http::request(
        &request.method,
        &request.url,
        &headers,
        request.body.as_deref(),
        std::time::Duration::from_secs(60),
    )
    .map_err(GoogleError::Other)
}

/// The account's calendars, for choosing the two-way one.
pub fn calendars(client_id: &str) -> Result<Vec<GoogleCalendarInfo>, GoogleError> {
    let token = access_token(client_id, &CALENDAR_SCOPES, SERVICE)?;
    let mut out = Vec::new();
    let mut page_token = None;
    loop {
        let request = google_calendar::google_calendar_request(
            GoogleCalendarCall::Calendars { page_token },
            token.clone(),
        );
        let reply = send(&request.method, &request.url, &token, None)?;
        if !reply.ok() {
            return Err(failure(SERVICE, &reply));
        }
        let page = google_calendar::google_calendar_calendars(reply.body).map_err(core_error)?;
        out.extend(page.calendars);
        match page.next_page_token {
            Some(next) => page_token = Some(next),
            None => return Ok(out),
        }
    }
}

/// One pass. None when there is no two-way calendar, or another device
/// holds the lease.
pub fn run_pass(
    core: &CoreWorkspace,
    client_id: &str,
    zone: &str,
) -> Result<Option<Summary>, GoogleError> {
    let Some(pass) = core
        .google_calendar_begin_pass(now_ms())
        .map_err(core_error)?
    else {
        return Ok(None);
    };
    let token = access_token(client_id, &CALENDAR_SCOPES, SERVICE)?;
    let calendar_id = pass.calendar_id;
    let mut sync_token = pass.sync_token;
    let mut events = Vec::new();
    let mut page_token = None;
    let next_sync = loop {
        let request = google_calendar::google_calendar_request(
            GoogleCalendarCall::Changes {
                calendar_id: calendar_id.clone(),
                sync_token: sync_token.clone(),
                page_token: page_token.clone(),
            },
            token.clone(),
        );
        let reply = send_request(&request)?;
        if reply.status == 410 {
            // The feed forgot us: read it whole.
            core.set_google_calendar_sync_token(calendar_id.clone(), None, now_ms())
                .map_err(core_error)?;
            sync_token = None;
            page_token = None;
            events.clear();
            continue;
        }
        if !reply.ok() {
            return Err(failure(SERVICE, &reply));
        }
        let page = google_calendar::google_calendar_events_page(calendar_id.clone(), reply.body)
            .map_err(core_error)?;
        events.extend(page.events);
        match page.next_page_token {
            Some(next) => page_token = Some(next),
            None => break page.next_sync_token,
        }
    };
    let reconciled = core
        .google_calendar_reconcile(
            calendar_id.clone(),
            events,
            sync_token.is_none(),
            token.clone(),
            now_ms(),
            zone.into(),
        )
        .map_err(core_error)?;
    let mut summary = Summary {
        tasks_updated: reconciled.tasks_updated,
        tasks_imported: reconciled.tasks_imported,
        ..Default::default()
    };
    for push in reconciled.pushes {
        let mut reply = send_request(&push.request)?;
        if reply.status == 409
            && let Some(retry) = &push.on_conflict
        {
            reply = send_request(retry)?;
        }
        let outcome = core
            .google_calendar_record_push(push, reply.status, reply.body, now_ms())
            .map_err(core_error)?;
        match outcome {
            GoogleCalendarPushOutcome::Done => summary.events_written += 1,
            GoogleCalendarPushOutcome::Unauthorized => {
                return Err(GoogleError::SignIn(
                    "Google refused the sign-in. Connect Google Calendar again.".into(),
                ));
            }
            GoogleCalendarPushOutcome::Failed { detail } => summary.failures.push(detail),
        }
    }
    core.set_google_calendar_sync_token(calendar_id, next_sync, now_ms())
        .map_err(core_error)?;
    Ok(Some(summary))
}
