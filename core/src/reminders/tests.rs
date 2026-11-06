use rusqlite::{Connection, params};

use super::*;
use crate::schema::migrate;

const T: &str = "2024-01-01 00:00:00.000";
const ZONE: &str = "Europe/London";

/// 2026-11-06 08:00 in London (GMT).
const NOW: i64 = 1_793_952_000_000;

fn ms(text: &str) -> i64 {
    parse_stored(text).unwrap().timestamp_millis()
}

fn on() -> ReminderSettings {
    ReminderSettings {
        due_time: true,
        due_today: true,
        dailies_left: true,
        ..ReminderSettings::default()
    }
}

/// List `l` in workspace `w`, no tasks.
fn workspace() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON")
        .unwrap();
    migrate(&mut connection).unwrap();
    connection
        .execute_batch(&format!(
            "INSERT INTO workspaces (id, name, createdAt, updatedAt) VALUES ('w', 'Mine', '{T}', '{T}');
             INSERT INTO task_lists (id, workspaceId, name, sortOrder, createdAt, updatedAt) VALUES
               ('l', 'w', 'Inbox', 0, '{T}', '{T}');"
        ))
        .unwrap();
    connection
}

fn task(connection: &Connection, id: &str, title: &str, due_at: Option<&str>) {
    connection
        .execute(
            "INSERT INTO tasks (id, listId, title, sortOrder, dueAt, createdAt, updatedAt)
             VALUES (?1, 'l', ?2, 0, ?3, ?4, ?4)",
            params![id, title, due_at, T],
        )
        .unwrap();
}

fn due_day(connection: &Connection, id: &str, title: &str, day: &str) {
    task(connection, id, title, None);
    connection
        .execute(
            "INSERT INTO task_metadata (taskId, tagsJSON, externalLinksJSON, planningJSON, updatedAt)
             VALUES (?1, '[]', '[]', ?2, ?3)",
            params![id, format!("{{\"dueDate\":\"{day}\"}}"), T],
        )
        .unwrap();
}

fn daily(connection: &Connection, id: &str, title: &str, mask: i64) {
    task(connection, &format!("t{id}"), title, None);
    connection
        .execute(
            "INSERT INTO dailies (id, taskId, activeWeekdaysMask, sortOrder, createdAt, updatedAt)
             VALUES (?1, ?2, ?3, 0, ?4, ?4)",
            params![id, format!("t{id}"), mask, T],
        )
        .unwrap();
}

fn read(connection: &Connection, settings: &ReminderSettings, days: u32) -> Vec<Reminder> {
    reminders(connection, settings, NOW, ZONE, days, 60).unwrap()
}

#[test]
fn everything_off_sets_nothing() {
    let connection = workspace();
    task(&connection, "a", "Call", Some("2026-11-06 15:00:00.000"));
    daily(&connection, "d", "Stretch", 127);
    assert!(read(&connection, &ReminderSettings::default(), 7).is_empty());
}

#[test]
fn a_due_time_goes_off_then_and_not_once_past_or_done() {
    let connection = workspace();
    task(
        &connection,
        "a",
        "Call the bank",
        Some("2026-11-06 15:00:00.000"),
    );
    task(&connection, "past", "Gone", Some("2026-11-06 07:00:00.000"));
    task(&connection, "far", "Later", Some("2026-11-20 07:00:00.000"));
    task(&connection, "done", "Done", Some("2026-11-06 16:00:00.000"));
    connection
        .execute("UPDATE tasks SET status = 'done' WHERE id = 'done'", [])
        .unwrap();
    let settings = ReminderSettings {
        due_time: true,
        ..ReminderSettings::default()
    };
    let got = read(&connection, &settings, 7);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].id, "due.a");
    assert_eq!(got[0].kind, ReminderKind::DueTime);
    assert_eq!(got[0].fire_at_ms, ms("2026-11-06 15:00:00.000"));
    assert_eq!(got[0].body, "Call the bank");
    assert_eq!(got[0].task_id.as_deref(), Some("a"));
}

