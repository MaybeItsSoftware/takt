use rusqlite::Connection;

use super::*;
use crate::conditions::{condition_usage, create_condition_in_category, merge_conditions};
use crate::journal::{journalled, undo};
use crate::schema::migrate;

const T0: &str = "2024-01-01 00:00:00.000";

fn workspace() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON")
        .unwrap();
    migrate(&mut connection).unwrap();
    connection
        .execute_batch(&format!(
            "INSERT INTO workspaces (id, name, createdAt, updatedAt) VALUES ('w', 'Mine', '{T0}', '{T0}');
             INSERT INTO task_lists (id, workspaceId, name, sortOrder, createdAt, updatedAt)
             VALUES ('revision', 'w', 'Revision', 0, '{T0}', '{T0}'),
                    ('chores', 'w', 'Chores', 1, '{T0}', '{T0}');"
        ))
        .unwrap();
    connection
}

fn make(connection: &mut Connection, name: &str) -> String {
    journalled(connection, "New Condition", |tx| {
        create_condition_in_category(tx, "w", name, None, 0)
    })
    .unwrap()
}

/// A task in `list` under `parent`, of `kind`, requiring `groups` (JSON).
fn task(
    connection: &Connection,
    id: &str,
    list: &str,
    parent: Option<&str>,
    kind: &str,
    groups: Option<&str>,
) {
    connection
        .execute(
            "INSERT INTO tasks (id, listId, parentTaskId, title, sortOrder, itemKind, createdAt, updatedAt)
             VALUES (?1, ?2, ?3, ?1, 0, ?4, ?5, ?5)",
            params![id, list, parent, kind, T0],
        )
        .unwrap();
    if let Some(groups) = groups {
        connection
            .execute(
                "INSERT INTO task_metadata (taskId, planningJSON, updatedAt) VALUES (?1, ?2, ?3)",
                params![id, format!("{{\"requirementGroups\":{groups}}}"), T0],
            )
            .unwrap();
    }
}

fn set(
    connection: &mut Connection,
    list: &str,
    groups: Vec<Vec<String>>,
) -> Result<bool, CoreError> {
    journalled(connection, "Edit Conditions", |tx| {
        set_list_requirements(tx, list, groups, 1)
    })
}

fn candidate_groups(connection: &Connection, id: &str) -> Vec<Vec<String>> {
    crate::focus::candidates(connection, 5_000, "UTC")
        .unwrap()
        .into_iter()
        .find(|candidate| candidate.id == id)
        .unwrap()
        .requirement_groups
}

#[test]
fn every_task_in_a_list_and_its_nested_lists_inherits_what_it_requires() {
    let mut connection = workspace();
    let campus = make(&mut connection, "Campus");
    let library = make(&mut connection, "Library");
    let quiet = make(&mut connection, "Quiet");
    let desk = make(&mut connection, "Desk");
    task(&connection, "essay", "revision", None, "task", None);
    task(
        &connection,
        "maths",
        "revision",
        None,
        "list",
        Some(&format!("[[\"!{quiet}\"]]")),
    );
    task(
        &connection,
        "proofs",
        "revision",
        Some("maths"),
        "task",
        Some(&format!("[[\"{desk}\"]]")),
    );
    task(&connection, "dishes", "chores", None, "task", None);

    assert!(
        set(
            &mut connection,
            "revision",
            vec![vec![campus.clone(), library.clone()]]
        )
        .unwrap()
    );
    let either = vec![campus.clone(), library.clone()];
    assert_eq!(candidate_groups(&connection, "essay"), vec![either.clone()]);
    // Its own first, then the list's, then the nested list's.
    assert_eq!(
        candidate_groups(&connection, "proofs"),
        vec![
            vec![desk.clone()],
            either.clone(),
            vec![format!("!{quiet}")]
        ]
    );
    assert!(candidate_groups(&connection, "dishes").is_empty());

    let chain = inherited_requirements(&connection, "proofs").unwrap();
    assert_eq!(
        chain
            .iter()
            .map(|link| (link.list_id.as_str(), link.is_nested))
            .collect::<Vec<_>>(),
        vec![("revision", false), ("maths", true)]
    );
    assert_eq!(chain[0].list_name, "Revision");
    // A nested list inherits its list's, not its own.
    assert_eq!(
        inherited_requirements(&connection, "maths").unwrap().len(),
        1
    );
    assert!(
        inherited_requirements(&connection, "dishes")
            .unwrap()
            .is_empty()
    );

    // The counts see a task that only inherits.
    let usage = condition_usage(&connection, "w").unwrap();
    let campus_count = usage
        .iter()
        .find(|usage| usage.condition_id == campus)
        .unwrap()
        .open_tasks;
    assert_eq!(campus_count, 3);
    assert!(
        crate::conditions::required_condition_ids(&connection, "w")
            .unwrap()
            .contains(&library)
    );
}

