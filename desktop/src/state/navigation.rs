//! Where you are: the scope, the view mode, a task entered as though it
//! were a list, and the back and forward history of lists visited.
//!
//! Follows the Swift app's `WorkspaceViewModel+Navigation.swift`: a list
//! opens as a board (the inbox as an outline), Everything and folders as a
//! board, and a switch keeps nothing selected that the new scope lacks.

use super::{App, Region, Scope, ViewMode};

const HISTORY_LIMIT: usize = 50;
const RECENT_LIMIT: usize = 30;

impl App {
    fn arrive(&mut self, scope: Scope, mode: ViewMode) {
        let leaving = self.current_list_id();
        self.scope = scope;
        self.view_mode = mode;
        self.sequence.reset();
        self.pending_delete = None;
        self.editing = None;
        self.clear_lingering();
        self.selected_task = None;
        self.reload_rows();
        let arriving = self.current_list_id();
        self.note_list_visit(leaving, arriving);
        self.save_place();
    }

    fn current_list_id(&self) -> Option<String> {
        match &self.scope {
            Scope::List { list_id, .. } if self.view_mode != ViewMode::Today => {
                Some(list_id.clone())
            }
            _ => None,
        }
    }

    fn default_mode_for_list(&self, list_id: &str) -> ViewMode {
        let inbox = self
            .list(list_id)
            .is_some_and(|list| list.system_role.as_deref() == Some("inbox"));
        if inbox {
            ViewMode::Outline
        } else {
            ViewMode::Board
        }
    }

    /// A different list opens in its own default mode; the same one keeps
    /// the mode you had.
    pub fn select_list(&mut self, id: &str) {
        let same = matches!(&self.scope, Scope::List { list_id, entered: None } if list_id == id)
            && self.view_mode != ViewMode::Today;
        if same {
            return;
        }
        let mode = self.default_mode_for_list(id);
        self.arrive(
            Scope::List {
                list_id: id.into(),
                entered: None,
            },
            mode,
        );
    }

    /// A nested list is a task in its list, entered.
    pub fn select_nested_list(&mut self, task_id: &str) {
        let Some(task) = self
            .nested_lists
            .iter()
            .find(|item| item.task.id == task_id)
            .map(|item| item.task.clone())
        else {
            return;
        };
        self.arrive(
            Scope::List {
                list_id: task.list_id,
                entered: Some(task.id),
            },
            ViewMode::Board,
        );
    }

    pub fn select_everything(&mut self) {
        if self.scope == Scope::Everything && self.view_mode != ViewMode::Today {
            return;
        }
        self.arrive(Scope::Everything, ViewMode::Board);
    }

    pub fn select_folder(&mut self, id: &str) {
        if self.scope == Scope::Folder(id.into()) && self.view_mode != ViewMode::Today {
            return;
        }
        self.arrive(Scope::Folder(id.into()), ViewMode::Board);
    }

    pub fn select_filter_list(&mut self, id: &str) {
        if self.scope == Scope::FilterList(id.into()) && self.view_mode != ViewMode::Today {
            return;
        }
        self.arrive(Scope::FilterList(id.into()), ViewMode::Outline);
    }

    pub fn select_today(&mut self) {
        if self.view_mode == ViewMode::Today {
            return;
        }
        self.view_mode = ViewMode::Today;
        self.editing = None;
        self.reload_today();
        self.save_place();
    }

    pub fn select_inbox(&mut self) {
        if let Some(id) = self.inbox().map(|inbox| inbox.id.clone()) {
            self.select_list(&id);
        }
    }

    /// `1`–`4`, the mode strip: the same scope drawn another way. Leaving
    /// Today goes back to the scope you were in.
    pub fn select_view_mode(&mut self, mode: ViewMode) {
        if mode == ViewMode::Today {
            self.select_today();
            return;
        }
        if self.view_mode == mode {
            return;
        }
        let from_today = self.view_mode == ViewMode::Today;
        self.view_mode = mode;
        self.editing = None;
        if from_today {
            self.reload_rows();
        } else {
            self.layout_board();
        }
        if self.selected_index().is_none() {
            self.selected_task = self.visible_ids().first().cloned();
        }
        self.save_place();
    }

    /// `Return` on a task with children (or `.`): the task becomes the
    /// scope, its subtasks the rows.
    pub fn enter_selected(&mut self) {
        let Some(task) = self.selected().cloned() else {
            return;
        };
        self.arrive(
            Scope::List {
                list_id: task.list_id,
                entered: Some(task.id),
            },
            if self.view_mode == ViewMode::Outline {
                ViewMode::Outline
            } else {
                ViewMode::Board
            },
        );
    }

    /// Back up a level from an entered task, selecting the task left.
    pub fn leave_entered_task(&mut self) {
        let Scope::List {
            list_id,
            entered: Some(entered),
        } = self.scope.clone()
        else {
            return;
        };
        let parent = self
            .store
            .core
            .task(entered.clone())
            .ok()
            .flatten()
            .and_then(|task| task.parent_task_id);
        let visible_root = self
            .list(&list_id)
            .and_then(|list| list.visible_root_task_id.clone());
        let entered_parent = parent.filter(|parent| Some(parent) != visible_root.as_ref());
        self.scope = Scope::List {
            list_id,
            entered: entered_parent,
        };
        self.clear_lingering();
        self.reload_rows();
        self.selected_task = Some(entered);
        self.save_place();
    }

