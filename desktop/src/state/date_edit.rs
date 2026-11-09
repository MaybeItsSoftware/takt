//! The calendar under the inspector's Scheduled and Due rows: a month of
//! days the arrows walk, and beside it the time typed rather than picked.
//! `Tab` goes from the days to the time and, for Scheduled, on to how long
//! the slot is (the task's estimate, which is also the length of its Google
//! Calendar event); `Shift+Tab` comes back. A Due with no time is a day, all
//! day; a Scheduled with none starts at nine.

use super::inspector::{parse_span, span};
use super::{App, Editing, Region};

/// The part of the calendar the keys are in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateFocus {
    Day,
    Time,
    /// Scheduled only: how long the slot is.
    Length,
}

/// The calendar under the Scheduled or Due row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateEdit {
    pub task_id: String,
    /// `start` (Scheduled) or `due`.
    pub stop: &'static str,
    /// The day the keyboard is on.
    pub day: chrono::NaiveDate,
    pub focus: DateFocus,
    /// The time as typed: `9`, `9:30`, `14:00`, `2pm`; for Scheduled also a
    /// range, `9-10:30`, which says the length too. Empty is no time.
    pub time: String,
    /// Scheduled only: the length as typed, `45`, `45m`, `1h 30m`.
    pub length: String,
    /// The length it opened with, so an untouched one writes nothing.
    pub opened_length: String,
    /// The focused field's text is as it arrived: the next character typed
    /// replaces it, as a selection would.
    pub fresh: bool,
}

/// When Scheduled is given a day and no time.
pub const DEFAULT_START: u32 = 9 * 60;

impl DateEdit {
    pub fn scheduling(&self) -> bool {
        self.stop == "start"
    }

    /// The fields in `Tab` order.
    fn order(&self) -> &'static [DateFocus] {
        if self.scheduling() {
            &[DateFocus::Day, DateFocus::Time, DateFocus::Length]
        } else {
            &[DateFocus::Day, DateFocus::Time]
        }
    }

    pub fn focus_on(&mut self, focus: DateFocus) {
        self.focus = focus;
        self.fresh = true;
    }

    fn tab(&mut self, by: i64) {
        let order = self.order();
        let at = order.iter().position(|f| *f == self.focus).unwrap_or(0) as i64;
        let next = order[(at + by).rem_euclid(order.len() as i64) as usize];
        self.focus_on(next);
    }

    fn text(&mut self) -> Option<&mut String> {
        match self.focus {
            DateFocus::Day => None,
            DateFocus::Time => Some(&mut self.time),
            DateFocus::Length => Some(&mut self.length),
        }
    }

    /// A key in the time or length field. True when the field took it.
    fn field_key(&mut self, key: &str) -> bool {
        let length = self.focus == DateFocus::Length;
        let typed = match key {
            "space" => Some(' '),
            _ => {
                let mut chars = key.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => Some(c),
                    _ => None,
                }
            }
        };
        let allowed = |c: char| {
            c.is_ascii_digit()
                || c == ' '
                || c == '.'
                || if length {
                    matches!(c, 'h' | 'm')
                } else {
                    matches!(c, ':' | 'a' | 'p' | 'm' | '-' | '\u{2013}')
                }
        };
        if let Some(c) = typed.filter(|c| allowed(*c)) {
            let fresh = std::mem::replace(&mut self.fresh, false);
            if let Some(text) = self.text() {
                if fresh {
                    text.clear();
                }
                text.push(c);
            }
            return true;
        }
        match key {
            "delete" | "backspace" => {
                let fresh = std::mem::replace(&mut self.fresh, false);
                if let Some(text) = self.text() {
                    if fresh {
                        text.clear();
                    } else {
                        text.pop();
                    }
                }
                true
            }
            "up" | "k" => {
                self.nudge(15);
                true
            }
            "down" | "j" => {
                self.nudge(-15);
                true
            }
            _ => false,
        }
    }

    /// ↑ / ↓ in a field: a quarter of an hour on or back, from what it
    /// reads (or nine, or half an hour, when it reads nothing yet).
    fn nudge(&mut self, by: i64) {
        self.fresh = true;
        match self.focus {
            DateFocus::Day => {}
            DateFocus::Time => {
                let (start, length) = parse_slot(&self.time).ok().unwrap_or((None, None));
                let from = start.unwrap_or(DEFAULT_START) as i64;
                let at = (from + by).clamp(0, 23 * 60 + 45) as u32;
                self.time = clock(at);
                if let Some(length) = length {
                    // A range keeps its length as it moves.
                    self.length = span(length);
                }
            }
            DateFocus::Length => {
                let from = parse_span(&self.length)
                    .filter(|s| *s > 0)
                    .map_or(30, |s| s / 60);
                self.length = span(((from + by).max(5)) * 60);
            }
        }
    }
}

