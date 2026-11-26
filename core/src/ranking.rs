//! The next-up ranking: which available task comes first, and why.
//! Replaces `NextUpSelector.evaluate` in Swift's TaktCore and its Kotlin port.
//!
//! Availability is decided first. The available tasks are then ordered by a
//! deterministic precedence: lateness and deadline risk before anything else,
//! then the board column (further right first, Today and dailies due today
//! first of all), then the list's priority (prioritised lists in their
//! order, the rest after), then conditions, a start, importance, priority,
//! nearness, urgency, age, size and position. So a prioritised list leads
//! within a column, never across one: its backlog stays behind anyone's
//! Today. Pins (a task's focus rank) place work within a tier but never
//! reverse lateness or deadline slack.

use std::cmp::Ordering;

use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;

use crate::focus::{Candidate, FocusContext, Unavailable, reasons};
use crate::periodic;

/// Today's column on the board.
const TODAY_COLUMN: &str = "today";
/// How far ahead a due date still counts as near.
const DUE_HORIZON_DAYS: f64 = 14.0;
const MINIMUM_DEADLINE_BUFFER: f64 = 300.0;
const DEADLINE_BUFFER_FRACTION: f64 = 0.2;

/// An available task with why it is where it is. `ScoredNextUp`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Scored {
    pub candidate: Candidate,
    pub score: f64,
    /// `NextUpReason`'s raw value: daily, overdue, dueToday, dueSoon, today,
    /// importance, priority, order, condition, started or deadlineRisk.
    pub reason: String,
    /// A specific explanation when there is one; otherwise the reason's own.
    pub explanation: Option<String>,
}

/// A task ruled out now, with every reason. `BlockedFocusTask`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Blocked {
    pub candidate: Candidate,
    pub reasons: Vec<Unavailable>,
}

/// The day's order. `FocusRanking`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Ranking {
    pub ranked: Vec<Scored>,
    pub blocked: Vec<Blocked>,
    /// The next moment the order could change on its own, if any.
    pub next_evaluation_at_ms: Option<i64>,
}

/// Orders `candidates` for `now` in `context`. `NextUpSelector.evaluate`.
pub fn evaluate(
    candidates: &[Candidate],
    now_ms: i64,
    zone: &str,
    context: &FocusContext,
) -> Ranking {
    let zone = periodic::zone(zone);
    let mut available = Vec::new();
    let mut blocked = Vec::new();
    for task in candidates {
        let why = reasons(task, context, now_ms);
        if why.is_empty() {
            available.push(task.clone());
        } else {
            blocked.push(Blocked {
                candidate: task.clone(),
                reasons: why,
            });
        }
    }
    available.sort_by(|a, b| precedence(a, b, now_ms, zone));
    let mut ranked = Vec::new();
    let mut index = 0;
    while index < available.len() {
        let key = primary(&available[index], now_ms, zone);
        let mut end = index + 1;
        while end < available.len() && primary(&available[end], now_ms, zone) == key {
            end += 1;
        }
        ranked.extend(place(&available[index..end]));
        index = end;
    }

    let next_day = periodic::resolve(
        zone,
        local(now_ms, zone)
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight exists")
            + Duration::days(1),
    )
    .timestamp_millis();
    let mut boundaries: Vec<i64> = Vec::new();
    for task in candidates {
        boundaries.extend(task.start_at_ms);
        let deadline = effective_deadline(task, zone);
        boundaries.extend(deadline);
        if task.due_date.is_none()
            && let Some(due) = task.due_at_ms
        {
            boundaries.push(due + 1);
        }
        if let (Some(deadline), Some(work)) =
            (deadline, task.remaining_seconds().filter(|w| *w > 0))
        {
            boundaries.push(deadline - ((work as f64 + buffer(work)) * 1000.0) as i64);
        }
        if let Some(end) = context.ends_at_ms {
            let minimum = task.minimum_block_seconds.unwrap_or(60).max(60);
            boundaries.push(end - minimum * 1000 + 1);
            if let Some(work) = task.remaining_seconds()
                && (context.mode == "finish" || task.requires_single_sitting)
            {
                boundaries.push(end - work.max(minimum) * 1000 + 1);
            }
        }
    }
    boundaries.extend(context.ends_at_ms);
    boundaries.push(next_day);
    blocked.sort_by(|a, b| precedence(&a.candidate, &b.candidate, now_ms, zone));
    Ranking {
        ranked: ranked
            .into_iter()
            .map(|task| score(task, now_ms, zone))
            .collect(),
        blocked,
        next_evaluation_at_ms: boundaries.into_iter().filter(|at| *at > now_ms).min(),
    }
}

