use rusqlite::{Connection, params};

use super::data::{grdb_timestamp, normalized_visible_root_name};
use super::*;

const FIXTURE: &str = include_str!("../../../cli/src/fixtures/workspace_schema.sql");

fn migrated() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    connection
}

fn migrated_through(last: &str) -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    migrate_through(&mut connection, Some(last)).unwrap();
    connection
}

fn ledger(connection: &Connection) -> Vec<String> {
    let mut statement = connection
        .prepare("SELECT identifier FROM grdb_migrations ORDER BY rowid")
        .unwrap();
    statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// The proof that the core and the app agree: a new database the core
/// migrates has, statement for statement, the schema the Mac's GRDB migrator
/// produced, as dumped into the fixture the CLI and Android are held to.
#[test]
fn a_new_database_has_exactly_the_fixtures_schema() {
    let dumped = dump_schema(&migrated()).unwrap();
    if dumped != FIXTURE {
        let first = dumped
            .lines()
            .zip(FIXTURE.lines())
            .position(|(a, b)| a != b)
            .unwrap_or(dumped.lines().count().min(FIXTURE.lines().count()));
        panic!(
            "schema differs from the fixture at line {}:\n core:    {:?}\n fixture: {:?}",
            first + 1,
            dumped.lines().nth(first),
            FIXTURE.lines().nth(first)
        );
    }
}

#[test]
fn every_migration_is_recorded_in_order_under_grdbs_identifiers() {
    let connection = migrated();
    assert_eq!(ledger(&connection), workspace_migrations());
    assert_eq!(workspace_migrations().len(), 35);
    assert_eq!(
        workspace_migrations().last().map(String::as_str),
        Some("v35_series_title")
    );
}

#[test]
fn migrating_again_applies_nothing_and_keeps_the_work() {
    let mut connection = migrated();
    connection
        .execute_batch(
            "INSERT INTO workspaces VALUES ('w', 'Mine', '2024-01-01 00:00:00.000', '2024-01-01 00:00:00.000');",
        )
        .unwrap();
    let before = dump_schema(&connection).unwrap();
    assert_eq!(migrate(&mut connection).unwrap(), "v35_series_title");
    assert_eq!(dump_schema(&connection).unwrap(), before);
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM workspaces", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn foreign_keys_are_restored_to_how_the_caller_had_them() {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON")
        .unwrap();
    migrate(&mut connection).unwrap();
    let on: bool = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .unwrap();
    assert!(on);
}

