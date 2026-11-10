//! The minute selector that floats under the inspector's Log time row:
//! a count of minutes the arrows step and digits type, with a row of common
//! lengths to click. Return logs it against the task, Esc leaves.

use super::{App, Editing, Region};

/// Common lengths, in minutes, offered as one click.
pub const PRESETS: [u32; 6] = [15, 25, 30, 45, 60, 90];
/// The longest stretch one entry logs: twelve hours.
pub const MAX_MINUTES: u32 = 12 * 60;
/// What the selector opens on.
const START: u32 = 25;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEdit {
    pub task_id: String,
    pub minutes: u32,
    /// Digits are being typed, so the next one grows the number rather
    /// than starting a new one.
    typing: bool,
}

impl LogEdit {
    pub fn new(task_id: String) -> Self {
        Self {
            task_id,
            minutes: START,
            typing: false,
        }
    }

    /// A key; true when it was the selector's. Enter and Esc are the App's.
    pub fn key(&mut self, key: &str) -> bool {
        if let Some(digit) = key
            .chars()
            .next()
            .filter(|_| key.len() == 1)
            .and_then(|c| c.to_digit(10))
        {
            self.type_digit(digit);
            return true;
        }
        self.typing = false;
        match key {
            "up" | "k" => self.step(5),
            "down" | "j" => self.step(-5),
            "shift+up" | "shift+k" | "K" => self.step(15),
            "shift+down" | "shift+j" | "J" => self.step(-15),
            "right" | "l" | "tab" => self.preset(1),
            "left" | "h" | "shift+tab" => self.preset(-1),
            "backspace" => {
                self.minutes /= 10;
                self.typing = self.minutes > 0;
            }
            _ => return false,
        }
        true
    }

    /// The arrows move in fives, landing on a multiple of the step.
    fn step(&mut self, by: i32) {
        let at = self.minutes as i32;
        let next = if by > 0 {
            (at / by + 1) * by
        } else {
            let step = -by;
            ((at + step - 1) / step - 1) * step
        };
        self.minutes = next.clamp(1, MAX_MINUTES as i32) as u32;
    }

    /// ←/→ walk the presets from wherever the count is.
    fn preset(&mut self, by: i32) {
        let next = if by > 0 {
            PRESETS.iter().find(|&&p| p > self.minutes)
        } else {
            PRESETS.iter().rev().find(|&&p| p < self.minutes)
        };
        if let Some(&p) = next {
            self.minutes = p;
        }
    }

    fn type_digit(&mut self, digit: u32) {
        let grown = self.minutes * 10 + digit;
        self.minutes = if self.typing && grown <= MAX_MINUTES {
            grown
        } else {
            digit
        };
        self.typing = true;
    }

    /// A preset chosen by a click.
    pub fn choose(&mut self, minutes: u32) {
        self.minutes = minutes.clamp(1, MAX_MINUTES);
        self.typing = false;
    }

    /// The count as read back: "25 min", "1 h 30 min".
    pub fn label(&self) -> String {
        minutes_text(self.minutes)
    }
}

pub fn minutes_text(minutes: u32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

impl App {
    /// Return on the Log time row: the selector opens.
    pub fn open_log_edit(&mut self) {
        let Some(task_id) = self.selected_task.clone() else {
            return;
        };
        self.region = Region::Inspector;
        self.inspecting_container = false;
        self.editing = Some(Editing::Log(LogEdit::new(task_id)));
    }

    pub fn log_edit_mut(&mut self) -> Option<&mut LogEdit> {
        match self.editing.as_mut() {
            Some(Editing::Log(edit)) => Some(edit),
            _ => None,
        }
    }

    pub fn log_key(&mut self, key: &str) {
        let Some(edit) = self.log_edit_mut() else {
            return;
        };
        // A typed 0 alone would log nothing; Return then waits for more.
        if edit.key(key) {
            return;
        }
        match key {
            "enter" | "space" => self.commit_log(),
            "escape" => self.editing = None,
            _ => {}
        }
    }

    pub fn commit_log(&mut self) {
        let Some(Editing::Log(edit)) = self.editing.take() else {
            return;
        };
        if edit.minutes == 0 {
            self.editing = Some(Editing::Log(edit));
            return;
        }
        if self.selected_task.as_deref() == Some(edit.task_id.as_str()) {
            self.log_selected_work(edit.minutes as i64 * 60);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(edit: &mut LogEdit, keys: &[&str]) {
        for k in keys {
            edit.key(k);
        }
    }

    #[test]
    fn arrows_step_in_fives_and_land_on_them() {
        let mut e = LogEdit::new("t".into());
        keys(&mut e, &["up"]);
        assert_eq!(e.minutes, 30);
        keys(&mut e, &["2", "7", "down"]);
        assert_eq!(e.minutes, 25);
        keys(&mut e, &["down", "down", "down", "down", "down", "down"]);
        assert_eq!(e.minutes, 1);
        keys(&mut e, &["shift+up"]);
        assert_eq!(e.minutes, 15);
    }

    #[test]
    fn digits_type_a_count_and_presets_walk() {
        let mut e = LogEdit::new("t".into());
        keys(&mut e, &["9", "0"]);
        assert_eq!(e.label(), "1 h 30 min");
        keys(&mut e, &["left"]);
        assert_eq!(e.minutes, 60);
        keys(&mut e, &["right", "right"]);
        assert_eq!(e.minutes, 90);
        keys(&mut e, &["1", "2", "3", "4"]);
        assert_eq!(e.minutes, 4, "past twelve hours starts again");
        assert_eq!(minutes_text(120), "2 h");
    }
}
