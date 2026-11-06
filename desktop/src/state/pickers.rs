//! The pickers: an overlay whose rows are choices about one thing, typed
//! into the way the palette is. Where a task or list goes, what a task
//! waits on, its links, a sidebar row's icon. The Swift app drew each as
//! its own sheet (`WorkspaceOverlayHost`); here they share the palette's
//! query and list, and only what a row means differs.

use super::store::now_ms;
use super::{App, Overlay, Region, Scope};
use takt_core::palette::{PaletteCandidate, palette_rank};
use takt_core::tasks::NewTask;

/// What the picker is choosing for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picker {
    /// Where a task (or a nested list, which is a task) goes.
    MoveTask {
        task_id: String,
    },
    /// Where a sidebar list goes: a folder, the top level, or into a list.
    MoveList {
        list_id: String,
    },
    /// A task for `task_id` to wait on, or a new one typed.
    Blocker {
        task_id: String,
    },
    /// An address to link, or one of the task's links to open or remove.
    Link {
        task_id: String,
    },
    /// `who, when, every n days`.
    Waiting {
        task_id: String,
    },
    /// A sidebar row's icon: a list, a nested list, a folder.
    Icon {
        row_id: String,
    },
    /// Where you are: a place, or anywhere.
    Place,
    /// The conditions a task needs; stays open while you tick.
    TaskConditions {
        task_id: String,
    },
    /// What a condition switches on while it holds; stays open.
    Provides {
        condition_id: String,
    },
    /// The category a condition is filed under, or a new one typed.
    Category {
        condition_id: String,
    },
    /// The condition another is merged into.
    Merge {
        from: String,
    },
    /// The conditions each day starts with; stays open.
    DayStart,
    Theme,
    /// Light, dark, or as the system is.
    Appearance,
    /// The list quick capture lands in.
    CaptureList,
}

impl Picker {
    /// Whether picking a row leaves the picker open for the next.
    pub fn stays_open(&self) -> bool {
        matches!(
            self,
            Picker::TaskConditions { .. } | Picker::Provides { .. } | Picker::DayStart
        )
    }
}

/// One row's meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    TaskToList(String),
    TaskUnder {
        list_id: String,
        parent: String,
    },
    TaskToFolder(Option<String>),
    ListToFolder(Option<String>),
    ListInto(String),
    AddBlocker(String),
    NewBlocker(String),
    RemoveBlocker(String),
    AddLink(String),
    OpenLink(String),
    RemoveLink(String),
    Waiting(String),
    StopWaiting,
    Icon(String),
    Place(Option<String>),
    /// A condition the task needs (or, negated, needs not to hold).
    Requirement {
        condition_id: String,
        negated: bool,
    },
    /// A new condition, typed, that the task then needs.
    NewRequirement(String),
    Provide(String),
    Category(Option<String>),
    MergeInto(String),
    DayStartCondition(String),
    Theme(String),
    Appearance(String),
    CaptureList(Option<String>),
}

/// A row: title, detail, the key hint, and what picking it does.
pub struct Row {
    pub title: String,
    pub detail: String,
    pub keys: String,
    pub choice: Choice,
}

fn row(title: impl Into<String>, detail: impl Into<String>, choice: Choice) -> Row {
    Row {
        title: title.into(),
        detail: detail.into(),
        keys: String::new(),
        choice,
    }
}

/// The emoji and glyphs a sidebar row can wear; anything typed is offered
/// too.
pub const ICONS: &[(&str, &str)] = &[
    ("≡", "List"),
    ("📥", "Inbox"),
    ("💼", "Work"),
    ("🎓", "Study"),
    ("🔨", "Projects"),
    ("🏠", "Home"),
    ("❤️", "Health"),
    ("🏃", "Fitness"),
    ("📖", "Reading"),
    ("🎵", "Music"),
    ("⛵", "Sailing"),
    ("⭐", "Goals"),
    ("👥", "People"),
    ("📅", "Plans"),
    ("💡", "Ideas"),
    ("🌿", "Habits"),
    ("✈️", "Travel"),
    ("🎮", "Fun"),
];