#[test]
fn a_file_database_is_created_and_closed_again() {
    let directory = std::env::temp_dir().join(format!("takt-core-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("workspace.sqlite");
    let latest = migrate_workspace(path.to_string_lossy().into_owned()).unwrap();
    assert_eq!(latest, "v35_series_title");
    let reopened = Connection::open(&path).unwrap();
    assert_eq!(ledger(&reopened).len(), 35);
    std::fs::remove_dir_all(directory).unwrap();
}

const T0: &str = "2023-11-14 22:13:20.000";
const T1: &str = "2023-11-15 09:00:00.000";

fn seed_workspace(connection: &Connection) {
    connection
        .execute(
            "INSERT INTO workspaces (id, name, createdAt, updatedAt) VALUES ('w', 'Mine', ?1, ?1)",
            [T0],
        )
        .unwrap();
}

fn seed_list(connection: &Connection, id: &str, name: &str, created_at: &str) {
    connection
        .execute(
            "INSERT INTO task_lists (id, workspaceId, name, sortOrder, createdAt, updatedAt)
             VALUES (?1, 'w', ?2, 0, ?3, ?3)",
            params![id, name, created_at],
        )
        .unwrap();
}

fn seed_task(
    connection: &Connection,
    id: &str,
    list: &str,
    parent: Option<&str>,
    title: &str,
    source: Option<&str>,
    created_at: &str,
) {
    connection
        .execute(
            "INSERT INTO tasks (id, listId, parentTaskId, title, sortOrder, createdAt, updatedAt, sourceSystem)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5, ?6)",
            params![id, list, parent, title, created_at, source],
        )
        .unwrap();
}

fn visible_root(connection: &Connection, list: &str) -> Option<String> {
    connection
        .query_row(
            "SELECT visibleRootTaskId FROM task_lists WHERE id = ?1",
            [list],
            |row| row.get(0),
        )
        .unwrap()
}

/// v11: an imported list whose single root is a wrapper named like the list
/// shows the wrapper's children; an unimported one, or one whose wrapper has
/// no children, does not.
#[test]
fn v11_recognises_an_imported_wrapper_and_nothing_else() {
    let mut connection = migrated_through("v10_focus_points");
    seed_workspace(&connection);
    seed_list(&connection, "imported", "Café Work", T0);
    seed_task(
        &connection,
        "root",
        "imported",
        None,
        "cafe — work",
        Some("checkvist"),
        T0,
    );
    seed_task(
        &connection,
        "child",
        "imported",
        Some("root"),
        "Proposal",
        Some("checkvist"),
        T0,
    );
    seed_list(&connection, "typed", "Home", T0);
    seed_task(&connection, "home", "typed", None, "Home", None, T1);
    seed_task(
        &connection,
        "chore",
        "typed",
        Some("home"),
        "Dishes",
        None,
        T1,
    );
    seed_list(&connection, "bare", "Bare", T0);
    seed_task(
        &connection,
        "bare-root",
        "bare",
        None,
        "Bare",
        Some("checkvist"),
        T0,
    );

    migrate_through(&mut connection, Some("v11_stable_visible_roots")).unwrap();

    assert_eq!(
        visible_root(&connection, "imported").as_deref(),
        Some("root")
    );
    assert_eq!(visible_root(&connection, "typed"), None);
    assert_eq!(visible_root(&connection, "bare"), None);
}

/// v11 also writes the new field into older undo snapshots of the list, so
/// undoing an old edit does not unhide the wrapper. That only works if the
/// row step runs before the snapshot update reads it.
#[test]
fn v11_carries_the_wrapper_into_older_undo_snapshots() {
    let mut connection = migrated_through("v10_focus_points");
    seed_workspace(&connection);
    seed_list(&connection, "imported", "Work", T0);
    seed_task(
        &connection,
        "root",
        "imported",
        None,
        "Work",
        Some("checkvist"),
        T0,
    );
    seed_task(
        &connection,
        "child",
        "imported",
        Some("root"),
        "Proposal",
        Some("checkvist"),
        T0,
    );
    connection
        .execute(
            "INSERT INTO change_log (groupId, label, tableName, rowId, operation, beforeJSON, afterJSON)
             VALUES ('g', 'Edit List', 'task_lists', 'imported', 'update', '{\"name\":\"Old\"}', '{\"name\":\"Work\"}')",
            [],
        )
        .unwrap();

    migrate_through(&mut connection, Some("v11_stable_visible_roots")).unwrap();

    let (before, after): (String, String) = connection
        .query_row(
            "SELECT json_extract(beforeJSON, '$.visibleRootTaskId'), json_extract(afterJSON, '$.visibleRootTaskId')
             FROM change_log WHERE rowId = 'imported'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((before.as_str(), after.as_str()), ("root", "root"));
}

/// v12: each existing workspace gets the four starting conditions.
#[test]
fn v12_gives_every_existing_workspace_its_starting_conditions() {
    let mut connection = migrated_through("v11_stable_visible_roots");
    seed_workspace(&connection);

    migrate_through(&mut connection, Some("v12_task_conditions_and_work")).unwrap();

    let mut statement = connection
        .prepare("SELECT name, isLocation, isArchived, length(id), id = upper(id) FROM task_conditions WHERE workspaceId = 'w' ORDER BY rowid")
        .unwrap();
    let rows: Vec<(String, bool, bool, i64, bool)> = statement
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let names: Vec<(&str, bool)> = rows.iter().map(|r| (r.0.as_str(), r.1)).collect();
    assert_eq!(
        names,
        [
            ("Home", true),
            ("Campus", true),
            ("Private", false),
            ("Floor space", false)
        ]
    );
    assert!(rows.iter().all(|r| !r.2 && r.3 == 36 && r.4));
}

/// v24: the places are filed under Location, quietly — no undo step, nothing
/// queued for sync — and the triggers name the new column.
#[test]
fn v24_files_every_place_under_location() {
    let mut connection = migrated_through("v23_filter_lists");
    seed_workspace(&connection);
    connection
        .execute_batch(&format!(
            "INSERT INTO task_conditions (id, workspaceId, name, isLocation, isArchived, createdAt, updatedAt)
               VALUES ('home', 'w', 'Home', 1, 0, '{T0}', '{T0}'), ('quiet', 'w', 'Quiet', 0, 0, '{T0}', '{T0}');
             UPDATE undo_control SET suppressed = 0, groupId = 'g', label = 'x' WHERE id = 0;
             UPDATE sync_control SET recording = 1 WHERE id = 0;
             DELETE FROM sync_outbox; DELETE FROM change_log;"
        ))
        .unwrap();

    migrate_through(&mut connection, Some("v24_condition_categories")).unwrap();

    let categories: Vec<(String, Option<String>)> = connection
        .prepare("SELECT id, category FROM task_conditions ORDER BY id")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        categories,
        [
            ("home".into(), Some("Location".into())),
            ("quiet".into(), None)
        ]
    );
    let count = |sql: &str| -> i64 { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(count("SELECT COUNT(*) FROM change_log"), 0);
    assert_eq!(count("SELECT COUNT(*) FROM sync_outbox"), 0);
    assert_eq!(count("SELECT suppressed FROM undo_control WHERE id = 0"), 0);
    assert_eq!(count("SELECT recording FROM sync_control WHERE id = 0"), 1);
    connection
        .execute(
            "UPDATE task_conditions SET category = 'Energy' WHERE id = 'quiet'",
            [],
        )
        .unwrap();
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM sync_outbox WHERE tableName = 'task_conditions' AND changedJSON LIKE '%category%'"
        ),
        1
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM change_log WHERE afterJSON LIKE '%Energy%'"),
        1
    );
}

/// v13: an early bulk import (no source system) is recognised only when the
/// wrapper and one of its children were created in the same batch as the
/// list.
#[test]
fn v13_recognises_a_same_batch_wrapper_from_before_source_identity() {
    let mut connection = migrated_through("v12_task_conditions_and_work");
    seed_workspace(&connection);
    seed_list(&connection, "batch", "Work", T0);
    seed_task(&connection, "root", "batch", None, "Work", None, T0);
    seed_task(
        &connection,
        "child",
        "batch",
        Some("root"),
        "Proposal",
        None,
        T0,
    );
    seed_list(&connection, "later", "Home", T0);
    seed_task(&connection, "home", "later", None, "Home", None, T1);
    seed_task(
        &connection,
        "chore",
        "later",
        Some("home"),
        "Dishes",
        None,
        T1,
    );

    migrate_through(&mut connection, Some("v13_legacy_visible_roots")).unwrap();

    assert_eq!(visible_root(&connection, "batch").as_deref(), Some("root"));
    assert_eq!(visible_root(&connection, "later"), None);
}

#[test]
fn a_list_name_folds_case_diacritics_and_punctuation_away() {
    assert_eq!(normalized_visible_root_name("Café — Work!"), "cafework");
    assert_eq!(normalized_visible_root_name("  ÉCOLE 2  "), "ecole2");
    assert_eq!(normalized_visible_root_name("— ."), "");
}

#[test]
fn a_timestamp_is_written_as_grdb_writes_one() {
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_millis(1_700_000_000_123);
    assert_eq!(grdb_timestamp(time), "2023-11-14 22:13:20.123");
    assert_eq!(
        grdb_timestamp(std::time::UNIX_EPOCH),
        "1970-01-01 00:00:00.000"
    );
}

/// The generator a new migration uses to reinstall the journal and outbox
/// triggers writes, on today's schema, exactly the triggers the captured
/// migrations left behind.
#[test]
fn triggers_match_the_fixture() {
    let connection = migrated();
    let stored = |name: &str| -> String {
        connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = ?1",
                [name],
                |row| row.get(0),
            )
            .unwrap()
    };
    let mut checked = 0;
    for statement in super::triggers::change_log_statements(&connection)
        .unwrap()
        .into_iter()
        .chain(super::triggers::sync_statements(&connection).unwrap())
        .filter(|s| s.starts_with("CREATE TRIGGER"))
    {
        let name = statement.split_whitespace().nth(2).unwrap().to_string();
        assert_eq!(statement, stored(&name), "trigger {name}");
        checked += 1;
    }
    assert_eq!(checked, 11 * 3 + 20 * 3);
    let before = dump_schema(&connection).unwrap();
    super::triggers::reinstall(&connection).unwrap();
    let after = dump_schema(&connection).unwrap();
    // Reinstalling moves each trigger to the end of sqlite_master, so compare
    // the set of statements rather than their order.
    let mut a: Vec<&str> = before.lines().collect();
    let mut b: Vec<&str> = after.lines().collect();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b);
}

