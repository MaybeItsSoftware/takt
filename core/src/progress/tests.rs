//! Ported from `corelogic-tests/` (`FocusDayTimelineTests`,
//! `WorkProgressSummaryTests`, `TaskProgressSeriesTests`,
//! `CompletedWorkDigestTests`, `StaleFocusPolicyTests`,
//! `CompletionMilestonePolicyTests`) and `WorkspaceFocusPointsTests`.

use chrono::{NaiveDate, TimeZone};
use rusqlite::Connection;

use super::*;
use crate::schema::migrate;

const MINUTE: i64 = 60_000;
const DAY: i64 = 24 * HOUR_MS;

fn utc(y: i32, m: u32, d: u32, h: u32, min: u32) -> i64 {
    Utc.with_ymd_and_hms(y, m, d, h, min, 0)
        .unwrap()
        .timestamp_millis()
}

fn london(y: i32, m: u32, d: u32, h: u32) -> i64 {
    let tz: Tz = "Europe/London".parse().unwrap();
    periodic::resolve(
        tz,
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, 0, 0)
            .unwrap(),
    )
    .timestamp_millis()
}

// -- work progress (Wednesday 2025-09-24, 15:00 UTC; weeks start Monday) ------

const WEDNESDAY: i64 = 1_758_726_000_000;

fn day(offset: i64, hour: i64) -> i64 {
    start_of_day(WEDNESDAY, Tz::UTC) + offset * DAY + hour * HOUR_MS
}

fn block(seconds: i64, at: i64) -> WorkBlockSeconds {
    WorkBlockSeconds {
        seconds,
        recorded_at_ms: at,
    }
}

fn progress(completions: Vec<i64>, blocks: Vec<WorkBlockSeconds>) -> WorkProgressTotals {
    summarise_work_progress(completions, blocks, WEDNESDAY, "UTC".into(), 2)
}

#[test]
fn work_progress_separates_today_from_the_rest_of_the_week() {
    let totals = progress(
        vec![day(0, 10), day(0, 14), day(-1, 10), day(-2, 10)],
        vec![
            block(1_800, day(0, 10)),
            block(900, day(0, 13)),
            block(3_600, day(-1, 10)),
        ],
    );
    assert_eq!((totals.today_completed, totals.today_seconds), (2, 2_700));
    assert_eq!((totals.week_completed, totals.week_seconds), (4, 6_300));
}

#[test]
fn the_week_stops_at_the_start_of_the_users_week() {
    let totals = progress(vec![day(-3, 10)], vec![block(7_200, day(-3, 10))]);
    assert_eq!((totals.week_completed, totals.week_seconds), (0, 0));
    assert_eq!(totals.elapsed_days, 3);
    // A Sunday-start week holds that Sunday.
    let sunday = summarise_work_progress(vec![day(-3, 10)], vec![], WEDNESDAY, "UTC".into(), 1);
    assert_eq!((sunday.week_completed, sunday.elapsed_days), (1, 4));
    assert_eq!(
        start_of_week_ms(WEDNESDAY, "UTC".into(), 2),
        utc(2025, 9, 22, 0, 0)
    );
}

#[test]
fn tomorrows_work_is_not_counted_today() {
    let totals = progress(vec![day(1, 10)], vec![block(600, day(1, 10))]);
    assert_eq!(totals.today_completed + totals.today_seconds, 0);
    assert_eq!(totals.week_completed + totals.week_seconds, 0);
}

#[test]
fn negative_block_seconds_cannot_subtract_from_the_day() {
    let totals = progress(
        vec![],
        vec![block(600, day(0, 10)), block(-600, day(0, 10))],
    );
    assert_eq!(totals.today_seconds, 600);
}

