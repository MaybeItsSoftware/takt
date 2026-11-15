use super::*;
use crate::agent_tools::WRITE_TOOLS;
use crate::keymap::{keymap_parse, keymap_resolve, workspace_command_catalog};
use serde_json::json;

/// The hint as one line: keys in brackets, words as they are.
fn render(tool: &str, input: Value) -> Option<String> {
    render_with(tool, input, workspace_command_catalog())
}

fn render_with(tool: &str, input: Value, commands: Vec<CommandEntry>) -> Option<String> {
    let hint = agent_key_hint(tool.to_string(), input.to_string(), commands)?;
    Some(
        hint.parts
            .iter()
            .map(|part| match part {
                AgentKeyHintPart::Key { key } => format!("[{key}]"),
                AgentKeyHintPart::Text { text } => text.clone(),
            })
            .collect::<Vec<_>>()
            .join(" "),
    )
}

const TASK: &str = "6F1C0B7E-0000-0000-0000-000000000001";
const OTHER: &str = "6F1C0B7E-0000-0000-0000-000000000002";

#[test]
fn a_due_date_is_the_due_chord_then_the_date() {
    assert_eq!(
        render(
            "mcp__takt__task_update",
            json!({"task_id": 4, "due": "friday"})
        )
        .as_deref(),
        Some("[dd] then type “friday”")
    );
    assert_eq!(
        render("task_update", json!({"task_id": 4, "due": ""})).as_deref(),
        Some("[cd]")
    );
}

#[test]
fn several_fields_are_several_steps() {
    assert_eq!(
        render(
            "task_update",
            json!({"task_id": 4, "content": "Buy oat milk", "tags": "shop"})
        )
        .as_deref(),
        Some("[ee] then type “Buy oat milk” · [tt] then type “shop”")
    );
}

#[test]
fn completing_cancelling_and_deleting_are_their_keys() {
    assert_eq!(
        render("task_complete", json!({"task_id": 1})).as_deref(),
        Some("[space]")
    );
    assert_eq!(
        render("task_reopen", json!({"task_id": 1})).as_deref(),
        Some("[space]")
    );
    assert_eq!(
        render("task_invalidate", json!({"task_id": 1})).as_deref(),
        Some("[shift+space]")
    );
    assert_eq!(
        render("task_delete", json!({"task_id": 1})).as_deref(),
        Some("[delete]")
    );
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "status": "completed"})
        )
        .as_deref(),
        Some("[space]")
    );
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "status": "cancelled"})
        )
        .as_deref(),
        Some("[shift+space]")
    );
    assert_eq!(
        render("workspace_task_delete", json!({"task_id": TASK})).as_deref(),
        Some("[delete]")
    );
}

#[test]
fn a_new_task_is_the_add_key_and_a_subtask_the_subtask_key() {
    assert_eq!(
        render(
            "workspace_task_add",
            json!({"title": "Milk", "list_id": OTHER})
        )
        .as_deref(),
        Some("[enter] then type “Milk”")
    );
    assert_eq!(
        render(
            "workspace_task_add",
            json!({"title": "Milk", "parent_task_id": TASK})
        )
        .as_deref(),
        Some("[tab] then type “Milk”")
    );
    assert_eq!(
        render("task_add", json!({"content": "Milk"})).as_deref(),
        Some("[enter] then type “Milk”")
    );
    assert_eq!(
        render(
            "workspace_task_add",
            json!({"title": "Trip", "kind": "list", "notes": "June"})
        )
        .as_deref(),
        Some("[enter] then type “Trip” · [cmd+shift+l] · [nn] then type “June”")
    );
}

