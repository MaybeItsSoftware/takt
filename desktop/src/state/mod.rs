//! What the window shows and where the keyboard is, held in memory.
//!
//! The Swift app's `WorkspaceViewModel`, rebuilt around one rule it never
//! kept: a write patches what is held, and only another process's write
//! (`data_version`) reads a scope again. Each concern is its own file:
//! `sidebar` the left dock's rows and cursor, `outline` the task rows,
//! `navigation` where you are, `commands` what each catalogue command does.

pub mod actions;
pub mod agent;
pub mod board;
pub mod capture;
pub mod columns;
pub mod commands;
pub mod conditions;
pub mod details;
pub mod docks;
pub mod focus;
pub mod focus_prompt;
pub mod forms;
pub mod inspector;
pub mod integrations;
pub mod library;
pub mod menubar;
pub mod navigation;
pub mod outline;
pub mod pickers;
pub mod prefs;
pub mod review;
pub mod rules;
pub mod settings;
pub mod sidebar;
pub mod store;
pub mod today;
pub mod typed;

use crate::keys::{Bindings, Sequence};
use outline::OutlineRow;
use prefs::Prefs;
use sidebar::SidebarRow;
use std::collections::HashSet;
use store::Store;
use takt_core::filter_lists::FilterListRow;
use takt_core::keymap::CommandSurface;
use takt_core::records::{FolderRow, ListRow, OutlineItem, TaskRow};

/// Where you are: what the main pane draws its tasks from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Scope {
    /// One list, or a task inside it entered as though it were a list.
    List {
        list_id: String,
        entered: Option<String>,
    },
    /// Every active list.
    Everything,
    /// A folder's lists, its sub-folders' included.
    Folder(String),
    /// A saved filter list's matches.
    FilterList(String),
}

/// How the main pane draws the scope. Today is a mode, not a scope: the day,
/// from every list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Today,
    Board,
    Outline,
    Matrix,
}

impl ViewMode {
    pub fn title(self) -> &'static str {
        match self {
            ViewMode::Today => "Today",
            ViewMode::Board => "Board",
            ViewMode::Outline => "Outline",
            ViewMode::Matrix => "Matrix",
        }
    }
}

/// Which region has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Sidebar,
    Tasks,
    Inspector,
    /// The right dock's Conditions tab.
    Conditions,
    /// The bottom dock's done rail.
    Done,
    /// The bottom dock's timeline.
    Timeline,
    /// The left dock's agent panel.
    Agent,
}

/// Something drawn over the panes that owns the keyboard while it is up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay {
    CommandPalette,
    ListNavigator,
    Search,
    KeyboardReference,
    /// A choice about one thing: where it goes, what it waits on.
    Picker(pickers::Picker),
    /// What the app knows about itself: files, problems, timings.
    Diagnostics,
    Settings,
}

/// A text field in a row, open for typing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Editing {
    /// Renaming a task in place.
    TaskTitle {
        task_id: String,
    },
    /// A new task, drafted in place below, above or under `anchor`.
    NewTask {
        list_id: String,
        parent_id: Option<String>,
        /// Inserted after this sibling; at the start when absent.
        after: Option<String>,
    },
    /// Renaming a list or folder in the sidebar.
    SidebarName {
        row_id: String,
    },
    /// A new list or folder, named in the sidebar.
    NewList {
        folder_id: Option<String>,
    },
    NewFolder {
        parent_id: Option<String>,
    },
    /// A board column's name, new or renamed.
    ColumnName(columns::ColumnName),
    /// A filter list's or a rule column's rules, as text.
    Rules(columns::RulesTarget),
    /// The Conditions tab's free time, typed.
    ConditionTime,
    /// A condition's name; none for a new one.
    ConditionName(Option<String>),
    /// A condition category's new name; the category, none for "Other".
    CategoryName(Option<String>),
    /// The agent panel's message box.
    AgentMessage,
    /// The note on the focus prompt.
    FocusNote,
    /// A global shortcut, recorded from the next key pressed.
    Hotkey(crate::platform::hotkeys::Hotkey),
    /// A field of the inspector, by its stop.
    InspectorField {
        task_id: String,
        stop: &'static str,
    },
}

