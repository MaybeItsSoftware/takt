//! The board and the matrix: the scope's cards laid out in the board's
//! columns, or at their points on the plane of urgency and importance
//! (`matrix.rs`).
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
    /// The matrix: every placed task at its point, quadrant by quadrant
    /// (do now, schedule, delegate, eliminate), each from the top and then
    /// from the urgent end.
    pub points: Vec<MatrixPoint>,
    /// The same tasks by quadrant, in that order.
    pub quadrants: Vec<Vec<TaskRow>>,
    pub unplaced: Vec<TaskRow>,
    /// The column with the keyboard.
    pub column: usize,
}

/// A task on the matrix's plane, at the point it is drawn: its stored one,
/// or where a legacy quadrant placement reads (`matrix::arrange`).
#[derive(Debug, Clone)]
pub struct MatrixPoint {
    pub task: TaskRow,
    pub urgency: i64,
    pub importance: i64,
}

/// A stored matrix place: urgency, importance and the old in-quadrant rank.
type StoredPoint = (i64, i64, Option<i64>);

pub const QUADRANT_TITLES: [&str; 4] = ["Do now", "Schedule", "Delegate", "Eliminate"];

impl App {
    /// The board's key in the core, as the Swift app spells it.
    pub fn board_key(&self) -> String {
        match &self.scope {
            // Today's cards sit in the columns they have in Everything.
            Scope::Everything | Scope::FilterList(_) | Scope::Today => "everything/root".into(),
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
        let mut cards = self.cards();
        cards.extend(self.cross_column_cards(&cards, &overrides, &drawn));
        let mut by_column: Vec<Vec<TaskRow>> = vec![Vec::new(); drawn.len().max(1)];
        // Each placed task with its stored urgency, importance and old rank.
        let mut placed: Vec<(TaskRow, StoredPoint)> = Vec::new();
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
            match (placement.urgency, placement.importance) {
                (Some(urgency), Some(importance)) => {
                    placed.push((task, (urgency, importance, placement.rank)))
                }
                _ => unplaced.push(task),
            }
        }
        let stored: Vec<_> = placed.iter().map(|(_, stored)| *stored).collect();
        let mut points: Vec<MatrixPoint> = placed
            .into_iter()
            .zip(super::matrix::arrange(&stored))
            .map(|((task, _), (urgency, importance))| MatrixPoint {
                task,
                urgency,
                importance,
            })
            .collect();
        points.sort_by_key(|point| {
            (
                super::matrix::quadrant_of(point.urgency, point.importance),
                -point.importance,
                -point.urgency,
            )
        });
        let mut quadrants: Vec<Vec<TaskRow>> = vec![Vec::new(); 4];
        for point in &points {
            quadrants[super::matrix::quadrant_of(point.urgency, point.importance)]
                .push(point.task.clone());
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
            points,
            quadrants,
            unplaced,
            column,
        };
        self.keep_board_selection();
    }

