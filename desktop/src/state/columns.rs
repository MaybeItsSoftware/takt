//! Editing a board's columns: naming, adding, deleting and moving them,
//! their rules, and the workspace's default. `WorkspaceBoardColumnViews`
//! and the column half of `WorkspaceViewModel+Board.swift`.
//!
//! A board's layout is the columns its key saves, hidden ones included;
//! every edit saves the whole layout as one step, and the board is laid out
//! again from it.

use super::rules;
use super::store::now_ms;
use super::{App, Editing, ViewMode};
use takt_core::board_columns::BoardColumn;
use takt_core::board_rules::board_column_id;

/// Where a typed column name goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnName {
    Rename(String),
    /// A new column at this index of the layout.
    New(usize),
}

/// Whose rules the rules editor holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RulesTarget {
    Column(String),
    FilterList(String),
    NewFilterList,
    /// A periodic commitment to `task_id`, or editing the one on `editing`.
    Commitment {
        task_id: String,
        editing: Option<String>,
    },
    /// A habit: editing the one on `habit_task_id`, or a new one, made from
    /// `source` when there is one.
    Habit {
        habit_task_id: Option<String>,
        source: Option<String>,
    },
    List(String),
    Folder(String),
}

impl RulesTarget {
    /// A form of `key value` lines rather than filter rules.
    pub fn is_form(&self) -> bool {
        matches!(
            self,
            RulesTarget::Commitment { .. }
                | RulesTarget::Habit { .. }
                | RulesTarget::List(_)
                | RulesTarget::Folder(_)
        )
    }

    pub fn title(&self) -> &'static str {
        match self {
            RulesTarget::Column(_) => "Column rules",
            RulesTarget::NewFilterList => "New filter list",
            RulesTarget::FilterList(_) => "Filter list",
            RulesTarget::Commitment { editing: None, .. } => "Commit periodically",
            RulesTarget::Commitment { .. } => "Commitment",
            RulesTarget::Habit {
                habit_task_id: None,
                ..
            } => "New habit",
            RulesTarget::Habit { .. } => "Habit",
            RulesTarget::List(_) => "List settings",
            RulesTarget::Folder(_) => "Folder settings",
        }
    }
}

impl App {
    /// The column with the keyboard, in the board's layout.
    fn focused_column(&self) -> Option<BoardColumn> {
        self.board.columns.get(self.board.column).cloned()
    }

    fn layout_index(&self, id: &str) -> Option<usize> {
        self.board.layout.iter().position(|column| column.id == id)
    }

    fn on_board(&mut self) -> bool {
        if self.view_mode != ViewMode::Board {
            self.message = Some("That's the board's; ⌘1 shows it.".into());
            return false;
        }
        true
    }

    /// Saves `columns` as the board's layout, as one step called `label`.
    fn save_columns(&mut self, columns: Vec<BoardColumn>, label: &str) -> bool {
        let key = self.board.key.clone();
        let label = label.to_string();
        let saved = self
            .write(|s| {
                s.core
                    .set_board_columns(key, columns, Vec::new(), None, label, now_ms())
            })
            .is_some();
        if saved {
            self.layout_board();
        }
        saved
    }

    pub fn begin_rename_column(&mut self) {
        if !self.on_board() {
            return;
        }
        if let Some(column) = self.focused_column() {
            self.editing = Some(Editing::ColumnName(ColumnName::Rename(column.id)));
        }
    }

    /// A new column: at the end, or beside the focused one.
    pub fn begin_new_column(&mut self, beside: Option<i64>) {
        if !self.on_board() {
            return;
        }
        let at = match (beside, self.focused_column()) {
            (Some(offset), Some(column)) => {
                let here = self.layout_index(&column.id).unwrap_or(0);
                if offset < 0 { here } else { here + 1 }
            }
            _ => self.board.layout.len(),
        };
        self.editing = Some(Editing::ColumnName(ColumnName::New(at)));
    }

    pub fn commit_column_name(&mut self, target: ColumnName, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let mut columns = self.board.layout.clone();
        match target {
            ColumnName::Rename(id) => {
                let Some(column) = columns.iter_mut().find(|c| c.id == id) else {
                    return;
                };
                column.title = name.to_string();
                self.save_columns(columns, "Rename Column");
            }
            ColumnName::New(at) => {
                let taken = columns.iter().map(|c| c.id.clone()).collect();
                let column = BoardColumn {
                    id: board_column_id(name.into(), taken),
                    title: name.to_string(),
                    rules: None,
                };
                let id = column.id.clone();
                columns.insert(at.min(columns.len()), column);
                if self.save_columns(columns, "Add Column")
                    && let Some(ix) = self.board.columns.iter().position(|c| c.id == id)
                {
                    self.board.column = ix;
                }
            }
        }
    }

    /// Asks first when the column holds cards, which go to the first column
    /// left.
    pub fn ask_to_delete_column(&mut self) {
        if !self.on_board() {
            return;
        }
        let Some(column) = self.focused_column() else {
            return;
        };
        if self.board.layout.len() <= 1 {
            self.message = Some("A board keeps at least one column.".into());
            return;
        }
        let cards = self.board.cards.get(self.board.column).map_or(0, Vec::len);
        if cards == 0 {
            self.delete_column(&column.id);
            return;
        }
        self.pending_delete = Some(super::actions::PendingDelete::Column(column.id.clone()));
        self.message = Some(format!(
            "Delete \u{201c}{}\u{201d}? Its {cards} card{} go to the first column. Return deletes.",
            column.title,
            if cards == 1 { "" } else { "s" }
        ));
    }

