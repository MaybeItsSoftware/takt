use rusqlite::Connection;

use super::*;
use crate::conditions::{create_condition_in_category, merge_conditions, save_condition};
use crate::focus::{Candidate, FocusContext, Unavailable, reasons};
use crate::focus_context::{StoredFocusContext, load, save};
use crate::journal::{journalled, undo};
use crate::schema::migrate;

fn condition(id: &str, category: Option<&str>, provides: &[&str]) -> ConditionRow {
    ConditionRow {
        id: id.into(),
        workspace_id: "w".into(),
        name: id.into(),
        is_location: category == Some(LOCATION_CATEGORY),
        is_archived: false,
        created_at_ms: 0,
        updated_at_ms: 0,
        category: category.map(Into::into),
        sort_order: None,
        provides: provides.iter().map(|id| id.to_string()).collect(),
        device_kind: None,
    }
}

fn strings(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

/// Home provides Floor space, a Desk and Quiet; the Desk provides Typing;
/// the Laptop provides Typing and a Big screen; the Library is a place
/// that provides Quiet and a Desk.
fn situation() -> Vec<ConditionRow> {
    vec![
        condition("home", Some("Location"), &["floor", "desk", "quiet"]),
        condition("library", Some("Location"), &["quiet", "desk"]),
        condition("gym", Some("Location"), &["floor"]),
        condition("laptop", Some("Tools on hand"), &["typing", "screen"]),
        condition("headphones", Some("Tools on hand"), &[]),
        condition("floor", Some("Space"), &[]),
        condition("desk", Some("Space"), &["typing"]),
        condition("quiet", None, &[]),
        condition("typing", None, &[]),
        condition("screen", None, &[]),
    ]
}

fn held(conditions: &[ConditionRow], set: &[&str], off: &[&str]) -> Vec<String> {
    effective_ids(conditions, &strings(set), &strings(off))
}

#[test]
fn what_a_setting_provides_holds_with_it_through_every_link() {
    let all = situation();
    assert_eq!(
        held(&all, &["home"], &[]),
        strings(&["desk", "floor", "home", "quiet", "typing"])
    );
    let holdings = holdings(&all, &strings(&["home", "laptop"]), &[]);
    let typing = holdings
        .iter()
        .find(|holding| holding.condition_id == "typing")
        .unwrap();
    // Through the Desk, and from the Laptop directly: both settings named,
    // in the tab's order.
    assert_eq!(typing.provided_by, strings(&["home", "laptop"]));
    assert!(typing.holds && !typing.is_set && !typing.is_off);
    let home = holdings.iter().find(|h| h.condition_id == "home").unwrap();
    assert!(home.is_set && home.provided_by.is_empty());
    // Nothing set, nothing provided.
    assert!(held(&all, &[], &[]).is_empty());
}

#[test]
fn a_cycle_of_links_ends() {
    let all = vec![
        condition("home", Some("Location"), &["quiet"]),
        condition("quiet", None, &["calm"]),
        condition("calm", None, &["home", "quiet"]),
    ];
    assert_eq!(
        held(&all, &["home"], &[]),
        strings(&["calm", "home", "quiet"])
    );
    let holdings = holdings(&all, &strings(&["quiet"]), &[]);
    // Quiet is set; Calm comes from it, and so does Home, round the loop,
    // but Quiet is never its own provider.
    let quiet = holdings.iter().find(|h| h.condition_id == "quiet").unwrap();
    assert!(quiet.provided_by.is_empty());
    assert_eq!(holdings.len(), 3);
}

#[test]
fn an_archived_condition_provides_nothing_and_is_provided_by_nothing() {
    let mut all = situation();
    for condition in &mut all {
        if condition.id == "desk" || condition.id == "laptop" {
            condition.is_archived = true;
        }
    }
    // Home no longer reaches Typing through the archived Desk.
    assert_eq!(
        held(&all, &["home"], &[]),
        strings(&["floor", "home", "quiet"])
    );
    // The Laptop said to hold still holds, but provides nothing.
    assert_eq!(held(&all, &["laptop"], &[]), strings(&["laptop"]));
    // A link to a condition that has gone is ignored.
    let gone = vec![condition("home", Some("Location"), &["nowhere"])];
    assert_eq!(held(&gone, &["home"], &[]), strings(&["home"]));
}

#[test]
fn a_provided_condition_switched_off_stays_off_and_stops_what_it_provides() {
    let all = situation();
    let said = ConditionsSaid {
        condition_ids: strings(&["home"]),
        off_ids: vec![],
    };
    let quiet_off = switched(&all, &said, "quiet", None);
    assert_eq!(quiet_off.condition_ids, strings(&["home"]));
    assert_eq!(quiet_off.off_ids, strings(&["quiet"]));
    assert_eq!(
        held(&all, &["home"], &["quiet"]),
        strings(&["desk", "floor", "home", "typing"])
    );
    // Off for now: shown as such, and not holding.
    let quiet = holdings(&all, &quiet_off.condition_ids, &quiet_off.off_ids)
        .into_iter()
        .find(|h| h.condition_id == "quiet")
        .unwrap();
    assert!(quiet.is_off && !quiet.holds);
    assert_eq!(quiet.provided_by, strings(&["home"]));
    // The Desk off takes Typing with it, which only came through it.
    let desk_off = switched(&all, &said, "desk", Some(false));
    assert_eq!(
        held(
            &all,
            &["home"],
            &desk_off
                .off_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        ),
        strings(&["floor", "home", "quiet"])
    );
    // Back on: the override goes, and nothing new is said to hold.
    let back = switched(&all, &quiet_off, "quiet", Some(true));
    assert_eq!(back, said);
    // Something nothing provides is said to hold, and off again.
    let headphones = switched(&all, &said, "headphones", None);
    assert_eq!(headphones.condition_ids, strings(&["headphones", "home"]));
    assert_eq!(switched(&all, &headphones, "headphones", None), said);
    // Said to hold and provided: off is both.
    let both = ConditionsSaid {
        condition_ids: strings(&["home", "quiet"]),
        off_ids: vec![],
    };
    let off = switched(&all, &both, "quiet", Some(false));
    assert_eq!(off.condition_ids, strings(&["home"]));
    assert_eq!(off.off_ids, strings(&["quiet"]));
}

#[test]
fn replacing_or_unsetting_the_provider_resets_its_overrides() {
    let all = situation();
    let at_home = ConditionsSaid {
        condition_ids: strings(&["home", "laptop"]),
        off_ids: strings(&["quiet", "typing"]),
    };
    // Another place replaces Home: Home's overrides go. Typing is still
    // provided by the Laptop, but it was Home's too, so it comes back on.
    let library = switched(&all, &at_home, "library", Some(true));
    assert_eq!(library.condition_ids, strings(&["laptop", "library"]));
    assert!(library.off_ids.is_empty());
    assert_eq!(
        held(&all, &["laptop", "library"], &[]),
        strings(&["desk", "laptop", "library", "quiet", "screen", "typing"])
    );
    // Unsetting the Laptop leaves Home's override on Quiet alone.
    let no_laptop = kept_overrides(
        &all,
        &strings(&["home", "laptop"]),
        &strings(&["home"]),
        &strings(&["quiet", "screen"]),
    );
    assert_eq!(no_laptop, strings(&["quiet"]));
    // An override on something nothing provides is dropped.
    assert!(kept_overrides(&all, &[], &strings(&["gym"]), &strings(&["quiet"])).is_empty());
}

fn candidate(id: &str, groups: Vec<Vec<String>>) -> Candidate {
    Candidate {
        id: id.into(),
        title: id.into(),
        is_daily_due_today: false,
        due_at_ms: None,
        start_at_ms: None,
        matrix_urgency: None,
        matrix_importance: None,
        priority: None,
        estimate_seconds: None,
        kanban_column: None,
        focus_rank: None,
        sort_order: 0,
        created_at_ms: 0,
        due_date: None,
        requirement_groups: groups,
        logged_seconds: 0,
        minimum_block_seconds: None,
        requires_single_sitting: false,
        daily_remaining_seconds: None,
        daily_unavailable: None,
        list_priority: None,
        column_rank: None,
    }
}

#[test]
fn requirements_are_checked_against_what_holds_negation_included() {
    let all = situation();
    let context = |set: &[&str], off: &[&str]| FocusContext {
        condition_ids: held(&all, set, off),
        ends_at_ms: None,
        mode: "progress".into(),
    };
    let stretch = candidate("stretch", vec![strings(&["floor"])]);
    let call = candidate("call", vec![strings(&["!quiet"])]);
    // Floor space at Home or the Gym, without saying so.
    assert!(reasons(&stretch, &context(&["home"], &[]), 0).is_empty());
    assert!(reasons(&stretch, &context(&["gym"], &[]), 0).is_empty());
    assert!(!reasons(&stretch, &context(&["library"], &[]), 0).is_empty());
    // "Not Quiet": Home provides Quiet, so a call waits there…
    assert_eq!(
        reasons(&call, &context(&["home"], &[]), 0),
        vec![Unavailable::MissingConditions {
            groups: vec![strings(&["!quiet"])]
        }]
    );
    // …until Quiet is switched off for now.
    assert!(reasons(&call, &context(&["home"], &["quiet"]), 0).is_empty());
    assert!(reasons(&call, &context(&["gym"], &[]), 0).is_empty());
}

#[test]
fn settings_come_first_and_their_providers_are_offered() {
    let all = situation();
    let titles: Vec<(String, bool)> = condition_sections(all.clone())
        .into_iter()
        .map(|section| (section.title, section.is_setting))
        .collect();
    assert_eq!(
        titles,
        vec![
            ("Location".to_string(), true),
            ("Space".into(), true),
            ("Tools on hand".into(), true),
            ("Other".into(), false),
        ]
    );
    // Only Floor space is required; the places and tools that provide it
    // are offered, the Laptop is not.
    let offered: Vec<String> =
        crate::conditions::available_condition_sections(all, strings(&["floor"]), vec![])
            .into_iter()
            .flat_map(|section| section.conditions)
            .map(|condition| condition.id)
            .collect();
    assert_eq!(offered, strings(&["home", "gym", "floor"]));
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
             VALUES ('w', 'Mine', '2024-01-01 00:00:00.000', '2024-01-01 00:00:00.000'),
                    ('v', 'Other', '2024-01-01 00:00:00.000', '2024-01-01 00:00:00.000');",
        )
        .unwrap();
    connection
}

