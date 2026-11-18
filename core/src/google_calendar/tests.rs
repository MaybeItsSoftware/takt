use std::sync::Arc;

use super::*;
use crate::schema::migrate_workspace;

const HOUR: i64 = 60 * 60 * 1000;
const NOW: i64 = 1_800_000_000_000;

fn open() -> (Arc<CoreWorkspace>, String) {
    let path = std::env::temp_dir().join(format!("takt-calendar-{}.sqlite", uuid::Uuid::new_v4()));
    let path = path.to_string_lossy().to_string();
    migrate_workspace(path.clone()).unwrap();
    let core = CoreWorkspace::open(path.clone()).unwrap();
    let workspace = core.bootstrap(NOW - 10 * HOUR).unwrap();
    let inbox = core.inbox(workspace).unwrap().unwrap().id;
    (core, inbox)
}

fn add(
    core: &CoreWorkspace,
    list: &str,
    title: &str,
    start: Option<i64>,
    due: Option<i64>,
) -> String {
    core.create_task(
        NewTask {
            list_id: list.into(),
            title: title.into(),
            parent_task_id: None,
            kind: "task".into(),
            notes: String::new(),
            kanban_column: None,
            start_at_ms: start,
            due_at_ms: due,
            estimate_seconds: None,
            tags: vec![],
            priority: None,
            waiting_on: None,
            external_links: vec![],
            at_top: false,
            adjacent_task_id: None,
            above: false,
        },
        NOW - 5 * HOUR,
    )
    .unwrap()
}

fn enable(core: &CoreWorkspace, import: bool) {
    core.set_google_calendar_settings(
        GoogleCalendarSettings {
            calendar_id: Some("takt@group.calendar.google.com".into()),
            import_events: import,
            agenda_calendar_ids: vec!["primary".into()],
        },
        NOW - 4 * HOUR,
    )
    .unwrap();
}

fn event(
    id: &str,
    summary: &str,
    start: i64,
    end: i64,
    updated: i64,
    task: Option<&str>,
) -> GoogleCalendarEvent {
    GoogleCalendarEvent {
        id: id.into(),
        calendar_id: "takt@group.calendar.google.com".into(),
        status: "confirmed".into(),
        summary: summary.into(),
        start_ms: Some(start),
        end_ms: Some(end),
        start_date: None,
        end_date: None,
        updated_ms: Some(updated),
        task_id: task.map(str::to_string),
        html_link: None,
    }
}

fn answer(push: &GoogleCalendarPush, updated: i64) -> String {
    json!({"id": push.event_id, "updated": rfc3339(updated)}).to_string()
}

fn send_all(core: &CoreWorkspace, pushes: &[GoogleCalendarPush], at: i64) {
    for push in pushes {
        let outcome = core
            .google_calendar_record_push(push.clone(), 200, answer(push, at), at)
            .unwrap();
        assert_eq!(outcome, GoogleCalendarPushOutcome::Done);
    }
}

fn reconcile(
    core: &CoreWorkspace,
    events: Vec<GoogleCalendarEvent>,
    full: bool,
    at: i64,
) -> GoogleCalendarReconcile {
    core.google_calendar_reconcile(
        "takt@group.calendar.google.com".into(),
        events,
        full,
        "token".into(),
        at,
        "Europe/London".into(),
    )
    .unwrap()
}

// MARK: - Pure parts

#[test]
fn the_event_id_is_derived_and_in_googles_alphabet() {
    let id = google_calendar_event_id("ABC-123".into());
    assert_eq!(id, google_calendar_event_id("ABC-123".into()));
    assert_ne!(id, google_calendar_event_id("ABC-124".into()));
    assert_eq!(id.len(), 36);
    assert!(
        id.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='v').contains(&c))
    );
}

#[test]
fn reads_are_built_with_the_token_and_encoded_ids() {
    let request = google_calendar_request(
        GoogleCalendarCall::Changes {
            calendar_id: "a#b@group.calendar.google.com".into(),
            sync_token: Some("tok/en=".into()),
            page_token: None,
        },
        "secret".into(),
    );
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.url,
        "https://www.googleapis.com/calendar/v3/calendars/a%23b%40group.calendar.google.com/events?showDeleted=true&maxResults=250&syncToken=tok%2Fen%3D"
    );
    assert_eq!(request.headers[0].value, "Bearer secret");
    let window = google_calendar_request(
        GoogleCalendarCall::Window {
            calendar_id: "primary".into(),
            from_ms: 0,
            to_ms: HOUR,
            page_token: Some("p".into()),
        },
        "t".into(),
    );
    assert!(window.url.contains("singleEvents=true"));
    assert!(window.url.contains("timeMin=1970-01-01T00%3A00%3A00Z"));
    assert!(window.url.contains("timeMax=1970-01-01T01%3A00%3A00Z"));
    assert!(window.url.ends_with("pageToken=p"));
}

