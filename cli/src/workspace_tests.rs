//! The workspace write tools, against scratch databases built from the app's
//! real schema (`fixtures/workspace_schema.sql`, dumped from a migrated
//! database). The real schema matters here more than anywhere: the undo
//! journal and the search index are both maintained by its triggers, and a
//! hand-written fixture would test neither.

use crate::checkvist::{CheckvistClient, CheckvistConfig};
use crate::local::LocalState;
use crate::tools::Tools;
use crate::workspace::Workspace;
use rusqlite::{Connection, params};
use serde_json::{Map, Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

const SCHEMA: &str = include_str!("fixtures/workspace_schema.sql");
const WORKSPACE: &str = "4A4BAC28-0000-4000-8000-000000000001";
const INBOX: &str = "827DF788-0000-4000-8000-000000000002";
const FOLDER: &str = "E0E463F3-0000-4000-8000-000000000003";
const PROJECTS: &str = "9FE01505-0000-4000-8000-000000000004";
const EARLIER: &str = "2026-09-01 09:00:00.000";

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct Fixture {
    path: PathBuf,
    tools: Tools,
}

impl Fixture {
    fn new() -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let directory = std::env::temp_dir().join(format!(
            "priority-workspace-tests-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("scratch directory");
        let path = directory.join("priority.sqlite");

        let connection = Connection::open(&path).expect("fixture database");
        // WAL, as the app's DatabasePool runs it, so the fixture exercises the
        // same journalling mode as the real file.
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .expect("wal");
        connection.execute_batch(SCHEMA).expect("schema");
        connection
            .execute_batch(&format!(
                "INSERT INTO workspaces VALUES ('{WORKSPACE}', 'My Workspace', '{EARLIER}', '{EARLIER}');
                 INSERT INTO list_folders VALUES ('{FOLDER}', '{WORKSPACE}', NULL, 'Computer Science', 0, '{EARLIER}', '{EARLIER}');
                 INSERT INTO task_lists (id, workspaceId, folderId, name, colorHex, sortOrder, isArchived, createdAt, updatedAt, systemRole)
                   VALUES ('{INBOX}', '{WORKSPACE}', NULL, 'Inbox', NULL, 0, 0, '{EARLIER}', '{EARLIER}', 'inbox');
                 INSERT INTO task_lists (id, workspaceId, folderId, name, colorHex, sortOrder, isArchived, createdAt, updatedAt)
                   VALUES ('{PROJECTS}', '{WORKSPACE}', NULL, 'Projects', '#7a4de8', 1, 0, '{EARLIER}', '{EARLIER}');"
            ))
            .expect("seed");
        drop(connection);

        let tools = Tools {
            client: CheckvistClient::new(CheckvistConfig {
                username: String::new(),
                remote_key: String::new(),
                default_list_id: "999".into(),
                base_url: "https://checkvist.invalid".into(),
            }),
            local: LocalState {
                prefs_path: directory.join("missing.plist"),
                store_directory: directory.clone(),
            },
            workspace: Workspace {
                database_path: path.clone(),
            },
        };
        Fixture { path, tools }
    }

    fn call(&self, name: &str, arguments: Value) -> Value {
        self.try_call(name, arguments)
            .unwrap_or_else(|error| panic!("{name} failed: {error}"))
    }

    fn try_call(&self, name: &str, arguments: Value) -> Result<Value, String> {
        let arguments: Map<String, Value> = arguments.as_object().cloned().unwrap_or_default();
        self.tools
            .call(name, &arguments)
            .map(|outcome| outcome.payload)
            .map_err(|error| error.message)
    }

    fn db(&self) -> Connection {
        Connection::open(&self.path).expect("reopen")
    }

    fn scalar<T: rusqlite::types::FromSql>(&self, sql: &str, id: &str) -> T {
        self.db()
            .query_row(sql, [id], |row| row.get(0))
            .unwrap_or_else(|error| panic!("{sql}: {error}"))
    }

    fn add(&self, list: &str, title: &str, parent: Option<&str>) -> String {
        let mut arguments = json!({ "list_id": list, "title": title });
        if let Some(parent) = parent {
            arguments["parent_task_id"] = json!(parent);
        }
        self.call("workspace_task_add", arguments)["id"]
            .as_str()
            .expect("id")
            .to_string()
    }

    fn order(&self, list: &str, parent: Option<&str>) -> Vec<String> {
        let connection = self.db();
        let mut statement = connection
            .prepare(
                "SELECT title FROM tasks WHERE listId = ?1 AND parentTaskId IS ?2 \
                 ORDER BY sortOrder, createdAt, id",
            )
            .unwrap();
        statement
            .query_map(params![list, parent], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<String>>>()
            .unwrap()
    }

    /// Undo groups, oldest first, as `(label, entry count)`.
    fn journal(&self) -> Vec<(String, i64)> {
        let connection = self.db();
        let mut statement = connection
            .prepare(
                "SELECT label, COUNT(*) FROM change_log WHERE undone = 0 \
                 GROUP BY groupId ORDER BY MAX(id)",
            )
            .unwrap();
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    }
}

// -- the rows a write leaves --------------------------------------------------

#[test]
fn a_new_task_has_the_shape_the_app_writes() {
    let fixture = Fixture::new();
    let id = fixture.add(PROJECTS, "  Write the report  ", None);

    // Foundation's `UUID().uuidString`: uppercase, hyphenated.
    assert_eq!(id.len(), 36);
    assert_eq!(id, id.to_uppercase());
    let (title, notes, status, order, kind, created, updated): (
        String,
        String,
        String,
        i64,
        String,
        String,
        String,
    ) = fixture
        .db()
        .query_row(
            "SELECT title, notes, status, sortOrder, itemKind, createdAt, updatedAt \
             FROM tasks WHERE id = ?1",
            [&id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        (title.as_str(), notes.as_str(), status.as_str()),
        ("Write the report", "", "open")
    );
    assert_eq!((order, kind.as_str()), (0, "task"));
    // GRDB's stored date format, milliseconds included: `2026-09-17 12:04:14.451`.
    assert_eq!(created.len(), 23, "{created}");
    assert_eq!(&created[10..11], " ");
    assert_eq!(&created[19..20], ".");
    assert_eq!(created, updated);

    // No metadata row until there is metadata, as `createTask` does.
    let metadata: i64 = fixture.scalar("SELECT COUNT(*) FROM task_metadata WHERE taskId = ?1", &id);
    assert_eq!(metadata, 0);

    // And the second one goes after it.
    fixture.add(PROJECTS, "Second", None);
    assert_eq!(
        fixture.order(PROJECTS, None),
        ["Write the report", "Second"]
    );
}

#[test]
fn notes_links_and_a_column_are_stored_where_the_app_reads_them() {
    let fixture = Fixture::new();
    let task = fixture.call(
        "workspace_task_add",
        json!({
            "list_id": PROJECTS,
            "title": "Read the paper",
            "notes": "Section 3 first.",
            "external_links": [
                " obsidian://open?vault=Studies&file=Paper ",
                "https://example.com/paper.pdf",
                "HTTPS://EXAMPLE.COM/paper.pdf",
                "",
            ],
            "kanban_column": "this-week",
        }),
    );
    let id = task["id"].as_str().unwrap();
    assert_eq!(task["notes"], json!("Section 3 first."));
    assert_eq!(task["kanban_column"], json!("this-week"));

    // Trimmed, blanks dropped, case-insensitive duplicates collapsed: the
    // editor's `normalizedStrings`.
    let links: String = fixture.scalar(
        "SELECT externalLinksJSON FROM task_metadata WHERE taskId = ?1",
        id,
    );
    assert_eq!(
        serde_json::from_str::<Value>(&links).unwrap(),
        json!([
            "obsidian://open?vault=Studies&file=Paper",
            "https://example.com/paper.pdf"
        ])
    );
    let tags: String = fixture.scalar("SELECT tagsJSON FROM task_metadata WHERE taskId = ?1", id);
    assert_eq!(tags, "[]");

    // Search finds it through the schema's own FTS triggers.
    let found: String = fixture.scalar(
        "SELECT t.id FROM tasks_fts f JOIN tasks t ON t.rowid = f.rowid WHERE tasks_fts MATCH ?1",
        "section",
    );
    assert_eq!(found, id);
}

#[test]
fn a_subtask_needs_only_its_parent() {
    let fixture = Fixture::new();
    let parent = fixture.add(PROJECTS, "Parent", None);
    let child = fixture.call(
        "workspace_task_add",
        json!({ "title": "Child", "parent_task_id": parent }),
    );
    assert_eq!(child["list_id"], json!(PROJECTS));
    assert_eq!(child["parent_task_id"], json!(parent));

    let wrong_list = fixture.try_call(
        "workspace_task_add",
        json!({ "title": "Stray", "list_id": INBOX, "parent_task_id": parent }),
    );
    assert!(
        wrong_list.unwrap_err().contains("not"),
        "a parent in another list is refused"
    );
}

#[test]
fn at_top_puts_a_task_first_and_renumbers_densely() {
    let fixture = Fixture::new();
    fixture.add(PROJECTS, "One", None);
    fixture.add(PROJECTS, "Two", None);
    fixture.call(
        "workspace_task_add",
        json!({ "list_id": PROJECTS, "title": "Zero", "at_top": true }),
    );
    assert_eq!(fixture.order(PROJECTS, None), ["Zero", "One", "Two"]);
    let orders: String = fixture
        .db()
        .query_row(
            "SELECT group_concat(sortOrder) FROM (SELECT sortOrder FROM tasks ORDER BY sortOrder)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(orders, "0,1,2");
}

// -- updates ------------------------------------------------------------------

#[test]
fn an_update_changes_only_what_it_names() {
    let fixture = Fixture::new();
    let id = fixture.call(
        "workspace_task_add",
        json!({ "list_id": PROJECTS, "title": "Draft", "notes": "keep me",
                "external_links": ["https://a.example"] }),
    )["id"]
        .as_str()
        .unwrap()
        .to_string();

    let updated = fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "title": "Final", "kanban_column": "today" }),
    );
    assert_eq!(updated["title"], json!("Final"));
    assert_eq!(updated["notes"], json!("keep me"));
    assert_eq!(updated["external_links"], json!(["https://a.example"]));
    assert_eq!(updated["kanban_column"], json!("today"));

    // Null clears the column; absence left it alone above.
    let cleared = fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "kanban_column": null }),
    );
    assert_eq!(cleared["kanban_column"], Value::Null);

    // An empty array removes every link.
    let unlinked = fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "external_links": [] }),
    );
    assert_eq!(unlinked["external_links"], json!([]));

    assert!(
        fixture
            .try_call("workspace_task_update", json!({ "task_id": id }))
            .unwrap_err()
            .contains("No updates")
    );
    assert!(
        fixture
            .try_call(
                "workspace_task_update",
                json!({ "task_id": id, "title": "   " })
            )
            .is_err()
    );
}