/// `09:05`.
pub fn clock(minutes: u32) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// A typed time of day in minutes after midnight: `9`, `930`, `9:30`,
/// `9.30`, `14:00`, `2pm`, `2:30 pm`, `12am`. Empty is no time; anything
/// else is an error.
pub fn parse_clock(text: &str) -> Result<Option<u32>, ()> {
    let text: String = text
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if text.is_empty() {
        return Ok(None);
    }
    let (digits, meridiem) =
        if let Some(rest) = text.strip_suffix("am").or_else(|| text.strip_suffix('a')) {
            (rest, Some(false))
        } else if let Some(rest) = text.strip_suffix("pm").or_else(|| text.strip_suffix('p')) {
            (rest, Some(true))
        } else {
            (text.as_str(), None)
        };
    let number = |s: &str| -> Result<u32, ()> {
        if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit()) {
            return Err(());
        }
        s.parse().map_err(|_| ())
    };
    let (hours, minutes) = match digits.split_once([':', '.']) {
        Some((h, m)) if m.len() == 2 => (number(h)?, number(m)?),
        Some(_) => return Err(()),
        None if digits.len() <= 2 => (number(digits)?, 0),
        None if digits.len() <= 4 => {
            let (h, m) = digits.split_at(digits.len() - 2);
            (number(h)?, number(m)?)
        }
        None => return Err(()),
    };
    if minutes > 59 {
        return Err(());
    }
    let hours = match meridiem {
        Some(pm) => {
            if !(1..=12).contains(&hours) {
                return Err(());
            }
            hours % 12 + if pm { 12 } else { 0 }
        }
        None if hours <= 23 => hours,
        None => return Err(()),
    };
    Ok(Some(hours * 60 + minutes))
}

/// A typed time, or a range of one (`9-10:30`, `2pm–3pm`, `14:00 - 15:15`):
/// the start, and the length in seconds when it was a range. An end that
/// would come before its start is read twelve hours on when the start is in
/// the afternoon (`2pm-3` is 14:00 to 15:00) or the slot is then four hours
/// at most (`11-1`); otherwise (`10-9`) it is an error.
pub fn parse_slot(text: &str) -> Result<(Option<u32>, Option<i64>), ()> {
    let Some((from, to)) = text.split_once(['-', '\u{2013}']) else {
        return Ok((parse_clock(text)?, None));
    };
    let start = parse_clock(from)?.ok_or(())?;
    let mut end = parse_clock(to)?.ok_or(())?;
    if end <= start
        && end < 12 * 60
        && end + 12 * 60 > start
        && (start >= 12 * 60 || end + 12 * 60 - start <= 4 * 60)
    {
        end += 12 * 60;
    }
    if end <= start {
        return Err(());
    }
    Ok((Some(start), Some(i64::from(end - start) * 60)))
}

fn local_parts(ms: i64) -> Option<(chrono::NaiveDate, u32)> {
    use chrono::{Local, TimeZone, Timelike};
    let time = Local.timestamp_millis_opt(ms).single()?;
    Some((time.date_naive(), time.hour() * 60 + time.minute()))
}

