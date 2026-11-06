//! The right dock's Conditions tab: saying what is available now (where you
//! are, your free time, what holds, when it expires), how each day starts,
//! and the conditions themselves — added, renamed, filed, moved, merged,
//! archived. `WorkspaceConditionsPane.swift` and its view-model extensions.
//!
//! What holds, what a setting provides, the sections and their order, the
//! day's start: all the core's. The cursor's rows and the steps `←`/`→`
//! take are the Mac's, here (`ConditionsPaneLayout`/`Steps` in TaktCore).

use super::store::now_ms;
use super::{App, Editing, Region};
use std::collections::HashSet;
use takt_core::conditions::{ConditionHolding, ConditionsSaid};
use takt_core::conditions::{ConditionSection, available_condition_sections, condition_sections};
use takt_core::day_start::DayStartSettings;
use takt_core::focus_context::StoredFocusContext;
use takt_core::rows::ConditionRow;

/// A row the cursor can stand on.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Row {
    Location,
    Available(String),
    Time,
    FinishOnly,
    Expiry,
    DayStart,
    DayStartHour,
    DayStartConditions,
    Add,
    Condition(String),
    Folded(String),
    Archived,
}

impl Row {
    pub fn condition_id(&self) -> Option<&str> {
        match self {
            Row::Available(id) | Row::Condition(id) => Some(id),
            _ => None,
        }
    }
}

/// When what you have said stops counting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expiry {
    Never,
    OneHour,
    TwoHours,
    FourHours,
    EndOfDay,
}

const EXPIRIES: [Expiry; 5] = [
    Expiry::Never,
    Expiry::OneHour,
    Expiry::TwoHours,
    Expiry::FourHours,
    Expiry::EndOfDay,
];

impl Expiry {
    pub fn at(self, now: i64) -> Option<i64> {
        let hour = 3600 * 1000;
        match self {
            Expiry::Never => None,
            Expiry::OneHour => Some(now + hour),
            Expiry::TwoHours => Some(now + 2 * hour),
            Expiry::FourHours => Some(now + 4 * hour),
            Expiry::EndOfDay => Some(super::commands::start_of_local_day(1)),
        }
    }

    pub fn nearest(at: Option<i64>, now: i64) -> Expiry {
        let Some(at) = at else {
            return Expiry::Never;
        };
        EXPIRIES[1..]
            .iter()
            .copied()
            .find(|stop| stop.at(now).is_none_or(|ms| ms >= at))
            .unwrap_or(Expiry::EndOfDay)
    }

    fn stepped(self, by: i64) -> Expiry {
        let ix = EXPIRIES.iter().position(|e| *e == self).unwrap_or(0) as i64;
        EXPIRIES[(ix + by).clamp(0, EXPIRIES.len() as i64 - 1) as usize]
    }

    pub fn title(self) -> &'static str {
        match self {
            Expiry::Never => "Never",
            Expiry::OneHour => "In an hour",
            Expiry::TwoHours => "In two hours",
            Expiry::FourHours => "In four hours",
            Expiry::EndOfDay => "At the end of the day",
        }
    }
}

const QUARTER: i64 = 15;
const MAX_MINUTES: i64 = 12 * 60;

/// The minutes left `by` quarter-hour steps on; none is no limit.
pub fn step_minutes(current: Option<i64>, by: i64) -> Option<i64> {
    if by == 0 {
        return current;
    }
    let left = current.unwrap_or(0).max(0);
    let snapped = if by > 0 {
        (left / QUARTER) * QUARTER
    } else {
        (left + QUARTER - 1) / QUARTER * QUARTER
    };
    let next = (snapped + by * QUARTER).clamp(0, MAX_MINUTES);
    (next != 0).then_some(next)
}

/// A time typed on the free-time row: `45`, `1h30`, `90m`, `1.5h`, or a
/// clock time (`17:00`, `5pm`), tomorrow if it has passed.
pub fn typed_time(text: &str, now: i64) -> Option<i64> {
    let entry: String = text
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if entry.is_empty() {
        return None;
    }
    if let Some(minutes) = duration(&entry).filter(|m| *m > 0) {
        return Some(now + minutes.min(MAX_MINUTES) * 60_000);
    }
    let (hour, minute) = clock(&entry)?;
    use chrono::{Local, TimeZone, Timelike};
    let now_local = Local.timestamp_millis_opt(now).single()?;
    let today = now_local
        .with_hour(hour)?
        .with_minute(minute)?
        .with_second(0)?;
    let today = today.with_nanosecond(0)?;
    let at = if today.timestamp_millis() > now {
        today
    } else {
        today + chrono::Duration::days(1)
    };
    Some(at.timestamp_millis())
}

