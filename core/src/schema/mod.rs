//! The workspace schema, as the ordered list of migrations that produced it.
//!
//! Step two of docs/rust-core-migration.md: this is now the one place the
//! schema is defined. Each migration's SQL is what GRDB's migrator ran in the
//! Swift app (captured by `workspace-tests/MigrationCaptureTests.swift`), and
//! progress is kept in GRDB's own `grdb_migrations` table under the same
//! identifiers, so a database any client has ever migrated carries on from
//! where it stopped. Three steps also act on rows a database already holds;
//! those are ported by hand in [`data`].
//!
//! A new migration goes at the end of [`MIGRATIONS`] with its SQL beside it,
//! and `scripts/dump_workspace_schema.sh` then regenerates the fixture the
//! CLI's and Android's tests read. Identifiers are permanent: a database in
//! the wild records which ones it ran.

pub(crate) mod data;
pub use data::grdb_timestamp;
pub mod triggers;

use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

use crate::CoreError;

/// Every migration, oldest first: its identifier and the SQL GRDB ran for it.
pub const MIGRATIONS: &[(&str, &str)] = &[
    (
        "v1_local_workspace",
        include_str!("migrations/v1_local_workspace.sql"),
    ),
    (
        "v2_metadata_and_focus",
        include_str!("migrations/v2_metadata_and_focus.sql"),
    ),
    (
        "v3_dailies_as_contributions",
        include_str!("migrations/v3_dailies_as_contributions.sql"),
    ),
    (
        "v4_manual_focus_order",
        include_str!("migrations/v4_manual_focus_order.sql"),
    ),
    (
        "v5_task_source_identity",
        include_str!("migrations/v5_task_source_identity.sql"),
    ),
    (
        "v6_inbox_as_a_system_list",
        include_str!("migrations/v6_inbox_as_a_system_list.sql"),
    ),
    (
        "v7_task_full_text_search",
        include_str!("migrations/v7_task_full_text_search.sql"),
    ),
    (
        "v8_undo_journal",
        include_str!("migrations/v8_undo_journal.sql"),
    ),
    (
        "v9_focus_block_start",
        include_str!("migrations/v9_focus_block_start.sql"),
    ),
    (
        "v10_focus_points",
        include_str!("migrations/v10_focus_points.sql"),
    ),
    (
        "v11_stable_visible_roots",
        include_str!("migrations/v11_stable_visible_roots.sql"),
    ),
    (
        "v12_task_conditions_and_work",
        include_str!("migrations/v12_task_conditions_and_work.sql"),
    ),
    (
        "v13_legacy_visible_roots",
        include_str!("migrations/v13_legacy_visible_roots.sql"),
    ),
    (
        "v14_nested_lists",
        include_str!("migrations/v14_nested_lists.sql"),
    ),
    (
        "v15_kanban_board_history",
        include_str!("migrations/v15_kanban_board_history.sql"),
    ),
    (
        "v16_task_completion_time",
        include_str!("migrations/v16_task_completion_time.sql"),
    ),
    ("v17_sync", include_str!("migrations/v17_sync.sql")),
    (
        "v18_themes_and_preferences",
        include_str!("migrations/v18_themes_and_preferences.sql"),
    ),
    (
        "v19_habit_options",
        include_str!("migrations/v19_habit_options.sql"),
    ),
    (
        "v20_waiting_follow_ups",
        include_str!("migrations/v20_waiting_follow_ups.sql"),
    ),
    (
        "v21_task_dependencies",
        include_str!("migrations/v21_task_dependencies.sql"),
    ),
    (
        "v22_matrix_order",
        include_str!("migrations/v22_matrix_order.sql"),
    ),
    (
        "v23_filter_lists",
        include_str!("migrations/v23_filter_lists.sql"),
    ),
    (
        "v24_condition_categories",
        include_str!("migrations/v24_condition_categories.sql"),
    ),
    (
        "v25_agent_instructions",
        include_str!("migrations/v25_agent_instructions.sql"),
    ),
    (
        "v26_commitment_missed_stack",
        include_str!("migrations/v26_commitment_missed_stack.sql"),
    ),
    (
        "v27_condition_order_and_context",
        include_str!("migrations/v27_condition_order_and_context.sql"),
    ),
    (
        "v28_synced_focus_context",
        include_str!("migrations/v28_synced_focus_context.sql"),
    ),
    (
        "v29_daily_contribution_skips",
        include_str!("migrations/v29_daily_contribution_skips.sql"),
    ),
    (
        "v30_google_calendar",
        include_str!("migrations/v30_google_calendar.sql"),
    ),
    (
        "v31_link_details",
        include_str!("migrations/v31_link_details.sql"),
    ),
    (
        "v32_follow_up_repeat",
        include_str!("migrations/v32_follow_up_repeat.sql"),
    ),
    (
        "v33_recurrence_series",
        include_str!("migrations/v33_recurrence_series.sql"),
    ),
    (
        "v34_condition_provides",
        include_str!("migrations/v34_condition_provides.sql"),
    ),
    (
        "v35_series_title",
        include_str!("migrations/v35_series_title.sql"),
    ),
];

/// Brings the database at `path` up to date, creating it if it does not
/// exist, and returns the identifier of the newest migration it now has.
///
/// Opens its own connection and closes it before returning, so call it
/// before the client opens the file. Waits up to five seconds for another
/// writer, as every client does.
#[uniffi::export]
pub fn migrate_workspace(path: String) -> Result<String, CoreError> {
    let mut connection = Connection::open(&path)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    migrate(&mut connection)
}

