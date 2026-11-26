use rusqlite::Connection;

use super::*;
use crate::schema::migrate;

/// Midday on Monday 16 June 2025, UTC.
const NOW: i64 = 1_750_075_200_000;
const HOUR: i64 = 3_600_000;
const T: &str = "2025-06-01 00:00:00.000";

fn candidate(id: &str) -> Candidate {
    Candidate {
        id: id.into(),
        title: id.into(),
        is_daily_due_today: false,
        due_at_ms: None,
        start_at_ms: None,
        matrix_urgency: None,
        matrix_importance: None,
        priority: None,
        estimate_seconds: None,
        kanban_column: None,
        focus_rank: None,
        sort_order: 0,
        created_at_ms: NOW - 24 * HOUR,
        due_date: None,
        requirement_groups: vec![],
        logged_seconds: 0,
        minimum_block_seconds: None,
        requires_single_sitting: false,
        daily_remaining_seconds: None,
        daily_unavailable: None,
        list_priority: None,
        column_rank: None,
    }
}

fn today(id: &str) -> Candidate {
    Candidate {
        kanban_column: Some("today".into()),
        ..candidate(id)
    }
}

#[test]
fn the_tasks_behind_the_back_end_the_day() {
    let back = |id: &str, priority: i64| Candidate {
        priority: Some(priority),
        ..today(id)
    };
    let tasks = vec![
        Candidate {
            sort_order: 0,
            ..back("last", -1)
        },
        Candidate {
            sort_order: 1,
            ..today("first")
        },
        Candidate {
            sort_order: 2,
            ..back("before-last", -3)
        },
        Candidate {
            due_at_ms: Some(NOW - HOUR),
            ..candidate("late")
        },
        Candidate {
            priority: Some(-1),
            ..candidate("running")
        },
    ];
    let day = plan(&tasks, Some("running"), NOW, "UTC");
    assert_eq!(
        entries(&day),
        [
            ("running", "running"),
            ("first", "planned"),
            ("late", "overdue"),
            ("before-last", "planned"),
            ("last", "planned"),
        ]
    );
}

fn entries(plan: &[DayEntry]) -> Vec<(&str, &str)> {
    plan.iter()
        .map(|e| (e.id.as_str(), e.reason.as_str()))
        .collect()
}

#[test]
fn the_day_is_running_then_planned_then_deadlines_then_starts() {
    let tasks = vec![
        today("planned"),
        Candidate {
            due_at_ms: Some(NOW + HOUR),
            ..candidate("due")
        },
        Candidate {
            due_at_ms: Some(NOW - 24 * HOUR),
            ..candidate("late")
        },
        Candidate {
            start_at_ms: Some(NOW - HOUR),
            ..candidate("starting")
        },
        Candidate {
            due_at_ms: Some(NOW + 24 * HOUR),
            ..candidate("tomorrow")
        },
        Candidate {
            start_at_ms: Some(NOW - 24 * HOUR),
            ..candidate("startedYesterday")
        },
        Candidate {
            start_at_ms: Some(NOW + HOUR),
            ..candidate("startsLaterToday")
        },
        candidate("someday"),
        candidate("elsewhere"),
    ];
    let day = plan(&tasks, Some("elsewhere"), NOW, "UTC");
    assert_eq!(
        entries(&day),
        [
            ("elsewhere", "running"),
            ("planned", "planned"),
            ("late", "overdue"),
            ("due", "dueToday"),
            ("starting", "startsToday"),
        ]
    );
}

#[test]
fn a_hand_rank_orders_the_column_and_a_task_is_claimed_once() {
    let tasks = vec![
        Candidate {
            sort_order: 3,
            ..today("third")
        },
        Candidate {
            sort_order: 9,
            focus_rank: Some(1),
            ..today("first")
        },
        Candidate {
            sort_order: 1,
            due_at_ms: Some(NOW - 60_000),
            ..today("second")
        },
        Candidate {
            due_date: Some("2025-06-16".into()),
            ..candidate("allDay")
        },
    ];
    let day = plan(&tasks, None, NOW, "UTC");
    assert_eq!(
        entries(&day),
        [
            ("first", "planned"),
            ("second", "planned"),
            ("third", "planned"),
            ("allDay", "dueToday"),
        ]
    );
}

