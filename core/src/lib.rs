//! Takt's shared core.
//!
//! Every client — the Mac and iPhone apps through Swift, the Android app
//! through Kotlin, the CLI and the sync server as a Rust dependency — will call
//! this one implementation of the workspace store instead of keeping its own
//! copy. Nothing here knows about a UI toolkit. The migration is incremental;
//! `docs/rust-core-migration.md` is the plan and records which behaviour has
//! moved so far.

uniffi::setup_scaffolding!();

pub mod agent_hints;
pub mod agent_instructions;
pub mod agent_tools;
pub mod board;
pub mod board_columns;
pub mod board_rules;
pub mod buried;
pub mod capture;
pub mod checkvist;
pub mod client_config;
pub mod command;
pub mod commitments;
pub mod conditions;
pub mod conversions;
pub mod dailies;
pub mod day_log;
pub mod day_start;
pub mod dependencies;
pub mod editor;
pub mod editor_draft;
pub mod error;
pub mod export;
pub mod filter_lists;
pub mod focus;
pub mod focus_context;
pub mod google_calendar;
pub mod google_tasks;
pub mod habits;
pub mod imports;
pub mod journal;
pub mod keymap;
pub mod links;
pub mod lists;
pub mod next_up;
pub mod packed_rows;
pub mod palette;
pub mod periodic;
pub mod planning;
pub mod progress;
pub mod ranking;
pub mod reads;
pub mod records;
pub mod recurrence;
pub mod reminders;
pub mod review;
pub mod rows;
pub mod schema;
pub mod search;
pub mod setup;
pub mod shaping;
pub mod sidebar;
mod swift_text;
pub mod sync;
pub mod task_work;
pub mod tasks;
pub mod theme;
pub mod time;
pub mod today;
pub mod waiting;
pub mod workspace;

pub use error::CoreError;

/// The version of this crate, as compiled into the library a client loaded.
///
/// The first call across the boundary, and the one a client shows in its
/// diagnostics: when a platform's bindings and its compiled library drift
/// apart, this is the number that says which library it actually has.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_is_the_crate_version() {
        assert_eq!(core_version(), env!("CARGO_PKG_VERSION"));
        assert!(!core_version().is_empty());
    }

    #[test]
    fn data_version_moves_for_other_connections_and_not_for_the_handles_own() {
        let path = std::env::temp_dir().join(format!(
            "takt-data-version-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path_text = path.to_string_lossy().to_string();
        schema::migrate_workspace(path_text.clone()).unwrap();
        let core = workspace::CoreWorkspace::open(path_text.clone()).unwrap();
        let start = core.data_version().unwrap();
        let workspace_id = core.bootstrap(1_000).unwrap();
        assert_eq!(core.data_version().unwrap(), start, "its own write");
        // The reading connection sees the handle's own commit straight away,
        // and reading through it moves nothing.
        let changes = core.own_changes().unwrap();
        let inbox = core.inbox(workspace_id.clone()).unwrap().unwrap();
        let task = core
            .create_task(
                tasks::NewTask {
                    list_id: inbox.id,
                    title: "Seen".into(),
                    parent_task_id: None,
                    kind: "task".into(),
                    notes: String::new(),
                    kanban_column: None,
                    start_at_ms: None,
                    due_at_ms: None,
                    estimate_seconds: None,
                    tags: vec![],
                    priority: None,
                    waiting_on: None,
                    external_links: vec![],
                    at_top: false,
                    adjacent_task_id: None,
                    above: false,
                },
                2_000,
            )
            .unwrap();
        assert_eq!(core.tasks_by_id(vec![task.clone()]).unwrap().len(), 1);
        let read = core
            .next_up(3_000, "UTC".into(), Default::default(), None, None)
            .unwrap();
        assert!(read.ranked.iter().any(|s| s.candidate.id == task));
        assert_eq!(core.data_version().unwrap(), start, "its own reads");
        let after_write = core.own_changes().unwrap();
        assert!(after_write > changes, "its own write is counted");
        assert_eq!(core.waiting_metadata().unwrap().len(), 0);
        assert_eq!(core.own_changes().unwrap(), after_write, "a read is not");
        let other = rusqlite::Connection::open(&path).unwrap();
        other
            .execute("UPDATE workspaces SET name = 'Elsewhere'", [])
            .unwrap();
        assert!(core.data_version().unwrap() > start, "someone else's write");
        let mode: String = other
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        drop(other);
        drop(core);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{path_text}{suffix}"));
        }
    }

    #[test]
    fn a_list_read_leaves_out_a_commitment_ticked_today_until_the_tick_is_undone() {
        let path = std::env::temp_dir().join(format!(
            "takt-resting-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path_text = path.to_string_lossy().to_string();
        schema::migrate_workspace(path_text.clone()).unwrap();
        let core = workspace::CoreWorkspace::open(path_text.clone()).unwrap();
        // The list reads take the device's clock and zone, so this does too.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let zone = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into());
        let workspace_id = core.bootstrap(now).unwrap();
        let inbox = core.inbox(workspace_id).unwrap().unwrap().id;
        let parent = core
            .create_task(
                tasks::NewTask {
                    list_id: inbox.clone(),
                    title: "Play the drums".into(),
                    parent_task_id: None,
                    kind: "task".into(),
                    notes: String::new(),
                    kanban_column: None,
                    start_at_ms: None,
                    due_at_ms: None,
                    estimate_seconds: None,
                    tags: vec![],
                    priority: None,
                    waiting_on: None,
                    external_links: vec![],
                    at_top: false,
                    adjacent_task_id: None,
                    above: false,
                },
                now,
            )
            .unwrap();
        let practice = core
            .save_commitment(
                parent.clone(),
                commitments::CommitmentDraft {
                    title: "Practice".into(),
                    rule: "daily".into(),
                    expires_at_ms: None,
                    stacks_missed: false,
                },
                None,
                now,
                zone.clone(),
            )
            .unwrap();
        let ids = |rows: Vec<records::TaskRow>| rows.into_iter().map(|t| t.id).collect::<Vec<_>>();
        assert!(ids(core.tasks_in_lists(vec![inbox.clone()]).unwrap()).contains(&practice));

        core.set_status(practice.clone(), "completed".into(), now, zone)
            .unwrap();
        assert!(!ids(core.tasks_in_lists(vec![inbox.clone()]).unwrap()).contains(&practice));
        assert!(
            !ids(core
                .child_tasks(inbox.clone(), Some(parent.clone()))
                .unwrap())
            .contains(&practice)
        );
        assert!(
            core.outline(inbox.clone(), None)
                .unwrap()
                .iter()
                .all(|item| item.task.id != practice)
        );
        // Its parent still says it is committed to.
        assert_eq!(core.commitments(parent).unwrap().len(), 1);

        core.undo().unwrap();
        assert!(ids(core.tasks_in_lists(vec![inbox]).unwrap()).contains(&practice));
        drop(core);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{path_text}{suffix}"));
        }
    }
}
