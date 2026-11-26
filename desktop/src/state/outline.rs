//! The task rows of the main pane, and the writes made on them.
//!
//! A scope is read when you arrive at it. A write then patches the rows
//! held: a title or a status in place. A write that moves rows about
//! (add, delete, indent, move) reads the one list again, which costs about
//! half a millisecond; Everything is never read again for a write of ours.

use super::store::now_ms;
use super::{App, Editing, Scope, ViewMode};
use std::collections::{HashMap, HashSet};
use takt_core::records::TaskRow;

#[derive(Debug, Clone)]
pub struct OutlineRow {
    pub task: TaskRow,
    pub depth: i64,
    pub has_children: bool,
}

/// How long a just-finished task stays in view while finished tasks are
/// hidden.
pub const LINGER_MS: i64 = 3000;

impl App {
    /// The board read of the scope's lists: every task's column and matrix
    /// place, and for a combined scope its cards.
    fn read_placements(&mut self) {
        self.placements.clear();
        self.combined_cards.clear();
        self.board_card_ids.clear();
        let ids = self.scope_list_ids();
        let board = match self.store.core.combined_board(ids, None) {
            Ok(board) => board,
            Err(error) => {
                self.message = Some(error.to_string());
                return;
            }
        };
        let rows = takt_core::packed_rows::unpack_task_rows(&board.rows).unwrap_or_default();
        for placement in board.placements {
            if let Some(task) = rows.get(placement.row as usize) {
                self.placements.insert(
                    task.id.clone(),
                    Placement {
                        column: placement.kanban_column,
                        urgency: placement.matrix_urgency,
                        importance: placement.matrix_importance,
                        rank: placement.matrix_rank,
                    },
                );
            }
        }
        self.combined_cards = unpack_u32s(&board.cards)
            .into_iter()
            .filter_map(|ix| rows.get(ix as usize).cloned())
            .collect();
        self.board_card_ids = unpack_u32s(&board.board_cards)
            .into_iter()
            .filter_map(|ix| rows.get(ix as usize).map(|task| task.id.clone()))
            .collect();
    }
}

/// Where a task stands on the board and the matrix.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Placement {
    pub column: Option<String>,
    pub urgency: Option<i64>,
    pub importance: Option<i64>,
    pub rank: Option<i64>,
}

/// `u32`s packed little-endian, as the board read crosses them.
fn unpack_u32s(bytes: &[u8]) -> Vec<u32> {
    let (chunks, _) = bytes.as_chunks::<4>();
    chunks
        .iter()
        .map(|chunk| u32::from_le_bytes(*chunk))
        .collect()
}

impl App {
    /// Every task the scope holds, before folding and the completed filter.
    /// Also reads each task's board column and matrix place, and the
    /// board's cards.
    pub fn read_scope(&mut self) -> Vec<OutlineRow> {
        let core = self.store.core.clone();
        self.read_placements();
        let result = match self.scope.clone() {
            Scope::List { list_id, entered } => {
                let root = entered.or_else(|| {
                    self.list(&list_id)
                        .and_then(|list| list.visible_root_task_id.clone())
                });
                core.outline(list_id, root).map(|items| {
                    let mut rows: Vec<OutlineRow> = items
                        .into_iter()
                        .map(|item| OutlineRow {
                            task: item.task,
                            depth: item.depth,
                            has_children: false,
                        })
                        .collect();
                    mark_parents(&mut rows);
                    rows
                })
            }
            Scope::Everything | Scope::Folder(_) => Ok(self
                .combined_cards
                .iter()
                .map(|task| OutlineRow {
                    task: task.clone(),
                    depth: 0,
                    has_children: false,
                })
                .collect()),
            Scope::Today => Ok(self
                .today
                .rows()
                .map(|row| OutlineRow {
                    task: row.task.clone(),
                    depth: 0,
                    has_children: false,
                })
                .collect()),
            Scope::FilterList(id) => {
                let Some(filter) = self.filter_lists.iter().find(|f| f.id == id).cloned() else {
                    return Vec::new();
                };
                core.filter_list_tasks(
                    self.store.workspace_id.clone(),
                    filter.definition,
                    now_ms(),
                    self.store.zone.clone(),
                )
                .map(|tasks| {
                    tasks
                        .into_iter()
                        .map(|task| OutlineRow {
                            task,
                            depth: 0,
                            has_children: false,
                        })
                        .collect()
                })
            }
        };
        match result {
            Ok(rows) => rows,
            Err(error) => {
                self.message = Some(error.to_string());
                Vec::new()
            }
        }
    }

