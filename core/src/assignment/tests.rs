use rusqlite::Connection;

use super::*;
use crate::schema::migrate;
use crate::tasks::{NewTask, create_task_in, set_status};

const T: &str = "2024-01-01 00:00:00.000";

/// A workspace with a list shared with Sam, a list of its own, and this
/// account as `me`.
fn workspace() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON")
        .unwrap();
    migrate(&mut connection).unwrap();
    connection
        .execute_batch(&format!(
            "INSERT INTO workspaces (id, name, createdAt, updatedAt) VALUES ('w', 'Mine', '{T}', '{T}');
             INSERT INTO task_lists (id, workspaceId, name, sortOrder, createdAt, updatedAt)
               VALUES ('shared', 'w', 'Groceries', 0, '{T}', '{T}'), ('own', 'w', 'Mine', 1, '{T}', '{T}');
             INSERT INTO sync_spaces (spaceId, listId, role, name, membersJSON) VALUES ('s', 'shared', 'owner', 'Groceries',
               '[{{\"accountId\":\"me\",\"email\":\"me@example.com\",\"role\":\"owner\",\"you\":true}},
                 {{\"accountId\":\"sam\",\"email\":\"sam@example.com\",\"role\":\"member\",\"you\":false}}]');"
        ))
        .unwrap();
    connection
}

fn add(connection: &mut Connection, list: &str, title: &str, parent: Option<&str>) -> String {
    let tx = connection.transaction().unwrap();
    let id = create_task_in(
        &tx,
        &NewTask {
            list_id: list.into(),
            title: title.into(),
            parent_task_id: parent.map(str::to_string),
            kind: "task".into(),
            ..NewTask::default()
        },
        1_000,
        "UTC",
    )
    .unwrap();
    tx.commit().unwrap();
    id
}

fn text(connection: &Connection, sql: &str) -> Option<String> {
    connection.query_row(sql, [], |row| row.get(0)).unwrap()
}

#[test]
fn a_task_in_a_shared_list_says_who_added_and_finished_it() {
    let mut connection = workspace();
    let milk = add(&mut connection, "shared", "Milk", None);
    let oat = add(&mut connection, "shared", "Oat", Some(&milk));
    let mine = add(&mut connection, "own", "Mine", None);
    let by = |c: &Connection, column: &str, id: &str| {
        text(c, &format!("SELECT {column} FROM tasks WHERE id = '{id}'"))
    };
    assert_eq!(by(&connection, "createdBy", &milk).as_deref(), Some("me"));
    assert_eq!(
        by(&connection, "createdBy", &mine),
        None,
        "a list of your own has no author"
    );

    let tx = connection.transaction().unwrap();
    set_status(&tx, &milk, "completed", 2_000, "UTC").unwrap();
    set_status(&tx, &mine, "completed", 2_000, "UTC").unwrap();
    tx.commit().unwrap();
    assert_eq!(by(&connection, "completedBy", &milk).as_deref(), Some("me"));
    assert_eq!(
        by(&connection, "completedBy", &oat).as_deref(),
        Some("me"),
        "closed with it"
    );
    assert_eq!(by(&connection, "completedBy", &mine), None);
    let sharing = task_sharing(&connection, &milk).unwrap().unwrap();
    assert!(sharing.completed_by.unwrap().you);
    assert!(task_sharing(&connection, &mine).unwrap().is_none());

    let tx = connection.transaction().unwrap();
    set_status(&tx, &milk, "open", 3_000, "UTC").unwrap();
    tx.commit().unwrap();
    assert_eq!(
        by(&connection, "completedBy", &milk),
        None,
        "reopening takes it back"
    );
    assert_eq!(
        by(&connection, "completedBy", &oat).as_deref(),
        Some("me"),
        "a subtask stays done"
    );
}

#[test]
fn a_task_assigned_to_someone_else_is_not_offered() {
    let mut connection = workspace();
    let milk = add(&mut connection, "shared", "Milk", None);
    let eggs = add(&mut connection, "shared", "Eggs", None);
    let mine = add(&mut connection, "own", "Mine", None);
    let offered = |c: &Connection| -> Vec<String> {
        let mut ids: Vec<String> = crate::focus::candidates(c, 5_000, "UTC")
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        ids.sort();
        ids
    };
    let mut all = vec![milk.clone(), eggs.clone(), mine.clone()];
    all.sort();
    assert_eq!(offered(&connection), all);

    let tx = connection.transaction().unwrap();
    assign_task(&tx, &milk, Some("sam"), 4_000).unwrap();
    assign_task(&tx, &eggs, Some("me"), 4_000).unwrap();
    assert!(
        assign_task(&tx, &eggs, Some("stranger"), 4_000).is_err(),
        "not in the list"
    );
    assert!(
        assign_task(&tx, &mine, Some("me"), 4_000).is_err(),
        "not shared"
    );
    tx.commit().unwrap();
    let mut expected = vec![eggs.clone(), mine.clone()];
    expected.sort();
    assert_eq!(offered(&connection), expected, "Sam's is Sam's");
    let sharing = task_sharing(&connection, &milk).unwrap().unwrap();
    assert_eq!(
        sharing.assignee.unwrap().email.as_deref(),
        Some("sam@example.com")
    );

    let tx = connection.transaction().unwrap();
    assign_task(&tx, &milk, None, 6_000).unwrap();
    tx.commit().unwrap();
    assert_eq!(
        offered(&connection),
        all,
        "unassigned, it is anyone's again"
    );
}