fn duration(entry: &str) -> Option<i64> {
    if let Ok(minutes) = entry.parse::<i64>() {
        return Some(minutes);
    }
    if let Some(minutes) = entry.strip_suffix('m').filter(|e| !e.contains('h')) {
        return minutes.parse().ok();
    }
    let (hours, rest) = entry.split_once('h')?;
    let hours: f64 = hours.parse().ok()?;
    let rest = rest.strip_suffix('m').unwrap_or(rest);
    let minutes: i64 = if rest.is_empty() {
        0
    } else {
        rest.parse().ok()?
    };
    (hours >= 0.0).then(|| (hours * 60.0).round() as i64 + minutes)
}

fn clock(entry: &str) -> Option<(u32, u32)> {
    let (text, meridiem) = match entry {
        e if e.ends_with("am") => (&e[..e.len() - 2], Some("am")),
        e if e.ends_with("pm") => (&e[..e.len() - 2], Some("pm")),
        e => (e, None),
    };
    let pieces: Vec<&str> = text.split(':').collect();
    if !(1..=2).contains(&pieces.len()) {
        return None;
    }
    let hour: u32 = pieces[0].parse().ok()?;
    let minute: u32 = if pieces.len() == 2 {
        pieces[1].parse().ok()?
    } else {
        0
    };
    if minute >= 60 || (pieces.len() == 1 && meridiem.is_none()) {
        return None;
    }
    match meridiem {
        Some("am") => (1..=12).contains(&hour).then_some((hour % 12, minute)),
        Some(_) => (1..=12).contains(&hour).then_some((hour % 12 + 12, minute)),
        None => (hour < 24).then_some((hour, minute)),
    }
}

/// What the tab holds in memory: the conditions, what holds, how each day
/// starts, and the cursor.
#[derive(Debug, Clone, Default)]
pub struct Conditions {
    pub all: Vec<ConditionRow>,
    pub context: StoredFocusContext,
    pub day_start: Option<DayStartSettings>,
    pub required: Vec<String>,
    /// Open tasks needing each condition, by id.
    pub usage: std::collections::HashMap<String, u32>,
    pub cursor: Option<Row>,
    pub shows_archived: bool,
    /// The tasks needing a condition, listed under it (`o`).
    pub tasks_for: Option<(String, Vec<takt_core::conditions::RequiringTask>)>,
    /// The free time being typed.
    pub time_entry: Option<String>,
}

impl App {
    /// Reads the conditions, what holds and the day's start again.
    pub fn reload_conditions(&mut self) {
        let core = self.store.core.clone();
        let ws = self.store.workspace_id.clone();
        let now = now_ms();
        let _ = core.apply_day_start(ws.clone(), now, self.store.zone.clone());
        self.conditions.all = core.conditions(ws.clone()).unwrap_or_default();
        self.conditions.context = core
            .focus_context(ws.clone())
            .ok()
            .flatten()
            .map(|c| takt_core::focus_context::focus_context_in_force(c, now))
            .unwrap_or_default();
        self.conditions.day_start = core.day_start_settings().ok();
        self.conditions.required = core.required_condition_ids(ws.clone()).unwrap_or_default();
        self.conditions.usage = core
            .condition_usage(ws)
            .unwrap_or_default()
            .into_iter()
            .map(|u| (u.condition_id, u.open_tasks))
            .collect();
    }

    pub fn holdings(&self) -> Vec<ConditionHolding> {
        takt_core::conditions::condition_holdings(
            self.conditions.all.clone(),
            self.conditions.context.condition_ids.clone(),
            self.conditions.context.off_ids.clone(),
        )
    }

    pub fn holding(&self, id: &str) -> Option<ConditionHolding> {
        self.holdings().into_iter().find(|h| h.condition_id == id)
    }

    pub fn condition(&self, id: &str) -> Option<&ConditionRow> {
        self.conditions.all.iter().find(|c| c.id == id)
    }

    pub fn places(&self) -> Vec<ConditionRow> {
        self.selectable_conditions()
            .into_iter()
            .filter(|c| c.is_location)
            .collect()
    }

    pub fn current_place(&self) -> Option<ConditionRow> {
        self.places()
            .into_iter()
            .find(|p| self.conditions.context.condition_ids.contains(&p.id))
    }