/// v25: the standing instructions get a table of their own, keyed by scope,
/// whose writes are journalled and queued for sync like any other work.
#[test]
fn v25_adds_agent_instructions_journalled_and_synced() {
    let mut connection = migrated_through("v24_condition_categories");
    seed_workspace(&connection);
    migrate_through(&mut connection, Some("v25_agent_instructions")).unwrap();
    connection
        .execute_batch(&format!(
            "UPDATE undo_control SET suppressed = 0, groupId = 'g', label = 'x' WHERE id = 0;
             UPDATE sync_control SET recording = 1 WHERE id = 0;
             INSERT INTO agent_instructions (scope, text, updatedAt) VALUES ('account', 'Verbs.', '{T0}');"
        ))
        .unwrap();
    let count = |sql: &str| -> i64 { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM change_log WHERE tableName = 'agent_instructions' AND rowId = 'account'"
        ),
        1
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM sync_outbox WHERE tableName = 'agent_instructions' AND rowId = 'account'"
        ),
        1
    );
}

/// v26: whether a habit's missed appearances stack is a column on `dailies`,
/// so changing it is journalled and queued for sync like the rest of the row.
#[test]
fn v26_adds_stacks_missed_from_to_dailies_journalled_and_synced() {
    let mut connection = migrated_through("v25_agent_instructions");
    seed_workspace(&connection);
    seed_list(&connection, "l", "Inbox", T0);
    connection
        .execute_batch(&format!(
            "INSERT INTO tasks (id, listId, title, sortOrder, createdAt, updatedAt)
               VALUES ('t', 'l', 'Write', 0, '{T0}', '{T0}');
             INSERT INTO dailies (id, taskId, sortOrder, createdAt, updatedAt)
               VALUES ('d', 't', 0, '{T0}', '{T0}');"
        ))
        .unwrap();
    migrate_through(&mut connection, Some("v26_commitment_missed_stack")).unwrap();
    connection
        .execute_batch(&format!(
            "UPDATE undo_control SET suppressed = 0, groupId = 'g', label = 'x' WHERE id = 0;
             UPDATE sync_control SET recording = 1 WHERE id = 0;
             UPDATE dailies SET stacksMissedFrom = '{T1}' WHERE id = 'd';"
        ))
        .unwrap();
    let count = |sql: &str| -> i64 { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(
        count("SELECT COUNT(*) FROM change_log WHERE tableName = 'dailies' AND rowId = 'd'"),
        1
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM sync_outbox WHERE tableName = 'dailies' AND rowId = 'd' AND changedJSON LIKE '%stacksMissedFrom%'"
        ),
        1
    );
}