/// The migration identifiers, oldest first.
#[uniffi::export]
pub fn workspace_migrations() -> Vec<String> {
    MIGRATIONS.iter().map(|(id, _)| id.to_string()).collect()
}

/// [`migrate_workspace`] over a connection the caller owns.
///
/// Each migration runs the way GRDB's migrator runs it by default: foreign
/// keys off, the steps in one immediate transaction, then a full foreign key
/// check before the commit, so a step that leaves a dangling reference saves
/// nothing.
pub fn migrate(connection: &mut Connection) -> Result<String, CoreError> {
    migrate_through(connection, None)
}

/// [`migrate`], stopping after `last` when given. Tests use it to build a
/// database as an older install had it.
pub(crate) fn migrate_through(
    connection: &mut Connection,
    last: Option<&str>,
) -> Result<String, CoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS grdb_migrations (identifier TEXT NOT NULL PRIMARY KEY)",
    )?;
    let foreign_keys: bool = connection.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    connection.execute_batch("PRAGMA foreign_keys = OFF")?;
    let result = apply_pending(connection, last);
    if foreign_keys {
        connection.execute_batch("PRAGMA foreign_keys = ON")?;
    }
    result
}

fn apply_pending(connection: &mut Connection, last: Option<&str>) -> Result<String, CoreError> {
    let mut latest = String::new();
    for (identifier, sql) in MIGRATIONS {
        if last == Some(latest.as_str()) {
            break;
        }
        latest = identifier.to_string();
        let applied = connection
            .query_row(
                "SELECT 1 FROM grdb_migrations WHERE identifier = ?1",
                [identifier],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if applied {
            continue;
        }
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let statements = statements(sql);
        // A step that walks existing rows runs where its Swift original did:
        // after the step's first statement, which adds what it fills in.
        let mut rest = statements.iter();
        if let Some(first) = rest.next() {
            transaction.execute_batch(first)?;
        }
        data::apply(&transaction, identifier)?;
        for statement in rest {
            transaction.execute_batch(statement)?;
        }
        data::finish(&transaction, identifier)?;
        let broken: u32 =
            transaction.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })?;
        if broken > 0 {
            return Err(CoreError::ForeignKeys {
                identifier: identifier.to_string(),
                count: broken,
            });
        }
        transaction.execute(
            "INSERT INTO grdb_migrations (identifier) VALUES (?1)",
            [identifier],
        )?;
        transaction.commit()?;
    }
    Ok(latest)
}

/// A captured file's statements. They are separated by a marker line rather
/// than split on `;`, which trigger bodies contain. Only the file's leading
/// header comment is dropped; a comment inside a statement is part of the
/// text SQLite stores, and the fixture comparison would see it go.
fn statements(sql: &str) -> Vec<String> {
    let body: String = {
        let mut lines = sql.lines().peekable();
        while lines.peek().is_some_and(|line| line.starts_with("-- ")) {
            lines.next();
        }
        lines.collect::<Vec<_>>().join("\n")
    };
    body.split("\n-- statement\n")
        .map(|statement| statement.trim().to_string())
        .filter(|statement| !statement.is_empty())
        .collect()
}

/// The schema as `scripts/dump_workspace_schema.sh` writes it into
/// `cli/src/fixtures/workspace_schema.sql`: DDL in creation order, then the
/// rows a migrated database starts with. Never a row of the user's data.
pub fn dump_schema(connection: &Connection) -> Result<String, CoreError> {
    let latest: String = connection.query_row(
        "SELECT identifier FROM grdb_migrations ORDER BY rowid DESC LIMIT 1",
        [],
        |row| row.get(0),
    )?;
    let mut out = format!(
        "-- The app's workspace schema, DDL only, as of {latest}.\n\
         -- Generated by scripts/dump_workspace_schema.sh; do not edit by hand.\n"
    );
    // sqlite_sequence and the FTS5 shadow tables are created by SQLite
    // itself, and creating them by hand is an error.
    let mut ddl = connection.prepare(
        "SELECT sql || ';' FROM sqlite_master
         WHERE sql IS NOT NULL
           AND name <> 'sqlite_sequence'
           AND name NOT LIKE 'tasks_fts_%'
         ORDER BY rowid",
    )?;
    for statement in ddl.query_map([], |row| row.get::<_, String>(0))? {
        out.push_str(&statement?);
        out.push('\n');
    }
    out.push_str("\n-- The migration ledger, so a fixture reads as migrated.\n");
    let mut ledger = connection.prepare("SELECT identifier FROM grdb_migrations ORDER BY rowid")?;
    for identifier in ledger.query_map([], |row| row.get::<_, String>(0))? {
        out.push_str(&format!(
            "INSERT INTO grdb_migrations (identifier) VALUES ('{}');\n",
            identifier?
        ));
    }
    out.push_str("INSERT INTO undo_control (id, suppressed) VALUES (0, 1);\n");
    // The sync triggers read their switches from this row; without it they
    // compare against NULL and never fire. Created by v17_sync.
    let has_sync: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'sync_control')",
        [],
        |row| row.get(0),
    )?;
    if has_sync {
        out.push_str("INSERT INTO sync_control (id) VALUES (0);\n");
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