fn local(ms: i64, zone: Tz) -> DateTime<Tz> {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .unwrap_or_default()
        .with_timezone(&zone)
}

fn buffer(seconds: i64) -> f64 {
    MINIMUM_DEADLINE_BUFFER.max(seconds as f64 * DEADLINE_BUFFER_FRACTION)
}

/// The end of a due date's local day, or the due time.
/// `NextUpCandidate.effectiveDeadline`.
pub(crate) fn effective_deadline(task: &Candidate, zone: Tz) -> Option<i64> {
    if let Some(day) = task
        .due_date
        .as_deref()
        .and_then(|text| chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").ok())
        .filter(|day| task.due_date.as_deref() == Some(day.format("%Y-%m-%d").to_string().as_str()))
    {
        let midnight = day.and_hms_opt(0, 0, 0).expect("midnight exists") + Duration::days(1);
        return Some(periodic::resolve(zone, midnight).timestamp_millis());
    }
    task.due_at_ms
}

/// The tier a task falls in, with what orders it inside the tier: late (by
/// deadline), due today (by deadline, then age), at risk (by slack), the rest.
fn primary(task: &Candidate, now_ms: i64, zone: Tz) -> Vec<f64> {
    let Some(due) = effective_deadline(task, zone) else {
        return vec![3.0, 0.0];
    };
    let seconds = |ms: i64| ms as f64 / 1000.0;
    let late = if task.due_date.is_none() {
        due < now_ms
    } else {
        due <= now_ms
    };
    if late {
        return vec![0.0, seconds(due)];
    }
    let today = local(now_ms, zone).date_naive();
    let is_today = match &task.due_date {
        Some(date) => *date == today.format("%Y-%m-%d").to_string(),
        None => local(due, zone).date_naive() == today,
    };
    if is_today {
        return vec![1.0, seconds(due), seconds(task.created_at_ms)];
    }
    if let Some(work) = task.remaining_seconds().filter(|w| *w > 0) {
        let slack = seconds(due - now_ms) - work as f64 - buffer(work);
        if slack <= 0.0 {
            return vec![2.0, slack];
        }
    }
    vec![3.0, 0.0]
}

fn lexicographic(a: &[f64], b: &[f64]) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        match x.partial_cmp(y).unwrap_or(Ordering::Equal) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    a.len().cmp(&b.len())
}

/// `NextUpSelector.precedes`, as an ordering.
fn precedence(left: &Candidate, right: &Candidate, now_ms: i64, zone: Tz) -> Ordering {
    let (lhs, rhs) = (primary(left, now_ms, zone), primary(right, now_ms, zone));
    if lhs != rhs {
        return lexicographic(&lhs, &rhs);
    }
    let secondary = |task: &Candidate| -> Vec<f64> {
        let days = effective_deadline(task, zone).map_or(f64::INFINITY, |due| {
            ((due - now_ms) as f64 / 1000.0 / 86_400.0).max(0.0)
        });
        vec![
            column_rank(task) as f64,
            // Unprioritised lists come after every prioritised one.
            task.list_priority.map_or(f64::INFINITY, |rank| rank as f64),
            if task.requirement_groups.is_empty() {
                1.0
            } else {
                0.0
            },
            if task.start_at_ms.is_none() { 1.0 } else { 0.0 },
            -(task.matrix_importance.unwrap_or(0) as f64),
            // 1 is the most urgent, 4 the least; no priority comes after all
            // four, and -4 to -1 after that, -1 last.
            crate::tasks::priority_rank(task.priority) as f64,
            if days <= DUE_HORIZON_DAYS {
                days
            } else {
                f64::INFINITY
            },
            -(task.matrix_urgency.unwrap_or(0) as f64),
            task.created_at_ms as f64 / 1000.0,
            task.remaining_seconds()
                .map_or(i64::MAX as f64, |r| r as f64),
            task.sort_order as f64,
        ]
    };
    let (a, b) = (secondary(left), secondary(right));
    if a == b {
        left.id.cmp(&right.id)
    } else {
        lexicographic(&a, &b)
    }
}

