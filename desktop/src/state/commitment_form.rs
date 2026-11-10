//! The "Commit periodically" form (`taskToggleDaily`): a title, how often,
//! until when and whether missed days stack, set with the keyboard rather
//! than typed as lines. Its keys are [`App::commitment_key`]'s; the title is
//! the window's text field while the form is open, so it is passed in when
//! the form is saved.
//!
//! The repeat choices are exactly what a commitment's schedule can hold
//! (`takt_core::commitments::CommitmentSchedule`): a set of weekdays, or
//! every so many days.

use super::store::now_ms;
use super::{App, Editing};
use chrono::{Datelike, Duration, NaiveDate};
use takt_core::commitments::{CommitmentDraft, CommitmentSchedule, rule_text, schedule};

/// The rows of the form, in the order Tab and ↓ move through them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitmentField {
    Title,
    Repeat,
    /// The day chips, only while the repeat is "On days".
    Days,
    Until,
    Stacks,
}

/// How often, as the repeat row's chips offer it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatPattern {
    Daily,
    Weekdays,
    /// The days ticked in [`CommitmentForm::days`].
    OnDays,
    EveryDays,
    EveryWeeks,
}

/// The repeat chips in the order ←/→ step through them.
pub const PATTERNS: [RepeatPattern; 5] = [
    RepeatPattern::Daily,
    RepeatPattern::Weekdays,
    RepeatPattern::OnDays,
    RepeatPattern::EveryDays,
    RepeatPattern::EveryWeeks,
];

/// Monday first, as a week is read; index 0 is Monday.
pub const DAY_SHORT: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

const MAX_COUNT: u32 = 365;

impl RepeatPattern {
    pub fn counts(self) -> bool {
        matches!(self, RepeatPattern::EveryDays | RepeatPattern::EveryWeeks)
    }

    /// The most the count can be: a schedule holds at most a year.
    fn max_count(self) -> u32 {
        if self == RepeatPattern::EveryWeeks {
            MAX_COUNT / 7
        } else {
            MAX_COUNT
        }
    }
}