/// v27: the conditions a database already holds are numbered in the order
/// they were shown — Location, the other categories alphabetically, then
/// the uncategorised — without journalling or queueing the numbering, and a
/// later move is journalled and synced like the rest of the row. The context
/// table is not journalled (v28 syncs it).
#[test]
fn v27_numbers_conditions_quietly_and_does_not_journal_the_context() {
    let mut connection = migrated_through("v26_commitment_missed_stack");
    seed_workspace(&connection);
    connection
        .execute_batch(&format!(
            "INSERT INTO task_conditions (id, workspaceId, name, isLocation, isArchived, createdAt, updatedAt, category) VALUES
               ('quiet', 'w', 'Quiet', 0, 0, '{T0}', '{T0}', NULL),
               ('mat', 'w', 'Mat', 0, 0, '{T0}', '{T0}', 'Equipment'),
               ('home', 'w', 'Home', 1, 0, '{T1}', '{T1}', 'Location');
             UPDATE sync_control SET recording = 1 WHERE id = 0;"
        ))
        .unwrap();
    migrate(&mut connection).unwrap();
    let order: Vec<(String, i64)> = connection
        .prepare("SELECT id, sortOrder FROM task_conditions ORDER BY sortOrder")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        order,
        vec![("home".into(), 0), ("mat".into(), 1), ("quiet".into(), 2)]
    );
    let count = |connection: &Connection, sql: &str| -> i64 {
        connection.query_row(sql, [], |row| row.get(0)).unwrap()
    };
    assert_eq!(count(&connection, "SELECT COUNT(*) FROM change_log"), 0);
    assert_eq!(
        count(
            &connection,
            "SELECT COUNT(*) FROM sync_outbox WHERE tableName = 'task_conditions'"
        ),
        0
    );
    connection
        .execute_batch(&format!(
            "UPDATE undo_control SET suppressed = 0, groupId = 'g', label = 'x' WHERE id = 0;
             UPDATE task_conditions SET sortOrder = 9 WHERE id = 'quiet';
             INSERT INTO focus_contexts (workspaceId, updatedAt) VALUES ('w', '{T0}');"
        ))
        .unwrap();
    assert_eq!(
        count(
            &connection,
            "SELECT COUNT(*) FROM sync_outbox WHERE tableName = 'task_conditions' AND changedJSON LIKE '%sortOrder%'"
        ),
        1
    );
    assert_eq!(
        count(
            &connection,
            "SELECT COUNT(*) FROM change_log WHERE tableName = 'focus_contexts'"
        ),
        0
    );
}

