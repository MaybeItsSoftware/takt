//! The forms that are more than one field, typed as a sheet of `key value`
//! lines in the rules editor: a periodic commitment, a habit, a list's
//! settings. The Swift app drew each as its own sheet of controls
//! (`WorkspaceCommitmentOverlay`, `WorkspaceHabitOverlay`, the list settings
//! page); a line each keeps them on the keyboard, and the line under the
//! sheet says what will be saved before it is.

use super::App;
use super::columns::RulesTarget;
use super::store::now_ms;
use takt_core::commitments::{CommitmentDraft, rule_text, schedule};
use takt_core::habits::HabitDraft;

/// `key value` lines; `#` starts a comment; keys are case-blind.
fn lines(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .filter(|line| !line.is_empty())
        .map(|line| match line.split_once(char::is_whitespace) {
            Some((key, value)) => (key.to_lowercase(), value.trim().to_string()),
            None => (line.to_lowercase(), String::new()),
        })
        .collect()
}

fn yes(value: &str) -> Result<bool, String> {
    match value.to_lowercase().as_str() {
        "yes" | "y" | "true" | "on" => Ok(true),
        "no" | "n" | "false" | "off" | "" => Ok(false),
        other => Err(format!("\u{201c}{other}\u{201d} is not yes or no.")),
    }
}

/// A day typed as `2026-12-01`, `fri`, `in 2 weeks`; the start of it.
fn day(value: &str, zone: &str) -> Result<Option<i64>, String> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("never") {
        return Ok(None);
    }
    super::typed::moment(value, zone)
        .map(|ms| Some(super::review::local_midnight(ms)))
        .ok_or_else(|| format!("Couldn't read \u{201c}{value}\u{201d} as a day."))
}