/// Monday-first index to the core's 1 = Sunday numbering.
fn from_sunday(monday_index: usize) -> u32 {
    (monday_index as u32 + 1) % 7 + 1
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitmentForm {
    /// The task it advances.
    pub task_id: String,
    /// The commitment's subtask when editing one.
    pub editing: Option<String>,
    /// The title it opened with; the field holds it while it is edited.
    pub title: String,
    pub field: CommitmentField,
    pub pattern: RepeatPattern,
    /// Monday first.
    pub days: [bool; 7],
    /// The day the keyboard is on in the day chips, Monday first.
    pub day_cursor: usize,
    pub count: u32,
    /// The last day it appears; `None` runs until the task is done.
    pub until: Option<NaiveDate>,
    pub stacks: bool,
    /// A digit was just typed into the count, so the next adds to it.
    typing: bool,
}

impl CommitmentForm {
    /// A new commitment: daily, no end, missed days vanish.
    pub fn new(task_id: String, title: String) -> Self {
        CommitmentForm {
            task_id,
            editing: None,
            title,
            field: CommitmentField::Title,
            pattern: RepeatPattern::Daily,
            days: [true, false, false, false, false, false, false],
            day_cursor: 0,
            count: 2,
            until: None,
            stacks: false,
            typing: false,
        }
    }

    /// The form opened on a saved commitment.
    pub fn editing(
        task_id: String,
        commitment_task_id: String,
        title: String,
        rule: &str,
        until: Option<NaiveDate>,
        stacks: bool,
    ) -> Self {
        let mut form = CommitmentForm::new(task_id, title);
        form.editing = Some(commitment_task_id);
        form.until = until;
        form.stacks = stacks;
        if let Some(parsed) = schedule(rule) {
            form.set_schedule(&parsed);
        }
        form
    }

    fn set_schedule(&mut self, schedule: &CommitmentSchedule) {
        if let Some(days) = schedule.interval_days.filter(|d| *d > 1) {
            let days = days.clamp(1, i64::from(MAX_COUNT)) as u32;
            if days.is_multiple_of(7) {
                self.pattern = RepeatPattern::EveryWeeks;
                self.count = days / 7;
            } else {
                self.pattern = RepeatPattern::EveryDays;
                self.count = days;
            }
            return;
        }
        let mut weekdays: Vec<u32> = schedule
            .weekdays
            .iter()
            .copied()
            .filter(|d| (1..=7).contains(d))
            .collect();
        weekdays.sort_unstable();
        weekdays.dedup();
        match weekdays.as_slice() {
            [] | [1, 2, 3, 4, 5, 6, 7] => self.pattern = RepeatPattern::Daily,
            [2, 3, 4, 5, 6] => self.pattern = RepeatPattern::Weekdays,
            chosen => {
                self.pattern = RepeatPattern::OnDays;
                self.days = std::array::from_fn(|ix| chosen.contains(&from_sunday(ix)));
                self.day_cursor = self.days.iter().position(|on| *on).unwrap_or(0);
            }
        }
    }

    /// The schedule the chips make.
    pub fn schedule(&self) -> CommitmentSchedule {
        let all: Vec<u32> = (1..=7).collect();
        let count = i64::from(self.count.max(1));
        match self.pattern {
            RepeatPattern::Daily => CommitmentSchedule {
                weekdays: all,
                interval_days: None,
            },
            RepeatPattern::Weekdays => CommitmentSchedule {
                weekdays: vec![2, 3, 4, 5, 6],
                interval_days: None,
            },
            RepeatPattern::OnDays => {
                let mut weekdays: Vec<u32> = (0..7)
                    .filter(|ix| self.days[*ix])
                    .map(from_sunday)
                    .collect();
                weekdays.sort_unstable();
                CommitmentSchedule {
                    weekdays,
                    interval_days: None,
                }
            }
            RepeatPattern::EveryDays => CommitmentSchedule {
                weekdays: all,
                interval_days: (count > 1).then_some(count),
            },
            RepeatPattern::EveryWeeks => CommitmentSchedule {
                weekdays: all,
                interval_days: Some(count * 7),
            },
        }
    }

    /// What is stored: the spelling `schedule` reads back.
    pub fn rule(&self) -> String {
        rule_text(&self.schedule())
    }

    /// The chip's words, with the count where it has one.
    pub fn pattern_label(&self, pattern: RepeatPattern) -> String {
        let count = self.count.max(1);
        let plural = if count == 1 { "" } else { "s" };
        match pattern {
            RepeatPattern::Daily => "Daily".into(),
            RepeatPattern::Weekdays => "Weekdays".into(),
            RepeatPattern::OnDays => "On days".into(),
            RepeatPattern::EveryDays => format!("Every {count} day{plural}"),
            RepeatPattern::EveryWeeks => format!("Every {count} week{plural}"),
        }
    }

    /// The rows there are: the day chips only while choosing days.
    pub fn fields(&self) -> Vec<CommitmentField> {
        let mut fields = vec![CommitmentField::Title, CommitmentField::Repeat];
        if self.pattern == RepeatPattern::OnDays {
            fields.push(CommitmentField::Days);
        }
        fields.extend([CommitmentField::Until, CommitmentField::Stacks]);
        fields
    }

    /// Tab and ↓ move down the rows, ⇧Tab and ↑ up; neither wraps.
    pub fn move_field(&mut self, by: i32) {
        let fields = self.fields();
        let at = fields.iter().position(|f| *f == self.field).unwrap_or(0) as i32;
        let next = (at + by).clamp(0, fields.len() as i32 - 1) as usize;
        self.field = fields[next];
        self.typing = false;
    }

    /// A repeat chosen outright, by a click or the arrows.
    pub fn choose(&mut self, pattern: RepeatPattern) {
        self.pattern = pattern;
        self.typing = false;
        self.count = self.count.min(pattern.max_count());
        if pattern == RepeatPattern::OnDays && !self.days.iter().any(|on| *on) {
            self.days[0] = true;
        }
        if pattern == RepeatPattern::EveryDays && self.count < 2 {
            self.count = 2;
        }
    }

    /// Ticks a day on or off; the last one ticked stays.
    pub fn toggle_day(&mut self, ix: usize) {
        let ix = ix.min(6);
        self.day_cursor = ix;
        if self.days[ix] && self.days.iter().filter(|on| **on).count() == 1 {
            return;
        }
        self.days[ix] = !self.days[ix];
    }

    /// Moves the end date by `days`, starting from today when it has none;
    /// never before today.
    pub fn step_until(&mut self, days: i64, today: NaiveDate) {
        let from = self.until.unwrap_or(today);
        self.until = Some((from + Duration::days(days)).max(today));
    }

    /// Digits build the count; on a choice with no count they mean days.
    fn type_digit(&mut self, digit: u32) {
        if !self.pattern.counts() {
            self.pattern = RepeatPattern::EveryDays;
            self.typing = false;
        }
        let grown = self.count * 10 + digit;
        self.count = if self.typing && grown <= self.pattern.max_count() {
            grown
        } else {
            digit
        };
        self.typing = true;
    }

    fn step_count(&mut self, by: i32) {
        if self.pattern.counts() {
            let max = self.pattern.max_count() as i32;
            self.count = (self.count as i32 + by).clamp(1, max) as u32;
        }
        self.typing = false;
    }

    /// A key that is the form's own: true when it was. Return, Esc and
    /// what the title field types are the caller's.
    pub fn key(&mut self, key: &str, today: NaiveDate) -> bool {
        match key {
            "tab" | "down" | "j" => {
                self.move_field(1);
                return true;
            }
            "shift+tab" | "up" | "k" => {
                self.move_field(-1);
                return true;
            }
            _ => {}
        }
        match self.field {
            CommitmentField::Title => return false,
            CommitmentField::Repeat => {
                if let Some(digit) = key
                    .chars()
                    .next()
                    .filter(|_| key.len() == 1)
                    .and_then(|c| c.to_digit(10))
                {
                    self.type_digit(digit);
                    return true;
                }
                let at = PATTERNS
                    .iter()
                    .position(|p| *p == self.pattern)
                    .unwrap_or(0);
                match key {
                    "left" | "h" => self.choose(PATTERNS[at.saturating_sub(1)]),
                    "right" | "l" => self.choose(PATTERNS[(at + 1).min(PATTERNS.len() - 1)]),
                    "+" | "=" => self.step_count(1),
                    "-" => self.step_count(-1),
                    "d" => self.choose(RepeatPattern::EveryDays),
                    "w" => self.choose(RepeatPattern::EveryWeeks),
                    _ => return false,
                }
            }
            CommitmentField::Days => {
                if let Some(digit) = key
                    .chars()
                    .next()
                    .filter(|_| key.len() == 1)
                    .and_then(|c| c.to_digit(10))
                    .filter(|d| (1..=7).contains(d))
                {
                    self.toggle_day(digit as usize - 1);
                    return true;
                }
                match key {
                    "left" | "h" => self.day_cursor = self.day_cursor.saturating_sub(1),
                    "right" | "l" => self.day_cursor = (self.day_cursor + 1).min(6),
                    "space" | "x" => self.toggle_day(self.day_cursor),
                    _ => return false,
                }
            }
            CommitmentField::Until => match key {
                "left" | "h" => self.step_until(-1, today),
                "right" | "l" => self.step_until(1, today),
                "shift+left" | "shift+h" => self.step_until(-7, today),
                "shift+right" | "shift+l" => self.step_until(7, today),
                "x" | "delete" | "n" => self.until = None,
                _ => return false,
            },
            CommitmentField::Stacks => match key {
                "space" | "x" | "left" | "right" | "h" | "l" => self.stacks = !self.stacks,
                _ => return false,
            },
        }
        true
    }

    /// What is saved, with the title the field holds; a title is required.
    pub fn draft(&self, title: &str) -> Result<CommitmentDraft, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("A commitment needs a title.".into());
        }
        Ok(CommitmentDraft {
            title: title.to_string(),
            rule: self.rule(),
            expires_at_ms: self.until.map(day_start_ms),
            stacks_missed: self.stacks,
        })
    }

    /// The next few days it would appear, from today. An interval counts
    /// from the day it was made, which only a new one knows, so a saved
    /// interval shows none.
    pub fn upcoming(&self, today: NaiveDate, how_many: usize) -> Vec<NaiveDate> {
        let schedule = self.schedule();
        let mut dates = Vec::with_capacity(how_many);
        if let Some(step) = schedule.interval_days {
            if self.editing.is_some() {
                return dates;
            }
            let mut day = today;
            while dates.len() < how_many {
                dates.push(day);
                day += Duration::days(step);
            }
        } else {
            let mut day = today;
            for _ in 0..14 {
                if dates.len() == how_many {
                    break;
                }
                let number = day.weekday().num_days_from_sunday() + 1;
                if schedule.weekdays.contains(&number) {
                    dates.push(day);
                }
                day += Duration::days(1);
            }
        }
        if let Some(until) = self.until {
            dates.retain(|d| *d <= until);
        }
        dates
    }
}

