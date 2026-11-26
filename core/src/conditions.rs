//! Conditions: the places and circumstances a task can require ("Home",
//! "Campus", "Floor space") before it is worth starting, and the categories
//! they are filed under.
//!
//! A category is free text ("Equipment", "Energy") or none. One is special:
//! the conditions in **Location** are places, and you are in one place at a
//! time, so saying one holds replaces the last. That is still carried by the
//! `isLocation` column, which every write here keeps equal to "the category
//! is Location", so an older client that only knows the flag, and sync, go on
//! behaving. A place such a client makes has no category and reads as
//! Location ([`effective_category`]).

use std::collections::{BTreeMap, HashSet};

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::CoreError;
use crate::lists::new_id;
use crate::rows::ConditionRow;
use crate::time::{non_empty_name, stored};

mod device;
mod inherited;
mod organise;
mod provides;
mod requirement;
pub use device::*;
pub use inherited::*;
pub use organise::*;
pub use provides::*;
pub use requirement::*;

/// The category whose conditions are places, and exclusive.
pub const LOCATION_CATEGORY: &str = "Location";

/// The heading conditions without a category are shown under.
pub const UNCATEGORISED_TITLE: &str = "Other";

/// A category as typed, made canonical: trimmed, none when empty, and any
/// spelling of "location" as [`LOCATION_CATEGORY`].
pub fn normalized_category(category: Option<&str>) -> Option<String> {
    let trimmed = category?.trim();
    if trimmed.is_empty() {
        None
    } else if trimmed.eq_ignore_ascii_case(LOCATION_CATEGORY) {
        Some(LOCATION_CATEGORY.to_string())
    } else {
        Some(trimmed.to_string())
    }
}

/// The category a condition is shown under: Device for a device condition,
/// whatever it is filed under; otherwise its own, or Location for a place
/// that has none.
pub fn effective_category(condition: &ConditionRow) -> Option<String> {
    if is_device(condition) {
        return Some(DEVICE_CATEGORY.to_string());
    }
    normalized_category(condition.category.as_deref())
        .or_else(|| condition.is_location.then(|| LOCATION_CATEGORY.to_string()))
}

/// `category` spelled as the workspace already spells it, so "equipment"
/// joins "Equipment" rather than starting a second heading.
fn canonical_category(
    transaction: &Transaction,
    workspace_id: &str,
    category: Option<&str>,
) -> Result<Option<String>, CoreError> {
    let Some(category) = normalized_category(category) else {
        return Ok(None);
    };
    let existing: Option<String> = transaction
        .query_row(
            "SELECT category FROM task_conditions
             WHERE workspaceId = ?1 AND category IS NOT NULL AND lower(trim(category)) = lower(?2)
             ORDER BY createdAt, id LIMIT 1",
            params![workspace_id, category],
            |row| row.get(0),
        )
        .optional()?;
    Ok(Some(
        existing
            .and_then(|existing| normalized_category(Some(&existing)))
            .unwrap_or(category),
    ))
}

fn is_location_category(category: Option<&str>) -> bool {
    category == Some(LOCATION_CATEGORY)
}

/// Creates a condition in a workspace and returns its id: a place when
/// `is_location`, filed under Location, otherwise uncategorised.
pub fn create_condition(
    transaction: &Transaction,
    workspace_id: &str,
    name: &str,
    is_location: bool,
    now_ms: i64,
) -> Result<String, CoreError> {
    create_condition_in_category(
        transaction,
        workspace_id,
        name,
        is_location.then_some(LOCATION_CATEGORY),
        now_ms,
    )
}

/// Creates a condition under `category` (none for uncategorised) and returns
/// its id. Location makes it a place.
pub fn create_condition_in_category(
    transaction: &Transaction,
    workspace_id: &str,
    name: &str,
    category: Option<&str>,
    now_ms: i64,
) -> Result<String, CoreError> {
    let name = non_empty_name(name)?;
    let workspace_exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = ?1)",
        [workspace_id],
        |row| row.get(0),
    )?;
    if !workspace_exists {
        return Err(CoreError::InvalidCondition);
    }
    // A live condition already called this is the one meant: "home" when
    // there is a Home should not make a second.
    let existing: Option<String> = transaction
        .query_row(
            "SELECT id FROM task_conditions
             WHERE workspaceId = ?1 AND isArchived = 0 AND lower(trim(name)) = lower(?2)
             ORDER BY createdAt, id LIMIT 1",
            params![workspace_id, name],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(existing) = existing {
        return Ok(existing);
    }
    let category = canonical_category(transaction, workspace_id, category)?;
    let id = new_id();
    let now = stored(now_ms);
    transaction.execute(
        "INSERT INTO task_conditions (id, workspaceId, name, isLocation, isArchived, createdAt, updatedAt, category)
         VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5, ?6)",
        params![
            id,
            workspace_id,
            name,
            is_location_category(category.as_deref()),
            now,
            category
        ],
    )?;
    Ok(id)
}

