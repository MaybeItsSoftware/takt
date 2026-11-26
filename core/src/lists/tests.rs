use rusqlite::Connection;

use super::*;
use crate::journal::{journalled, undo};
use crate::schema::migrate;

const T: &str = "2024-01-01 00:00:00.000";

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
               ('f', 'w', NULL, 'Work', 0, '{T}', '{T}'),
               ('sub', 'w', 'f', 'Clients', 0, '{T}', '{T}');
             INSERT INTO task_lists (id, workspaceId, folderId, name, sortOrder, createdAt, updatedAt, systemRole) VALUES
               ('inbox', 'w', NULL, 'Inbox', 0, '{T}', '{T}', 'inbox'),
               ('l', 'w', 'f', 'Proposals', 1, '{T}', '{T}', NULL),
               ('deep', 'w', 'sub', 'Acme', 2, '{T}', '{T}', NULL);
             INSERT INTO tasks (id, listId, title, sortOrder, createdAt, updatedAt) VALUES
               ('a', 'l', 'Draft', 0, '{T}', '{T}'),
               ('b', 'l', 'Send', 1, '{T}', '{T}');"
        ))
        .unwrap();
    connection
}

fn count(connection: &Connection, sql: &str) -> i64 {
    connection.query_row(sql, [], |row| row.get(0)).unwrap()
}

#[test]
fn deleting_a_list_takes_its_tasks_and_one_undo_brings_them_back() {
    let mut connection = workspace();
    let deleted = journalled(&mut connection, "Delete List", |tx| delete_list(tx, "l")).unwrap();
    assert_eq!(
        deleted,
        DeletedList {
            id: "l".into(),
            name: "Proposals".into(),
            tasks_deleted: 2
        }
    );
    assert_eq!(count(&connection, "SELECT COUNT(*) FROM tasks"), 0);
    undo(&mut connection).unwrap();
    assert_eq!(
        count(&connection, "SELECT COUNT(*) FROM tasks WHERE listId = 'l'"),
        2
    );
}

#[test]
fn the_inbox_cannot_be_deleted_and_a_missing_list_is_named() {
    let mut connection = workspace();
    assert!(matches!(
        journalled(&mut connection, "Delete List", |tx| delete_list(
            tx, "inbox"
        )),
        Err(CoreError::SystemListIsPermanent)
    ));
    assert!(matches!(
        journalled(&mut connection, "Delete List", |tx| delete_list(tx, "nope")),
        Err(CoreError::MissingList { ref id }) if id == "nope"
    ));
    assert_eq!(count(&connection, "SELECT COUNT(*) FROM change_log"), 0);
}

#[test]
fn deleting_a_folder_keeps_its_lists_and_takes_its_folders() {
    let mut connection = workspace();
    journalled(&mut connection, "Delete Folder", |tx| {
        delete_folder(tx, "f")
    })
    .unwrap();
    assert_eq!(count(&connection, "SELECT COUNT(*) FROM list_folders"), 0);
    assert_eq!(
        count(
            &connection,
            "SELECT COUNT(*) FROM task_lists WHERE folderId IS NULL"
        ),
        3
    );
    assert_eq!(count(&connection, "SELECT COUNT(*) FROM tasks"), 2);

    undo(&mut connection).unwrap();
    assert_eq!(count(&connection, "SELECT COUNT(*) FROM list_folders"), 2);
    assert_eq!(
        connection
            .query_row(
                "SELECT folderId FROM task_lists WHERE id = 'deep'",
                [],
                |row| row.get::<_, Option<String>>(0)
            )
            .unwrap()
            .as_deref(),
        Some("sub")
    );
}

#[test]
fn deleting_a_missing_folder_is_named() {
    let mut connection = workspace();
    assert!(matches!(
        journalled(&mut connection, "Delete Folder", |tx| delete_folder(tx, "nope")),
        Err(CoreError::MissingFolder { ref id }) if id == "nope"
    ));
}

const NOW: i64 = 1_700_000_000_123;
const NOW_TEXT: &str = "2023-11-14 22:13:20.123";

fn steps(connection: &Connection) -> i64 {
    count(connection, "SELECT COUNT(DISTINCT groupId) FROM change_log")
}

