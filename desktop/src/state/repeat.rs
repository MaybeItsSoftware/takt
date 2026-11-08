//! The repeat selector that opens under the inspector's repeat row:
//! "Every ‹3› ‹days›", set with the keyboard. Nothing in it is typed but
//! the count; its keys are [`App::repeat_key`]'s.
//!
//! It offers exactly what the core's cadences can hold (`periodic.rs`):
//! every n days, every n weeks, every weekday, or one named day.

use super::store::now_ms;
use super::{App, Editing, Region};
use takt_core::recurrence::{PeriodicCadence, periodic_cadence, periodic_next_occurrence};

/// Which half of "Every ‹count› ‹unit›" the arrows change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatField {
    Count,
    Unit,
}

/// What the count counts, or a day that needs no count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatUnit {
    Days,
    Weeks,
    Weekdays,
    /// 1 = Sunday through 7 = Saturday, as `PeriodicCadence` counts.
    Weekday(u32),
}

/// The units in the order ↑ and ↓ step through them: Monday first.
pub const UNITS: [RepeatUnit; 10] = [
    RepeatUnit::Days,
    RepeatUnit::Weeks,
    RepeatUnit::Weekdays,
    RepeatUnit::Weekday(2),
    RepeatUnit::Weekday(3),
    RepeatUnit::Weekday(4),
    RepeatUnit::Weekday(5),
    RepeatUnit::Weekday(6),
    RepeatUnit::Weekday(7),
    RepeatUnit::Weekday(1),
];

const DAY_NAMES: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

const MAX_COUNT: u32 = 365;

impl RepeatUnit {
    pub fn counts(self) -> bool {
        matches!(self, RepeatUnit::Days | RepeatUnit::Weeks)
    }

    /// The chip's name in the row of units.
    pub fn short(self) -> String {
        match self {
            RepeatUnit::Days => "day".into(),
            RepeatUnit::Weeks => "week".into(),
            RepeatUnit::Weekdays => "weekday".into(),
            RepeatUnit::Weekday(day) => DAY_NAMES[(day as usize + 6) % 7][..3].into(),
        }
    }

