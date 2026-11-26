use rusqlite::Connection;

use super::*;
use crate::conditions::{
    ConditionsSaid, condition_sections, create_condition_in_category, holdings_on,
    set_condition_provides, switched,
};
use crate::focus_context::{StoredFocusContext, load, save};
use crate::journal::{journalled, undo};
use crate::schema::migrate;

fn condition(id: &str, device: Option<&str>, provides: &[&str]) -> ConditionRow {
    ConditionRow {
        id: id.into(),
        workspace_id: "w".into(),
        name: id.into(),
        is_location: false,
        is_archived: false,
        created_at_ms: 0,
        updated_at_ms: 0,
        category: None,
        sort_order: None,
        provides: provides.iter().map(|id| id.to_string()).collect(),
        device_kind: device.map(Into::into),
    }
}

fn strings(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

/// A Laptop that provides a Big screen, a Phone, and Home, which (from
/// before the Laptop was a device) still names it among what it provides.
fn situation() -> Vec<ConditionRow> {
    vec![
        condition("laptop", Some("laptop"), &["screen"]),
        condition("phone", Some("phone"), &[]),
        condition("home", None, &["laptop", "quiet"]),
        condition("screen", None, &[]),
        condition("quiet", None, &[]),
    ]
}

fn held(device: Option<&str>, set: &[&str]) -> Vec<String> {
    let mut ids: Vec<String> = holdings_on(&situation(), &strings(set), &[], device)
        .into_iter()
        .filter(|holding| holding.holds)
        .map(|holding| holding.condition_id)
        .collect();
    ids.sort();
    ids
}

#[test]
fn a_device_condition_holds_on_its_kind_of_device_and_nowhere_else() {
    assert_eq!(held(Some("laptop"), &[]), strings(&["laptop", "screen"]));
    assert_eq!(held(Some("phone"), &[]), strings(&["phone"]));
    assert_eq!(held(Some("tablet"), &[]), Vec::<String>::new());
    assert_eq!(held(None, &[]), Vec::<String>::new());
    // Said to hold, it is ignored, and Home cannot provide it: on the phone
    // at Home the laptop is not in your hand.
    assert_eq!(
        held(Some("phone"), &["laptop", "home"]),
        strings(&["home", "phone", "quiet"])
    );
    let holdings = holdings_on(&situation(), &[], &[], Some("laptop"));
    let laptop = holdings
        .iter()
        .find(|h| h.condition_id == "laptop")
        .unwrap();
    assert!(laptop.is_device && laptop.holds && !laptop.is_set);
    let screen = holdings
        .iter()
        .find(|h| h.condition_id == "screen")
        .unwrap();
    assert_eq!(screen.provided_by, strings(&["laptop"]));
    assert!(!screen.is_device);
    // Switched off by hand, it still holds: the device decides.
    let off = holdings_on(&situation(), &[], &strings(&["laptop"]), Some("laptop"));
    assert!(off.iter().any(|h| h.condition_id == "laptop" && h.holds));
}

#[test]
fn a_device_condition_cannot_be_switched() {
    let said = ConditionsSaid {
        condition_ids: strings(&["home"]),
        off_ids: Vec::new(),
    };
    assert_eq!(switched(&situation(), &said, "laptop", Some(true)), said);
    assert_eq!(switched(&situation(), &said, "laptop", None), said);
}

#[test]
fn device_conditions_come_first_under_their_own_heading() {
    let sections = condition_sections(situation());
    assert_eq!(sections[0].title, DEVICE_CATEGORY);
    assert!(sections[0].is_device && sections[0].is_setting);
    let names: Vec<&str> = sections[0]
        .conditions
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(names, vec!["laptop", "phone"]);
    assert!(sections[1..].iter().all(|section| !section.is_device));
}

#[test]
fn this_device_is_what_the_process_says_it_is() {
    set_this_device_kind(Some(" Tablet ".into()));
    assert_eq!(this_device_kind().as_deref(), Some("tablet"));
    set_this_device_kind(Some("toaster".into()));
    assert_eq!(this_device_kind(), None);
}

fn workspace() -> Connection {
    let mut connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON")
        .unwrap();
    migrate(&mut connection).unwrap();
    connection
        .execute_batch(
            "INSERT INTO workspaces (id, name, createdAt, updatedAt)
             VALUES ('w', 'Mine', '2024-01-01 00:00:00.000', '2024-01-01 00:00:00.000');",
        )
        .unwrap();
    connection
}

fn make(connection: &mut Connection, name: &str, category: Option<&str>) -> String {
    journalled(connection, "New Condition", |tx| {
        create_condition_in_category(tx, "w", name, category, 0)
    })
    .unwrap()
}

fn row(connection: &Connection, id: &str) -> ConditionRow {
    crate::rows::condition(connection, id).unwrap().unwrap()
}

#[test]
fn marking_a_device_takes_it_out_of_the_context_and_what_provides_it() {
    let mut connection = workspace();
    let laptop = make(&mut connection, "Laptop", Some("Location"));
    let home = make(&mut connection, "Home", Some("Location"));
    let quiet = make(&mut connection, "Quiet", None);
    journalled(&mut connection, "Edit Condition", |tx| {
        set_condition_provides(tx, &home, vec![laptop.clone(), quiet.clone()], 1)
    })
    .unwrap();
    connection
        .execute(
            "INSERT INTO preferences (key, value, updatedAt) VALUES ('dayStart.conditionIds', ?1, '2024-01-01 00:00:00.000')",
            [format!("[\"{laptop}\"]")],
        )
        .unwrap();
    {
        let tx = connection.transaction().unwrap();
        save(
            &tx,
            "w",
            &StoredFocusContext {
                condition_ids: vec![laptop.clone(), quiet.clone()],
                ..Default::default()
            },
            1,
        )
        .unwrap();
        tx.commit().unwrap();
    }

    assert!(matches!(
        journalled(&mut connection, "Edit Condition", |tx| {
            set_condition_device(tx, &laptop, Some("watch"), 2)
        }),
        Err(CoreError::InvalidCondition)
    ));
    assert!(
        journalled(&mut connection, "Edit Condition", |tx| {
            set_condition_device(tx, &laptop, Some("Laptop"), 2)
        })
        .unwrap()
    );
    let marked = row(&connection, &laptop);
    assert_eq!(marked.device_kind.as_deref(), Some("laptop"));
    // A device is never a place.
    assert!(!marked.is_location && marked.category.is_none());
    assert_eq!(row(&connection, &home).provides, vec![quiet.clone()]);
    assert_eq!(
        load(&connection, "w").unwrap().unwrap().condition_ids,
        vec![quiet.clone()]
    );
    assert!(
        crate::day_start::settings(&connection)
            .unwrap()
            .condition_ids
            .is_empty()
    );
    // Again is nothing.
    assert!(
        !journalled(&mut connection, "Edit Condition", |tx| {
            set_condition_device(tx, &laptop, Some("laptop"), 3)
        })
        .unwrap()
    );
    // Nothing can be made to provide it, and a context cannot say it holds.
    assert!(matches!(
        journalled(&mut connection, "Edit Condition", |tx| {
            set_condition_provides(tx, &home, vec![laptop.clone()], 3)
        }),
        Err(CoreError::InvalidCondition)
    ));
    {
        let tx = connection.transaction().unwrap();
        save(
            &tx,
            "w",
            &StoredFocusContext {
                condition_ids: vec![laptop.clone(), home.clone()],
                off_ids: vec![laptop.clone()],
                ..Default::default()
            },
            4,
        )
        .unwrap();
        tx.commit().unwrap();
    }
    let context = load(&connection, "w").unwrap().unwrap();
    assert_eq!(context.condition_ids, vec![home.clone()]);
    assert!(context.off_ids.is_empty());

    // The marking undoes, links and all (the context is not an edit).
    assert_eq!(
        undo(&mut connection).unwrap().as_deref(),
        Some("Edit Condition")
    );
    assert_eq!(row(&connection, &laptop).device_kind, None);
    assert_eq!(row(&connection, &home).provides.len(), 2);

    journalled(&mut connection, "Edit Condition", |tx| {
        set_condition_device(tx, &laptop, Some("laptop"), 5)
    })
    .unwrap();
    assert!(
        journalled(&mut connection, "Edit Condition", |tx| {
            set_condition_device(tx, &laptop, None, 6)
        })
        .unwrap()
    );
    assert_eq!(row(&connection, &laptop).device_kind, None);
}