/// v28: a paired device queues the context it already holds, stamped when
/// it was saved, and from then on a change to it is queued like any synced
/// row's — whole, whatever columns moved — while staying out of the journal.
/// An unpaired device queues nothing.
#[test]
fn v28_queues_the_context_a_paired_device_holds_and_syncs_it_whole() {
    let mut paired = migrated_through("v27_condition_order_and_context");
    seed_workspace(&paired);
    paired
        .execute_batch(&format!(
            "INSERT INTO focus_contexts (workspaceId, conditionIdsJSON, conditionsSetAt, updatedAt)
               VALUES ('w', '[\"campus\"]', '{T1}', '{T1}');
             UPDATE sync_control SET recording = 1 WHERE id = 0;"
        ))
        .unwrap();
    migrate(&mut paired).unwrap();
    let queued: (String, String, i64) = paired
        .query_row(
            "SELECT rowId, operation, changedAtMs FROM sync_outbox WHERE tableName = 'focus_contexts'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let saved = crate::time::parse_stored(T1).unwrap().timestamp_millis();
    assert_eq!(queued, ("w".to_string(), "insert".to_string(), saved));

    paired
        .execute_batch(
            "DELETE FROM sync_outbox;
             UPDATE undo_control SET suppressed = 0, groupId = 'g', label = 'x' WHERE id = 0;
             UPDATE focus_contexts SET mode = 'finish' WHERE workspaceId = 'w';",
        )
        .unwrap();
    let pending = crate::sync::pending_changes(&paired, 500).unwrap();
    let change = &pending.changes[0];
    assert_eq!(
        (change.table.as_str(), change.operation.as_str()),
        ("focus_contexts", "upsert")
    );
    for column in ["conditionIdsJSON", "conditionsSetAt", "mode", "updatedAt"] {
        assert!(change.values.contains_key(column), "{column} goes too");
    }
    let journalled: i64 = paired
        .query_row(
            "SELECT COUNT(*) FROM change_log WHERE tableName = 'focus_contexts'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(journalled, 0);

    let mut unpaired = migrated_through("v27_condition_order_and_context");
    seed_workspace(&unpaired);
    unpaired
        .execute_batch(&format!(
            "INSERT INTO focus_contexts (workspaceId, updatedAt) VALUES ('w', '{T1}');"
        ))
        .unwrap();
    migrate(&mut unpaired).unwrap();
    let queued: i64 = unpaired
        .query_row("SELECT COUNT(*) FROM sync_outbox", [], |row| row.get(0))
        .unwrap();
    assert_eq!(queued, 0);
}

/// v29: a skipped day is a column on `daily_contributions`, so skipping one
/// is journalled and queued for sync like the rest of the row.
#[test]
fn v29_adds_skipped_at_to_contributions_journalled_and_synced() {
    let mut connection = migrated_through("v28_synced_focus_context");
    seed_workspace(&connection);
    seed_list(&connection, "l", "Inbox", T0);
    connection
        .execute_batch(&format!(
            "INSERT INTO tasks (id, listId, title, sortOrder, createdAt, updatedAt)
               VALUES ('t', 'l', 'Write', 0, '{T0}', '{T0}');
             INSERT INTO dailies (id, taskId, sortOrder, createdAt, updatedAt)
               VALUES ('d', 't', 0, '{T0}', '{T0}');
             INSERT INTO daily_contributions (id, dailyId, taskId, dayKey, createdAt)
               VALUES ('c', 'd', 't', '2024-01-01', '{T0}');"
        ))
        .unwrap();
    migrate_through(&mut connection, Some("v29_daily_contribution_skips")).unwrap();
    connection
        .execute_batch(&format!(
            "UPDATE undo_control SET suppressed = 0, groupId = 'g', label = 'x' WHERE id = 0;
             UPDATE sync_control SET recording = 1 WHERE id = 0;
             UPDATE daily_contributions SET skippedAt = '{T1}' WHERE id = 'c';"
        ))
        .unwrap();
    let count = |sql: &str| -> i64 { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM change_log WHERE tableName = 'daily_contributions' AND rowId = 'c' AND afterJSON LIKE '%skippedAt%'"
        ),
        1
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM sync_outbox WHERE tableName = 'daily_contributions' AND rowId = 'c' AND changedJSON LIKE '%skippedAt%'"
        ),
        1
    );
}