#[test]
fn completing_stamps_once_and_reopening_clears_the_stamp() {
    let fixture = Fixture::new();
    let id = fixture.add(PROJECTS, "Ship it", None);

    let done = fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "status": "completed" }),
    );
    assert_eq!(done["status"], json!("completed"));
    let stamped: String = fixture.scalar("SELECT completedAt FROM tasks WHERE id = ?1", &id);

    // Cancelling a completed task keeps its original completion time.
    fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "status": "cancelled" }),
    );
    let still: String = fixture.scalar("SELECT completedAt FROM tasks WHERE id = ?1", &id);
    assert_eq!(still, stamped);

    fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "status": "open" }),
    );
    let cleared: Option<String> =
        fixture.scalar("SELECT completedAt FROM tasks WHERE id = ?1", &id);
    assert_eq!(cleared, None);

    assert!(
        fixture
            .try_call(
                "workspace_task_update",
                json!({ "task_id": id, "status": "done" })
            )
            .is_err()
    );
}

#[test]
fn waiting_on_and_a_follow_up_file_the_task_in_waiting_on() {
    let fixture = Fixture::new();
    let task = fixture.add(PROJECTS, "Contract signed", None);
    fixture.call(
        "workspace_task_update",
        json!({ "task_id": task, "kanban_column": "today" }),
    );

    let updated = fixture.call(
        "workspace_task_update",
        json!({ "task_id": task, "waiting_on": "  Sam ", "follow_up_at": "2026-10-08 14:00" }),
    );
    assert_eq!(updated["waiting_on"], json!("Sam"));
    assert!(
        updated["follow_up_at"]
            .as_str()
            .is_some_and(|at| at.starts_with("2026-10-08 14:00"))
    );
    let column: Option<String> = fixture.scalar(
        "SELECT kanbanColumn FROM task_metadata WHERE taskId = ?1",
        &task,
    );
    assert_eq!(column.as_deref(), Some("waiting-on"));
    let stored: Option<String> = fixture.scalar(
        "SELECT waitingFollowUpAt FROM task_metadata WHERE taskId = ?1",
        &task,
    );
    assert!(stored.is_some_and(|at| at.ends_with(":00.000")));

    // Null clears the tag and leaves the time and the column.
    fixture.call(
        "workspace_task_update",
        json!({ "task_id": task, "waiting_on": null }),
    );
    let tag: Option<String> = fixture.scalar(
        "SELECT waitingOn FROM task_metadata WHERE taskId = ?1",
        &task,
    );
    assert_eq!(tag, None);

    let error = fixture
        .try_call(
            "workspace_task_update",
            json!({ "task_id": task, "follow_up_at": "soonish" }),
        )
        .unwrap_err();
    assert!(error.contains("2026-10-08 14:00"), "{error}");
}

#[test]
fn a_follow_up_repeats_every_n_days_and_left_out_keeps_its_repeat() {
    let fixture = Fixture::new();
    let task = fixture.add(PROJECTS, "Contract signed", None);
    let updated = fixture.call(
        "workspace_task_update",
        json!({ "task_id": task, "waiting_on": "Sam", "follow_up_at": "2099-10-08 14:00", "follow_up_every_days": 3 }),
    );
    assert_eq!(updated["follow_up_every_days"], json!(3));
    // A new time alone keeps the repeat.
    let updated = fixture.call(
        "workspace_task_update",
        json!({ "task_id": task, "follow_up_at": "2099-10-09 14:00" }),
    );
    assert_eq!(updated["follow_up_every_days"], json!(3));
    // Null makes it once.
    let updated = fixture.call(
        "workspace_task_update",
        json!({ "task_id": task, "follow_up_every_days": null }),
    );
    assert_eq!(updated.get("follow_up_every_days"), None);
    let error = fixture
        .try_call(
            "workspace_task_update",
            json!({ "task_id": task, "follow_up_every_days": 400 }),
        )
        .unwrap_err();
    assert!(error.contains("1 to 365"), "{error}");
}

#[test]
fn due_start_priority_and_estimate_set_and_clear() {
    let fixture = Fixture::new();
    let id = fixture.add(PROJECTS, "File the report", None);

    // A day is the editor's "any time that day": the planning's due date.
    let dated = fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "due": "2026-10-16" }),
    );
    assert_eq!(dated["due_date"], json!("2026-10-16"));
    assert_eq!(dated.get("due_at"), None);
    assert_eq!(fixture.journal().last().unwrap().0, "MCP: Set Due Date");

    // A time replaces the day.
    let timed = fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "due": "2026-10-17 17:00" }),
    );
    assert_eq!(timed.get("due_date"), None);
    assert!(
        timed["due_at"]
            .as_str()
            .is_some_and(|at| at.starts_with("2026-10-17 17:00")),
        "{timed}"
    );
    // And a day replaces the time again.
    let back = fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "due": "2026-10-18" }),
    );
    assert_eq!(back["due_date"], json!("2026-10-18"));
    assert_eq!(back.get("due_at"), None);

    // Priority, start and estimate in one call: one undo step.
    let steps = fixture.journal().len();
    let planned = fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "priority": 1, "start": "2026-10-15 09:00", "estimate_minutes": 45 }),
    );
    assert_eq!(planned["priority"], json!(1));
    assert_eq!(planned["estimate_seconds"], json!(2700));
    assert!(
        planned["start_at"]
            .as_str()
            .is_some_and(|at| at.starts_with("2026-10-15 09:00")),
        "{planned}"
    );
    assert_eq!(
        planned["due_date"],
        json!("2026-10-18"),
        "the due day stays"
    );
    assert_eq!(fixture.journal().len(), steps + 1);
    assert_eq!(fixture.journal().last().unwrap().0, "MCP: Edit Task");

    // Moving the start past the old deadline is fine when the deadline
    // moves with it.
    let moved = fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "start": "2026-10-20", "due": "2026-10-24" }),
    );
    assert_eq!(moved["due_date"], json!("2026-10-24"));
    assert!(moved["start_at"].as_str().is_some_and(|at| !at.is_empty()));
    // But not past it on its own.
    let error = fixture
        .try_call(
            "workspace_task_update",
            json!({ "task_id": id, "start": "2026-10-30" }),
        )
        .unwrap_err();
    assert!(error.contains("before the deadline"), "{error}");

    // Null clears each; 0 is the app's "clear the priority" key.
    let cleared = fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "due": null, "start": null, "priority": 0, "estimate_minutes": null }),
    );
    for key in [
        "due_date",
        "due_at",
        "start_at",
        "priority",
        "estimate_seconds",
    ] {
        assert_eq!(cleared.get(key), None, "{key} in {cleared}");
    }
    let priority: Option<i64> =
        fixture.scalar("SELECT priority FROM task_metadata WHERE taskId = ?1", &id);
    assert_eq!(priority, None);

    // Refusals name what they want.
    for (arguments, expected) in [
        (json!({ "task_id": id, "priority": 5 }), "priority must be"),
        (json!({ "task_id": id, "due": "friday" }), "2026-10-17"),
        (
            json!({ "task_id": id, "estimate_minutes": -5 }),
            "estimate_minutes",
        ),
        (json!({ "task_id": id, "due": "2026-02-30" }), "2026-10-17"),
    ] {
        let error = fixture
            .try_call("workspace_task_update", arguments)
            .unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn a_single_change_to_priority_or_start_is_named_for_it() {
    let fixture = Fixture::new();
    let id = fixture.add(PROJECTS, "Book the venue", None);
    fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "priority": 2 }),
    );
    assert_eq!(fixture.journal().last().unwrap().0, "MCP: Set Priority");
    fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "start": "2026-10-20" }),
    );
    assert_eq!(fixture.journal().last().unwrap().0, "MCP: Schedule Task");
    // Setting what it already has leaves no step.
    let steps = fixture.journal().len();
    fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "priority": 2 }),
    );
    assert_eq!(fixture.journal().len(), steps);
    // -4 to -1 are priorities too: behind the rest.
    let later = fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "priority": -1 }),
    );
    assert_eq!(later["priority"], json!(-1));
    let error = fixture
        .try_call(
            "workspace_task_update",
            json!({ "task_id": id, "priority": -5 }),
        )
        .unwrap_err();
    assert!(error.contains("priority must be"), "{error}");
}

#[test]
fn completing_a_source_task_ends_the_habits_made_from_it() {
    let fixture = Fixture::new();
    let source = fixture.add(PROJECTS, "Learn drums", None);
    let habit = fixture.add(PROJECTS, "Practise drums", None);
    let other = fixture.add(PROJECTS, "Stretch", None);
    let db = fixture.db();
    for (id, task, rule) in [("H1", &habit, "source"), ("H2", &other, "never")] {
        db.execute(
            "INSERT INTO dailies (id, taskId, sortOrder, createdAt, updatedAt, sourceTaskId, \
             placementColumn, expiryRule) VALUES (?1, ?2, 0, ?3, ?3, ?4, 'today', ?5)",
            params![id, task, EARLIER, source, rule],
        )
        .unwrap();
        db.execute(
            "INSERT INTO task_metadata (taskId, tagsJSON, externalLinksJSON, kanbanColumn, \
             updatedAt) VALUES (?1, '[]', '[]', 'today', ?2)",
            params![task, EARLIER],
        )
        .unwrap();
    }

    fixture.call(
        "workspace_task_update",
        json!({ "task_id": source, "status": "completed" }),
    );

    let archived: Option<String> =
        fixture.scalar("SELECT archivedAt FROM dailies WHERE id = ?1", "H1");
    assert!(archived.is_some());
    let column: Option<String> = fixture.scalar(
        "SELECT kanbanColumn FROM task_metadata WHERE taskId = ?1",
        &habit,
    );
    assert_eq!(column, None);
    let kept: Option<String> = fixture.scalar("SELECT archivedAt FROM dailies WHERE id = ?1", "H2");
    assert_eq!(kept, None, "a habit that never expires outlives its source");
}

