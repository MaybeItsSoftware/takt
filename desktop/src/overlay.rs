//! The overlays drawn over the panes, Zed's way: one at a time, at the top,
//! a query field over a ranked list. The command palette, the list
//! navigator (the file finder), search and the keyboard reference.
//!
//! Ranking is the core's (`palette`), as on every other client.

use crate::keys::Bindings;
use crate::state::sidebar::RowKind;
use crate::state::{App, Overlay, Region};
use crate::text_field::TextField;
use takt_core::palette::{PaletteCandidate, palette_rank, palette_rank_commands};

/// What picking a row does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    Command(String),
    /// A command typed in full: `due fri`, `tag errand`.
    Typed(String),
    Place(RowKind),
    Task {
        id: String,
        list_id: String,
    },
    /// A picker's row.
    Choice(crate::state::pickers::Picker, crate::state::pickers::Choice),
    Setting(String),
    Nothing,
}

#[derive(Debug, Clone)]
pub struct Item {
    pub title: String,
    pub detail: String,
    pub keys: String,
    pub pick: Pick,
}

#[derive(Default)]
pub struct OverlayState {
    pub field: TextField,
    pub items: Vec<Item>,
    pub index: usize,
}

const SEARCH_LIMIT: i64 = 60;

impl OverlayState {
    pub fn refresh(&mut self, app: &App) {
        let query = self.field.text.clone();
        self.items = match app.overlay {
            Some(Overlay::CommandPalette) => commands(app, &query),
            Some(Overlay::ListNavigator) => places(app, &query),
            Some(Overlay::Search) => search(app, &query),
            Some(Overlay::KeyboardReference) => reference(app, &query),
            Some(Overlay::Settings) => settings(app, &query),
            Some(Overlay::Diagnostics) => app
                .diagnostics_rows()
                .into_iter()
                .filter(|(title, detail)| {
                    let q = query.to_lowercase();
                    q.is_empty()
                        || title.to_lowercase().contains(&q)
                        || detail.to_lowercase().contains(&q)
                })
                .map(|(title, detail)| Item {
                    title,
                    detail,
                    keys: String::new(),
                    pick: Pick::Nothing,
                })
                .collect(),
            Some(Overlay::Picker(ref picker)) => app
                .picker_rows(picker, &query)
                .into_iter()
                .map(|row| Item {
                    title: row.title,
                    detail: row.detail,
                    keys: row.keys,
                    pick: Pick::Choice(picker.clone(), row.choice),
                })
                .collect(),
            None => Vec::new(),
        };
        self.index = self.index.min(self.items.len().saturating_sub(1));
    }

    pub fn reset(&mut self) {
        *self = OverlayState::default();
    }

    pub fn step(&mut self, by: i64) {
        let count = self.items.len();
        if count > 0 {
            self.index = (self.index as i64 + by).rem_euclid(count as i64) as usize;
        }
    }

    pub fn picked(&self) -> Option<Pick> {
        self.items.get(self.index).map(|item| item.pick.clone())
    }
}