    /// The unit after "Every ‹count›", or after "Every" alone.
    pub fn label(self, count: u32) -> String {
        let plural = if count == 1 { "" } else { "s" };
        match self {
            RepeatUnit::Days => format!("day{plural}"),
            RepeatUnit::Weeks => format!("week{plural}"),
            RepeatUnit::Weekdays => "weekday".into(),
            RepeatUnit::Weekday(day) => DAY_NAMES[(day as usize + 6) % 7].into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeatEdit {
    pub task_id: String,
    pub count: u32,
    pub unit: RepeatUnit,
    pub field: RepeatField,
    /// Whether the task repeated when the selector opened, so ⌫ means
    /// something.
    pub had_rule: bool,
    /// A digit has been typed into the count since the last other key, so
    /// the next one adds to it rather than starting again.
    typing: bool,
}

impl RepeatEdit {
    /// Opens on the task's own rule, or on every day.
    pub fn new(task_id: String, rule: Option<&str>) -> Self {
        let cadence = rule.and_then(|rule| periodic_cadence(rule.into()));
        let (count, unit) = match cadence {
            Some(PeriodicCadence::Days { count }) => (count, RepeatUnit::Days),
            Some(PeriodicCadence::Weeks { count }) => (count, RepeatUnit::Weeks),
            Some(PeriodicCadence::Weekdays) => (1, RepeatUnit::Weekdays),
            Some(PeriodicCadence::Weekday { weekday }) => (1, RepeatUnit::Weekday(weekday)),
            None => (1, RepeatUnit::Days),
        };
        RepeatEdit {
            task_id,
            count,
            unit,
            field: if unit.counts() {
                RepeatField::Count
            } else {
                RepeatField::Unit
            },
            had_rule: cadence.is_some(),
            typing: false,
        }
    }

    pub fn cadence(&self) -> PeriodicCadence {
        let count = self.count.max(1);
        match self.unit {
            RepeatUnit::Days => PeriodicCadence::Days { count },
            RepeatUnit::Weeks => PeriodicCadence::Weeks { count },
            RepeatUnit::Weekdays => PeriodicCadence::Weekdays,
            RepeatUnit::Weekday(weekday) => PeriodicCadence::Weekday { weekday },
        }
    }

    /// What is stored: the spelling `Cadence::parse` reads back.
    pub fn rule(&self) -> String {
        match self.cadence() {
            PeriodicCadence::Days { count: 1 } => "daily".into(),
            PeriodicCadence::Days { count } => format!("every {count} days"),
            PeriodicCadence::Weeks { count: 1 } => "weekly".into(),
            PeriodicCadence::Weeks { count } => format!("every {count} weeks"),
            PeriodicCadence::Weekdays => "weekdays".into(),
            PeriodicCadence::Weekday { weekday } => {
                format!(
                    "every {}",
                    DAY_NAMES[(weekday as usize + 6) % 7].to_lowercase()
                )
            }
        }
    }

    /// A key; true when it was the selector's. Enter, ⌫ and Esc are the
    /// App's, since they close it.
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
            "left" | "h" | "shift+tab" => self.field = RepeatField::Count,
            "right" | "l" | "tab" => self.field = RepeatField::Unit,
            "up" | "k" => self.step(1),
            "down" | "j" => self.step(-1),
            "d" => self.choose(RepeatUnit::Days),
            "w" => self.choose(RepeatUnit::Weeks),
            _ => return false,
        }
        if !self.unit.counts() {
            self.field = RepeatField::Unit;
        }
        true
    }

    /// ↑ adds to the count, or moves up the units; ↓ the other way.
    fn step(&mut self, by: i32) {
        match self.field {
            RepeatField::Count => {
                self.count = (self.count as i32 + by).clamp(1, MAX_COUNT as i32) as u32;
            }
            RepeatField::Unit => {
                let at = UNITS.iter().position(|u| *u == self.unit).unwrap_or(0) as i32;
                let next = (at - by).rem_euclid(UNITS.len() as i32) as usize;
                self.unit = UNITS[next];
            }
        }
    }

    /// A unit chosen outright, by its letter or a click.
    pub fn choose(&mut self, unit: RepeatUnit) {
        self.unit = unit;
        self.field = RepeatField::Unit;
    }

    /// Digits build the count; on a named day they mean days.
    fn type_digit(&mut self, digit: u32) {
        if !self.unit.counts() {
            self.unit = RepeatUnit::Days;
        }
        self.field = RepeatField::Count;
        let grown = self.count * 10 + digit;
        self.count = if self.typing && grown <= MAX_COUNT {
            grown
        } else {
            digit
        };
        // A lone 0 waits for the digit after it; it is never stored.
        self.typing = true;
    }

    /// The next few times it would come round, from its due date or from
    /// now, as finishing it counts them.
    pub fn upcoming(&self, due_ms: Option<i64>, zone: &str, how_many: usize) -> Vec<i64> {
        let now = now_ms();
        let mut after = due_ms.unwrap_or(now);
        let mut dates = Vec::with_capacity(how_many);
        for _ in 0..how_many {
            let Some(next) =
                periodic_next_occurrence(self.cadence(), after, Some(now), zone.to_string())
            else {
                break;
            };
            dates.push(next);
            after = next;
        }
        dates
    }
}

impl App {
    /// Return on the repeat row: the selector opens on the task's rule.
    pub fn open_repeat_edit(&mut self) {
        let Some(task_id) = self.selected_task.clone() else {
            return;
        };
        let rule = self
            .inspected()
            .and_then(|snapshot| snapshot.metadata.recurrence_rule);
        self.region = Region::Inspector;
        self.editing = Some(Editing::Repeat(RepeatEdit::new(task_id, rule.as_deref())));
    }

    pub fn repeat_edit(&mut self) -> Option<&mut RepeatEdit> {
        match self.editing.as_mut() {
            Some(Editing::Repeat(edit)) => Some(edit),
            _ => None,
        }
    }

