//! The day and the focus ladder in one read, so only what is shown crosses.
//!
//! The Mac used to read every candidate (`focus::candidates`) into Swift, plan
//! the day there, and hand every candidate back across for the ranking, which
//! returned them all again. At a few thousand open tasks that was three
//! crossings of every candidate per write, where the work itself was a few
//! milliseconds. Here the candidates are read, the day planned and the ladder
//! ranked without leaving the core, and the ladder is cut to what a client
//! draws.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use rusqlite::{Connection, OptionalExtension};

use crate::CoreError;
use crate::focus::{self, Candidate, FocusContext};
use crate::periodic;
use crate::ranking::{self, Blocked, Scored};

/// One task in the day and what put it there. `DayPlanEntry`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DayEntry {
    pub id: String,
    /// `DayPlanReason`'s raw value: running, planned, overdue, dueToday or
    /// startsToday.
    pub reason: String,
}

/// The day and the ladder. `WorkspaceNextUpSnapshot`'s ranking half.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NextUp {
    pub day_plan: Vec<DayEntry>,
    /// The ladder, best first. With a limit, its head and every task in the
    /// day, in ladder order; without one, all of it.
    pub ranked: Vec<Scored>,
    /// How long the whole ladder is, whatever was returned.
    pub ranked_count: u64,
    pub blocked: Vec<Blocked>,
    pub next_evaluation_at_ms: Option<i64>,
}

const TODAY_COLUMN: &str = "today";

/// Reads the candidates, plans the day from all of them and ranks them.
/// `ladder_limit` keeps the ladder's first so many entries, plus any task in
/// the day further down, which a client looks up by id.
pub fn next_up(
    connection: &Connection,
    now_ms: i64,
    zone: &str,
    context: &FocusContext,
    running_id: Option<&str>,
    ladder_limit: Option<u32>,
) -> Result<NextUp, CoreError> {
    let candidates = focus::candidates(connection, now_ms, zone)?;
    // The day is gathered from every candidate rather than from the ranked
    // ones: a task due today that a condition rules out is still today's.
    let rolled = roll_up_cards(connection, plan(&candidates, running_id, now_ms, zone))?;
    // A card is never a candidate (it is a parent of open work), so the
    // priorities of what the day now holds are read from the store.
    let priorities = priorities_of(connection, &rolled)?;
    let day_plan = behind_the_back_last(rolled, |id| priorities.get(id).copied(), running_id);
    let ranking = ranking::evaluate(&candidates, now_ms, zone, context);
    let ranked_count = ranking.ranked.len() as u64;
    let ranked = match ladder_limit {
        None => ranking.ranked,
        Some(limit) => {
            let in_day: HashSet<&str> = day_plan.iter().map(|e| e.id.as_str()).collect();
            ranking
                .ranked
                .into_iter()
                .enumerate()
                .filter(|(index, scored)| {
                    *index < limit as usize || in_day.contains(scored.candidate.id.as_str())
                })
                .map(|(_, scored)| scored)
                .collect()
        }
    };
    Ok(NextUp {
        day_plan,
        ranked,
        ranked_count,
        blocked: ranking.blocked,
        next_evaluation_at_ms: ranking.next_evaluation_at_ms,
    })
}