/// How committed a task's column is, further right on its board first:
/// Today and a daily due today are 0, the commitments. A candidate the core
/// read carries its rank on its own list's board; one a client built falls
/// back on the stock board's order.
fn column_rank(task: &Candidate) -> i64 {
    if task.is_daily_due_today || task.kanban_column.as_deref() == Some(TODAY_COLUMN) {
        return 0;
    }
    if let Some(rank) = task.column_rank {
        return rank;
    }
    match task.kanban_column.as_deref() {
        Some(crate::board_columns::THIS_WEEK_COLUMN_ID) => 1,
        Some("in-progress") => 2,
        _ => 3,
    }
}

/// Puts pinned tasks at their ranks within a tier, the rest in order around
/// them. `NextUpSelector.place`.
fn place(tasks: &[Candidate]) -> Vec<Candidate> {
    let mut pinned: Vec<&Candidate> = tasks.iter().filter(|t| t.focus_rank.is_some()).collect();
    pinned.sort_by(|a, b| {
        (a.focus_rank.unwrap_or(0), &a.id).cmp(&(b.focus_rank.unwrap_or(0), &b.id))
    });
    let free: Vec<&Candidate> = tasks.iter().filter(|t| t.focus_rank.is_none()).collect();
    let (mut pin, mut unpinned) = (0, 0);
    let mut result = Vec::with_capacity(tasks.len());
    while result.len() < tasks.len() {
        if pin < pinned.len()
            && (pinned[pin].focus_rank.unwrap_or(0) <= result.len() as i64
                || unpinned == free.len())
        {
            result.push(pinned[pin].clone());
            pin += 1;
        } else {
            result.push(free[unpinned].clone());
            unpinned += 1;
        }
    }
    result
}

/// [`score`] for a zone given by name.
pub fn score_one(task: Candidate, now_ms: i64, zone: &str) -> Scored {
    score(task, now_ms, periodic::zone(zone))
}

/// Why a ranked task is where it is. `NextUpSelector.score`.
fn score(task: Candidate, now_ms: i64, zone: Tz) -> Scored {
    let key = primary(&task, now_ms, zone);
    let mut explanation = None;
    let reason = match key[0] as i64 {
        0 => {
            let deadline = effective_deadline(&task, zone).unwrap_or(now_ms);
            let days = ((now_ms - deadline) / 1000 / 86_400).max(0);
            explanation = Some(if days > 0 {
                format!("overdue by {days} days")
            } else {
                "past its deadline".to_string()
            });
            "overdue"
        }
        1 => {
            let age = ((now_ms - task.created_at_ms).max(0) / 1000) / 86_400;
            if age >= 30 {
                explanation = Some(format!("due today; added {age} days ago"));
            }
            "dueToday"
        }
        2 => "deadlineRisk",
        _ => {
            if !task.requirement_groups.is_empty() {
                "condition"
            } else if task.start_at_ms.is_some() {
                "started"
            } else if task.is_daily_due_today {
                "daily"
            } else if task.kanban_column.as_deref() == Some(TODAY_COLUMN) {
                "today"
            } else if task.matrix_importance.unwrap_or(0) > 0
                || task.matrix_urgency.unwrap_or(0) > 0
            {
                "importance"
            } else if task.priority.is_some_and(|p| (1..=4).contains(&p)) {
                "priority"
            } else if effective_deadline(&task, zone)
                .is_some_and(|due| (due - now_ms) as f64 / 1000.0 <= DUE_HORIZON_DAYS * 86_400.0)
            {
                "dueSoon"
            } else {
                "order"
            }
        }
    };
    Scored {
        score: (4.0 - key[0]) * 1000.0,
        reason: reason.to_string(),
        explanation,
        candidate: task,
    }
}

#[cfg(test)]
mod tests;