fn local_ms(day: chrono::NaiveDate, minutes: u32) -> Option<i64> {
    use chrono::{Local, TimeZone};
    let at = day.and_hms_opt(minutes / 60, minutes % 60, 0)?;
    Local
        .from_local_datetime(&at)
        .earliest()
        .map(|time| time.timestamp_millis())
}

pub fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

impl App {
    /// Return on the Scheduled or Due row: the calendar opens on its day,
    /// with the keys on the days.
    pub fn open_date_edit(&mut self, stop: &'static str) {
        let Some(task_id) = self.selected_task.clone() else {
            return;
        };
        let Some(snapshot) = self.inspected() else {
            return;
        };
        let planning = snapshot.planning.clone().unwrap_or_default();
        let (day, time) = match stop {
            "start" => planning
                .start_at_ms
                .and_then(local_parts)
                .map(|(d, t)| (d, Some(t)))
                .unwrap_or((today(), None)),
            _ => snapshot
                .due_at_ms
                .and_then(local_parts)
                .map(|(d, t)| (d, Some(t)))
                .or_else(|| {
                    planning
                        .due_date
                        .as_deref()
                        .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                        .map(|d| (d, None))
                })
                .unwrap_or((today(), None)),
        };
        let length = if stop == "start" {
            snapshot
                .estimate_seconds
                .filter(|s| *s > 0)
                .map(span)
                .unwrap_or_default()
        } else {
            String::new()
        };
        self.region = Region::Inspector;
        self.editing = Some(Editing::Date(DateEdit {
            task_id,
            stop,
            day,
            focus: DateFocus::Day,
            time: time.map(clock).unwrap_or_default(),
            opened_length: length.clone(),
            length,
            fresh: true,
        }));
    }

    pub fn date_edit(&mut self) -> Option<&mut DateEdit> {
        match self.editing.as_mut() {
            Some(Editing::Date(edit)) => Some(edit),
            _ => None,
        }
    }

    /// A key while the calendar is open. Everywhere: `Tab` / `Shift+Tab`
    /// move between the days and the fields, Return sets it, Esc leaves it
    /// as it was. On the days: arrows and hjkl move the day, [ ] and the
    /// page keys the month, t today, Delete clears the date. In a field:
    /// the time or length is typed, ↑ / ↓ move it a quarter of an hour,
    /// Delete takes a character back.
    pub fn date_key(&mut self, key: &str) {
        use chrono::{Duration, Months};
        let Some(edit) = self.date_edit() else {
            return;
        };
        match key {
            "tab" => return edit.tab(1),
            "shift+tab" => return edit.tab(-1),
            "enter" => return self.commit_date(),
            "escape" => {
                self.editing = None;
                return;
            }
            _ => {}
        }
        if edit.focus != DateFocus::Day {
            edit.field_key(key);
            return;
        }
        let day = edit.day;
        let moved = match key {
            "left" | "h" => day.checked_sub_signed(Duration::days(1)),
            "right" | "l" => day.checked_add_signed(Duration::days(1)),
            "up" | "k" => day.checked_sub_signed(Duration::days(7)),
            "down" | "j" => day.checked_add_signed(Duration::days(7)),
            "[" | "pageup" => day.checked_sub_months(Months::new(1)),
            "]" | "pagedown" => day.checked_add_months(Months::new(1)),
            "t" => Some(today()),
            _ => None,
        };
        if let Some(day) = moved {
            edit.day = day;
            return;
        }
        match key {
            "space" => self.commit_date(),
            "backspace" | "delete" => self.clear_date(),
            // A digit on the days starts the time: `9` is typed, not lost.
            _ if key.len() == 1 && key.chars().all(|c| c.is_ascii_digit()) => {
                edit.focus_on(DateFocus::Time);
                edit.field_key(key);
            }
            _ => {}
        }
    }

    /// A day clicked: it is set at once, at the time typed.
    pub fn pick_date_day(&mut self, day: chrono::NaiveDate) {
        if let Some(edit) = self.date_edit() {
            edit.day = day;
        }
        self.commit_date();
    }

