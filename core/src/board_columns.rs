//! A board's columns: which ones a board draws, the column each task stands
//! in on it, and the edits to them.
//!
//! **Layouts.** `kanban_boards` holds one layout per board key: a list's
//! board is `<list>/root`, a board entered on a task `<list>/<task>`,
//! Everything's `everything/root`, a folder's `folder:<id>/root`, and the
//! workspace's default columns `default`. A board with no layout of its own
//! uses, in turn, its list's (for a board entered on a task), the default,
//! then the stock five ([`board_stock_columns`]). So a new list, or one whose
//! columns were reset, follows the default, and editing a board's columns
//! gives it a layout of its own from then on.
//!
//! **Rule columns.** A column can carry a [`FilterDefinition`], the same
//! rules a filter list has. A task the rules match is drawn in it — the
//! first such column, left to right, wins — whatever column it is filed in;
//! one no rule column matches stays in the column it is filed in. A rule
//! column's `show` is not read: membership is by its rules alone, so a
//! finished task stays where an open one would be. A column whose rules are
//! empty is an ordinary column. Dropping a task into a rule column edits the
//! task until the rules hold, where that is unambiguous
//! ([`move_to_board_column`]); otherwise it is refused.
//!
//! **Entered tasks.** A board entered on a task cannot hold a child at an
//! earlier stage than the task itself: the columns to the left of the one
//! the task stands in on its parent's board are hidden, and a child filed in
//! one of them is drawn in the task's column. Nothing is rewritten; leaving
//! the task, or moving it back, shows everything where it was filed. Waiting
//! on and rule columns are not stages, so a task standing in one hides
//! nothing, and rule columns are never hidden. Waiting on to the left is
//! hidden too unless a task beneath is filed in it, so nothing waiting is
//! drawn as though it were at the task's stage.
//!
//! **Built in.** `today` and `waiting-on` are ids other features read (the
//! Today view plans by the first, waiting follow-ups by the second). They can
//! be renamed, moved and deleted from a board like any column; deleting one
//! moves nothing, because a task filed there is planned or waiting, not
//! merely placed, and stays so.

use std::collections::HashSet;

use chrono::{Duration, NaiveDate};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::CoreError;
use crate::editor;
use crate::filter_lists::{
    self, Context, FactScope, Facts, FilterDefinition, FilterRule, filter_definition_json,
};
use crate::journal;
use crate::workspace::CoreWorkspace;

/// The key of the workspace's default columns.
pub const DEFAULT_BOARD_KEY: &str = "default";
/// Everything's board.
pub const EVERYTHING_BOARD_KEY: &str = "everything/root";
/// The column waiting follow-ups read.
pub const WAITING_COLUMN_ID: &str = "waiting-on";
/// The column Today plans by.
pub const TODAY_COLUMN_ID: &str = "today";
/// The column a task starting later this week moves to from Today.
pub const THIS_WEEK_COLUMN_ID: &str = "this-week";
/// Where a task filed nowhere stands.
pub const FIRST_STOCK_COLUMN_ID: &str = "backlog";

/// One column of a board.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BoardColumn {
    pub id: String,
    pub title: String,
    /// The rules that fill it, for a rule column; `None` for one tasks are
    /// filed in.
    #[uniffi(default = None)]
    pub rules: Option<FilterDefinition>,
}

/// A task drawn somewhere other than its filed column says: by a rule
/// column, or clamped into an entered task's column.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BoardColumnPlacement {
    pub task_id: String,
    pub column_id: String,
}

/// A board's columns as a client draws them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BoardColumns {
    pub key: String,
    /// The layout, as editing it saves: hidden columns included, columns
    /// drawn only for the cards filed in them not.
    pub layout: Vec<BoardColumn>,
    /// Where the layout comes from: "own", "list" (an entered task's board
    /// using its list's), "default" or "builtin".
    pub source: String,
    /// The columns drawn, left to right: the layout less the hidden ones,
    /// then any column a card here is filed in that the layout lacks.
    pub columns: Vec<BoardColumn>,
    /// Columns to the left of an entered task's own, hidden.
    pub hidden_column_ids: Vec<String>,
    /// The entered task's column, when it hides any.
    pub floor_column_id: Option<String>,
    /// Every task of the board's lists drawn somewhere other than where a
    /// client would put it by its filed column alone — that column if it is
    /// drawn, else the first drawn one.
    pub placements: Vec<BoardColumnPlacement>,
}

