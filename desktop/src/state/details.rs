//! What a row draws beside its title that the task row itself does not
//! carry: blocked and buried (greyed, and sunk by the core), waiting on,
//! linked, priority and tags. Read whole when a scope is read or another
//! process wrote; a write of ours patches the one task it touched, and
//! rereads the blocked and buried sets, which are a query each.

use super::App;
use super::store::now_ms;
use std::collections::{HashMap, HashSet};
use takt_core::rows::MetadataRow;

#[derive(Debug, Clone, Default)]
pub struct Details {
    pub blocked: HashSet<String>,
    /// Buried until this moment: open, and its start still to come.
    pub buried: HashSet<String>,
    pub meta: HashMap<String, Meta>,
}

/// One task's metadata, as the rows draw it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    pub priority: Option<i64>,
    pub tags: Vec<String>,
    pub waiting_on: Option<String>,
    pub follow_up_at_ms: Option<i64>,
    pub linked: bool,
    pub start_at_ms: Option<i64>,
    pub recurrence: Option<String>,
}

impl Meta {
    fn from_row(row: &MetadataRow) -> Self {
        Meta {
            priority: row.priority.filter(|p| *p != 0),
            tags: serde_json::from_str(&row.tags_json).unwrap_or_default(),
            waiting_on: row.waiting_on.clone().filter(|w| !w.trim().is_empty()),
            follow_up_at_ms: row.waiting_follow_up_at_ms,
            linked: !matches!(row.external_links_json.trim(), "" | "[]"),
            start_at_ms: row.start_at_ms,
            recurrence: row.recurrence_rule.clone().filter(|r| !r.is_empty()),
        }
    }
}

impl App {
    /// Reads every task's details again.
    pub fn reload_details(&mut self) {
        let core = &self.store.core;
        let meta = core
            .all_metadata()
            .unwrap_or_default()
            .iter()
            .map(|row| (row.task_id.clone(), Meta::from_row(row)))
            .filter(|(_, meta)| *meta != Meta::default())
            .collect();
        self.details.meta = meta;
        self.reload_blocked();
    }

    /// The blocked and buried sets, which any status or start change moves.
    pub fn reload_blocked(&mut self) {
        let core = &self.store.core;
        self.details.blocked = core
            .blocked_task_ids()
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.details.buried = core
            .buried_task_ids(now_ms())
            .unwrap_or_default()
            .into_iter()
            .collect();
    }

    /// After a write to one task: its row, and the blocked and buried sets.
    pub fn patch_details(&mut self, id: &str) {
        if let Ok(rows) = self.store.core.metadata_for_tasks(vec![id.to_string()]) {
            match rows.first().map(Meta::from_row) {
                Some(meta) if meta != Meta::default() => {
                    self.details.meta.insert(id.to_string(), meta);
                }
                _ => {
                    self.details.meta.remove(id);
                }
            }
        }
        self.reload_blocked();
    }

    pub fn meta(&self, id: &str) -> Option<&Meta> {
        self.details.meta.get(id)
    }

    /// Greyed: blocked by an open task, or buried until its start.
    pub fn is_greyed(&self, id: &str) -> bool {
        self.details.blocked.contains(id) || self.details.buried.contains(id)
    }
}