    /// The month shown moved by `by`.
    pub fn step_date_month(&mut self, by: i32) {
        use chrono::Months;
        if let Some(edit) = self.date_edit() {
            let months = Months::new(by.unsigned_abs());
            let moved = if by < 0 {
                edit.day.checked_sub_months(months)
            } else {
                edit.day.checked_add_months(months)
            };
            if let Some(day) = moved {
                edit.day = day;
            }
        }
    }

    /// Return: what was chosen and typed is saved as one undo step. A time
    /// or length that does not read keeps the calendar open on it, saying
    /// why.
    pub fn commit_date(&mut self) {
        let Some(Editing::Date(mut edit)) = self.editing.take() else {
            return;
        };
        if self.selected_task.as_deref() != Some(edit.task_id.as_str()) {
            return;
        }
        let slot = if edit.scheduling() {
            parse_slot(&edit.time)
        } else {
            parse_clock(&edit.time).map(|time| (time, None))
        };
        let Ok((time, range)) = slot else {
            self.message = Some(format!(
                "\u{201c}{}\u{201d} isn't a time: try 9, 9:30, 14:00 or 2pm{}.",
                edit.time.trim(),
                if edit.scheduling() {
                    ", or 9-10:30"
                } else {
                    ""
                }
            ));
            edit.focus_on(DateFocus::Time);
            self.editing = Some(Editing::Date(edit));
            return;
        };
        if !edit.scheduling() {
            let day = edit.day.format("%Y-%m-%d").to_string();
            let at = time.and_then(|t| local_ms(edit.day, t));
            return self.edit_selected(|snapshot| {
                let mut planning = snapshot.planning.clone().unwrap_or_default();
                match at {
                    Some(ms) => {
                        snapshot.due_at_ms = Some(ms);
                        planning.due_date = None;
                    }
                    None => {
                        snapshot.due_at_ms = None;
                        planning.due_date = Some(day);
                    }
                }
                snapshot.planning = planning.normalized();
            });
        }
        // How long: a range says it, else the length field when it was
        // changed. An untouched length leaves the estimate alone.
        let length = match range {
            Some(seconds) => Some(Some(seconds)),
            None if edit.length.trim() != edit.opened_length.trim() => {
                match parse_span(&edit.length) {
                    Some(seconds) => Some((seconds > 0).then_some(seconds)),
                    None => {
                        self.message = Some(format!(
                            "\u{201c}{}\u{201d} isn't a length of time: try 45, 45m or 1h 30m.",
                            edit.length.trim()
                        ));
                        edit.focus_on(DateFocus::Length);
                        self.editing = Some(Editing::Date(edit));
                        return;
                    }
                }
            }
            None => None,
        };
        let at = local_ms(edit.day, time.unwrap_or(DEFAULT_START));
        match length {
            None => self.schedule_selected(at),
            Some(estimate) => {
                self.edit_selected(|snapshot| {
                    let mut planning = snapshot.planning.clone().unwrap_or_default();
                    planning.start_at_ms = at;
                    snapshot.planning = planning.normalized();
                    snapshot.estimate_seconds = estimate;
                });
                // A start moves the task between Today and This week, and
                // buries or unburies it.
                self.reload_rows();
            }
        }
    }