/// v30: a task's calendar link is synced but not journalled, and the sync
/// tokens are neither.
#[test]
fn v30_syncs_calendar_links_and_keeps_sync_tokens_local() {
    let connection = migrated();
    connection
        .execute_batch(&format!(
            "UPDATE undo_control SET suppressed = 0, groupId = 'g', label = 'x' WHERE id = 0;
             UPDATE sync_control SET recording = 1 WHERE id = 0;
             INSERT INTO calendar_event_links (taskId, calendarId, eventId, createdAt, updatedAt)
               VALUES ('t', 'c', 'e', '{T0}', '{T0}');
             INSERT INTO calendar_sync_tokens (calendarId, syncToken, updatedAt)
               VALUES ('c', 'token', '{T0}');"
        ))
        .unwrap();
    let count = |sql: &str| -> i64 { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM sync_outbox WHERE tableName = 'calendar_event_links' AND rowId = 't'"
        ),
        1
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM sync_outbox WHERE tableName = 'calendar_sync_tokens'"),
        0
    );
    assert_eq!(count("SELECT COUNT(*) FROM change_log"), 0);
}

/// v31: what a link is beyond its address is a column on `task_metadata`, so
/// naming one is journalled and queued for sync like the rest of the row.
#[test]
fn v31_adds_link_details_to_metadata_journalled_and_synced() {
    let mut connection = migrated_through("v29_daily_contribution_skips");
    seed_workspace(&connection);
    seed_list(&connection, "l", "Inbox", T0);
    connection
        .execute_batch(&format!(
            "INSERT INTO tasks (id, listId, title, sortOrder, createdAt, updatedAt)
               VALUES ('t', 'l', 'Write', 0, '{T0}', '{T0}');
             INSERT INTO task_metadata (taskId, updatedAt) VALUES ('t', '{T0}');"
        ))
        .unwrap();
    migrate_through(&mut connection, Some("v31_link_details")).unwrap();
    connection
        .execute_batch(
            "UPDATE undo_control SET suppressed = 0, groupId = 'g', label = 'x' WHERE id = 0;
             UPDATE sync_control SET recording = 1 WHERE id = 0;
             UPDATE task_metadata SET linkDetailsJSON = '{}' WHERE taskId = 't';",
        )
        .unwrap();
    let count = |sql: &str| -> i64 { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM change_log WHERE tableName = 'task_metadata' AND rowId = 't' AND afterJSON LIKE '%linkDetailsJSON%'"
        ),
        1
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM sync_outbox WHERE tableName = 'task_metadata' AND rowId = 't' AND changedJSON LIKE '%linkDetailsJSON%'"
        ),
        1
    );
}

