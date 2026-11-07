//! The board and the matrix: the scope's cards laid out in the board's
//! columns, or in the four quadrants of urgency and importance.
//!
//! Laid out from what the scope read already holds (`rows`, `placements`)
//! and the columns the core resolves (`board_columns`), so switching between
//! outline, board and matrix reads only the columns.

use super::store::now_ms;
use super::{App, Scope, ViewMode};
use takt_core::board_columns::BoardColumn;
use takt_core::records::TaskRow;

#[derive(Debug, Clone, Default)]
pub struct Board {
    pub key: String,
    /// The columns as saved, hidden ones included: what an edit changes.
    pub layout: Vec<BoardColumn>,
    pub columns: Vec<BoardColumn>,
    /// Each drawn column's cards, in the order drawn.
    pub cards: Vec<Vec<TaskRow>>,
    /// The matrix: do now, schedule, delegate, eliminate; then unplaced.
    pub quadrants: Vec<Vec<TaskRow>>,
    pub unplaced: Vec<TaskRow>,
    /// The column with the keyboard.
    pub column: usize,
}

pub const QUADRANT_TITLES: [&str; 4] = ["Do now", "Schedule", "Delegate", "Eliminate"];

impl App {
    /// The board's key in the core, as the Swift app spells it.
    pub fn board_key(&self) -> String {
        match &self.scope {
            Scope::Everything | Scope::FilterList(_) => "everything/root".into(),
            Scope::Folder(id) => format!("folder:{id}/root"),
            Scope::List { list_id, entered } => {
                format!("{list_id}/{}", entered.as_deref().unwrap_or("root"))
            }
        }
    }

    /// The cards: a combined scope's board cards, or a list's top level.
    fn cards(&self) -> Vec<TaskRow> {
        let finished_hidden = self.prefs.hide_completed;
        let keep = |task: &TaskRow| {
            !(finished_hidden && task.status != "open" && !self.lingering.contains(&task.id))
        };
        match &self.scope {
            Scope::Everything | Scope::Folder(_) => {
                let by_id: std::collections::HashMap<&str, &TaskRow> = self
                    .combined_cards
                    .iter()
                    .map(|task| (task.id.as_str(), task))
                    .collect();
                self.board_card_ids
                    .iter()
                    .filter_map(|id| by_id.get(id.as_str()).map(|task| (*task).clone()))
                    .filter(keep)
                    .collect()
            }
            _ => {
                let top = self.all_rows.iter().map(|row| row.depth).min().unwrap_or(0);
                self.all_rows
                    .iter()
                    .filter(|row| row.depth == top)
                    .map(|row| row.task.clone())
                    .filter(keep)
                    .collect()
            }
        }
    }

    /// Lays the board and the matrix out from what is held, reading only
    /// the columns.
    pub fn layout_board(&mut self) {
        if !matches!(self.view_mode, ViewMode::Board | ViewMode::Matrix) {
            return;
        }
        let key = self.board_key();
        let columns = match self.store.core.board_columns(
            key.clone(),
            self.scope_list_ids(),
            now_ms(),
            self.store.zone.clone(),
        ) {
            Ok(columns) => columns,
            Err(error) => {
                self.message = Some(error.to_string());
                return;
            }
        };
        let overrides: std::collections::HashMap<String, String> = columns
            .placements
            .iter()
            .map(|p| (p.task_id.clone(), p.column_id.clone()))
            .collect();
        let layout = columns.layout;
        let drawn: Vec<BoardColumn> = columns.columns;
        let cards = self.cards();
        let mut by_column: Vec<Vec<TaskRow>> = vec![Vec::new(); drawn.len().max(1)];
        let mut quadrants: Vec<Vec<(i64, TaskRow)>> = vec![Vec::new(); 4];
        let mut unplaced = Vec::new();
        for task in cards {
            let placement = self.placements.get(&task.id).cloned().unwrap_or_default();
            let column_id = overrides
                .get(&task.id)
                .cloned()
                .or(placement.column.clone());
            let index = column_id
                .and_then(|id| drawn.iter().position(|column| column.id == id))
                .unwrap_or(0);
            if let Some(column) = by_column.get_mut(index) {
                column.push(task.clone());
            }
            let quadrant = takt_core::board_rules::matrix_quadrant(
                placement.urgency.map(|v| v as i32),
                placement.importance.map(|v| v as i32),
            );
            match quadrant {
                Some(q) => quadrants[q as usize].push((placement.rank.unwrap_or(i64::MAX), task)),
                None => unplaced.push(task),
            }
        }
        for quadrant in &mut quadrants {
            quadrant.sort_by_key(|(rank, _)| *rank);
        }
        let column = if self.board.key == key {
            self.board.column.min(drawn.len().saturating_sub(1))
        } else {
            0
        };
        self.board = Board {
            key,
            layout,
            columns: drawn,
            cards: by_column,
            quadrants: quadrants
                .into_iter()
                .map(|q| q.into_iter().map(|(_, task)| task).collect())
                .collect(),
            unplaced,
            column,
        };
        self.keep_board_selection();
    }