    /// Reads the scope and lays the rows out, keeping the selection on the
    /// same task where it survives, or on the row that took its place.
    pub fn reload_rows(&mut self) {
        let previous_index = self
            .selected_task
            .as_deref()
            .and_then(|id| self.rows.iter().position(|row| row.task.id == id));
        self.all_rows = self.read_scope();
        self.layout_rows();
        let keeps = self
            .selected_task
            .as_deref()
            .is_some_and(|id| self.rows.iter().any(|row| row.task.id == id));
        if !keeps {
            self.selected_task = previous_index
                .map(|ix| ix.min(self.rows.len().saturating_sub(1)))
                .and_then(|ix| self.rows.get(ix))
                .or_else(|| self.rows.first())
                .map(|row| row.task.id.clone());
        }
    }

    /// The rows drawn: folded subtrees and, when asked, finished tasks left
    /// out. A task ticked off this session stays until the scope changes, so
    /// a slip of `x` can be seen and undone.
    pub fn layout_rows(&mut self) {
        let hide_completed = self.prefs.hide_completed;
        let mut rows = Vec::with_capacity(self.all_rows.len());
        let mut folded_below: Option<i64> = None;
        for row in &self.all_rows {
            if let Some(depth) = folded_below {
                if row.depth > depth {
                    continue;
                }
                folded_below = None;
            }
            let finished = row.task.status != "open";
            if hide_completed && finished && !self.lingering.contains(&row.task.id) {
                if row.has_children {
                    folded_below = Some(row.depth);
                }
                continue;
            }
            let later = self.prefs.hide_later
                && row.task.status == "open"
                && self.details.buried.contains(&row.task.id);
            if later && !self.lingering.contains(&row.task.id) {
                if row.has_children {
                    folded_below = Some(row.depth);
                }
                continue;
            }
            if row.has_children && self.prefs.folded_tasks.contains(&row.task.id) {
                folded_below = Some(row.depth);
            }
            rows.push(row.clone());
        }
        self.rows = rows;
        self.layout_board();
    }

    /// The ids the keys move through: Today's plan in its outline, the
    /// outline's or the cards' everywhere else.
    pub fn visible_ids(&self) -> Vec<String> {
        match self.view_mode {
            ViewMode::Outline if self.scope == Scope::Today => self.today_ids(),
            ViewMode::Board | ViewMode::Matrix => self.board_ids(),
            ViewMode::Outline => self.rows.iter().map(|row| row.task.id.clone()).collect(),
        }
    }

    pub fn selected_index(&self) -> Option<usize> {
        let id = self.selected_task.as_deref()?;
        self.visible_ids().iter().position(|other| other == id)
    }

    pub fn select_row(&mut self, index: usize) {
        if let Some(id) = self.visible_ids().get(index) {
            self.selected_task = Some(id.clone());
        }
    }

    /// `j`/`k`, page and the ends, over the rows drawn.
    pub fn move_selection(&mut self, key: &str) {
        let count = self.visible_ids().len();
        if count == 0 {
            return;
        }
        let current = self.selected_index();
        let index = match key {
            "home" | "cmd+up" | "gg" => 0,
            "end" | "cmd+down" | "shift+g" => count - 1,
            _ => {
                let offset = match key {
                    "down" | "j" => 1,
                    "up" | "k" => -1,
                    "pagedown" | "ctrl+d" => 10,
                    "pageup" | "ctrl+u" => -10,
                    _ => return,
                };
                match current {
                    None if offset < 0 => count - 1,
                    None => 0,
                    Some(current) => (current as i64 + offset).clamp(0, count as i64 - 1) as usize,
                }
            }
        };
        self.select_row(index);
    }