#[test]
fn moving_is_mm_and_reordering_the_option_arrows() {
    assert_eq!(
        render(
            "workspace_task_move",
            json!({"task_id": TASK, "list_id": OTHER})
        )
        .as_deref(),
        Some("[mm] then choose the list")
    );
    assert_eq!(
        render(
            "workspace_task_move",
            json!({"task_id": TASK, "parent_task_id": OTHER})
        )
        .as_deref(),
        Some("[mm] then choose the new parent")
    );
    assert_eq!(
        render(
            "workspace_task_move",
            json!({"task_id": TASK, "position": 1})
        )
        .as_deref(),
        Some("[option+up] or [option+down]")
    );
    assert_eq!(
        render("task_reparent", json!({"task_id": 3, "parent_task_id": 9})).as_deref(),
        Some("[mm] then choose where it goes")
    );
    assert_eq!(
        render(
            "project_move",
            json!({"task_id": 3, "source_list_id": "1", "target_list_id": "2"})
        )
        .as_deref(),
        Some("[mm] then choose the list")
    );
    assert_eq!(
        render("task_move", json!({"task_id": 3, "position": 2})).as_deref(),
        Some("[option+up] or [option+down]")
    );
}

#[test]
fn a_blocker_is_bb_and_waiting_is_ww() {
    assert_eq!(
        render(
            "workspace_task_blocker_add",
            json!({"task_id": TASK, "blocked_by_task_id": OTHER})
        )
        .as_deref(),
        Some("[bb] then find the task it waits on")
    );
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "waiting_on": "Sam"})
        )
        .as_deref(),
        Some("[ww] then type “Sam”")
    );
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "follow_up_at": "2026-10-20 09:00"})
        )
        .as_deref(),
        Some("[ww]")
    );
}

#[test]
fn the_workspace_edit_covers_each_field_it_has_a_key_for() {
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "title": "Call the bank", "notes": ""})
        )
        .as_deref(),
        Some("[ee] then type “Call the bank” · [cn]")
    );
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "kanban_column": "doing"})
        )
        .as_deref(),
        Some("[cmd+1] then [cmd+left] or [cmd+right]")
    );
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "kind": "list"})
        )
        .as_deref(),
        Some("[cmd+shift+l]")
    );
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "pinned": true})
        )
        .as_deref(),
        Some("[cmd+option+p]")
    );
    // Links have no key, so a change to them alone has no hint.
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "external_links": ["https://a.b"]})
        ),
        None
    );
}

#[test]
fn due_start_priority_and_estimate_are_their_keys() {
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "due": "2026-10-17"})
        )
        .as_deref(),
        Some("[dd] then type “2026-10-17”")
    );
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "due": null})
        )
        .as_deref(),
        Some("[cd]")
    );
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "priority": 1})
        )
        .as_deref(),
        Some("[1]")
    );
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "priority": 4})
        )
        .as_deref(),
        Some("[4]")
    );
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "priority": -1})
        )
        .as_deref(),
        Some("[-]")
    );
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "priority": -3})
        )
        .as_deref(),
        Some("[-3]")
    );
    for cleared in [json!(null), json!(0)] {
        assert_eq!(
            render(
                "workspace_task_update",
                json!({"task_id": TASK, "priority": cleared})
            )
            .as_deref(),
            Some("[0]")
        );
    }
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "start": "2026-10-20", "estimate_minutes": 45})
        )
        .as_deref(),
        Some("[ds] then type “2026-10-20” · [option+t] then type “45”")
    );
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "start": null, "estimate_minutes": null})
        )
        .as_deref(),
        Some("[ds] then clear it · [option+t] then clear it")
    );
    // "Due friday, and make it urgent": two steps.
    assert_eq!(
        render(
            "workspace_task_update",
            json!({"task_id": TASK, "due": "2026-10-16", "priority": 1})
        )
        .as_deref(),
        Some("[dd] then type “2026-10-16” · [1]")
    );
}