fn stock() -> Vec<BoardColumn> {
    [
        ("backlog", "Backlog"),
        ("in-progress", "In progress"),
        (THIS_WEEK_COLUMN_ID, "This week"),
        (WAITING_COLUMN_ID, "Waiting on"),
        (TODAY_COLUMN_ID, "Today"),
    ]
    .iter()
    .map(|(id, title)| BoardColumn {
        id: (*id).into(),
        title: (*title).into(),
        rules: None,
    })
    .collect()
}

/// The stock columns, which a workspace with no default of its own uses.
#[uniffi::export]
pub fn board_stock_columns() -> Vec<BoardColumn> {
    stock()
}

/// Columns as `kanban_boards.columnsJSON` holds them: id then title, as
/// Swift's `JSONEncoder` wrote them, `/` escaped, and a rule column's rules.
pub(crate) fn columns_json(columns: &[BoardColumn]) -> String {
    let items: Vec<String> = columns
        .iter()
        .map(|column| {
            let mut item = format!(
                "{{\"id\":{},\"title\":{}",
                serde_json::Value::from(column.id.clone()),
                serde_json::Value::from(column.title.clone())
            );
            if let Some(rules) = &column.rules {
                item.push_str(",\"rules\":");
                item.push_str(&filter_definition_json(rules.clone()));
            }
            item.push('}');
            item
        })
        .collect();
    format!("[{}]", items.join(",")).replace('/', "\\/")
}

/// A stored layout, or `None` for one that is empty or is not a layout.
/// Rules that do not read leave the column an ordinary one.
pub(crate) fn parse_columns(json: &str) -> Option<Vec<BoardColumn>> {
    let values: Vec<serde_json::Value> = serde_json::from_str(json).ok()?;
    if values.is_empty() {
        return None;
    }
    values
        .iter()
        .map(|value| {
            Some(BoardColumn {
                id: value.get("id")?.as_str()?.to_string(),
                title: value.get("title")?.as_str()?.to_string(),
                rules: value
                    .get("rules")
                    .filter(|rules| rules.is_object())
                    .and_then(|rules| serde_json::from_value(rules.clone()).ok())
                    .filter(|rules: &FilterDefinition| !rules.rules.is_empty()),
            })
        })
        .collect()
}

/// A layout's columns read from its stored JSON; empty if it is not one.
#[uniffi::export]
pub fn parse_board_columns(json: String) -> Vec<BoardColumn> {
    parse_columns(&json).unwrap_or_default()
}

fn stored_layout(
    connection: &Connection,
    key: &str,
) -> Result<Option<Vec<BoardColumn>>, CoreError> {
    let json: Option<String> = connection
        .query_row(
            "SELECT columnsJSON FROM kanban_boards WHERE id = ?1",
            [key],
            |row| row.get(0),
        )
        .optional()?;
    Ok(json.and_then(|json| parse_columns(&json)))
}

/// A list board's key split into its list and its scope (`root`, or the
/// task it is entered on). Not Everything's, a folder's or the default.
fn scoped(key: &str) -> Option<(&str, &str)> {
    if key == EVERYTHING_BOARD_KEY || key == DEFAULT_BOARD_KEY || key.starts_with("folder:") {
        return None;
    }
    key.rsplit_once('/')
}

/// The layout a board uses, and where it comes from.
pub(crate) fn layout(
    connection: &Connection,
    key: &str,
) -> Result<(Vec<BoardColumn>, &'static str), CoreError> {
    if let Some(columns) = stored_layout(connection, key)? {
        return Ok((columns, "own"));
    }
    if let Some((list, scope)) = scoped(key)
        && scope != "root"
        && let Some(columns) = stored_layout(connection, &format!("{list}/root"))?
    {
        return Ok((columns, "list"));
    }
    if key != DEFAULT_BOARD_KEY
        && let Some(columns) = stored_layout(connection, DEFAULT_BOARD_KEY)?
    {
        return Ok((columns, "default"));
    }
    Ok((stock(), "builtin"))
}