    /// What "Available now" offers, under its headings.
    pub fn available_sections(&self) -> Vec<ConditionSection> {
        let active = self
            .holdings()
            .into_iter()
            .map(|h| h.condition_id)
            .collect();
        available_condition_sections(
            self.conditions.all.clone(),
            self.conditions.required.clone(),
            active,
        )
    }

    fn selectable_conditions(&self) -> Vec<ConditionRow> {
        self.available_sections()
            .into_iter()
            .flat_map(|s| s.conditions)
            .collect()
    }

    pub fn live_sections(&self) -> Vec<ConditionSection> {
        condition_sections(
            self.conditions
                .all
                .iter()
                .filter(|c| !c.is_archived)
                .cloned()
                .collect(),
        )
    }

    pub fn archived_conditions(&self) -> Vec<ConditionRow> {
        condition_sections(
            self.conditions
                .all
                .iter()
                .filter(|c| c.is_archived)
                .cloned()
                .collect(),
        )
        .into_iter()
        .flat_map(|s| s.conditions)
        .collect()
    }

    pub fn section_id(section: &ConditionSection) -> String {
        section
            .category
            .clone()
            .map(|c| c.to_lowercase())
            .unwrap_or_else(|| "other".into())
    }

    /// Every row the cursor can stand on, top to bottom.
    pub fn condition_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        if !self.places().is_empty() {
            rows.push(Row::Location);
        }
        rows.push(Row::Time);
        if self.conditions.context.available_until_ms.is_some() {
            rows.push(Row::FinishOnly);
        }
        rows.extend(
            self.selectable_conditions()
                .into_iter()
                .filter(|c| !c.is_location)
                .map(|c| Row::Available(c.id)),
        );
        rows.push(Row::Expiry);
        rows.push(Row::DayStart);
        rows.push(Row::DayStartHour);
        if self.conditions.day_start.as_ref().is_some_and(|d| d.resets) {
            rows.push(Row::DayStartConditions);
        }
        rows.push(Row::Add);
        for section in self.live_sections() {
            if section.conditions.is_empty() {
                continue;
            }
            let id = Self::section_id(&section);
            if self.prefs.folded_conditions.contains(&id) {
                rows.push(Row::Folded(id));
            } else {
                rows.extend(
                    section
                        .conditions
                        .iter()
                        .map(|c| Row::Condition(c.id.clone())),
                );
            }
        }
        let archived = self.archived_conditions();
        if !archived.is_empty() {
            rows.push(Row::Archived);
            if self.conditions.shows_archived {
                rows.extend(archived.into_iter().map(|c| Row::Condition(c.id)));
            }
        }
        rows
    }

    /// The row the cursor is drawn on: one whose row has gone lands on the
    /// nearest about the same condition.
    pub fn condition_cursor(&self) -> Row {
        let rows = self.condition_rows();
        let Some(cursor) = self.conditions.cursor.clone() else {
            return rows.first().cloned().unwrap_or(Row::Add);
        };
        if rows.contains(&cursor) {
            return cursor;
        }
        if cursor == Row::FinishOnly {
            return Row::Time;
        }
        if cursor == Row::DayStartConditions {
            return Row::DayStart;
        }
        if let Some(id) = cursor.condition_id() {
            let row = Row::Condition(id.to_string());
            if rows.contains(&row) {
                return row;
            }
            if let Some(section) = self
                .live_sections()
                .into_iter()
                .find(|s| s.conditions.iter().any(|c| c.id == id))
            {
                let folded = Row::Folded(Self::section_id(&section));
                if rows.contains(&folded) {
                    return folded;
                }
            }
        }
        rows.first().cloned().unwrap_or(Row::Add)
    }

    fn condition_under_cursor(&self) -> Option<ConditionRow> {
        let row = self.condition_cursor();
        row.condition_id()
            .and_then(|id| self.condition(id))
            .cloned()
    }

    fn section_under_cursor(&self) -> Option<ConditionSection> {
        let row = self.condition_cursor();
        let sections = self.live_sections();
        match row {
            Row::Folded(id) => sections.into_iter().find(|s| Self::section_id(s) == id),
            Row::Condition(id) | Row::Available(id) => sections
                .into_iter()
                .find(|s| s.conditions.iter().any(|c| c.id == id)),
            _ => None,
        }
    }

    // MARK: Saying what holds

    /// Saves the context, and has Today ranked again.
    fn save_context(&mut self, mut context: StoredFocusContext) {
        context.suggested_ids.clear();
        let ws = self.store.workspace_id.clone();
        let saved = self
            .store
            .core
            .save_focus_context(ws, context.clone(), now_ms());
        if let Err(error) = saved {
            self.message = Some(error.to_string());
        }
        self.reload_conditions();
        self.reload_today();
    }

    pub fn toggle_condition(&mut self, id: &str, on: Option<bool>) {
        let said = ConditionsSaid {
            condition_ids: self.conditions.context.condition_ids.clone(),
            off_ids: self.conditions.context.off_ids.clone(),
        };
        let next = takt_core::conditions::focus_context_switch(
            self.conditions.all.clone(),
            said,
            id.into(),
            on,
        );
        let mut context = self.conditions.context.clone();
        context.condition_ids = next.condition_ids;
        context.off_ids = next.off_ids;
        self.save_context(context);
    }

    /// Says you are at `place`, or nowhere in particular.
    pub fn set_place(&mut self, place: Option<String>) {
        let places: Vec<String> = self.places().into_iter().map(|p| p.id).collect();
        let previous = self.conditions.context.condition_ids.clone();
        let next = takt_core::focus_context::focus_context_changed(
            previous.clone(),
            place.clone().into_iter().collect(),
            if place.is_none() {
                places.clone()
            } else {
                Vec::new()
            },
            places,
        );
        let off = takt_core::conditions::focus_context_kept_overrides(
            self.conditions.all.clone(),
            previous,
            next.clone(),
            self.conditions.context.off_ids.clone(),
        );
        let mut context = self.conditions.context.clone();
        context.condition_ids = next;
        context.off_ids = off;
        self.save_context(context);
    }

    pub fn clear_conditions(&mut self) {
        let mut context = self.conditions.context.clone();
        context.condition_ids.clear();
        context.off_ids.clear();
        self.save_context(context);
    }

    pub fn use_context_again(&mut self) {
        let mut context = self.conditions.context.clone();
        if context.suggested_ids.is_empty() {
            return;
        }
        context.condition_ids = std::mem::take(&mut context.suggested_ids);
        context.off_ids.clear();
        self.save_context(context);
    }

    fn adjust_condition_row(&mut self, by: i64) {
        let now = now_ms();
        match self.condition_cursor() {
            Row::Location => {
                let places: Vec<Option<String>> = std::iter::once(None)
                    .chain(self.places().into_iter().map(|p| Some(p.id)))
                    .collect();
                let current = self.current_place().map(|p| p.id);
                let ix = places.iter().position(|p| *p == current).unwrap_or(0) as i64;
                let next = places[(ix + by).clamp(0, places.len() as i64 - 1) as usize].clone();
                self.set_place(next);
            }
            Row::Time => {
                let left = self
                    .conditions
                    .context
                    .available_until_ms
                    .map(|at| ((at - now).max(0) + 59_999) / 60_000);
                let minutes = step_minutes(left, by);
                let mut context = self.conditions.context.clone();
                context.available_until_ms = minutes.map(|m| now + m * 60_000);
                self.save_context(context);
            }
            Row::FinishOnly => {
                let mode = if by > 0 { "finish" } else { "progress" };
                if self.conditions.context.mode != mode {
                    let mut context = self.conditions.context.clone();
                    context.mode = mode.into();
                    self.save_context(context);
                }
            }
            Row::Expiry => {
                let stop = Expiry::nearest(self.conditions.context.expires_at_ms, now).stepped(by);
                let mut context = self.conditions.context.clone();
                context.expires_at_ms = stop.at(now);
                self.save_context(context);
            }
            Row::Available(id) => {
                let holds = self.holding(&id).is_some_and(|h| h.holds);
                if holds != (by > 0) {
                    self.toggle_condition(&id, Some(by > 0));
                }
            }
            Row::DayStart => self.save_day_start(|d| d.resets = by > 0),
            Row::DayStartHour => {
                self.save_day_start(|d| d.hour = (d.hour + by as i32).rem_euclid(24))
            }
            _ => {}
        }
    }

    fn save_day_start(&mut self, change: impl FnOnce(&mut DayStartSettings)) {
        let Some(mut next) = self.conditions.day_start.clone() else {
            return;
        };
        change(&mut next);
        if Some(&next) == self.conditions.day_start.as_ref() {
            return;
        }
        let zone = self.store.zone.clone();
        if self
            .write(|s| s.core.set_day_start_settings(next, now_ms(), zone))
            .is_some()
        {
            self.reload_conditions();
        }
    }

    pub fn toggle_day_start_condition(&mut self, id: &str) {
        let places: Vec<String> = self.places().into_iter().map(|p| p.id).collect();
        let id = id.to_string();
        self.save_day_start(|settings| {
            if settings.condition_ids.contains(&id) {
                settings.condition_ids.retain(|other| *other != id);
            } else {
                settings.condition_ids = takt_core::focus_context::focus_context_changed(
                    settings.condition_ids.clone(),
                    vec![id],
                    Vec::new(),
                    places,
                );
                settings.resets = true;
            }
        });
    }

    pub fn commit_typed_time(&mut self, text: &str) {
        let trimmed = text.trim();
        let until = if trimmed.is_empty() {
            None
        } else {
            match typed_time(trimmed, now_ms()) {
                Some(at) => Some(at),
                None => {
                    self.message = Some(format!(
                        "\u{201c}{trimmed}\u{201d} is not a time: try 45, 1h30 or 5pm."
                    ));
                    return;
                }
            }
        };
        let mut context = self.conditions.context.clone();
        context.available_until_ms = until;
        self.save_context(context);
    }

    // MARK: The conditions themselves

    pub fn commit_condition_name(&mut self, id: Option<String>, name: &str) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let now = now_ms();
        match id {
            Some(id) => {
                let Some(condition) = self.condition(&id).cloned() else {
                    return;
                };
                self.write(|s| {
                    s.core.save_condition(
                        id,
                        name,
                        condition.is_location,
                        condition.is_archived,
                        now,
                    )
                });
            }
            None => {
                let ws = self.store.workspace_id.clone();
                let category = self
                    .condition_under_cursor()
                    .and_then(|c| takt_core::conditions::effective_category(&c));
                if let Some(id) =
                    self.write(|s| s.core.create_condition_in_category(ws, name, category, now))
                {
                    self.conditions.cursor = Some(Row::Condition(id));
                }
            }
        }
        self.reload_conditions();
    }

    pub fn commit_category_name(&mut self, from: Option<String>, name: &str) {
        if from.as_deref() == Some("Location") {
            self.message =
                Some("Location keeps its name: it is what makes a condition a place.".into());
            return;
        }
        let ws = self.store.workspace_id.clone();
        let to = Some(name.trim().to_string()).filter(|n| !n.is_empty());
        if self
            .write(|s| s.core.rename_condition_category(ws, from, to, now_ms()))
            .is_some()
        {
            self.reload_conditions();
        }
    }

    /// The tab's commands, by catalogue id.
    pub fn run_conditions(&mut self, id: &str, key: &str) {
        let row = self.condition_cursor();
        let now = now_ms();
        match id {
            "motionConditionsSelect" => {
                let rows = self.condition_rows();
                if rows.is_empty() {
                    return;
                }
                let here = rows.iter().position(|r| *r == row).unwrap_or(0) as i64;
                let last = rows.len() as i64 - 1;
                let next = match key {
                    "home" | "cmd+up" => 0,
                    "end" | "cmd+down" => last,
                    "pageup" => (here - 8).max(0),
                    "pagedown" => (here + 8).min(last),
                    "up" | "k" => (here - 1).rem_euclid(last + 1),
                    _ => (here + 1).rem_euclid(last + 1),
                };
                self.conditions.cursor = Some(rows[next as usize].clone());
            }
            "conditionsAdjust" => {
                self.adjust_condition_row(if key == "left" || key == "h" { -1 } else { 1 })
            }
            "conditionsTypeTime" => {
                self.conditions.cursor = Some(Row::Time);
                self.conditions.time_entry = Some(key.to_string());
                self.editing = Some(Editing::ConditionTime);
            }
            "conditionsToggle" => match row {
                Row::FinishOnly => {
                    let by = if self.conditions.context.mode == "finish" {
                        -1
                    } else {
                        1
                    };
                    self.adjust_condition_row(by);
                }
                Row::Location => {
                    let places = self.places();
                    if self.current_place().map(|p| p.id) == places.last().map(|p| p.id.clone()) {
                        self.set_place(None);
                    } else {
                        self.adjust_condition_row(1);
                    }
                }
                Row::DayStart => {
                    let resets = self.conditions.day_start.as_ref().is_some_and(|d| d.resets);
                    self.save_day_start(|d| d.resets = !resets);
                }
                Row::DayStartConditions => self.open_picker(super::pickers::Picker::DayStart),
                Row::Folded(_) | Row::Archived => self.run_conditions("conditionsFold", ""),
                Row::Available(id) | Row::Condition(id)
                    if self.selectable_conditions().iter().any(|c| c.id == id) =>
                {
                    self.toggle_condition(&id, None);
                }
                _ => {}
            },
            "conditionsEdit" => match row {
                Row::Time => {
                    self.conditions.time_entry = Some(String::new());
                    self.editing = Some(Editing::ConditionTime);
                }
                Row::DayStartConditions => self.open_picker(super::pickers::Picker::DayStart),
                Row::Add => self.editing = Some(Editing::ConditionName(None)),
                Row::Available(id) | Row::Condition(id) => {
                    self.conditions.cursor = Some(Row::Condition(id.clone()));
                    self.editing = Some(Editing::ConditionName(Some(id)));
                }
                Row::Folded(_) | Row::Archived => self.run_conditions("conditionsFold", ""),
                _ => {}
            },
            "conditionsNew" => {
                self.conditions.cursor = Some(Row::Add);
                self.editing = Some(Editing::ConditionName(None));
            }
            "conditionsProvides" => match row {
                Row::DayStartConditions => self.open_picker(super::pickers::Picker::DayStart),
                _ => {
                    if let Some(condition) = self.condition_under_cursor() {
                        self.open_picker(super::pickers::Picker::Provides {
                            condition_id: condition.id,
                        });
                    }
                }
            },
            "conditionsCategory" => {
                if let Some(condition) = self.condition_under_cursor() {
                    self.open_picker(super::pickers::Picker::Category {
                        condition_id: condition.id,
                    });
                }
            }
            "conditionsArchive" => {
                if let Some(c) = self.condition_under_cursor() {
                    self.write(|s| {
                        s.core.save_condition(
                            c.id.clone(),
                            c.name.clone(),
                            c.is_location,
                            !c.is_archived,
                            now,
                        )
                    });
                    self.reload_conditions();
                }
            }
            "conditionsMoveUp" | "conditionsMoveDown" => {
                if let Some(c) = self.condition_under_cursor() {
                    let by = if id.ends_with("Up") { -1 } else { 1 };
                    self.conditions.cursor = Some(Row::Condition(c.id.clone()));
                    self.write(|s| s.core.move_condition(c.id.clone(), by, now));
                    self.reload_conditions();
                }
            }
            "conditionsMoveCategoryUp" | "conditionsMoveCategoryDown" => {
                if let Some(section) = self.section_under_cursor() {
                    let by = if id.ends_with("Up") { -1 } else { 1 };
                    let ws = self.store.workspace_id.clone();
                    self.write(|s| {
                        s.core
                            .move_condition_category(ws, section.category, by, now)
                    });
                    self.reload_conditions();
                }
            }
            "conditionsRenameCategory" => {
                if let Some(section) = self.section_under_cursor() {
                    self.editing = Some(Editing::CategoryName(section.category));
                }
            }
            "conditionsMerge" => {
                if let Some(c) = self.condition_under_cursor() {
                    self.open_picker(super::pickers::Picker::Merge { from: c.id });
                }
            }
            "conditionsShowTasks" => {
                if let Some(c) = self.condition_under_cursor() {
                    if self
                        .conditions
                        .tasks_for
                        .as_ref()
                        .is_some_and(|(id, _)| *id == c.id)
                    {
                        self.conditions.tasks_for = None;
                    } else {
                        let ws = self.store.workspace_id.clone();
                        let tasks = self
                            .store
                            .core
                            .tasks_requiring_condition(ws, c.id.clone())
                            .unwrap_or_default();
                        self.conditions.cursor = Some(Row::Condition(c.id.clone()));
                        self.conditions.tasks_for = Some((c.id, tasks));
                    }
                }
            }
            "conditionsFold" => {
                if row == Row::Archived {
                    self.conditions.shows_archived = !self.conditions.shows_archived;
                    return;
                }
                if let Some(section) = self.section_under_cursor() {
                    let sid = Self::section_id(&section);
                    if self.prefs.folded_conditions.remove(&sid) {
                        self.conditions.cursor = section
                            .conditions
                            .first()
                            .map(|c| Row::Condition(c.id.clone()));
                    } else {
                        self.prefs.folded_conditions.insert(sid.clone());
                        self.conditions.cursor = Some(Row::Folded(sid));
                    }
                    self.prefs.save();
                }
            }
            "conditionsFoldAll" => {
                let ids: HashSet<String> =
                    self.live_sections().iter().map(Self::section_id).collect();
                self.prefs.folded_conditions = if ids.is_subset(&self.prefs.folded_conditions) {
                    HashSet::new()
                } else {
                    ids
                };
                self.prefs.save();
            }
            "conditionsClear" => self.clear_conditions(),
            "conditionsUseAgain" => self.use_context_again(),
            "conditionsClose" => self.region = Region::Tasks,
            _ => {}
        }
    }

    /// Opens the tab with the keyboard in it; "Manage conditions…" lands on
    /// the new-condition row.
    pub fn open_conditions_tab(&mut self, on_new: bool) {
        self.reload_conditions();
        self.prefs.right_dock_visible = true;
        self.prefs.right_dock_tab = "conditions".into();
        self.prefs.save();
        if on_new {
            self.conditions.cursor = Some(Row::Add);
        }
        self.region = Region::Conditions;
    }

    /// `cc`: the selected task's conditions; with nothing selected, the tab.
    pub fn edit_task_conditions(&mut self) {
        match self.selected_task.clone() {
            Some(task_id) => {
                self.reload_conditions();
                self.open_picker(super::pickers::Picker::TaskConditions { task_id });
            }
            None => self.open_conditions_tab(false),
        }
    }

    /// Ticks a condition the task needs, or (negated) needs not to hold;
    /// again takes it off.
    pub fn toggle_task_requirement(&mut self, task_id: &str, condition_id: &str, negated: bool) {
        let member = takt_core::conditions::requirement_member(condition_id.into(), negated);
        let previous = self.selected_task.clone();
        self.selected_task = Some(task_id.into());
        self.edit_selected(|edit| {
            let mut planning = edit.planning.clone().unwrap_or_default();
            let mut groups = planning.requirement_groups.clone().unwrap_or_default();
            let before = groups.len();
            groups.retain(|group| {
                !group.iter().any(|m| {
                    takt_core::conditions::requirement_condition_id(m.clone()) == condition_id
                }) || group.len() > 1
            });
            let had = groups.len() != before;
            if !had {
                groups.push(vec![member]);
            }
            planning.requirement_groups = Some(groups);
            edit.planning = planning.normalized();
        });
        self.selected_task = previous;
        self.reload_conditions();
        self.reload_today();
    }

    /// The requirement a task has on a condition: none, needs, needs not.
    pub fn task_requirement(
        &self,
        planning: Option<&takt_core::editor::Planning>,
        id: &str,
    ) -> Option<bool> {
        planning?
            .requirement_groups
            .as_ref()?
            .iter()
            .flatten()
            .find(|m| takt_core::conditions::requirement_condition_id((*m).clone()) == id)
            .map(|m| takt_core::conditions::requirement_is_negated(m.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minutes_step_by_quarters_and_none_sits_below() {
        assert_eq!(step_minutes(None, 1), Some(15));
        assert_eq!(step_minutes(Some(15), -1), None);
        assert_eq!(step_minutes(Some(20), 1), Some(30));
        assert_eq!(step_minutes(Some(20), -1), Some(15));
    }

    #[test]
    fn typed_times_read_as_durations_or_clock_times() {
        let now = 1_000_000_000_000;
        assert_eq!(typed_time("45", now), Some(now + 45 * 60_000));
        assert_eq!(typed_time("1h30", now), Some(now + 90 * 60_000));
        assert_eq!(typed_time("1.5h", now), Some(now + 90 * 60_000));
        assert!(typed_time("5pm", now).is_some());
        assert!(typed_time("17", now).is_some());
        assert!(typed_time("tea", now).is_none());
    }
}

/// How a row of the Conditions tab is drawn.
#[derive(Debug, Clone, Default)]
pub struct RowView {
    /// A heading drawn above the row: the group it starts.
    pub heading: Option<String>,
    pub label: String,
    pub value: String,
    /// A switch's state, drawn as a box.
    pub on: Option<bool>,
    /// Stepped back: archived, or held only by another condition.
    pub dim: bool,
    pub indent: bool,
}

impl App {
    /// Every row of the tab with how it is drawn.
    pub fn condition_views(&self) -> Vec<(Row, RowView)> {
        let holdings = self.holdings();
        let holds = |id: &str| holdings.iter().find(|h| h.condition_id == id);
        let name = |id: &str| {
            self.condition(id)
                .map(|c| c.name.clone())
                .unwrap_or_default()
        };
        let context = &self.conditions.context;
        let day = self.conditions.day_start.clone().unwrap_or_default();
        let live = self.live_sections();
        let mut started: HashSet<String> = HashSet::new();
        let mut first_available = true;
        let mut archived_heading = false;
        self.condition_rows()
            .into_iter()
            .map(|row| {
                let mut view = RowView::default();
                match &row {
                    Row::Location => {
                        view.heading = Some("Now".into());
                        view.label = "Where".into();
                        view.value = self
                            .current_place()
                            .map(|p| p.name)
                            .unwrap_or_else(|| "Anywhere".into());
                    }
                    Row::Time => {
                        if self.places().is_empty() {
                            view.heading = Some("Now".into());
                        }
                        view.label = "Free time".into();
                        view.value = match context.available_until_ms {
                            Some(until) => {
                                let left = (until - now_ms()).max(0) / 60_000;
                                format!(
                                    "until {} · {}",
                                    super::inspector::format_moment(until),
                                    clock_minutes(left)
                                )
                            }
                            None => "Open-ended".into(),
                        };
                    }
                    Row::FinishOnly => {
                        view.label = "Only what fits".into();
                        view.on = Some(context.mode == "finish");
                        view.value = if context.mode == "finish" {
                            "Tasks that finish in the time".into()
                        } else {
                            "Progress on anything".into()
                        };
                    }
                    Row::Available(id) => {
                        if first_available {
                            view.heading = Some("Available".into());
                            first_available = false;
                        }
                        let holding = holds(id);
                        view.on = Some(holding.is_some_and(|h| h.holds));
                        view.label = name(id);
                        if let Some(h) = holding
                            && h.holds
                            && !h.is_set
                            && !h.provided_by.is_empty()
                        {
                            view.value = format!(
                                "via {}",
                                h.provided_by
                                    .iter()
                                    .map(|p| name(p))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            );
                            view.dim = true;
                        }
                    }
                    Row::Expiry => {
                        view.heading = Some("Until".into());
                        view.label = "Expires".into();
                        view.value = Expiry::nearest(context.expires_at_ms, now_ms())
                            .title()
                            .into();
                    }
                    Row::DayStart => {
                        view.heading = Some("Each day".into());
                        view.label = "Start afresh".into();
                        view.on = Some(day.resets);
                        view.value = if day.resets {
                            "Clears what's available".into()
                        } else {
                            "Keeps yesterday's".into()
                        };
                    }
                    Row::DayStartHour => {
                        view.label = "Day starts".into();
                        view.value = format!("{:02}:00", day.hour);
                    }
                    Row::DayStartConditions => {
                        view.label = "Starts with".into();
                        view.value = if day.condition_ids.is_empty() {
                            "Nothing".into()
                        } else {
                            day.condition_ids
                                .iter()
                                .map(|id| name(id))
                                .collect::<Vec<_>>()
                                .join(", ")
                        };
                    }
                    Row::Add => {
                        view.heading = Some("Conditions".into());
                        view.label = "New condition…".into();
                        view.dim = true;
                    }
                    Row::Folded(section) => {
                        let count = live
                            .iter()
                            .find(|s| Self::section_id(s) == *section)
                            .map_or(0, |s| s.conditions.len());
                        view.label = format!(
                            "▸ {}",
                            live.iter()
                                .find(|s| Self::section_id(s) == *section)
                                .map(|s| s.title.clone())
                                .unwrap_or_default()
                        );
                        view.value = count.to_string();
                        view.dim = true;
                    }
                    Row::Archived => {
                        archived_heading = true;
                        view.label = format!(
                            "{} Archived",
                            if self.conditions.shows_archived {
                                "▾"
                            } else {
                                "▸"
                            }
                        );
                        view.value = self.archived_conditions().len().to_string();
                        view.dim = true;
                    }
                    Row::Condition(id) => {
                        let condition = self.condition(id);
                        let archived = condition.is_some_and(|c| c.is_archived);
                        if !archived
                            && let Some(section) = live
                                .iter()
                                .find(|s| s.conditions.iter().any(|c| c.id == *id))
                            && started.insert(Self::section_id(section))
                        {
                            view.heading = Some(format!("▾ {}", section.title));
                        }
                        view.label = name(id);
                        view.indent = true;
                        view.dim = archived || archived_heading;
                        let used = self.conditions.usage.get(id).copied().unwrap_or(0);
                        let provides = condition
                            .map(|c| c.provides.iter().map(|p| name(p)).collect::<Vec<_>>())
                            .unwrap_or_default();
                        let mut parts = Vec::new();
                        if used > 0 {
                            parts.push(format!("{used} task{}", if used == 1 { "" } else { "s" }));
                        }
                        if !provides.is_empty() {
                            parts.push(format!("gives {}", provides.join(", ")));
                        }
                        view.value = parts.join(" · ");
                    }
                }
                (row, view)
            })
            .collect()
    }
}

fn clock_minutes(minutes: i64) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m left"),
        (h, 0) => format!("{h}h left"),
        (h, m) => format!("{h}h {m}m left"),
    }
}