/// Which tasks make up today, and why. `DayPlanSelector.plan`: the running
/// task, then the Today column in its own order, then the overdue and the due
/// today by deadline, then what starts today by start time. Each task once.
/// A task sent behind the back (priority -1 to -4) goes after all of that,
/// -4 first and -1 last, whatever brought it into the day.
pub fn plan<'a>(
    candidates: &'a [Candidate],
    running_id: Option<&str>,
    now_ms: i64,
    zone: &str,
) -> Vec<DayEntry> {
    let tz = periodic::zone(zone);
    let today = local(now_ms, tz).date_naive();
    let end_of_today = periodic::resolve(
        tz,
        today.and_hms_opt(0, 0, 0).expect("midnight exists") + Duration::days(1),
    )
    .timestamp_millis();

    let mut entries: Vec<DayEntry> = Vec::new();
    let mut claimed: HashSet<&str> = HashSet::new();
    let mut claim = |candidate: &'a Candidate, reason: &str| {
        if claimed.insert(candidate.id.as_str()) {
            entries.push(DayEntry {
                id: candidate.id.clone(),
                reason: reason.to_string(),
            });
        }
    };

    if let Some(running) = running_id.and_then(|id| candidates.iter().find(|c| c.id == id)) {
        claim(running, "running");
    }

    // The column's own arrangement is the one thing here a person chose: a
    // rank given by hand, then the list's order, then the id.
    let mut planned: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| c.kanban_column.as_deref() == Some(TODAY_COLUMN))
        .collect();
    planned.sort_by(|a, b| {
        let rank = match (a.focus_rank, b.focus_rank) {
            (Some(left), Some(right)) => left.cmp(&right),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        };
        rank.then(a.sort_order.cmp(&b.sort_order))
            .then_with(|| a.id.cmp(&b.id))
    });
    for candidate in planned {
        claim(candidate, "planned");
    }

    let mut dated: Vec<(&Candidate, i64)> = candidates
        .iter()
        .filter_map(|c| ranking::effective_deadline(c, tz).map(|at| (c, at)))
        .collect();
    dated.sort_by_key(|(_, at)| *at);
    for (candidate, _) in dated.iter().filter(|(_, at)| *at <= now_ms) {
        claim(candidate, "overdue");
    }
    for (candidate, _) in dated
        .iter()
        .filter(|(_, at)| *at > now_ms && *at <= end_of_today)
    {
        claim(candidate, "dueToday");
    }

    let mut starting: Vec<(&Candidate, i64)> = candidates
        .iter()
        .filter_map(|c| {
            // Started today: a start still to come later today keeps the
            // task off the day until it arrives, rather than drawing it
            // greyed out all morning.
            c.start_at_ms
                .filter(|at| *at <= now_ms && local(*at, tz).date_naive() == today)
                .map(|at| (c, at))
        })
        .collect();
    starting.sort_by_key(|(_, at)| *at);
    for (candidate, _) in starting {
        claim(candidate, "startsToday");
    }
    behind_the_back_last(
        entries,
        |id| {
            candidates
                .iter()
                .find(|c| c.id == id)
                .and_then(|c| c.priority)
        },
        running_id,
    )
}

/// Moves the tasks sent behind the back (priority -1 to -4) to the end of the
/// day, -4 first and -1 last. A stable sort, so each keeps its place among
/// those of its own priority. The running task stays first: it is what is
/// happening. Run after anything else that orders the day, or that order
/// puts them back.
fn behind_the_back_last(
    mut entries: Vec<DayEntry>,
    priority: impl Fn(&str) -> Option<i64>,
    running_id: Option<&str>,
) -> Vec<DayEntry> {
    let behind = |id: &str| -> Option<i64> {
        if Some(id) == running_id {
            return None;
        }
        priority(id).filter(|p| (-4..=-1).contains(p))
    };
    entries.sort_by_key(|e| behind(&e.id).map_or(0, |p| 10 + p));
    entries
}

/// The priority of each task in `entries` that has one, read from the store.
fn priorities_of(
    connection: &Connection,
    entries: &[DayEntry],
) -> Result<HashMap<String, i64>, CoreError> {
    let mut statement =
        connection.prepare("SELECT priority FROM task_metadata WHERE taskId = ?1")?;
    let mut priorities = HashMap::new();
    for entry in entries {
        let priority: Option<i64> = statement
            .query_row([&entry.id], |row| row.get(0))
            .optional()?
            .flatten();
        if let Some(priority) = priority {
            priorities.insert(entry.id.clone(), priority);
        }
    }
    Ok(priorities)
}

