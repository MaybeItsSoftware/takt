use rusqlite::Connection;

use super::*;
use crate::journal::{journalled, redo, undo, undoable_label};
use crate::schema::migrate;

const T: &str = "2024-01-01 00:00:00.000";
const NOW: i64 = 1_718_452_800_000;

fn workspace() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON")
        .unwrap();
    migrate(&mut connection).unwrap();
    connection
        .execute_batch(&format!(
            "INSERT INTO workspaces (id, name, createdAt, updatedAt) VALUES ('w', 'Mine', '{T}', '{T}');
             INSERT INTO list_folders (id, workspaceId, parentFolderId, name, sortOrder, createdAt, updatedAt) VALUES
               ('work', 'w', NULL, 'Work', 0, '{T}', '{T}');
             INSERT INTO task_lists (id, workspaceId, folderId, name, sortOrder, isArchived, systemRole, createdAt, updatedAt) VALUES
               ('groceries', 'w', NULL, 'Groceries', 0, 0, NULL, '{T}', '{T}'),
               ('reports', 'w', 'work', 'Reports', 1, 0, NULL, '{T}', '{T}');"
        ))
        .unwrap();
    connection
}

fn set(
    connection: &mut Connection,
    kind: AgentInstructionKind,
    id: Option<&str>,
    text: &str,
    append: bool,
) -> Result<String, CoreError> {
    journalled(connection, "Edit Agent Instructions", |tx| {
        set_agent_instructions(tx, kind, id, text, append, NOW)
    })
}