    pub fn delete_column(&mut self, id: &str) {
        let to = self
            .board
            .layout
            .iter()
            .find(|column| column.id != id && column.rules.is_none())
            .map(|column| column.id.clone());
        let key = self.board.key.clone();
        let lists = self.scope_list_ids();
        let id = id.to_string();
        if self
            .write(|s| s.core.delete_board_column(key, lists, id, to, now_ms()))
            .is_some()
        {
            self.after_structure_change();
        }
    }

    pub fn move_column(&mut self, by: i64) {
        if !self.on_board() {
            return;
        }
        let Some(column) = self.focused_column() else {
            return;
        };
        let Some(from) = self.layout_index(&column.id) else {
            return;
        };
        let to = from as i64 + by;
        if to < 0 || to >= self.board.layout.len() as i64 {
            return;
        }
        let mut columns = self.board.layout.clone();
        let moved = columns.remove(from);
        columns.insert(to as usize, moved);
        if self.save_columns(columns, "Move Column")
            && let Some(ix) = self.board.columns.iter().position(|c| c.id == column.id)
        {
            self.board.column = ix;
        }
    }

    pub fn make_default_columns(&mut self) {
        let key = self.board.key.clone();
        if self
            .write(|s| s.core.make_default_board_columns(key))
            .is_some()
        {
            self.message = Some("These are now the default columns.".into());
            self.layout_board();
        }
    }

    pub fn reset_columns(&mut self) {
        let key = self.board.key.clone();
        if self.write(|s| s.core.reset_board_columns(key)).is_some() {
            self.message = Some("Back to the default columns.".into());
            self.layout_board();
        }
    }

    // MARK: Rules

    pub fn begin_column_rules(&mut self) {
        if !self.on_board() {
            return;
        }
        if let Some(column) = self.focused_column() {
            self.editing = Some(Editing::Rules(RulesTarget::Column(column.id)));
        }
    }

    pub fn begin_filter_list_edit(&mut self, new: bool) {
        let target = if new {
            RulesTarget::NewFilterList
        } else {
            let id = match self.sidebar_cursor_row().map(|r| r.kind.clone()) {
                Some(super::sidebar::RowKind::FilterList(id)) => Some(id),
                _ => match &self.scope {
                    super::Scope::FilterList(id) => Some(id.clone()),
                    _ => None,
                },
            };
            match id {
                Some(id) => RulesTarget::FilterList(id),
                None => {
                    self.message = Some("Go to a filter list to edit it.".into());
                    return;
                }
            }
        };
        self.editing = Some(Editing::Rules(target));
    }

    /// What the rules editor opens with.
    pub fn rules_text(&self, target: &RulesTarget) -> String {
        if target.is_form() {
            return self.form_text(target);
        }
        match target {
            RulesTarget::Column(id) => {
                let definition = self
                    .board
                    .layout
                    .iter()
                    .find(|c| c.id == *id)
                    .and_then(|c| c.rules.clone())
                    .unwrap_or_default();
                rules::write(None, &definition)
            }
            RulesTarget::FilterList(id) => self
                .filter_lists
                .iter()
                .find(|f| f.id == *id)
                .map(|f| rules::write(Some(&f.name), &f.definition))
                .unwrap_or_default(),
            RulesTarget::NewFilterList => rules::write(Some("New filter"), &Default::default()),
            _ => String::new(),
        }
    }

    /// Saves typed rules; false (with the reason in the status bar) when
    /// they don't read.
    pub fn commit_rules(&mut self, target: RulesTarget, text: &str) -> bool {
        if target.is_form() {
            return self.commit_form(target, text);
        }
        let reading = match rules::read(text) {
            Ok(reading) => reading,
            Err(error) => {
                self.message = Some(error);
                return false;
            }
        };
        let now = now_ms();
        match target {
            RulesTarget::Column(id) => {
                let mut columns = self.board.layout.clone();
                let Some(column) = columns.iter_mut().find(|c| c.id == id) else {
                    return true;
                };
                // No rules at all is a column tasks are filed in again.
                column.rules = (!reading.definition.rules.is_empty()).then_some(reading.definition);
                self.save_columns(columns, "Edit Column Rules");
            }
            RulesTarget::FilterList(id) => {
                let Some(filter) = self.filter_lists.iter().find(|f| f.id == id).cloned() else {
                    return true;
                };
                let name = reading.name.unwrap_or(filter.name);
                if self
                    .write(|s| {
                        s.core.update_filter_list(
                            id.clone(),
                            name,
                            filter.icon,
                            filter.color_hex,
                            reading.definition,
                            now,
                        )
                    })
                    .is_some()
                {
                    self.reload_sidebar();
                    self.reload_rows();
                }
            }
            RulesTarget::NewFilterList => {
                let name = reading.name.unwrap_or_else(|| "New filter".into());
                let workspace = self.store.workspace_id.clone();
                if let Some(row) = self.write(|s| {
                    s.core
                        .create_filter_list(workspace, name, None, None, reading.definition, now)
                }) {
                    self.reload_sidebar();
                    self.select_filter_list(&row.id);
                }
            }
            _ => {}
        }
        true
    }
}