/// A column that is a stage of work, so an entered task's position hides
/// the ones before it: not Waiting on, not a rule column.
pub(crate) fn is_stage(column: &BoardColumn) -> bool {
    column.rules.is_none() && column.id != WAITING_COLUMN_ID
}

/// A board's layout with what an entered task hides.
struct Resolved {
    layout: Vec<BoardColumn>,
    source: &'static str,
    hidden: HashSet<String>,
    floor: Option<String>,
}

impl Resolved {
    fn visible(&self) -> Vec<BoardColumn> {
        self.layout
            .iter()
            .filter(|column| !self.hidden.contains(&column.id))
            .cloned()
            .collect()
    }
}

/// How deep a chain of entered tasks is followed for its floors.
const MAX_DEPTH: usize = 32;

fn resolve(
    connection: &Connection,
    key: &str,
    context: Option<&Context>,
    depth: usize,
) -> Result<Resolved, CoreError> {
    let (layout, source) = layout(connection, key)?;
    let mut hidden = HashSet::new();
    let mut floor = None;
    if let Some(context) = context
        && depth < MAX_DEPTH
        && let Some(parent_column) = scope_column(connection, key, context, depth)?
        && let Some(index) = layout
            .iter()
            .position(|column| column.id == parent_column && is_stage(column))
    {
        let waiting_used = layout[..index]
            .iter()
            .any(|column| column.id == WAITING_COLUMN_ID)
            && waits_beneath(connection, key)?;
        hidden = layout[..index]
            .iter()
            .filter(|column| is_stage(column) || (column.id == WAITING_COLUMN_ID && !waiting_used))
            .map(|column| column.id.clone())
            .collect();
        if !hidden.is_empty() {
            floor = Some(parent_column);
        }
    }
    Ok(Resolved {
        layout,
        source,
        hidden,
        floor,
    })
}

/// Whether an open task beneath an entered board's task is filed in Waiting
/// on, which keeps that column drawn to the left of the task's own.
fn waits_beneath(connection: &Connection, key: &str) -> Result<bool, CoreError> {
    let Some((_, scope)) = scoped(key) else {
        return Ok(false);
    };
    Ok(connection.query_row(
        "WITH RECURSIVE subtree(id) AS (
           SELECT id FROM tasks WHERE parentTaskId = ?1
           UNION
           SELECT tasks.id FROM tasks JOIN subtree ON tasks.parentTaskId = subtree.id
         )
         SELECT EXISTS(SELECT 1 FROM subtree JOIN tasks t ON t.id = subtree.id
                       JOIN task_metadata m ON m.taskId = t.id
                       WHERE t.status = 'open' AND m.kanbanColumn = ?2)",
        params![scope, WAITING_COLUMN_ID],
        |row| row.get(0),
    )?)
}

/// The column an entered board's task stands in on its parent's board: the
/// board entered on its parent task, or its list's.
fn scope_column(
    connection: &Connection,
    key: &str,
    context: &Context,
    depth: usize,
) -> Result<Option<String>, CoreError> {
    let Some((_, scope)) = scoped(key) else {
        return Ok(None);
    };
    if scope == "root" {
        return Ok(None);
    }
    let Some(task) = filter_lists::facts_in(connection, "all", FactScope::Task(scope))?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    let visible_root: Option<String> = connection
        .query_row(
            "SELECT visibleRootTaskId FROM task_lists WHERE id = ?1",
            [&task.list_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    let parent_key = match &task.parent_id {
        Some(parent) if Some(parent) != visible_root.as_ref() => {
            format!("{}/{parent}", task.list_id)
        }
        _ => format!("{}/root", task.list_id),
    };
    let parent = resolve(connection, &parent_key, Some(context), depth + 1)?;
    Ok(Some(drawn(
        &task,
        &parent.visible(),
        &parent,
        Some(context),
    )))
}

/// Whether a rule column's rules hold for `task`. Its `show` is not read,
/// and rules this build does not know are skipped; with none left it holds
/// for nothing.
fn rules_hold(definition: &FilterDefinition, task: &Facts, context: &Context) -> bool {
    let mut known = definition
        .rules
        .iter()
        .filter_map(|rule| filter_lists::holds(rule, task, context))
        .peekable();
    if known.peek().is_none() {
        return false;
    }
    if definition.match_mode == "any" {
        known.any(|held| held)
    } else {
        known.all(|held| held)
    }
}

/// The first rule column of `columns` whose rules hold for `task`.
fn rule_column<'a>(
    task: &Facts,
    columns: &'a [BoardColumn],
    context: Option<&Context>,
) -> Option<&'a BoardColumn> {
    let context = context?;
    columns.iter().find(|column| {
        column
            .rules
            .as_ref()
            .is_some_and(|rules| rules_hold(rules, task, context))
    })
}