fn rows(connection: &Connection) -> Vec<(String, String)> {
    connection
        .prepare("SELECT scope, text FROM agent_instructions ORDER BY scope")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn each_scope_is_one_row_keyed_by_the_scope_itself() {
    let mut connection = workspace();
    use AgentInstructionKind::*;
    set(
        &mut connection,
        Account,
        None,
        "  Titles are verbs.  ",
        false,
    )
    .unwrap();
    set(
        &mut connection,
        Folder,
        Some("work"),
        "Work is Monday to Thursday.",
        false,
    )
    .unwrap();
    set(
        &mut connection,
        List,
        Some("groceries"),
        "Group by aisle.",
        false,
    )
    .unwrap();
    assert_eq!(
        rows(&connection),
        [
            ("account".into(), "Titles are verbs.".into()),
            ("folder:work".into(), "Work is Monday to Thursday.".into()),
            ("list:groceries".into(), "Group by aisle.".into()),
        ]
    );

    let all = agent_instructions(&connection).unwrap();
    assert_eq!(
        all.iter()
            .map(|entry| (entry.kind, entry.name.clone()))
            .collect::<Vec<_>>(),
        [
            (Account, None),
            (Folder, Some("Work".into())),
            (List, Some("Groceries".into())),
        ]
    );
    assert_eq!(all[0].updated_at_ms, NOW);

    let one = agent_instruction(&connection, List, Some("groceries")).unwrap();
    assert_eq!(one.text, "Group by aisle.");
    assert_eq!(one.id.as_deref(), Some("groceries"));
    let none = agent_instruction(&connection, List, Some("reports")).unwrap();
    assert_eq!(none.text, "");
    assert_eq!(none.name.as_deref(), Some("Reports"));
}

#[test]
fn append_adds_a_paragraph_and_blank_text_clears_the_scope() {
    let mut connection = workspace();
    use AgentInstructionKind::*;
    assert_eq!(
        set(
            &mut connection,
            List,
            Some("groceries"),
            "Group by aisle.",
            true
        )
        .unwrap(),
        "Group by aisle."
    );
    assert_eq!(
        set(
            &mut connection,
            List,
            Some("groceries"),
            "Oat milk, not dairy.",
            true
        )
        .unwrap(),
        "Group by aisle.\n\nOat milk, not dairy."
    );
    assert_eq!(
        set(&mut connection, List, Some("groceries"), " ", false).unwrap(),
        ""
    );
    assert!(rows(&connection).is_empty());
}

#[test]
fn a_scope_that_does_not_exist_is_refused() {
    let mut connection = workspace();
    use AgentInstructionKind::*;
    assert!(matches!(
        set(&mut connection, List, Some("nope"), "x", false),
        Err(CoreError::MissingList { .. })
    ));
    assert!(matches!(
        set(&mut connection, Folder, None, "x", false),
        Err(CoreError::MissingFolder { .. })
    ));
    assert!(rows(&connection).is_empty());
}

#[test]
fn a_change_is_one_undo_step_and_an_unchanged_save_is_none() {
    let mut connection = workspace();
    use AgentInstructionKind::*;
    set(&mut connection, Account, None, "First.", false).unwrap();
    set(&mut connection, Account, None, "Second.", false).unwrap();
    let steps = crate::journal::history(&connection, 10).unwrap();
    set(&mut connection, Account, None, "Second.", false).unwrap();
    assert_eq!(crate::journal::history(&connection, 10).unwrap(), steps);
    assert_eq!(
        undoable_label(&connection).unwrap().as_deref(),
        Some("Edit Agent Instructions")
    );

    undo(&mut connection).unwrap();
    assert_eq!(rows(&connection), [("account".into(), "First.".into())]);
    undo(&mut connection).unwrap();
    assert!(rows(&connection).is_empty());
    redo(&mut connection).unwrap();
    assert_eq!(rows(&connection), [("account".into(), "First.".into())]);
}

#[test]
fn a_write_is_queued_for_sync_once_paired() {
    let mut connection = workspace();
    connection
        .execute_batch("UPDATE sync_control SET recording = 1 WHERE id = 0")
        .unwrap();
    set(
        &mut connection,
        AgentInstructionKind::Account,
        None,
        "Hi.",
        false,
    )
    .unwrap();
    let queued: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sync_outbox WHERE tableName = 'agent_instructions' AND rowId = 'account'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(queued, 1);
}

#[test]
fn an_import_fills_only_empty_scopes_outside_the_journal() {
    let mut connection = workspace();
    use AgentInstructionKind::*;
    set(&mut connection, Account, None, "Kept.", false).unwrap();
    let steps = crate::journal::history(&connection, 10).unwrap();
    let seed = |kind, id: Option<&str>, text: &str| AgentInstructionSeed {
        kind,
        id: id.map(str::to_string),
        text: text.into(),
    };
    let tx = connection.transaction().unwrap();
    let written = import_agent_instructions(
        &tx,
        &[
            seed(Account, None, "Overwritten?"),
            seed(Folder, Some("work"), "Folder text"),
            seed(List, Some("deleted"), "Gone"),
            seed(List, Some("groceries"), "   "),
        ],
        NOW,
    )
    .unwrap();
    tx.commit().unwrap();
    assert_eq!(written, 1);
    assert_eq!(
        rows(&connection),
        [
            ("account".into(), "Kept.".into()),
            ("folder:work".into(), "Folder text".into()),
        ]
    );
    assert_eq!(crate::journal::history(&connection, 10).unwrap(), steps);
}

#[test]
fn a_row_for_a_deleted_list_is_not_read_back() {
    let mut connection = workspace();
    set(
        &mut connection,
        AgentInstructionKind::List,
        Some("groceries"),
        "Aisles.",
        false,
    )
    .unwrap();
    connection
        .execute("DELETE FROM task_lists WHERE id = 'groceries'", [])
        .unwrap();
    assert!(agent_instructions(&connection).unwrap().is_empty());
}

#[test]
fn a_tasks_agent_context_is_its_own_and_one_undo_step() {
    let mut connection = workspace();
    connection
        .execute_batch(&format!(
            "INSERT INTO tasks (id, listId, title, notes, sortOrder, createdAt, updatedAt)
               VALUES ('t', 'groceries', 'Milk', 'oat, not soy', 0, '{T}', '{T}');"
        ))
        .unwrap();
    assert_eq!(task_agent_context(&connection, "t").unwrap(), "");
    let set_context = |connection: &mut Connection, text: &str, append: bool| {
        journalled(connection, "Edit Agent Context", |tx| {
            set_task_agent_context(tx, "t", text, append, NOW)
        })
    };
    // A task with no metadata row gets one.
    assert_eq!(
        set_context(&mut connection, "  The shop shuts at six.  ", false).unwrap(),
        "The shop shuts at six."
    );
    assert_eq!(
        set_context(&mut connection, "Ask before buying more than two.", true).unwrap(),
        "The shop shuts at six.\n\nAsk before buying more than two."
    );
    // The notes are untouched.
    let notes: String = connection
        .query_row("SELECT notes FROM tasks WHERE id = 't'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(notes, "oat, not soy");
    assert_eq!(
        undoable_label(&connection).unwrap().as_deref(),
        Some("Edit Agent Context")
    );
    undo(&mut connection).unwrap();
    assert_eq!(
        task_agent_context(&connection, "t").unwrap(),
        "The shop shuts at six."
    );
    // Blank clears it to NULL.
    assert_eq!(set_context(&mut connection, " ", false).unwrap(), "");
    let stored: Option<String> = connection
        .query_row(
            "SELECT agentContext FROM task_metadata WHERE taskId = 't'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, None);
    assert!(matches!(
        task_agent_context(&connection, "nope"),
        Err(CoreError::MissingTask { .. })
    ));
}
