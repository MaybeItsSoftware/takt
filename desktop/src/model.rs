//! What the window shows, held in memory and patched in place by a write.
//!
//! A list's outline is read once when it is selected. Ticking a task off is
//! one core call and a change to the one row it touched; nothing is read
//! again. That is the shape every write is meant to take in this app.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use takt_core::records::{ListRow, TaskRow};
use takt_core::workspace::CoreWorkspace;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Sidebar,
    Outline,
}

pub struct OutlineRow {
    pub task: TaskRow,
    pub depth: usize,
}

pub struct Model {
    core: Arc<CoreWorkspace>,
    pub lists: Vec<ListRow>,
    pub list_index: usize,
    pub rows: Vec<OutlineRow>,
    pub row_index: usize,
    pub pane: Pane,
    pub message: Option<String>,
}

impl Model {
    pub fn open() -> Result<Self, String> {
        let path = std::env::var_os("TAKT_DESKTOP_DB")
            .map(PathBuf::from)
            .unwrap_or_else(default_database_path);
        if !path.exists() {
            return Err(format!("no workspace database at {}", path.display()));
        }
        let path = path.to_string_lossy().into_owned();
        takt_core::schema::migrate_workspace(path.clone()).map_err(|e| e.to_string())?;
        let core = CoreWorkspace::open(path).map_err(|e| e.to_string())?;
        let workspace_id = core.bootstrap(now_ms()).map_err(|e| e.to_string())?;
        let lists = core.lists(workspace_id, false).map_err(|e| e.to_string())?;
        let mut model = Model {
            core,
            lists,
            list_index: 0,
            rows: Vec::new(),
            row_index: 0,
            pane: Pane::Sidebar,
            message: None,
        };
        model.load_outline();
        Ok(model)
    }

    /// Moves the cursor in the pane that has it; in the sidebar, the outline
    /// follows.
    pub fn step(&mut self, by: isize) {
        match self.pane {
            Pane::Sidebar => {
                let next = clamp(self.list_index, by, self.lists.len());
                if next != self.list_index {
                    self.list_index = next;
                    self.load_outline();
                }
            }
            Pane::Outline => self.row_index = clamp(self.row_index, by, self.rows.len()),
        }
    }

    pub fn toggle_selected_done(&mut self) {
        if self.pane != Pane::Outline {
            return;
        }
        let Some(row) = self.rows.get_mut(self.row_index) else {
            return;
        };
        let status = if row.task.status == "open" {
            "completed"
        } else {
            "open"
        };
        let zone = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into());
        match self
            .core
            .set_status(row.task.id.clone(), status.into(), now_ms(), zone)
        {
            // The one row the write touched, patched; nothing read again.
            Ok(()) => {
                row.task.status = status.into();
                self.message = None;
            }
            Err(error) => self.message = Some(error.to_string()),
        }
    }

    fn load_outline(&mut self) {
        self.rows.clear();
        self.row_index = 0;
        let Some(list) = self.lists.get(self.list_index) else {
            return;
        };
        match self.core.tasks_in_lists(vec![list.id.clone()]) {
            Ok(tasks) => {
                self.rows = outline(tasks, list.visible_root_task_id.as_deref());
                self.message = None;
            }
            Err(error) => self.message = Some(error.to_string()),
        }
    }
}

/// Depth first in sibling order, beneath the list's visible root when it
/// has one.
fn outline(tasks: Vec<TaskRow>, root: Option<&str>) -> Vec<OutlineRow> {
    let mut children: HashMap<Option<String>, Vec<TaskRow>> = HashMap::new();
    for task in tasks {
        if task.archived_at_ms.is_some() {
            continue;
        }
        children
            .entry(task.parent_task_id.clone())
            .or_default()
            .push(task);
    }
    for siblings in children.values_mut() {
        siblings.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.id.cmp(&b.id)));
    }
    let mut rows = Vec::new();
    let mut stack: Vec<(TaskRow, usize)> = children
        .remove(&root.map(str::to_owned))
        .unwrap_or_default()
        .into_iter()
        .rev()
        .map(|task| (task, 0))
        .collect();
    while let Some((task, depth)) = stack.pop() {
        if let Some(kids) = children.remove(&Some(task.id.clone())) {
            stack.extend(kids.into_iter().rev().map(|kid| (kid, depth + 1)));
        }
        rows.push(OutlineRow { task, depth });
    }
    rows
}

fn clamp(index: usize, by: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (index as isize + by).clamp(0, len as isize - 1) as usize
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn default_database_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join("Library/Application Support/Takt/priority.sqlite")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, parent: Option<&str>, order: i64) -> TaskRow {
        TaskRow {
            id: id.into(),
            list_id: "l".into(),
            parent_task_id: parent.map(Into::into),
            title: id.into(),
            notes: String::new(),
            status: "open".into(),
            sort_order: order,
            due_at_ms: None,
            estimate_seconds: None,
            source_system: None,
            source_id: None,
            item_kind: None,
            is_promoted: None,
            archived_at_ms: None,
            completed_at_ms: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    #[test]
    fn outline_is_depth_first_in_sibling_order() {
        let rows = outline(
            vec![
                task("b", None, 2),
                task("a", None, 1),
                task("a2", Some("a"), 2),
                task("a1", Some("a"), 1),
            ],
            None,
        );
        let shape: Vec<_> = rows.iter().map(|r| (r.task.id.as_str(), r.depth)).collect();
        assert_eq!(shape, [("a", 0), ("a1", 1), ("a2", 1), ("b", 0)]);
    }

    #[test]
    fn outline_starts_beneath_the_visible_root() {
        let rows = outline(
            vec![task("root", None, 1), task("child", Some("root"), 1)],
            Some("root"),
        );
        let ids: Vec<_> = rows.iter().map(|r| r.task.id.as_str()).collect();
        assert_eq!(ids, ["child"]);
    }

    #[test]
    fn clamp_stays_in_range() {
        assert_eq!(clamp(0, -1, 3), 0);
        assert_eq!(clamp(2, 1, 3), 2);
        assert_eq!(clamp(1, 1, 3), 2);
        assert_eq!(clamp(0, 1, 0), 0);
    }
}