/// Runs a pick on the app, closing the overlay first so the command lands
/// on the panes.
pub fn apply(app: &mut App, pick: Pick) {
    // A picker you tick through stays up for the next tick.
    if !matches!(&pick, Pick::Choice(picker, _) if picker.stays_open()) {
        app.close_overlay();
    }
    match pick {
        Pick::Typed(input) => app.apply_typed(&input),
        Pick::Command(id) => {
            app.prefs.recent_commands.retain(|other| *other != id);
            app.prefs.recent_commands.insert(0, id.clone());
            app.prefs.recent_commands.truncate(20);
            app.prefs.save();
            app.run(&id, None);
        }
        Pick::Place(kind) => {
            let row = app
                .sidebar_rows
                .iter()
                .find(|row| row.kind == kind)
                .cloned();
            match (row, kind) {
                (Some(row), _) => app.apply_sidebar_cursor(&row),
                (None, RowKind::List(id)) => app.select_list(&id),
                (None, RowKind::Folder(id)) => app.select_folder(&id),
                (None, RowKind::NestedList(id)) => app.select_nested_list(&id),
                (None, RowKind::FilterList(id)) => app.select_filter_list(&id),
                (None, RowKind::Everything) => app.select_everything(),
                (None, RowKind::Today) => app.select_today(),
            }
            app.sidebar_cursor = None;
            app.enter_tasks_from_sidebar();
        }
        Pick::Task { id, list_id } => {
            if app.view_mode == crate::state::ViewMode::Today {
                app.select_view_mode(crate::state::ViewMode::Outline);
            }
            app.select_list(&list_id);
            if app.view_mode != crate::state::ViewMode::Outline {
                app.select_view_mode(crate::state::ViewMode::Outline);
            }
            app.selected_task = Some(id);
            app.region = Region::Tasks;
        }
        Pick::Choice(picker, choice) => {
            let back_to_settings = matches!(
                picker,
                crate::state::pickers::Picker::Theme
                    | crate::state::pickers::Picker::Appearance
                    | crate::state::pickers::Picker::CaptureList
            );
            app.apply_choice(picker, choice);
            if back_to_settings && app.overlay.is_none() {
                app.open_overlay(Overlay::Settings);
            }
        }
        Pick::Setting(id) => app.change_setting(&id),
        Pick::Nothing => {}
    }
}

fn commands(app: &App, query: &str) -> Vec<Item> {
    let runnable: Vec<_> = app
        .bindings
        .commands
        .iter()
        .filter(|entry| app.palette_can_run(&entry.id))
        .cloned()
        .collect();
    let ranked = palette_rank_commands(query.into(), runnable, app.prefs.recent_commands.clone());
    let typed = crate::state::typed::describe(query).map(|label| Item {
        title: label,
        detail: if app.selected().is_some() {
            "on the selection".into()
        } else {
            String::new()
        },
        keys: "↩".into(),
        pick: Pick::Typed(query.to_string()),
    });
    typed
        .into_iter()
        .chain(
            ranked
                .into_iter()
                .filter_map(|id| app.bindings.entry(&id))
                .map(|entry| Item {
                    title: entry.title.clone(),
                    detail: entry.group.clone(),
                    keys: entry
                        .keys
                        .first()
                        .map(|key| Bindings::glyphs(key))
                        .unwrap_or_default(),
                    pick: Pick::Command(entry.id.clone()),
                }),
        )
        .collect()
}

/// Every place the sidebar can go: lists, nested lists, folders, filter
/// lists, Everything and Today; most recently visited first.
fn places(app: &App, query: &str) -> Vec<Item> {
    let mut items = vec![
        Item {
            title: "Today".into(),
            detail: String::new(),
            keys: String::new(),
            pick: Pick::Place(RowKind::Today),
        },
        Item {
            title: "Everything".into(),
            detail: String::new(),
            keys: String::new(),
            pick: Pick::Place(RowKind::Everything),
        },
    ];
    for list in &app.lists {
        let folder = list
            .folder_id
            .as_deref()
            .and_then(|id| app.folder(id))
            .map(|folder| folder.name.clone())
            .unwrap_or_default();
        items.push(Item {
            title: list.name.clone(),
            detail: folder,
            keys: String::new(),
            pick: Pick::Place(RowKind::List(list.id.clone())),
        });
    }
    for item in &app.nested_lists {
        let list = app
            .list(&item.task.list_id)
            .map(|list| list.name.clone())
            .unwrap_or_default();
        items.push(Item {
            title: item.task.title.clone(),
            detail: list,
            keys: String::new(),
            pick: Pick::Place(RowKind::NestedList(item.task.id.clone())),
        });
    }
    for folder in &app.folders {
        items.push(Item {
            title: folder.name.clone(),
            detail: "Folder".into(),
            keys: String::new(),
            pick: Pick::Place(RowKind::Folder(folder.id.clone())),
        });
    }
    for filter in &app.filter_lists {
        items.push(Item {
            title: filter.name.clone(),
            detail: "Filter".into(),
            keys: String::new(),
            pick: Pick::Place(RowKind::FilterList(filter.id.clone())),
        });
    }
    let candidates: Vec<PaletteCandidate> = items
        .iter()
        .map(|item| {
            let recent = match &item.pick {
                Pick::Place(RowKind::List(id)) => app
                    .prefs
                    .recent_lists
                    .iter()
                    .position(|other| other == id)
                    .map_or(0, |ix| 100 - ix as i64),
                _ => 0,
            };
            PaletteCandidate {
                title: item.title.clone(),
                detail: item.detail.clone(),
                keys: Vec::new(),
                boost: recent,
            }
        })
        .collect();
    palette_rank(query.into(), candidates)
        .into_iter()
        .filter_map(|ix| items.get(ix as usize).cloned())
        .collect()
}