    /// A key while the selector is open: arrows change it, digits type the
    /// count, Return sets it, ⌫ stops it repeating, Esc leaves it as it was.
    pub fn repeat_key(&mut self, key: &str) {
        let Some(edit) = self.repeat_edit() else {
            return;
        };
        if edit.key(key) {
            return;
        }
        match key {
            "enter" | "space" => self.commit_repeat(),
            "backspace" | "delete" | "n" => self.clear_repeat(),
            "escape" => self.editing = None,
            _ => {}
        }
    }

    pub fn commit_repeat(&mut self) {
        let Some(Editing::Repeat(edit)) = self.editing.take() else {
            return;
        };
        if self.selected_task.as_deref() == Some(edit.task_id.as_str()) {
            let rule = edit.rule();
            self.edit_selected(|snapshot| snapshot.metadata.recurrence_rule = Some(rule));
        }
    }

    pub fn clear_repeat(&mut self) {
        let Some(Editing::Repeat(edit)) = self.editing.take() else {
            return;
        };
        if edit.had_rule && self.selected_task.as_deref() == Some(edit.task_id.as_str()) {
            self.edit_selected(|snapshot| snapshot.metadata.recurrence_rule = None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(rule: Option<&str>) -> RepeatEdit {
        RepeatEdit::new("t".into(), rule)
    }

    #[test]
    fn it_opens_on_the_tasks_rule_and_writes_what_the_core_reads() {
        for rule in [
            "daily",
            "every 3 days",
            "weekly",
            "every 2 weeks",
            "weekdays",
            "every friday",
        ] {
            let opened = edit(Some(rule));
            assert_eq!(opened.rule(), rule);
            assert_eq!(periodic_cadence(opened.rule()), Some(opened.cadence()));
        }
        assert_eq!(edit(None).rule(), "daily");
        assert!(!edit(None).had_rule);
    }

    #[test]
    fn arrows_change_the_count_and_step_through_the_units() {
        let mut e = edit(None);
        assert_eq!(e.field, RepeatField::Count);
        e.key("up");
        e.key("up");
        assert_eq!(e.rule(), "every 3 days");
        e.key("down");
        e.key("down");
        e.key("down");
        assert_eq!(e.count, 1, "never below one");
        e.key("right");
        e.key("down");
        assert_eq!(e.unit, RepeatUnit::Weeks);
        e.key("down");
        assert_eq!(e.rule(), "weekdays");
        assert_eq!(e.field, RepeatField::Unit);
        e.key("left");
        assert_eq!(
            e.field,
            RepeatField::Unit,
            "a weekday has no count to move to"
        );
        e.key("down");
        assert_eq!(e.rule(), "every monday");
        e.key("up");
        e.key("up");
        e.key("up");
        assert_eq!(e.unit, RepeatUnit::Days, "↑ goes back up the list");
        e.key("up");
        assert_eq!(e.unit, RepeatUnit::Weekday(1), "and wraps to Sunday");
    }

    #[test]
    fn digits_type_the_count_and_letters_pick_the_unit() {
        let mut e = edit(Some("every friday"));
        e.key("1");
        e.key("4");
        assert_eq!(e.rule(), "every 14 days");
        e.key("w");
        assert_eq!(e.rule(), "every 14 weeks");
        e.key("right");
        e.key("3");
        assert_eq!(
            e.rule(),
            "every 3 weeks",
            "a new run of digits starts again"
        );
        e.key("9");
        e.key("9");
        assert_eq!(e.count, 9, "past the limit it starts again");
        assert!(!e.key("enter"), "Return is the App's");
    }

    #[test]
    fn upcoming_steps_on_from_the_due_date() {
        let e = edit(Some("every 3 days"));
        let dates = e.upcoming(None, "Europe/London", 3);
        assert_eq!(dates.len(), 3);
        assert!(dates.windows(2).all(|w| w[1] - w[0] >= 2 * 86_400_000));
        assert!(dates[0] > now_ms());
    }
}