/// v32: a follow-up's repeat and the count of follow-ups made are columns on
/// `task_metadata`, journalled and synced like the rest of the row; every
/// existing row starts at nought made and no repeat.
#[test]
fn v32_adds_the_follow_up_repeat_and_count_journalled_and_synced() {
    let mut connection = migrated_through("v31_link_details");
    seed_workspace(&connection);
    seed_list(&connection, "l", "Inbox", T0);
    connection
        .execute_batch(&format!(
            "INSERT INTO tasks (id, listId, title, sortOrder, createdAt, updatedAt)
               VALUES ('t', 'l', 'Write', 0, '{T0}', '{T0}');
             INSERT INTO task_metadata (taskId, updatedAt) VALUES ('t', '{T0}');"
        ))
        .unwrap();
    migrate_through(&mut connection, Some("v32_follow_up_repeat")).unwrap();
    let count = |sql: &str| -> i64 { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM task_metadata WHERE waitingFollowUpCount = 0 AND waitingFollowUpEveryDays IS NULL"
        ),
        1
    );
    connection
        .execute_batch(
            "UPDATE undo_control SET suppressed = 0, groupId = 'g', label = 'x' WHERE id = 0;
             UPDATE sync_control SET recording = 1 WHERE id = 0;
             UPDATE task_metadata SET waitingFollowUpEveryDays = 3, waitingFollowUpCount = 2 WHERE taskId = 't';",
        )
        .unwrap();
    let count = |sql: &str| -> i64 { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM change_log WHERE tableName = 'task_metadata' AND rowId = 't'
               AND afterJSON LIKE '%waitingFollowUpEveryDays%' AND afterJSON LIKE '%waitingFollowUpCount%'"
        ),
        1
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM sync_outbox WHERE tableName = 'task_metadata' AND rowId = 't'
               AND changedJSON LIKE '%waitingFollowUpEveryDays%' AND changedJSON LIKE '%waitingFollowUpCount%'"
        ),
        1
    );
}

/// v33: the series an occurrence belongs to is a column on `task_metadata`,
/// journalled and synced like the rest of the row; every existing row is its
/// own series.
#[test]
fn v33_adds_the_recurrence_series_journalled_and_synced() {
    let mut connection = migrated_through("v32_follow_up_repeat");
    seed_workspace(&connection);
    seed_list(&connection, "l", "Inbox", T0);
    connection
        .execute_batch(&format!(
            "INSERT INTO tasks (id, listId, title, sortOrder, createdAt, updatedAt)
               VALUES ('t', 'l', 'Write', 0, '{T0}', '{T0}');
             INSERT INTO task_metadata (taskId, updatedAt) VALUES ('t', '{T0}');"
        ))
        .unwrap();
    migrate_through(&mut connection, Some("v33_recurrence_series")).unwrap();
    let count = |sql: &str| -> i64 { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(
        count("SELECT COUNT(*) FROM task_metadata WHERE seriesTaskId IS NULL"),
        1
    );
    connection
        .execute_batch(
            "UPDATE undo_control SET suppressed = 0, groupId = 'g', label = 'x' WHERE id = 0;
             UPDATE sync_control SET recording = 1 WHERE id = 0;
             UPDATE task_metadata SET seriesTaskId = 'first' WHERE taskId = 't';",
        )
        .unwrap();
    let count = |sql: &str| -> i64 { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM change_log WHERE tableName = 'task_metadata' AND rowId = 't'
               AND afterJSON LIKE '%seriesTaskId%'"
        ),
        1
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM sync_outbox WHERE tableName = 'task_metadata' AND rowId = 't'
               AND changedJSON LIKE '%seriesTaskId%'"
        ),
        1
    );
}