/// Ranks `rows` against `query` with the palette's ranking; an empty query
/// keeps their order.
fn ranked(rows: Vec<Row>, query: &str) -> Vec<Row> {
    if query.trim().is_empty() {
        return rows;
    }
    let candidates = rows
        .iter()
        .map(|row| PaletteCandidate {
            title: row.title.clone(),
            detail: row.detail.clone(),
            keys: Vec::new(),
            boost: 0,
        })
        .collect();
    let order = palette_rank(query.into(), candidates);
    let mut slots: Vec<Option<Row>> = rows.into_iter().map(Some).collect();
    order
        .into_iter()
        .filter_map(|ix| slots.get_mut(ix as usize).and_then(Option::take))
        .collect()
}

impl App {
    pub fn open_picker(&mut self, picker: Picker) {
        self.open_overlay(Overlay::Picker(picker));
    }

    /// The placeholder the picker's field shows.
    pub fn picker_prompt(picker: &Picker) -> &'static str {
        match picker {
            Picker::MoveTask { .. } | Picker::MoveList { .. } => "Move to…",
            Picker::Blocker { .. } => "Wait on a task, or type a new one…",
            Picker::Link { .. } => "Paste an address to link…",
            Picker::Waiting { .. } => "Who, when to follow up, every n days…",
            Picker::Icon { .. } => "An icon, or type an emoji…",
            Picker::Place => "Where are you?",
            Picker::TaskConditions { .. } => "Conditions it needs: ↩ ticks, !name for not…",
            Picker::Provides { .. } => "What it provides while it holds…",
            Picker::Category { .. } => "File under a category, or type a new one…",
            Picker::Merge { .. } => "Merge into…",
            Picker::DayStart => "What each day starts with…",
            Picker::Theme => "Choose a theme…",
            Picker::Appearance => "Light, dark, or the system's…",
            Picker::CaptureList => "Where quick capture lands…",
        }
    }

    pub fn picker_rows(&self, picker: &Picker, query: &str) -> Vec<Row> {
        match picker {
            Picker::MoveTask { task_id } => self.move_task_rows(task_id, query),
            Picker::MoveList { list_id } => self.move_list_rows(list_id, query),
            Picker::Blocker { task_id } => self.blocker_rows(task_id, query),
            Picker::Link { task_id } => self.link_rows(task_id, query),
            Picker::Waiting { task_id } => self.waiting_rows(task_id, query),
            Picker::Place => {
                let current = self.current_place().map(|p| p.id);
                let rows = std::iter::once(row(
                    "Anywhere",
                    if current.is_none() { "now" } else { "" },
                    Choice::Place(None),
                ))
                .chain(self.places().into_iter().map(|place| {
                    let here = current.as_deref() == Some(place.id.as_str());
                    row(
                        place.name,
                        if here { "now" } else { "" },
                        Choice::Place(Some(place.id)),
                    )
                }))
                .collect();
                ranked(rows, query)
            }
            Picker::TaskConditions { task_id } => self.task_condition_rows(task_id, query),
            Picker::Provides { condition_id } => {
                let provides = self
                    .condition(condition_id)
                    .map(|c| c.provides.clone())
                    .unwrap_or_default();
                let rows = self
                    .conditions
                    .all
                    .iter()
                    .filter(|c| c.id != *condition_id && !c.is_archived)
                    .map(|c| {
                        let on = provides.contains(&c.id);
                        row(
                            format!("{} {}", if on { "☑" } else { "☐" }, c.name),
                            takt_core::conditions::effective_category(c).unwrap_or_default(),
                            Choice::Provide(c.id.clone()),
                        )
                    })
                    .collect();
                ranked(rows, query)
            }
            Picker::Category { .. } => {
                let mut rows: Vec<Row> = Vec::new();
                let typed = query.trim();
                let categories =
                    takt_core::conditions::condition_categories(self.conditions.all.clone());
                if !typed.is_empty() && !categories.iter().any(|c| c.eq_ignore_ascii_case(typed)) {
                    rows.push(row(
                        format!("New category \u{201c}{typed}\u{201d}"),
                        "",
                        Choice::Category(Some(typed.into())),
                    ));
                }
                let named = categories
                    .into_iter()
                    .map(|c| row(c.clone(), "", Choice::Category(Some(c))))
                    .chain(std::iter::once(row(
                        "No category",
                        "Other",
                        Choice::Category(None),
                    )))
                    .collect();
                rows.extend(ranked(named, query));
                rows
            }
            Picker::Merge { from } => {
                let rows = self
                    .conditions
                    .all
                    .iter()
                    .filter(|c| c.id != *from)
                    .map(|c| {
                        row(
                            c.name.clone(),
                            takt_core::conditions::effective_category(c).unwrap_or_default(),
                            Choice::MergeInto(c.id.clone()),
                        )
                    })
                    .collect();
                ranked(rows, query)
            }
            Picker::DayStart => {
                let starts = self
                    .conditions
                    .day_start
                    .as_ref()
                    .map(|d| d.condition_ids.clone())
                    .unwrap_or_default();
                let rows = self
                    .conditions
                    .all
                    .iter()
                    .filter(|c| !c.is_archived)
                    .map(|c| {
                        let on = starts.contains(&c.id);
                        row(
                            format!("{} {}", if on { "☑" } else { "☐" }, c.name),
                            takt_core::conditions::effective_category(c).unwrap_or_default(),
                            Choice::DayStartCondition(c.id.clone()),
                        )
                    })
                    .collect();
                ranked(rows, query)
            }
            Picker::Theme => {
                let current = self.prefs.theme.clone();
                let rows = self
                    .library
                    .themes
                    .iter()
                    .map(|theme| {
                        row(
                            theme.name(),
                            if theme.id() == current {
                                "in force"
                            } else {
                                ""
                            },
                            Choice::Theme(theme.id()),
                        )
                    })
                    .collect();
                ranked(rows, query)
            }
            Picker::CaptureList => {
                let rows = std::iter::once(row("Inbox", "", Choice::CaptureList(None)))
                    .chain(
                        self.lists_in_sidebar_order()
                            .into_iter()
                            .map(|id| row(self.list_name(&id), "", Choice::CaptureList(Some(id)))),
                    )
                    .collect();
                ranked(rows, query)
            }
            Picker::Appearance => {
                let rows = ["system", "light", "dark"]
                    .into_iter()
                    .map(|mode| {
                        row(
                            match mode {
                                "light" => "Light",
                                "dark" => "Dark",
                                _ => "As the system is",
                            },
                            if self.prefs.appearance == mode {
                                "in force"
                            } else {
                                ""
                            },
                            Choice::Appearance(mode.into()),
                        )
                    })
                    .collect();
                ranked(rows, query)
            }
            Picker::Icon { .. } => {
                let typed = query.trim();
                let mut rows: Vec<Row> = Vec::new();
                if !typed.is_empty() && typed.chars().count() <= 2 && !typed.is_ascii() {
                    rows.push(row(typed, "Typed", Choice::Icon(typed.into())));
                }
                let icons = ICONS
                    .iter()
                    .map(|(glyph, name)| {
                        row(
                            format!("{glyph}  {name}"),
                            "",
                            Choice::Icon((*glyph).into()),
                        )
                    })
                    .collect();
                rows.extend(ranked(icons, query));
                rows
            }
        }
    }

    fn list_name(&self, id: &str) -> String {
        self.list(id).map(|l| l.name.clone()).unwrap_or_default()
    }

    fn move_task_rows(&self, task_id: &str, query: &str) -> Vec<Row> {
        let task = self
            .task(task_id)
            .cloned()
            .or_else(|| self.store.core.task(task_id.into()).ok().flatten());
        let mut rows = Vec::new();
        for id in self.lists_in_sidebar_order() {
            if task
                .as_ref()
                .is_some_and(|t| t.list_id == id && t.parent_task_id.is_none())
            {
                continue;
            }
            let folder = self
                .list(&id)
                .and_then(|l| l.folder_id.as_deref())
                .and_then(|f| self.folder(f))
                .map(|f| f.name.clone())
                .unwrap_or_default();
            rows.push(row(self.list_name(&id), folder, Choice::TaskToList(id)));
        }
        for nested in &self.nested_lists {
            if nested.task.id == task_id {
                continue;
            }
            rows.push(row(
                nested.task.title.clone(),
                format!("in {}", self.list_name(&nested.task.list_id)),
                Choice::TaskUnder {
                    list_id: nested.task.list_id.clone(),
                    parent: nested.task.id.clone(),
                },
            ));
        }
        // A nested list can also leave its list for the sidebar.
        if task
            .as_ref()
            .is_some_and(|t| t.item_kind.as_deref() == Some("list"))
        {
            rows.push(row(
                "Top level",
                "As its own list",
                Choice::TaskToFolder(None),
            ));
            for folder in &self.folders {
                rows.push(row(
                    folder.name.clone(),
                    "As its own list, in the folder",
                    Choice::TaskToFolder(Some(folder.id.clone())),
                ));
            }
        }
        ranked(rows, query)
    }

    fn move_list_rows(&self, list_id: &str, query: &str) -> Vec<Row> {
        let current = self.list(list_id).and_then(|l| l.folder_id.clone());
        let mut rows = Vec::new();
        if current.is_some() {
            rows.push(row(
                "Top level",
                "Out of its folder",
                Choice::ListToFolder(None),
            ));
        }
        for folder in &self.folders {
            if Some(&folder.id) != current.as_ref() {
                rows.push(row(
                    folder.name.clone(),
                    "Folder",
                    Choice::ListToFolder(Some(folder.id.clone())),
                ));
            }
        }
        for id in self.lists_in_sidebar_order() {
            if id != list_id && self.list(&id).is_some_and(|l| l.system_role.is_none()) {
                rows.push(row(
                    self.list_name(&id),
                    "Nest inside, as a list in it",
                    Choice::ListInto(id),
                ));
            }
        }
        ranked(rows, query)
    }

    fn blocker_rows(&self, task_id: &str, query: &str) -> Vec<Row> {
        let mut rows = Vec::new();
        let typed = query.trim();
        if !typed.is_empty() {
            rows.push(row(
                format!("New task \u{201c}{typed}\u{201d}"),
                "Made just above, to do first",
                Choice::NewBlocker(typed.into()),
            ));
            let hits = self
                .store
                .core
                .search_tasks(
                    self.store.workspace_id.clone(),
                    typed.into(),
                    false,
                    false,
                    40,
                )
                .unwrap_or_default();
            for hit in hits {
                if hit.task.id == task_id {
                    continue;
                }
                rows.push(row(
                    hit.task.title.clone(),
                    self.list_name(&hit.task.list_id),
                    Choice::AddBlocker(hit.task.id.clone()),
                ));
            }
        }
        for blocker in self
            .store
            .core
            .task_blockers(task_id.into())
            .unwrap_or_default()
        {
            let state = if blocker.status == "open" {
                "waits on it"
            } else {
                "done"
            };
            rows.push(row(
                format!("Stop waiting on \u{201c}{}\u{201d}", blocker.title),
                state,
                Choice::RemoveBlocker(blocker.id.clone()),
            ));
        }
        rows
    }

    fn link_rows(&self, task_id: &str, query: &str) -> Vec<Row> {
        let mut rows = Vec::new();
        if let Some(url) = takt_core::links::link_from_text(query.into()) {
            rows.push(row(format!("Link {url}"), "Add", Choice::AddLink(url)));
        } else if let Some(path) = local_path(query) {
            rows.push(row(
                format!("Link {path}"),
                "Add a file",
                Choice::AddLink(path),
            ));
        }
        let links = self
            .store
            .core
            .task_links(task_id.into())
            .unwrap_or_default();
        let existing: Vec<Row> = links
            .iter()
            .flat_map(|link| {
                [
                    row(
                        link.title.clone(),
                        link.url.clone(),
                        Choice::OpenLink(link.url.clone()),
                    ),
                    row(
                        format!("Remove \u{201c}{}\u{201d}", link.title),
                        link.url.clone(),
                        Choice::RemoveLink(link.url.clone()),
                    ),
                ]
            })
            .collect();
        rows.extend(ranked(existing, if rows.is_empty() { query } else { "" }));
        rows
    }

    fn waiting_rows(&self, task_id: &str, query: &str) -> Vec<Row> {
        let mut rows = Vec::new();
        if let Some(reading) = read_waiting(query, &self.store.zone) {
            rows.push(row(
                reading.describe(),
                "Set",
                Choice::Waiting(query.into()),
            ));
        }
        if self.meta(task_id).is_some_and(|m| m.waiting_on.is_some()) {
            rows.push(row(
                "Stop waiting",
                "Take it out of Waiting on",
                Choice::StopWaiting,
            ));
        }
        rows
    }

    fn task_condition_rows(&self, task_id: &str, query: &str) -> Vec<Row> {
        let planning = self
            .store
            .core
            .editor_snapshot(task_id.into())
            .ok()
            .and_then(|s| s.planning);
        let (negated, typed) = match query.trim().strip_prefix('!') {
            Some(rest) => (true, rest.trim()),
            None => (false, query.trim()),
        };
        let mut rows: Vec<Row> = Vec::new();
        let exists = self
            .conditions
            .all
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(typed));
        if !typed.is_empty() && !exists {
            rows.push(row(
                format!("New condition \u{201c}{typed}\u{201d}"),
                "Made, and needed",
                Choice::NewRequirement(typed.into()),
            ));
        }
        let listed = self
            .conditions
            .all
            .iter()
            .filter(|c| !c.is_archived)
            .map(|c| {
                let mark = match self.task_requirement(planning.as_ref(), &c.id) {
                    Some(true) => "☒ not",
                    Some(false) => "☑",
                    None => "☐",
                };
                row(
                    format!("{mark} {}", c.name),
                    takt_core::conditions::effective_category(c).unwrap_or_default(),
                    Choice::Requirement {
                        condition_id: c.id.clone(),
                        negated,
                    },
                )
            })
            .collect();
        rows.extend(ranked(listed, typed));
        rows
    }

    /// Picking a row.
    pub fn apply_choice(&mut self, picker: Picker, choice: Choice) {
        let now = now_ms();
        match (picker, choice) {
            (Picker::Place, Choice::Place(place)) => self.set_place(place),
            (
                Picker::TaskConditions { task_id },
                Choice::Requirement {
                    condition_id,
                    negated,
                },
            ) => self.toggle_task_requirement(&task_id, &condition_id, negated),
            (Picker::TaskConditions { task_id }, Choice::NewRequirement(name)) => {
                let ws = self.store.workspace_id.clone();
                if let Some(id) =
                    self.write(|s| s.core.create_condition_in_category(ws, name, None, now))
                {
                    self.reload_conditions();
                    self.toggle_task_requirement(&task_id, &id, false);
                }
            }
            (Picker::Provides { condition_id }, Choice::Provide(id)) => {
                let mut provides = self
                    .condition(&condition_id)
                    .map(|c| c.provides.clone())
                    .unwrap_or_default();
                if provides.contains(&id) {
                    provides.retain(|other| *other != id);
                } else {
                    provides.push(id);
                }
                self.write(|s| {
                    s.core
                        .set_condition_provides(condition_id.clone(), provides, now)
                });
                self.reload_conditions();
                self.reload_today();
            }
            (Picker::Category { condition_id }, Choice::Category(category)) => {
                self.write(|s| {
                    s.core
                        .set_condition_category(condition_id.clone(), category, now)
                });
                self.reload_conditions();
            }
            (Picker::Merge { from }, Choice::MergeInto(into)) => {
                if self
                    .write(|s| s.core.merge_conditions(from.clone(), into.clone(), now))
                    .is_some()
                {
                    self.conditions.cursor = Some(super::conditions::Row::Condition(into));
                    self.reload_conditions();
                    self.reload_today();
                }
            }
            (Picker::Theme, Choice::Theme(id)) => {
                self.prefs.theme = id;
                self.prefs.save();
            }
            (Picker::CaptureList, Choice::CaptureList(id)) => {
                self.prefs.quick_capture_list = id;
                self.prefs.save();
            }
            (Picker::Appearance, Choice::Appearance(mode)) => {
                self.prefs.appearance = mode;
                self.prefs.save();
            }
            (Picker::DayStart, Choice::DayStartCondition(id)) => {
                self.toggle_day_start_condition(&id)
            }
            (Picker::MoveTask { task_id }, Choice::TaskToList(list_id)) => {
                if self
                    .write(|s| {
                        s.core
                            .move_task(task_id.clone(), list_id.clone(), None, true, now)
                    })
                    .is_some()
                {
                    self.message = Some(format!("Moved to {}.", self.list_name(&list_id)));
                    self.after_structure_change();
                }
            }
            (Picker::MoveTask { task_id }, Choice::TaskUnder { list_id, parent }) => {
                if self
                    .write(|s| {
                        s.core
                            .move_task(task_id.clone(), list_id, Some(parent), false, now)
                    })
                    .is_some()
                {
                    self.after_structure_change();
                }
            }
            (Picker::MoveTask { task_id }, Choice::TaskToFolder(folder)) => {
                if let Some(list_id) =
                    self.write(|s| s.core.move_task_to_folder(task_id.clone(), folder, now))
                {
                    self.after_structure_change();
                    if matches!(&self.scope, Scope::List { entered: Some(e), .. } if *e == task_id)
                    {
                        self.select_list(&list_id);
                    }
                }
            }
            (Picker::MoveList { list_id }, Choice::ListToFolder(folder)) => {
                if self
                    .write(|s| s.core.move_list(list_id.clone(), folder.clone(), now))
                    .is_some()
                {
                    if let Some(folder) = folder {
                        self.prefs.expanded_folders.insert(folder);
                        self.prefs.save();
                    }
                    self.reload_sidebar();
                }
            }
            (Picker::MoveList { list_id }, Choice::ListInto(into)) => {
                if let Some(task_id) =
                    self.write(|s| s.core.nest_list(list_id.clone(), into.clone(), None, now))
                {
                    self.reload_sidebar();
                    self.select_list(&into);
                    self.selected_task = Some(task_id);
                    self.region = Region::Tasks;
                }
            }
            (Picker::Blocker { task_id }, Choice::AddBlocker(blocker)) => {
                if self
                    .write(|s| {
                        s.core
                            .add_task_blocker(task_id.clone(), blocker.clone(), now)
                    })
                    .is_some()
                {
                    self.message = Some("Now waits on it.".into());
                    self.after_structure_change();
                }
            }
            (Picker::Blocker { task_id }, Choice::NewBlocker(title)) => {
                let Some(task) = self.task(&task_id).cloned() else {
                    return;
                };
                let new = NewTask {
                    list_id: task.list_id.clone(),
                    title,
                    parent_task_id: task.parent_task_id.clone(),
                    kind: "task".into(),
                    adjacent_task_id: Some(task.id.clone()),
                    above: true,
                    ..Default::default()
                };
                if self
                    .write(|s| s.core.create_task_blocker(task_id.clone(), new, now))
                    .is_some()
                {
                    self.after_structure_change();
                }
            }
            (Picker::Blocker { task_id }, Choice::RemoveBlocker(blocker)) => {
                if self
                    .write(|s| s.core.remove_task_blocker(task_id.clone(), blocker))
                    .is_some()
                {
                    self.after_structure_change();
                }
            }
            (Picker::Link { task_id }, Choice::AddLink(url)) => {
                if self
                    .write(|s| s.core.add_task_link(task_id.clone(), url, None, None, now))
                    .is_some()
                {
                    self.patch_details(&task_id);
                    self.message = Some("Linked.".into());
                }
            }
            (Picker::Link { .. }, Choice::OpenLink(url)) => open_address(&url),
            (Picker::Link { task_id }, Choice::RemoveLink(url)) => {
                if self
                    .write(|s| s.core.remove_task_link(task_id.clone(), url, now))
                    .is_some()
                {
                    self.patch_details(&task_id);
                }
            }
            (Picker::Waiting { task_id }, Choice::Waiting(raw)) => {
                let Some(reading) = read_waiting(&raw, &self.store.zone) else {
                    return;
                };
                if self
                    .write(|s| {
                        s.core.set_waiting(
                            task_id.clone(),
                            Some(reading.who.clone()),
                            reading.follow_up_ms,
                            reading.every_days,
                            now,
                        )
                    })
                    .is_some()
                {
                    self.message = Some(format!("Waiting on {}.", reading.who));
                    self.after_structure_change();
                }
            }
            (Picker::Waiting { task_id }, Choice::StopWaiting) => {
                if self
                    .write(|s| s.core.set_waiting(task_id.clone(), None, None, None, now))
                    .is_some()
                {
                    self.after_structure_change();
                }
            }
            (Picker::Icon { row_id }, Choice::Icon(glyph)) => {
                if glyph == "≡" {
                    self.prefs.icons.remove(&row_id);
                } else {
                    self.prefs.icons.insert(row_id, glyph);
                }
                self.prefs.save();
            }
            _ => {}
        }
    }

    /// A write that moved tasks about: the scope, the sidebar and the
    /// details are read again.
    pub fn after_structure_change(&mut self) {
        self.reload_sidebar();
        self.reload_details();
        self.reload_rows();
        self.reload_today();
    }

    /// `taskMove`: the selection, or from the sidebar the row the cursor is
    /// on.
    pub fn begin_move(&mut self) {
        if self.region == Region::Sidebar {
            match self.sidebar_cursor_row().map(|row| row.kind.clone()) {
                Some(super::sidebar::RowKind::List(id))
                    if self.list(&id).is_some_and(|l| l.system_role.is_none()) =>
                {
                    self.open_picker(Picker::MoveList { list_id: id })
                }
                Some(super::sidebar::RowKind::NestedList(id)) => {
                    self.open_picker(Picker::MoveTask { task_id: id })
                }
                _ => {}
            }
            return;
        }
        if let Some(id) = self.selected_task.clone() {
            self.open_picker(Picker::MoveTask { task_id: id });
        }
    }

    /// Opens a picker on the selected task, unless it is a list.
    pub fn pick_for_selected_task(&mut self, make: impl FnOnce(String) -> Picker) {
        match self.selected() {
            Some(task) if task.item_kind.as_deref() != Some("list") => {
                let picker = make(task.id.clone());
                self.open_picker(picker);
            }
            Some(_) => self.message = Some("A list can't do that.".into()),
            None => {}
        }
    }
}

