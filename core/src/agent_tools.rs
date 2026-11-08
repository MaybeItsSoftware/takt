//! Which of the MCP server's tools only read, and which write.
//!
//! The Mac's agent panel pre-allows the reads on the assistant's command line
//! and sends every write through an approval card (`AgentToolPolicy` in
//! Swift, `docs/agent-panel.md`). The table itself is the CLI's
//! (`cli/src/tools.rs`), but the app cannot link the CLI, so the
//! classification lives here, where both can reach it: the CLI's tests hold
//! its declared tools to exactly these two lists, so a tool added there
//! without being classified here fails a test instead of drifting.
//!
//! The read list is an allow-list on purpose. A tool missing from both is
//! treated as a write by the panel, so forgetting one costs a click rather
//! than a change nobody saw.

/// The tools that change nothing: the Checkvist reads, the local day log and
/// dailies, the focus clock (opened read-only by the CLI), the workspace
/// tree and its filter lists, and the agent's standing instructions.
pub const READ_ONLY_TOOLS: &[&str] = &[
    "task_lists",
    "task_fetch",
    "task_search",
    "task_metadata",
    "daily_log_fetch",
    "dailies_list",
    "focus_status",
    "focus_history",
    "workspace_tree",
    "workspace_shared_lists",
    "workspace_tasks",
    "workspace_task_dependencies",
    "workspace_filter_lists",
    "workspace_filter_list_tasks",
    "workspace_board_columns",
    "workspace_conditions",
    "agent_instructions_get",
];

/// Every other tool the server declares. Each goes through the approval card.
pub const WRITE_TOOLS: &[&str] = &[
    "task_add",
    "task_update",
    "task_note_add",
    "task_move",
    "task_reparent",
    "project_move",
    "task_complete",
    "task_reopen",
    "task_invalidate",
    "task_delete",
    "list_create",
    "task_matrix_set",
    "daily_add",
    "daily_update",
    "daily_tick",
    "workspace_task_add",
    "workspace_task_update",
    "workspace_task_move",
    "workspace_task_to_list",
    "workspace_task_delete",
    "workspace_task_blocker_add",
    "workspace_task_blocker_remove",
    "workspace_task_commit",
    "workspace_filter_list_create",
    "workspace_filter_list_update",
    "workspace_filter_list_delete",
    "workspace_folder_create",
    "workspace_list_create",
    "workspace_list_move",
    "workspace_list_delete",
    "workspace_board_column_set",
    "workspace_board_column_delete",
    "workspace_task_conditions_set",
    "workspace_condition_provides_set",
    "workspace_context_set",
    "workspace_day_start_set",
    "agent_instructions_set",
];

/// [`READ_ONLY_TOOLS`], for the apps.
#[uniffi::export]
pub fn agent_read_only_tools() -> Vec<String> {
    READ_ONLY_TOOLS
        .iter()
        .map(|name| name.to_string())
        .collect()
}

/// [`WRITE_TOOLS`], for the apps.
#[uniffi::export]
pub fn agent_write_tools() -> Vec<String> {
    WRITE_TOOLS.iter().map(|name| name.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn the_lists_are_disjoint_and_have_no_repeats() {
        let read: HashSet<_> = READ_ONLY_TOOLS.iter().collect();
        let write: HashSet<_> = WRITE_TOOLS.iter().collect();
        assert_eq!(read.len(), READ_ONLY_TOOLS.len());
        assert_eq!(write.len(), WRITE_TOOLS.len());
        assert!(read.is_disjoint(&write));
    }
}