    /// The cards in reading order for the keys: the focused column's on the
    /// board; on the matrix, quadrant by quadrant then the unplaced.
    pub fn board_ids(&self) -> Vec<String> {
        match self.view_mode {
            ViewMode::Board => self
                .board
                .cards
                .get(self.board.column)
                .map(|cards| cards.iter().map(|task| task.id.clone()).collect())
                .unwrap_or_default(),
            ViewMode::Matrix => self
                .board
                .quadrants
                .iter()
                .flatten()
                .chain(self.board.unplaced.iter())
                .map(|task| task.id.clone())
                .collect(),
            _ => Vec::new(),
        }
    }

    fn keep_board_selection(&mut self) {
        if self.view_mode == ViewMode::Board {
            // The selection's own column takes the keyboard.
            if let Some(id) = &self.selected_task
                && let Some(column) = self
                    .board
                    .cards
                    .iter()
                    .position(|cards| cards.iter().any(|task| &task.id == id))
            {
                self.board.column = column;
                return;
            }
        }
        let ids = self.board_ids();
        if !self
            .selected_task
            .as_ref()
            .is_some_and(|id| ids.contains(id))
        {
            self.selected_task = ids.first().cloned();
        }
    }

    /// `h`/`l` on the board: the next column that has cards, or the
    /// adjacent one when none does.
    pub fn move_board_column(&mut self, by: i64) {
        let count = self.board.columns.len();
        if count == 0 {
            return;
        }
        let mut index = self.board.column as i64;
        loop {
            index += by;
            if index < 0 || index >= count as i64 {
                break;
            }
            if !self.board.cards[index as usize].is_empty() {
                self.board.column = index as usize;
                self.selected_task = self.board.cards[index as usize]
                    .first()
                    .map(|task| task.id.clone());
                return;
            }
        }
        let next = (self.board.column as i64 + by).clamp(0, count as i64 - 1) as usize;
        self.board.column = next;
        self.selected_task = self.board.cards[next].first().map(|task| task.id.clone());
    }

    /// `⇧H`/`⇧L`: the selected card into the next column.
    pub fn move_card_to_adjacent_column(&mut self, by: i64) {
        let Some(id) = self.selected_task.clone() else {
            return;
        };
        let target = self.board.column as i64 + by;
        let Some(column) = usize::try_from(target)
            .ok()
            .and_then(|ix| self.board.columns.get(ix))
            .map(|column| column.id.clone())
        else {
            return;
        };
        let key = self.board.key.clone();
        let zone = self.store.zone.clone();
        if self
            .write(|store| {
                store
                    .core
                    .move_to_board_column(key, vec![id.clone()], column, now_ms(), zone)
            })
            .is_some()
        {
            self.read_scope_into_rows();
            self.board.column = target as usize;
            self.selected_task = Some(id);
            self.layout_board();
            self.reload_today();
        }
    }