fn make(
    connection: &mut Connection,
    workspace: &str,
    name: &str,
    category: Option<&str>,
) -> String {
    journalled(connection, "New Condition", |tx| {
        create_condition_in_category(tx, workspace, name, category, 0)
    })
    .unwrap()
}

fn provides(connection: &Connection, id: &str) -> Vec<String> {
    crate::rows::condition(connection, id)
        .unwrap()
        .unwrap()
        .provides
}

fn set(connection: &mut Connection, id: &str, provided: &[&String]) -> Result<bool, CoreError> {
    journalled(connection, "Edit Condition", |tx| {
        set_condition_provides(
            tx,
            id,
            provided.iter().map(|id| id.to_string()).collect(),
            1,
        )
    })
}

#[test]
fn links_are_set_checked_and_undone() {
    let mut connection = workspace();
    let home = make(&mut connection, "w", "Home", Some("Location"));
    let floor = make(&mut connection, "w", "Floor space", None);
    let quiet = make(&mut connection, "w", "Quiet", None);
    let elsewhere = make(&mut connection, "v", "Elsewhere", None);
    assert!(set(&mut connection, &home, &[&quiet, &floor, &quiet]).unwrap());
    let mut expected = vec![floor.clone(), quiet.clone()];
    expected.sort();
    assert_eq!(provides(&connection, &home), expected);
    // The same again writes nothing.
    assert!(!set(&mut connection, &home, &[&floor, &quiet]).unwrap());
    // Not itself, not another workspace's, not one that does not exist.
    for wrong in [&home, &elsewhere, &"nope".to_string()] {
        assert!(matches!(
            set(&mut connection, &home, &[wrong]),
            Err(CoreError::InvalidCondition)
        ));
    }
    // Not a new link to an archived one, though an existing one stays.
    journalled(&mut connection, "Edit Condition", |tx| {
        save_condition(tx, &quiet, "Quiet", false, true, 2)
    })
    .unwrap();
    assert!(!set(&mut connection, &home, &[&floor, &quiet]).unwrap());
    let shed = make(&mut connection, "w", "Shed", None);
    journalled(&mut connection, "Edit Condition", |tx| {
        save_condition(tx, &shed, "Shed", false, true, 2)
    })
    .unwrap();
    assert!(set(&mut connection, &home, &[&shed]).is_err());
    assert!(set(&mut connection, &home, &[]).unwrap());
    assert!(provides(&connection, &home).is_empty());
    let null: Option<String> = connection
        .query_row(
            "SELECT providesJSON FROM task_conditions WHERE id = ?1",
            [&home],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(null, None, "nothing provided is stored as nothing");
    assert_eq!(
        undo(&mut connection).unwrap().as_deref(),
        Some("Edit Condition")
    );
    assert_eq!(provides(&connection, &home), expected);
}

#[test]
fn merging_rewrites_the_links_once_each() {
    let mut connection = workspace();
    let home = make(&mut connection, "w", "Home", Some("Location"));
    let house = make(&mut connection, "w", "House", Some("Location"));
    let quiet = make(&mut connection, "w", "Quiet", None);
    let silence = make(&mut connection, "w", "Silence", None);
    let floor = make(&mut connection, "w", "Floor space", None);
    // Home provides Quiet and Silence; House provides Floor space and Home.
    set(&mut connection, &home, &[&quiet, &silence]).unwrap();
    set(&mut connection, &house, &[&floor, &home]).unwrap();
    journalled(&mut connection, "Merge Conditions", |tx| {
        merge_conditions(tx, &silence, &quiet, 3)
    })
    .unwrap();
    assert_eq!(provides(&connection, &home), vec![quiet.clone()]);
    // House into Home: Home takes Floor space, and not itself.
    journalled(&mut connection, "Merge Conditions", |tx| {
        merge_conditions(tx, &house, &home, 4)
    })
    .unwrap();
    let mut expected = vec![floor.clone(), quiet.clone()];
    expected.sort();
    assert_eq!(provides(&connection, &home), expected);
    // The archived House provides nothing any more, whatever it keeps.
    let all = crate::rows::conditions(&connection, "w").unwrap();
    assert_eq!(
        effective_ids(&all, std::slice::from_ref(&house), &[]),
        vec![house.clone()]
    );
    undo(&mut connection).unwrap();
    assert_eq!(provides(&connection, &home), vec![quiet.clone()]);
}

#[test]
fn saving_a_context_keeps_overrides_only_while_their_provider_holds() {
    let mut connection = workspace();
    let home = make(&mut connection, "w", "Home", Some("Location"));
    let library = make(&mut connection, "w", "Library", Some("Location"));
    let quiet = make(&mut connection, "w", "Quiet", None);
    set(&mut connection, &home, &[&quiet]).unwrap();
    set(&mut connection, &library, &[&quiet]).unwrap();
    let context = |set: &[&String], off: &[&String]| StoredFocusContext {
        condition_ids: set.iter().map(|id| id.to_string()).collect(),
        off_ids: off.iter().map(|id| id.to_string()).collect(),
        ..Default::default()
    };
    let tx = connection.transaction().unwrap();
    assert!(save(&tx, "w", &context(&[&home], &[&quiet]), 1).unwrap());
    assert_eq!(
        load(&tx, "w").unwrap().unwrap().off_ids,
        vec![quiet.clone()]
    );
    // The time changing keeps it.
    let mut longer = context(&[&home], &[&quiet]);
    longer.available_until_ms = Some(9_000);
    assert!(save(&tx, "w", &longer, 2).unwrap());
    assert_eq!(
        load(&tx, "w").unwrap().unwrap().off_ids,
        vec![quiet.clone()]
    );
    // A client that replaces the place but passes the old override along:
    // the Library's Quiet holds, since Home's override went with Home.
    assert!(save(&tx, "w", &context(&[&library], &[&quiet]), 3).unwrap());
    assert!(load(&tx, "w").unwrap().unwrap().off_ids.is_empty());
    // An override on something nothing provides is not kept.
    assert!(save(&tx, "w", &context(&[], &[&quiet]), 4).unwrap());
    assert!(load(&tx, "w").unwrap().unwrap().off_ids.is_empty());
    tx.commit().unwrap();
}
