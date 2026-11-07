//! Dropping a dragged task: before another row, into a list in the
//! sidebar, into a board column or a matrix quadrant. Each is one undo
//! step through the core, the same writes the keys make.

use super::App;
use super::store::now_ms;

impl App {
    /// Before `target`, as its sibling: under the same parent, in its list.
    pub fn drop_task_before(&mut self, dragged: &str, target: &str) {
        if dragged == target {
            return;
        }
        let (dragged, target) = (dragged.to_string(), target.to_string());
        let done = self.write(|store| {
            store
                .core
                .move_task_before(dragged.clone(), target.clone(), None, now_ms())
        });
        if done.is_some() {
            self.selected_task = Some(dragged);
            self.reload_rows();
            self.reload_sidebar();
            self.reload_today();
        }
    }

    /// To the end of a list's top level.
    pub fn drop_task_into_list(&mut self, dragged: &str, list_id: &str) {
        if self
            .task(dragged)
            .is_some_and(|task| task.list_id == list_id && task.parent_task_id.is_none())
        {
            return;
        }
        self.selected_task = Some(dragged.to_string());
        self.move_selected_to_list(list_id);
        if let Some(list) = self.list(list_id) {
            self.message = Some(format!("Moved to {}.", list.name));
        }
    }

    pub fn drop_task_into_column(&mut self, dragged: &str, column: usize) {
        self.selected_task = Some(dragged.to_string());
        let Some(from) = self
            .board
            .cards
            .iter()
            .position(|cards| cards.iter().any(|task| task.id == dragged))
        else {
            return;
        };
        if from == column {
            return;
        }
        self.board.column = from;
        self.move_card_to_adjacent_column(column as i64 - from as i64);
    }

    pub fn drop_task_into_quadrant(&mut self, dragged: &str, quadrant: usize) {
        self.selected_task = Some(dragged.to_string());
        self.place_in_quadrant(quadrant);
    }
}