fn search(app: &App, query: &str) -> Vec<Item> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let hits = app
        .store
        .core
        .search_tasks(
            app.store.workspace_id.clone(),
            query.into(),
            true,
            false,
            SEARCH_LIMIT,
        )
        .unwrap_or_default();
    hits.into_iter()
        .map(|hit| {
            let list = app
                .list(&hit.task.list_id)
                .map(|list| list.name.clone())
                .unwrap_or_default();
            let done = if hit.task.status == "open" {
                ""
            } else {
                " · done"
            };
            Item {
                title: hit.task.title.clone(),
                detail: format!("{list}{done}"),
                keys: String::new(),
                pick: Pick::Task {
                    id: hit.task.id.clone(),
                    list_id: hit.task.list_id.clone(),
                },
            }
        })
        .collect()
}

/// The catalogue, every key it answers to, grouped as it is grouped.
fn reference(app: &App, query: &str) -> Vec<Item> {
    let needle = query.trim().to_lowercase();
    app.bindings
        .commands
        .iter()
        .filter(|entry| !entry.keys.is_empty() || !entry.surface_keys.is_empty())
        .filter(|entry| {
            needle.is_empty()
                || entry.title.to_lowercase().contains(&needle)
                || entry.group.to_lowercase().contains(&needle)
                || entry.keys.iter().any(|key| key.contains(&needle))
        })
        .map(|entry| {
            let keys: Vec<String> = entry
                .keys
                .iter()
                .chain(entry.surface_keys.iter().flat_map(|s| s.keys.iter()))
                .map(|key| Bindings::glyphs(key))
                .collect();
            let ported = crate::state::commands::PORTED.contains(&entry.id.as_str());
            Item {
                title: entry.title.clone(),
                detail: if ported {
                    entry.group.clone()
                } else {
                    format!("{} · not yet", entry.group)
                },
                keys: keys.join("  "),
                pick: if app.palette_can_run(&entry.id) {
                    Pick::Command(entry.id.clone())
                } else {
                    Pick::Nothing
                },
            }
        })
        .collect()
}

fn settings(app: &App, query: &str) -> Vec<Item> {
    let rows = app.settings_rows();
    let candidates = rows
        .iter()
        .map(|row| PaletteCandidate {
            title: row.title.clone(),
            detail: row.group.into(),
            keys: Vec::new(),
            boost: 0,
        })
        .collect();
    let order: Vec<usize> = if query.trim().is_empty() {
        (0..rows.len()).collect()
    } else {
        palette_rank(query.into(), candidates)
            .into_iter()
            .map(|ix| ix as usize)
            .collect()
    };
    order
        .into_iter()
        .filter_map(|ix| rows.get(ix))
        .map(|row| Item {
            title: row.title.clone(),
            detail: row.group.into(),
            keys: row.value.clone(),
            pick: Pick::Setting(row.id.clone()),
        })
        .collect()
}