/// The column `task` is drawn in among `columns`.
fn drawn(
    task: &Facts,
    columns: &[BoardColumn],
    resolved: &Resolved,
    context: Option<&Context>,
) -> String {
    if let Some(column) = rule_column(task, columns, context) {
        return column.id.clone();
    }
    let wanted = task.column.as_deref().unwrap_or(FIRST_STOCK_COLUMN_ID);
    if columns
        .iter()
        .any(|column| column.id == wanted && column.rules.is_none())
    {
        return wanted.to_string();
    }
    if resolved.hidden.contains(wanted) {
        return resolved.floor.clone().unwrap_or_else(|| wanted.to_string());
    }
    resolved
        .floor
        .clone()
        .or_else(|| {
            columns
                .iter()
                .find(|column| column.rules.is_none())
                .map(|column| column.id.clone())
        })
        .or_else(|| columns.first().map(|column| column.id.clone()))
        .unwrap_or_else(|| wanted.to_string())
}

/// The tasks below `id`, at any depth.
fn descendants(connection: &Connection, id: &str) -> Result<HashSet<String>, CoreError> {
    let mut statement = connection.prepare_cached(
        "WITH RECURSIVE subtree(id) AS (
           SELECT id FROM tasks WHERE parentTaskId = ?1
           UNION
           SELECT tasks.id FROM tasks JOIN subtree ON tasks.parentTaskId = subtree.id
         )
         SELECT id FROM subtree",
    )?;
    let ids = statement
        .query_map([id], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// The columns the open tasks of `list_ids` are filed in — beneath the
/// entered task, for an entered board.
fn used_columns(
    connection: &Connection,
    key: &str,
    list_ids: &[String],
) -> Result<HashSet<String>, CoreError> {
    if list_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let within = match scoped(key) {
        Some((_, scope)) if scope != "root" => Some(descendants(connection, scope)?),
        _ => None,
    };
    let mut used = HashSet::new();
    for chunk in list_ids.chunks(500) {
        let marks = vec!["?"; chunk.len()].join(", ");
        let mut statement = connection.prepare(&format!(
            "SELECT t.id, m.kanbanColumn FROM tasks t JOIN task_metadata m ON m.taskId = t.id
             WHERE m.kanbanColumn IS NOT NULL AND t.status = 'open' AND t.listId IN ({marks})"
        ))?;
        let rows = statement.query_map(rusqlite::params_from_iter(chunk), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, column) = row?;
            if within.as_ref().is_none_or(|within| within.contains(&id)) {
                used.insert(column);
            }
        }
    }
    Ok(used)
}

/// Whether reading a board needs the rule context: a rule column, or an
/// entered task whose position can hide columns.
fn needs_context(key: &str, layout: &[BoardColumn]) -> bool {
    layout.iter().any(|column| column.rules.is_some())
        || scoped(key).is_some_and(|(_, scope)| scope != "root")
}