/// Saves a condition's name, whether it is a place, and whether it is
/// archived. Making it a place files it under Location; making a Location
/// condition not a place leaves it uncategorised; any other category stays.
/// Saving what is already there records nothing.
pub fn save_condition(
    transaction: &Transaction,
    id: &str,
    name: &str,
    is_location: bool,
    is_archived: bool,
    now_ms: i64,
) -> Result<(), CoreError> {
    let name = non_empty_name(name)?;
    let current: (String, bool, bool, Option<String>) = transaction
        .query_row(
            "SELECT name, isLocation, isArchived, category FROM task_conditions WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?
        .ok_or(CoreError::InvalidCondition)?;
    let category = if is_location == current.1 {
        current.3.clone()
    } else if is_location {
        Some(LOCATION_CATEGORY.to_string())
    } else if is_location_category(normalized_category(current.3.as_deref()).as_deref()) {
        None
    } else {
        current.3.clone()
    };
    if current == (name.clone(), is_location, is_archived, category.clone()) {
        return Ok(());
    }
    transaction.execute(
        "UPDATE task_conditions SET name = ?1, isLocation = ?2, isArchived = ?3, category = ?4, updatedAt = ?5
         WHERE id = ?6",
        params![name, is_location, is_archived, category, stored(now_ms), id],
    )?;
    Ok(())
}

/// Files a condition under `category`, or under none. Location makes it a
/// place and anything else makes it not one. A category the workspace
/// already has is spelled as it has it. Filing it where it is records
/// nothing.
pub fn set_condition_category(
    transaction: &Transaction,
    id: &str,
    category: Option<&str>,
    now_ms: i64,
) -> Result<(), CoreError> {
    let (workspace_id, current, is_location): (String, Option<String>, bool) = transaction
        .query_row(
            "SELECT workspaceId, category, isLocation FROM task_conditions WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or(CoreError::InvalidCondition)?;
    let category = canonical_category(transaction, &workspace_id, category)?;
    let place = is_location_category(category.as_deref());
    if current == category && is_location == place {
        return Ok(());
    }
    // Under a new heading it goes last, rather than wherever its old number
    // would put it; the heading it left keeps its place.
    transaction.execute(
        "UPDATE task_conditions SET category = ?1, isLocation = ?2, sortOrder = NULL, updatedAt = ?3
         WHERE id = ?4",
        params![category, place, stored(now_ms), id],
    )?;
    Ok(())
}

/// The ids of the conditions any open task in the workspace requires, either
/// way (a negated requirement names its condition too): not completed, not
/// archived, and in a list that is not archived. What a live list requires
/// counts, as its tasks inherit it.
pub fn required_condition_ids(
    connection: &Connection,
    workspace_id: &str,
) -> Result<Vec<String>, CoreError> {
    let mut statement = connection.prepare(
        "SELECT m.planningJSON FROM task_metadata m
         JOIN tasks t ON t.id = m.taskId
         JOIN task_lists l ON l.id = t.listId
         WHERE l.workspaceId = ?1 AND l.isArchived = 0
           AND t.status = 'open' AND t.archivedAt IS NULL
           AND m.planningJSON IS NOT NULL",
    )?;
    let blobs = statement
        .query_map([workspace_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut ids = std::collections::BTreeSet::new();
    for json in blobs {
        // One unreadable plan should not hide every condition.
        let Ok(planning) = crate::planning::Planning::from_json(&json) else {
            continue;
        };
        for group in planning.requirement_groups.unwrap_or_default() {
            ids.extend(
                group
                    .iter()
                    .map(|member| condition_id_of(member).to_string()),
            );
        }
    }
    // What a list requires, its open tasks require.
    for groups in list_requirements(connection, workspace_id)?.into_values() {
        ids.extend(
            groups
                .iter()
                .flatten()
                .map(|member| condition_id_of(member).to_string()),
        );
    }
    Ok(ids.into_iter().collect())
}

/// Conditions under one heading.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ConditionSection {
    /// The category, or none for the uncategorised.
    pub category: Option<String>,
    /// What the heading says: the category, or "Other".
    pub title: String,
    /// Whether saying one holds replaces the rest: the Location heading.
    pub is_exclusive: bool,
    /// Whether it is a setting — something you say about your situation,
    /// which switches on what a task needs — rather than properties: the
    /// Location heading, and any heading a condition under which provides
    /// something ("Tools on hand"). Settings come first.
    pub is_setting: bool,
    /// Whether it is the Device heading: device conditions, which hold on a
    /// device of their kind by themselves and are never switched by hand.
    /// It comes first of all.
    #[uniffi(default = false)]
    pub is_device: bool,
    /// In the order they were given.
    pub conditions: Vec<ConditionRow>,
}

/// Conditions under their headings, settings first (Location, and any
/// heading whose conditions provide something), then the rest, each half in
/// the order the workspace keeps (`sortOrder`): the headings by their first
/// placed condition, and the conditions under each by their own. What nobody
/// has placed — every condition before v27, and one an older client makes —
/// comes after what has been, in the order it was given, and a heading with
/// nothing placed falls back to Location first, then every other category
/// alphabetically, then the uncategorised under "Other".
#[uniffi::export]
pub fn condition_sections(conditions: Vec<ConditionRow>) -> Vec<ConditionSection> {
    let mut named: BTreeMap<String, (String, Vec<ConditionRow>)> = BTreeMap::new();
    let mut other = Vec::new();
    for condition in conditions {
        match effective_category(&condition) {
            Some(category) => {
                named
                    .entry(category.to_lowercase())
                    .or_insert_with(|| (category, Vec::new()))
                    .1
                    .push(condition);
            }
            None => other.push(condition),
        }
    }
    let mut sections: Vec<ConditionSection> = named
        .into_values()
        .map(|(category, conditions)| ConditionSection {
            is_exclusive: category == LOCATION_CATEGORY,
            is_setting: conditions.iter().any(is_setting),
            is_device: conditions.iter().any(is_device),
            title: category.clone(),
            category: Some(category),
            conditions,
        })
        .collect();
    if !other.is_empty() {
        sections.push(ConditionSection {
            category: None,
            title: UNCATEGORISED_TITLE.to_string(),
            is_exclusive: false,
            is_setting: other.iter().any(is_setting),
            is_device: false,
            conditions: other,
        });
    }
    for section in &mut sections {
        // Stable, so the unplaced keep the order they were given.
        section
            .conditions
            .sort_by_key(|condition| condition.sort_order.unwrap_or(i64::MAX));
    }
    sections.sort_by_cached_key(|section| {
        let placed = section
            .conditions
            .iter()
            .filter_map(|condition| condition.sort_order)
            .min()
            .unwrap_or(i64::MAX);
        let fallback = match section.category.as_deref() {
            Some(LOCATION_CATEGORY) => 0u8,
            Some(_) => 1,
            None => 2,
        };
        (
            !section.is_device,
            !section.is_setting,
            placed,
            fallback,
            section.title.to_lowercase(),
        )
    });
    sections
}

/// What "Available now" offers: only the conditions an open task requires
/// (`required_ids`), and the settings that provide one, since saying
/// anything else holds changes nothing, plus any that is on now
/// (`active_ids`: what holds, provided ones included, and any switched off
/// for now), so it can be turned off or back on. Under their headings, with
/// a heading left out when nothing under it is offered. Empty when no open
/// task needs a condition and none is on.
#[uniffi::export]
pub fn available_condition_sections(
    conditions: Vec<ConditionRow>,
    required_ids: Vec<String>,
    active_ids: Vec<String>,
) -> Vec<ConditionSection> {
    let required: HashSet<String> = required_ids.into_iter().collect();
    let providers = conditions
        .iter()
        .filter(|condition| {
            condition_holdings(conditions.clone(), vec![condition.id.clone()], Vec::new())
                .iter()
                .any(|holding| {
                    holding.condition_id != condition.id && required.contains(&holding.condition_id)
                })
        })
        .map(|condition| condition.id.clone())
        .collect::<Vec<_>>();
    let offered: HashSet<String> = required
        .into_iter()
        .chain(active_ids)
        .chain(providers)
        .collect();
    condition_sections(
        conditions
            .into_iter()
            .filter(|condition| offered.contains(&condition.id))
            .collect(),
    )
}

/// The categories a condition can be filed under, for a finder: Location
/// always, then every other category the conditions use, alphabetically and
/// once each however it is cased.
#[uniffi::export]
pub fn condition_categories(conditions: Vec<ConditionRow>) -> Vec<String> {
    let mut named: BTreeMap<String, String> = BTreeMap::new();
    // Device is where a device kind files a condition, not a category to
    // choose.
    for category in conditions.iter().filter_map(effective_category) {
        if category != LOCATION_CATEGORY && category != DEVICE_CATEGORY {
            named.entry(category.to_lowercase()).or_insert(category);
        }
    }
    std::iter::once(LOCATION_CATEGORY.to_string())
        .chain(named.into_values())
        .collect()
}

/// The heading a condition is shown under: its category, Location for a
/// place filed under none, or "Other".
#[uniffi::export]
pub fn condition_section_title(condition: ConditionRow) -> String {
    effective_category(&condition).unwrap_or_else(|| UNCATEGORISED_TITLE.to_string())
}

/// A typed category made canonical — trimmed, none when empty, any spelling
/// of "location" as "Location" — for a finder deciding whether what was typed
/// is new.
#[uniffi::export]
pub fn condition_category_normalized(category: Option<String>) -> Option<String> {
    normalized_category(category.as_deref())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::*;
    use crate::journal::{journalled, undo};
    use crate::schema::migrate;

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

    fn row(connection: &Connection, id: &str) -> (String, bool, bool) {
        connection
            .query_row(
                "SELECT name, isLocation, isArchived FROM task_conditions WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap()
    }

    #[test]
    fn a_condition_is_created_saved_and_undone() {
        let mut connection = workspace();
        let id = journalled(&mut connection, "New Condition", |tx| {
            create_condition(tx, "w", " Library ", true, 1_700_000_000_000)
        })
        .unwrap();
        assert_eq!(row(&connection, &id), ("Library".into(), true, false));

        journalled(&mut connection, "Edit Condition", |tx| {
            save_condition(tx, &id, "Quiet room", false, true, 1_700_000_000_000)
        })
        .unwrap();
        assert_eq!(row(&connection, &id), ("Quiet room".into(), false, true));
        let steps = |c: &Connection| -> i64 {
            c.query_row("SELECT COUNT(DISTINCT groupId) FROM change_log", [], |r| {
                r.get(0)
            })
            .unwrap()
        };
        let before = steps(&connection);
        journalled(&mut connection, "Edit Condition", |tx| {
            save_condition(tx, &id, "Quiet room", false, true, 1_700_000_000_000)
        })
        .unwrap();
        assert_eq!(steps(&connection), before);

        assert_eq!(
            undo(&mut connection).unwrap().as_deref(),
            Some("Edit Condition")
        );
        assert_eq!(row(&connection, &id), ("Library".into(), true, false));
    }

    #[test]
    fn a_condition_needs_a_workspace_a_name_and_to_exist() {
        let mut connection = workspace();
        assert!(matches!(
            journalled(&mut connection, "New Condition", |tx| create_condition(
                tx, "nope", "X", false, 0
            )),
            Err(CoreError::InvalidCondition)
        ));
        assert!(matches!(
            journalled(&mut connection, "New Condition", |tx| create_condition(
                tx, "w", "  ", false, 0
            )),
            Err(CoreError::EmptyName)
        ));
        assert!(matches!(
            journalled(&mut connection, "Edit Condition", |tx| save_condition(
                tx, "nope", "X", false, false, 0
            )),
            Err(CoreError::InvalidCondition)
        ));
    }

    fn category(connection: &Connection, id: &str) -> (Option<String>, bool) {
        connection
            .query_row(
                "SELECT category, isLocation FROM task_conditions WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap()
    }

    #[test]
    fn a_category_is_kept_with_the_place_flag() {
        let mut connection = workspace();
        let gym = journalled(&mut connection, "New Condition", |tx| {
            create_condition_in_category(tx, "w", "Gym", Some(" location "), 0)
        })
        .unwrap();
        assert_eq!(category(&connection, &gym), (Some("Location".into()), true));

        let kettlebell = journalled(&mut connection, "New Condition", |tx| {
            create_condition_in_category(tx, "w", "Kettlebell", Some("Equipment"), 0)
        })
        .unwrap();
        // A second spelling joins the first heading.
        let mat = journalled(&mut connection, "New Condition", |tx| {
            create_condition_in_category(tx, "w", "Mat", Some("equipment"), 0)
        })
        .unwrap();
        assert_eq!(
            category(&connection, &kettlebell),
            (Some("Equipment".into()), false)
        );
        assert_eq!(
            category(&connection, &mat),
            (Some("Equipment".into()), false)
        );

        journalled(&mut connection, "Edit Condition", |tx| {
            set_condition_category(tx, &gym, Some("Equipment"), 1)
        })
        .unwrap();
        assert_eq!(
            category(&connection, &gym),
            (Some("Equipment".into()), false)
        );
        journalled(&mut connection, "Edit Condition", |tx| {
            set_condition_category(tx, &mat, Some("Location"), 1)
        })
        .unwrap();
        assert_eq!(category(&connection, &mat), (Some("Location".into()), true));
        journalled(&mut connection, "Edit Condition", |tx| {
            set_condition_category(tx, &mat, Some("  "), 1)
        })
        .unwrap();
        assert_eq!(category(&connection, &mat), (None, false));
        assert_eq!(
            undo(&mut connection).unwrap().as_deref(),
            Some("Edit Condition")
        );
        assert_eq!(category(&connection, &mat), (Some("Location".into()), true));

        // The old flag-only save files a place under Location and back out.
        journalled(&mut connection, "Edit Condition", |tx| {
            save_condition(tx, &kettlebell, "Kettlebell", true, false, 2)
        })
        .unwrap();
        assert_eq!(
            category(&connection, &kettlebell),
            (Some("Location".into()), true)
        );
        journalled(&mut connection, "Edit Condition", |tx| {
            save_condition(tx, &kettlebell, "Kettlebell", false, false, 2)
        })
        .unwrap();
        assert_eq!(category(&connection, &kettlebell), (None, false));
        // A rename leaves another category alone.
        journalled(&mut connection, "Edit Condition", |tx| {
            save_condition(tx, &gym, "The gym", false, false, 3)
        })
        .unwrap();
        assert_eq!(
            category(&connection, &gym),
            (Some("Equipment".into()), false)
        );
        assert!(matches!(
            journalled(&mut connection, "Edit Condition", |tx| {
                set_condition_category(tx, "nope", None, 0)
            }),
            Err(CoreError::InvalidCondition)
        ));
    }

    fn condition(id: &str, category: Option<&str>, is_location: bool) -> ConditionRow {
        ConditionRow {
            id: id.into(),
            workspace_id: "w".into(),
            name: id.into(),
            is_location,
            is_archived: false,
            created_at_ms: 0,
            updated_at_ms: 0,
            category: category.map(Into::into),
            sort_order: None,
            provides: Vec::new(),
            device_kind: None,
        }
    }

    fn titles(sections: &[ConditionSection]) -> Vec<(String, Vec<String>, bool)> {
        sections
            .iter()
            .map(|section| {
                (
                    section.title.clone(),
                    section.conditions.iter().map(|c| c.id.clone()).collect(),
                    section.is_exclusive,
                )
            })
            .collect()
    }

    #[test]
    fn conditions_are_grouped_location_first_and_other_last() {
        let all = vec![
            condition("quiet", None, false),
            condition("mat", Some("equipment"), false),
            condition("home", None, true),
            condition("campus", Some("Location"), true),
            condition("tired", Some("Energy"), false),
            condition("bell", Some("Equipment"), false),
        ];
        assert_eq!(
            titles(&condition_sections(all.clone())),
            vec![
                (
                    "Location".into(),
                    vec!["home".into(), "campus".into()],
                    true
                ),
                ("Energy".into(), vec!["tired".into()], false),
                ("equipment".into(), vec!["mat".into(), "bell".into()], false),
                ("Other".into(), vec!["quiet".into()], false),
            ]
        );
        assert_eq!(
            condition_categories(all.clone()),
            vec!["Location".to_string(), "Energy".into(), "equipment".into()]
        );
        assert_eq!(condition_categories(vec![]), vec!["Location".to_string()]);

        // Only what an open task needs, and what is on.
        assert_eq!(
            titles(&available_condition_sections(
                all.clone(),
                vec!["home".into(), "bell".into()],
                vec!["quiet".into()]
            )),
            vec![
                ("Location".into(), vec!["home".into()], true),
                ("Equipment".into(), vec!["bell".into()], false),
                ("Other".into(), vec!["quiet".into()], false),
            ]
        );
        assert!(available_condition_sections(all, vec![], vec![]).is_empty());
        assert_eq!(
            condition_category_normalized(Some(" LOCATION ".into())),
            Some("Location".into())
        );
        assert_eq!(condition_category_normalized(Some(" ".into())), None);
    }

    #[test]
    fn only_open_tasks_make_a_condition_required() {
        let connection = workspace();
        connection
            .execute_batch(
                "INSERT INTO task_lists (id, workspaceId, name, sortOrder, createdAt, updatedAt)
                   VALUES ('l', 'w', 'Inbox', 0, '2024-01-01 00:00:00.000', '2024-01-01 00:00:00.000'),
                          ('gone', 'w', 'Old', 1, '2024-01-01 00:00:00.000', '2024-01-01 00:00:00.000');
                 UPDATE task_lists SET isArchived = 1 WHERE id = 'gone';
                 INSERT INTO tasks (id, listId, title, status, sortOrder, createdAt, updatedAt, archivedAt) VALUES
                   ('open', 'l', 'Open', 'open', 0, '2024-01-01 00:00:00.000', '2024-01-01 00:00:00.000', NULL),
                   ('done', 'l', 'Done', 'completed', 1, '2024-01-01 00:00:00.000', '2024-01-01 00:00:00.000', NULL),
                   ('shelved', 'l', 'Shelved', 'open', 2, '2024-01-01 00:00:00.000', '2024-01-01 00:00:00.000', '2024-01-01 00:00:00.000'),
                   ('old', 'gone', 'Old', 'open', 0, '2024-01-01 00:00:00.000', '2024-01-01 00:00:00.000', NULL),
                   ('broken', 'l', 'Broken', 'open', 3, '2024-01-01 00:00:00.000', '2024-01-01 00:00:00.000', NULL);
                 INSERT INTO task_metadata (taskId, planningJSON, updatedAt) VALUES
                   ('open', '{\"requirementGroups\":[[\"home\",\"campus\"],[\"!mat\"]]}', '2024-01-01 00:00:00.000'),
                   ('done', '{\"requirementGroups\":[[\"desk\"]]}', '2024-01-01 00:00:00.000'),
                   ('shelved', '{\"requirementGroups\":[[\"desk\"]]}', '2024-01-01 00:00:00.000'),
                   ('old', '{\"requirementGroups\":[[\"desk\"]]}', '2024-01-01 00:00:00.000'),
                   ('broken', '{\"requirementGroups\":7}', '2024-01-01 00:00:00.000');",
            )
            .unwrap();
        // Requiring Mat not to hold still makes Mat worth saying holds.
        assert_eq!(
            required_condition_ids(&connection, "w").unwrap(),
            vec!["campus".to_string(), "home".into(), "mat".into()]
        );
        assert!(
            required_condition_ids(&connection, "elsewhere")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_place_filed_under_nothing_is_shown_under_location() {
        let place = ConditionRow {
            id: "h".into(),
            workspace_id: "w".into(),
            name: "Home".into(),
            is_location: true,
            is_archived: false,
            created_at_ms: 0,
            updated_at_ms: 0,
            category: None,
            sort_order: None,
            provides: Vec::new(),
            device_kind: None,
        };
        assert_eq!(condition_section_title(place.clone()), "Location");
        let loose = ConditionRow {
            is_location: false,
            ..place.clone()
        };
        assert_eq!(condition_section_title(loose), "Other");
        let filed = ConditionRow {
            is_location: false,
            category: Some(" Energy ".into()),
            ..place
        };
        assert_eq!(condition_section_title(filed), "Energy");
    }
}