#[test]
fn committing_to_a_task_adds_a_subtask_with_a_habit_on_it() {
    let fixture = Fixture::new();
    let goal = fixture.add(PROJECTS, "Memorise the sonnets", None);

    let payload = fixture.call(
        "workspace_task_commit",
        json!({ "task_id": goal, "title": "Memorise sonnets for 5 minutes" }),
    );
    assert_eq!(payload["parent_task_id"], json!(goal));
    assert_eq!(payload["title"], json!("Memorise sonnets for 5 minutes"));
    assert_eq!(payload["stacks_missed"], json!(false));
    let subtask = payload["task_id"].as_str().unwrap().to_string();
    let parent: Option<String> =
        fixture.scalar("SELECT parentTaskId FROM tasks WHERE id = ?1", &subtask);
    assert_eq!(parent.as_deref(), Some(goal.as_str()));
    let source: Option<String> = fixture.scalar(
        "SELECT sourceTaskId FROM dailies WHERE taskId = ?1",
        &subtask,
    );
    assert_eq!(source.as_deref(), Some(goal.as_str()));

    // With no title it is named after the task, as the app's form does.
    let named = fixture.call(
        "workspace_task_commit",
        json!({ "task_id": goal, "rule": "weekdays" }),
    );
    assert_eq!(named["title"], json!("Work on Memorise the sonnets"));
}

#[test]
fn completing_a_repeating_task_writes_its_next_occurrence() {
    let fixture = Fixture::new();
    let id = fixture.add(PROJECTS, "Water the plants", None);
    fixture
        .db()
        .execute(
            "INSERT INTO task_metadata (taskId, tagsJSON, externalLinksJSON, recurrenceRule, updatedAt) \
             VALUES (?1, '[]', '[]', 'every week', ?2)",
            params![id, EARLIER],
        )
        .unwrap();
    fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "status": "completed" }),
    );
    let status: String = fixture.scalar("SELECT status FROM tasks WHERE id = ?1", &id);
    assert_eq!(status, "completed");
    // The series goes on: the next occurrence is open and carries the rule
    // (the Rust core's `tasks::set_status`).
    let next: String = fixture.scalar(
        "SELECT t.id FROM tasks t JOIN task_metadata m ON m.taskId = t.id \
         WHERE t.title = 'Water the plants' AND t.status = 'open' AND m.recurrenceRule = 'every week' \
         AND t.id <> ?1",
        &id,
    );
    assert_ne!(next, id);
}

// -- moving -------------------------------------------------------------------

#[test]
fn reparenting_carries_the_subtree_and_refuses_a_cycle() {
    let fixture = Fixture::new();
    let parent = fixture.add(PROJECTS, "Parent", None);
    let child = fixture.add(PROJECTS, "Child", Some(&parent));
    let grandchild = fixture.add(PROJECTS, "Grandchild", Some(&child));

    // Into its own descendant: refused, and nothing changes.
    let error = fixture
        .try_call(
            "workspace_task_move",
            json!({ "task_id": parent, "parent_task_id": grandchild }),
        )
        .unwrap_err();
    assert!(error.contains("itself"), "{error}");

    // To another list's top level: the subtree follows.
    let moved = fixture.call(
        "workspace_task_move",
        json!({ "task_id": child, "list_id": INBOX }),
    );
    assert_eq!(moved["list_id"], json!(INBOX));
    assert_eq!(moved["parent_task_id"], Value::Null);
    let list: String = fixture.scalar("SELECT listId FROM tasks WHERE id = ?1", &grandchild);
    assert_eq!(list, INBOX);
    let parent_of: String =
        fixture.scalar("SELECT parentTaskId FROM tasks WHERE id = ?1", &grandchild);
    assert_eq!(parent_of, child);

    // Back under a parent in the first list.
    fixture.call(
        "workspace_task_move",
        json!({ "task_id": child, "parent_task_id": parent }),
    );
    let list: String = fixture.scalar("SELECT listId FROM tasks WHERE id = ?1", &grandchild);
    assert_eq!(list, PROJECTS);
}

#[test]
fn a_move_across_lists_journals_every_row_of_the_subtree() {
    // The subtree's listId is rewritten in one `UPDATE … WHERE id IN (…)`, as
    // the app does. The change_log triggers are per row, so the undo step
    // must still hold one entry per moved task plus the root's reparent.
    let fixture = Fixture::new();
    let root = fixture.add(PROJECTS, "Root", None);
    let child = fixture.add(PROJECTS, "Child", Some(&root));
    let grandchild = fixture.add(PROJECTS, "Grandchild", Some(&child));
    fixture.call(
        "workspace_task_move",
        json!({ "task_id": root, "list_id": INBOX }),
    );

    let (label, entries) = fixture.journal().pop().expect("an undo step");
    assert!(label.starts_with("MCP: "), "{label}");
    // Three listId rewrites, then the root's parent and order.
    assert_eq!(entries, 4);
    for id in [&root, &child, &grandchild] {
        let list: String = fixture.scalar("SELECT listId FROM tasks WHERE id = ?1", id);
        assert_eq!(list, INBOX);
        let rows: i64 = fixture.scalar(
            "SELECT COUNT(*) FROM change_log WHERE rowId = ?1 AND operation = 'update' \
             AND json_extract(afterJSON, '$.listId') = '827DF788-0000-4000-8000-000000000002'",
            id,
        );
        assert!(rows >= 1, "no journalled listId change for {id}");
    }
}

#[test]
fn position_reorders_among_siblings() {
    let fixture = Fixture::new();
    for title in ["A", "B", "C", "D"] {
        fixture.add(PROJECTS, title, None);
    }
    let d: String = fixture.scalar("SELECT id FROM tasks WHERE title = ?1", "D");
    fixture.call(
        "workspace_task_move",
        json!({ "task_id": d, "position": 2 }),
    );
    assert_eq!(fixture.order(PROJECTS, None), ["A", "D", "B", "C"]);

    // Past the end means last.
    let a: String = fixture.scalar("SELECT id FROM tasks WHERE title = ?1", "A");
    fixture.call(
        "workspace_task_move",
        json!({ "task_id": a, "position": 99 }),
    );
    assert_eq!(fixture.order(PROJECTS, None), ["D", "B", "C", "A"]);

    assert!(
        fixture
            .try_call(
                "workspace_task_move",
                json!({ "task_id": a, "position": 0 })
            )
            .is_err()
    );
}

#[test]
fn moving_to_a_list_lands_under_its_visible_root() {
    let fixture = Fixture::new();
    // An imported list whose one root task is a wrapper named after it.
    let wrapper = fixture.add(PROJECTS, "Projects", None);
    fixture.add(PROJECTS, "Inside", Some(&wrapper));
    fixture
        .db()
        .execute(
            "UPDATE task_lists SET visibleRootTaskId = ?1 WHERE id = ?2",
            params![wrapper, PROJECTS],
        )
        .unwrap();
    let stray = fixture.add(INBOX, "Stray", None);
    let moved = fixture.call(
        "workspace_task_move",
        json!({ "task_id": stray, "list_id": PROJECTS }),
    );
    assert_eq!(moved["parent_task_id"], json!(wrapper));
}

// -- promoting a task to its own list ----------------------------------------

#[test]
fn a_task_promoted_to_a_list_keeps_its_subtree_as_the_lists_contents() {
    let fixture = Fixture::new();
    let project = fixture.add(PROJECTS, "Get a first in ELEC0150", None);
    let week = fixture.add(PROJECTS, "Week 1", Some(&project));
    let reading = fixture.add(PROJECTS, "Reading", Some(&week));
    fixture.add(PROJECTS, "Unrelated", None);

    let result = fixture.call(
        "workspace_task_to_list",
        json!({ "task_id": project, "folder_id": FOLDER }),
    );
    let list = &result["list"];
    let list_id = list["id"].as_str().unwrap();
    assert_eq!(list["name"], json!("Get a first in ELEC0150"));
    assert_eq!(list["folder_id"], json!(FOLDER));
    // The task survives as the list's visible root: same id, now a list.
    assert_eq!(list["visible_root_task_id"], json!(project));
    assert_eq!(result["root_task"]["kind"], json!("list"));
    assert_eq!(result["root_task"]["parent_task_id"], Value::Null);
    assert_eq!(result["moved_task_count"], json!(3));

    // Subtasks moved with it and kept their shape.
    for id in [&week, &reading] {
        let moved_to: String = fixture.scalar("SELECT listId FROM tasks WHERE id = ?1", id);
        assert_eq!(moved_to, list_id);
    }
    let parent_of: String = fixture.scalar("SELECT parentTaskId FROM tasks WHERE id = ?1", &week);
    assert_eq!(parent_of, project);
    // The colour comes from the list it left, as in the app.
    let color: String = fixture.scalar("SELECT colorHex FROM task_lists WHERE id = ?1", list_id);
    assert_eq!(color, "#7a4de8");
    assert_eq!(fixture.order(PROJECTS, None), ["Unrelated"]);

    // One undo step for the whole thing.
    let journal = fixture.journal();
    assert_eq!(journal.last().unwrap().0, "MCP: Move Item to Folder");

    // The tree tool shows it in the folder, and its tasks without the wrapper
    // being mistaken for a nested list.
    let tree = fixture.call("workspace_tree", json!({}));
    assert!(
        tree["lists"]
            .as_array()
            .unwrap()
            .iter()
            .any(|list| list["id"] == json!(list_id) && list["folder_id"] == json!(FOLDER))
    );
    assert_eq!(tree["nested_lists"], json!([]));
}

#[test]
fn a_task_becomes_a_pinned_nested_list_in_place() {
    let fixture = Fixture::new();
    let parent = fixture.add(PROJECTS, "Reading list", None);
    fixture.add(PROJECTS, "Book", Some(&parent));

    // Pinning a plain task is refused: only a nested list has a sidebar row.
    assert!(
        fixture
            .try_call(
                "workspace_task_update",
                json!({ "task_id": parent, "pinned": true })
            )
            .unwrap_err()
            .contains("not a nested list")
    );

    let updated = fixture.call(
        "workspace_task_update",
        json!({ "task_id": parent, "kind": "list", "pinned": true }),
    );
    assert_eq!(updated["kind"], json!("list"));
    assert_eq!(updated["is_promoted"], json!(true));
    assert_eq!(updated["list_id"], json!(PROJECTS), "it stays where it was");

    let tree = fixture.call("workspace_tree", json!({}));
    assert_eq!(tree["nested_lists"][0]["id"], json!(parent));
    assert_eq!(tree["nested_lists"][0]["is_promoted"], json!(true));

    // Back to a task clears the pin, as `setItemKind` does.
    let task = fixture.call(
        "workspace_task_update",
        json!({ "task_id": parent, "kind": "task" }),
    );
    assert_eq!(task.get("is_promoted"), None);
    assert_eq!(fixture.journal().last().unwrap().0, "MCP: Convert to Task");
}