/// The board `key` over `list_ids` (its list, or Everything's or a folder's
/// open lists): its columns, and where each task that is not simply in its
/// filed column is drawn.
pub fn board_columns(
    connection: &Connection,
    key: &str,
    list_ids: &[String],
    now_ms: i64,
    zone: &str,
) -> Result<BoardColumns, CoreError> {
    let (first_layout, _) = layout(connection, key)?;
    let context = if needs_context(key, &first_layout) {
        Some(filter_lists::context(connection, now_ms, zone)?)
    } else {
        None
    };
    let resolved = resolve(connection, key, context.as_ref(), 0)?;
    let mut columns = resolved.visible();

    // A card filed in a column this layout lacks keeps a column of its own.
    let used = used_columns(connection, key, list_ids)?;
    let wanted = |id: &str, columns: &[BoardColumn]| {
        used.contains(id)
            && !resolved.hidden.contains(id)
            && !columns.iter().any(|column| column.id == id)
    };
    if !used.is_empty() {
        let mut candidates: Vec<BoardColumn> = Vec::new();
        if key == EVERYTHING_BOARD_KEY {
            for list in list_ids {
                candidates.extend(layout(connection, &format!("{list}/root"))?.0);
            }
        } else {
            candidates.extend(layout(connection, EVERYTHING_BOARD_KEY)?.0);
            candidates.extend(stored_layout(connection, DEFAULT_BOARD_KEY)?.unwrap_or_default());
            candidates.extend(stock());
        }
        for candidate in candidates {
            if wanted(&candidate.id, &columns) {
                columns.push(BoardColumn {
                    rules: None,
                    ..candidate
                });
            }
        }
    }

    let mut placements = Vec::new();
    let has_rules = columns.iter().any(|column| column.rules.is_some());
    if (has_rules || resolved.floor.is_some()) && !columns.is_empty() {
        let lists: Vec<String> = match scoped(key) {
            Some((list, scope)) if scope != "root" => vec![list.to_string()],
            _ => list_ids.to_vec(),
        };
        for task in filter_lists::facts_in(connection, "all", FactScope::Lists(&lists))? {
            let column = drawn(&task, &columns, &resolved, context.as_ref());
            let filed = task.column.as_deref().unwrap_or(FIRST_STOCK_COLUMN_ID);
            let plain = if columns.iter().any(|c| c.id == filed) {
                filed
            } else {
                columns[0].id.as_str()
            };
            if column != plain {
                placements.push(BoardColumnPlacement {
                    task_id: task.id,
                    column_id: column,
                });
            }
        }
    }

    let mut hidden: Vec<String> = resolved
        .layout
        .iter()
        .filter(|column| resolved.hidden.contains(&column.id))
        .map(|column| column.id.clone())
        .collect();
    hidden.dedup();
    Ok(BoardColumns {
        key: key.to_string(),
        layout: resolved.layout,
        source: resolved.source.to_string(),
        columns,
        hidden_column_ids: hidden,
        floor_column_id: resolved.floor,
        placements,
    })
}

// MARK: - Writes

fn refused(detail: impl Into<String>) -> CoreError {
    CoreError::ColumnRefused {
        detail: detail.into(),
    }
}

/// A layout tidied for saving: titles trimmed, rules validated, empty rules
/// dropped. Refuses an empty layout, a blank title and a repeated id.
fn tidy(columns: &[BoardColumn]) -> Result<Vec<BoardColumn>, CoreError> {
    if columns.is_empty() {
        return Err(refused("A board keeps at least one column."));
    }
    let mut seen = HashSet::new();
    let mut tidied = Vec::with_capacity(columns.len());
    for column in columns {
        let title = column.title.trim();
        if title.is_empty() {
            return Err(CoreError::EmptyName);
        }
        let id = column.id.trim();
        if id.is_empty() || !seen.insert(id.to_string()) {
            return Err(refused(format!("Two columns cannot share the id “{id}”.")));
        }
        let rules = match &column.rules {
            Some(rules) if !rules.rules.is_empty() => Some(filter_lists::validate(rules)?),
            _ => None,
        };
        tidied.push(BoardColumn {
            id: id.to_string(),
            title: title.to_string(),
            rules,
        });
    }
    Ok(tidied)
}

fn save_layout(
    transaction: &Transaction,
    key: &str,
    columns: &[BoardColumn],
) -> Result<(), CoreError> {
    let columns = tidy(columns)?;
    transaction.execute(
        "INSERT INTO kanban_boards(id, columnsJSON) VALUES (?1, ?2)
         ON CONFLICT(id) DO UPDATE SET columnsJSON = excluded.columnsJSON
         WHERE columnsJSON != excluded.columnsJSON",
        params![key, columns_json(&columns)],
    )?;
    Ok(())
}