    /// The subtasks filed in another column than their parent card's,
    /// which stand as cards of their own there (the core's
    /// `board_cross_column`, as the Swift app and Android ask it). A
    /// subtask nobody filed is in its parent's column, on its parent's card.
    fn cross_column_cards(
        &self,
        cards: &[TaskRow],
        overrides: &std::collections::HashMap<String, String>,
        drawn: &[BoardColumn],
    ) -> Vec<TaskRow> {
        let mut children: std::collections::HashMap<&str, Vec<&TaskRow>> = Default::default();
        for row in &self.all_rows {
            if let Some(parent) = row.task.parent_task_id.as_deref() {
                children.entry(parent).or_default().push(&row.task);
            }
        }
        // The cards, then each card's tree beneath it, in drawing order.
        let mut tree: Vec<&TaskRow> = cards.iter().collect();
        let mut index: std::collections::HashMap<&str, usize> = tree
            .iter()
            .enumerate()
            .map(|(ix, task)| (task.id.as_str(), ix))
            .collect();
        let mut parents: Vec<i32> = vec![-1; tree.len()];
        for card in cards {
            let mut stack: Vec<&TaskRow> = children
                .get(card.id.as_str())
                .map(|kids| kids.iter().rev().copied().collect())
                .unwrap_or_default();
            while let Some(task) = stack.pop() {
                if index.contains_key(task.id.as_str()) {
                    continue;
                }
                index.insert(task.id.as_str(), tree.len());
                let parent = task
                    .parent_task_id
                    .as_deref()
                    .and_then(|p| index.get(p))
                    .map_or(-1, |ix| *ix as i32);
                parents.push(parent);
                tree.push(task);
                if let Some(kids) = children.get(task.id.as_str()) {
                    stack.extend(kids.iter().rev().copied());
                }
            }
        }
        let column_of = |task: &TaskRow| -> Option<String> {
            overrides.get(&task.id).cloned().or_else(|| {
                self.placements
                    .get(&task.id)
                    .and_then(|placement| placement.column.clone())
            })
        };
        let code = |id: Option<String>| -> i32 {
            id.and_then(|id| drawn.iter().position(|column| column.id == id))
                .map_or(-1, |ix| ix as i32)
        };
        let filed: Vec<i32> = tree.iter().map(|task| code(column_of(task))).collect();
        if filed[cards.len()..].iter().all(|code| *code < 0) {
            return Vec::new();
        }
        let hide = self.prefs.hide_completed;
        let eligible: Vec<bool> = tree
            .iter()
            .enumerate()
            .map(|(ix, task)| {
                ix >= cards.len()
                    && task.item_kind.as_deref() != Some("list")
                    && !(hide && task.status != "open" && !self.lingering.contains(&task.id))
            })
            .collect();
        takt_core::board_rules::board_cross_column(parents, filed, eligible, 0)
            .into_iter()
            .map(|ix| tree[ix as usize].clone())
            .collect()
    }

    /// The cards in reading order for the keys: the focused column's on the
    /// board, or the subtask rows of the card the selection is inside; on
    /// the matrix, the plane's quadrant by quadrant then the unplaced.
    pub fn board_ids(&self) -> Vec<String> {
        match self.view_mode {
            ViewMode::Board => {
                if let Some(id) = self.selected_task.as_deref()
                    && let Some((column, card)) = self.card_holding(id)
                    && column == self.board.column
                {
                    return self.card_row_ids(&card);
                }
                self.board
                    .cards
                    .get(self.board.column)
                    .map(|cards| cards.iter().map(|task| task.id.clone()).collect())
                    .unwrap_or_default()
            }
            ViewMode::Matrix => self
                .board
                .points
                .iter()
                .map(|point| &point.task)
                .chain(self.board.unplaced.iter())
                .map(|task| task.id.clone())
                .collect(),
            _ => Vec::new(),
        }
    }