pub struct App {
    pub store: Store,
    pub bindings: Bindings,
    pub sequence: Sequence,
    pub prefs: Prefs,

    // What the sidebar is made of.
    pub folders: Vec<FolderRow>,
    pub lists: Vec<ListRow>,
    pub archived_lists: Vec<ListRow>,
    pub nested_lists: Vec<OutlineItem>,
    pub filter_lists: Vec<FilterListRow>,
    pub task_counts: std::collections::HashMap<String, i64>,
    pub sidebar_rows: Vec<SidebarRow>,
    pub sidebar_cursor: Option<String>,

    // Where you are.
    pub scope: Scope,
    pub view_mode: ViewMode,
    pub region: Region,
    pub overlay: Option<Overlay>,
    pub editing: Option<Editing>,
    pub back_stack: Vec<String>,
    pub forward_stack: Vec<String>,
    walking_history: bool,

    // The tasks on screen: every row the scope holds, and those drawn.
    /// Blocked, buried, waiting, linked, priority and tags, by task.
    pub details: details::Details,
    /// The Conditions tab: the conditions, what holds, the cursor.
    pub conditions: conditions::Conditions,
    /// The themes and the keymap file, and what in them could not be read.
    pub library: library::Library,
    pub agent: agent::Agent,
    /// Text the agent's message box opens with.
    pub agent_draft: Option<String>,
    /// The done rail's cursor.
    pub done_cursor: Option<String>,
    pub timeline: review::Timeline,
    pub progress: review::Progress,
    /// `:` typed: the digits so far of a row to jump to.
    pub number_jump: Option<String>,
    pub all_rows: Vec<OutlineRow>,
    pub rows: Vec<OutlineRow>,
    /// The day and the ladder, and whether they want ranking again.
    pub today: today::Today,
    pub today_stale: bool,
    /// Ticked off this visit, kept on screen while finished tasks are hidden.
    pub lingering: HashSet<String>,
    /// Each task's board column and matrix place, from the scope's board read.
    pub placements: std::collections::HashMap<String, outline::Placement>,
    /// A combined scope's cards (every open task) and those its board draws.
    pub combined_cards: Vec<TaskRow>,
    pub board_card_ids: Vec<String>,
    pub board: board::Board,
    /// The running focus session, and a task waiting on "start anyway".
    pub focus_session: Option<takt_core::rows::SessionRow>,
    pub pending_focus: Option<String>,
    /// "How did it go?", while it is asked.
    pub focus_prompt: Option<focus_prompt::FocusPrompt>,
    /// The block (or break) whose end has been announced.
    pub focus_notified: Option<String>,
    pub focus_checkpointed_at: i64,
    /// A task just finished, and when: drawn in the status bar a moment.
    pub celebration: Option<(String, i64)>,
    /// The floating focus panel asked for (true) or away (false).
    pub wants_panel: Option<bool>,
    /// The quick capture window, asked for.
    pub wants_capture: bool,
    /// The global shortcuts changed and want registering again.
    pub hotkeys_changed: bool,
    /// Shortcuts the system wouldn't register, for diagnostics.
    pub hotkey_problems: Vec<String>,
    /// The done rail: what was finished in the last fortnight, newest first.
    pub done: Vec<TaskRow>,
    /// Text the palette opens with: an edit's word (`due `, `tag `).
    pub palette_prefill: Option<String>,
    pub selected_task: Option<String>,
    pub pending_delete: Option<actions::PendingDelete>,
    /// `f`/`F` waiting for the letter to find (true: forward).
    pub pending_find: Option<bool>,
    pub last_find: Option<(char, bool)>,
    /// Where the cursor starts when a title opens for editing.
    pub edit_cursor: commands::EditCursor,

    /// The inspector's row cursor, and its read of the selection keyed by
    /// the task and the write generation.
    pub inspector_cursor: usize,
    pub inspected: Option<((String, u64), takt_core::editor::EditorSnapshot)>,
    /// Bumped by every write and reload, so caches of what was read know
    /// to read again. Our own writes do not move `data_version`.
    pub generation: u64,