/// Twelve open tasks, two of them in Today and ranked last: the other ten
/// started yesterday, which ranks a task ahead of a commitment without
/// putting it in the day. A short ladder has to reach for the two.
fn workspace() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    let mut sql = format!(
        "INSERT INTO workspaces (id, name, createdAt, updatedAt) VALUES ('w', 'Mine', '{T}', '{T}');
         INSERT INTO task_lists (id, workspaceId, name, sortOrder, isArchived, createdAt, updatedAt)
           VALUES ('l', 'w', 'Work', 0, 0, '{T}', '{T}');"
    );
    for index in 0..12 {
        sql += &format!(
            "INSERT INTO tasks (id, listId, title, status, sortOrder, itemKind, createdAt, updatedAt)
               VALUES ('t{index:02}', 'l', 'Task {index}', 'open', {index}, 'task', '{T}', '{T}');
             INSERT INTO task_metadata (taskId, startAt, kanbanColumn, updatedAt)
               VALUES ('t{index:02}', {start}, {column}, '{T}');",
            start = if index >= 10 {
                "NULL"
            } else {
                "'2025-06-15 12:00:00.000'"
            },
            column = if index >= 10 { "'today'" } else { "NULL" },
        );
    }
    connection.execute_batch(&sql).unwrap();
    connection
}

/// Arranging the day by hand ranks its tasks, and the day re-sorts what is
/// in Today by that rank, so a task sent to the back has to go after it.
#[test]
fn a_task_sent_to_the_back_ends_the_day_even_when_ranked_first() {
    let connection = workspace();
    connection
        .execute_batch(&format!(
            "UPDATE task_metadata SET focusRank = 0, priority = -1, updatedAt = '{T}' WHERE taskId = 't10';
             UPDATE task_metadata SET focusRank = 1 WHERE taskId = 't11';"
        ))
        .unwrap();
    let read = next_up(
        &connection,
        NOW,
        "UTC",
        &FocusContext::default(),
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        entries(&read.day_plan),
        [("t11", "planned"), ("t10", "planned")]
    );
}

#[test]
fn a_card_sent_to_the_back_ends_the_day_too() {
    let connection = workspace();
    // t10 is a card (a parent of open work, so never a candidate itself)
    // with a subtask filed in Today; the card is at -1.
    connection
        .execute_batch(&format!(
            "INSERT INTO tasks (id, listId, parentTaskId, title, status, sortOrder, itemKind, createdAt, updatedAt)
               VALUES ('a', 'l', 't10', 'A', 'open', 0, 'task', '{T}', '{T}');
             INSERT INTO task_metadata (taskId, kanbanColumn, updatedAt) VALUES ('a', 'today', '{T}');
             UPDATE task_metadata SET focusRank = 0, priority = -1 WHERE taskId = 't10';
             UPDATE task_metadata SET focusRank = 1 WHERE taskId = 't11';"
        ))
        .unwrap();
    let read = next_up(
        &connection,
        NOW,
        "UTC",
        &FocusContext::default(),
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        entries(&read.day_plan),
        [("t11", "planned"), ("t10", "planned")]
    );
}

#[test]
fn a_limited_ladder_keeps_its_head_and_the_days_tasks() {
    let connection = workspace();
    // Today's tasks lead the rest, so three late tasks make a head of the
    // ladder that the day's tasks fall outside.
    connection
        .execute_batch(
            "UPDATE tasks SET dueAt = '2025-06-16 08:00:00.000' WHERE id IN ('t00', 't01', 't02');",
        )
        .unwrap();
    let context = FocusContext::default();
    let full = next_up(&connection, NOW, "UTC", &context, None, None).unwrap();
    assert_eq!(full.ranked.len(), 12);
    assert_eq!(full.ranked_count, 12);

    let short = next_up(&connection, NOW, "UTC", &context, None, Some(3)).unwrap();
    let ids: Vec<&str> = short
        .ranked
        .iter()
        .map(|s| s.candidate.id.as_str())
        .collect();
    assert_eq!(ids, ["t00", "t01", "t02", "t10", "t11"]);
    assert_eq!(short.ranked_count, 12);
    assert_eq!(
        entries(&short.day_plan),
        [
            ("t10", "planned"),
            ("t11", "planned"),
            ("t00", "overdue"),
            ("t01", "overdue"),
            ("t02", "overdue")
        ]
    );
    // Everything else is the full read's, unchanged.
    assert_eq!(short.day_plan, full.day_plan);
    assert_eq!(short.blocked, full.blocked);
    assert_eq!(short.next_evaluation_at_ms, full.next_evaluation_at_ms);
    for scored in &short.ranked {
        assert!(full.ranked.contains(scored));
    }
}