/// Saves a board's columns, moving the cards in `moving_task_ids` to
/// `to_column` in the same step. An empty column list is ignored.
/// `WorkspaceStore.setKanbanBoardColumns`.
pub fn set_board_columns(
    transaction: &Transaction,
    key: &str,
    columns: &[BoardColumn],
    moving_task_ids: &[String],
    to_column: Option<&str>,
    now_ms: i64,
) -> Result<(), CoreError> {
    if columns.is_empty() {
        return Ok(());
    }
    if !moving_task_ids.is_empty() {
        crate::tasks::set_kanban_column(transaction, moving_task_ids, to_column, now_ms)?;
    }
    save_layout(transaction, key, columns)
}

/// Deletes a column from board `key`, which keeps a layout of its own from
/// then on. Tasks filed in it among `list_ids` (beneath the entered task, on
/// an entered board) move to `to_column`. Nothing moves out of a rule column,
/// which holds nothing filed, or out of Today or Waiting on, whose tasks stay
/// planned or waiting.
pub fn delete_board_column(
    transaction: &Transaction,
    key: &str,
    list_ids: &[String],
    column_id: &str,
    to_column: Option<&str>,
    now_ms: i64,
) -> Result<(), CoreError> {
    let (mut columns, _) = layout(transaction, key)?;
    let Some(index) = columns.iter().position(|column| column.id == column_id) else {
        return Ok(());
    };
    if columns.len() == 1 {
        return Err(refused("A board keeps at least one column."));
    }
    let removed = columns.remove(index);
    let keeps_tasks =
        removed.rules.is_some() || removed.id == TODAY_COLUMN_ID || removed.id == WAITING_COLUMN_ID;
    if let Some(to_column) = to_column.filter(|to| !keeps_tasks && *to != column_id) {
        let within = match scoped(key) {
            Some((_, scope)) if scope != "root" => Some(descendants(transaction, scope)?),
            _ => None,
        };
        let mut moving = Vec::new();
        for chunk in list_ids.chunks(500) {
            let marks = vec!["?"; chunk.len()].join(", ");
            let mut statement = transaction.prepare(&format!(
                "SELECT t.id FROM tasks t JOIN task_metadata m ON m.taskId = t.id
                 WHERE m.kanbanColumn = ? AND t.listId IN ({marks})"
            ))?;
            let mut values: Vec<&str> = vec![column_id];
            values.extend(chunk.iter().map(String::as_str));
            let rows = statement.query_map(rusqlite::params_from_iter(values), |row| {
                row.get::<_, String>(0)
            })?;
            for row in rows {
                let id = row?;
                if within.as_ref().is_none_or(|within| within.contains(&id)) {
                    moving.push(id);
                }
            }
        }
        if !moving.is_empty() {
            crate::tasks::set_kanban_column(transaction, &moving, Some(to_column), now_ms)?;
        }
    }
    save_layout(transaction, key, &columns)
}

/// Removes board `key`'s own layout, so it follows its list's or the
/// default again. On the default itself, back to the stock columns.
pub fn reset_board_columns(transaction: &Transaction, key: &str) -> Result<(), CoreError> {
    transaction.execute("DELETE FROM kanban_boards WHERE id = ?1", [key])?;
    Ok(())
}

/// Makes board `key`'s columns the workspace's default, and has the board
/// follow the default from then on.
pub fn make_default_board_columns(transaction: &Transaction, key: &str) -> Result<(), CoreError> {
    if key == DEFAULT_BOARD_KEY {
        return Ok(());
    }
    let (columns, _) = layout(transaction, key)?;
    save_layout(transaction, DEFAULT_BOARD_KEY, &columns)?;
    reset_board_columns(transaction, key)
}

/// An edit that makes a rule hold.
#[derive(Debug, Clone, PartialEq)]
enum Edit {
    AddTag(String),
    RemoveTag(String),
    ClearTags,
    Priority(Option<i64>),
    DueDay(Option<NaiveDate>),
    Column(String),
}