    // MARK: Folding

    pub fn toggle_fold(&mut self) {
        // By id, as the board's cards and subtask rows are not the outline's
        // rows and their index there means nothing.
        let Some(row) = self
            .selected_task
            .as_deref()
            .and_then(|id| self.all_rows.iter().find(|row| row.task.id == id))
        else {
            return;
        };
        if !row.has_children {
            return;
        }
        let id = row.task.id.clone();
        if self.toggle_on_board(&id) {
            return;
        }
        if !self.prefs.folded_tasks.remove(&id) {
            self.prefs.folded_tasks.insert(id);
        }
        self.prefs.save();
        self.layout_rows();
    }

    /// `l` on a folded row unfolds it; on an open one moves into it.
    pub fn unfold_or_descend(&mut self) {
        let Some(index) = self.selected_index() else {
            return;
        };
        let row = &self.rows[index];
        if row.has_children && self.prefs.folded_tasks.contains(&row.task.id) {
            let id = row.task.id.clone();
            self.prefs.folded_tasks.remove(&id);
            self.prefs.save();
            self.layout_rows();
        } else if row.has_children {
            self.select_row(index + 1);
        }
    }

    /// `h` on an open parent folds it; on anything else goes to its parent.
    /// `←` in the outline: fold the subtasks, else step out to the parent,
    /// else (at the top level, or with nothing to stand on) back to the
    /// sidebar, as `←` past the board's first column does.
    pub fn fold_or_ascend(&mut self) {
        let Some(index) = self.selected_index() else {
            self.focus_region(super::Region::Sidebar);
            return;
        };
        let row = &self.rows[index];
        if row.has_children && !self.prefs.folded_tasks.contains(&row.task.id) {
            let id = row.task.id.clone();
            self.prefs.folded_tasks.insert(id);
            self.prefs.save();
            self.layout_rows();
            return;
        }
        if let Some(parent) = row.task.parent_task_id.clone()
            && self.rows.iter().any(|r| r.task.id == parent)
        {
            self.selected_task = Some(parent);
            return;
        }
        self.focus_region(super::Region::Sidebar);
    }

    pub fn set_all_folded(&mut self, folded: bool) {
        let parents: Vec<String> = self
            .all_rows
            .iter()
            .filter(|row| row.has_children)
            .map(|row| row.task.id.clone())
            .collect();
        if folded {
            self.prefs.opened_cards.clear();
        }
        for id in parents {
            if folded {
                self.prefs.folded_tasks.insert(id);
            } else {
                self.prefs.folded_tasks.remove(&id);
            }
        }
        self.prefs.save();
        self.layout_rows();
    }

    /// The subtasks of `id` that have subtasks of their own and are drawn
    /// (finished ones are left out while those are hidden).
    fn subtask_parents(&self, id: &str) -> Vec<String> {
        let hide = self.prefs.hide_completed;
        self.all_rows
            .iter()
            .filter(|row| row.has_children && row.task.parent_task_id.as_deref() == Some(id))
            .filter(|row| {
                !(hide && row.task.status != "open" && !self.lingering.contains(&row.task.id))
            })
            .map(|row| row.task.id.clone())
            .collect()
    }

    /// The layer below an open `root` for `.` to open (see [`layer_to_open`]).
    pub(super) fn layer_to_open(
        &self,
        root: &str,
        open: impl Fn(&Self, &str) -> bool,
    ) -> Vec<String> {
        layer_to_open(root, |id| self.subtask_parents(id), |id| open(self, id))
    }

    /// The layer below an open `root` for `,` to shut (see [`layer_to_shut`]).
    pub(super) fn layer_to_shut(
        &self,
        root: &str,
        open: impl Fn(&Self, &str) -> bool,
    ) -> Vec<String> {
        layer_to_shut(root, |id| self.subtask_parents(id), |id| open(self, id))
    }