/// The start of a local day, as a commitment's end is stored.
pub fn day_start_ms(day: NaiveDate) -> i64 {
    use chrono::{Local, TimeZone};
    let midnight = day.and_hms_opt(0, 0, 0).unwrap_or_default();
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map_or(0, |t| t.timestamp_millis())
}

/// The local day a stored end falls on.
pub fn day_of(ms: i64) -> Option<NaiveDate> {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|t| t.date_naive())
}

/// What a key in the form came to, for the window to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormKey {
    /// The form took it.
    Handled,
    /// Return: save with the field's title.
    Save,
    Cancel,
    /// Not the form's.
    Ignored,
}

impl App {
    pub fn commitment_form(&mut self) -> Option<&mut CommitmentForm> {
        match self.editing {
            Some(Editing::Commitment) => self.commitment_form.as_mut(),
            _ => None,
        }
    }

    /// Opens the form on `form`.
    pub(super) fn open_commitment_form(&mut self, form: CommitmentForm) {
        self.commitment_form = Some(form);
        self.editing = Some(Editing::Commitment);
    }

    /// A key while the form is open, as `spell` names it.
    pub fn commitment_key(&mut self, key: &str) -> FormKey {
        let today = super::date_edit::today();
        let Some(form) = self.commitment_form() else {
            return FormKey::Ignored;
        };
        if form.key(key, today) {
            return FormKey::Handled;
        }
        match key {
            "enter" | "cmd+enter" => FormKey::Save,
            "escape" => FormKey::Cancel,
            "cmd+delete" => {
                if let Some(id) = form.editing.clone() {
                    self.close_commitment_form();
                    self.stop_commitment(&id);
                    self.message = Some("Commitment stopped.".into());
                    return FormKey::Handled;
                }
                FormKey::Ignored
            }
            _ => FormKey::Ignored,
        }
    }

