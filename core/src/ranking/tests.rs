use super::*;

const NOW: i64 = 1_773_223_200_000; // Wednesday 11 March 2026, 10:00 UTC
const HOUR: i64 = 3_600_000;

fn task(id: &str) -> Candidate {
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

fn ids(ranking: &Ranking) -> Vec<&str> {
    ranking
        .ranked
        .iter()
        .map(|s| s.candidate.id.as_str())
        .collect()
}

#[test]
fn late_work_comes_first_then_today_then_the_rest_by_importance() {
    let late = Candidate {
        due_at_ms: Some(NOW - 2 * 24 * HOUR - HOUR),
        ..task("late")
    };
    let today = Candidate {
        due_at_ms: Some(NOW + 3 * HOUR),
        ..task("today")
    };
    let important = Candidate {
        matrix_importance: Some(2),
        ..task("important")
    };
    let plain = task("plain");
    let ranking = evaluate(
        &[plain, important, today, late],
        NOW,
        "UTC",
        &FocusContext::default(),
    );
    assert_eq!(ids(&ranking), ["late", "today", "important", "plain"]);
    assert_eq!(ranking.ranked[0].reason, "overdue");
    assert_eq!(
        ranking.ranked[0].explanation.as_deref(),
        Some("overdue by 2 days")
    );
    assert_eq!(ranking.ranked[2].reason, "importance");
    assert_eq!(ranking.next_evaluation_at_ms, Some(NOW + 3 * HOUR));
}

#[test]
fn a_pin_places_free_work_but_does_not_jump_a_tier() {
    let pinned = Candidate {
        focus_rank: Some(0),
        sort_order: 9,
        ..task("pinned")
    };
    let first = Candidate {
        sort_order: 1,
        ..task("first")
    };
    let late = Candidate {
        due_at_ms: Some(NOW - HOUR),
        ..task("late")
    };
    let ranking = evaluate(&[first, pinned, late], NOW, "UTC", &FocusContext::default());
    assert_eq!(ids(&ranking), ["late", "pinned", "first"]);
}

#[test]
fn blocked_work_is_kept_apart_with_its_reasons() {
    let later = Candidate {
        start_at_ms: Some(NOW + HOUR),
        ..task("later")
    };
    let ranking = evaluate(&[later, task("now")], NOW, "UTC", &FocusContext::default());
    assert_eq!(ids(&ranking), ["now"]);
    assert_eq!(
        ranking.blocked[0].reasons,
        [Unavailable::StartsLater { at_ms: NOW + HOUR }]
    );
}

#[test]
fn a_negated_requirement_holds_while_its_condition_does_not() {
    let needing = |id: &str, groups: &[&[&str]]| Candidate {
        requirement_groups: groups
            .iter()
            .map(|group| group.iter().map(|m| m.to_string()).collect())
            .collect(),
        ..task(id)
    };
    let tasks = [
        needing("away", &[&["!home"]]),
        needing("either", &[&["campus", "!quiet"]]),
        needing("both", &[&["home", "!home"]]),
    ];
    let context = |held: &[&str]| FocusContext {
        condition_ids: held.iter().map(|id| id.to_string()).collect(),
        ..Default::default()
    };
    let ranked = |held: &[&str]| {
        let mut ids: Vec<String> = evaluate(&tasks, NOW, "UTC", &context(held))
            .ranked
            .into_iter()
            .map(|s| s.candidate.id)
            .collect();
        ids.sort();
        ids
    };
    assert_eq!(ranked(&[]), ["away", "both", "either"]);
    assert_eq!(ranked(&["home", "quiet"]), ["both"]);
    assert_eq!(ranked(&["campus", "home", "quiet"]), ["both", "either"]);
    // What holds it back is the requirement as written.
    let ranking = evaluate(&tasks, NOW, "UTC", &context(&["home", "quiet"]));
    let away = ranking
        .blocked
        .iter()
        .find(|b| b.candidate.id == "away")
        .unwrap();
    assert_eq!(
        away.reasons,
        [Unavailable::MissingConditions {
            groups: vec![vec!["!home".into()]]
        }]
    );
    // Counted as a condition task, like any other requirement.
    assert_eq!(ranking.ranked[0].reason, "condition");
}

#[test]
fn priority_one_is_the_most_urgent_and_no_priority_comes_last() {
    let with = |id: &str, priority: Option<i64>| Candidate {
        priority,
        ..task(id)
    };
    let ranking = evaluate(
        &[
            with("none", None),
            with("four", Some(4)),
            with("one", Some(1)),
            with("three", Some(3)),
            with("two", Some(2)),
        ],
        NOW,
        "UTC",
        &FocusContext::default(),
    );
    assert_eq!(ids(&ranking), ["one", "two", "three", "four", "none"]);
    assert_eq!(ranking.ranked[0].reason, "priority");
    assert_eq!(ranking.ranked[4].reason, "order");
}

/// -1 is the very back, -2 just before it, and every negative one comes
/// after the tasks with no priority at all.
#[test]
fn negative_priorities_come_after_no_priority_with_minus_one_last() {
    let with = |id: &str, priority: Option<i64>| Candidate {
        priority,
        ..task(id)
    };
    let ranking = evaluate(
        &[
            with("minus-one", Some(-1)),
            with("none", None),
            with("minus-four", Some(-4)),
            with("four", Some(4)),
            with("minus-two", Some(-2)),
            with("one", Some(1)),
            with("minus-three", Some(-3)),
        ],
        NOW,
        "UTC",
        &FocusContext::default(),
    );
    assert_eq!(
        ids(&ranking),
        [
            "one",
            "four",
            "none",
            "minus-four",
            "minus-three",
            "minus-two",
            "minus-one"
        ]
    );
    // A negative priority is no reason to be up next.
    assert_eq!(ranking.ranked[6].reason, "order");
}

fn in_list(id: &str, rank: Option<i64>) -> Candidate {
    Candidate {
        list_priority: rank,
        ..task(id)
    }
}

#[test]
fn within_a_column_a_prioritised_list_beats_a_started_task_elsewhere() {
    let started = Candidate {
        start_at_ms: Some(NOW - 24 * HOUR),
        ..in_list("started", None)
    };
    let exam = in_list("exam", Some(1));
    let ranking = evaluate(&[started, exam], NOW, "UTC", &FocusContext::default());
    assert_eq!(ids(&ranking), ["exam", "started"]);
}

#[test]
fn the_first_prioritised_list_beats_the_second() {
    let second = Candidate {
        matrix_importance: Some(2),
        ..in_list("second", Some(2))
    };
    let first = in_list("first", Some(1));
    let ranking = evaluate(&[second, first], NOW, "UTC", &FocusContext::default());
    assert_eq!(ids(&ranking), ["first", "second"]);
}

#[test]
fn lateness_and_due_today_still_beat_list_priority() {
    let late = Candidate {
        due_at_ms: Some(NOW - HOUR),
        ..in_list("late", None)
    };
    let due = Candidate {
        due_at_ms: Some(NOW + 3 * HOUR),
        ..in_list("due", None)
    };
    let today = Candidate {
        kanban_column: Some("today".into()),
        ..in_list("today", Some(1))
    };
    let ranking = evaluate(&[today, due, late], NOW, "UTC", &FocusContext::default());
    assert_eq!(ids(&ranking), ["late", "due", "today"]);
}

#[test]
fn a_column_further_right_beats_a_prioritised_list_further_left() {
    let today = Candidate {
        kanban_column: Some("today".into()),
        ..in_list("today", None)
    };
    let daily = Candidate {
        is_daily_due_today: true,
        sort_order: 1,
        ..in_list("daily", None)
    };
    let week = Candidate {
        kanban_column: Some("this-week".into()),
        ..in_list("week", None)
    };
    let backlog = in_list("backlog", Some(1));
    let ranking = evaluate(
        &[backlog, week, daily, today],
        NOW,
        "UTC",
        &FocusContext::default(),
    );
    assert_eq!(ids(&ranking), ["today", "daily", "week", "backlog"]);
}

#[test]
fn a_rank_on_the_lists_own_board_orders_the_columns() {
    // A list whose board puts a column the stock board lacks just left of
    // Today: the core reads it as 1, ahead of a stock board's backlog.
    let custom = Candidate {
        kanban_column: Some("doing".into()),
        column_rank: Some(1),
        ..in_list("custom", None)
    };
    let backlog = Candidate {
        column_rank: Some(4),
        ..in_list("backlog", Some(1))
    };
    let ranking = evaluate(&[backlog, custom], NOW, "UTC", &FocusContext::default());
    assert_eq!(ids(&ranking), ["custom", "backlog"]);
}