#[test]
fn a_list_s_requirements_are_set_checked_merged_and_undone() {
    let mut connection = workspace();
    let campus = make(&mut connection, "Campus");
    let library = make(&mut connection, "Library");
    task(&connection, "maths", "revision", None, "list", None);

    assert!(matches!(
        set(&mut connection, "nope", vec![vec![campus.clone()]]),
        Err(CoreError::MissingList { .. })
    ));
    assert!(matches!(
        set(&mut connection, "revision", vec![vec!["ghost".into()]]),
        Err(CoreError::InvalidCondition)
    ));
    assert!(matches!(
        set(
            &mut connection,
            "revision",
            vec![vec![campus.clone()], vec![campus.clone()]]
        ),
        Err(CoreError::InvalidCondition)
    ));
    set(
        &mut connection,
        "revision",
        vec![vec![format!("!{campus}")]],
    )
    .unwrap();
    assert!(
        !set(
            &mut connection,
            "revision",
            vec![vec![format!("!{campus}")]]
        )
        .unwrap()
    );
    assert_eq!(
        list_requirement_groups(&connection, "revision").unwrap(),
        vec![vec![format!("!{campus}")]]
    );
    let list = crate::records::list(&connection, "revision")
        .unwrap()
        .unwrap();
    assert_eq!(list.requirement_groups, vec![vec![format!("!{campus}")]]);

    // A nested list's are its planning's.
    set(&mut connection, "maths", vec![vec![library.clone()]]).unwrap();
    assert_eq!(
        list_requirement_groups(&connection, "maths").unwrap(),
        vec![vec![library.clone()]]
    );

    // Merging Campus into the Library rewrites the list, negation kept.
    journalled(&mut connection, "Merge Conditions", |tx| {
        merge_conditions(tx, &campus, &library, 2)
    })
    .unwrap();
    assert_eq!(
        list_requirement_groups(&connection, "revision").unwrap(),
        vec![vec![format!("!{library}")]]
    );
    undo(&mut connection).unwrap();
    undo(&mut connection).unwrap();
    assert_eq!(
        list_requirement_groups(&connection, "revision").unwrap(),
        vec![vec![format!("!{campus}")]]
    );
    // Empty requires nothing, and is stored as none.
    set(&mut connection, "revision", Vec::new()).unwrap();
    let stored: Option<String> = connection
        .query_row(
            "SELECT requirementGroupsJSON FROM task_lists WHERE id = 'revision'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, None);
}

#[test]
fn groups_the_task_has_already_are_not_added_twice() {
    let own = vec![vec!["a".to_string(), "b".to_string()]];
    let inherited = vec![
        vec!["b".to_string(), "a".to_string()],
        vec!["c".to_string()],
    ];
    assert_eq!(
        with_inherited(&own, &inherited),
        vec![
            vec!["a".to_string(), "b".to_string()],
            vec!["c".to_string()]
        ]
    );
    assert!(stored_groups("7").is_empty());
    assert_eq!(stored_groups("[[\"a\"],[]]"), vec![vec!["a".to_string()]]);
}