/// v34: what a condition provides is a column on `task_conditions`,
/// journalled and synced like the rest of the row, and the provided
/// conditions switched off for now a column on the synced `focus_contexts`.
/// Nothing existing provides anything or is switched off.
#[test]
fn v34_adds_provides_and_overrides_journalled_and_synced() {
    let mut connection = migrated_through("v33_recurrence_series");
    seed_workspace(&connection);
    connection
        .execute_batch(&format!(
            "INSERT INTO task_conditions (id, workspaceId, name, isLocation, isArchived, createdAt, updatedAt)
               VALUES ('home', 'w', 'Home', 1, 0, '{T0}', '{T0}');
             INSERT INTO focus_contexts (workspaceId, updatedAt) VALUES ('w', '{T0}');"
        ))
        .unwrap();
    migrate_through(&mut connection, Some("v34_condition_provides")).unwrap();
    let count = |sql: &str| -> i64 { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(
        count("SELECT COUNT(*) FROM task_conditions WHERE providesJSON IS NULL"),
        1
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM focus_contexts WHERE offIdsJSON = '[]'"),
        1
    );
    connection
        .execute_batch(
            "UPDATE undo_control SET suppressed = 0, groupId = 'g', label = 'x' WHERE id = 0;
             UPDATE sync_control SET recording = 1 WHERE id = 0;
             UPDATE task_conditions SET providesJSON = '[\"quiet\"]' WHERE id = 'home';
             UPDATE focus_contexts SET offIdsJSON = '[\"quiet\"]' WHERE workspaceId = 'w';",
        )
        .unwrap();
    let count = |sql: &str| -> i64 { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM change_log WHERE tableName = 'task_conditions' AND rowId = 'home'
               AND afterJSON LIKE '%providesJSON%'"
        ),
        1
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM sync_outbox WHERE tableName = 'task_conditions' AND rowId = 'home'
               AND changedJSON LIKE '%providesJSON%'"
        ),
        1
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM sync_outbox WHERE tableName = 'focus_contexts' AND rowId = 'w'
               AND changedJSON LIKE '%offIdsJSON%'"
        ),
        1
    );
}

/// v35: the title a repeating task's next turn takes is a column on
/// `task_metadata`, journalled and synced like the rest of the row. Nothing
/// existing carries one.
#[test]
fn v35_adds_the_series_title_journalled_and_synced() {
    let mut connection = migrated_through("v34_condition_provides");
    seed_workspace(&connection);
    seed_list(&connection, "l", "Inbox", T0);
    connection
        .execute_batch(&format!(
            "INSERT INTO tasks (id, listId, title, sortOrder, createdAt, updatedAt)
               VALUES ('t', 'l', 'Stretch', 0, '{T0}', '{T0}');
             INSERT INTO task_metadata (taskId, updatedAt) VALUES ('t', '{T0}');"
        ))
        .unwrap();
    migrate_through(&mut connection, Some("v35_series_title")).unwrap();
    let count = |sql: &str| -> i64 { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(
        count("SELECT COUNT(*) FROM task_metadata WHERE seriesTitle IS NULL"),
        1
    );
    connection
        .execute_batch(
            "UPDATE undo_control SET suppressed = 0, groupId = 'g', label = 'x' WHERE id = 0;
             UPDATE sync_control SET recording = 1 WHERE id = 0;
             UPDATE task_metadata SET seriesTitle = 'Stretch' WHERE taskId = 't';",
        )
        .unwrap();
    let count = |sql: &str| -> i64 { connection.query_row(sql, [], |row| row.get(0)).unwrap() };
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM change_log WHERE tableName = 'task_metadata' AND rowId = 't'
               AND afterJSON LIKE '%seriesTitle%'"
        ),
        1
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM sync_outbox WHERE tableName = 'task_metadata' AND rowId = 't'
               AND changedJSON LIKE '%seriesTitle%'"
        ),
        1
    );
}