    pub fn toggle_hide_completed(&mut self) {
        self.prefs.hide_completed = !self.prefs.hide_completed;
        self.prefs.save();
        self.lingering.clear();
        self.layout_rows();
        self.keep_selection_drawn();
    }

    fn keep_selection_drawn(&mut self) {
        if self.selected_index().is_none() {
            self.selected_task = self.rows.first().map(|row| row.task.id.clone());
        }
    }

    // MARK: Writes

    /// `x`: ticks the selection off, or opens it again. Patched in place.
    pub fn toggle_complete(&mut self) {
        let Some(task) = self.selected().cloned() else {
            return;
        };
        if task.status == "open" && self.toggle_daily(&task.id) {
            return;
        }
        let status = if task.status == "open" {
            "completed"
        } else {
            "open"
        };
        self.set_status(&task.id, status);
    }

    /// A daily due today is ticked for the day, or unticked, and its task
    /// left open: the phones do the same. False when the task is no daily
    /// due today.
    fn toggle_daily(&mut self, task_id: &str) -> bool {
        let Some(daily) = self.today.daily_by_task.get(task_id).cloned() else {
            return false;
        };
        let zone = self.store.zone.clone();
        let now = now_ms();
        let done = if daily.done {
            self.write(|store| {
                store
                    .core
                    .clear_contribution(daily.daily_id.clone(), now, zone)
            })
        } else {
            self.write(|store| {
                store
                    .core
                    .log_contribution(daily.daily_id.clone(), 0, true, now, zone)
                    .map(|_| ())
            })
        };
        if done.is_some() {
            if !daily.done {
                self.start_celebration(task_id);
                self.linger(task_id);
                // Drawn ticked until the day is ranked again.
                for row in self
                    .today
                    .day
                    .iter_mut()
                    .chain(self.today.dailies.iter_mut())
                    .chain(self.today.ladder.iter_mut())
                    .filter(|row| row.task.id == task_id)
                {
                    row.task.status = "completed".into();
                }
            }
            if let Some(entry) = self.today.daily_by_task.get_mut(task_id) {
                entry.done = !daily.done;
            }
            self.reload_today();
            self.reload_done();
        }
        true
    }

    /// Marks the selection cancelled ("invalidated"), or opens it again.
    pub fn toggle_invalidate(&mut self) {
        let Some(task) = self.selected().cloned() else {
            return;
        };
        let status = if task.status == "cancelled" {
            "open"
        } else {
            "cancelled"
        };
        self.set_status(&task.id, status);
    }

    pub fn set_status(&mut self, id: &str, status: &str) {
        let zone = self.store.zone.clone();
        let done = self.write(|store| {
            store
                .core
                .set_status(id.into(), status.into(), now_ms(), zone)
        });
        if done.is_none() {
            return;
        }
        if status == "completed" {
            self.start_celebration(id);
        }
        // A recurring task makes its next occurrence on completion, which is
        // a new row: then, and only then, the list is read again.
        let recurs = self
            .store
            .core
            .task(id.into())
            .ok()
            .flatten()
            .map(|task| task.status.as_str() == status)
            .unwrap_or(true);
        if !recurs {
            self.reload_rows();
            return;
        }
        let now = now_ms();
        for row in self.all_rows.iter_mut().filter(|row| row.task.id == id) {
            row.task.status = status.into();
            row.task.completed_at_ms = (status != "open").then_some(now);
            row.task.updated_at_ms = now;
        }
        for row in self
            .today
            .day
            .iter_mut()
            .chain(self.today.dailies.iter_mut())
            .chain(self.today.ladder.iter_mut())
        {
            if row.task.id == id {
                row.task.status = status.into();
            }
        }
        if status != "open" {
            self.linger(id);
        }
        self.layout_rows();
        self.reload_today();
        self.reload_done();
    }