#[test]
fn a_card_in_today_with_subtasks_stands_in_the_day_once_in_their_place() {
    let connection = workspace();
    // t10 becomes a card under a this-week task, with two subtasks: one filed
    // in Today itself, one filed nowhere. t11 stays a task of its own.
    connection
        .execute_batch(&format!(
            "INSERT INTO tasks (id, listId, title, status, sortOrder, itemKind, createdAt, updatedAt)
               VALUES ('week', 'l', 'Week', 'open', 20, 'task', '{T}', '{T}');
             INSERT INTO task_metadata (taskId, kanbanColumn, updatedAt) VALUES ('week', 'this-week', '{T}');
             UPDATE tasks SET parentTaskId = 'week' WHERE id = 't10';
             INSERT INTO tasks (id, listId, parentTaskId, title, status, sortOrder, itemKind, createdAt, updatedAt)
               VALUES ('a', 'l', 't10', 'A', 'open', 0, 'task', '{T}', '{T}'),
                      ('b', 'l', 't10', 'B', 'open', 1, 'task', '{T}', '{T}');
             INSERT INTO task_metadata (taskId, kanbanColumn, updatedAt) VALUES ('a', 'today', '{T}');"
        ))
        .unwrap();
    let context = FocusContext::default();
    let read = next_up(&connection, NOW, "UTC", &context, None, None).unwrap();
    assert_eq!(
        entries(&read.day_plan),
        [("t10", "planned"), ("t11", "planned")]
    );

    // A running subtask keeps its own row, ahead of its card.
    let running = next_up(&connection, NOW, "UTC", &context, Some("b"), None).unwrap();
    assert_eq!(
        entries(&running.day_plan),
        [("b", "running"), ("t10", "planned"), ("t11", "planned")]
    );
}

#[test]
fn a_card_arranged_by_hand_keeps_its_place_rather_than_its_subtasks() {
    let connection = workspace();
    // t10 is a card with a subtask filed in Today and given no rank, as a
    // subtask planned before its card was. Arranging the day ranked the card
    // after t11; the unranked subtask must not drag it back ahead.
    connection
        .execute_batch(&format!(
            "INSERT INTO tasks (id, listId, parentTaskId, title, status, sortOrder, itemKind, createdAt, updatedAt)
               VALUES ('a', 'l', 't10', 'A', 'open', 0, 'task', '{T}', '{T}');
             INSERT INTO task_metadata (taskId, kanbanColumn, updatedAt) VALUES ('a', 'today', '{T}');
             UPDATE task_metadata SET focusRank = 1 WHERE taskId = 't10';
             UPDATE task_metadata SET focusRank = 0 WHERE taskId = 't11';"
        ))
        .unwrap();
    let context = FocusContext::default();
    let read = next_up(&connection, NOW, "UTC", &context, None, None).unwrap();
    assert_eq!(
        entries(&read.day_plan),
        [("t11", "planned"), ("t10", "planned")]
    );

    // And moved back up, it comes back up.
    connection
        .execute_batch(
            "UPDATE task_metadata SET focusRank = 0 WHERE taskId = 't10';
             UPDATE task_metadata SET focusRank = 1 WHERE taskId = 't11';",
        )
        .unwrap();
    let read = next_up(&connection, NOW, "UTC", &context, None, None).unwrap();
    assert_eq!(
        entries(&read.day_plan),
        [("t10", "planned"), ("t11", "planned")]
    );
}

#[test]
fn a_prioritised_lists_task_leads_its_column_but_not_today() {
    let connection = workspace();
    connection
        .execute_batch(&format!(
            "INSERT INTO task_lists (id, workspaceId, name, sortOrder, isArchived, createdAt, updatedAt, priorityRank)
               VALUES ('exams', 'w', 'Exams', 1, 0, '{T}', '{T}', 1);
             INSERT INTO tasks (id, listId, title, status, sortOrder, itemKind, createdAt, updatedAt)
               VALUES ('revise', 'exams', 'Revise', 'open', 0, 'task', '{T}', '{T}');"
        ))
        .unwrap();
    let read = next_up(
        &connection,
        NOW,
        "UTC",
        &FocusContext::default(),
        None,
        None,
    )
    .unwrap();
    let ids: Vec<&str> = read
        .ranked
        .iter()
        .map(|s| s.candidate.id.as_str())
        .take(4)
        .collect();
    // Today's two first, then the prioritised list's backlog task ahead of
    // the started backlog tasks of a list nobody prioritised.
    assert_eq!(ids, ["t10", "t11", "revise", "t00"]);
    let revise = read
        .ranked
        .iter()
        .find(|s| s.candidate.id == "revise")
        .unwrap();
    assert_eq!(revise.candidate.list_priority, Some(1));
    assert_eq!(revise.candidate.column_rank, Some(3));
}
