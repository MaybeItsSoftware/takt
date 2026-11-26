//! The tools the hosted server offers, and what a call does around them.
//!
//! It offers the tools that need only the synced workspace: everything that
//! starts `workspace_`, the agent instructions and the focus reads. Not the
//! Checkvist tools (there is no Checkvist key here) nor the ones that read
//! or write the Mac's own files — dailies through `LocalState`, the day log,
//! Checkvist task metadata — since those files live on the Mac and nowhere
//! else. Everything offered is the stdio server's own declaration, word for
//! word, so the two can't drift.

use super::accounts::{Accounts, SyncFail};
use crate::mcp::{ToolHost, call_result, error_result, tool_definitions};
use serde_json::{Map, Value, json};
use uuid::Uuid;

/// Whether the hosted server offers the tool `name`.
pub fn offered(name: &str) -> bool {
    name.starts_with("workspace_")
        || matches!(
            name,
            "agent_instructions_get" | "agent_instructions_set" | "focus_status" | "focus_history"
        )
}

/// The tool table as one signed-in account sees it.
pub struct AccountHost<'a> {
    pub accounts: &'a Accounts,
    pub account: Uuid,
    pub token: &'a str,
}

impl ToolHost for AccountHost<'_> {
    fn definitions(&self) -> Vec<Value> {
        tool_definitions()
            .into_iter()
            .filter(|tool| tool["name"].as_str().is_some_and(offered))
            .collect()
    }

    fn call_tool(&self, name: &str, arguments: &Map<String, Value>) -> Value {
        if !offered(name) {
            return error_result(
                &format!("Error: Unknown tool: {name}"),
                &json!({ "message": "The hosted Takt server offers the workspace tools; this one needs the Mac." }),
            );
        }
        match self
            .accounts
            .with_workspace(self.account, self.token, |tools| {
                tools.call(name, arguments)
            }) {
            Ok((result, None)) => call_result(result),
            Ok((result, Some(fail))) => {
                // The write is made and will reach the other devices with the
                // next call's cycle; say so rather than hide it.
                super::log(
                    "warn",
                    &format!("push after {name} failed: {}", fail.message()),
                );
                let mut answer = call_result(result);
                if let Some(content) = answer["content"].as_array_mut() {
                    content.push(json!({
                        "type": "text",
                        "text": format!("Note: this change is saved but hasn't reached your other devices yet ({}). It will with the next call.", fail.message()),
                    }));
                }
                answer
            }
            Err(fail) => {
                let title = match &fail {
                    SyncFail::Subscription(_) => "Error: Takt Sync needed",
                    SyncFail::Unauthorized => "Error: Not signed in",
                    SyncFail::Other(_) => "Error: Takt couldn't sync",
                };
                error_result(title, &json!({ "message": fail.message() }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn the_hosted_server_offers_the_workspace_tools_and_nothing_that_needs_the_mac() {
        let names: BTreeSet<String> = tool_definitions()
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .filter(|name| offered(name))
            .map(str::to_owned)
            .collect();
        for wanted in [
            "workspace_tree",
            "workspace_task_add",
            "workspace_task_update",
            "agent_instructions_get",
            "agent_instructions_set",
            "focus_status",
        ] {
            assert!(names.contains(wanted), "{wanted} is offered");
        }
        for withheld in [
            "task_add",
            "task_lists",
            "dailies_list",
            "daily_tick",
            "daily_log_fetch",
            "task_metadata",
        ] {
            assert!(!names.contains(withheld), "{withheld} is not offered");
        }
    }
}