#[test]
fn work_progress_reads_the_rows_itself() {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    let t = "2025-01-01 00:00:00.000";
    let stamp = |ms: i64| crate::time::stored(ms);
    connection
        .execute_batch(&format!(
            "INSERT INTO workspaces (id, name, createdAt, updatedAt) VALUES ('w', 'Mine', '{t}', '{t}');
             INSERT INTO task_lists (id, workspaceId, name, sortOrder, createdAt, updatedAt)
               VALUES ('l', 'w', 'Inbox', 0, '{t}', '{t}');
             INSERT INTO tasks (id, listId, title, sortOrder, status, completedAt, createdAt, updatedAt) VALUES
               ('a', 'l', 'A', 0, 'completed', '{today}', '{t}', '{t}'),
               ('b', 'l', 'B', 1, 'completed', '{monday}', '{t}', '{t}'),
               ('c', 'l', 'C', 2, 'completed', '{sunday}', '{t}', '{t}');
             INSERT INTO focus_work_blocks (id, taskTitle, seconds, recordedAt)
               VALUES ('x', 'A', 1200, '{today}'), ('y', 'B', 600, '{monday}');",
            today = stamp(day(0, 9)),
            monday = stamp(day(-2, 9)),
            sunday = stamp(day(-3, 9)),
        ))
        .unwrap();
    let totals = work_progress(&connection, WEDNESDAY, "UTC", 2).unwrap();
    assert_eq!(
        totals,
        WorkProgressTotals {
            today_completed: 1,
            today_seconds: 1_200,
            week_completed: 2,
            week_seconds: 1_800,
            elapsed_days: 3,
        }
    );
}

// -- task progress (Europe/London, March 2026) -------------------------------

fn days(completions: Vec<i64>, creations: Vec<i64>, now: i64) -> Vec<TaskProgressCount> {
    task_progress_days(7, completions, creations, now, "Europe/London".into())
}

#[test]
fn every_day_of_the_period_is_present_empty_ones_included() {
    let series = days(vec![], vec![], london(2026, 3, 20, 12));
    assert_eq!(series.len(), 7);
    assert_eq!(series[0].day_start_ms, london(2026, 3, 14, 0));
    assert_eq!(series[6].day_start_ms, london(2026, 3, 20, 0));
    assert!(series.iter().all(|d| d.completed == 0 && d.added == 0));
}

#[test]
fn counts_completions_and_creations_per_day_with_a_running_total() {
    let series = days(
        vec![
            london(2026, 3, 15, 9),
            london(2026, 3, 15, 23),
            london(2026, 3, 18, 0),
        ],
        vec![
            london(2026, 3, 15, 12),
            london(2026, 3, 19, 12),
            london(2026, 3, 19, 12),
            london(2026, 3, 20, 12),
        ],
        london(2026, 3, 20, 8),
    );
    let pick = |f: fn(&TaskProgressCount) -> i64| series.iter().map(f).collect::<Vec<_>>();
    assert_eq!(pick(|d| d.completed), [0, 2, 0, 0, 1, 0, 0]);
    assert_eq!(pick(|d| d.added), [0, 1, 0, 0, 0, 2, 1]);
    assert_eq!(pick(|d| d.cumulative_completed), [0, 2, 2, 2, 3, 3, 3]);
}

#[test]
fn times_outside_the_period_are_ignored_not_clamped() {
    let series = days(
        vec![london(2026, 3, 13, 23), london(2026, 3, 21, 0)],
        vec![london(2026, 3, 1, 12)],
        london(2026, 3, 20, 12),
    );
    assert!(series.iter().all(|d| d.completed == 0 && d.added == 0));
}

#[test]
fn the_interval_runs_from_the_first_days_start_to_the_end_of_today() {
    let span = task_progress_interval(30, london(2026, 3, 20, 15), "Europe/London".into());
    assert_eq!(span.start_ms, london(2026, 2, 19, 0));
    assert_eq!(span.end_ms, london(2026, 3, 21, 0));
}

#[test]
fn a_daylight_saving_change_still_gives_one_bucket_per_day() {
    let series = days(vec![london(2026, 3, 29, 3)], vec![], london(2026, 4, 1, 12));
    assert_eq!(series.len(), 7);
    let mut starts: Vec<i64> = series.iter().map(|d| d.day_start_ms).collect();
    starts.dedup();
    assert_eq!(starts.len(), 7);
    assert_eq!(series.iter().map(|d| d.completed).sum::<i64>(), 1);
}

// -- finished work by day ----------------------------------------------------

#[test]
fn days_and_their_items_come_back_newest_first() {
    let groups = group_completed_work(
        vec![day(-3, 10), day(0, 9), day(-1, 10), day(0, 21), day(0, 13)],
        WEDNESDAY,
        "UTC".into(),
    );
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].items, [3, 4, 1]);
    assert_eq!(groups[1].items, [2]);
    assert_eq!(groups[2].items, [0]);
    assert_eq!(groups[0].day_start_ms, day(0, 0));
}

#[test]
fn late_last_night_is_yesterday_not_today() {
    let groups = group_completed_work(vec![day(-1, 23), day(0, 1)], WEDNESDAY, "UTC".into());
    let kinds: Vec<CompletedDayKind> = groups.iter().map(|g| g.kind).collect();
    assert_eq!(
        kinds,
        [CompletedDayKind::Today, CompletedDayKind::Yesterday]
    );
    assert_eq!(groups[0].items, [1]);
}