/// The one edit that makes `rule` hold, if there is exactly one sensible.
fn edit_for(rule: &FilterRule, context: &Context) -> Option<Edit> {
    let value = rule.value.trim();
    let date = || NaiveDate::parse_from_str(value, "%Y-%m-%d").ok();
    let priority = || {
        value
            .parse::<i64>()
            .ok()
            .filter(|p| crate::tasks::is_priority(*p))
    };
    Some(match (rule.field.as_str(), rule.op.as_str()) {
        ("tag", "has") => Edit::AddTag(value.trim_start_matches('#').to_string()),
        ("tag", "lacks") => Edit::RemoveTag(value.to_string()),
        ("tag", "none") => Edit::ClearTags,
        ("priority", "at-least" | "at-most" | "is") => Edit::Priority(Some(priority()?)),
        ("priority", "none") => Edit::Priority(None),
        ("due", "today" | "within") => Edit::DueDay(Some(context.today)),
        ("due", "before") => Edit::DueDay(Some(date()?.checked_sub_signed(Duration::days(1))?)),
        ("due", "on") => Edit::DueDay(Some(date()?)),
        ("due", "after") => Edit::DueDay(Some(date()?.checked_add_signed(Duration::days(1))?)),
        ("due", "none") => Edit::DueDay(None),
        ("today", "is") => Edit::Column(TODAY_COLUMN_ID.into()),
        ("waiting", "is") => Edit::Column(WAITING_COLUMN_ID.into()),
        ("column", "is") if !value.is_empty() => Edit::Column(value.to_string()),
        _ => return None,
    })
}

/// The edits that make `definition` hold for `task`, or why there are none.
fn plan_edits(
    column: &BoardColumn,
    definition: &FilterDefinition,
    task: &Facts,
    context: &Context,
) -> Result<Vec<Edit>, CoreError> {
    let catalog = filter_lists::filter_rule_catalog();
    let failing: Vec<&FilterRule> = definition
        .rules
        .iter()
        .filter(|rule| filter_lists::holds(rule, task, context) == Some(false))
        .collect();
    let cannot = |rule: &FilterRule| {
        refused(format!(
            "Moving a task cannot make “{}” true, so it cannot go into “{}”. Edit the task instead.",
            filter_lists::describe_rule(rule, &catalog),
            column.title
        ))
    };
    if definition.match_mode == "any" {
        if failing.len() < definition.rules.len() {
            return Ok(Vec::new());
        }
        return failing
            .iter()
            .find_map(|rule| edit_for(rule, context))
            .map(|edit| vec![edit])
            .ok_or_else(|| {
                failing
                    .first()
                    .map_or_else(|| refused("No rule to meet."), |r| cannot(r))
            });
    }
    failing
        .iter()
        .map(|rule| edit_for(rule, context).ok_or_else(|| cannot(rule)))
        .collect()
}

fn apply(
    transaction: &Transaction,
    id: &str,
    edits: &[Edit],
    now_ms: i64,
    zone: &str,
) -> Result<(), CoreError> {
    let mut metadata = editor::snapshot(transaction, id)?.metadata;
    let mut metadata_changed = false;
    for edit in edits {
        match edit {
            Edit::AddTag(tag) => {
                metadata.tags.push(tag.clone());
                metadata_changed = true;
            }
            Edit::RemoveTag(tag) => {
                let tag = filter_lists::normalized_tag(tag);
                metadata
                    .tags
                    .retain(|existing| filter_lists::normalized_tag(existing) != tag);
                metadata_changed = true;
            }
            Edit::ClearTags => {
                metadata.tags.clear();
                metadata_changed = true;
            }
            Edit::Priority(priority) => {
                metadata.priority = *priority;
                metadata_changed = true;
            }
            Edit::DueDay(day) => {
                let day = day.map(|day| day.format("%Y-%m-%d").to_string());
                editor::set_due_day(transaction, id, day.as_deref(), now_ms, zone)?;
            }
            Edit::Column(column) => {
                crate::tasks::set_kanban_column(
                    transaction,
                    &[id.to_string()],
                    Some(column),
                    now_ms,
                )?;
            }
        }
    }
    if metadata_changed {
        editor::write_editor_metadata(transaction, id, &metadata, now_ms)?;
    }
    Ok(())
}