    // The chrome.
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
    pub message: Option<String>,
    /// The `data_version` last read, to tell another process's write.
    pub seen_version: i64,
}

impl App {
    pub fn open() -> Result<Self, String> {
        let store = Store::open()?;
        let prefs = Prefs::load();
        let seen_version = store.core.data_version().unwrap_or_default();
        let mut app = App {
            store,
            bindings: Bindings::shipped(),
            sequence: Sequence::default(),
            folders: Vec::new(),
            lists: Vec::new(),
            archived_lists: Vec::new(),
            nested_lists: Vec::new(),
            filter_lists: Vec::new(),
            task_counts: Default::default(),
            sidebar_rows: Vec::new(),
            sidebar_cursor: None,
            scope: Scope::Everything,
            view_mode: ViewMode::Outline,
            region: Region::Tasks,
            overlay: None,
            editing: None,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            walking_history: false,
            details: Default::default(),
            conditions: Default::default(),
            library: Default::default(),
            agent: Default::default(),
            agent_draft: None,
            done_cursor: None,
            timeline: Default::default(),
            progress: None,
            number_jump: None,
            all_rows: Vec::new(),
            rows: Vec::new(),
            lingering: HashSet::new(),
            placements: Default::default(),
            combined_cards: Vec::new(),
            board_card_ids: Vec::new(),
            board: board::Board::default(),
            focus_session: None,
            pending_focus: None,
            focus_prompt: None,
            focus_notified: None,
            focus_checkpointed_at: 0,
            celebration: None,
            wants_panel: None,
            wants_capture: false,
            hotkeys_changed: false,
            hotkey_problems: Vec::new(),
            done: Vec::new(),
            palette_prefill: None,
            today: today::Today::default(),
            today_stale: true,
            selected_task: None,
            pending_delete: None,
            pending_find: None,
            last_find: None,
            edit_cursor: commands::EditCursor::default(),
            inspector_cursor: 0,
            inspected: None,
            generation: 0,
            undo_label: None,
            redo_label: None,
            message: None,
            seen_version,
            prefs,
        };
        app.reload_sidebar();
        app.reload_details();
        app.reload_themes();
        if library::keymap_path().exists() {
            app.reload_keymap();
            app.message = None;
        }
        app.reload_conditions();
        app.reload_focus();
        app.reload_done();
        app.restore_scope();
        app.refresh_history();
        Ok(app)
    }

    /// The surface the catalogue resolves keys against.
    pub fn surface(&self) -> CommandSurface {
        match self.region {
            Region::Sidebar => CommandSurface::Sidebar,
            Region::Inspector => CommandSurface::Inspector,
            Region::Conditions => CommandSurface::Conditions,
            Region::Done => CommandSurface::Done,
            Region::Timeline => CommandSurface::Timeline,
            Region::Agent => CommandSurface::Anywhere,
            Region::Tasks => match self.view_mode {
                ViewMode::Today => CommandSurface::Today,
                ViewMode::Board => CommandSurface::Board,
                ViewMode::Outline => CommandSurface::Outline,
                ViewMode::Matrix => CommandSurface::Matrix,
            },
        }
    }

    pub fn list(&self, id: &str) -> Option<&ListRow> {
        self.lists.iter().find(|list| list.id == id)
    }

    pub fn folder(&self, id: &str) -> Option<&FolderRow> {
        self.folders.iter().find(|folder| folder.id == id)
    }

    pub fn inbox(&self) -> Option<&ListRow> {
        self.lists
            .iter()
            .find(|list| list.system_role.as_deref() == Some("inbox"))
    }

    pub fn task(&self, id: &str) -> Option<&TaskRow> {
        self.rows
            .iter()
            .map(|row| &row.task)
            .chain(self.today.rows().map(|row| &row.task))
            .chain(self.nested_lists.iter().map(|item| &item.task))
            .find(|task| task.id == id)
    }

    pub fn selected(&self) -> Option<&TaskRow> {
        self.selected_task.as_deref().and_then(|id| self.task(id))
    }