#[test]
fn the_week_ends_at_six_days_so_a_weekday_name_stays_unambiguous() {
    let kind = |offset| completed_day_kind(day(offset, 10), WEDNESDAY, "UTC".into());
    assert_eq!(kind(0), CompletedDayKind::Today);
    assert_eq!(kind(-1), CompletedDayKind::Yesterday);
    assert_eq!(kind(-2), CompletedDayKind::ThisWeek);
    assert_eq!(kind(-6), CompletedDayKind::ThisWeek);
    assert_eq!(kind(-7), CompletedDayKind::Earlier);
    // A clock run ahead, or a sync from further east, is still today.
    assert_eq!(kind(1), CompletedDayKind::Today);
}

#[test]
fn nothing_finished_is_no_days() {
    assert!(group_completed_work(vec![], WEDNESDAY, "UTC".into()).is_empty());
}

// -- the day's focus timeline (14 November 2023, UTC) --------------------------

const TIMELINE_DAY: i64 = 1_700_000_000_000;

fn at(hour: u32, minute: u32) -> i64 {
    utc(2023, 11, 14, hour, minute)
}

fn timeline_block(id: &str, ended_at: i64, minutes: i64) -> TimelineBlock {
    TimelineBlock {
        id: id.into(),
        seconds: minutes * 60,
        ended_at_ms: ended_at,
    }
}

fn layout(blocks: Vec<TimelineBlock>) -> TimelineLayout {
    focus_day_layout(blocks, TIMELINE_DAY, "UTC".into())
}

fn hour_count(layout: &TimelineLayout) -> i64 {
    ((layout.end_ms - layout.start_ms) as f64 / HOUR_MS as f64)
        .round()
        .max(1.0) as i64
}

#[test]
fn a_block_is_placed_where_it_ran_rather_than_where_it_was_logged() {
    let layout = layout(vec![timeline_block("a", at(10, 30), 30)]);
    assert_eq!(layout.placements[0].minutes, 30.0);
    assert_eq!(layout.start_ms, at(10, 0));
}

#[test]
fn the_window_covers_every_block_on_whole_hours() {
    let layout = layout(vec![
        timeline_block("morning", at(9, 20), 35),
        timeline_block("evening", at(17, 10), 40),
    ]);
    assert_eq!((layout.start_ms, layout.end_ms), (at(8, 0), at(18, 0)));
    assert_eq!(hour_count(&layout), 10);
    assert_eq!(layout.placements[0].offset_minutes, 45.0);
}

#[test]
fn a_short_day_still_gets_a_readable_ruler() {
    let layout = layout(vec![timeline_block("a", at(14, 20), 20)]);
    assert_eq!(hour_count(&layout), TIMELINE_MINIMUM_HOURS);
    assert_eq!(layout.start_ms, at(14, 0));
}

#[test]
fn a_minimum_window_never_overhangs_the_end_of_the_day() {
    let layout = layout(vec![timeline_block("late", at(23, 50), 20)]);
    assert_eq!(layout.end_ms, utc(2023, 11, 15, 0, 0));
    assert_eq!(hour_count(&layout), TIMELINE_MINIMUM_HOURS);
}

#[test]
fn an_empty_day_keeps_its_shape() {
    let layout = layout(vec![]);
    assert!(layout.placements.is_empty());
    assert_eq!(layout.lane_count, 1);
    assert_eq!((layout.start_ms, layout.end_ms), (at(9, 0), at(18, 0)));
}

#[test]
fn overlapping_blocks_take_separate_lanes() {
    let layout = layout(vec![
        timeline_block("a", at(11, 0), 60),
        timeline_block("b", at(11, 30), 60),
        timeline_block("c", at(13, 0), 30),
    ]);
    assert_eq!(layout.lane_count, 2);
    let lanes: Vec<u32> = layout.placements.iter().map(|p| p.lane).collect();
    assert_eq!(lanes, [0, 1, 0]);
}

#[test]
fn work_running_through_midnight_is_clamped_to_the_day_it_lands_on() {
    let layout = layout(vec![timeline_block("overnight", at(0, 30), 90)]);
    assert_eq!(layout.placements[0].minutes, 30.0);
    assert_eq!(layout.start_ms, at(0, 0));
}