#[test]
fn due_today_names_timed_and_untimed_tasks_at_its_minute() {
    let connection = workspace();
    due_day(&connection, "a", "Essay", "2026-11-06");
    task(&connection, "b", "Call", Some("2026-11-06 15:00:00.000"));
    due_day(&connection, "c", "Rent", "2026-11-07");
    let settings = ReminderSettings {
        due_today: true,
        ..ReminderSettings::default()
    };
    let got = read(&connection, &settings, 2);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].id, "today.2026-11-06");
    assert_eq!(got[0].fire_at_ms, ms("2026-11-06 09:00:00.000"));
    assert_eq!(got[0].title, "2 due today");
    assert!(got[0].body.contains("Essay") && got[0].body.contains("Call"));
    assert_eq!(got[1].id, "today.2026-11-07");
    assert_eq!(got[1].title, "1 due today");
    assert_eq!(got[1].body, "Rent");
}

#[test]
fn due_today_is_skipped_once_its_minute_has_passed() {
    let connection = workspace();
    due_day(&connection, "a", "Essay", "2026-11-06");
    let settings = ReminderSettings {
        due_today: true,
        due_today_minute: 7 * 60,
        ..ReminderSettings::default()
    };
    assert!(read(&connection, &settings, 1).is_empty());
}

#[test]
fn a_long_summary_counts_the_rest() {
    let connection = workspace();
    for (id, title) in [("a", "A"), ("b", "B"), ("c", "C"), ("d", "D"), ("e", "E")] {
        due_day(&connection, id, title, "2026-11-06");
    }
    let settings = ReminderSettings {
        due_today: true,
        ..ReminderSettings::default()
    };
    let got = read(&connection, &settings, 1);
    assert_eq!(got[0].title, "5 due today");
    assert!(got[0].body.ends_with("and 2 more"), "{}", got[0].body);
}

#[test]
fn dailies_left_leaves_out_done_ones_and_days_off() {
    let connection = workspace();
    daily(&connection, "every", "Stretch", 127);
    daily(&connection, "ticked", "Read", 127);
    // 2026-11-06 is a Friday: Foundation's 6, bit 5. Saturdays only (bit 6).
    daily(&connection, "saturday", "Long run", 1 << 6);
    connection
        .execute(
            "INSERT INTO daily_contributions (id, dailyId, taskId, dayKey, completedAt, createdAt)
             VALUES ('c', 'ticked', 'tticked', '2026-11-06', ?1, ?1)",
            [T],
        )
        .unwrap();
    let settings = ReminderSettings {
        dailies_left: true,
        ..ReminderSettings::default()
    };
    let got = read(&connection, &settings, 2);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].id, "dailies.2026-11-06");
    assert_eq!(got[0].fire_at_ms, ms("2026-11-06 20:00:00.000"));
    assert_eq!(got[0].title, "1 daily left");
    assert_eq!(got[0].body, "Stretch");
    assert_eq!(got[1].id, "dailies.2026-11-07");
    assert_eq!(got[1].title, "3 dailies left");
}

#[test]
fn no_daily_reminder_once_all_are_done() {
    let connection = workspace();
    daily(&connection, "d", "Stretch", 127);
    connection
        .execute(
            "INSERT INTO daily_contributions (id, dailyId, taskId, dayKey, completedAt, createdAt)
             VALUES ('c', 'd', 'td', '2026-11-06', ?1, ?1)",
            [T],
        )
        .unwrap();
    let settings = ReminderSettings {
        dailies_left: true,
        ..ReminderSettings::default()
    };
    assert!(read(&connection, &settings, 1).is_empty());
}

#[test]
fn soonest_first_and_capped() {
    let connection = workspace();
    task(&connection, "a", "A", Some("2026-11-06 21:00:00.000"));
    task(&connection, "b", "B", Some("2026-11-06 08:30:00.000"));
    daily(&connection, "d", "Stretch", 127);
    let all = read(&connection, &on(), 1);
    let ids: Vec<&str> = all.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        ["due.b", "today.2026-11-06", "dailies.2026-11-06", "due.a"]
    );
    let capped = reminders(&connection, &on(), NOW, ZONE, 1, 2).unwrap();
    assert_eq!(capped.len(), 2);
    assert_eq!(capped[1].id, "today.2026-11-06");
}