#[test]
fn a_lists_own_visible_root_cannot_be_promoted_out_of_it() {
    let fixture = Fixture::new();
    let wrapper = fixture.add(PROJECTS, "Projects", None);
    fixture
        .db()
        .execute(
            "UPDATE task_lists SET visibleRootTaskId = ?1 WHERE id = ?2",
            params![wrapper, PROJECTS],
        )
        .unwrap();
    let error = fixture
        .try_call("workspace_task_to_list", json!({ "task_id": wrapper }))
        .unwrap_err();
    assert!(error.contains("visible root"), "{error}");
}

// -- folders and lists --------------------------------------------------------

#[test]
fn folders_and_lists_append_and_move_like_the_sidebar() {
    let fixture = Fixture::new();
    let folder = fixture.call("workspace_folder_create", json!({ "name": "Studies" }));
    assert_eq!(folder["sort_order"], json!(1), "after Computer Science");
    let folder_id = folder["id"].as_str().unwrap();

    let list = fixture.call(
        "workspace_list_create",
        json!({ "name": "Revision", "folder_id": folder_id }),
    );
    assert_eq!(list["folder_id"], json!(folder_id));
    assert_eq!(list["sort_order"], json!(0));

    // Projects moves into Computer Science, then back out to the end of the
    // top level.
    let moved = fixture.call(
        "workspace_list_move",
        json!({ "list_id": PROJECTS, "folder_id": FOLDER }),
    );
    assert_eq!(moved["folder_id"], json!(FOLDER));
    let out = fixture.call("workspace_list_move", json!({ "list_id": PROJECTS }));
    assert_eq!(out["folder_id"], Value::Null);
    assert_eq!(out["sort_order"], json!(1), "after the Inbox");

    assert!(
        fixture
            .try_call(
                "workspace_list_move",
                json!({ "list_id": PROJECTS, "folder_id": "NOPE" })
            )
            .unwrap_err()
            .contains("No folder")
    );
}

#[test]
fn deleting_a_list_takes_its_tasks_and_spares_the_inbox() {
    let fixture = Fixture::new();
    fixture.add(PROJECTS, "Goes with it", None);
    let deleted = fixture.call("workspace_list_delete", json!({ "list_id": PROJECTS }));
    assert_eq!(deleted["tasks_deleted"], json!(1));
    assert!(
        fixture
            .try_call("workspace_tasks", json!({ "list_id": PROJECTS }))
            .unwrap_err()
            .contains("No list")
    );

    let inbox = fixture.call("workspace_tree", json!({}))["lists"]
        .as_array()
        .unwrap()
        .iter()
        .find(|list| list["system_role"] == json!("inbox"))
        .map(|list| list["id"].as_str().unwrap().to_string())
        .unwrap();
    assert!(
        fixture
            .try_call("workspace_list_delete", json!({ "list_id": inbox }))
            .unwrap_err()
            .contains("Inbox")
    );
}

#[test]
fn the_task_tree_nests_children_and_hides_closed_work() {
    let fixture = Fixture::new();
    let parent = fixture.add(PROJECTS, "Parent", None);
    fixture.add(PROJECTS, "Child", Some(&parent));
    let done = fixture.add(PROJECTS, "Done", None);
    fixture.call(
        "workspace_task_update",
        json!({ "task_id": done, "status": "completed" }),
    );

    let open = fixture.call("workspace_tasks", json!({ "list_id": PROJECTS }));
    assert_eq!(open["task_count"], json!(2));
    assert_eq!(open["tasks"][0]["children"][0]["title"], json!("Child"));

    let all = fixture.call(
        "workspace_tasks",
        json!({ "list_id": PROJECTS, "include_closed": true }),
    );
    assert_eq!(all["task_count"], json!(3));

    let subtree = fixture.call(
        "workspace_tasks",
        json!({ "list_id": PROJECTS, "parent_task_id": parent }),
    );
    assert_eq!(subtree["tasks"][0]["title"], json!("Child"));
}

// -- deleting -----------------------------------------------------------------

#[test]
fn deleting_takes_the_subtree_by_cascade() {
    let fixture = Fixture::new();
    let parent = fixture.add(PROJECTS, "Parent", None);
    fixture.add(PROJECTS, "Child", Some(&parent));
    let result = fixture.call("workspace_task_delete", json!({ "task_id": parent }));
    assert_eq!(result["subtasks_deleted"], json!(1));
    let left: i64 = fixture.scalar("SELECT COUNT(*) FROM tasks WHERE listId = ?1", PROJECTS);
    assert_eq!(left, 0, "foreign keys were on, so the child went too");
}

// -- the undo journal ---------------------------------------------------------

#[test]
fn each_write_is_one_labelled_undo_step() {
    let fixture = Fixture::new();
    let id = fixture.call(
        "workspace_task_add",
        json!({ "list_id": PROJECTS, "title": "Linked", "external_links": ["https://a.example"] }),
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "status": "completed" }),
    );
    fixture.call("workspace_folder_create", json!({ "name": "F" }));

    let journal = fixture.journal();
    assert_eq!(
        journal,
        vec![
            // The task row and its metadata row, together.
            ("MCP: New Task".to_string(), 2),
            ("MCP: Change Status".to_string(), 1),
            ("MCP: New Folder".to_string(), 1),
        ]
    );
    // Recording is switched off again afterwards, so the app's own
    // unjournalled writes are not swept into the last MCP step.
    let suppressed: i64 = fixture.scalar("SELECT suppressed FROM undo_control WHERE id = ?1", "0");
    assert_eq!(suppressed, 1);

    // The journal holds what replaying needs: the full row before and after.
    let after: String = fixture.scalar(
        "SELECT afterJSON FROM change_log WHERE label = 'MCP: Change Status' AND rowId = ?1",
        &id,
    );
    assert_eq!(
        serde_json::from_str::<Value>(&after).unwrap()["status"],
        json!("completed")
    );
}

#[test]
fn a_no_op_leaves_no_undo_step_and_keeps_redo() {
    let fixture = Fixture::new();
    let id = fixture.add(PROJECTS, "Same", None);
    // Something the app has undone, waiting to be redone.
    fixture
        .db()
        .execute(
            "INSERT INTO change_log (groupId, label, tableName, rowId, operation, undone) \
             VALUES ('G', 'Rename List', 'task_lists', ?1, 'update', 1)",
            [PROJECTS],
        )
        .unwrap();

    fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "title": "Same" }),
    );
    let redo: i64 = fixture.scalar("SELECT COUNT(*) FROM change_log WHERE undone = ?1", "1");
    assert_eq!(
        redo, 1,
        "a write that changed nothing does not discard redo"
    );

    fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "title": "Different" }),
    );
    let redo: i64 = fixture.scalar("SELECT COUNT(*) FROM change_log WHERE undone = ?1", "1");
    assert_eq!(redo, 0, "a real change starts a new branch of history");
}

#[test]
fn a_refused_write_leaves_nothing_behind() {
    let fixture = Fixture::new();
    let before = fixture.journal();
    assert!(
        fixture
            .try_call(
                "workspace_task_move",
                json!({ "task_id": "MISSING", "position": 1 })
            )
            .is_err()
    );
    assert_eq!(fixture.journal(), before);
    let suppressed: i64 = fixture.scalar("SELECT suppressed FROM undo_control WHERE id = ?1", "0");
    assert_eq!(suppressed, 1);
}

#[test]
fn an_unmigrated_database_is_refused_rather_than_written() {
    let fixture = Fixture::new();
    fixture
        .db()
        .execute(
            "DELETE FROM grdb_migrations WHERE identifier = 'v16_task_completion_time'",
            [],
        )
        .unwrap();
    let error = fixture
        .try_call(
            "workspace_task_add",
            json!({ "list_id": PROJECTS, "title": "x" }),
        )
        .unwrap_err();
    assert!(error.contains("predates"), "{error}");
}

#[test]
fn a_write_waits_out_another_writer_instead_of_failing() {
    let fixture = Fixture::new();
    let holder = fixture.db();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let path = fixture.path.clone();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        holder.execute_batch("COMMIT").unwrap();
        drop(path);
    });
    // Blocks on the lock for ~300ms, then succeeds.
    fixture.add(PROJECTS, "Patient", None);
    release.join().unwrap();
}

#[test]
fn reads_work_with_the_app_closed_and_still_cannot_write() {
    // A WAL database with no `-shm` beside it, which a strictly read-only
    // connection cannot create: what SQLite leaves when the app's last
    // connection closes cleanly, and what a `.backup` copy looks like. Built
    // deliberately, since whether SQLite removes the file on close varies.
    let fixture = Fixture::new();
    fixture.add(PROJECTS, "Visible", None);
    fixture
        .db()
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .unwrap();
    for suffix in ["sqlite-wal", "sqlite-shm"] {
        let _ = std::fs::remove_file(fixture.path.with_extension(suffix));
    }
    let strict =
        Connection::open_with_flags(&fixture.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    assert!(
        strict
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get::<_, i64>(0))
            .is_err(),
        "the state this test is about: a strictly read-only open cannot read"
    );
    drop(strict);

    let tasks = fixture.call("workspace_tasks", json!({ "list_id": PROJECTS }));
    assert_eq!(tasks["tasks"][0]["title"], json!("Visible"));
    assert_eq!(
        fixture.call("focus_status", json!({}))["running"],
        json!(false)
    );

    // The fallback connection is still read-only.
    let connection = fixture.tools.workspace.open().expect("open");
    assert!(
        connection.execute("DELETE FROM tasks", []).is_err(),
        "query_only refuses writes"
    );
}

// -- dependencies -------------------------------------------------------------

fn ids(tasks: &Value) -> Vec<String> {
    tasks
        .as_array()
        .expect("array")
        .iter()
        .map(|task| task["id"].as_str().expect("id").to_string())
        .collect()
}