    /// Saves a title typed in place. Patched in place.
    pub fn rename_task(&mut self, id: &str, title: &str) {
        let title = title.trim();
        if title.is_empty() {
            return;
        }
        let Some(task) = self.task(id).cloned() else {
            return;
        };
        if task.title == title {
            return;
        }
        let zone = self.store.zone.clone();
        let done = self.write(|store| {
            let baseline = store.core.editor_snapshot(id.into())?;
            let mut edit = baseline.clone();
            edit.title = title.into();
            store.core.save_editor(edit, baseline, now_ms(), zone)
        });
        if done.is_some() {
            for row in self.all_rows.iter_mut().filter(|row| row.task.id == id) {
                row.task.title = title.into();
            }
            self.layout_rows();
            if self.nested_lists.iter().any(|item| item.task.id == id) {
                self.reload_sidebar();
            }
        }
    }

    /// Adds a task from the draft row, closes the row and selects the new
    /// task, so the keys are back in normal mode on it.
    /// False when the text was kept back, its problem in the status bar.
    pub fn create_task(&mut self, title: &str) -> bool {
        let Some(Editing::NewTask {
            list_id,
            parent_id,
            after,
        }) = self.editing.clone()
        else {
            return true;
        };
        let title = title.trim();
        if title.is_empty() {
            self.editing = None;
            return true;
        }
        // Today's draft is filed in its list without a place among the
        // rows on screen (they are the plan, from every list): a task at the
        // top, as quick capture files one, a subtask last under its parent.
        let today = self.scope == Scope::Today;
        let new = match self.new_task_from(title, &list_id) {
            Ok(new) => takt_core::tasks::NewTask {
                parent_task_id: parent_id.clone(),
                at_top: if today {
                    parent_id.is_none()
                } else {
                    after.is_none()
                },
                adjacent_task_id: if today { None } else { after.clone() },
                ..new
            },
            Err(problem) => {
                self.message = Some(problem);
                return false;
            }
        };
        let created = self.write(|store| store.core.create_task(new, now_ms()));
        if let Some(id) = created {
            self.lingering.clear();
            if today {
                self.plan_new_task_for_today(&id);
            }
            self.reload_rows();
            self.layout_board();
            self.selected_task = Some(id);
            self.editing = None;
        }
        true
    }

    /// Opens a draft row: below the selection, above it, or (`tab`) as its
    /// last subtask.
    pub fn begin_new_task(&mut self, placement: NewPlacement) {
        let selected = self.selected().cloned();
        let Some(list_id) = selected
            .as_ref()
            .map(|task| task.list_id.clone())
            .or_else(|| self.scope_list_id())
        else {
            self.message = Some("Make a list first.".into());
            return;
        };
        let entered = match &self.scope {
            Scope::List { entered, .. } => entered.clone(),
            _ => None,
        };
        let (parent_id, after) = match (placement, selected) {
            (NewPlacement::Below, Some(task)) => (task.parent_task_id.clone(), Some(task.id)),
            (NewPlacement::Above, Some(task)) => {
                let previous = self.previous_sibling(&task);
                (task.parent_task_id.clone(), previous)
            }
            (NewPlacement::Child, Some(task)) => {
                let last = self.last_child(&task.id);
                (Some(task.id.clone()), last)
            }
            (_, None) => (entered, self.rows.last().map(|row| row.task.id.clone())),
        };
        // A subtask of the selected card is drafted on the board, under the
        // card; anything else there is a new card.
        if self.view_mode == ViewMode::Board
            && placement == NewPlacement::Child
            && parent_id.is_some()
        {
            self.editing = Some(Editing::NewTask {
                list_id,
                parent_id,
                after,
            });
            return;
        }
        if matches!(self.view_mode, ViewMode::Board | ViewMode::Matrix) {
            self.begin_new_card();
            return;
        }
        // Today's plan draws a draft row of its own, so the new task is
        // typed where you are rather than in another list: it is filed where
        // quick capture files (a subtask, with Tab, under the selected task
        // in that task's list) and planned for today.
        if self.scope == Scope::Today {
            let parent = match placement {
                NewPlacement::Child => self.selected().cloned(),
                _ => None,
            };
            let (list_id, parent_id) = match parent {
                Some(task) => (task.list_id, Some(task.id)),
                None => match self.quick_capture_destination() {
                    Some((id, _)) => (id, None),
                    None => {
                        self.message = Some("Make a list first.".into());
                        return;
                    }
                },
            };
            self.editing = Some(Editing::NewTask {
                list_id,
                parent_id,
                after: None,
            });
            return;
        }
        if self.view_mode != ViewMode::Outline {
            self.select_view_mode(ViewMode::Outline);
        }
        self.editing = Some(Editing::NewTask {
            list_id,
            parent_id,
            after,
        });
    }