#[test]
fn lists_folders_and_columns() {
    assert_eq!(
        render("workspace_list_create", json!({"name": "Garden"})).as_deref(),
        Some("[cmd+shift+n] then type “Garden”")
    );
    assert_eq!(
        render("list_create", json!({"name": "Garden"})).as_deref(),
        Some("[cmd+shift+n] then type “Garden”")
    );
    assert_eq!(
        render("workspace_folder_create", json!({"name": "Home"})).as_deref(),
        Some("[cmd+option+n] then type “Home”")
    );
    assert_eq!(
        render(
            "workspace_list_move",
            json!({"list_id": OTHER, "folder_id": TASK})
        )
        .as_deref(),
        Some("in the sidebar, [mm] then choose the folder")
    );
    assert_eq!(
        render("workspace_list_delete", json!({"list_id": OTHER})).as_deref(),
        Some("[cmd+shift+delete]")
    );
    assert_eq!(
        render("workspace_task_to_list", json!({"task_id": TASK})).as_deref(),
        Some("[ex]")
    );
    assert_eq!(
        render("workspace_filter_list_create", json!({"name": "Soon"})).as_deref(),
        Some("[cmd+option+shift+n]")
    );
    assert_eq!(
        render(
            "workspace_filter_list_update",
            json!({"filter_list_id": OTHER, "name": "Soon"})
        )
        .as_deref(),
        Some("[cmd+option+shift+e]")
    );
    assert_eq!(
        render("workspace_board_column_set", json!({"title": "Later"})).as_deref(),
        Some("[cmd+shift+c] then type “Later”")
    );
    assert_eq!(
        render("workspace_board_column_delete", json!({"column_id": OTHER})).as_deref(),
        Some("[cmd+ctrl+c]")
    );
    assert_eq!(
        render(
            "agent_instructions_set",
            json!({"scope": "list", "id": OTHER, "text": "x"})
        )
        .as_deref(),
        Some("[oo]")
    );
    assert_eq!(
        render(
            "agent_instructions_set",
            json!({"scope": "account", "text": "x"})
        ),
        None
    );
}

