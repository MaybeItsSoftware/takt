//! Unit tests for the parts of the CLI that are logic rather than plumbing.
//!
//! The server's behaviour is tested here; `scripts/mcp_smoke_check.py` covers
//! only the seam between the app and this binary. These cover the CLI's own
//! argument handling too, and the invariants that keep the tool table, the
//! subcommands and the files the app reads in step.

use crate::checkvist::{CheckvistClient, CheckvistConfig};
use crate::cli::{Cli, parse_weekdays, resolve};
use crate::config::{Config, Source, choose};
use crate::local::LocalState;
use crate::mcp::{tool_definitions, tool_result_text};
use crate::tools::{Tools, as_bool, as_optional_int, as_optional_weekdays, filter_tasks};
use chrono::{Local, TimeZone};
use clap::Parser;
use serde_json::{Map, Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

// -- helpers -----------------------------------------------------------------

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A private store directory per test. Tests run in threads within one process
/// and several of them write `dailies.json`, so sharing one would have them
/// take each other's lock and see each other's dailies.
fn scratch() -> LocalState {
    let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
    let directory =
        std::env::temp_dir().join(format!("priority-tests-{}-{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("scratch directory");
    LocalState {
        prefs_path: directory.join("missing.plist"),
        store_directory: directory,
    }
}

fn tools_for(local: LocalState) -> Tools {
    Tools {
        client: CheckvistClient::new(CheckvistConfig {
            username: String::new(),
            remote_key: String::new(),
            default_list_id: "999".into(),
            base_url: "https://checkvist.invalid".into(),
        }),
        local,
        workspace: crate::workspace::Workspace {
            database_path: std::env::temp_dir().join("priority-tests-missing.sqlite"),
        },
    }
}

fn stored_dailies(local: &LocalState) -> Vec<Value> {
    let text = std::fs::read_to_string(local.dailies_path()).expect("dailies.json");
    serde_json::from_str::<Value>(&text).expect("valid JSON")["dailies"]
        .as_array()
        .expect("dailies array")
        .clone()
}

// -- day boundary ------------------------------------------------------------

#[test]
fn work_finished_after_midnight_belongs_to_the_previous_day() {
    let local = scratch();
    // 01:30 under the default 4am rollover is still the day that began
    // yesterday morning — the whole point of a logical day.
    let late = Local.with_ymd_and_hms(2026, 8, 15, 1, 30, 0).unwrap();
    assert_eq!(local.day_key(late), "2026-08-14");

    let morning = Local.with_ymd_and_hms(2026, 8, 15, 5, 0, 0).unwrap();
    assert_eq!(local.day_key(morning), "2026-08-15");
}

#[test]
fn logical_day_is_idempotent() {
    // These values are passed back in as day identifiers, so re-keying one must
    // not shift it another day earlier every time.
    let local = scratch();
    let moment = Local.with_ymd_and_hms(2026, 8, 15, 1, 30, 0).unwrap();
    let once = local.logical_day(moment);
    assert_eq!(local.logical_day(once), once);
    assert_eq!(local.day_key(once), local.day_key(moment));
}

#[test]
fn a_missing_preferences_file_still_gives_the_default_rollover() {
    assert_eq!(
        scratch().rollover_hour(),
        crate::local::DEFAULT_ROLLOVER_HOUR
    );
}

// -- aggregation -------------------------------------------------------------

fn write_log(local: &LocalState, events: &[Value]) {
    let lines: Vec<String> = events.iter().map(Value::to_string).collect();
    std::fs::write(local.daylog_path(), lines.join("\n") + "\n").expect("write day log");
}

#[test]
fn a_reopen_cancels_its_completion_even_on_a_later_day() {
    let local = scratch();
    write_log(
        &local,
        &[
            json!({"kind": "completed", "at": "2026-08-14T10:00:00Z", "taskId": 11, "title": "Survives"}),
            json!({"kind": "completed", "at": "2026-08-14T10:05:00Z", "taskId": 12, "title": "Undone"}),
            // The reopen lands the *next* day, and must still cancel yesterday's
            // completion rather than counting against today.
            json!({"kind": "reopened", "at": "2026-08-15T11:00:00Z", "taskId": 12, "title": "Undone"}),
        ],
    );

    let summaries = local.day_summaries(Local.with_ymd_and_hms(2026, 8, 15, 12, 0, 0).unwrap(), 2);
    let yesterday = summaries
        .iter()
        .find(|day| day["day"] == json!("2026-08-14"))
        .unwrap();
    assert_eq!(yesterday["completed_count"], json!(1));
    assert_eq!(yesterday["completed"][0]["task_id"], json!(11));
}

#[test]
fn an_untick_only_reaches_the_day_it_was_made_on() {
    let local = scratch();
    std::fs::write(
        local.dailies_path(),
        json!({"version": 1, "dailies": [
            {"id": "d1", "title": "Habit", "activeWeekdays": [1,2,3,4,5,6,7],
             "sortIndex": 0, "createdAt": "2026-08-01T00:00:00Z"}
        ]})
        .to_string(),
    )
    .unwrap();
    write_log(
        &local,
        &[
            json!({"kind": "dailyCompleted", "at": "2026-08-14T10:00:00Z", "taskId": 0, "title": "Habit", "dailyId": "d1"}),
            // A daily is a fresh question each morning, so this un-tick belongs to
            // the 15th and must leave the 14th ticked.
            json!({"kind": "dailyUncompleted", "at": "2026-08-15T10:00:00Z", "taskId": 0, "title": "Habit", "dailyId": "d1"}),
        ],
    );

    let summaries = local.day_summaries(Local.with_ymd_and_hms(2026, 8, 15, 12, 0, 0).unwrap(), 2);
    let done_on = |key: &str| {
        summaries
            .iter()
            .find(|day| day["day"] == json!(key))
            .unwrap()["dailies"][0]["done"]
            .clone()
    };
    assert_eq!(done_on("2026-08-14"), json!(true));
    assert_eq!(done_on("2026-08-15"), json!(false));
}

#[test]
fn a_damaged_line_costs_one_event_not_the_history() {
    let local = scratch();
    std::fs::write(
        local.daylog_path(),
        "{\"kind\": \"completed\", \"at\": \"2026-08-14T10:00:00Z\", \"taskId\": 11, \"title\": \"Kept\"}\n\
         {not json at all\n\
         {\"kind\": \"completed\", \"at\": \"2026-08-14T10:01:00Z\", \"taskId\": 12, \"title\": \"Also kept\"}\n",
    )
    .unwrap();

    let summaries = local.day_summaries(Local.with_ymd_and_hms(2026, 8, 14, 12, 0, 0).unwrap(), 1);
    assert_eq!(summaries[0]["completed_count"], json!(2));
}

#[test]
fn a_deferred_task_is_not_reported_as_unfinished() {
    let local = scratch();
    write_log(
        &local,
        &[
            json!({"kind": "planSnapshot", "at": "2026-08-14T09:00:00Z", "taskId": 0, "title": "",
               "plannedTaskIds": [11, 12, 13]}),
            json!({"kind": "completed", "at": "2026-08-14T10:00:00Z", "taskId": 11, "title": "Done"}),
            // Deliberately pushed, which is a decision rather than a failure.
            json!({"kind": "deferred", "at": "2026-08-14T11:00:00Z", "taskId": 12, "title": "Pushed"}),
        ],
    );

    let day = &local.day_summaries(Local.with_ymd_and_hms(2026, 8, 14, 12, 0, 0).unwrap(), 1)[0];
    assert_eq!(day["unfinished_task_ids"], json!([13]));
    assert_eq!(day["deferred_task_ids"], json!([12]));
}

// -- the serialised format is a cross-process interface ----------------------

#[test]
fn created_at_is_written_without_fractional_seconds() {
    // Swift decodes with `.iso8601`, which uses `.withInternetDateTime` and so
    // *rejects* fractional seconds — and `DailyDefinitionsStore.load()` turns a
    // decode failure into an empty collection. One bad timestamp written from
    // here makes every daily vanish in the app, and its next save persists that
    // emptiness over the file.
    let local = scratch();
    local
        .add_daily("Written by the CLI", None, None)
        .expect("add");

    let created_at = stored_dailies(&local)[0]["createdAt"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        created_at.len(),
        20,
        "expected yyyy-MM-ddTHH:mm:ssZ, got {created_at}"
    );
    assert!(
        created_at.ends_with('Z'),
        "expected a UTC Z suffix, got {created_at}"
    );
    assert!(
        !created_at.contains('.'),
        "fractional seconds would not decode: {created_at}"
    );
    assert!(chrono::DateTime::parse_from_rfc3339(&created_at).is_ok());
}

#[test]
fn active_weekdays_are_written_in_sorted_order() {
    // `Set` has no order on the Swift side, so an unsorted write here would
    // make every save a spurious diff in a file the user is invited to edit.
    let local = scratch();
    local
        .add_daily("Habit", Some(vec![6, 2, 4]), None)
        .expect("add");
    assert_eq!(
        stored_dailies(&local)[0]["activeWeekdays"],
        json!([2, 4, 6])
    );
}

#[test]
fn a_daily_is_appended_at_the_end_of_the_current_order() {
    let local = scratch();
    for title in ["First", "Second", "Third"] {
        local.add_daily(title, None, None).expect("add");
    }
    let indexes: Vec<i64> = stored_dailies(&local)
        .iter()
        .map(|d| d["sortIndex"].as_i64().unwrap())
        .collect();
    assert_eq!(indexes, vec![0, 1, 2]);
}

#[test]
fn archiving_keeps_the_daily_rather_than_deleting_it() {
    // History references the id, so a delete would leave past days rendering an
    // orphan instead of a title.
    let local = scratch();
    let added = local.add_daily("Habit", None, None).expect("add");
    let id = added["id"].as_str().unwrap().to_string();

    let archived = local
        .update_daily(&id, None, None, Some(true), None)
        .expect("archive");
    assert_eq!(archived["archived"], json!(true));
    assert_eq!(stored_dailies(&local).len(), 1);

    let restored = local
        .update_daily(&id, None, None, Some(false), None)
        .expect("unarchive");
    assert_eq!(restored["archived"], json!(false));
    assert!(stored_dailies(&local)[0].get("archivedAt").is_none());
}

#[test]
fn ticking_twice_is_idempotent_and_appends_only_once() {
    let local = scratch();
    let id = local.add_daily("Habit", None, None).expect("add")["id"]
        .as_str()
        .unwrap()
        .to_string();

    let first = local.set_daily(&id, true).expect("tick");
    assert_eq!(first["changed"], json!(true));
    let second = local.set_daily(&id, true).expect("tick again");
    assert_eq!(second["changed"], json!(false));

    let log = std::fs::read_to_string(local.daylog_path()).unwrap();
    assert_eq!(
        log.lines().filter(|line| !line.trim().is_empty()).count(),
        1
    );
}

#[test]
fn a_mutation_sees_what_is_on_disk_now_rather_than_an_earlier_snapshot() {
    // The app and the two other servers edit the same file. A caller that saved
    // its own in-memory copy would erase whatever they added in the meantime,
    // with no error, because from its point of view the write succeeded.
    let local = scratch();
    local.add_daily("From the CLI", None, None).expect("add");

    // Stand in for the other process: write a second daily straight to the file.
    let mut collection: Value =
        serde_json::from_str(&std::fs::read_to_string(local.dailies_path()).unwrap()).unwrap();
    collection["dailies"].as_array_mut().unwrap().push(json!({
        "id": "external", "title": "From the app", "activeWeekdays": [1,2,3,4,5,6,7],
        "sortIndex": 1, "createdAt": "2026-08-01T00:00:00Z"
    }));
    std::fs::write(local.dailies_path(), collection.to_string()).unwrap();

    local
        .add_daily("From the CLI again", None, None)
        .expect("add");

    let saved = stored_dailies(&local);
    let titles: Vec<&str> = saved.iter().map(|d| d["title"].as_str().unwrap()).collect();
    assert_eq!(
        titles,
        vec!["From the CLI", "From the app", "From the CLI again"]
    );
}

// -- rotating schedules ------------------------------------------------------

/// Writes `dailies.json` directly, so a cycle can be pinned to a fixed anchor
/// rather than to whenever the test happens to run.
fn write_dailies(local: &LocalState, dailies: &[Value]) {
    let collection = json!({ "version": 1, "dailies": dailies });
    std::fs::write(local.dailies_path(), collection.to_string()).expect("write dailies");
}

fn due_ids(local: &LocalState, on: chrono::DateTime<Local>) -> Vec<String> {
    local.dailies_snapshot(on)["dailies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|daily| daily["due_today"] == json!(true))
        .map(|daily| daily["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn an_every_n_days_daily_rotates_through_the_week() {
    // Anchored on a Friday. A weekday set could never express this: by the
    // third occurrence it has walked onto Sunday.
    let local = scratch();
    write_dailies(
        &local,
        &[json!({
            "id": "cycle", "title": "Bins", "activeWeekdays": [1, 2, 3, 4, 5, 6, 7],
            "intervalDays": 3, "intervalAnchor": "2026-08-14T09:00:00Z",
            "sortIndex": 0, "createdAt": "2026-08-01T00:00:00Z",
        })],
    );

    let at = |day| Local.with_ymd_and_hms(2026, 8, day, 12, 0, 0).unwrap();
    assert_eq!(due_ids(&local, at(14)), vec!["cycle".to_string()]);
    assert!(due_ids(&local, at(15)).is_empty());
    assert!(due_ids(&local, at(16)).is_empty());
    assert_eq!(due_ids(&local, at(17)), vec!["cycle".to_string()]);
    // Two days before the anchor: the cycle runs backwards too, so the history
    // behind it isn't full of days it was "never expected on".
    assert_eq!(due_ids(&local, at(11)), vec!["cycle".to_string()]);
}

#[test]
fn a_cycle_is_labelled_by_its_length() {
    let local = scratch();
    let added = local.add_daily("Bins", None, Some(3)).expect("add");
    assert_eq!(added["schedule"], json!("Every 3 days"));
    assert_eq!(added["interval_days"], json!(3));

    let every_other = local.add_daily("Long run", None, Some(2)).expect("add");
    assert_eq!(every_other["schedule"], json!("Every other day"));
}

/// A weekday-scheduled daily reports the field as null rather than omitting it,
/// so a client can tell "not on a cycle" from "server predates cycles".
#[test]
fn a_weekday_daily_reports_a_null_interval() {
    let local = scratch();
    let added = local
        .add_daily("Standup", Some(vec![2, 3, 4, 5, 6]), None)
        .expect("add");
    assert_eq!(added["interval_days"], Value::Null);
    assert!(stored_dailies(&local)[0].get("intervalDays").is_none());
}

#[test]
fn adding_a_cycle_anchors_it_and_switching_to_weekdays_clears_it() {
    let local = scratch();
    let id = local.add_daily("Bins", None, Some(4)).expect("add")["id"]
        .as_str()
        .unwrap()
        .to_string();
    let stored = stored_dailies(&local);
    assert_eq!(stored[0]["intervalDays"], json!(4));
    let anchor = stored[0]["intervalAnchor"].as_str().expect("anchor");
    assert!(chrono::DateTime::parse_from_rfc3339(anchor).is_ok());
    assert_eq!(
        anchor.len(),
        20,
        "expected yyyy-MM-ddTHH:mm:ssZ, got {anchor}"
    );

    // Changing the length re-spaces the cycle rather than restarting it.
    let anchor = anchor.to_string();
    local
        .update_daily(&id, None, None, None, Some(6))
        .expect("relength");
    assert_eq!(stored_dailies(&local)[0]["intervalAnchor"], json!(anchor));

    // Weekdays end the cycle outright; a stale interval left behind would keep
    // winning over the weekday set the user just chose.
    let back = local
        .update_daily(&id, None, Some(vec![2, 4]), None, None)
        .expect("weekdays");
    assert_eq!(back["schedule"], json!("Mon Wed"));
    assert_eq!(back["interval_days"], Value::Null);
    assert!(stored_dailies(&local)[0].get("intervalAnchor").is_none());
}

#[test]
fn the_two_schedules_cannot_be_combined() {
    let tools = tools_for(scratch());
    let mut arguments = Map::new();
    arguments.insert("title".into(), json!("Bins"));
    arguments.insert("active_weekdays".into(), json!([2, 4]));
    arguments.insert("interval_days".into(), json!(3));
    let Err(error) = tools.call("daily_add", &arguments) else {
        panic!("a daily with both schedules must be refused");
    };
    assert_eq!(
        error.message,
        "Pass either active_weekdays or interval_days, not both."
    );
}

#[test]
fn an_out_of_range_interval_is_refused_rather_than_stored() {
    let tools = tools_for(scratch());
    let mut arguments = Map::new();
    arguments.insert("title".into(), json!("Bins"));
    arguments.insert("interval_days".into(), json!(0));
    let Err(error) = tools.call("daily_add", &arguments) else {
        panic!("an out-of-range interval must be refused");
    };
    assert_eq!(
        error.message,
        "interval_days must be an integer between 1 and 366."
    );
}

#[test]
fn every_days_reaches_the_tool_layer_as_interval_days() {
    let cli = Cli::parse_from(["priority", "daily", "add", "Bins", "--every-days", "3"]);
    let (name, arguments) = resolve(&cli).expect("resolve");
    assert_eq!(name, "daily_add");
    assert_eq!(arguments["interval_days"], json!(3));
    assert!(arguments.get("active_weekdays").is_none());
}

#[test]
fn an_unreadable_dailies_file_reads_as_empty_rather_than_failing() {
    let local = scratch();
    std::fs::write(local.dailies_path(), "{ this is not JSON").unwrap();
    let snapshot = local.dailies_snapshot(Local::now());
    assert_eq!(snapshot["dailies"], json!([]));
}

// -- task metadata -----------------------------------------------------------

#[test]
fn the_two_priority_stores_are_read_in_their_own_encodings() {
    // `ListScopedPriorityStore` writes a JSON blob via `defaults.set(Data)`,
    // `ListScopedTaskIDStore` a plain plist dictionary. Reading either one the
    // other's way yields silently empty results.
    let local = scratch();
    let mut prefs = plist::Dictionary::new();
    prefs.insert(
        "priorityTaskIdsByParentIdByListId".into(),
        plist::Value::Data(
            json!({"999": {"0": [11, 12], "11": [13]}})
                .to_string()
                .into_bytes(),
        ),
    );
    let mut absolute = plist::Dictionary::new();
    absolute.insert(
        "999".into(),
        plist::Value::Array(vec![12.into(), 11.into()]),
    );
    prefs.insert(
        "absolutePriorityTaskIdsByListId".into(),
        plist::Value::Dictionary(absolute),
    );
    plist::Value::Dictionary(prefs)
        .to_file_binary(&local.prefs_path)
        .unwrap();

    let metadata = local.task_metadata("999");
    assert_eq!(
        metadata["absolute_priority_rank"],
        json!({"12": 1, "11": 2})
    );
    assert_eq!(
        metadata["scoped_priority_rank_by_parent_id"],
        json!({"0": {"11": 1, "12": 2}, "11": {"13": 1}})
    );
}

#[test]
fn priority_queues_drop_non_positive_and_duplicate_ids() {
    let local = scratch();
    let mut prefs = plist::Dictionary::new();
    let mut absolute = plist::Dictionary::new();
    absolute.insert(
        "999".into(),
        plist::Value::Array(vec![11.into(), 0.into(), (-3).into(), 11.into(), 12.into()]),
    );
    prefs.insert(
        "absolutePriorityTaskIdsByListId".into(),
        plist::Value::Dictionary(absolute),
    );
    plist::Value::Dictionary(prefs)
        .to_file_binary(&local.prefs_path)
        .unwrap();

    assert_eq!(
        local.task_metadata("999")["absolute_priority_rank"],
        json!({"11": 1, "12": 2})
    );
}

#[test]
fn an_empty_list_id_reads_the_offline_scope() {
    let local = scratch();
    let mut prefs = plist::Dictionary::new();
    let mut absolute = plist::Dictionary::new();
    absolute.insert("__offline__".into(), plist::Value::Array(vec![7.into()]));
    prefs.insert(
        "absolutePriorityTaskIdsByListId".into(),
        plist::Value::Dictionary(absolute),
    );
    plist::Value::Dictionary(prefs)
        .to_file_binary(&local.prefs_path)
        .unwrap();

    assert_eq!(
        local.task_metadata("")["absolute_priority_rank"],
        json!({"7": 1})
    );
}

// -- argument coercion -------------------------------------------------------

#[test]
fn a_json_integer_is_never_mistaken_for_a_boolean() {
    // The bug this pins: a Swift `NSNumber(1)` casts to `Bool` successfully, so
    // an `is Bool` guard rejected `position: 1` and `parent_task_id: 0`
    // outright. In JSON they are distinct types and must stay so.
    assert_eq!(as_optional_int(Some(&json!(1))).unwrap(), Some(1));
    assert_eq!(as_optional_int(Some(&json!(0))).unwrap(), Some(0));
    assert!(as_optional_int(Some(&json!(true))).is_err());
    assert!(as_optional_int(Some(&json!(false))).is_err());
}

#[test]
fn coercion_is_tolerant_where_clients_are_loose() {
    assert_eq!(as_optional_int(Some(&json!("42"))).unwrap(), Some(42));
    assert_eq!(as_optional_int(Some(&json!(""))).unwrap(), None);
    assert_eq!(as_optional_int(None).unwrap(), None);
    assert!(as_bool(Some(&json!("yes")), false).unwrap());
    assert!(!as_bool(Some(&json!("0")), true).unwrap());
    assert!(as_bool(None, true).unwrap());
    assert!(as_optional_int(Some(&json!(1.5))).is_err());
}

#[test]
fn weekdays_must_be_calendar_numbers_in_range() {
    assert_eq!(
        as_optional_weekdays(Some(&json!([6, 2, 2, 4]))).unwrap(),
        Some(vec![2, 4, 6])
    );
    assert_eq!(as_optional_weekdays(None).unwrap(), None);
    assert!(as_optional_weekdays(Some(&json!([0]))).is_err());
    assert!(as_optional_weekdays(Some(&json!([8]))).is_err());
    assert!(as_optional_weekdays(Some(&json!([]))).is_err());
    assert!(as_optional_weekdays(Some(&json!("mon"))).is_err());
}

// -- dailies.json and the day log, written beside the app --------------------

#[test]
fn a_dailies_file_that_does_not_decode_is_not_saved_over() {
    // The app refuses to save over it too. Overwriting would replace every
    // daily it held with the one being added.
    let local = scratch();
    let damaged = "{\"version\": 1, \"dailies\": [{\"id\": \"A\", ";
    std::fs::write(local.dailies_path(), damaged).unwrap();

    let error = local.add_daily("New", None, None).expect_err("refused");
    assert!(error.message.contains("dailies.json"), "{}", error.message);
    assert_eq!(
        std::fs::read_to_string(local.dailies_path()).unwrap(),
        damaged
    );

    // Reading stays tolerant: the damaged file is simply no dailies.
    assert_eq!(local.dailies_snapshot(Local::now())["dailies"], json!([]));

    // A missing file is the ordinary first-run state, not damage.
    let fresh = scratch();
    fresh.add_daily("First", None, None).expect("created");
    assert_eq!(stored_dailies(&fresh).len(), 1);
}

#[test]
fn a_day_log_event_starts_its_own_line_even_after_a_torn_one() {
    use takt_core::day_log::{DayLogRecord, DayLogRecordKind, append, load};
    let tick = |id: &str| DayLogRecord {
        kind: DayLogRecordKind::DailyCompleted,
        at_ms: 1_786_701_600_000,
        task_id: 0,
        title: id.into(),
        duration_seconds: None,
        planned_task_ids: None,
        daily_id: Some(id.into()),
    };
    let local = scratch();
    let path = local.daylog_path();
    // A writer that died mid-line, and a line that is not even UTF-8.
    let mut damaged = b"{\"kind\":\"dailyCompleted\",\"at\":\"2026-08-14T10:00:00Z\",\"taskId\":0,\"title\":\"A\",\"dailyId\":\"A\"}\n".to_vec();
    damaged.extend_from_slice(b"\xff\xfe not utf8\n{\"kind\":\"dailyComp");
    std::fs::write(&path, &damaged).unwrap();

    append(&path, &tick("B")).expect("append");

    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.ends_with(b"}\n"));
    // The torn line and the invalid one cost themselves only: the events
    // either side of them both survive.
    let ids: Vec<String> = load(&path)
        .into_iter()
        .filter_map(|event| event.daily_id)
        .collect();
    assert_eq!(ids, vec!["A", "B"]);

    // Into a file that already ends cleanly, no blank line is added, and the
    // line is the one the app writes.
    let clean = scratch().daylog_path();
    append(&clean, &tick("1")).unwrap();
    append(&clean, &tick("2")).unwrap();
    let line = |id: &str| {
        format!(
            "{{\"at\":\"2026-08-14T10:00:00Z\",\"dailyId\":\"{id}\",\"kind\":\"dailyCompleted\",\"taskId\":0,\"title\":\"{id}\"}}\n"
        )
    };
    assert_eq!(
        std::fs::read_to_string(&clean).unwrap(),
        line("1") + &line("2")
    );
}

// -- the CLI's own surface ---------------------------------------------------

#[test]
fn friendly_weekday_spellings_become_calendar_numbers() {
    assert_eq!(parse_weekdays("mon,wed,fri").unwrap(), vec![2, 4, 6]);
    assert_eq!(parse_weekdays("weekdays").unwrap(), vec![2, 3, 4, 5, 6]);
    assert_eq!(parse_weekdays("weekend").unwrap(), vec![1, 7]);
    assert_eq!(
        parse_weekdays("every day").unwrap(),
        vec![1, 2, 3, 4, 5, 6, 7]
    );
    assert_eq!(parse_weekdays("1,3,5").unwrap(), vec![1, 3, 5]);
    assert_eq!(parse_weekdays("Monday Tuesday").unwrap(), vec![2, 3]);
    assert!(parse_weekdays("funday").is_err());
    assert!(parse_weekdays("9").is_err());
}

fn resolved(argv: &[&str]) -> (String, Map<String, Value>) {
    resolve(&Cli::parse_from(argv)).expect("resolve")
}

#[test]
fn subcommands_map_onto_the_tools_they_claim_to_be() {
    assert_eq!(resolved(&["priority", "lists"]).0, "task_lists");
    assert_eq!(resolved(&["priority", "done", "5"]).0, "task_complete");
    assert_eq!(resolved(&["priority", "rm", "5"]).0, "task_delete");
    assert_eq!(resolved(&["priority", "dailies"]).0, "dailies_list");

    let (name, args) = resolved(&["priority", "add", "buy", "milk", "--due", "tomorrow"]);
    assert_eq!(name, "task_add");
    assert_eq!(args["content"], json!("buy milk"));
    assert_eq!(args["due"], json!("tomorrow"));
    // No parent given, so the task goes to the list root and `location` stays
    // at its default rather than being sent as "specific" with no parent.
    assert!(!args.contains_key("location"));

    let (_, args) = resolved(&["priority", "add", "sub", "--parent", "7"]);
    assert_eq!(args["location"], json!("specific"));
    assert_eq!(args["parent_task_id"], json!(7));
}

#[test]
fn update_sends_tags_including_an_empty_string_that_clears_them() {
    let (name, args) = resolved(&["priority", "update", "5", "--tags", "work,urgent"]);
    assert_eq!(name, "task_update");
    assert_eq!(args["tags"], json!("work,urgent"));
    // Empty is meaningful to the tool (remove every tag), so it is sent.
    let (_, args) = resolved(&["priority", "update", "5", "--tags", ""]);
    assert_eq!(args["tags"], json!(""));
    assert!(
        !resolved(&["priority", "update", "5", "-c", "x"])
            .1
            .contains_key("tags")
    );
}

#[test]
fn move_project_names_both_lists_and_defaults_the_source_to_the_global_list() {
    let (name, args) = resolved(&["priority", "move-project", "5", "--from", "1", "--to", "2"]);
    assert_eq!(name, "project_move");
    assert_eq!(args["task_id"], json!(5));
    assert_eq!(args["source_list_id"], json!("1"));
    assert_eq!(args["target_list_id"], json!("2"));
    // The tool has no `list_id`, and its schema refuses unknown keys.
    assert!(!args.contains_key("list_id"));

    let (_, args) = resolved(&["priority", "-l", "7", "move-project", "5", "--to", "2"]);
    assert_eq!(args["source_list_id"], json!("7"));
    assert!(!args.contains_key("list_id"));

    assert!(
        resolve(&Cli::parse_from([
            "priority",
            "move-project",
            "5",
            "--to",
            "2"
        ]))
        .is_err()
    );
}

/// One invocation per subcommand that names a tool. A new subcommand belongs
/// here; a new tool with no subcommand fails the test below.
const EVERY_SUBCOMMAND: &[&[&str]] = &[
    &["lists"],
    &["new-list", "L"],
    &["matrix", "1:2:3"],
    &["tasks"],
    &["search", "-q", "x"],
    &["add", "x"],
    &["update", "1", "-c", "x"],
    &["note", "1", "x"],
    &["move", "1", "2"],
    &["reparent", "1"],
    &["move-project", "1", "--from", "2", "--to", "3"],
    &["done", "1"],
    &["reopen", "1"],
    &["invalidate", "1"],
    &["rm", "1"],
    &["log"],
    &["focus"],
    &["focused"],
    &["dailies"],
    &["daily", "add", "x"],
    &["daily", "update", "d", "--title", "x"],
    &["daily", "tick", "d"],
    &["metadata"],
    &["ws", "tree"],
    &["ws", "shared"],
    &["ws", "tasks", "L"],
    &["ws", "add", "x", "--list", "L"],
    &["ws", "update", "T", "--title", "x"],
    &["ws", "done", "T"],
    &["ws", "move", "T", "--position", "1"],
    &["ws", "to-list", "T"],
    &["ws", "rm", "T"],
    &["ws", "deps", "T"],
    &["ws", "conditions"],
    &["ws", "require", "T", "Home|Campus", "Mat"],
    &["ws", "here", "Campus", "--for", "30"],
    &["ws", "provides", "Home", "Floor space", "Quiet"],
    &["ws", "block", "T", "B"],
    &["ws", "unblock", "T", "B"],
    &["ws", "filters"],
    &["ws", "filter", "Today"],
    &["ws", "filter-new", "Work", "-r", "tag:has:work"],
    &["ws", "filter-edit", "F", "--name", "x"],
    &["ws", "filter-rm", "F"],
    &["ws", "new-folder", "F"],
    &["ws", "new-list", "L"],
    &["ws", "move-list", "L"],
    &["ws", "rm-list", "L"],
    &["ws", "commit", "T", "-r", "weekdays"],
    &["ws", "columns"],
    &["ws", "column-set", "L", "-t", "New"],
    &["ws", "column-rm", "L", "backlog"],
    &["ws", "instructions"],
    &["ws", "day-start", "Home", "--hour", "4"],
    &[
        "ws",
        "instructions-set",
        "--list",
        "L",
        "--append",
        "Keep",
        "it",
        "short",
    ],
];

/// Tools a terminal reaches only through `takt call`, each with the reason.
/// Empty today; an entry here is a decision, not an oversight.
const CALL_ONLY_TOOLS: &[(&str, &str)] = &[];

#[test]
fn every_tool_has_a_subcommand() {
    // The reverse of `subcommands_map_onto_the_tools_they_claim_to_be`: the
    // shared tool table exists so a terminal can reach anything an assistant
    // can. A tool added to `tool_definitions` without a `Command` and a
    // `resolve` arm fails here (project_move and task_update's tags once did).
    let mut reached: Vec<String> = EVERY_SUBCOMMAND
        .iter()
        .map(|argv| {
            let mut full = vec!["priority"];
            full.extend_from_slice(argv);
            resolved(&full).0
        })
        .collect();
    reached.sort_unstable();
    reached.dedup();

    for definition in tool_definitions() {
        let name = definition["name"].as_str().expect("tool name");
        if CALL_ONLY_TOOLS.iter().any(|(tool, _)| *tool == name) {
            continue;
        }
        assert!(
            reached.iter().any(|tool| tool == name),
            "{name} has no subcommand: add a `Command` case and `resolve` arm, \
             or list it in CALL_ONLY_TOOLS with a reason"
        );
    }
}

#[test]
fn reparent_without_a_parent_means_the_list_root() {
    // Omitting the key is the tool's way of saying "root"; sending 0 would work
    // too, but the absence is what the schema documents.
    let (name, args) = resolved(&["priority", "reparent", "5"]);
    assert_eq!(name, "task_reparent");
    assert!(!args.contains_key("parent_task_id"));

    let (_, args) = resolved(&["priority", "reparent", "5", "--parent", "9"]);
    assert_eq!(args["parent_task_id"], json!(9));
}

#[test]
fn archive_and_unarchive_send_the_flag_rather_than_omitting_it() {
    // `archived` absent means "leave it alone", so an unarchive that omitted
    // the key would silently do nothing.
    let (_, args) = resolved(&["priority", "daily", "update", "d1", "--archive"]);
    assert_eq!(args["archived"], json!(true));

    let (_, args) = resolved(&["priority", "daily", "update", "d1", "--unarchive"]);
    assert_eq!(args["archived"], json!(false));

    let (_, args) = resolved(&["priority", "daily", "update", "d1", "--title", "New"]);
    assert!(!args.contains_key("archived"));
}

#[test]
fn tick_off_un_ticks() {
    let (name, args) = resolved(&["priority", "daily", "tick", "d1", "--off"]);
    assert_eq!(name, "daily_tick");
    assert_eq!(args["done"], json!(false));
    assert_eq!(
        resolved(&["priority", "daily", "tick", "d1"]).1["done"],
        json!(true)
    );
}

#[test]
fn the_global_list_id_reaches_the_tool_and_call_arguments_override_it() {
    let (_, args) = resolved(&["priority", "--list-id", "123", "tasks"]);
    assert_eq!(args["list_id"], json!("123"));

    let (name, args) = resolved(&[
        "priority",
        "-l",
        "123",
        "call",
        "task_fetch",
        r#"{"list_id": "456"}"#,
    ]);
    assert_eq!(name, "task_fetch");
    assert_eq!(args["list_id"], json!("456"));
}

#[test]
fn call_rejects_arguments_that_are_not_a_json_object() {
    assert!(
        resolve(&Cli::parse_from([
            "priority",
            "call",
            "task_lists",
            "[1,2]"
        ]))
        .is_err()
    );
    assert!(resolve(&Cli::parse_from(["priority", "call", "task_lists", "nope"])).is_err());
}

#[test]
fn the_clap_definition_is_internally_consistent() {
    use clap::CommandFactory;
    Cli::command().debug_assert();
}

// -- the tool surface --------------------------------------------------------

#[test]
fn every_advertised_tool_is_actually_implemented() {
    // The failure this catches: a tool listed in `tool_definitions` but missing
    // from the dispatch table answers "Unknown tool" at the moment an assistant
    // finally reaches for it, having been told all along that it exists.
    let tools = tools_for(scratch());
    for definition in tool_definitions() {
        let name = definition["name"].as_str().expect("tool name");
        let error = tools
            .call(name, &Map::new())
            .err()
            .map(|error| error.message);
        assert!(
            !error
                .as_deref()
                .is_some_and(|message| message.starts_with("Unknown tool")),
            "{name} is advertised but not implemented"
        );
    }
}

#[test]
fn the_tool_surface_is_fifty_four_uniquely_named_tools() {
    let definitions = tool_definitions();
    let mut names: Vec<&str> = definitions
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    let total = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), total, "duplicate tool name");
    assert_eq!(total, 54);

    for definition in &definitions {
        assert!(
            definition["description"]
                .as_str()
                .is_some_and(|text| !text.is_empty())
        );
        assert_eq!(definition["inputSchema"]["type"], json!("object"));
    }
}

#[test]
fn a_search_limit_below_one_is_refused_before_any_request() {
    // The schema says minimum 1. The base URL is unreachable, so getting the
    // limit error rather than a network one also shows nothing was sent.
    let tools = tools_for(scratch());
    let mut arguments = Map::new();
    arguments.insert("limit".into(), json!(-5));
    let error = tools.call("task_search", &arguments).expect_err("refused");
    assert_eq!(error.message, "limit must be 1 or greater.");
}

#[test]
fn a_list_id_must_be_a_number_before_it_reaches_a_url() {
    use crate::checkvist::validate_list_id;
    assert!(validate_list_id("945183").is_ok());
    for bad in ["", "../checklists", "12?x=1", "12/tasks", " 12", "-3"] {
        assert!(validate_list_id(bad).is_err(), "{bad:?} accepted");
    }

    // Checked where the id enters: the explicit argument, the configured
    // default, and both ends of project_move.
    let tools = tools_for(scratch());
    let mut arguments = Map::new();
    arguments.insert("list_id".into(), json!("1/../2"));
    let error = tools.call("task_fetch", &arguments).expect_err("refused");
    assert!(
        error.message.starts_with("Invalid list ID"),
        "{}",
        error.message
    );

    let mut arguments = Map::new();
    arguments.insert("source_list_id".into(), json!("1"));
    arguments.insert("target_list_id".into(), json!("x"));
    arguments.insert("task_id".into(), json!(5));
    let error = tools.call("project_move", &arguments).expect_err("refused");
    assert!(
        error.message.starts_with("Invalid list ID"),
        "{}",
        error.message
    );
}

#[test]
fn a_tool_result_is_a_title_a_blank_line_and_sorted_json() {
    let text = tool_result_text("Dailies", &json!({"b": 2, "a": 1}));
    assert_eq!(text, "Dailies\n\n{\n  \"a\": 1,\n  \"b\": 2\n}");
}

// -- search filtering --------------------------------------------------------

fn task(id: i64, content: &str, due: Option<&str>, tags: Value) -> Value {
    json!({"id": id, "content": content, "due": due, "tags": tags, "position": id, "parent_id": 0})
}

#[test]
fn filters_are_anded_and_an_omitted_filter_drops_out() {
    let tasks = vec![
        task(
            1,
            "Write the report #work",
            Some("2026-08-20"),
            json!(["work"]),
        ),
        task(2, "Buy milk #home", Some("2026-08-14"), json!(["home"])),
        task(3, "Write the other report", None, json!([])),
    ];

    let ids = |matches: Vec<Value>| -> Vec<i64> {
        matches
            .iter()
            .map(|task| task["id"].as_i64().unwrap())
            .collect()
    };

    assert_eq!(
        ids(filter_tasks(tasks.clone(), Some("write"), None, None)),
        vec![1, 3]
    );
    assert_eq!(
        ids(filter_tasks(tasks.clone(), None, Some("#work"), None)),
        vec![1]
    );
    assert_eq!(
        ids(filter_tasks(tasks.clone(), None, None, Some("2026-08-15"))),
        vec![2]
    );
    assert_eq!(
        ids(filter_tasks(
            tasks.clone(),
            Some("write"),
            Some("work"),
            None
        )),
        vec![1]
    );
    assert_eq!(
        ids(filter_tasks(tasks.clone(), None, None, None)),
        vec![1, 2, 3]
    );
}

#[test]
fn a_task_with_no_due_date_is_never_due_before_anything() {
    let tasks = vec![task(3, "No due date", None, json!([]))];
    assert!(filter_tasks(tasks, None, None, Some("2999-01-01")).is_empty());
}

#[test]
fn tags_are_matched_however_checkvist_happens_to_return_them() {
    // Array, dictionary, or inline in the content — matching only one of the
    // three would silently miss most tagged tasks.
    let as_array = task(1, "One", None, json!(["work"]));
    let as_dictionary = task(2, "Two", None, json!({"work": "1"}));
    let inline = task(3, "Three #work", None, json!([]));
    for candidate in [as_array, as_dictionary, inline] {
        assert_eq!(
            filter_tasks(vec![candidate], None, Some("work"), None).len(),
            1
        );
    }
}

// -- the CLI's own configuration ---------------------------------------------

fn scratch_config(contents: Option<&str>) -> Config {
    let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
    let directory =
        std::env::temp_dir().join(format!("priority-config-{}-{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("scratch directory");
    let path = directory.join("config.json");
    if let Some(contents) = contents {
        std::fs::write(&path, contents).expect("write config");
    }
    Config::load_from(path)
}

#[test]
fn the_environment_beats_the_config_file() {
    // An MCP client config that sets CHECKVIST_REMOTE_KEY must keep working
    // untouched, and a one-off `CHECKVIST_LIST_ID=... priority tasks` must
    // override the stored default. It is also what lets
    // `scripts/mcp_smoke_check.py` drive the server with fake credentials in
    // the environment.
    let (value, source) = choose(
        Some("from-env".into()),
        "CHECKVIST_LIST_ID",
        Some("from-file".into()),
    );
    assert_eq!(value.as_deref(), Some("from-env"));
    assert_eq!(source, Source::Environment("CHECKVIST_LIST_ID"));
}

#[test]
fn the_config_file_is_used_when_the_environment_is_unset() {
    let (value, source) = choose(None, "CHECKVIST_LIST_ID", Some("from-file".into()));
    assert_eq!(value.as_deref(), Some("from-file"));
    assert_eq!(source, Source::ConfigFile);

    let (value, source) = choose(None, "CHECKVIST_LIST_ID", None);
    assert_eq!(value, None);
    assert_eq!(source, Source::Unset);
}

#[test]
fn a_missing_config_file_is_an_empty_config_rather_than_an_error() {
    // This is the state before `auth login`, and every local command works
    // there — failing here would break `priority dailies` for a file it never
    // consults.
    let config = scratch_config(None);
    assert!(!config.exists());
    assert_eq!(
        config.resolve("NOT_A_REAL_VARIABLE", "username").1,
        Source::Unset
    );
}

#[test]
fn a_malformed_config_file_is_tolerated_rather_than_fatal() {
    let config = scratch_config(Some("{ this is not JSON"));
    assert_eq!(config.resolve("NOT_A_REAL_VARIABLE", "username").0, None);

    // A JSON document that isn't an object is equally not a config.
    let config = scratch_config(Some("[1, 2, 3]"));
    assert_eq!(config.resolve("NOT_A_REAL_VARIABLE", "username").0, None);
}

#[test]
fn a_malformed_config_file_is_not_overwritten() {
    // Read as empty so local commands still work, but saving would replace
    // whatever the person had with only the value just set.
    let mut config = scratch_config(Some("{ \"username\": \"me\", oops"));
    assert!(config.is_malformed());
    config.set("list_id", Some("1"));
    let error = config.save().expect_err("refused");
    assert!(
        error.message.contains(&config.path.display().to_string()),
        "{}",
        error.message
    );
    assert_eq!(
        std::fs::read_to_string(&config.path).unwrap(),
        "{ \"username\": \"me\", oops"
    );

    // Not JSON-object-shaped counts too; a missing file does not.
    assert!(scratch_config(Some("[1]")).is_malformed());
    assert!(!scratch_config(None).is_malformed());
}

#[test]
fn an_empty_config_file_is_an_empty_config_that_can_be_saved() {
    // The app reads it the same way, and it holds nothing to lose.
    let mut config = scratch_config(Some("  \n"));
    assert!(!config.is_malformed());
    config.set("username", Some("me"));
    config.save().expect("save");
    assert_eq!(
        std::fs::read_to_string(&config.path).unwrap(),
        "{\n  \"username\": \"me\"\n}\n"
    );
}

#[test]
fn only_a_directory_the_cli_created_is_tightened() {
    use std::os::unix::fs::PermissionsExt;
    let mode =
        |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;

    // A directory that already existed — one named by PRIORITY_CONFIG_PATH or
    // XDG_CONFIG_HOME, say — keeps the mode its owner gave it.
    let mut config = scratch_config(None);
    let parent = config.path.parent().unwrap().to_path_buf();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
    config.set("username", Some("me"));
    config.save().expect("save");
    assert_eq!(mode(&parent), 0o755);

    // One the CLI had to make is private from the start.
    let fresh = parent.join("made-here");
    let mut config = Config::load_from(fresh.join("config.json"));
    config.set("username", Some("me"));
    config.save().expect("save");
    assert_eq!(mode(&fresh), 0o700);
}

#[test]
fn blank_values_in_the_file_count_as_unset() {
    let config = scratch_config(Some(r#"{"username": "   ", "list_id": "945183"}"#));
    assert_eq!(config.resolve("NOT_A_REAL_VARIABLE", "username").0, None);
    assert_eq!(
        config
            .resolve("NOT_A_REAL_VARIABLE_EITHER", "list_id")
            .0
            .as_deref(),
        Some("945183")
    );
}

#[test]
fn the_config_file_is_written_owner_only() {
    // It holds the remote key in the clear, as CLI credential files
    // conventionally do. Mode 0600 from creation is the whole protection, and
    // it only has to leak once.
    use std::os::unix::fs::PermissionsExt;
    let mut config = scratch_config(None);
    config.set("remote_key", Some("secret"));
    config.save().expect("save");

    let mode = std::fs::metadata(&config.path)
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "config file is {:o}, expected 600",
        mode & 0o777
    );
}

#[test]
fn a_saved_config_reads_back_and_logout_removes_only_the_key() {
    let mut config = scratch_config(None);
    config.set("username", Some("you@example.com"));
    config.set("remote_key", Some("secret"));
    config.set("list_id", Some("945183"));
    config.save().expect("save");

    let reloaded = Config::load_from(config.path.clone());
    assert_eq!(
        reloaded
            .resolve("NOT_A_REAL_VARIABLE", "remote_key")
            .0
            .as_deref(),
        Some("secret")
    );

    let mut reloaded = reloaded;
    reloaded.set("remote_key", None);
    reloaded.save().expect("save");

    let after = Config::load_from(config.path.clone());
    assert_eq!(after.resolve("NOT_A_REAL_VARIABLE", "remote_key").0, None);
    // Signing out should not make you re-type the things that aren't secret.
    assert_eq!(
        after
            .resolve("NOT_A_REAL_VARIABLE", "username")
            .0
            .as_deref(),
        Some("you@example.com")
    );
    assert_eq!(
        after.resolve("NOT_A_REAL_VARIABLE", "list_id").0.as_deref(),
        Some("945183")
    );
}

#[test]
fn a_tilde_in_a_hand_edited_path_is_expanded() {
    // Nothing else in the stack expands it, and a person editing this file by
    // hand will write one.
    let config = scratch_config(Some(r#"{"store_directory": "~/Somewhere/Else"}"#));
    let (path, source) = config.resolve_path("NOT_A_REAL_VARIABLE", "store_directory");
    let path = path.expect("a path");
    assert_eq!(source, Source::ConfigFile);
    assert!(
        !path.to_string_lossy().starts_with('~'),
        "left unexpanded: {}",
        path.display()
    );
    assert!(path.ends_with("Somewhere/Else"));
}

#[test]
fn the_store_defaults_to_the_apps_own_directory() {
    // The one place the CLI and the app are deliberately joined: reading the
    // dailies and day log the app writes is why those commands exist.
    // `Takt`, or the pre-rename `Priority` on a machine that still has only
    // that — which one depends on the machine running the tests.
    let local = LocalState::resolve(&scratch_config(None));
    assert!(
        local.store_directory.ends_with("Application Support/Takt")
            || local
                .store_directory
                .ends_with("Application Support/Priority")
    );
}

#[test]
fn a_renamed_location_falls_back_to_the_old_one_only_while_the_new_one_is_missing() {
    let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("takt-rename-{}-{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let current = root.join("Takt");
    let legacy = root.join("Priority");
    let candidates = || vec![current.clone(), legacy.clone()];

    // Neither exists: the new name, so a fresh machine starts there.
    assert_eq!(crate::config::first_existing(candidates()), current);

    // Only the old one: read what is actually on disk.
    std::fs::create_dir_all(&legacy).expect("legacy directory");
    assert_eq!(crate::config::first_existing(candidates()), legacy);

    // Both: the new one wins, for good.
    std::fs::create_dir_all(&current).expect("current directory");
    assert_eq!(crate::config::first_existing(candidates()), current);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_store_can_be_pointed_elsewhere_by_the_config() {
    let config = scratch_config(Some(r#"{"store_directory": "/tmp/somewhere"}"#));
    assert_eq!(
        LocalState::resolve(&config).store_directory,
        PathBuf::from("/tmp/somewhere")
    );
}

// -- locking -----------------------------------------------------------------

#[test]
fn the_lock_file_is_a_sibling_of_the_file_it_protects() {
    // A lock on `dailies.json` itself would guard nothing: the save is atomic
    // (temp + rename), so the inode is replaced and the next process opens a
    // different file with a free lock. The path is also the interface with the
    // app's `FileLock` — get it wrong and it stops excluding it
    // without ever failing.
    // A directory of its own rather than a fixed path: `/tmp/x` was whatever
    // the machine happened to have there, and a stray file of that name failed
    // this test for a reason that had nothing to do with locking.
    let directory = std::env::temp_dir().join(format!("priority-lock-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("scratch directory");
    let protected = directory.join("dailies.json");
    let lock = crate::lock::FileLock::protecting(&protected);
    let mut held = None;
    lock.with_exclusive(|| {
        held = Some(std::fs::metadata(directory.join("dailies.json.lock")).is_ok());
        Ok(())
    })
    .expect("lock");
    assert_eq!(held, Some(true));
}

/// The matrix write refuses while Priority is running, because macOS keeps a
/// running app's `UserDefaults` in memory and the app rewrites the whole blob
/// on its next save. Writing anyway looks like it worked and is discarded the
/// moment the user drags one card.
#[test]
fn the_matrix_write_is_declared_as_needing_the_app_closed() {
    let definition = tool_definitions()
        .into_iter()
        .find(|tool| tool["name"] == "task_matrix_set")
        .expect("task_matrix_set should be in the tool surface");
    let description = definition["description"].as_str().unwrap();
    assert!(
        description.contains("closed"),
        "the description must say the app has to be closed: {description}"
    );
    let placements = &definition["inputSchema"]["properties"]["placements"];
    assert_eq!(placements["type"], "array");
    let required = definition["inputSchema"]["required"].as_array().unwrap();
    assert!(required.iter().any(|value| value == "placements"));
}

/// `cli.rs`, `tui/` and `mcp.rs` are three front ends onto one tool table, so a
/// tool only one of them can reach is a bug in the arrangement rather than a
/// missing feature. `task_matrix_set` shipped MCP-only and needed a subcommand
/// before it was reachable from a terminal at all.
#[test]
fn the_matrix_write_is_reachable_from_the_command_line() {
    let (name, arguments) = resolve(&Cli::parse_from([
        "priority",
        "matrix",
        "71981562:5:9",
        "71981563:-3:4",
    ]))
    .expect("`priority matrix` should resolve");
    assert_eq!(name, "task_matrix_set");
    let placements = arguments["placements"].as_array().unwrap();
    assert_eq!(placements.len(), 2);
    assert_eq!(placements[0]["task_id"], 71_981_562_i64);
    assert_eq!(placements[0]["urgency"], 5.0);
    assert_eq!(placements[1]["urgency"], -3.0);
    assert_eq!(placements[1]["importance"], 4.0);
}

/// A triple that does not parse must stop the command rather than quietly
/// placing a subset — a partial write to the matrix is worse than none.
#[test]
fn a_malformed_placement_is_rejected_rather_than_guessed_at() {
    for bad in ["not-a-triple", "1:2", "1:2:3:4", "x:2:3", "1:y:3"] {
        assert!(
            resolve(&Cli::parse_from(["priority", "matrix", bad])).is_err(),
            "`{bad}` should be rejected"
        );
    }
}

// -- the app's workspace, read-only ------------------------------------------

/// A throwaway database with only the columns these reads touch. The real
/// schema is GRDB's and has more; what matters here is the shape of the read.
fn workspace_fixture(rows: &[&str]) -> crate::workspace::Workspace {
    let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "priority-ws-{}-{unique}.sqlite",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let connection = rusqlite::Connection::open(&path).expect("fixture database");
    connection
        .execute_batch(
            "CREATE TABLE tasks (id TEXT PRIMARY KEY, title TEXT NOT NULL);
             CREATE TABLE focus_sessions (id TEXT PRIMARY KEY, startedAt DATETIME NOT NULL,
               endedAt DATETIME, phase TEXT NOT NULL, activeTaskId TEXT,
               workDurationSeconds INTEGER NOT NULL, breakDurationSeconds INTEGER,
               breakEndsAt DATETIME, activeTaskStartedAt DATETIME NOT NULL,
               activeBlockId TEXT, accumulatedSeconds INTEGER, pausedAt DATETIME,
               checkpointAt DATETIME, openEnded BOOLEAN NOT NULL DEFAULT 0);
             CREATE TABLE focus_queue_items (id TEXT PRIMARY KEY, sessionId TEXT NOT NULL,
               taskId TEXT NOT NULL, sortOrder INTEGER NOT NULL, state TEXT NOT NULL,
               completedAt DATETIME, skippedAt DATETIME, createdAt DATETIME NOT NULL,
               plannedSeconds INTEGER);
             CREATE TABLE focus_work_blocks (id TEXT PRIMARY KEY, sessionId TEXT, taskId TEXT,
               taskTitle TEXT NOT NULL, seconds INTEGER NOT NULL, recordedAt DATETIME NOT NULL,
               originalTaskId TEXT);",
        )
        .expect("fixture schema");
    for row in rows {
        connection.execute_batch(row).expect("fixture row");
    }
    crate::workspace::Workspace {
        database_path: path,
    }
}

fn utc(text: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
        .map(|naive| chrono::TimeZone::from_utc_datetime(&chrono::Utc, &naive))
        .expect("timestamp")
}

#[test]
fn a_focus_query_that_fails_is_an_error_not_no_session() {
    // Only "no such row" means not running. A row the query cannot read
    // must say so rather than report an idle timer.
    let workspace = workspace_fixture(&[
        "INSERT INTO focus_sessions (id, startedAt, phase, workDurationSeconds, activeTaskStartedAt)
         VALUES ('s1', '2026-09-25 15:21:38.349', 'running', 'not a number',
           '2026-09-25 15:21:38.349');",
    ]);
    assert!(workspace.focus_status(utc("2026-09-26 10:00:00")).is_err());

    let empty = workspace_fixture(&[]);
    assert_eq!(
        empty
            .focus_status(utc("2026-09-26 10:00:00"))
            .expect("status")["running"],
        json!(false)
    );
}

/// The reading that started all this. A block paused seconds after it started
/// and left overnight had been reported as nineteen hours, because the clock
/// was wall time since `startedAt`. Paused time does not accrue.
#[test]
fn a_paused_block_does_not_accrue_while_it_is_paused() {
    let workspace = workspace_fixture(&[
        "INSERT INTO tasks VALUES ('t1', 'Read 5 min');",
        "INSERT INTO focus_sessions (id, startedAt, phase, activeTaskId, workDurationSeconds,
           activeTaskStartedAt, accumulatedSeconds, pausedAt)
         VALUES ('s1', '2026-09-25 15:21:38.349', 'running', 't1', 1500,
           '2026-09-25 15:21:38.349', 0, '2026-09-25 15:21:38.830');",
    ]);

    let status = workspace
        .focus_status(utc("2026-09-26 10:00:00"))
        .expect("status");

    assert_eq!(status["running"], json!(true));
    assert_eq!(status["paused"], json!(true));
    assert_eq!(status["task"], json!("Read 5 min"));
    assert_eq!(status["elapsed_seconds"], json!(0));
    assert_eq!(status["remaining_seconds"], json!(1500));
    assert_eq!(status["clock"], json!("25:00"));
    assert_eq!(status["overrun"], json!(false));
}

#[test]
fn a_running_block_counts_from_the_active_task_rather_than_the_session() {
    let workspace = workspace_fixture(&[
        "INSERT INTO tasks VALUES ('t2', 'Write the report');",
        // Accumulated from an earlier task in the same session, plus four
        // minutes on this one.
        "INSERT INTO focus_sessions (id, startedAt, phase, activeTaskId, workDurationSeconds,
           activeTaskStartedAt, accumulatedSeconds)
         VALUES ('s2', '2026-09-26 08:00:00.000', 'running', 't2', 600,
           '2026-09-26 09:56:00.000', 60);",
    ]);

    let status = workspace
        .focus_status(utc("2026-09-26 10:00:00"))
        .expect("status");

    assert_eq!(status["elapsed_seconds"], json!(300));
    assert_eq!(status["remaining_seconds"], json!(300));
    assert_eq!(status["clock"], json!("5:00"));
}

#[test]
fn overrun_is_reported_as_overrun_rather_than_as_negative_time() {
    let workspace = workspace_fixture(&[
        "INSERT INTO tasks VALUES ('t3', 'Long one');",
        "INSERT INTO focus_sessions (id, startedAt, phase, activeTaskId, workDurationSeconds,
           activeTaskStartedAt, accumulatedSeconds)
         VALUES ('s3', '2026-09-26 09:00:00.000', 'running', 't3', 600,
           '2026-09-26 09:00:00.000', 0);",
    ]);

    let status = workspace
        .focus_status(utc("2026-09-26 09:12:30"))
        .expect("status");

    assert_eq!(status["overrun"], json!(true));
    assert_eq!(status["remaining_seconds"], json!(-150));
    assert_eq!(status["clock"], json!("2:30"));
}

/// A timer has no length: its clock is the time so far, and nothing remains.
#[test]
fn a_timer_reads_as_time_so_far() {
    let workspace = workspace_fixture(&[
        "INSERT INTO tasks VALUES ('t4', 'Open ended');",
        "INSERT INTO focus_sessions (id, startedAt, phase, activeTaskId, workDurationSeconds,
           activeTaskStartedAt, accumulatedSeconds, openEnded)
         VALUES ('s4', '2026-09-26 09:00:00.000', 'running', 't4', 86400,
           '2026-09-26 09:00:00.000', 0, 1);",
    ]);

    let status = workspace
        .focus_status(utc("2026-09-26 09:12:30"))
        .expect("status");

    assert_eq!(status["timer"], json!(true));
    assert_eq!(status["remaining_seconds"], json!(null));
    assert_eq!(status["overrun"], json!(false));
    assert_eq!(status["clock"], json!("12:30"));
}

/// A finished session is not a running one, however recently it ended.
#[test]
fn a_finished_session_reads_as_nothing_running() {
    let workspace = workspace_fixture(&[
        "INSERT INTO focus_sessions (id, startedAt, phase, workDurationSeconds,
           activeTaskStartedAt) VALUES ('s4', '2026-09-26 09:00:00.000', 'finished', 1500,
           '2026-09-26 09:00:00.000');",
    ]);

    let status = workspace
        .focus_status(utc("2026-09-26 09:30:00"))
        .expect("status");

    assert_eq!(status["running"], json!(false));
}

#[test]
fn recorded_blocks_come_back_newest_first_with_a_total() {
    let workspace = workspace_fixture(&[
        "INSERT INTO focus_work_blocks (id, taskTitle, seconds, recordedAt)
         VALUES ('b1', 'Morning', 900, '2026-09-26 08:00:00.000');",
        "INSERT INTO focus_work_blocks (id, taskTitle, seconds, recordedAt)
         VALUES ('b2', 'Afternoon', 1500, '2026-09-26 14:00:00.000');",
        "INSERT INTO focus_work_blocks (id, taskTitle, seconds, recordedAt)
         VALUES ('b3', 'Last week', 600, '2026-09-19 14:00:00.000');",
    ]);

    let since = Local.with_ymd_and_hms(2026, 9, 26, 4, 0, 0).unwrap();
    let until = Local.with_ymd_and_hms(2026, 9, 27, 4, 0, 0).unwrap();
    let history = workspace.focus_history(since, until).expect("history");

    let blocks = history["blocks"].as_array().expect("blocks");
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0]["task"], json!("Afternoon"));
    assert_eq!(blocks[1]["task"], json!("Morning"));
    assert_eq!(history["total_seconds"], json!(2400));
    assert_eq!(history["total"], json!("40:00"));
}

/// The one guarantee worth pinning: the connection is opened read-only, so a
/// bug here cannot corrupt the app's store.
#[test]
fn the_workspace_connection_refuses_to_write() {
    let workspace = workspace_fixture(&["INSERT INTO tasks VALUES ('t5', 'Untouchable');"]);
    let connection = rusqlite::Connection::open_with_flags(
        &workspace.database_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("read-only connection");

    assert!(
        connection
            .execute("UPDATE tasks SET title = 'changed' WHERE id = 't5'", [])
            .is_err()
    );
}

/// A missing database is a sentence, not a stack trace — the common cause is
/// simply never having opened the app.
#[test]
fn a_missing_database_says_where_it_looked() {
    let workspace = crate::workspace::Workspace {
        database_path: std::env::temp_dir().join("priority-definitely-absent.sqlite"),
    };
    let error = workspace
        .focus_status(utc("2026-09-26 10:00:00"))
        .expect_err("should fail");
    assert!(error.message.contains("priority-definitely-absent.sqlite"));
}

#[test]
fn instructions_set_names_its_scope_and_refuses_without_one() {
    let (tool, arguments) = resolved(&[
        "takt",
        "ws",
        "instructions-set",
        "--folder",
        "F",
        "--append",
        "Modules",
        "by",
        "code",
    ]);
    assert_eq!(tool, "agent_instructions_set");
    assert_eq!(arguments["scope"], json!("folder"));
    assert_eq!(arguments["id"], json!("F"));
    assert_eq!(arguments["text"], json!("Modules by code"));
    assert_eq!(arguments["mode"], json!("append"));

    let (tool, arguments) = resolved(&[
        "takt",
        "ws",
        "instructions-set",
        "--task",
        "T",
        "Warm",
        "up",
    ]);
    assert_eq!(tool, "agent_instructions_set");
    assert_eq!(arguments["scope"], json!("task"));
    assert_eq!(arguments["id"], json!("T"));

    let (tool, arguments) = resolved(&["takt", "ws", "instructions", "--account"]);
    assert_eq!(tool, "agent_instructions_get");
    assert_eq!(arguments["scope"], json!("account"));

    let cli = Cli::try_parse_from(["takt", "ws", "instructions-set", "Hello"]).unwrap();
    assert!(resolve(&cli).is_err());
}