    /// Puts a task made in Today on today's plan, so it is drawn where it
    /// was typed. Its own undo step, after the one that made it.
    pub fn plan_new_task_for_today(&mut self, id: &str) {
        self.write(|store| {
            store
                .core
                .set_planned_for_today(true, vec![id.to_string()], now_ms())
        });
        self.reload_today();
    }

    /// The last of a task's direct subtasks, folded or not: a new subtask
    /// goes after it, so it stays where it is drawn rather than jumping to
    /// the top of the branch.
    pub fn last_child(&self, parent: &str) -> Option<String> {
        self.all_rows
            .iter()
            .rev()
            .find(|row| row.task.parent_task_id.as_deref() == Some(parent))
            .map(|row| row.task.id.clone())
    }

    fn previous_sibling(&self, task: &TaskRow) -> Option<String> {
        let index = self
            .all_rows
            .iter()
            .position(|row| row.task.id == task.id)?;
        self.all_rows[..index]
            .iter()
            .rev()
            .take_while(|row| row.task.id != task.parent_task_id.clone().unwrap_or_default())
            .find(|row| row.task.parent_task_id == task.parent_task_id)
            .map(|row| row.task.id.clone())
    }

    pub fn delete_task(&mut self, id: &str) {
        let Some(task) = self
            .task(id)
            .cloned()
            .or_else(|| self.store.core.task(id.into()).ok().flatten())
        else {
            return;
        };
        let index = self.selected_index().unwrap_or(0);
        if self
            .write(|store| store.core.delete_task(task.id.clone()))
            .is_some()
        {
            self.pending_delete = None;
            // Today draws from its held ranking, not the store: take the
            // task out of it now, so it goes at once, and rank again for
            // the subtasks that went with it.
            self.today.day.retain(|row| row.task.id != task.id);
            self.today.dailies.retain(|row| row.task.id != task.id);
            self.today.ladder.retain(|row| row.task.id != task.id);
            self.reload_today();
            self.reload_done();
            self.reload_rows();
            let index = index.min(self.rows.len().saturating_sub(1));
            self.select_row(index);
            if task.item_kind.as_deref() == Some("list") {
                self.reload_sidebar();
                if matches!(&self.scope, Scope::List { entered: Some(e), .. } if *e == task.id) {
                    self.leave_entered_task();
                }
            }
        }
    }

    pub fn indent_selected(&mut self, outdent: bool) {
        let Some(id) = self.selected_task.clone() else {
            return;
        };
        let done = self.write(|store| {
            if outdent {
                store.core.outdent_task(id.clone(), now_ms())
            } else {
                store.core.indent_task(id.clone(), now_ms())
            }
        });
        if done.is_some() {
            self.reload_rows();
        }
    }

    pub fn move_selected(&mut self, by: i64) {
        let Some(id) = self.selected_task.clone() else {
            return;
        };
        let done = self.write(|store| {
            store
                .core
                .move_task_within_siblings(id.clone(), by as i32, now_ms())
        });
        if done.is_some() {
            self.reload_rows();
        }
    }

    /// Moves the selection to the end of another list.
    pub fn move_selected_to_list(&mut self, list_id: &str) {
        let Some(id) = self.selected_task.clone() else {
            return;
        };
        let done = self.write(|store| {
            store
                .core
                .move_task(id.clone(), list_id.into(), None, false, now_ms())
        });
        if done.is_some() {
            self.reload_rows();
            self.reload_sidebar();
        }
    }