/// An absolute or home-relative path to something on disk.
fn local_path(text: &str) -> Option<String> {
    let text = text.trim();
    let expanded = match text.strip_prefix("~/") {
        Some(rest) => format!("{}/{rest}", std::env::var("HOME").ok()?),
        None => text.to_string(),
    };
    (expanded.starts_with('/') && std::path::Path::new(&expanded).exists())
        .then(|| format!("file://{expanded}"))
}

/// Opens an address with whatever the Mac opens it with.
pub fn open_address(url: &str) {
    let _ = std::process::Command::new("/usr/bin/open").arg(url).spawn();
}

/// `Sam, fri 9am, every 3`: who, when to follow up, how often again.
#[derive(Debug, Clone, PartialEq)]
pub struct WaitingReading {
    pub who: String,
    pub follow_up_ms: Option<i64>,
    pub every_days: Option<u32>,
}

impl WaitingReading {
    fn describe(&self) -> String {
        let mut text = format!("Waiting on {}", self.who);
        if let Some(ms) = self.follow_up_ms {
            text.push_str(&format!(
                " · follow up {}",
                super::inspector::format_moment(ms)
            ));
        }
        if let Some(days) = self.every_days {
            text.push_str(&format!(
                " · again every {days} day{}",
                if days == 1 { "" } else { "s" }
            ));
        }
        text
    }
}

pub fn read_waiting(raw: &str, zone: &str) -> Option<WaitingReading> {
    let mut parts = raw.split(',').map(str::trim).filter(|p| !p.is_empty());
    let who = parts.next()?.to_string();
    let mut reading = WaitingReading {
        who,
        follow_up_ms: None,
        every_days: None,
    };
    for part in parts {
        let lower = part.to_lowercase();
        if let Some(rest) = lower.strip_prefix("every") {
            let days = rest
                .trim()
                .trim_end_matches("days")
                .trim_end_matches("day")
                .trim();
            reading.every_days = if days.is_empty() {
                Some(1)
            } else {
                days.parse().ok()
            };
        } else {
            reading.follow_up_ms = super::typed::moment(part, zone);
        }
    }
    Some(reading)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_reads_who_when_and_how_often() {
        let reading = read_waiting("Sam, every 3 days", "Europe/London").unwrap();
        assert_eq!(reading.who, "Sam");
        assert_eq!(reading.every_days, Some(3));
        assert_eq!(reading.follow_up_ms, None);
        let reading = read_waiting("Sam, tomorrow 9am", "Europe/London").unwrap();
        assert!(reading.follow_up_ms.is_some());
        assert!(read_waiting("  ", "UTC").is_none());
    }
}