/// Moves tasks into `column_id` on board `key`. Into an ordinary column the
/// tasks are filed there. Into a rule column the first task is edited until
/// the rules hold, where each failing rule has one clear edit — a tag added
/// or removed, a priority set, a due day set or cleared, planned for today,
/// waiting, or filed in a column; anything else is refused. Either way the
/// first task must then be drawn in `column_id`, not caught by a rule column
/// before it, or nothing is saved.
pub fn move_to_board_column(
    transaction: &Transaction,
    key: &str,
    task_ids: &[String],
    column_id: &str,
    now_ms: i64,
    zone: &str,
) -> Result<(), CoreError> {
    let Some(first) = task_ids.first() else {
        return Ok(());
    };
    let context = filter_lists::context(transaction, now_ms, zone)?;
    let resolved = resolve(transaction, key, Some(&context), 0)?;
    let columns = resolved.visible();
    let facts = |id: &str| -> Result<Facts, CoreError> {
        filter_lists::facts_in(transaction, "all", FactScope::Task(id))?
            .into_iter()
            .next()
            .ok_or_else(|| CoreError::MissingTask { id: id.to_string() })
    };
    let target = columns.iter().find(|column| column.id == column_id);
    match target.and_then(|column| column.rules.as_ref().map(|rules| (column, rules))) {
        Some((column, rules)) => {
            let task = facts(first)?;
            let edits = plan_edits(column, rules, &task, &context)?;
            apply(transaction, first, &edits, now_ms, zone)?;
        }
        None => crate::tasks::set_kanban_column(transaction, task_ids, Some(column_id), now_ms)?,
    }
    // The context's clock and blocked set are unchanged by these edits.
    let task = facts(first)?;
    if let Some(catching) = rule_column(&task, &columns, Some(&context))
        && catching.id != column_id
    {
        let target_title = target.map_or(column_id, |column| column.title.as_str());
        return Err(refused(format!(
            "That task matches the rules of “{}”, which comes before “{target_title}”, so it would stay there.",
            catching.title
        )));
    }
    Ok(())
}

// MARK: - The handle

#[uniffi::export]
impl CoreWorkspace {
    /// Board `key`'s columns over `list_ids`, and where its tasks are drawn.
    pub fn board_columns(
        &self,
        key: String,
        list_ids: Vec<String>,
        now_ms: i64,
        zone: String,
    ) -> Result<BoardColumns, CoreError> {
        board_columns(&self.read(), &key, &list_ids, now_ms, &zone)
    }

    /// Saves a board's columns, moving cards out of a removed one, as one step.
    pub fn set_board_columns(
        &self,
        key: String,
        columns: Vec<BoardColumn>,
        moving_task_ids: Vec<String>,
        to_column: Option<String>,
        label: String,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), &label, |tx| {
            set_board_columns(
                tx,
                &key,
                &columns,
                &moving_task_ids,
                to_column.as_deref(),
                now_ms,
            )
        })
    }

    /// Deletes a column, moving its tasks, as one "Delete Column" step.
    pub fn delete_board_column(
        &self,
        key: String,
        list_ids: Vec<String>,
        column_id: String,
        to_column: Option<String>,
        now_ms: i64,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Delete Column", |tx| {
            delete_board_column(
                tx,
                &key,
                &list_ids,
                &column_id,
                to_column.as_deref(),
                now_ms,
            )
        })
    }

    /// Has a board follow the default columns again, as one "Reset Columns" step.
    pub fn reset_board_columns(&self, key: String) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Reset Columns", |tx| {
            reset_board_columns(tx, &key)
        })
    }

    /// Makes a board's columns the default, as one "Make Default Columns" step.
    pub fn make_default_board_columns(&self, key: String) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Make Default Columns", |tx| {
            make_default_board_columns(tx, &key)
        })
    }

    /// Moves tasks into a column, editing the first to meet a rule column's
    /// rules, as one "Move Task" step.
    pub fn move_to_board_column(
        &self,
        key: String,
        task_ids: Vec<String>,
        column_id: String,
        now_ms: i64,
        zone: String,
    ) -> Result<(), CoreError> {
        journal::journalled(&mut self.lock(), "Move Task", |tx| {
            move_to_board_column(tx, &key, &task_ids, &column_id, now_ms, &zone)
        })
    }
}

#[cfg(test)]
mod tests;