#[test]
fn renaming_a_folder_trims_stamps_and_ignores_its_own_name() {
    let mut connection = workspace();
    journalled(&mut connection, "Rename Folder", |tx| {
        rename_folder(tx, "f", "  Office ", NOW)
    })
    .unwrap();
    let (name, updated): (String, String) = connection
        .query_row(
            "SELECT name, updatedAt FROM list_folders WHERE id = 'f'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((name.as_str(), updated.as_str()), ("Office", NOW_TEXT));
    assert_eq!(steps(&connection), 1);

    journalled(&mut connection, "Rename Folder", |tx| {
        rename_folder(tx, "f", "Office", NOW)
    })
    .unwrap();
    assert_eq!(steps(&connection), 1);
    assert!(matches!(
        journalled(&mut connection, "Rename Folder", |tx| rename_folder(
            tx, "f", "   ", NOW
        )),
        Err(CoreError::EmptyName)
    ));
}

#[test]
fn renaming_a_list_leaves_its_colour_and_editing_sets_both() {
    let mut connection = workspace();
    journalled(&mut connection, "Edit List", |tx| {
        update_list(tx, "l", "Bids", Some(" #ABCDEF "), NOW)
    })
    .unwrap();
    journalled(&mut connection, "Rename List", |tx| {
        rename_list(tx, "l", "Tenders", NOW)
    })
    .unwrap();
    let (name, colour): (String, Option<String>) = connection
        .query_row(
            "SELECT name, colorHex FROM task_lists WHERE id = 'l'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        (name.as_str(), colour.as_deref()),
        ("Tenders", Some("#ABCDEF"))
    );

    journalled(&mut connection, "Edit List", |tx| {
        update_list(tx, "l", "Tenders", Some("  "), NOW)
    })
    .unwrap();
    let colour: Option<String> = connection
        .query_row(
            "SELECT colorHex FROM task_lists WHERE id = 'l'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(colour, None);
    assert_eq!(undo(&mut connection).unwrap().as_deref(), Some("Edit List"));
}

#[test]
fn the_inbox_cannot_be_archived_but_can_be_restored() {
    let mut connection = workspace();
    assert!(matches!(
        journalled(&mut connection, "Archive List", |tx| set_list_archived(
            tx, "inbox", true, NOW
        )),
        Err(CoreError::SystemListIsPermanent)
    ));
    journalled(&mut connection, "Archive List", |tx| {
        set_list_archived(tx, "inbox", false, NOW)
    })
    .unwrap();
    journalled(&mut connection, "Archive List", |tx| {
        set_list_archived(tx, "l", true, NOW)
    })
    .unwrap();
    assert_eq!(
        count(
            &connection,
            "SELECT isArchived FROM task_lists WHERE id = 'l'"
        ),
        1
    );
    assert!(matches!(
        journalled(&mut connection, "Archive List", |tx| set_list_archived(
            tx, "nope", true, NOW
        )),
        Err(CoreError::MissingList { .. })
    ));
}

#[test]
fn a_new_folder_and_list_go_after_their_siblings_and_undo_as_one_step_each() {
    let mut connection = workspace();
    let folder = journalled(&mut connection, "New Folder", |tx| {
        create_folder(tx, "w", "  Clients ", Some("f"), NOW)
    })
    .unwrap();
    assert_eq!((folder.name.as_str(), folder.sort_order), ("Clients", 1));
    assert_eq!(folder.id, folder.id.to_uppercase());
    assert_eq!(folder.id.len(), 36);

    let list = journalled(&mut connection, "New List", |tx| {
        create_list(tx, "w", "Acme", None, NOW)
    })
    .unwrap();
    assert_eq!((list.name.as_str(), list.sort_order), ("Acme", 1));
    let (archived, created): (bool, String) = connection
        .query_row(
            "SELECT isArchived, createdAt FROM task_lists WHERE id = ?1",
            [&list.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((archived, created.as_str()), (false, NOW_TEXT));

    assert_eq!(undo(&mut connection).unwrap().as_deref(), Some("New List"));
    assert_eq!(
        undo(&mut connection).unwrap().as_deref(),
        Some("New Folder")
    );
    assert_eq!(count(&connection, "SELECT COUNT(*) FROM list_folders"), 2);
}

#[test]
fn a_new_item_in_a_folder_from_another_workspace_or_none_is_refused() {
    let mut connection = workspace();
    connection
        .execute_batch(&format!(
            "INSERT INTO workspaces (id, name, createdAt, updatedAt) VALUES ('w2', 'Theirs', '{T}', '{T}');
             INSERT INTO list_folders (id, workspaceId, name, sortOrder, createdAt, updatedAt)
               VALUES ('elsewhere', 'w2', 'Away', 0, '{T}', '{T}');"
        ))
        .unwrap();
    for folder in ["elsewhere", "nope"] {
        assert!(matches!(
            journalled(&mut connection, "New List", |tx| create_list(
                tx,
                "w",
                "X",
                Some(folder),
                NOW
            )),
            Err(CoreError::MissingFolder { .. })
        ));
        assert!(matches!(
            journalled(&mut connection, "New Folder", |tx| create_folder(
                tx,
                "w",
                "X",
                Some(folder),
                NOW
            )),
            Err(CoreError::MissingFolder { .. })
        ));
    }
    assert!(matches!(
        journalled(&mut connection, "New List", |tx| create_list(
            tx, "w", " ", None, NOW
        )),
        Err(CoreError::EmptyName)
    ));
}

fn order(connection: &Connection, sql: &str) -> Vec<String> {
    let mut statement = connection.prepare(sql).unwrap();
    statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn top_lists(connection: &Connection) -> Vec<String> {
    order(
        connection,
        "SELECT id FROM task_lists WHERE folderId IS NULL AND isArchived = 0 ORDER BY sortOrder, createdAt, id",
    )
}

#[test]
fn a_folder_cannot_move_into_itself_or_below_itself() {
    let mut connection = workspace();
    for parent in ["f", "sub"] {
        assert!(matches!(
            journalled(&mut connection, "Move Folder", |tx| move_folder(
                tx,
                "f",
                Some(parent),
                NOW
            )),
            Err(CoreError::InvalidFolderMove)
        ));
        assert!(matches!(
            journalled(&mut connection, "Reorder Folder", |tx| place_folder(
                tx,
                "f",
                None,
                Some(parent),
                NOW
            )),
            Err(CoreError::InvalidFolderMove)
        ));
    }
    journalled(&mut connection, "Move Folder", |tx| {
        move_folder(tx, "sub", None, NOW)
    })
    .unwrap();
    let parent: Option<String> = connection
        .query_row(
            "SELECT parentFolderId FROM list_folders WHERE id = 'sub'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(parent, None);
    assert_eq!(
        count(
            &connection,
            "SELECT sortOrder FROM list_folders WHERE id = 'sub'"
        ),
        1
    );
}

#[test]
fn a_list_moves_to_the_end_of_its_new_folder_and_not_at_all_when_already_there() {
    let mut connection = workspace();
    journalled(&mut connection, "Move List", |tx| {
        move_list(tx, "l", None, NOW)
    })
    .unwrap();
    assert_eq!(top_lists(&connection), ["inbox", "l"]);
    let before = steps(&connection);
    journalled(&mut connection, "Move List", |tx| {
        move_list(tx, "l", None, NOW)
    })
    .unwrap();
    assert_eq!(steps(&connection), before);
}

#[test]
fn nudging_a_list_renumbers_every_sibling_and_stops_at_the_ends() {
    let mut connection = workspace();
    journalled(&mut connection, "Move List", |tx| {
        move_list(tx, "l", None, NOW)
    })
    .unwrap();
    journalled(&mut connection, "Move List", |tx| {
        move_list(tx, "deep", None, NOW)
    })
    .unwrap();
    assert_eq!(top_lists(&connection), ["inbox", "l", "deep"]);

    journalled(&mut connection, "Reorder List", |tx| {
        move_list_within_folder(tx, "deep", -5, NOW)
    })
    .unwrap();
    assert_eq!(top_lists(&connection), ["deep", "inbox", "l"]);
    assert_eq!(
        order(
            &connection,
            "SELECT sortOrder || ':' || updatedAt FROM task_lists WHERE folderId IS NULL ORDER BY sortOrder"
        ),
        [
            format!("0:{NOW_TEXT}"),
            format!("1:{NOW_TEXT}"),
            format!("2:{NOW_TEXT}")
        ]
    );
    let before = steps(&connection);
    journalled(&mut connection, "Reorder List", |tx| {
        move_list_within_folder(tx, "deep", -1, NOW)
    })
    .unwrap();
    assert_eq!(steps(&connection), before);
}

#[test]
fn dropping_a_list_before_another_moves_it_into_that_folder_in_that_place() {
    let mut connection = workspace();
    journalled(&mut connection, "Reorder List", |tx| {
        place_list(tx, "deep", Some("l"), Some("f"), NOW)
    })
    .unwrap();
    assert_eq!(
        order(
            &connection,
            "SELECT id FROM task_lists WHERE folderId = 'f' ORDER BY sortOrder"
        ),
        ["deep", "l"]
    );
    journalled(&mut connection, "Reorder List", |tx| {
        place_list(tx, "deep", None, Some("f"), NOW)
    })
    .unwrap();
    assert_eq!(
        order(
            &connection,
            "SELECT id FROM task_lists WHERE folderId = 'f' ORDER BY sortOrder"
        ),
        ["l", "deep"]
    );
    assert_eq!(
        undo(&mut connection).unwrap().as_deref(),
        Some("Reorder List")
    );
    assert_eq!(
        order(
            &connection,
            "SELECT id FROM task_lists WHERE folderId = 'f' ORDER BY sortOrder"
        ),
        ["deep", "l"]
    );
}

#[test]
fn dropping_a_folder_and_nudging_one_reorder_their_siblings() {
    let mut connection = workspace();
    journalled(&mut connection, "Reorder Folder", |tx| {
        place_folder(tx, "sub", Some("f"), None, NOW)
    })
    .unwrap();
    let top = || "SELECT id FROM list_folders WHERE parentFolderId IS NULL ORDER BY sortOrder";
    assert_eq!(order(&connection, top()), ["sub", "f"]);
    journalled(&mut connection, "Reorder Folder", |tx| {
        move_folder_within_siblings(tx, "sub", 1, NOW)
    })
    .unwrap();
    assert_eq!(order(&connection, top()), ["f", "sub"]);
}

#[test]
fn the_settings_sheets_save_what_changed_and_guard_the_tree_and_the_inbox() {
    let mut connection = workspace();
    journalled(&mut connection, "Edit Folder", |tx| {
        save_folder_settings(tx, "sub", "Clients", None, NOW)
    })
    .unwrap();
    assert_eq!(
        order(
            &connection,
            "SELECT id FROM list_folders WHERE parentFolderId IS NULL ORDER BY sortOrder"
        ),
        ["f", "sub"]
    );
    assert!(matches!(
        journalled(&mut connection, "Edit Folder", |tx| save_folder_settings(
            tx,
            "f",
            "Work",
            Some("f"),
            NOW
        )),
        Err(CoreError::InvalidFolderMove)
    ));
    let inbox = ListSettings {
        name: "Inbox".into(),
        colour_hex: None,
        folder_id: None,
        is_archived: true,
        visible_root_task_id: None,
    };
    assert!(matches!(
        journalled(&mut connection, "Edit List", |tx| save_list_settings(
            tx, "inbox", &inbox, NOW
        )),
        Err(CoreError::SystemListIsPermanent)
    ));
    let bad_root = ListSettings {
        name: "Proposals".into(),
        colour_hex: Some(" #FFAA00 ".into()),
        folder_id: None,
        is_archived: false,
        visible_root_task_id: Some("a".into()),
    };
    assert!(matches!(
        journalled(&mut connection, "Edit List", |tx| save_list_settings(
            tx, "l", &bad_root, NOW
        )),
        Err(CoreError::InvalidVisibleRoot)
    ));
    let moved = ListSettings {
        visible_root_task_id: None,
        ..bad_root
    };
    journalled(&mut connection, "Edit List", |tx| {
        save_list_settings(tx, "l", &moved, NOW)
    })
    .unwrap();
    let (colour, folder): (Option<String>, Option<String>) = connection
        .query_row(
            "SELECT colorHex, folderId FROM task_lists WHERE id = 'l'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((colour.as_deref(), folder), (Some("#FFAA00"), None));
}

fn priorities(connection: &Connection) -> Vec<(String, i64)> {
    let mut statement = connection
        .prepare("SELECT id, priorityRank FROM task_lists WHERE priorityRank IS NOT NULL ORDER BY priorityRank")
        .unwrap();
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn prioritise(connection: &mut Connection, id: &str, position: Option<i64>) {
    journalled(connection, "Prioritise List", |tx| {
        set_list_priority(tx, id, position, NOW)
    })
    .unwrap();
}

#[test]
fn prioritising_inserts_moves_and_removes_keeping_ranks_contiguous() {
    let mut connection = workspace();
    let ranks = |c: &Connection| -> Vec<(String, i64)> { priorities(c) };
    let named = |pairs: &[(&str, i64)]| -> Vec<(String, i64)> {
        pairs
            .iter()
            .map(|(id, rank)| (id.to_string(), *rank))
            .collect()
    };
    prioritise(&mut connection, "l", Some(1));
    // Past the end is last.
    prioritise(&mut connection, "deep", Some(99));
    assert_eq!(ranks(&connection), named(&[("l", 1), ("deep", 2)]));
    // Below 1 is first, and the rest move down.
    prioritise(&mut connection, "inbox", Some(0));
    assert_eq!(
        ranks(&connection),
        named(&[("inbox", 1), ("l", 2), ("deep", 3)])
    );
    // Moving within them shifts only what lies between.
    prioritise(&mut connection, "deep", Some(1));
    assert_eq!(
        ranks(&connection),
        named(&[("deep", 1), ("inbox", 2), ("l", 3)])
    );
    // Taking one out closes the gap.
    prioritise(&mut connection, "inbox", None);
    assert_eq!(ranks(&connection), named(&[("deep", 1), ("l", 2)]));
    // Each was one step, and undo takes the last one back whole.
    undo(&mut connection).unwrap();
    assert_eq!(
        ranks(&connection),
        named(&[("deep", 1), ("inbox", 2), ("l", 3)])
    );
    // Putting a list where it already is changes nothing.
    let before = steps(&connection);
    let _ = journalled(&mut connection, "Remove List Priority", |tx| {
        set_list_priority(tx, "inbox", Some(2), NOW)
    });
    assert_eq!(steps(&connection), before);
}

#[test]
fn an_archived_list_cannot_be_prioritised_and_archiving_drops_its_rank() {
    let mut connection = workspace();
    prioritise(&mut connection, "l", Some(1));
    prioritise(&mut connection, "deep", Some(2));
    journalled(&mut connection, "Archive List", |tx| {
        set_list_archived(tx, "l", true, NOW)
    })
    .unwrap();
    assert_eq!(priorities(&connection), [("deep".to_string(), 1)]);
    assert!(matches!(
        journalled(&mut connection, "Prioritise List", |tx| {
            set_list_priority(tx, "l", Some(1), NOW)
        }),
        Err(CoreError::ListCannotBePrioritised)
    ));
    // Restoring it does not bring the rank back; undoing the archive does.
    undo(&mut connection).unwrap();
    assert_eq!(
        priorities(&connection),
        [("l".to_string(), 1), ("deep".to_string(), 2)]
    );
    // Deleting one closes the gap too.
    journalled(&mut connection, "Delete List", |tx| delete_list(tx, "l")).unwrap();
    assert_eq!(priorities(&connection), [("deep".to_string(), 1)]);
    assert!(matches!(
        journalled(&mut connection, "Prioritise List", |tx| {
            set_list_priority(tx, "gone", Some(1), NOW)
        }),
        Err(CoreError::MissingList { .. })
    ));
}

#[test]
fn the_prioritised_lists_read_in_order_with_any_merge_gap_closed() {
    let connection = workspace();
    connection
        .execute_batch(
            "UPDATE task_lists SET priorityRank = 5 WHERE id = 'l';
                        UPDATE task_lists SET priorityRank = 2 WHERE id = 'deep';",
        )
        .unwrap();
    let read = crate::records::prioritised_lists(&connection, "w").unwrap();
    let read: Vec<(&str, Option<i64>)> = read
        .iter()
        .map(|list| (list.id.as_str(), list.priority_rank))
        .collect();
    assert_eq!(read, [("deep", Some(1)), ("l", Some(2))]);
}