    /// The list a new task goes in: the scope's, or the inbox.
    pub fn scope_list_id(&self) -> Option<String> {
        match &self.scope {
            Scope::List { list_id, .. } => Some(list_id.clone()),
            Scope::Folder(_) if self.folder_destination().is_some() => self.folder_destination(),
            _ => self
                .selected()
                .map(|task| task.list_id.clone())
                .or_else(|| self.inbox().map(|inbox| inbox.id.clone())),
        }
    }

    /// The list ids a combined scope reads.
    pub fn scope_list_ids(&self) -> Vec<String> {
        match &self.scope {
            Scope::List { list_id, .. } => vec![list_id.clone()],
            Scope::Everything | Scope::FilterList(_) => {
                self.lists.iter().map(|list| list.id.clone()).collect()
            }
            Scope::Folder(folder_id) => takt_core::sidebar::outline::sidebar_list_ids_in_folder(
                folder_id.clone(),
                self.sidebar_folders(),
                self.sidebar_lists(),
            ),
        }
    }

    /// The scope's name, for the title bar and the tab.
    pub fn scope_title(&self) -> String {
        if self.view_mode == ViewMode::Today {
            return "Today".into();
        }
        match &self.scope {
            Scope::Everything => "Everything".into(),
            Scope::List { list_id, entered } => {
                let list = self
                    .list(list_id)
                    .map(|list| list.name.clone())
                    .unwrap_or_default();
                match entered.as_deref().and_then(|id| self.entered_title(id)) {
                    Some(title) => format!("{list} › {title}"),
                    None => list,
                }
            }
            Scope::Folder(id) => self
                .folder(id)
                .map(|folder| folder.name.clone())
                .unwrap_or_default(),
            Scope::FilterList(id) => self
                .filter_lists
                .iter()
                .find(|filter| &filter.id == id)
                .map(|filter| filter.name.clone())
                .unwrap_or_default(),
        }
    }

    fn entered_title(&self, id: &str) -> Option<String> {
        self.task(id).map(|task| task.title.clone()).or_else(|| {
            self.store
                .core
                .task(id.into())
                .ok()
                .flatten()
                .map(|t| t.title)
        })
    }

    /// Runs a write through the core and, on success, `patch` on what is
    /// held. Errors land in the status bar rather than in a dialog.
    pub fn write<T>(
        &mut self,
        work: impl FnOnce(&Store) -> Result<T, takt_core::CoreError>,
    ) -> Option<T> {
        match work(&self.store) {
            Ok(value) => {
                self.message = None;
                self.generation += 1;
                self.refresh_history();
                // Our own write: note the version so the poll does not take
                // it for another process's.
                self.seen_version = self.store.core.data_version().unwrap_or(self.seen_version);
                Some(value)
            }
            Err(error) => {
                self.message = Some(error.to_string());
                None
            }
        }
    }

    /// The done rail's rows: finished in the last fourteen days.
    pub fn reload_done(&mut self) {
        let since = store::now_ms() - 14 * 24 * 3600 * 1000;
        self.done = self
            .store
            .core
            .completed_tasks_since(since, 300)
            .unwrap_or_default();
    }

    pub fn refresh_history(&mut self) {
        self.undo_label = self.store.core.undoable_label().ok().flatten();
        self.redo_label = self.store.core.redoable_label().ok().flatten();
    }

    /// Polled about once a second: another process (the CLI, the MCP
    /// server, the Swift app, sync) wrote, so what is held is read again.
    pub fn check_external_writes(&mut self) -> bool {
        let Ok(version) = self.store.core.data_version() else {
            return false;
        };
        if version == self.seen_version {
            return false;
        }
        self.seen_version = version;
        self.reload_all();
        true
    }

    /// Reads the sidebar and the scope again, keeping the cursor and the
    /// selection where they were when they still exist.
    pub fn reload_all(&mut self) {
        self.generation += 1;
        self.reload_focus();
        self.reload_done();
        self.reload_sidebar();
        self.reload_details();
        self.reload_conditions();
        self.reload_rows();
        self.refresh_history();
    }
}