    pub fn undo(&mut self, redo: bool) {
        let label = self.write(|store| {
            if redo {
                store.core.redo()
            } else {
                store.core.undo()
            }
        });
        if let Some(label) = label {
            let verb = if redo { "Redid" } else { "Undid" };
            // Said on the right of the status bar, where the history is, and
            // kept there until something else happens.
            self.history_note =
                label.map(|label| (format!("{verb} {label}"), self.history_top.clone()));
            self.lingering.clear();
            self.reload_sidebar();
            self.reload_rows();
            self.reload_focus();
            self.reload_done();
            self.reload_today();
        }
    }

    /// Ids of the tasks below `id` in the rows held, for a parent's count.
    pub fn child_counts(&self) -> HashMap<String, (usize, usize)> {
        let mut counts: HashMap<String, (usize, usize)> = HashMap::new();
        for row in &self.all_rows {
            if let Some(parent) = &row.task.parent_task_id {
                let entry = counts.entry(parent.clone()).or_default();
                entry.1 += 1;
                if row.task.status != "open" {
                    entry.0 += 1;
                }
            }
        }
        counts
    }

    /// A task just finished: kept in view for a few seconds while finished
    /// tasks are hidden, then let go by `expire_lingering`.
    pub fn linger(&mut self, id: &str) {
        self.lingering.insert(id.to_string());
        self.lingering_since.insert(id.to_string(), now_ms());
    }

    /// Called every second: finished tasks whose few seconds are up leave
    /// the list.
    pub fn expire_lingering(&mut self) {
        let now = now_ms();
        let due: Vec<String> = self
            .lingering_since
            .iter()
            .filter(|(_, since)| now - **since >= LINGER_MS)
            .map(|(id, _)| id.clone())
            .collect();
        if due.is_empty() {
            return;
        }
        for id in &due {
            self.lingering_since.remove(id);
            self.lingering.remove(id);
        }
        if self.today.let_go(&due) && self.scope == super::Scope::Today {
            let at = self.selected_index();
            self.reload_rows();
            self.keep_selection_in_place(at);
        }
        if self.prefs.hide_completed {
            let at = self.selected_index();
            self.layout_rows();
            self.keep_selection_in_place(at);
        }
    }

    /// The selected row left the list: the selection stays where it was,
    /// on the row that took its place (or the last, at the end), rather than
    /// going nowhere and so back to the top.
    fn keep_selection_in_place(&mut self, at: Option<usize>) {
        if self.selected_index().is_some() {
            return;
        }
        let Some(at) = at else { return };
        let ids = self.visible_ids();
        if let Some(id) = ids.get(at.min(ids.len().saturating_sub(1))) {
            self.selected_task = Some(id.clone());
        }
    }

    /// Once: finished tasks leave the list, whatever an earlier version
    /// saved, as they always did in the Swift app.
    pub fn adopt_finished_tasks_leave(&mut self) {
        if self.prefs.finished_tasks_leave || std::env::var_os("TAKT_DESKTOP_PREFS").is_some() {
            return;
        }
        self.prefs.finished_tasks_leave = true;
        self.prefs.hide_completed = true;
        self.prefs.save();
        self.layout_rows();
    }