#[test]
fn pages_of_events_and_calendars_are_read() {
    let body = r#"{
      "nextSyncToken": "next",
      "items": [
        {"id": "a", "status": "confirmed", "summary": "Lunch",
         "start": {"dateTime": "2026-10-17T12:00:00+01:00"},
         "end": {"dateTime": "2026-10-17T13:00:00+01:00"},
         "updated": "2026-10-16T09:00:00.000Z",
         "extendedProperties": {"private": {"taktTaskId": "T1"}}},
        {"id": "b", "summary": "Holiday", "start": {"date": "2026-10-17"}, "end": {"date": "2026-10-18"}},
        {"id": "c", "status": "cancelled"}
      ]}"#;
    let page = google_calendar_events_page("cal".into(), body.into()).unwrap();
    assert_eq!(page.next_sync_token.as_deref(), Some("next"));
    assert_eq!(page.events.len(), 3);
    let lunch = &page.events[0];
    assert_eq!(lunch.task_id.as_deref(), Some("T1"));
    assert_eq!(lunch.end_ms.unwrap() - lunch.start_ms.unwrap(), HOUR);
    assert_eq!(page.events[1].start_date.as_deref(), Some("2026-10-17"));
    assert!(page.events[1].start_ms.is_none());
    assert!(page.events[2].is_cancelled());
    assert!(google_calendar_events_page("cal".into(), "nope".into()).is_err());

    let calendars = google_calendar_calendars(
        r##"{"items": [
          {"id": "me@x", "summary": "Me", "primary": true, "accessRole": "owner", "backgroundColor": "#123456"},
          {"id": "hol", "summary": "Holidays", "summaryOverride": "Days off", "accessRole": "reader"}
        ]}"##
            .into(),
    )
    .unwrap();
    assert_eq!(calendars.calendars.len(), 2);
    assert!(calendars.calendars[0].is_primary && calendars.calendars[0].can_write);
    assert_eq!(calendars.calendars[1].summary, "Days off");
    assert!(!calendars.calendars[1].can_write);
}

#[test]
fn the_lease_is_taken_when_free_kept_by_its_holder_and_refused_to_others() {
    let (may, lease) = decide_lease(None, "mac", NOW);
    assert!(may);
    let lease = lease.unwrap();
    assert_eq!(decide_lease(Some(&lease), "mac", NOW + 1), (true, None));
    assert_eq!(decide_lease(Some(&lease), "phone", NOW + 1), (false, None));
    let (may, renewed) = decide_lease(Some(&lease), "mac", NOW + LEASE_MS * 3 / 4);
    assert!(may && renewed.is_some());
    let (may, taken) = decide_lease(Some(&lease), "phone", NOW + LEASE_MS + 1);
    assert!(may && taken.unwrap().contains("phone"));
}

fn task(id: &str, title: &str, start: i64, changed: i64) -> LocalTask {
    LocalTask {
        id: id.into(),
        title: title.into(),
        shape: Some((Anchor::Start, start, start + HOUR)),
        is_open: true,
        done: false,
        changed_ms: changed,
    }
}

fn link(id: &str, title: &str, start: i64) -> Link {
    Link {
        task_id: id.into(),
        event_id: format!("e-{id}"),
        detached: false,
        agreed: Some(Agreed {
            title: title.into(),
            start,
            end: start + HOUR,
            done: false,
        }),
    }
}

#[test]
fn both_sides_moving_the_same_field_go_to_the_later_writer() {
    let local = task("t", "Write", NOW + 2 * HOUR, NOW - 10);
    let base = link("t", "Write", NOW + HOUR);
    let remote = event(
        "e-t",
        "Write",
        NOW + 3 * HOUR,
        NOW + 4 * HOUR,
        NOW - 5,
        Some("t"),
    );
    let decisions = decide(
        std::slice::from_ref(&local),
        std::slice::from_ref(&base),
        &[remote],
        false,
        false,
        NOW,
    );
    assert!(
        decisions
            .iter()
            .any(|d| matches!(d, Decision::WriteTask { start, .. } if *start == NOW + 3 * HOUR))
    );

    let older = event(
        "e-t",
        "Write",
        NOW + 3 * HOUR,
        NOW + 4 * HOUR,
        NOW - 20,
        Some("t"),
    );
    let decisions = decide(&[local], &[base], &[older], false, false, NOW);
    assert!(
        decisions.iter().any(
            |d| matches!(d, Decision::Update { values, .. } if values.start == NOW + 2 * HOUR)
        )
    );
    assert!(
        !decisions
            .iter()
            .any(|d| matches!(d, Decision::WriteTask { .. }))
    );
}