#[test]
fn blocks_with_no_time_in_them_are_not_drawn() {
    assert!(
        layout(vec![timeline_block("empty", at(12, 0), 0)])
            .placements
            .is_empty()
    );
}

#[test]
fn placements_are_in_clock_order_and_index_the_input() {
    let layout = layout(vec![
        timeline_block("running", at(11, 15), 15),
        timeline_block("logged", at(10, 0), 25),
    ]);
    let order: Vec<u32> = layout.placements.iter().map(|p| p.block).collect();
    assert_eq!(order, [1, 0]);
    assert_eq!(layout.placements[1].offset_minutes, 120.0);
    assert_eq!(
        layout.placements[1].minutes * MINUTE as f64,
        15.0 * MINUTE as f64
    );
}

fn entry(key: &str, minutes: i64) -> FocusTimelineEntry {
    FocusTimelineEntry {
        key: key.into(),
        seconds: minutes * 60,
    }
}

#[test]
fn summaries_gather_a_tasks_blocks_and_put_the_most_time_first() {
    let summaries = focus_day_summaries(vec![
        entry("task-a", 20),
        entry("task-b", 30),
        entry("task-a", 25),
        entry("loose", 5),
    ]);
    let keys: Vec<&str> = summaries.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(keys, ["task-a", "task-b", "loose"]);
    assert_eq!(summaries[0].seconds, 45 * 60);
    assert_eq!(summaries[0].blocks, 2);
    // Its latest block names it.
    assert_eq!(summaries[0].latest, 2);
}

#[test]
fn summaries_with_equal_time_keep_a_stable_order() {
    let summaries = focus_day_summaries(vec![entry("y", 10), entry("x", 10)]);
    let keys: Vec<&str> = summaries.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(keys, ["x", "y"]);
}

// -- a block left paused (UTC, the day rolling over at 04:00) ----------------

fn stale(paused: Option<i64>, seconds: i64, has_task: bool, now: i64) -> StaleFocusOutcome {
    stale_focus_outcome(paused, seconds, has_task, now, 4, "UTC".into())
}

fn sept(day: u32, hour: u32) -> i64 {
    utc(2026, 9, day, hour, 0)
}

#[test]
fn stale_focus_follows_the_logical_day() {
    use StaleFocusOutcome::*;
    // Running, not paused.
    assert_eq!(stale(None, 900, true, sept(25, 15)), Keep);
    // Paused earlier today.
    assert_eq!(stale(Some(sept(25, 9)), 900, true, sept(25, 15)), Keep);
    // Nothing on it: gone the same day.
    assert_eq!(stale(Some(sept(25, 9)), 210, false, sept(25, 11)), Discard);
    // After midnight is still the day that started at four.
    assert_eq!(stale(Some(sept(25, 23)), 900, true, sept(26, 1)), Keep);
    assert_eq!(stale(Some(sept(24, 15)), 900, true, sept(25, 15)), Close);
    assert_eq!(stale(Some(sept(24, 15)), 0, true, sept(25, 15)), Discard);
    assert_eq!(stale(Some(sept(24, 15)), 59, true, sept(25, 15)), Discard);
    assert_eq!(stale(Some(sept(24, 15)), 60, true, sept(25, 15)), Close);
    assert_eq!(stale(Some(sept(24, 15)), 900, false, sept(25, 15)), Discard);
    assert_eq!(stale(Some(sept(18, 11)), 1_500, true, sept(25, 15)), Close);
}

fn paused_workspace(paused_at: i64, accumulated: i64) -> (Connection, String) {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON")
        .unwrap();
    migrate(&mut connection).unwrap();
    let t = "2026-01-01 00:00:00.000";
    connection
        .execute_batch(&format!(
            "INSERT INTO workspaces (id, name, createdAt, updatedAt) VALUES ('w', 'Mine', '{t}', '{t}');
             INSERT INTO task_lists (id, workspaceId, name, sortOrder, createdAt, updatedAt)
               VALUES ('l', 'w', 'Inbox', 0, '{t}', '{t}');
             INSERT INTO tasks (id, listId, title, sortOrder, createdAt, updatedAt)
               VALUES ('write', 'l', 'Write', 0, '{t}', '{t}');"
        ))
        .unwrap();
    let tx = connection.transaction().unwrap();
    let id = focus::start_session(
        &tx,
        "write",
        None,
        1_500,
        300,
        None,
        false,
        false,
        paused_at - accumulated * 1000,
        "UTC",
    )
    .unwrap();
    focus::pause(&tx, &id, paused_at).unwrap();
    tx.commit().unwrap();
    (connection, id)
}