    pub fn close_commitment_form(&mut self) {
        if matches!(self.editing, Some(Editing::Commitment)) {
            self.editing = None;
        }
        self.commitment_form = None;
    }

    /// Saves the form with `title`; false, with the reason in the status
    /// bar, when it can't be.
    pub fn save_commitment_form(&mut self, title: &str) -> bool {
        let Some(form) = self.commitment_form.clone() else {
            return false;
        };
        let draft = match form.draft(title) {
            Ok(d) => d,
            Err(e) => {
                self.message = Some(e);
                return false;
            }
        };
        let zone = self.store.zone.clone();
        let now = now_ms();
        if self
            .write(|s| {
                s.core
                    .save_commitment(form.task_id.clone(), draft, form.editing, now, zone)
            })
            .is_some()
        {
            self.close_commitment_form();
            self.message = Some("Committed.".into());
            self.after_structure_change();
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn saved(rule: &str) -> CommitmentForm {
        CommitmentForm::editing("t".into(), "c".into(), "Read".into(), rule, None, false)
    }

    #[test]
    fn a_saved_rule_opens_as_its_chips_and_saves_back_unchanged() {
        for rule in [
            "daily",
            "weekdays",
            "every friday",
            "mon, wed",
            "every 3 days",
            "weekly",
            "every 2 weeks",
        ] {
            let form = saved(rule);
            assert_eq!(form.rule(), rule, "{rule}");
            assert_eq!(schedule(&form.rule()), schedule(rule), "{rule}");
        }
        let form = saved("mon, wed");
        assert_eq!(form.pattern, RepeatPattern::OnDays);
        assert_eq!(form.days, [true, false, true, false, false, false, false]);
        assert!(saved("every sunday").days[6]);
        assert_eq!(saved("every 2 weeks").pattern, RepeatPattern::EveryWeeks);
        assert_eq!(saved("every 2 weeks").count, 2);
    }

    #[test]
    fn arrows_move_between_rows_and_change_the_one_in_focus() {
        let today = day(2026, 11, 10);
        let mut f = CommitmentForm::new("t".into(), "Work on it".into());
        assert_eq!(f.field, CommitmentField::Title);
        assert!(!f.key("a", today), "the title's letters are typed");
        assert!(f.key("down", today));
        assert_eq!(f.field, CommitmentField::Repeat);
        f.key("right", today);
        assert_eq!(f.rule(), "weekdays");
        f.key("right", today);
        assert_eq!(f.pattern, RepeatPattern::OnDays);
        assert_eq!(
            f.fields(),
            vec![
                CommitmentField::Title,
                CommitmentField::Repeat,
                CommitmentField::Days,
                CommitmentField::Until,
                CommitmentField::Stacks
            ]
        );
        f.key("tab", today);
        assert_eq!(f.field, CommitmentField::Days);
        f.key("right", today);
        f.key("right", today);
        f.key("space", today);
        assert_eq!(f.rule(), "mon, wed");
        f.key("1", today);
        assert_eq!(f.rule(), "every wednesday");
        f.key("3", today);
        assert_eq!(f.rule(), "every wednesday", "the last day stays ticked");
        f.key("tab", today);
        assert_eq!(f.field, CommitmentField::Until);
        f.key("shift+right", today);
        f.key("right", today);
        assert_eq!(f.until, Some(day(2026, 11, 18)));
        f.key("shift+left", today);
        f.key("shift+left", today);
        assert_eq!(f.until, Some(today), "never before today");
        f.key("x", today);
        assert_eq!(f.until, None);
        f.key("down", today);
        f.key("space", today);
        assert!(f.stacks);
        f.key("down", today);
        assert_eq!(f.field, CommitmentField::Stacks, "it doesn't wrap");
        assert!(!f.key("enter", today), "Return is the caller's");
    }

    #[test]
    fn digits_type_a_count() {
        let today = day(2026, 11, 10);
        let mut f = CommitmentForm::new("t".into(), "X".into());
        f.field = CommitmentField::Repeat;
        f.key("1", today);
        f.key("0", today);
        assert_eq!(f.rule(), "every 10 days");
        f.key("w", today);
        assert_eq!(f.rule(), "every 10 weeks");
        f.key("-", today);
        assert_eq!(f.rule(), "every 9 weeks");
        f.key("left", today);
        f.key("left", today);
        assert_eq!(f.pattern, RepeatPattern::OnDays);
        f.key("4", today);
        assert_eq!(f.rule(), "every 4 days", "a digit means days");
    }

    #[test]
    fn the_draft_needs_a_title_and_carries_the_rest() {
        let mut f = CommitmentForm::new("t".into(), String::new());
        assert!(f.draft("  ").is_err());
        f.pattern = RepeatPattern::Weekdays;
        f.stacks = true;
        f.until = Some(day(2026, 12, 1));
        let d = f.draft(" Read ").unwrap();
        assert_eq!(d.title, "Read");
        assert_eq!(d.rule, "weekdays");
        assert!(d.stacks_missed);
        assert_eq!(day_of(d.expires_at_ms.unwrap()), Some(day(2026, 12, 1)));
    }

    #[test]
    fn upcoming_days_follow_the_chips() {
        let tuesday = day(2026, 11, 10);
        let f = saved("mon, wed");
        assert_eq!(
            f.upcoming(tuesday, 3),
            vec![day(2026, 11, 11), day(2026, 11, 16), day(2026, 11, 18)]
        );
        let mut new = CommitmentForm::new("t".into(), "X".into());
        new.choose(RepeatPattern::EveryDays);
        new.count = 3;
        assert_eq!(new.upcoming(tuesday, 2), vec![tuesday, day(2026, 11, 13)]);
        assert!(saved("every 3 days").upcoming(tuesday, 2).is_empty());
        new.until = Some(tuesday);
        assert_eq!(new.upcoming(tuesday, 4), vec![tuesday]);
    }
}