#[test]
fn fields_merge_separately() {
    // Renamed in Google, moved in Takt: both survive.
    let local = task("t", "Write", NOW + 2 * HOUR, NOW);
    let base = link("t", "Write", NOW + HOUR);
    let remote = event(
        "e-t",
        "Write the report",
        NOW + HOUR,
        NOW + 2 * HOUR,
        NOW + 1,
        Some("t"),
    );
    let decisions = decide(&[local], &[base], &[remote], false, false, NOW);
    assert!(decisions.contains(&Decision::WriteTask {
        task_id: "t".into(),
        title: "Write the report".into(),
        start: NOW + 2 * HOUR,
        end: NOW + 3 * HOUR,
    }));
    assert!(
        decisions
            .iter()
            .any(|d| matches!(d, Decision::Update { values, .. }
        if values.title == "Write the report" && values.start == NOW + 2 * HOUR))
    );
}

#[test]
fn stale_tasks_are_not_created_and_unknown_events_are_not_imported_by_default() {
    let old = task("old", "Last week", NOW - 100 * HOUR, NOW);
    let new = task("new", "Soon", NOW + HOUR, NOW);
    let stranger = event("g", "Dentist", NOW + HOUR, NOW + 2 * HOUR, NOW, None);
    let decisions = decide(
        &[old, new],
        &[],
        std::slice::from_ref(&stranger),
        false,
        false,
        NOW,
    );
    assert_eq!(decisions.len(), 1);
    assert!(matches!(&decisions[0], Decision::Create { task_id, .. } if task_id == "new"));
    let decisions = decide(&[], &[], &[stranger], false, true, NOW);
    assert!(matches!(&decisions[0], Decision::Import { .. }));
}

#[test]
fn a_detached_task_comes_back_only_when_its_time_changes() {
    let mut detached = link("t", "Write", NOW + HOUR);
    detached.detached = true;
    let same = task("t", "Write", NOW + HOUR, NOW);
    assert!(
        decide(
            &[same],
            std::slice::from_ref(&detached),
            &[],
            false,
            false,
            NOW
        )
        .is_empty()
    );
    let moved = task("t", "Write", NOW + 5 * HOUR, NOW);
    let decisions = decide(&[moved], &[detached], &[], false, false, NOW);
    assert!(matches!(&decisions[0], Decision::Create { .. }));
}

#[test]
fn a_full_list_without_a_linked_event_detaches_it() {
    let local = task("t", "Write", NOW + HOUR, NOW);
    let base = link("t", "Write", NOW + HOUR);
    assert!(
        decide(
            std::slice::from_ref(&local),
            std::slice::from_ref(&base),
            &[],
            false,
            false,
            NOW
        )
        .is_empty()
    );
    let decisions = decide(&[local], &[base], &[], true, false, NOW);
    assert!(matches!(&decisions[0], Decision::Detach { .. }));
}

// MARK: - Through the workspace