    /// `1`–`4` on the matrix: the selected card into a quadrant.
    pub fn place_in_quadrant(&mut self, quadrant: usize) {
        let Some(id) = self.selected_task.clone() else {
            return;
        };
        let (urgency, importance) = match quadrant {
            0 => (1, 1),
            1 => (0, 1),
            2 => (1, 0),
            _ => (0, 0),
        };
        if self
            .write(|store| {
                store.core.set_matrix_position(
                    id.clone(),
                    Some(urgency),
                    Some(importance),
                    now_ms(),
                )
            })
            .is_some()
        {
            let placement = self.placements.entry(id).or_default();
            placement.urgency = Some(urgency);
            placement.importance = Some(importance);
            placement.rank = None;
            self.layout_board();
        }
    }

    fn read_scope_into_rows(&mut self) {
        self.all_rows = self.read_scope();
        self.layout_rows();
    }
}

/// A row of a card's subtree.
#[derive(Debug, Clone)]
pub struct CardRow {
    pub task: TaskRow,
    pub depth: usize,
    /// The column it stands in as a card of its own, when that is another.
    pub elsewhere: Option<String>,
}

/// How many subtask rows a card shows before "+N more".
pub const CARD_SUBTASK_ROWS: usize = 6;

impl App {
    fn card_columns(&self) -> std::collections::HashMap<&str, usize> {
        self.board
            .cards
            .iter()
            .enumerate()
            .flat_map(|(ix, cards)| cards.iter().map(move |t| (t.id.as_str(), ix)))
            .collect()
    }

    /// A card's whole subtree, a compact row per task, and how many rows
    /// there are in all. A subtask filed in another column keeps its row,
    /// naming that column; its own subtasks are that card's to draw.
    pub fn card_subtree(&self, card: &TaskRow) -> (Vec<CardRow>, usize) {
        let columns = self.card_columns();
        let own = columns.get(card.id.as_str()).copied();
        let mut children: std::collections::HashMap<&str, Vec<&TaskRow>> = Default::default();
        for row in &self.all_rows {
            if let Some(parent) = row.task.parent_task_id.as_deref() {
                children.entry(parent).or_default().push(&row.task);
            }
        }
        let hide = self.prefs.hide_completed;
        let mut out = Vec::new();
        let mut stack: Vec<(&TaskRow, usize)> = children
            .get(card.id.as_str())
            .map(|kids| kids.iter().rev().map(|t| (*t, 0)).collect())
            .unwrap_or_default();
        while let Some((task, depth)) = stack.pop() {
            if hide && task.status != "open" && !self.lingering.contains(&task.id) {
                continue;
            }
            let elsewhere = columns
                .get(task.id.as_str())
                .filter(|ix| Some(**ix) != own)
                .and_then(|ix| self.board.columns.get(*ix))
                .map(|column| column.title.clone());
            let filed_elsewhere = elsewhere.is_some();
            out.push(CardRow {
                task: task.clone(),
                depth,
                elsewhere,
            });
            if filed_elsewhere {
                continue;
            }
            if let Some(kids) = children.get(task.id.as_str()) {
                stack.extend(kids.iter().rev().map(|t| (*t, depth + 1)));
            }
        }
        let total = out.len();
        out.truncate(CARD_SUBTASK_ROWS);
        (out, total)
    }

    /// Whose subtask a card is, when it is one: "↳ parent", with the
    /// parent's column when the parent is a card in another.
    pub fn card_parent_hint(&self, card: &TaskRow) -> Option<String> {
        let parent_id = card.parent_task_id.as_deref()?;
        let parent = self
            .all_rows
            .iter()
            .map(|r| &r.task)
            .find(|t| t.id == parent_id)?;
        let columns = self.card_columns();
        let column = columns
            .get(parent_id)
            .filter(|ix| columns.get(card.id.as_str()) != Some(*ix))
            .and_then(|ix| self.board.columns.get(*ix))
            .map(|c| format!(" ({})", c.title))
            .unwrap_or_default();
        Some(format!("↳ {}{column}", parent.title))
    }
}