    pub fn clear_lingering(&mut self) {
        self.lingering_since.clear();
        self.lingering = HashSet::new();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewPlacement {
    Below,
    Above,
    Child,
}

/// Marks each row that the next row is a child of.
fn mark_parents(rows: &mut [OutlineRow]) {
    for ix in 0..rows.len() {
        let next_is_child = rows
            .get(ix + 1)
            .is_some_and(|next| next.task.parent_task_id.as_deref() == Some(&rows[ix].task.id));
        rows[ix].has_children = next_is_child;
    }
}

/// The layer under an open `root` for `.` to open: the shallowest one with
/// a shut branch, reached through open ones. `branches` gives a task's
/// subtasks that have subtasks. Empty when every branch under it is open.
fn layer_to_open(
    root: &str,
    branches: impl Fn(&str) -> Vec<String>,
    open: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut seen = HashSet::from([root.to_string()]);
    let mut layer = branches(root);
    while !layer.is_empty() {
        layer.retain(|id| seen.insert(id.clone()));
        let shut: Vec<String> = layer.iter().filter(|id| !open(id)).cloned().collect();
        if !shut.is_empty() {
            return shut;
        }
        layer = layer.iter().flat_map(|id| branches(id)).collect();
    }
    Vec::new()
}

/// The layer under an open `root` for `,` to shut: its deepest open
/// branches, reached through open ones. Empty when none under it is open,
/// so `root` itself is next.
fn layer_to_shut(
    root: &str,
    branches: impl Fn(&str) -> Vec<String>,
    open: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut seen = HashSet::from([root.to_string()]);
    let mut deepest = Vec::new();
    let mut layer: Vec<String> = branches(root).into_iter().filter(|id| open(id)).collect();
    loop {
        layer.retain(|id| seen.insert(id.clone()));
        if layer.is_empty() {
            return deepest;
        }
        let next = layer
            .iter()
            .flat_map(|id| branches(id))
            .filter(|id| open(id))
            .collect();
        deepest = std::mem::replace(&mut layer, next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, parent: Option<&str>, depth: i64) -> OutlineRow {
        OutlineRow {
            task: TaskRow {
                id: id.into(),
                list_id: "l".into(),
                parent_task_id: parent.map(Into::into),
                title: id.into(),
                notes: String::new(),
                status: "open".into(),
                sort_order: 0,
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
            },
            depth,
            has_children: false,
        }
    }

    #[test]
    fn a_row_followed_by_its_child_is_a_parent() {
        let mut rows = vec![
            row("a", None, 0),
            row("a1", Some("a"), 1),
            row("b", None, 0),
        ];
        mark_parents(&mut rows);
        let parents: Vec<bool> = rows.iter().map(|r| r.has_children).collect();
        assert_eq!(parents, [true, false, false]);
    }

    #[test]
    fn packed_u32s_round_trip() {
        let bytes: Vec<u8> = [1u32, 70_000, u32::MAX]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        assert_eq!(unpack_u32s(&bytes), [1, 70_000, u32::MAX]);
    }

    /// a ─ b ─ d ─ f
    ///   └ c ─ e
    fn tree(id: &str) -> Vec<String> {
        let kids: &[&str] = match id {
            "a" => &["b", "c"],
            "b" => &["d"],
            "c" => &["e"],
            "d" => &["f"],
            _ => &[],
        };
        // Only branches: f and e have no subtasks of their own.
        kids.iter()
            .filter(|k| !matches!(**k, "e" | "f"))
            .map(|k| k.to_string())
            .collect()
    }

    #[test]
    fn dot_opens_a_layer_a_press_and_comma_shuts_the_deepest_first() {
        let mut open: HashSet<String> = HashSet::from(["a".to_string()]);
        let grow = |open: &HashSet<String>| layer_to_open("a", tree, |id| open.contains(id));
        let shrink = |open: &HashSet<String>| layer_to_shut("a", tree, |id| open.contains(id));

        assert_eq!(grow(&open), ["b", "c"]);
        open.extend(grow(&open));
        assert_eq!(grow(&open), ["d"]);
        open.extend(grow(&open));
        assert!(grow(&open).is_empty(), "every branch is open");

        assert_eq!(shrink(&open), ["d"]);
        open.remove("d");
        let mut layer = shrink(&open);
        layer.sort();
        assert_eq!(layer, ["b", "c"]);
        for id in layer {
            open.remove(&id);
        }
        assert!(shrink(&open).is_empty(), "the task itself is next");
    }

    #[test]
    fn dot_opens_a_shut_branch_beside_an_open_one_first() {
        let open: HashSet<String> = ["a", "b", "d"].map(String::from).into();
        assert_eq!(layer_to_open("a", tree, |id| open.contains(id)), ["c"]);
        assert_eq!(layer_to_shut("a", tree, |id| open.contains(id)), ["d"]);
    }
}