    pub fn clear_date(&mut self) {
        let Some(Editing::Date(edit)) = self.editing.take() else {
            return;
        };
        match edit.stop {
            "start" => self.schedule_selected(None),
            _ => self.apply_typed("clear due"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_as_people_type_them() {
        let at = |h: u32, m: u32| Ok(Some(h * 60 + m));
        assert_eq!(parse_clock(""), Ok(None));
        assert_eq!(parse_clock("9"), at(9, 0));
        assert_eq!(parse_clock("09"), at(9, 0));
        assert_eq!(parse_clock("930"), at(9, 30));
        assert_eq!(parse_clock("9:30"), at(9, 30));
        assert_eq!(parse_clock("9.30"), at(9, 30));
        assert_eq!(parse_clock("14:00"), at(14, 0));
        assert_eq!(parse_clock("1400"), at(14, 0));
        assert_eq!(parse_clock("2pm"), at(14, 0));
        assert_eq!(parse_clock("2 PM"), at(14, 0));
        assert_eq!(parse_clock("2:30pm"), at(14, 30));
        assert_eq!(parse_clock("2p"), at(14, 0));
        assert_eq!(parse_clock("12am"), at(0, 0));
        assert_eq!(parse_clock("12pm"), at(12, 0));
        assert_eq!(parse_clock("11a"), at(11, 0));
        for bad in ["24", "9:7", "9:60", "13pm", "0am", "noon", "1:2:3", "12345"] {
            assert_eq!(parse_clock(bad), Err(()), "{bad}");
        }
    }

    #[test]
    fn a_range_says_how_long() {
        assert_eq!(parse_slot("9-10:30"), Ok((Some(540), Some(90 * 60))));
        assert_eq!(parse_slot("2pm\u{2013}3pm"), Ok((Some(840), Some(3600))));
        assert_eq!(parse_slot("2pm-3"), Ok((Some(840), Some(3600))));
        assert_eq!(parse_slot("14:00 - 15:15"), Ok((Some(840), Some(75 * 60))));
        assert_eq!(parse_slot("14:00"), Ok((Some(840), None)));
        assert_eq!(parse_slot(""), Ok((None, None)));
        assert_eq!(parse_slot("11-1"), Ok((Some(660), Some(2 * 3600))));
        assert_eq!(parse_slot("10-9"), Err(()));
        assert_eq!(parse_slot("9-"), Err(()));
    }

    fn edit(stop: &'static str) -> DateEdit {
        DateEdit {
            task_id: "t".into(),
            stop,
            day: chrono::NaiveDate::from_ymd_opt(2026, 11, 9).unwrap(),
            focus: DateFocus::Day,
            time: "14:00".into(),
            length: "30m".into(),
            opened_length: "30m".into(),
            fresh: true,
        }
    }

    #[test]
    fn tab_walks_the_days_the_time_and_for_scheduled_the_length() {
        let mut due = edit("due");
        due.tab(1);
        assert_eq!(due.focus, DateFocus::Time);
        due.tab(1);
        assert_eq!(due.focus, DateFocus::Day);
        due.tab(-1);
        assert_eq!(due.focus, DateFocus::Time);

        let mut start = edit("start");
        start.tab(1);
        start.tab(1);
        assert_eq!(start.focus, DateFocus::Length);
        start.tab(1);
        assert_eq!(start.focus, DateFocus::Day);
        start.tab(-1);
        assert_eq!(start.focus, DateFocus::Length);
    }

    #[test]
    fn typing_replaces_the_time_it_opened_with_then_adds_to_it() {
        let mut e = edit("due");
        e.focus_on(DateFocus::Time);
        for key in ["2", ":", "3", "0", "p", "m"] {
            assert!(e.field_key(key));
        }
        assert_eq!(e.time, "2:30pm");
        assert!(e.field_key("delete"));
        assert_eq!(e.time, "2:30p");
        // Letters that mean nothing in a time are not typed.
        assert!(!e.field_key("x"));
        // A fresh field's Delete empties it: the day, all day.
        e.focus_on(DateFocus::Time);
        e.field_key("delete");
        assert_eq!(e.time, "");
    }

    #[test]
    fn arrows_in_a_field_move_it_a_quarter_hour() {
        let mut e = edit("start");
        e.focus_on(DateFocus::Time);
        e.field_key("up");
        assert_eq!(e.time, "14:15");
        e.time.clear();
        e.field_key("down");
        assert_eq!(e.time, "08:45");
        e.focus_on(DateFocus::Length);
        e.field_key("up");
        assert_eq!(e.length, "45m");
        e.length = "10".into();
        e.field_key("down");
        assert_eq!(e.length, "5m");
    }
}