#[test]
fn a_blocker_is_added_listed_and_removed_across_lists() {
    let fixture = Fixture::new();
    let waiting = fixture.add(PROJECTS, "Write it up", None);
    let free = fixture.add(PROJECTS, "Unrelated", None);
    let blocker = fixture.add(INBOX, "Get the data", None);

    let added = fixture.call(
        "workspace_task_blocker_add",
        json!({ "task_id": waiting, "blocked_by_task_id": blocker }),
    );
    assert_eq!(added["changed"], json!(true));
    assert_eq!(added["blocked"], json!(true));
    assert_eq!(added["task"]["blocked"], json!(true));
    assert_eq!(added["blocked_by"][0]["id"], json!(blocker));
    assert_eq!(added["blocked_by"][0]["title"], json!("Get the data"));
    assert_eq!(added["blocked_by"][0]["status"], json!("open"));
    assert_eq!(added["blocked_by"][0]["list_id"], json!(INBOX));
    assert_eq!(added["blocked_by"][0]["list_name"], json!("Inbox"));

    // The other end sees it too, and is not itself blocked.
    let other = fixture.call("workspace_task_dependencies", json!({ "task_id": blocker }));
    assert_eq!(other["blocked"], json!(false));
    assert_eq!(ids(&other["blocking"]), vec![waiting.clone()]);
    assert_eq!(other["blocked_by"], json!([]));

    // The list read flags it, sinks it below its unblocked sibling, and
    // leaves the sibling unflagged.
    let tasks = fixture.call("workspace_tasks", json!({ "list_id": PROJECTS }));
    assert_eq!(tasks["tasks"][0]["id"], json!(free));
    assert_eq!(tasks["tasks"][0].get("blocked"), None);
    assert_eq!(tasks["tasks"][1]["id"], json!(waiting));
    assert_eq!(tasks["tasks"][1]["blocked"], json!(true));

    // A second add is not an error, and not a step.
    let steps = fixture.journal().len();
    let again = fixture.call(
        "workspace_task_blocker_add",
        json!({ "task_id": waiting, "blocked_by_task_id": blocker }),
    );
    assert_eq!(again["changed"], json!(false));
    assert_eq!(fixture.journal().len(), steps);

    // A done blocker no longer holds it back, though the edge stays.
    fixture.call(
        "workspace_task_update",
        json!({ "task_id": blocker, "status": "completed" }),
    );
    let after = fixture.call("workspace_task_dependencies", json!({ "task_id": waiting }));
    assert_eq!(after["blocked"], json!(false));
    assert_eq!(after["blocked_by"][0]["status"], json!("completed"));

    let removed = fixture.call(
        "workspace_task_blocker_remove",
        json!({ "task_id": waiting, "blocked_by_task_id": blocker }),
    );
    assert_eq!(removed["changed"], json!(true));
    assert_eq!(removed["blocked_by"], json!([]));
    let again = fixture.call(
        "workspace_task_blocker_remove",
        json!({ "task_id": waiting, "blocked_by_task_id": blocker }),
    );
    assert_eq!(again["changed"], json!(false));
}

#[test]
fn a_dependency_cycle_is_refused_and_leaves_nothing_behind() {
    let fixture = Fixture::new();
    let a = fixture.add(PROJECTS, "A", None);
    let b = fixture.add(PROJECTS, "B", None);
    let c = fixture.add(PROJECTS, "C", None);
    fixture.call(
        "workspace_task_blocker_add",
        json!({ "task_id": a, "blocked_by_task_id": b }),
    );
    fixture.call(
        "workspace_task_blocker_add",
        json!({ "task_id": b, "blocked_by_task_id": c }),
    );
    let before = fixture.journal();

    for (task, blocker) in [(&c, &a), (&b, &a), (&a, &a)] {
        let error = fixture
            .try_call(
                "workspace_task_blocker_add",
                json!({ "task_id": task, "blocked_by_task_id": blocker }),
            )
            .unwrap_err();
        assert!(error.contains("cannot wait on itself"), "{error}");
    }
    let error = fixture
        .try_call(
            "workspace_task_blocker_add",
            json!({ "task_id": a, "blocked_by_task_id": "MISSING" }),
        )
        .unwrap_err();
    assert!(error.contains("MISSING"), "{error}");

    assert_eq!(fixture.journal(), before);
    let edges: i64 = fixture.scalar(
        "SELECT COUNT(*) FROM task_dependencies WHERE taskId <> ?1",
        "",
    );
    assert_eq!(edges, 2);
}

#[test]
fn each_dependency_write_is_one_labelled_undo_step() {
    let fixture = Fixture::new();
    let task = fixture.add(PROJECTS, "Waits", None);
    let blocker = fixture.add(PROJECTS, "First", None);
    let steps = fixture.journal().len();
    fixture.call(
        "workspace_task_blocker_add",
        json!({ "task_id": task, "blocked_by_task_id": blocker }),
    );
    fixture.call(
        "workspace_task_blocker_remove",
        json!({ "task_id": task, "blocked_by_task_id": blocker }),
    );
    let journal = fixture.journal();
    assert_eq!(
        journal[steps..],
        [
            ("MCP: Add Blocker".to_string(), 1),
            ("MCP: Remove Blocker".to_string(), 1),
        ]
    );
    let suppressed: i64 = fixture.scalar("SELECT suppressed FROM undo_control WHERE id = ?1", "0");
    assert_eq!(suppressed, 1);
}

#[test]
fn a_workspace_without_dependencies_reads_empty_and_refuses_writes() {
    let fixture = Fixture::new();
    let task = fixture.add(PROJECTS, "Old", None);
    let other = fixture.add(PROJECTS, "Older", None);
    fixture
        .db()
        .execute_batch(
            "DROP TABLE task_dependencies;
             DELETE FROM grdb_migrations WHERE identifier = 'v21_task_dependencies';",
        )
        .unwrap();

    let read = fixture.call("workspace_task_dependencies", json!({ "task_id": task }));
    assert_eq!(read["blocked"], json!(false));
    assert_eq!(read["blocked_by"], json!([]));
    assert_eq!(read["blocking"], json!([]));
    let tasks = fixture.call("workspace_tasks", json!({ "list_id": PROJECTS }));
    assert_eq!(tasks["task_count"], json!(2));

    for tool in [
        "workspace_task_blocker_add",
        "workspace_task_blocker_remove",
    ] {
        let error = fixture
            .try_call(
                tool,
                json!({ "task_id": task, "blocked_by_task_id": other }),
            )
            .unwrap_err();
        assert!(
            error.contains("predates task dependencies"),
            "{tool}: {error}"
        );
    }
}

// -- board columns ------------------------------------------------------------

fn column_ids(payload: &Value) -> Vec<String> {
    ids(&payload["columns"])
}