    /// `⌥⌘↑`/`⌥⌘↓`: the list before or after this one, in sidebar order.
    pub fn select_adjacent_list(&mut self, by: i64) {
        let order = self.lists_in_sidebar_order();
        if order.is_empty() {
            return;
        }
        let current = self
            .current_list_id()
            .and_then(|id| order.iter().position(|other| *other == id));
        let index = match current {
            Some(current) => (current as i64 + by).rem_euclid(order.len() as i64) as usize,
            None if by < 0 => order.len() - 1,
            None => 0,
        };
        let id = order[index].clone();
        self.select_list(&id);
        self.sidebar_cursor = None;
    }

    /// `⌃-` and `⌃⇧-`: back and forward through the lists visited,
    /// skipping any since deleted.
    pub fn walk_history(&mut self, forward: bool) {
        let current = self.current_list_id();
        loop {
            let next = if forward {
                self.forward_stack.pop()
            } else {
                self.back_stack.pop()
            };
            let Some(id) = next else {
                return;
            };
            if self.list(&id).is_none() {
                continue;
            }
            if let Some(current) = current.clone() {
                if forward {
                    self.back_stack.push(current);
                } else {
                    self.forward_stack.push(current);
                }
            }
            self.walking_history = true;
            self.select_list(&id);
            self.walking_history = false;
            self.sidebar_cursor = None;
            return;
        }
    }

    fn note_list_visit(&mut self, leaving: Option<String>, arriving: Option<String>) {
        if leaving == arriving {
            return;
        }
        if let Some(arriving) = &arriving {
            self.prefs.recent_lists.retain(|id| id != arriving);
            self.prefs.recent_lists.insert(0, arriving.clone());
            self.prefs.recent_lists.truncate(RECENT_LIMIT);
        }
        if self.walking_history {
            return;
        }
        if let Some(leaving) = leaving {
            self.back_stack.push(leaving);
            if self.back_stack.len() > HISTORY_LIMIT {
                self.back_stack.remove(0);
            }
            self.forward_stack.clear();
        }
    }

    // MARK: Regions

    pub fn focus_region(&mut self, region: Region) {
        if region == Region::Sidebar {
            self.return_to_sidebar();
            return;
        }
        if region == Region::Inspector && self.selected().is_none() {
            return;
        }
        if region == Region::Inspector {
            self.prefs.right_dock_visible = true;
            self.prefs.right_dock_tab = "inspector".into();
            self.prefs.save();
        }
        self.region = region;
    }

    /// `⌃Tab`/`⌃⇧Tab`: sidebar, tasks, inspector, round.
    pub fn cycle_region(&mut self, backward: bool) {
        let mut order = Vec::new();
        if self.prefs.sidebar_visible {
            order.push(Region::Sidebar);
        }
        order.push(Region::Tasks);
        if self.prefs.right_dock_visible && self.selected().is_some() {
            order.push(Region::Inspector);
        }
        let current = order.iter().position(|r| *r == self.region).unwrap_or(0) as i64;
        let next = (current + if backward { -1 } else { 1 }).rem_euclid(order.len() as i64);
        self.focus_region(order[next as usize]);
    }

    // MARK: Remembering where you were

    fn save_place(&mut self) {
        self.prefs.scope = match &self.scope {
            Scope::Everything => "everything".into(),
            Scope::List {
                list_id,
                entered: None,
            } => format!("list:{list_id}"),
            Scope::List {
                list_id,
                entered: Some(task),
            } => format!("task:{list_id}:{task}"),
            Scope::Folder(id) => format!("folder:{id}"),
            Scope::FilterList(id) => format!("filter:{id}"),
        };
        self.prefs.view_mode = self.view_mode.title().to_lowercase();
        self.prefs.save();
    }

    /// At launch: the scope and mode last shown, if they still exist; the
    /// inbox otherwise.
    pub fn restore_scope(&mut self) {
        let saved = self.prefs.scope.clone();
        let mut parts = saved.splitn(3, ':');
        let scope = match (parts.next(), parts.next(), parts.next()) {
            (Some("everything"), _, _) => Some(Scope::Everything),
            (Some("list"), Some(id), _) if self.list(id).is_some() => Some(Scope::List {
                list_id: id.into(),
                entered: None,
            }),
            (Some("task"), Some(list), Some(task))
                if self.list(list).is_some()
                    && self.store.core.task(task.into()).ok().flatten().is_some() =>
            {
                Some(Scope::List {
                    list_id: list.into(),
                    entered: Some(task.into()),
                })
            }
            (Some("folder"), Some(id), _) if self.folder(id).is_some() => {
                Some(Scope::Folder(id.into()))
            }
            (Some("filter"), Some(id), _) if self.filter_lists.iter().any(|f| f.id == id) => {
                Some(Scope::FilterList(id.into()))
            }
            _ => None,
        };
        let scope = scope.or_else(|| {
            self.inbox().map(|inbox| Scope::List {
                list_id: inbox.id.clone(),
                entered: None,
            })
        });
        let mode = match self.prefs.view_mode.as_str() {
            "today" => ViewMode::Today,
            "board" => ViewMode::Board,
            "matrix" => ViewMode::Matrix,
            "outline" => ViewMode::Outline,
            _ => match &scope {
                Some(Scope::List { list_id, .. }) => self.default_mode_for_list(list_id),
                _ => ViewMode::Board,
            },
        };
        self.scope = scope.unwrap_or(Scope::Everything);
        self.view_mode = mode;
        self.reload_rows();
        if mode == ViewMode::Today {
            self.reload_today();
        }
    }
}