#[test]
fn a_command_with_no_key_is_reached_through_the_palette() {
    assert_eq!(
        render(
            "workspace_board_column_set",
            json!({"column_id": OTHER, "title": "Soon"})
        )
        .as_deref(),
        Some("[cmd+shift+p] then “Rename this board column” then type “Soon”")
    );
    let unbound = keymap_parse(r#"[{"bindings": {"cmd+shift+p": null, "cmd+k": null}}]"#.into());
    let commands = keymap_resolve(unbound.bindings, workspace_command_catalog()).commands;
    assert_eq!(
        render_with(
            "workspace_board_column_set",
            json!({"column_id": OTHER, "title": "Soon"}),
            commands
        ),
        None
    );
}

#[test]
fn the_matrix_is_its_view_then_the_quadrants_digit() {
    let one = |urgency: i64, importance: i64| {
        render(
            "task_matrix_set",
            json!({"placements": [{"task_id": 1, "urgency": urgency, "importance": importance}]}),
        )
    };
    assert_eq!(one(5, 5).as_deref(), Some("[cmd+3] then [option+1]"));
    assert_eq!(one(-2, 5).as_deref(), Some("[cmd+3] then [option+2]"));
    assert_eq!(one(5, -2).as_deref(), Some("[cmd+3] then [option+3]"));
    assert_eq!(one(-1, -1).as_deref(), Some("[cmd+3] then [option+4]"));
    assert_eq!(one(0, 0), None);
    assert_eq!(
        render(
            "task_matrix_set",
            json!({"placements": [
                {"task_id": 1, "urgency": 1, "importance": 1},
                {"task_id": 2, "urgency": 1, "importance": 1}
            ]})
        ),
        None
    );
}

#[test]
fn a_rebound_command_moves_its_hint() {
    let parsed = keymap_parse(
        r#"[{"bindings": {"dd": null, "cmd+d": null, "cmd+shift+u": "taskEditDue"}}]"#.into(),
    );
    let commands = keymap_resolve(parsed.bindings, workspace_command_catalog()).commands;
    assert_eq!(
        render_with(
            "task_update",
            json!({"task_id": 1, "due": "friday"}),
            commands
        )
        .as_deref(),
        Some("[cmd+shift+u] then type “friday”")
    );
}

#[test]
fn a_long_value_is_cut_short_on_one_line() {
    assert_eq!(
        render(
            "task_note_add",
            json!({"task_id": 1, "note": "first line\nand a second line that goes on and on"})
        )
        .as_deref(),
        Some("[nn] then type “first line and a second line tha…”")
    );
}

#[test]
fn reads_dailies_and_unknown_tools_have_none() {
    for tool in [
        "workspace_tasks",
        "task_search",
        "daily_tick",
        "daily_add",
        "nonsense",
    ] {
        assert_eq!(
            render(tool, json!({"daily_id": "a", "title": "x"})),
            None,
            "{tool}"
        );
    }
    assert_eq!(render("task_update", json!({"task_id": 1})), None);
}

#[test]
fn conditions_are_cc_and_the_context_its_tab() {
    assert_eq!(
        render(
            "workspace_task_conditions_set",
            json!({"task_id": TASK, "requires": [["Home", "Campus"]]})
        )
        .as_deref(),
        Some("[cc] then tick what it needs")
    );
    assert_eq!(
        render("workspace_context_set", json!({"on": ["Campus"]})).as_deref(),
        Some("[cmd+option+8] then Space on each condition that holds")
    );
}

/// One plausible call for each write tool, as the assistant makes them.
fn sample(tool: &str) -> Value {
    match tool {
        "task_add" => json!({"content": "Milk"}),
        "task_update" => json!({"task_id": 1, "due": "friday"}),
        "task_note_add" => json!({"task_id": 1, "note": "n"}),
        "task_move" => json!({"task_id": 1, "position": 1}),
        "task_reparent" => json!({"task_id": 1, "parent_task_id": 2}),
        "project_move" => json!({"task_id": 1, "source_list_id": "1", "target_list_id": "2"}),
        "list_create" | "workspace_list_create" | "workspace_folder_create" => json!({"name": "N"}),
        "task_matrix_set" => json!({"placements": [{"task_id": 1, "urgency": 1, "importance": 1}]}),
        "workspace_task_add" => json!({"title": "T"}),
        "workspace_task_update" => json!({"task_id": TASK, "title": "T"}),
        "workspace_task_move" => json!({"task_id": TASK, "list_id": OTHER}),
        "workspace_task_blocker_add" | "workspace_task_blocker_remove" => {
            json!({"task_id": TASK, "blocked_by_task_id": OTHER})
        }
        "workspace_filter_list_create" => json!({"name": "F"}),
        "workspace_filter_list_update" | "workspace_filter_list_delete" => {
            json!({"filter_list_id": OTHER})
        }
        "workspace_list_move" | "workspace_list_delete" => json!({"list_id": OTHER}),
        "workspace_board_column_set" => json!({"title": "C"}),
        "workspace_board_column_delete" => json!({"column_id": OTHER}),
        "agent_instructions_set" => json!({"scope": "list", "id": OTHER, "text": "x"}),
        "workspace_task_conditions_set" => json!({"task_id": TASK, "requires": ["Home"]}),
        "workspace_context_set" => json!({"on": ["Campus"]}),
        "workspace_day_start_set" => json!({"resets": true, "conditions": ["Home"]}),
        "workspace_condition_provides_set" => {
            json!({"condition": "Home", "provides": ["Quiet"]})
        }
        "daily_add" => json!({"title": "D"}),
        "daily_update" | "daily_tick" => json!({"daily_id": "d"}),
        _ => json!({"task_id": TASK}),
    }
}

#[test]
fn every_write_tool_has_a_hint_unless_it_is_listed_as_having_none() {
    for tool in WRITE_TOOLS {
        let hint = render(tool, sample(tool));
        if has_key_equivalent(tool) {
            assert!(hint.is_some(), "{tool} has no hint");
        } else {
            assert_eq!(hint, None, "{tool} is listed as having no keys");
        }
    }
    for tool in TOOLS_WITHOUT_KEYS {
        assert!(WRITE_TOOLS.contains(tool), "{tool} is not a write tool");
    }
}