fn day_text(ms: Option<i64>) -> String {
    use chrono::{Local, TimeZone};
    ms.and_then(|ms| Local.timestamp_millis_opt(ms).single())
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// What a commitment sheet reads as.
pub fn read_commitment(text: &str, zone: &str) -> Result<CommitmentDraft, String> {
    let mut draft = CommitmentDraft {
        title: String::new(),
        rule: "daily".into(),
        expires_at_ms: None,
        stacks_missed: false,
    };
    for (key, value) in lines(text) {
        match key.as_str() {
            "title" | "name" => draft.title = value,
            "repeat" | "every" | "on" => {
                let rule = if key == "every" {
                    format!("every {value}")
                } else {
                    value
                };
                let parsed = schedule(&rule).ok_or_else(|| {
                    format!("Couldn't read \u{201c}{rule}\u{201d} as a schedule.")
                })?;
                draft.rule = rule_text(&parsed);
            }
            "until" | "ends" => draft.expires_at_ms = day(&value, zone)?,
            "stacks" | "stack" => draft.stacks_missed = yes(&value)?,
            other => return Err(format!("A commitment has no \u{201c}{other}\u{201d}.")),
        }
    }
    if draft.title.trim().is_empty() {
        return Err("A commitment needs a title.".into());
    }
    Ok(draft)
}

pub fn read_habit(text: &str, zone: &str) -> Result<HabitDraft, String> {
    let mut draft = HabitDraft {
        title: String::new(),
        weekdays: (1..=7).collect(),
        interval_days: None,
        drops_at_day_end: true,
        estimate_seconds: None,
        expiry_rule: "never".into(),
        expires_at_ms: None,
        placement: "today".into(),
        source_task_id: None,
    };
    for (key, value) in lines(text) {
        match key.as_str() {
            "title" | "name" => draft.title = value,
            "repeat" => {
                let parsed = schedule(&value).ok_or_else(|| {
                    format!("Couldn't read \u{201c}{value}\u{201d} as a schedule.")
                })?;
                draft.weekdays = parsed.weekdays;
                draft.interval_days = parsed.interval_days;
            }
            "drops" => draft.drops_at_day_end = yes(&value)?,
            "estimate" => {
                draft.estimate_seconds = takt_core::editor_draft::editor_parse_minutes(value)
                    .map_err(|e| e.to_string())?
            }
            "expires" => match value.to_lowercase().as_str() {
                "never" | "" => draft.expiry_rule = "never".into(),
                "source" | "with the task" => draft.expiry_rule = "source".into(),
                _ => {
                    draft.expiry_rule = "date".into();
                    draft.expires_at_ms = day(&value, zone)?;
                }
            },
            "column" | "placement" => {
                draft.placement = match value.to_lowercase().as_str() {
                    "today" => "today",
                    "this week" | "this-week" | "week" => "this-week",
                    "waiting" | "waiting on" | "waiting-on" => "waiting-on",
                    other => {
                        return Err(format!(
                            "\u{201c}{other}\u{201d} is not today, this week or waiting."
                        ));
                    }
                }
                .into()
            }
            other => return Err(format!("A habit has no \u{201c}{other}\u{201d}.")),
        }
    }
    if draft.title.trim().is_empty() {
        return Err("A habit needs a title.".into());
    }
    Ok(draft)
}

impl App {
    /// `taskToggleDaily`: commits to the task periodically, or edits the
    /// commitment it has or carries; a task that is a daily itself is turned
    /// back into an ordinary one.
    pub fn begin_commitment(&mut self) {
        let Some(task) = self.selected().cloned() else {
            return;
        };
        if task.item_kind.as_deref() == Some("list") {
            return;
        }
        let core = &self.store.core;
        if let Some(carried) = core.commitment(task.id.clone()).ok().flatten() {
            self.editing = Some(super::Editing::Rules(RulesTarget::Commitment {
                task_id: carried.parent_task_id,
                editing: Some(carried.task_id),
            }));
            return;
        }
        if let Some(first) = core
            .commitments(task.id.clone())
            .ok()
            .and_then(|c| c.into_iter().next())
        {
            self.editing = Some(super::Editing::Rules(RulesTarget::Commitment {
                task_id: task.id,
                editing: Some(first.task_id),
            }));
            return;
        }
        let daily = core.daily_for_task(task.id.clone()).ok().flatten();
        if let Some(daily) = daily.filter(|d| d.source_task_id.is_none()) {
            let _ = daily;
            self.edit_selected(|edit| edit.daily_progress = false);
            self.message = Some("No longer a daily.".into());
            return;
        }
        self.editing = Some(super::Editing::Rules(RulesTarget::Commitment {
            task_id: task.id,
            editing: None,
        }));
    }

    /// `taskHabit`: makes a habit from the selection, edits its habit, or
    /// with nothing selected a new one.
    pub fn begin_habit(&mut self) {
        let task = self
            .selected()
            .filter(|t| t.item_kind.as_deref() != Some("list"))
            .cloned();
        let habit = task.as_ref().and_then(|t| {
            self.store
                .core
                .daily_for_task(t.id.clone())
                .ok()
                .flatten()
                .map(|_| t.id.clone())
        });
        self.editing = Some(super::Editing::Rules(RulesTarget::Habit {
            habit_task_id: habit.clone(),
            source: if habit.is_some() {
                None
            } else {
                task.map(|t| t.id)
            },
        }));
    }

    pub fn form_text(&self, target: &RulesTarget) -> String {
        let core = &self.store.core;
        match target {
            RulesTarget::Commitment { task_id, editing } => {
                let existing = editing
                    .as_ref()
                    .and_then(|id| core.commitment(id.clone()).ok().flatten());
                let title = core
                    .task(task_id.clone())
                    .ok()
                    .flatten()
                    .map(|t| t.title)
                    .unwrap_or_default();
                match existing {
                    Some(c) => format!(
                        "title {}\nrepeat {}\nuntil {}\nstacks {}",
                        c.title,
                        c.rule,
                        day_text(c.expires_at_ms),
                        if c.stacks_missed { "yes" } else { "no" }
                    ),
                    None => format!(
                        "title {}\nrepeat daily\nuntil \nstacks no",
                        takt_core::commitments::default_title(&title)
                    ),
                }
            }
            RulesTarget::Habit {
                habit_task_id,
                source,
            } => {
                let daily = habit_task_id
                    .as_ref()
                    .and_then(|id| core.daily_for_task(id.clone()).ok().flatten());
                let title = habit_task_id
                    .as_ref()
                    .or(source.as_ref())
                    .and_then(|id| core.task(id.clone()).ok().flatten())
                    .map(|t| t.title)
                    .unwrap_or_default();
                match daily {
                    Some(d) => {
                        let weekdays: Vec<u32> = (1..=7u32)
                            .filter(|day| d.active_weekdays_mask & (1 << (day - 1)) != 0)
                            .collect();
                        let repeat = rule_text(&takt_core::commitments::CommitmentSchedule {
                            weekdays,
                            interval_days: d.interval_days,
                        });
                        let expires = match d.expiry_rule.as_str() {
                            "source" => "source".to_string(),
                            "date" => day_text(d.expires_at_ms),
                            _ => "never".to_string(),
                        };
                        format!(
                            "title {title}\nrepeat {repeat}\ndrops {}\nestimate {}\nexpires {expires}\ncolumn {}",
                            if d.drops_at_day_end { "yes" } else { "no" },
                            d.target_seconds
                                .map(|s| (s / 60).to_string())
                                .unwrap_or_default(),
                            d.placement_column.unwrap_or_else(|| "today".into()),
                        )
                    }
                    None => format!(
                        "title {title}\nrepeat daily\ndrops yes\nestimate \nexpires {}\ncolumn today",
                        if source.is_some() { "source" } else { "never" }
                    ),
                }
            }
            RulesTarget::List(id) => {
                let Some(list) = self.list(id) else {
                    return String::new();
                };
                let folder = list
                    .folder_id
                    .as_deref()
                    .and_then(|f| self.folder(f))
                    .map(|f| f.name.clone())
                    .unwrap_or_else(|| "none".into());
                let root = list
                    .visible_root_task_id
                    .as_ref()
                    .and_then(|r| core.task(r.clone()).ok().flatten())
                    .map(|t| t.title)
                    .unwrap_or_else(|| "none".into());
                format!(
                    "name {}\nfolder {folder}\ncolour {}\nroot {root}\narchived {}",
                    list.name,
                    list.color_hex.clone().unwrap_or_default(),
                    if list.is_archived { "yes" } else { "no" }
                )
            }
            RulesTarget::Folder(id) => {
                let Some(folder) = self.folder(id) else {
                    return String::new();
                };
                let parent = folder
                    .parent_folder_id
                    .as_deref()
                    .and_then(|f| self.folder(f))
                    .map(|f| f.name.clone())
                    .unwrap_or_else(|| "none".into());
                format!("name {}\nin {parent}", folder.name)
            }
            _ => String::new(),
        }
    }

    /// A folder named in a sheet: `none` is the top level.
    fn folder_named(&self, name: &str) -> Result<Option<String>, String> {
        if name.is_empty() || name.eq_ignore_ascii_case("none") {
            return Ok(None);
        }
        self.folders
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
            .map(|f| Some(f.id.clone()))
            .ok_or_else(|| format!("No folder called \u{201c}{name}\u{201d}."))
    }

    fn read_list_settings(
        &self,
        id: &str,
        text: &str,
    ) -> Result<takt_core::lists::ListSettings, String> {
        let list = self.list(id).ok_or("That list is gone.")?;
        let mut settings = takt_core::lists::ListSettings {
            name: list.name.clone(),
            colour_hex: list.color_hex.clone(),
            folder_id: list.folder_id.clone(),
            is_archived: list.is_archived,
            visible_root_task_id: list.visible_root_task_id.clone(),
        };
        for (key, value) in lines(text) {
            match key.as_str() {
                "name" => settings.name = value,
                "folder" | "in" => settings.folder_id = self.folder_named(&value)?,
                "colour" | "color" => settings.colour_hex = (!value.is_empty()).then_some(value),
                "archived" => settings.is_archived = yes(&value)?,
                "root" => {
                    settings.visible_root_task_id =
                        if value.is_empty() || value.eq_ignore_ascii_case("none") {
                            None
                        } else {
                            let candidates = self
                                .store
                                .core
                                .visible_root_candidates(id.into())
                                .unwrap_or_default();
                            let current = list
                                .visible_root_task_id
                                .as_ref()
                                .and_then(|r| self.store.core.task(r.clone()).ok().flatten());
                            Some(
                                candidates
                                    .into_iter()
                                    .chain(current)
                                    .find(|t| t.title.eq_ignore_ascii_case(&value))
                                    .map(|t| t.id)
                                    .ok_or_else(|| {
                                        format!("\u{201c}{value}\u{201d} can't be the list's root.")
                                    })?,
                            )
                        }
                }
                other => return Err(format!("A list has no \u{201c}{other}\u{201d}.")),
            }
        }
        Ok(settings)
    }

    fn read_folder_settings(&self, text: &str) -> Result<(String, Option<String>), String> {
        let mut name = String::new();
        let mut parent = None;
        for (key, value) in lines(text) {
            match key.as_str() {
                "name" => name = value,
                "in" | "folder" | "parent" => parent = self.folder_named(&value)?,
                other => return Err(format!("A folder has no \u{201c}{other}\u{201d}.")),
            }
        }
        Ok((name, parent))
    }

    pub fn form_describe(&self, target: &RulesTarget, text: &str) -> String {
        let zone = &self.store.zone;
        match target {
            RulesTarget::Commitment { .. } => match read_commitment(text, zone) {
                Ok(d) => format!(
                    "\u{201c}{}\u{201d} {}, {}{}",
                    d.title,
                    d.rule,
                    if d.expires_at_ms.is_some() {
                        format!("until {}", day_text(d.expires_at_ms))
                    } else {
                        "until the task is done".into()
                    },
                    if d.stacks_missed {
                        ", missed days stack"
                    } else {
                        ""
                    }
                ),
                Err(e) => e,
            },
            RulesTarget::Habit { .. } => match read_habit(text, zone) {
                Ok(d) => {
                    let repeat = rule_text(&takt_core::commitments::CommitmentSchedule {
                        weekdays: d.weekdays.clone(),
                        interval_days: d.interval_days,
                    });
                    format!(
                        "\u{201c}{}\u{201d} {repeat}, in {}{}",
                        d.title,
                        d.placement,
                        if d.drops_at_day_end {
                            ", dropped at day end"
                        } else {
                            ", kept until done"
                        }
                    )
                }
                Err(e) => e,
            },
            RulesTarget::List(id) => match self.read_list_settings(id, text) {
                Ok(settings) => format!(
                    "{}{}{}",
                    settings.name,
                    settings
                        .folder_id
                        .as_deref()
                        .and_then(|f| self.folder(f))
                        .map(|f| format!(" in {}", f.name))
                        .unwrap_or_default(),
                    if settings.is_archived {
                        ", archived"
                    } else {
                        ""
                    }
                ),
                Err(e) => e,
            },
            RulesTarget::Folder(_) => match self.read_folder_settings(text) {
                Ok((name, parent)) => format!(
                    "{name}{}",
                    parent
                        .as_deref()
                        .and_then(|f| self.folder(f))
                        .map(|f| format!(" in {}", f.name))
                        .unwrap_or_default()
                ),
                Err(e) => e,
            },
            _ => String::new(),
        }
    }

    pub fn form_help(target: &RulesTarget) -> Vec<String> {
        match target {
            RulesTarget::Commitment { .. } => vec![
                "repeat  daily · weekdays · every friday · mon, wed · every 2 weeks".into(),
                "until   a day it stops (2026-12-01, in 3 weeks), or blank".into(),
                "stacks  yes: a missed day stays owed until done".into(),
            ],
            RulesTarget::Habit { .. } => vec![
                "repeat   daily · weekdays · every friday · mon, wed · every 3 days".into(),
                "drops    yes: a missed day goes at the end of it".into(),
                "estimate minutes".into(),
                "expires  never · source (with its task) · a day".into(),
                "column   today · this week · waiting".into(),
            ],
            RulesTarget::List(_) => vec![
                "folder   a folder's name, or none".into(),
                "colour   #rrggbb, or blank".into(),
                "root     the one task whose children the list shows, or none".into(),
                "archived yes or no".into(),
            ],
            RulesTarget::Folder(_) => vec!["in  the folder it sits in, or none".into()],
            _ => Vec::new(),
        }
    }

    /// Saves a form sheet; false with the reason in the status bar.
    pub fn commit_form(&mut self, target: RulesTarget, text: &str) -> bool {
        let zone = self.store.zone.clone();
        let now = now_ms();
        match target {
            RulesTarget::Commitment { task_id, editing } => {
                let draft = match read_commitment(text, &zone) {
                    Ok(d) => d,
                    Err(e) => {
                        self.message = Some(e);
                        return false;
                    }
                };
                if self
                    .write(|s| {
                        s.core
                            .save_commitment(task_id.clone(), draft, editing, now, zone)
                    })
                    .is_some()
                {
                    self.message = Some("Committed.".into());
                    self.after_structure_change();
                    return true;
                }
                false
            }
            RulesTarget::Habit {
                habit_task_id,
                source,
            } => {
                let mut draft = match read_habit(text, &zone) {
                    Ok(d) => d,
                    Err(e) => {
                        self.message = Some(e);
                        return false;
                    }
                };
                draft.source_task_id = source.or_else(|| {
                    habit_task_id.as_ref().and_then(|id| {
                        self.store
                            .core
                            .daily_for_task(id.clone())
                            .ok()
                            .flatten()
                            .and_then(|d| d.source_task_id)
                    })
                });
                if draft.expiry_rule == "source" && draft.source_task_id.is_none() {
                    draft.expiry_rule = "never".into();
                }
                if self
                    .write(|s| s.core.save_habit(draft, habit_task_id, now, zone))
                    .is_some()
                {
                    self.message = Some("Habit saved.".into());
                    self.after_structure_change();
                    return true;
                }
                false
            }
            RulesTarget::List(id) => {
                let settings = match self.read_list_settings(&id, text) {
                    Ok(s) => s,
                    Err(e) => {
                        self.message = Some(e);
                        return false;
                    }
                };
                if self
                    .write(|s| s.core.save_list_settings(id.clone(), settings, now))
                    .is_some()
                {
                    self.reload_sidebar();
                    self.reload_rows();
                    return true;
                }
                false
            }
            RulesTarget::Folder(id) => {
                let (name, parent) = match self.read_folder_settings(text) {
                    Ok(s) => s,
                    Err(e) => {
                        self.message = Some(e);
                        return false;
                    }
                };
                if self
                    .write(|s| s.core.save_folder_settings(id.clone(), name, parent, now))
                    .is_some()
                {
                    self.reload_sidebar();
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// `oo`: the settings sheet of the list or folder you are in, or the
    /// sidebar row the cursor is on.
    pub fn begin_list_settings(&mut self) {
        use super::sidebar::RowKind;
        let kind = if self.region == super::Region::Sidebar {
            self.sidebar_cursor_row().map(|row| row.kind.clone())
        } else {
            match &self.scope {
                super::Scope::List { list_id, .. } => Some(RowKind::List(list_id.clone())),
                super::Scope::Folder(id) => Some(RowKind::Folder(id.clone())),
                super::Scope::FilterList(id) => Some(RowKind::FilterList(id.clone())),
                super::Scope::Everything => None,
            }
        };
        self.editing = match kind {
            Some(RowKind::List(id)) => Some(super::Editing::Rules(RulesTarget::List(id))),
            Some(RowKind::Folder(id)) => Some(super::Editing::Rules(RulesTarget::Folder(id))),
            Some(RowKind::FilterList(id)) => {
                Some(super::Editing::Rules(RulesTarget::FilterList(id)))
            }
            _ => {
                self.message = Some("Everything and Today have no settings.".into());
                None
            }
        };
    }

    /// The commitment the inspector stops: what was logged is kept.
    pub fn stop_commitment(&mut self, commitment_task_id: &str) {
        let zone = self.store.zone.clone();
        if self
            .write(|s| {
                s.core
                    .stop_commitment(commitment_task_id.into(), now_ms(), zone)
            })
            .is_some()
        {
            self.after_structure_change();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_commitment_reads_its_lines() {
        let d = read_commitment("title Read\nrepeat mon, wed\nstacks yes", "UTC").unwrap();
        assert_eq!(d.title, "Read");
        assert!(d.stacks_missed);
        assert_ne!(d.rule, "daily");
        assert!(read_commitment("repeat daily", "UTC").is_err());
        assert!(read_commitment("title X\ncolour red", "UTC").is_err());
    }

    #[test]
    fn a_habit_reads_its_lines() {
        let d = read_habit(
            "title Stretch\nrepeat weekdays\ndrops no\nestimate 10\ncolumn this week",
            "UTC",
        )
        .unwrap();
        assert_eq!(d.weekdays, vec![2, 3, 4, 5, 6]);
        assert!(!d.drops_at_day_end);
        assert_eq!(d.estimate_seconds, Some(600));
        assert_eq!(d.placement, "this-week");
    }
}