fn phase_and_logged(connection: &Connection, id: &str) -> (String, i64) {
    let phase = connection
        .query_row(
            "SELECT phase FROM focus_sessions WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    let logged = connection
        .query_row(
            "SELECT COALESCE(SUM(seconds), 0) FROM focus_work_blocks",
            [],
            |row| row.get(0),
        )
        .unwrap();
    (phase, logged)
}

#[test]
fn yesterdays_paused_block_is_credited_and_closed() {
    let (mut connection, id) = paused_workspace(sept(24, 15), 900);
    let outcome = resolve_stale_session(
        &mut connection,
        sept(25, 15),
        "UTC",
        4,
        &FocusContext::default(),
    )
    .unwrap();
    assert_eq!(outcome, StaleFocusOutcome::Close);
    assert_eq!(phase_and_logged(&connection, &id), ("finished".into(), 900));
    // Credited to the day it was worked on, and the task left open.
    let recorded: String = connection
        .query_row("SELECT recordedAt FROM focus_work_blocks", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(recorded, crate::time::stored(sept(24, 15)));
    let status: String = connection
        .query_row("SELECT status FROM tasks WHERE id = 'write'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(status, "open");
}

#[test]
fn todays_paused_block_is_kept_and_a_moment_of_yesterday_discarded() {
    let (mut connection, id) = paused_workspace(sept(25, 9), 900);
    let context = FocusContext::default();
    let kept = resolve_stale_session(&mut connection, sept(25, 15), "UTC", 4, &context).unwrap();
    assert_eq!(kept, StaleFocusOutcome::Keep);
    assert_eq!(
        phase_and_logged(&connection, &id),
        ("running".into(), 0),
        "paused, not finished"
    );

    let (mut connection, id) = paused_workspace(sept(24, 9), 20);
    let gone = resolve_stale_session(&mut connection, sept(25, 15), "UTC", 4, &context).unwrap();
    assert_eq!(gone, StaleFocusOutcome::Discard);
    assert_eq!(phase_and_logged(&connection, &id), ("finished".into(), 0));
}

// -- points --------------------------------------------------------------------

#[test]
fn a_score_is_minutes_times_the_multiplier_to_one_decimal_place() {
    assert_eq!(focus_minutes(1_500), 25.0);
    assert_eq!(focus_minutes(750), 12.5);
    assert_eq!(focus_score(750, 1.0), 12.5);
    assert_eq!(focus_score(1_500, 1.5), 37.5);
    assert_eq!(focus_score(1_500, 0.5), 12.5);
    assert_eq!(focus_score(0, 2.0), 0.0);
    assert_eq!(focus_minutes(-30), 0.0);
}

#[test]
fn an_out_of_range_or_nonsense_multiplier_cannot_distort_the_totals() {
    assert_eq!(clamped_focus_multiplier(900.0), 5.0);
    assert_eq!(clamped_focus_multiplier(-3.0), 0.0);
    assert_eq!(clamped_focus_multiplier(f64::NAN), 1.0);
    assert_eq!(clamped_focus_multiplier(f64::INFINITY), 1.0);
    assert_eq!(focus_score(600, f64::INFINITY), 10.0);
}

// -- completion milestones ---------------------------------------------------

#[test]
fn milestones_take_their_precedence() {
    use MilestoneOccasion::*;
    let task = |remaining, ordinal, streak| completion_milestone(false, remaining, ordinal, streak);
    let daily = |ordinal, streak| completion_milestone(true, 0, ordinal, streak);
    assert_eq!(task(5, 3, 0), Ordinary);
    assert_eq!(task(1, 3, 0), ListCleared);
    assert_eq!(task(0, 3, 0), ListCleared);
    assert_eq!(task(5, 10, 0), DailyTally { count: 10 });
    assert_eq!(task(5, 20, 0), DailyTally { count: 20 });
    assert_eq!(task(5, 1, 0), Ordinary);
    assert_eq!(task(1, 10, 0), ListCleared);
    // A daily never clears the list, and its tick outranks a tally.
    assert_eq!(daily(10, 0), DailyTicked);
    assert_eq!(daily(1, 9), DailyStreak { days: 9 });
    assert_eq!(task(5, 1, 4), DailyStreak { days: 4 });
    assert_eq!(task(5, 2, 4), Ordinary);
    assert_eq!(task(5, 1, 2), Ordinary);
    assert_eq!(task(1, 1, 4), ListCleared);
}