#[test]
fn nothing_happens_until_a_calendar_is_chosen_and_the_lease_is_one_devices() {
    let (core, _) = open();
    assert!(core.google_calendar_begin_pass(NOW).unwrap().is_none());
    enable(&core, false);
    let pass = core.google_calendar_begin_pass(NOW).unwrap().unwrap();
    assert_eq!(pass.sync_token, None);
    core.set_google_calendar_sync_token(pass.calendar_id.clone(), Some("s1".into()), NOW)
        .unwrap();
    let pass = core.google_calendar_begin_pass(NOW + 1).unwrap().unwrap();
    assert_eq!(pass.sync_token.as_deref(), Some("s1"));
    // Another device took it.
    core.set_preference(
        PREF_LEASE.into(),
        Some(r#"{"device":"phone","until":1900000000000}"#.into()),
        NOW,
    )
    .unwrap();
    assert!(core.google_calendar_begin_pass(NOW + 2).unwrap().is_none());
    assert_eq!(
        core.google_calendar_settings().unwrap().agenda_calendar_ids,
        vec!["primary".to_string()]
    );
}

#[test]
fn a_timed_task_round_trips_with_google() {
    let (core, inbox) = open();
    enable(&core, false);
    let id = add(&core, &inbox, "Write", Some(NOW + HOUR), None);
    add(&core, &inbox, "Someday", None, None);

    // First pass: one event, under the derived id, tagged with the task.
    let first = reconcile(&core, vec![], true, NOW);
    assert_eq!(first.pushes.len(), 1);
    let push = &first.pushes[0];
    assert_eq!(push.kind, GoogleCalendarPushKind::Create);
    assert_eq!(push.event_id, google_calendar_event_id(id.clone()));
    let body: Value = serde_json::from_str(push.request.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["summary"], "Write");
    assert_eq!(
        body["extendedProperties"]["private"]["taktTaskId"],
        json!(id)
    );
    assert_eq!(body["start"]["timeZone"], "Europe/London");
    assert_eq!(push.on_conflict.as_ref().unwrap().method, "PATCH");
    send_all(&core, &first.pushes, NOW);

    // The echo of our own insert changes nothing.
    let echo = event(
        &push.event_id,
        "Write",
        NOW + HOUR,
        NOW + HOUR + DEFAULT_DURATION_MS,
        NOW,
        Some(&id),
    );
    let quiet = reconcile(&core, vec![echo], false, NOW + 1000);
    assert!(quiet.pushes.is_empty());
    assert_eq!(quiet.tasks_updated, 0);

    // Moved and lengthened in Google: the task follows, estimate included.
    let moved = event(
        &push.event_id,
        "Write",
        NOW + 3 * HOUR,
        NOW + 5 * HOUR,
        NOW + HOUR,
        Some(&id),
    );
    let pulled = reconcile(&core, vec![moved], false, NOW + 2 * HOUR);
    assert_eq!(pulled.tasks_updated, 1);
    assert!(pulled.pushes.is_empty());
    let snapshot = core.editor_snapshot(id.clone()).unwrap();
    assert_eq!(snapshot.planning.unwrap().start_at_ms, Some(NOW + 3 * HOUR));
    assert_eq!(snapshot.estimate_seconds, Some(2 * 3600));
    assert_eq!(
        core.undoable_label().unwrap().as_deref(),
        Some("Google Calendar")
    );

    // Completed here: the event is kept and ticked.
    core.set_status(
        id.clone(),
        "completed".into(),
        NOW + 3 * HOUR,
        "Europe/London".into(),
    )
    .unwrap();
    let ticked = reconcile(&core, vec![], false, NOW + 3 * HOUR);
    assert_eq!(ticked.pushes.len(), 1);
    let body: Value =
        serde_json::from_str(ticked.pushes[0].request.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["summary"], "✓ Write");
    send_all(&core, &ticked.pushes, NOW + 3 * HOUR);

    // Deleted here: the event goes, then the link.
    core.delete_task(id.clone()).unwrap();
    let gone = reconcile(&core, vec![], false, NOW + 4 * HOUR);
    assert_eq!(gone.pushes.len(), 1);
    assert_eq!(gone.pushes[0].kind, GoogleCalendarPushKind::Delete);
    assert_eq!(gone.pushes[0].request.method, "DELETE");
    core.google_calendar_record_push(gone.pushes[0].clone(), 204, String::new(), NOW + 4 * HOUR)
        .unwrap();
    assert!(
        reconcile(&core, vec![], false, NOW + 5 * HOUR)
            .pushes
            .is_empty()
    );
}

#[test]
fn deleting_the_event_in_google_leaves_the_task_alone() {
    let (core, inbox) = open();
    enable(&core, false);
    let id = add(&core, &inbox, "Call", Some(NOW + HOUR), None);
    let first = reconcile(&core, vec![], true, NOW);
    send_all(&core, &first.pushes, NOW);
    let mut cancelled = event(&first.pushes[0].event_id, "", 0, 0, NOW + 1, None);
    cancelled.status = "cancelled".into();
    let pass = reconcile(&core, vec![cancelled], false, NOW + 1000);
    assert!(pass.pushes.is_empty());
    assert_eq!(core.task(id.clone()).unwrap().unwrap().status, "open");
    // Not recreated while the time stands...
    assert!(
        reconcile(&core, vec![], false, NOW + 2000)
            .pushes
            .is_empty()
    );
    // ...and recreated, under the same id, once it moves.
    core.schedule_task(
        id.clone(),
        Some(NOW + 6 * HOUR),
        NOW + 3000,
        "Europe/London".into(),
    )
    .unwrap();
    let back = reconcile(&core, vec![], false, NOW + 4000);
    assert_eq!(back.pushes.len(), 1);
    assert_eq!(back.pushes[0].kind, GoogleCalendarPushKind::Create);
}

#[test]
fn only_tasks_with_an_event_standing_count_as_linked() {
    let (core, inbox) = open();
    enable(&core, false);
    let id = add(&core, &inbox, "Call", Some(NOW + HOUR), None);
    add(&core, &inbox, "Untimed", None, None);
    assert!(core.google_calendar_linked_task_ids().unwrap().is_empty());
    let first = reconcile(&core, vec![], true, NOW);
    send_all(&core, &first.pushes, NOW);
    assert_eq!(core.google_calendar_linked_task_ids().unwrap(), [id]);
    let mut cancelled = event(&first.pushes[0].event_id, "", 0, 0, NOW + 1, None);
    cancelled.status = "cancelled".into();
    reconcile(&core, vec![cancelled], false, NOW + 1000);
    assert!(core.google_calendar_linked_task_ids().unwrap().is_empty());
}

#[test]
fn an_insert_another_device_already_made_is_adopted_not_duplicated() {
    let (core, inbox) = open();
    enable(&core, false);
    let id = add(&core, &inbox, "Write", Some(NOW + HOUR), None);
    let derived = google_calendar_event_id(id.clone());
    // The other device's event arrives before its link row does.
    let theirs = event(
        &derived,
        "Write",
        NOW + HOUR,
        NOW + HOUR + DEFAULT_DURATION_MS,
        NOW,
        Some(&id),
    );
    let pass = reconcile(&core, vec![theirs], true, NOW);
    assert!(pass.pushes.is_empty());
    assert!(reconcile(&core, vec![], false, NOW + 1).pushes.is_empty());
}

#[test]
fn events_made_in_google_become_tasks_only_when_asked() {
    let (core, _) = open();
    enable(&core, true);
    let made = event("googleid", "Dentist", NOW + HOUR, NOW + 2 * HOUR, NOW, None);
    let pass = reconcile(&core, vec![made.clone()], true, NOW);
    assert_eq!(pass.tasks_imported, 1);
    assert_eq!(pass.pushes.len(), 1);
    assert_eq!(pass.pushes[0].kind, GoogleCalendarPushKind::Tag);
    let task_id = pass.pushes[0].task_id.clone();
    send_all(&core, &pass.pushes, NOW);
    let snapshot = core.editor_snapshot(task_id.clone()).unwrap();
    assert_eq!(snapshot.title, "Dentist");
    assert_eq!(snapshot.estimate_seconds, Some(3600));
    // Tagged now; the next pass sees an agreed event.
    let mut tagged = made;
    tagged.task_id = Some(task_id);
    assert!(
        reconcile(&core, vec![tagged], false, NOW + 1)
            .pushes
            .is_empty()
    );
}

#[test]
fn a_due_time_is_the_events_end_and_a_due_date_is_no_event() {
    let (core, inbox) = open();
    enable(&core, false);
    add(&core, &inbox, "Deadline", None, Some(NOW + 2 * HOUR));
    let pass = reconcile(&core, vec![], true, NOW);
    assert_eq!(pass.pushes.len(), 1);
    let body: Value =
        serde_json::from_str(pass.pushes[0].request.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["end"]["dateTime"], json!(rfc3339(NOW + 2 * HOUR)));
    assert_eq!(
        body["start"]["dateTime"],
        json!(rfc3339(NOW + 2 * HOUR - DEFAULT_DURATION_MS))
    );
}

#[test]
fn the_agenda_interleaves_events_and_timed_tasks() {
    let (core, inbox) = open();
    enable(&core, false);
    add(&core, &inbox, "Write", Some(NOW + 2 * HOUR), None);
    add(&core, &inbox, "Tomorrow", Some(NOW + 30 * HOUR), None);
    let mut all_day = event("h", "Holiday", 0, 0, NOW, None);
    all_day.start_ms = None;
    all_day.end_ms = None;
    all_day.start_date = Some("2027-01-15".into());
    let entries = core
        .google_calendar_agenda(
            vec![
                event("b", "Lunch", NOW + 3 * HOUR, NOW + 4 * HOUR, NOW, None),
                event("a", "Standup", NOW + HOUR, NOW + 2 * HOUR, NOW, None),
                all_day,
                event(
                    "ours",
                    "Write",
                    NOW + 2 * HOUR,
                    NOW + 3 * HOUR,
                    NOW,
                    Some("x"),
                ),
            ],
            NOW,
            NOW + 24 * HOUR,
        )
        .unwrap();
    let titles: Vec<&str> = entries.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(titles, vec!["Holiday", "Standup", "Write", "Lunch"]);
    assert_eq!(entries[2].kind, GoogleCalendarAgendaKind::Task);
}