#[test]
fn board_columns_are_read_added_ruled_and_deleted_with_their_tasks() {
    let fixture = Fixture::new();
    let read = fixture.call("workspace_board_columns", json!({ "board": PROJECTS }));
    assert_eq!(read["source"], json!("builtin"));
    assert_eq!(
        column_ids(&read),
        vec!["backlog", "in-progress", "this-week", "waiting-on", "today"]
    );

    let card = fixture.add(PROJECTS, "Draft", None);
    fixture.call(
        "workspace_task_update",
        json!({ "task_id": card, "kanban_column": "in-progress" }),
    );

    let added = fixture.call(
        "workspace_board_column_set",
        json!({
            "board": PROJECTS,
            "title": "Urgent",
            "position": 0,
            "rules": [{ "field": "tag", "op": "has", "value": "urgent" }],
        }),
    );
    assert_eq!(added["source"], json!("own"));
    assert_eq!(added["column_id"], json!("urgent"));
    assert_eq!(column_ids(&added)[0], "urgent");
    assert_eq!(
        added["columns"][0]["rules"]["rules"][0]["value"],
        json!("urgent")
    );

    // The default is untouched: another board still follows it.
    let everything = fixture.call("workspace_board_columns", json!({}));
    assert_eq!(everything["source"], json!("builtin"));

    let steps = fixture.journal().len();
    let deleted = fixture.call(
        "workspace_board_column_delete",
        json!({ "board": PROJECTS, "column_id": "in-progress", "move_to": "this-week" }),
    );
    assert_eq!(deleted["moved_to"], json!("this-week"));
    assert!(!column_ids(&deleted).contains(&"in-progress".to_string()));
    assert_eq!(fixture.journal().len(), steps + 1);
    let filed: String = fixture
        .db()
        .query_row(
            "SELECT kanbanColumn FROM task_metadata WHERE taskId = ?1",
            [&card],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(filed, "this-week");

    let refused = fixture.try_call(
        "workspace_board_column_delete",
        json!({ "board": PROJECTS, "column_id": "nope" }),
    );
    assert!(refused.is_err());
}

// -- filter lists -------------------------------------------------------------

fn filter_ids(payload: &Value) -> Vec<String> {
    ids(&payload["filter_lists"])
}

#[test]
fn a_filter_list_is_created_listed_edited_and_deleted() {
    let fixture = Fixture::new();
    let before = fixture.call("workspace_filter_lists", json!({}));
    assert_eq!(
        filter_ids(&before),
        vec!["builtin:today", "builtin:everything"]
    );
    assert_eq!(before["filter_lists"][0]["builtin"], json!(true));

    let steps = fixture.journal().len();
    let created = fixture.call(
        "workspace_filter_list_create",
        json!({
            "name": "  Reports  ",
            "icon": "doc.text",
            "rules": [{ "field": "text", "op": "contains", "value": " report " }],
        }),
    );
    let id = created["id"].as_str().expect("id").to_string();
    assert_eq!(created["name"], json!("Reports"));
    assert_eq!(created["icon"], json!("doc.text"));
    assert_eq!(created["builtin"], json!(false));
    assert_eq!(created["match"], json!("all"));
    assert_eq!(created["show"], json!("open"));
    // The core tidies the value it stores.
    assert_eq!(created["rules"][0]["value"], json!("report"));
    assert_eq!(
        created["summary"],
        json!("Open tasks where title or notes contains report")
    );

    let listed = fixture.call("workspace_filter_lists", json!({}));
    assert_eq!(listed["count"], json!(3));
    assert_eq!(filter_ids(&listed)[2], id);

    // Only what is named changes; the rules replace the whole set.
    let edited = fixture.call(
        "workspace_filter_list_update",
        json!({
            "filter_list_id": id,
            "match": "any",
            "rules": [
                { "field": "list", "op": "in", "value": PROJECTS },
                { "field": "priority", "op": "at-least", "value": 2 },
            ],
        }),
    );
    assert_eq!(edited["name"], json!("Reports"));
    assert_eq!(edited["icon"], json!("doc.text"));
    assert_eq!(edited["match"], json!("any"));
    assert_eq!(edited["rules"].as_array().map(Vec::len), Some(2));
    assert_eq!(edited["rules"][1]["value"], json!("2"));

    // A present-but-empty icon clears it.
    let cleared = fixture.call(
        "workspace_filter_list_update",
        json!({ "filter_list_id": id, "icon": "" }),
    );
    assert_eq!(cleared["icon"], Value::Null);

    let deleted = fixture.call(
        "workspace_filter_list_delete",
        json!({ "filter_list_id": id }),
    );
    assert_eq!(deleted["deleted"], json!(id));
    assert_eq!(deleted["name"], json!("Reports"));
    assert_eq!(
        filter_ids(&fixture.call("workspace_filter_lists", json!({}))).len(),
        2
    );

    let journal = fixture.journal();
    assert_eq!(
        journal[steps..]
            .iter()
            .map(|(label, _)| label.as_str())
            .collect::<Vec<_>>(),
        [
            "MCP: New Filter List",
            "MCP: Edit Filter List",
            "MCP: Edit Filter List",
            "MCP: Delete Filter List",
        ]
    );
}

#[test]
fn a_filter_list_shows_the_tasks_its_rules_match_across_lists() {
    let fixture = Fixture::new();
    let report = fixture.add(PROJECTS, "Write the report", None);
    let milk = fixture.add(PROJECTS, "Buy milk", None);
    let inbox = fixture.add(INBOX, "Report back", None);
    fixture.call(
        "workspace_filter_list_create",
        json!({
            "name": "Reports",
            "rules": [{ "field": "text", "op": "contains", "value": "report" }],
        }),
    );

    // By name, in any case; the inbox's tasks come first.
    let shown = fixture.call(
        "workspace_filter_list_tasks",
        json!({ "filter_list": "reports" }),
    );
    assert_eq!(shown["filter_list"]["name"], json!("Reports"));
    assert_eq!(shown["task_count"], json!(2));
    assert_eq!(ids(&shown["tasks"]), vec![inbox.clone(), report.clone()]);
    assert_eq!(shown["tasks"][0]["list_name"], json!("Inbox"));
    assert_eq!(shown["tasks"][1]["list_name"], json!("Projects"));
    assert_eq!(shown["tasks"][1]["title"], json!("Write the report"));

    // A finished task drops out of an open filter, and is back under all.
    fixture.call(
        "workspace_task_update",
        json!({ "task_id": report, "status": "completed" }),
    );
    let shown = fixture.call(
        "workspace_filter_list_tasks",
        json!({ "filter_list": "Reports" }),
    );
    assert_eq!(ids(&shown["tasks"]), vec![inbox.clone()]);

    // The built-in lists are evaluated the same way, by id or name.
    let everything = fixture.call(
        "workspace_filter_list_tasks",
        json!({ "filter_list": "builtin:everything" }),
    );
    assert_eq!(ids(&everything["tasks"]), vec![inbox.clone(), milk.clone()]);
    fixture.call(
        "workspace_task_update",
        json!({ "task_id": milk, "kanban_column": "today" }),
    );
    let today = fixture.call(
        "workspace_filter_list_tasks",
        json!({ "filter_list": "Today" }),
    );
    assert_eq!(today["filter_list"]["id"], json!("builtin:today"));
    assert_eq!(ids(&today["tasks"]), vec![milk]);

    let error = fixture
        .try_call(
            "workspace_filter_list_tasks",
            json!({ "filter_list": "Nowhere" }),
        )
        .unwrap_err();
    assert!(error.contains("No filter list \"Nowhere\""), "{error}");
}

#[test]
fn a_bad_filter_list_write_is_refused_and_leaves_nothing_behind() {
    let fixture = Fixture::new();
    let created = fixture.call("workspace_filter_list_create", json!({ "name": "Kept" }));
    let id = created["id"].as_str().expect("id").to_string();
    let before = fixture.journal();

    for (arguments, expected) in [
        (
            json!({ "name": "X", "rules": [{ "field": "colour", "op": "is", "value": "red" }] }),
            "There is no filter field \"colour\"",
        ),
        (
            json!({ "name": "X", "rules": [{ "field": "due", "op": "within" }] }),
            "needs a whole number",
        ),
        (json!({ "name": "X", "match": "some" }), "is not all or any"),
        (
            json!({ "name": "X", "rules": "tag:has:work" }),
            "rules must be an array",
        ),
        (json!({ "name": "   " }), "name"),
    ] {
        let error = fixture
            .try_call("workspace_filter_list_create", arguments)
            .unwrap_err();
        assert!(error.contains(expected), "{expected}: {error}");
    }
    for (tool, arguments, expected) in [
        (
            "workspace_filter_list_update",
            json!({ "filter_list_id": "builtin:today", "name": "Mine" }),
            "built in",
        ),
        (
            "workspace_filter_list_delete",
            json!({ "filter_list_id": "builtin:everything" }),
            "built in",
        ),
        (
            "workspace_filter_list_update",
            json!({ "filter_list_id": "MISSING", "name": "Mine" }),
            "No filter list with id MISSING",
        ),
        (
            "workspace_filter_list_delete",
            json!({ "filter_list_id": "MISSING" }),
            "No filter list with id MISSING",
        ),
        (
            "workspace_filter_list_update",
            json!({ "filter_list_id": id, "show": "later" }),
            "is not open, done or all",
        ),
        (
            "workspace_filter_list_update",
            json!({ "filter_list_id": id }),
            "No updates provided",
        ),
    ] {
        let error = fixture.try_call(tool, arguments).unwrap_err();
        assert!(error.contains(expected), "{tool} {expected}: {error}");
    }

    assert_eq!(fixture.journal(), before);
    let rows: i64 = fixture.scalar("SELECT COUNT(*) FROM filter_lists WHERE id <> ?1", "");
    assert_eq!(rows, 1);
}

#[test]
fn a_workspace_without_filter_lists_reads_the_built_ins_and_refuses_writes() {
    let fixture = Fixture::new();
    let task = fixture.add(PROJECTS, "Old", None);
    fixture
        .db()
        .execute_batch(
            "DROP TABLE filter_lists;
             DELETE FROM grdb_migrations WHERE identifier = 'v23_filter_lists';",
        )
        .unwrap();

    let listed = fixture.call("workspace_filter_lists", json!({}));
    assert_eq!(
        filter_ids(&listed),
        vec!["builtin:today", "builtin:everything"]
    );
    let everything = fixture.call(
        "workspace_filter_list_tasks",
        json!({ "filter_list": "Everything" }),
    );
    assert_eq!(ids(&everything["tasks"]), vec![task]);

    for (tool, arguments) in [
        ("workspace_filter_list_create", json!({ "name": "New" })),
        (
            "workspace_filter_list_update",
            json!({ "filter_list_id": "ANY", "name": "New" }),
        ),
        (
            "workspace_filter_list_delete",
            json!({ "filter_list_id": "ANY" }),
        ),
    ] {
        let error = fixture.try_call(tool, arguments).unwrap_err();
        assert!(error.contains("predates filter lists"), "{tool}: {error}");
    }
}

// -- the command line reaches the same tools ----------------------------------

#[test]
fn the_ws_subcommands_resolve_to_the_workspace_tools() {
    use crate::cli::{Cli, resolve};
    use clap::Parser;

    let resolved = |args: &[&str]| {
        let cli = Cli::try_parse_from(std::iter::once("priority").chain(args.iter().copied()))
            .unwrap_or_else(|error| panic!("{args:?}: {error}"));
        resolve(&cli).unwrap()
    };

    let (name, arguments) = resolved(&[
        "ws",
        "add",
        "Read",
        "the",
        "paper",
        "--parent",
        "P",
        "--link",
        "https://a",
        "--link",
        "obsidian://b",
        "-c",
        "today",
    ]);
    assert_eq!(name, "workspace_task_add");
    assert_eq!(arguments["title"], json!("Read the paper"));
    assert_eq!(arguments["parent_task_id"], json!("P"));
    assert_eq!(
        arguments["external_links"],
        json!(["https://a", "obsidian://b"])
    );
    assert_eq!(arguments["kanban_column"], json!("today"));

    let (name, arguments) = resolved(&["ws", "done", "T"]);
    assert_eq!(name, "workspace_task_update");
    assert_eq!(arguments["status"], json!("completed"));

    let (name, arguments) = resolved(&["ws", "update", "T", "--no-links"]);
    assert_eq!(name, "workspace_task_update");
    assert_eq!(arguments["external_links"], json!([]));

    let (name, arguments) = resolved(&[
        "ws",
        "update",
        "T",
        "--due",
        "2026-10-17",
        "-p",
        "1",
        "--start",
        "",
        "-e",
        "30",
    ]);
    assert_eq!(name, "workspace_task_update");
    assert_eq!(arguments["due"], json!("2026-10-17"));
    assert_eq!(arguments["priority"], json!(1));
    assert_eq!(arguments["start"], json!(""));
    assert_eq!(arguments["estimate_minutes"], json!(30));

    // The global --list-id is Checkvist's, and must not leak in.
    let (name, arguments) = resolved(&["-l", "123", "ws", "to-list", "T", "-f", "F"]);
    assert_eq!(name, "workspace_task_to_list");
    assert_eq!(arguments.get("list_id"), None);
    assert_eq!(arguments["folder_id"], json!("F"));

    let (name, arguments) = resolved(&["ws", "block", "T", "B"]);
    assert_eq!(name, "workspace_task_blocker_add");
    assert_eq!(arguments["task_id"], json!("T"));
    assert_eq!(arguments["blocked_by_task_id"], json!("B"));
    let (name, arguments) = resolved(&["ws", "unblock", "T", "B"]);
    assert_eq!(name, "workspace_task_blocker_remove");
    assert_eq!(arguments["blocked_by_task_id"], json!("B"));

    let (name, arguments) = resolved(&[
        "ws",
        "filter-new",
        "Due",
        "work",
        "-m",
        "any",
        "-r",
        "tag:has:work",
        "-r",
        "due:within:7",
        "-r",
        "text:contains:a:b",
        "-r",
        "due:overdue",
    ]);
    assert_eq!(name, "workspace_filter_list_create");
    assert_eq!(arguments["name"], json!("Due work"));
    assert_eq!(arguments["match"], json!("any"));
    assert_eq!(
        arguments["rules"],
        json!([
            { "field": "tag", "op": "has", "value": "work" },
            { "field": "due", "op": "within", "value": "7" },
            { "field": "text", "op": "contains", "value": "a:b" },
            { "field": "due", "op": "overdue", "value": "" },
        ])
    );
    let (_, arguments) = resolved(&[
        "ws",
        "filter-new",
        "J",
        "--rules",
        r#"[{"field":"tag","op":"any"}]"#,
    ]);
    assert_eq!(arguments["rules"], json!([{ "field": "tag", "op": "any" }]));

    let (name, arguments) = resolved(&["ws", "filter-edit", "F", "--no-rules", "-s", "all"]);
    assert_eq!(name, "workspace_filter_list_update");
    assert_eq!(arguments["filter_list_id"], json!("F"));
    assert_eq!(arguments["rules"], json!([]));
    assert_eq!(arguments["show"], json!("all"));
    assert_eq!(arguments.get("name"), None);

    let (name, arguments) = resolved(&["ws", "filter", "Due", "work"]);
    assert_eq!(name, "workspace_filter_list_tasks");
    assert_eq!(arguments["filter_list"], json!("Due work"));

    for bad in [
        &["ws", "filter-new", "X", "-r", "tag"][..],
        &["ws", "filter-new", "X", "--rules", "{}"][..],
        &["ws", "filter-new", "X", "--rules", "nope"][..],
    ] {
        let cli = Cli::try_parse_from(std::iter::once("priority").chain(bad.iter().copied()))
            .expect("parses");
        assert!(resolve(&cli).is_err(), "{bad:?}");
    }

    for (args, tool) in [
        (&["ws", "filters"][..], "workspace_filter_lists"),
        (
            &["ws", "filter-rm", "F"][..],
            "workspace_filter_list_delete",
        ),
        (&["ws", "tree"][..], "workspace_tree"),
        (&["ws", "tasks", "L"][..], "workspace_tasks"),
        (
            &["ws", "move", "T", "--position", "2"][..],
            "workspace_task_move",
        ),
        (&["ws", "rm", "T"][..], "workspace_task_delete"),
        (&["ws", "deps", "T"][..], "workspace_task_dependencies"),
        (&["ws", "new-folder", "F"][..], "workspace_folder_create"),
        (&["ws", "new-list", "L"][..], "workspace_list_create"),
        (&["ws", "move-list", "L"][..], "workspace_list_move"),
    ] {
        assert_eq!(resolved(args).0, tool, "{args:?}");
    }
}

#[test]
fn agent_instructions_are_read_set_appended_and_each_write_is_one_undo_step() {
    let fixture = Fixture::new();
    let empty = fixture.call("agent_instructions_get", json!({}));
    assert_eq!(empty["instructions"], json!([]));

    let saved = fixture.call(
        "agent_instructions_set",
        json!({ "scope": "list", "id": PROJECTS, "text": "  Keep titles short.  " }),
    );
    assert_eq!(saved["text"], json!("Keep titles short."));
    assert_eq!(saved["previous"], json!(""));
    assert_eq!(saved["name"], json!("Projects"));
    assert_eq!(saved["changed"], json!(true));

    let appended = fixture.call(
        "agent_instructions_set",
        json!({ "scope": "list", "id": PROJECTS, "text": "Due dates on Fridays.", "mode": "append" }),
    );
    assert_eq!(appended["previous"], json!("Keep titles short."));
    assert_eq!(
        appended["text"],
        json!("Keep titles short.\n\nDue dates on Fridays.")
    );

    fixture.call(
        "agent_instructions_set",
        json!({ "scope": "account", "text": "I work in British English." }),
    );
    let folder = fixture.call(
        "agent_instructions_set",
        json!({ "scope": "folder", "id": FOLDER, "text": "Modules by code." }),
    );
    assert_eq!(folder["name"], json!("Computer Science"));

    let all = fixture.call("agent_instructions_get", json!({}));
    let scopes: Vec<&str> = all["instructions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|instruction| instruction["scope"].as_str().unwrap())
        .collect();
    assert_eq!(scopes, ["account", "folder", "list"]);
    let one = fixture.call(
        "agent_instructions_get",
        json!({ "scope": "list", "id": PROJECTS }),
    );
    assert_eq!(one["instructions"][0]["text"], appended["text"]);

    // Saving what is there already is not a step.
    let unchanged = fixture.call(
        "agent_instructions_set",
        json!({ "scope": "account", "text": "I work in British English." }),
    );
    assert_eq!(unchanged["changed"], json!(false));
    let steps: Vec<String> = fixture
        .journal()
        .into_iter()
        .map(|(label, _)| label)
        .filter(|label| label == "MCP: Edit Agent Instructions")
        .collect();
    assert_eq!(steps.len(), 4);

    // Blank text clears the scope.
    fixture.call(
        "agent_instructions_set",
        json!({ "scope": "account", "text": "" }),
    );
    let account = fixture.call("agent_instructions_get", json!({ "scope": "account" }));
    assert_eq!(account["instructions"][0]["text"], json!(""));

    for (arguments, message) in [
        (json!({ "scope": "list", "text": "x" }), "needs an id"),
        (
            json!({ "scope": "account", "id": PROJECTS, "text": "x" }),
            "takes no id",
        ),
        (json!({ "scope": "shelf", "text": "x" }), "Unknown scope"),
        (
            json!({ "scope": "list", "id": "NOPE", "text": "x" }),
            "No list with id NOPE",
        ),
        (
            json!({ "scope": "list", "id": PROJECTS, "text": "x", "mode": "prepend" }),
            "Unknown mode",
        ),
    ] {
        let error = fixture
            .try_call("agent_instructions_set", arguments)
            .expect_err("refused");
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn a_write_names_its_undo_step_and_a_write_that_changes_nothing_does_not() {
    let fixture = Fixture::new();
    let added = fixture.call(
        "workspace_task_add",
        json!({ "list_id": PROJECTS, "title": "Milk" }),
    );
    let step = added["undo_step"].as_str().expect("undo_step").to_string();
    let label: String = fixture.scalar(
        "SELECT label FROM change_log WHERE groupId = ?1 LIMIT 1",
        &step,
    );
    assert_eq!(label, "MCP: New Task");

    let same = fixture.call(
        "agent_instructions_set",
        json!({ "scope": "account", "text": "" }),
    );
    assert!(same.get("undo_step").is_none(), "{same}");
    // A read never carries one, even straight after a write.
    let tree = fixture.call("workspace_tree", json!({}));
    assert!(tree.get("undo_step").is_none());
}

#[test]
fn conditions_are_listed_required_and_the_context_set() {
    let fixture = Fixture::new();
    fixture
        .db()
        .execute_batch(&format!(
            "INSERT INTO task_conditions (id, workspaceId, name, isLocation, isArchived, createdAt, updatedAt, category, sortOrder) VALUES
               ('home', '{WORKSPACE}', 'Home', 1, 0, '{EARLIER}', '{EARLIER}', 'Location', 0),
               ('campus', '{WORKSPACE}', 'Campus', 1, 0, '{EARLIER}', '{EARLIER}', 'Location', 1),
               ('mat', '{WORKSPACE}', 'Mat', 0, 0, '{EARLIER}', '{EARLIER}', 'Equipment', 2);"
        ))
        .unwrap();
    let task = fixture.add(PROJECTS, "Stretch", None);

    let set = fixture.call(
        "workspace_task_conditions_set",
        json!({ "task_id": task, "requires": [["home", "Campus"], "mat"] }),
    );
    assert_eq!(set["changed"], json!(true));
    assert_eq!(set["requires"], json!([["Home", "Campus"], ["Mat"]]));
    assert_eq!(fixture.journal().last().unwrap().0, "MCP: Edit Conditions");
    // The task read shows them by name.
    let tasks = fixture.call("workspace_tasks", json!({ "list_id": PROJECTS }));
    assert_eq!(
        tasks["tasks"][0]["requires"],
        json!([["Home", "Campus"], ["Mat"]])
    );

    let unknown = fixture
        .try_call(
            "workspace_task_conditions_set",
            json!({ "task_id": task, "requires": ["Library"] }),
        )
        .unwrap_err();
    assert!(
        unknown.contains("No condition called \"Library\""),
        "{unknown}"
    );
    let created = fixture.call(
        "workspace_task_conditions_set",
        json!({ "task_id": task, "requires": ["Library"], "create_missing": true }),
    );
    assert_eq!(created["requires"], json!([["Library"]]));

    // A condition that must not hold: "!", "not " or "!" before an id.
    let away = fixture.call(
        "workspace_task_conditions_set",
        json!({ "task_id": task, "requires": ["not home", ["!Campus", "!mat"], "Library"] }),
    );
    assert_eq!(
        away["requires"],
        json!([["not Home"], ["not Campus", "not Mat"], ["Library"]])
    );
    let stored: String = fixture
        .db()
        .query_row(
            "SELECT planningJSON FROM task_metadata WHERE taskId = ?1",
            [&task],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        stored.contains("[\"!home\"],[\"!campus\",\"!mat\"]"),
        "{stored}"
    );
    let needing_home = fixture.call("workspace_conditions", json!({ "condition": "Home" }));
    assert_eq!(needing_home["tasks"][0]["title"], json!("Stretch"));
    fixture.call(
        "workspace_task_conditions_set",
        json!({ "task_id": task, "requires": ["Library"] }),
    );

    let read = fixture.call("workspace_conditions", json!({}));
    assert_eq!(read["sections"][0]["category"], json!("Location"));
    assert_eq!(read["sections"][0]["one_at_a_time"], json!(true));
    let library = read["sections"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|section| section["conditions"].as_array().unwrap().clone())
        .find(|condition| condition["name"] == json!("Library"))
        .unwrap();
    assert_eq!(library["open_tasks"], json!(1));
    assert_eq!(read["context"]["anywhere"], json!(true));
    let needing = fixture.call("workspace_conditions", json!({ "condition": "library" }));
    assert_eq!(needing["tasks"][0]["title"], json!("Stretch"));

    // One place at a time; capabilities combine; the stamp comes with it.
    fixture.call("workspace_context_set", json!({ "on": ["Home", "Mat"] }));
    let moved = fixture.call(
        "workspace_context_set",
        json!({ "on": ["campus"], "available_minutes": 30, "mode": "finish" }),
    );
    let names: Vec<&str> = moved["on"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Campus", "Mat"]);
    assert_eq!(moved["mode"], json!("finish"));
    assert!(moved["available_until"].is_string());
    assert!(moved["set_at"].is_string());
    assert_eq!(moved["stale"], json!(false));
    // Saying where you are is not an undo step.
    assert_eq!(fixture.journal().last().unwrap().0, "MCP: Edit Conditions");
    let cleared = fixture.call("workspace_context_set", json!({ "clear": true }));
    assert_eq!(cleared["anywhere"], json!(true));
    assert!(
        fixture
            .try_call("workspace_context_set", json!({ "mode": "rush" }))
            .is_err()
    );
}

#[test]
fn a_setting_provides_conditions_and_one_can_be_switched_off_for_now() {
    let fixture = Fixture::new();
    fixture
        .db()
        .execute_batch(&format!(
            "INSERT INTO task_conditions (id, workspaceId, name, isLocation, isArchived, createdAt, updatedAt, category, sortOrder) VALUES
               ('home', '{WORKSPACE}', 'Home', 1, 0, '{EARLIER}', '{EARLIER}', 'Location', 0),
               ('library', '{WORKSPACE}', 'Library', 1, 0, '{EARLIER}', '{EARLIER}', 'Location', 1),
               ('floor', '{WORKSPACE}', 'Floor space', 0, 0, '{EARLIER}', '{EARLIER}', NULL, 2),
               ('quiet', '{WORKSPACE}', 'Quiet', 0, 0, '{EARLIER}', '{EARLIER}', NULL, 3);"
        ))
        .unwrap();
    let set = fixture.call(
        "workspace_condition_provides_set",
        json!({ "condition": "Home", "provides": ["floor space", "Quiet", "Desk"], "create_missing": true }),
    );
    let names: Vec<&str> = set["provides"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Floor space", "Quiet", "Desk"]);
    assert_eq!(fixture.journal().last().unwrap().0, "MCP: Edit Condition");
    let itself = fixture
        .try_call(
            "workspace_condition_provides_set",
            json!({ "condition": "Home", "provides": ["Home"] }),
        )
        .unwrap_err();
    assert!(itself.contains("cannot provide itself"), "{itself}");

    let read = fixture.call("workspace_conditions", json!({}));
    assert_eq!(read["sections"][0]["setting"], json!(true));
    // Saying Home: what it provides holds too, with where it comes from.
    let home = fixture.call("workspace_context_set", json!({ "on": ["Home"] }));
    let holds = |context: &Value| -> Vec<String> {
        context["holds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(holds(&home), vec!["Home", "Floor space", "Quiet", "Desk"]);
    assert_eq!(home["holds"][1]["from"], json!(["Home"]));
    assert_eq!(home["holds"][1]["said"], json!(false));
    // Housemates in: Quiet off for now, and it stays off while Home holds.
    let noisy = fixture.call("workspace_context_set", json!({ "off": ["Quiet"] }));
    assert_eq!(holds(&noisy), vec!["Home", "Floor space", "Desk"]);
    assert_eq!(noisy["off_for_now"][0]["name"], json!("Quiet"));
    let timed = fixture.call("workspace_context_set", json!({ "available_minutes": 30 }));
    assert_eq!(timed["off_for_now"][0]["name"], json!("Quiet"));
    // Somewhere else, and back: the override went with Home.
    fixture.call("workspace_context_set", json!({ "on": ["Library"] }));
    let back = fixture.call("workspace_context_set", json!({ "on": ["Home"] }));
    assert_eq!(holds(&back), vec!["Home", "Floor space", "Quiet", "Desk"]);
    assert_eq!(back["off_for_now"], json!([]));
}

#[test]
fn each_day_starts_with_the_conditions_set_for_it() {
    let fixture = Fixture::new();
    fixture
        .db()
        .execute_batch(&format!(
            "INSERT INTO task_conditions (id, workspaceId, name, isLocation, isArchived, createdAt, updatedAt, category, sortOrder) VALUES
               ('home', '{WORKSPACE}', 'Home', 1, 0, '{EARLIER}', '{EARLIER}', 'Location', 0),
               ('library', '{WORKSPACE}', 'Library', 1, 0, '{EARLIER}', '{EARLIER}', 'Location', 1),
               ('laptop', '{WORKSPACE}', 'Laptop', 0, 0, '{EARLIER}', '{EARLIER}', NULL, 2);"
        ))
        .unwrap();
    // Nothing set: the defaults, and reading writes nothing.
    let read = fixture.call("workspace_day_start_set", json!({}));
    assert_eq!(read["resets"], json!(false));
    assert_eq!(read["hour"], json!(4));
    assert_eq!(read["changed"], json!(false));

    // Two places named: the last is kept, as saying where you are keeps one.
    let set = fixture.call(
        "workspace_day_start_set",
        json!({ "conditions": ["Library", "laptop", "Home"], "hour": 5 }),
    );
    assert_eq!(set["resets"], json!(true));
    assert_eq!(set["hour"], json!(5));
    let names: Vec<&str> = set["starts_with"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Home", "Laptop"]);
    // Today counts as started: setting it reset nothing.
    assert!(set["last_reset_day"].is_string());
    let here = fixture.call("workspace_context_set", json!({ "on": ["Library"] }));
    assert_eq!(here["on"][0]["name"], json!("Library"));
    assert_eq!(here["day_start"]["resets"], json!(true));

    // As if the last reset was yesterday and Library was said then: the next
    // read starts the day at Home.
    fixture
        .db()
        .execute_batch(
            "UPDATE preferences SET value = '2000-01-01' WHERE key = 'dayStart.lastReset';
             UPDATE focus_contexts SET updatedAt = '2000-01-01 12:00:00.000';",
        )
        .unwrap();
    let morning = fixture.call("workspace_conditions", json!({}));
    let on: Vec<&str> = morning["context"]["on"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(on, vec!["Home", "Laptop"]);
    assert_ne!(
        morning["context"]["day_start"]["last_reset_day"],
        json!("2000-01-01")
    );

    let wrong = fixture
        .try_call("workspace_day_start_set", json!({ "hour": 24 }))
        .unwrap_err();
    assert!(wrong.contains("0 to 23"), "{wrong}");
    let off = fixture.call("workspace_day_start_set", json!({ "resets": false }));
    assert_eq!(off["resets"], json!(false));
    assert_eq!(
        off["starts_with"][0]["name"],
        json!("Home"),
        "kept for next time"
    );
}

#[test]
fn clearing_who_and_when_drops_a_repeating_follow_up_too() {
    let fixture = Fixture::new();
    let task = fixture.add(PROJECTS, "Contract signed", None);
    let updated = fixture.call(
        "workspace_task_update",
        json!({ "task_id": task, "waiting_on": "Sam", "follow_up_at": "2099-10-08 14:00", "follow_up_every_days": 3 }),
    );
    assert_eq!(updated["follow_up_every_days"], json!(3));
    let updated = fixture.call(
        "workspace_task_update",
        json!({ "task_id": task, "waiting_on": null, "follow_up_at": null }),
    );
    assert_eq!(updated.get("waiting_on"), None);
    assert_eq!(updated.get("follow_up_at"), None);
    assert_eq!(updated.get("follow_up_every_days"), None);

    // Naming a repeat while clearing the time is still refused.
    fixture.call(
        "workspace_task_update",
        json!({ "task_id": task, "waiting_on": "Sam", "follow_up_at": "2099-10-08 14:00", "follow_up_every_days": 3 }),
    );
    let error = fixture
        .try_call(
            "workspace_task_update",
            json!({ "task_id": task, "follow_up_at": null, "follow_up_every_days": 5 }),
        )
        .unwrap_err();
    assert!(error.contains("has no follow_up_at"), "{error}");
    assert!(!error.contains("  "), "{error}");
}

#[test]
fn a_column_in_the_same_call_wins_and_clearing_who_and_when_stops_waiting() {
    let fixture = Fixture::new();
    let task = fixture.add(PROJECTS, "Contract signed", None);
    let updated = fixture.call(
        "workspace_task_update",
        json!({ "task_id": task, "waiting_on": "Sam", "kanban_column": "today" }),
    );
    assert_eq!(updated["kanban_column"], json!("today"));
    assert_eq!(updated["waiting_on"], json!("Sam"));

    let updated = fixture.call(
        "workspace_task_update",
        json!({ "task_id": task, "kanban_column": "waiting-on" }),
    );
    assert_eq!(updated["kanban_column"], json!("waiting-on"));
    let updated = fixture.call(
        "workspace_task_update",
        json!({ "task_id": task, "waiting_on": null, "follow_up_at": null }),
    );
    assert_eq!(updated["kanban_column"], Value::Null);

    // A repeat with no follow-up time to repeat is refused, not dropped.
    let error = fixture
        .try_call(
            "workspace_task_update",
            json!({ "task_id": task, "follow_up_every_days": 3 }),
        )
        .unwrap_err();
    assert!(error.contains("follow_up_at"), "{error}");
}

#[test]
fn a_tasks_agent_context_is_the_task_scope_and_not_its_notes() {
    let fixture = Fixture::new();
    let task = fixture.add(PROJECTS, "Essay", None);
    let empty = fixture.call(
        "agent_instructions_get",
        json!({ "scope": "task", "id": task }),
    );
    assert_eq!(empty["text"], json!(""));
    let saved = fixture.call(
        "agent_instructions_set",
        json!({ "scope": "task", "id": task, "text": "Marked by Dr Lee; she wants citations." }),
    );
    assert_eq!(saved["changed"], json!(true));
    let appended = fixture.call(
        "agent_instructions_set",
        json!({ "scope": "task", "id": task, "text": "Harvard style.", "mode": "append" }),
    );
    assert_eq!(
        appended["text"],
        json!("Marked by Dr Lee; she wants citations.\n\nHarvard style.")
    );
    let notes: String = fixture.scalar("SELECT notes FROM tasks WHERE id = ?1", &task);
    assert_eq!(notes, "");
    let undoable: String = fixture.scalar(
        "SELECT label FROM change_log WHERE tableName = 'task_metadata' AND rowId = ?1
           ORDER BY rowid DESC LIMIT 1",
        &task,
    );
    assert!(undoable.contains("Edit Agent Context"), "{undoable}");
}

#[test]
fn a_task_in_a_shared_list_is_assigned_and_says_who_did_it() {
    let fixture = Fixture::new();
    fixture
        .db()
        .execute_batch(&format!(
            "INSERT INTO sync_spaces (spaceId, listId, role, name, membersJSON) VALUES ('s', '{PROJECTS}', 'owner', 'Projects',
               '[{{\"accountId\":\"me\",\"email\":\"me@example.com\",\"role\":\"owner\",\"you\":true}},
                 {{\"accountId\":\"sam\",\"email\":\"sam@example.com\",\"role\":\"member\",\"you\":false}}]');"
        ))
        .expect("share");
    let id = fixture.call(
        "workspace_task_add",
        json!({ "list_id": PROJECTS, "title": "Buy milk" }),
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "assignee": "SAM@example.com", "status": "completed" }),
    );
    let tree = fixture.call(
        "workspace_tasks",
        json!({ "list_id": PROJECTS, "include_closed": true }),
    );
    let task = &tree["tasks"][0];
    assert_eq!(task["assignee"], json!("sam@example.com"));
    assert_eq!(task["completed_by"], json!("you"));
    assert_eq!(task["added_by"], json!("you"));

    assert!(
        fixture
            .try_call(
                "workspace_task_update",
                json!({ "task_id": id, "assignee": "stranger@example.com" })
            )
            .unwrap_err()
            .contains("isn't in this list")
    );
    fixture.call(
        "workspace_task_update",
        json!({ "task_id": id, "assignee": null }),
    );
    let tree = fixture.call(
        "workspace_tasks",
        json!({ "list_id": PROJECTS, "include_closed": true }),
    );
    assert!(tree["tasks"][0].get("assignee").is_none());
}