/// A task beneath a card planned for today is drawn under that card, as the
/// board draws it, so the day holds the card instead: the topmost open
/// ancestor filed in Today, once. A parent of open work is never a candidate,
/// so without this a card with subtasks showed only as its loose subtasks.
/// The running task keeps its own row.
///
/// The card stands where its own rank puts it among the planned tasks, when
/// it has one — the rank arranging the day gave it — and otherwise where its
/// first task stood. Taking its subtasks' place regardless meant a card
/// moved by hand went straight back: its subtasks, filed in Today with no
/// rank, still sorted after everything ranked.
fn roll_up_cards(
    connection: &Connection,
    entries: Vec<DayEntry>,
) -> Result<Vec<DayEntry>, CoreError> {
    struct Node {
        parent: Option<String>,
        open: bool,
        today: bool,
        rank: Option<i64>,
        sort_order: i64,
    }
    let mut tree: HashMap<String, Node> = HashMap::new();
    let mut statement = connection.prepare(
        "SELECT t.id, t.parentTaskId, t.status = 'open', COALESCE(m.kanbanColumn = ?1, 0),
                m.focusRank, t.sortOrder
         FROM tasks t LEFT JOIN task_metadata m ON m.taskId = t.id",
    )?;
    for row in statement.query_map([TODAY_COLUMN], |row| {
        Ok((
            row.get::<_, String>(0)?,
            Node {
                parent: row.get(1)?,
                open: row.get(2)?,
                today: row.get(3)?,
                rank: row.get(4)?,
                sort_order: row.get(5)?,
            },
        ))
    })? {
        let (id, node) = row?;
        tree.insert(id, node);
    }
    let card = |id: &str| -> Option<String> {
        let mut found = None;
        let mut current = tree.get(id).and_then(|node| node.parent.clone());
        // Bounded, so a corrupt cycle ends rather than loops.
        for _ in 0..tree.len() {
            let Some(ancestor) = current else { break };
            let Some(node) = tree.get(&ancestor) else {
                break;
            };
            if !node.open {
                break;
            }
            if node.today {
                found = Some(ancestor.clone());
            }
            current = node.parent.clone();
        }
        found
    };
    // `plan`'s order for the column: a rank, then the list's order, then the id.
    let key = |id: &str| {
        let node = tree.get(id);
        (
            node.and_then(|n| n.rank).is_none(),
            node.and_then(|n| n.rank),
            node.map_or(0, |n| n.sort_order),
            id.to_string(),
        )
    };
    let mut seen: HashSet<String> = HashSet::new();
    let mut running = Vec::new();
    let mut planned = Vec::new();
    let mut rest = Vec::new();
    for entry in entries {
        let rolled = (entry.reason != "running")
            .then(|| card(&entry.id))
            .flatten();
        let (entry, sort_key) = match rolled {
            Some(card) => {
                let ranked = tree.get(&card).and_then(|n| n.rank).is_some();
                let sort_key = if ranked {
                    Some(key(&card))
                } else {
                    (entry.reason == "planned").then(|| key(&entry.id))
                };
                (
                    DayEntry {
                        id: card,
                        reason: "planned".into(),
                    },
                    sort_key,
                )
            }
            None => {
                let sort_key = (entry.reason == "planned").then(|| key(&entry.id));
                (entry, sort_key)
            }
        };
        if !seen.insert(entry.id.clone()) {
            continue;
        }
        match sort_key {
            Some(sort_key) => planned.push((sort_key, entry)),
            None if entry.reason == "running" => running.push(entry),
            None => rest.push(entry),
        }
    }
    planned.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(running
        .into_iter()
        .chain(planned.into_iter().map(|(_, entry)| entry))
        .chain(rest)
        .collect())
}

fn local(ms: i64, zone: Tz) -> DateTime<Tz> {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .unwrap_or_default()
        .with_timezone(&zone)
}

#[cfg(test)]
mod tests;