    fn keep_board_selection(&mut self) {
        if self.view_mode == ViewMode::Board
            && let Some(id) = self.selected_task.clone()
        {
            // The selection's own column takes the keyboard: the card's, or
            // the column of the card a selected subtask row is drawn on.
            let column = self
                .board
                .cards
                .iter()
                .position(|cards| cards.iter().any(|task| task.id == id))
                .or_else(|| self.card_holding(&id).map(|(column, _)| column));
            if let Some(column) = column {
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

    pub(super) fn read_scope_into_rows(&mut self) {
        self.all_rows = self.read_scope();
        self.layout_rows();
    }
}

/// Where a new card is drafted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardPlace {
    Column(usize),
    /// A matrix quadrant (the new card lands at its centre), or the
    /// unplaced cards.
    Quadrant(Option<usize>),
}

impl App {
    /// Enter on the board or the matrix: a draft card below the selected
    /// card in the focused column (at its foot when none is selected there), or at the centre of the selected card's quadrant, in
    /// the selected card's list (or the list on screen).
    pub fn begin_new_card(&mut self) {
        let selected = self.selected().cloned();
        let Some(list_id) = selected
            .as_ref()
            .map(|task| task.list_id.clone())
            .or_else(|| self.scope_list_id())
        else {
            self.message = Some("Select a card first, so the new one has a list.".into());
            return;
        };
        let place = match self.view_mode {
            ViewMode::Matrix => CardPlace::Quadrant(selected.as_ref().and_then(|task| {
                self.board
                    .quadrants
                    .iter()
                    .position(|cards| cards.iter().any(|card| card.id == task.id))
            })),
            _ => CardPlace::Column(self.board.column),
        };
        // Below the selected card, or the card a selected subtask is drawn
        // on, when it is a top-level card of this list in that column: cards
        // stand in their list's order, so the new one goes in beside it. A
        // subtask standing as a card of its own would make the new one its
        // sibling, so that drafts at the foot instead.
        let after = match place {
            CardPlace::Column(column) => selected
                .and_then(|task| self.card_in_column(&task.id, column))
                .and_then(|id| self.task(&id).cloned())
                .filter(|card| card.parent_task_id.is_none() && card.list_id == list_id)
                .map(|card| card.id),
            CardPlace::Quadrant(_) => None,
        };
        self.editing = Some(super::Editing::NewCard {
            list_id,
            place,
            after,
        });
    }

    /// The card in `column` that `id` is, or is drawn on as a subtask.
    pub fn card_in_column(&self, id: &str, column: usize) -> Option<String> {
        let cards = self.board.cards.get(column)?;
        let mut at = Some(id.to_string());
        while let Some(id) = at {
            if cards.iter().any(|card| card.id == id) {
                return Some(id);
            }
            at = self.task(&id).and_then(|task| task.parent_task_id.clone());
        }
        None
    }

    /// Adds the drafted card, files it where it was drafted, closes the
    /// draft and selects it. False when the text was kept back, its problem
    /// in the status bar.
    pub fn create_card(&mut self, title: &str) -> bool {
        let Some(super::Editing::NewCard {
            list_id,
            place,
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
        let new = match self.new_task_from(title, &list_id) {
            Ok(new) => takt_core::tasks::NewTask {
                parent_task_id: None,
                at_top: false,
                adjacent_task_id: after,
                ..new
            },
            Err(problem) => {
                self.message = Some(problem);
                return false;
            }
        };
        let Some(id) = self.write(|store| store.core.create_task(new, now_ms())) else {
            return false;
        };
        self.read_scope_into_rows();
        self.selected_task = Some(id.clone());
        match place {
            CardPlace::Column(column) => {
                let key = self.board.key.clone();
                let zone = self.store.zone.clone();
                if let Some(column_id) = self.board.columns.get(column).map(|c| c.id.clone()) {
                    self.write(|store| {
                        store.core.move_to_board_column(
                            key,
                            vec![id.clone()],
                            column_id,
                            now_ms(),
                            zone,
                        )
                    });
                    self.read_scope_into_rows();
                }
                self.board.column = column;
            }
            CardPlace::Quadrant(Some(quadrant)) => self.place_in_quadrant(quadrant),
            CardPlace::Quadrant(None) => {}
        }
        self.layout_board();
        self.selected_task = Some(id);
        self.reload_today();
        self.editing = None;
        true
    }
}

/// A row of a card's subtree.
#[derive(Debug, Clone)]
pub struct CardRow {
    pub task: TaskRow,
    pub depth: usize,
    /// The column it stands in as a card of its own, when that is another.
    pub elsewhere: Option<String>,
    /// Its own subtasks are folded away (`,`), so none are drawn under it.
    pub folded: bool,
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
        let folded = &self.prefs.folded_tasks;
        let mut out = Vec::new();
        if folded.contains(&card.id) {
            return (out, 0);
        }
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
            let kids = children
                .get(task.id.as_str())
                .filter(|kids| !kids.is_empty());
            let folded_away = kids.is_some() && folded.contains(&task.id);
            out.push(CardRow {
                task: task.clone(),
                depth,
                elsewhere,
                folded: folded_away && !filed_elsewhere,
            });
            if filed_elsewhere || folded_away {
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

    /// How many subtasks a folded card keeps out of sight; none when the
    /// card is not folded or has none.
    pub fn card_folded_count(&self, card: &TaskRow) -> usize {
        if !self.prefs.folded_tasks.contains(&card.id) {
            return 0;
        }
        let hide = self.prefs.hide_completed;
        self.all_rows
            .iter()
            .filter(|row| row.task.parent_task_id.as_deref() == Some(card.id.as_str()))
            .filter(|row| {
                !(hide && row.task.status != "open" && !self.lingering.contains(&row.task.id))
            })
            .count()
    }

    /// The card in the board's columns whose drawn subtask rows hold `id`,
    /// and that card's column; none when `id` is a card itself, or is not
    /// drawn on one (folded away, past "+N more", or a card elsewhere).
    pub fn card_holding(&self, id: &str) -> Option<(usize, TaskRow)> {
        let columns = self.card_columns();
        if columns.contains_key(id) {
            return None;
        }
        let parent_of = |id: &str| -> Option<&str> {
            self.all_rows
                .iter()
                .find(|row| row.task.id == id)
                .and_then(|row| row.task.parent_task_id.as_deref())
        };
        // The nearest ancestor that is a card is the one drawing it: a
        // subtask that is a card elsewhere draws its own subtree.
        let mut parent = parent_of(id);
        let (column, card_id) = loop {
            let here = parent?;
            if let Some(column) = columns.get(here) {
                break (*column, here);
            }
            parent = parent_of(here);
        };
        let card = self
            .board
            .cards
            .get(column)?
            .iter()
            .find(|c| c.id == card_id)?;
        let drawn = self
            .card_subtree(card)
            .0
            .iter()
            .any(|row| row.task.id == id && row.elsewhere.is_none());
        drawn.then(|| (column, card.clone()))
    }

    /// The subtask rows the keys stand on inside a card: every one drawn,
    /// but those that are cards in another column, which are reached there.
    pub fn card_row_ids(&self, card: &TaskRow) -> Vec<String> {
        self.card_subtree(card)
            .0
            .into_iter()
            .filter(|row| row.elsewhere.is_none())
            .map(|row| row.task.id)
            .collect()
    }

    /// `l` on a board card: the cursor onto its first subtask row, unfolding
    /// the card first when it is folded.
    pub fn enter_card_rows(&mut self) {
        if self.view_mode != ViewMode::Board {
            return;
        }
        let Some(id) = self.selected_task.clone() else {
            return;
        };
        let Some(card) = self
            .board
            .cards
            .get(self.board.column)
            .and_then(|cards| cards.iter().find(|card| card.id == id))
            .cloned()
        else {
            return;
        };
        if self.prefs.folded_tasks.remove(&card.id) {
            self.prefs.save();
            self.layout_rows();
        }
        match self.card_row_ids(&card).into_iter().next() {
            Some(first) => self.selected_task = Some(first),
            None => self.message = Some("This card has no subtasks to step into.".into()),
        }
    }

    /// `h` (or `Esc`) on a subtask row: the cursor back onto its card. False
    /// when the selection is not inside a card.
    pub fn leave_card_rows(&mut self) -> bool {
        if self.view_mode != ViewMode::Board {
            return false;
        }
        let Some((column, card)) = self
            .selected_task
            .as_deref()
            .and_then(|id| self.card_holding(id))
        else {
            return false;
        };
        self.board.column = column;
        self.selected_task = Some(card.id);
        true
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
