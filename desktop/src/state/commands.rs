//! What each catalogue command does, and the key press that reaches one.
//!
//! The Swift app's `WorkspaceViewModel+Commands.swift` `run` switch, by the
//! catalogue's ids. A command the Rust app cannot do yet says so in the
//! status bar rather than doing nothing; `PORTED` lists those it can, and a
//! test holds every catalogue id to being either ported or reported.

use super::outline::NewPlacement;
use super::store::now_ms;
use super::{App, Editing, Overlay, Region, Scope, ViewMode};
use crate::keys::Step;
use std::time::Instant;
use takt_core::editor::EditorSnapshot;
use takt_core::keymap::CommandSurface;

/// What a key press came to, for the window to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyOutcome {
    /// Nothing claimed it: let the toolkit have it.
    Unhandled,
    /// Handled; draw again.
    Handled,
    /// The first of a two-key sequence is held: arm the release timer.
    Held(Instant),
}

impl App {
    /// A spelled key press with no text field or overlay open.
    pub fn handle_key(&mut self, key: &str, at: Instant) -> KeyOutcome {
        if self.focus_prompt_key(key) {
            return KeyOutcome::Handled;
        }
        if self.confirm_pending_focus(key) {
            return KeyOutcome::Handled;
        }
        if self.pending_delete.is_some() {
            if key == "enter" {
                self.delete_pending();
            } else {
                self.pending_delete = None;
                self.message = None;
            }
            return KeyOutcome::Handled;
        }
        if self.number_jump_key(key) {
            return KeyOutcome::Handled;
        }
        if let Some(forward) = self.pending_find.take() {
            if key != "escape" && key.chars().count() == 1 {
                self.find(key.chars().next().unwrap_or(' '), forward);
            }
            return KeyOutcome::Handled;
        }
        let surface = self.surface();
        let sequences = self.bindings.sequences(surface);
        match self.sequence.advance(key, at, &sequences) {
            Step::Pending => KeyOutcome::Held(at),
            Step::Command(sequence) => {
                self.dispatch(&sequence);
                KeyOutcome::Handled
            }
            Step::Flush(held) => {
                self.dispatch(&held);
                self.dispatch_or_unhandled(key)
            }
            Step::FlushAndHold(held) => {
                self.dispatch(&held);
                KeyOutcome::Held(at)
            }
            Step::Pass => self.dispatch_or_unhandled(key),
        }
    }

    /// The release timer fired: a held key runs on its own.
    pub fn release_held_key(&mut self, held_at: Instant) -> bool {
        match self.sequence.expire(held_at) {
            Some(key) => {
                self.dispatch(&key);
                true
            }
            None => false,
        }
    }

    fn dispatch_or_unhandled(&mut self, key: &str) -> KeyOutcome {
        if self.dispatch(key) {
            KeyOutcome::Handled
        } else {
            KeyOutcome::Unhandled
        }
    }

    /// Runs the command bound to `key` on the surface with the keyboard.
    pub fn dispatch(&mut self, key: &str) -> bool {
        let surface = self.surface();
        let Some(command) = self.bindings.command(key, surface).map(|c| c.id.clone()) else {
            return false;
        };
        self.run(&command, Some(key));
        true
    }

    /// Runs a command by catalogue id; `key` is the press that asked, which
    /// motions read (`j` or `k`, which digit).
    pub fn run(&mut self, id: &str, key: Option<&str>) {
        let key = key.unwrap_or("");
        let tasks = self.region != Region::Sidebar;
        match id {
            // Go
            "goToday" => self.select_view_mode(ViewMode::Today),
            "goBoard" => self.select_view_mode(ViewMode::Board),
            "goOutline" => self.select_view_mode(ViewMode::Outline),
            "goMatrix" => self.select_view_mode(ViewMode::Matrix),
            "goEverything" => {
                self.select_everything();
                self.region = Region::Tasks;
            }
            "goInbox" => {
                self.select_inbox();
                self.region = Region::Tasks;
            }
            "goPreviousList" => self.select_adjacent_list(-1),
            "goNextList" => self.select_adjacent_list(1),
            "goBack" => self.walk_history(false),
            "goForward" => self.walk_history(true),
            "goSidebarRegion" => self.focus_region(Region::Sidebar),
            "goTaskRegion" => self.focus_region(Region::Tasks),
            "goInspectorRegion" => self.focus_region(Region::Inspector),
            "goCycleRegion" => self.cycle_region(key.starts_with("ctrl+shift")),
            "goCommandPalette" => self.open_overlay(Overlay::CommandPalette),
            "goListNavigator" => self.open_overlay(Overlay::ListNavigator),
            "goSearch" => self.open_overlay(Overlay::Search),
            "goKeyboardReference" => self.open_overlay(Overlay::KeyboardReference),

            // Focus
            "goFocus" => self.wants_panel = Some(true),
            "taskStartFocus" => self.start_focus_on_selected(false),
            "focusFinish" => self.request_focus_completion(true),
            "focusStop" => self.stop_focus(),

            // The done rail
            "windowToggleDoneRail" => self.toggle_bottom_tab("done"),
            "doneReopen" => {
                if self.region == Region::Done {
                    self.reopen_done()
                } else {
                    self.reopen_selected()
                }
            }
            "doneClose" => {
                if self.region == Region::Done {
                    self.region = Region::Tasks;
                }
            }
            "goTimeline" => self.go_timeline(),
            "timelinePreviousDay" => self.step_timeline_day(-1),
            "timelineNextDay" => self.step_timeline_day(1),
            "timelineToday" => self.timeline_today(),
            "motionTimelineSelect" => self.move_timeline_cursor(key),
            "timelineReveal" => self.reveal_timeline(),
            "timelineClose" => self.leave_timeline(),
            "windowToggleProgressDock" => self.toggle_bottom_dock(),
            "windowDockNextTab" => self.cycle_dock_tab(1),
            "windowDockPreviousTab" => self.cycle_dock_tab(-1),

            // Task
            "taskNew" => self.begin_new_task(NewPlacement::Below),
            "taskNewAbove" => self.begin_new_task(NewPlacement::Above),
            "taskNewChild" => self.begin_new_task(NewPlacement::Child),
            "taskComplete" => self.toggle_complete(),
            "taskInvalidate" => self.toggle_invalidate(),
            "taskDelete" => self.ask_to_delete(),
            "taskRename" | "taskRenameAppend" | "taskRenameInsert" => self.begin_rename(id),
            "taskEditDue" | "taskEditStart" | "taskEditRecurrence" | "taskEditTags" => {
                self.begin_typed_edit(id)
            }
            "taskDueToday" => self.set_due_in_days(Some(0)),
            "taskDueTomorrow" => self.set_due_in_days(Some(1)),
            "taskClearDue" => self.set_due_in_days(None),
            "taskClearNotes" => self.edit_selected(|edit| edit.notes.clear()),
            "taskClearTags" => self.edit_selected(|edit| edit.metadata.tags.clear()),
            "taskClearPriority" => self.edit_selected(|edit| edit.metadata.priority = None),
            "taskSendToBack" => self.edit_selected(|edit| edit.metadata.priority = Some(-1)),
            "taskClearStart" => self.schedule_selected(None),
            "taskTogglePlannedToday" => self.toggle_planned_today(),
            "taskToggleInspector" => self.toggle_right_tab("inspector"),
            "taskGoToList" => self.go_to_selected_list(),
            "taskIndent" => self.indent_selected(false),
            "taskOutdent" => self.indent_selected(true),
            "taskMoveUp" => self.move_selected(-1),
            "taskMoveDown" => self.move_selected(1),
            "taskMoveToPreviousList" => self.move_selected_to_adjacent_list(-1),
            "taskMoveToNextList" => self.move_selected_to_adjacent_list(1),

            // Plan
            "planEnterTask" => self.enter_selected(),
            "planLeaveTask" => self.leave_entered_task(),
            "planHideCompleted" => self.toggle_hide_completed(),
            "planToggleFold" => self.toggle_fold(),
            "planFoldAll" => self.set_all_folded(true),
            "planBoardMoveCardLeft" => self.move_card_to_adjacent_column(-1),
            "planBoardMoveCardRight" => self.move_card_to_adjacent_column(1),
            "planMatrixPlace" => {
                if let Some(digit) = key.chars().last().and_then(|c| c.to_digit(10)) {
                    self.place_in_quadrant(digit.saturating_sub(1) as usize);
                }
            }
            "planUnfoldAll" => self.set_all_folded(false),

            // Lists and folders
            "listNew" => self.begin_new_list(),
            "folderNew" => self.begin_new_folder(),
            "listRename" => self.begin_sidebar_rename(),
            "listArchive" => self.archive_current_list(true),
            "folderCollapseAll" => self.set_all_folders_expanded(false),
            "folderExpandAll" => self.set_all_folders_expanded(true),
            "listMoveUp" => self.move_current_list(-1),
            "listMoveDown" => self.move_current_list(1),
            "folderMoveUp" => self.move_current_folder(-1),
            "folderMoveDown" => self.move_current_folder(1),

            // Window
            "windowUndo" => self.undo(false),
            "windowRedo" => self.undo(true),
            "windowToggleSidebar" => self.toggle_sidebar(),
            "windowToggleInspectorPane" => self.toggle_right_tab("inspector"),
            "windowToggleRightDock" => self.toggle_right_dock(),
            "windowCloseAllDocks" => self.close_all_docks(),

            // Motions
            "motionSelectNext"
            | "motionSelectPrevious"
            | "motionSelectEnds"
            | "motionSelectPage" => {
                if tasks {
                    self.move_selection(key)
                } else {
                    self.move_sidebar_cursor(key)
                }
            }
            "motionBoardColumn" => {
                let back = key.contains("left") || key == "h";
                self.move_board_column(if back { -1 } else { 1 });
            }
            "motionSidebarSelect" => self.move_sidebar_cursor(key),
            "motionSidebarExpand" => self.activate_sidebar_cursor(key != "enter", key == "enter"),
            "motionSidebarCollapse" => self.collapse_sidebar_cursor(),
            "motionOutlineUnfold" | "motionTodayUnfold" => self.unfold_or_descend(),
            "motionOutlineFold" => self.fold_or_ascend(),
            "motionTodayLeave" => self.focus_region(Region::Sidebar),
            "motionFindForward" => self.pending_find = Some(true),
            "motionFindBackward" => self.pending_find = Some(false),
            "motionFindRepeat" => self.repeat_find(),
            "motionDismiss" => self.dismiss(),

            // The inspector's cursor
            "motionInspectorNext" => self.step_inspector(1),
            "motionInspectorPrevious" => self.step_inspector(-1),
            "motionInspectorEnds" => self.inspector_to_end(!(key == "gg" || key == "home")),
            "motionInspectorEdit" => self.activate_inspector(),
            "motionInspectorClear" => self.clear_inspector_stop(),
            "motionInspectorLeave" => self.inspector_left(key == "left"),
            "taskEditNotes" => self.edit_in_inspector("notes"),
            "taskBury" => self.bury_selected(),
            "planHideLater" => self.toggle_hide_later(),
            "planGrowTask" => self.grow_selected(),
            "planShrinkTask" => self.shrink_selected(),
            "motionSetPriority" => self.set_priority_from_key(key),
            "motionGoToNumber" => self.begin_number_jump(),
            "taskMove" => self.begin_move(),
            "taskConvertToList" => self.convert_selected(),
            "taskPromoteList" => self.toggle_promoted(),
            "taskExtractBranch" => self.extract_branch(),
            "taskOpenLink" => self.open_first_link(),
            "taskAddLink" => self.begin_add_link(),
            "taskShowProgress" => self.show_progress(),
            "taskAddBlocker" => {
                self.pick_for_selected_task(|task_id| super::pickers::Picker::Blocker { task_id })
            }
            "taskWaiting" => {
                self.pick_for_selected_task(|task_id| super::pickers::Picker::Waiting { task_id })
            }
            "listIcon" => self.begin_icon_pick(),
            "listSettings" => self.begin_list_settings(),
            "windowToggleAgentPanel" => self.toggle_agent_panel(),
            "windowUseClaudeCode" => self.use_agent(super::agent::Client::ClaudeCode),
            "windowUseCodex" => self.use_agent(super::agent::Client::Codex),
            "taskAskAgent" => self.ask_agent_about_selected(),
            "windowOpenKeymap" => self.open_keymap_file(),
            "windowReloadKeymap" => self.reload_keymap(),
            "windowOpenThemesFolder" => self.open_themes_folder(),
            "windowReloadThemes" => {
                self.reload_themes();
                let issues = self.library.theme_issues.len();
                self.message = Some(format!(
                    "{} themes{}.",
                    self.library.themes.len(),
                    if issues > 0 {
                        format!(", {issues} problems: see diagnostics")
                    } else {
                        String::new()
                    }
                ));
            }
            "windowExportTheme" => self.export_theme(),
            "windowShowDiagnostics" => self.open_overlay(Overlay::Diagnostics),
            "taskToggleDaily" => self.begin_commitment(),
            "taskHabit" => self.begin_habit(),
            "todayStart" => {
                let running = self
                    .focus_session
                    .as_ref()
                    .and_then(|s| s.active_task_id.clone());
                match (running, self.selected_task.clone()) {
                    (Some(running), Some(selected)) if running == selected => {
                        self.request_focus_completion(true)
                    }
                    (_, _) if self.focus_running() => self.add_selected_to_focus_queue(),
                    _ => self.start_focus_on_selected(true),
                }
            }
            "taskConditions" => self.edit_task_conditions(),
            "focusSetAvailable" => self.open_conditions_tab(false),
            "focusManageConditions" => self.open_conditions_tab(true),
            "focusSetPlace" => {
                self.reload_conditions();
                self.open_picker(super::pickers::Picker::Place);
            }
            "motionConditionsSelect"
            | "conditionsToggle"
            | "conditionsAdjust"
            | "conditionsTypeTime"
            | "conditionsEdit"
            | "conditionsNew"
            | "conditionsCategory"
            | "conditionsProvides"
            | "conditionsArchive"
            | "conditionsMoveUp"
            | "conditionsMoveDown"
            | "conditionsMoveCategoryUp"
            | "conditionsMoveCategoryDown"
            | "conditionsRenameCategory"
            | "conditionsMerge"
            | "conditionsShowTasks"
            | "conditionsFold"
            | "conditionsFoldAll"
            | "conditionsClear"
            | "conditionsUseAgain"
            | "conditionsClose" => self.run_conditions(id, key),
            "motionDoneSelect" => self.move_done_cursor(key),
            "doneReveal" => self.reveal_done(),
            "planBoardNewColumn" => self.begin_new_column(None),
            "planBoardAddColumnLeft" => self.begin_new_column(Some(-1)),
            "planBoardAddColumnRight" => self.begin_new_column(Some(1)),
            "planBoardRenameColumn" => self.begin_rename_column(),
            "planBoardRemoveColumn" => self.ask_to_delete_column(),
            "planBoardMoveColumnLeft" => self.move_column(-1),
            "planBoardMoveColumnRight" => self.move_column(1),
            "planBoardEditColumnRules" => self.begin_column_rules(),
            "planBoardMakeDefaultColumns" => self.make_default_columns(),
            "planBoardResetColumns" => self.reset_columns(),
            "filterListNew" => self.begin_filter_list_edit(true),
            "filterListEdit" => self.begin_filter_list_edit(false),
            "listRestore" => self.restore_last_archived(),
            "listComplete" => self.toggle_list_completed(),
            "listDelete" => self.ask_to_delete_list(),
            "folderSelectPrevious" => self.select_adjacent_folder(-1),
            "folderSelectNext" => self.select_adjacent_folder(1),
            "listNewTaskDestination" => {
                self.step_new_task_destination(if key.contains('[') { -1 } else { 1 })
            }
            "taskEditEstimate" => self.edit_in_inspector("estimate"),

            other => {
                let title = self
                    .bindings
                    .entry(other)
                    .map(|entry| entry.title.clone())
                    .unwrap_or_else(|| other.to_string());
                self.message = Some(format!("{title} isn't in the Rust app yet."));
            }
        }
    }

    // MARK: Overlays and docks

    pub fn open_overlay(&mut self, overlay: Overlay) {
        self.sequence.reset();
        self.overlay = Some(overlay);
    }

    pub fn close_overlay(&mut self) {
        self.overlay = None;
    }

    fn reopen_selected(&mut self) {
        let Some(task) = self.selected().cloned().or_else(|| {
            let id = self.selected_task.clone()?;
            self.done.iter().find(|task| task.id == id).cloned()
        }) else {
            return;
        };
        if task.status != "open" {
            self.set_status(&task.id, "open");
            self.message = Some(format!("Reopened \u{201c}{}\u{201d}.", task.title));
        }
    }

    /// Esc: closes what is open, innermost first.
    fn dismiss(&mut self) {
        if self.overlay.is_some() {
            self.overlay = None;
        } else if self.editing.is_some() {
            self.editing = None;
        } else if self.region == Region::Inspector {
            self.region = Region::Tasks;
        } else if matches!(
            self.scope,
            Scope::List {
                entered: Some(_),
                ..
            }
        ) {
            self.leave_entered_task();
        } else {
            self.message = None;
        }
    }

    // MARK: Task commands

    fn ask_to_delete(&mut self) {
        let Some(task) = self.selected().cloned() else {
            return;
        };
        if !self.prefs.confirm_delete {
            self.delete_task(&task.id);
            return;
        }
        self.pending_delete = Some(super::actions::PendingDelete::Task(task.id));
        self.message = Some(format!(
            "Delete \u{201c}{}\u{201d}? Return deletes, any other key keeps it.",
            task.title
        ));
    }

    fn begin_rename(&mut self, id: &str) {
        let Some(task) = self.selected().cloned() else {
            return;
        };
        self.edit_cursor = match id {
            "taskRenameInsert" => EditCursor::Start,
            "taskRenameAppend" => EditCursor::End,
            _ => EditCursor::SelectAll,
        };
        self.editing = Some(Editing::TaskTitle { task_id: task.id });
    }

    /// Edits the selection through the core's editor, as one undo step.
    pub fn edit_selected(&mut self, change: impl FnOnce(&mut EditorSnapshot)) {
        let Some(id) = self.selected_task.clone() else {
            return;
        };
        let zone = self.store.zone.clone();
        let saved = self.write(|store| {
            let baseline = store.core.editor_snapshot(id.clone())?;
            let mut edit = baseline.clone();
            change(&mut edit);
            store.core.save_editor(edit, baseline, now_ms(), zone)
        });
        if let Some(saved) = saved {
            for row in self.all_rows.iter_mut().filter(|row| row.task.id == id) {
                row.task.title = saved.title.clone();
                row.task.notes = saved.notes.clone();
                row.task.due_at_ms = saved.due_at_ms;
                row.task.estimate_seconds = saved.estimate_seconds;
            }
            self.patch_details(&id);
            self.layout_rows();
            self.reload_today();
        }
    }

    fn set_due_in_days(&mut self, days: Option<i64>) {
        let due = days.map(start_of_local_day);
        self.edit_selected(|edit| edit.due_at_ms = due);
    }

    pub fn schedule_selected(&mut self, start: Option<i64>) {
        let Some(id) = self.selected_task.clone() else {
            return;
        };
        let zone = self.store.zone.clone();
        if self
            .write(|store| store.core.schedule_task(id.clone(), start, now_ms(), zone))
            .is_some()
        {
            self.reload_rows();
            self.reload_today();
        }
    }

    fn toggle_planned_today(&mut self) {
        let Some(id) = self.selected_task.clone() else {
            return;
        };
        let planned = self
            .today
            .day
            .iter()
            .any(|row| row.task.id == id && row.reason == "planned");
        if self
            .write(|store| {
                store
                    .core
                    .set_planned_for_today(!planned, vec![id.clone()], now_ms())
            })
            .is_some()
        {
            self.message = Some(
                if planned {
                    "Taken off today."
                } else {
                    "Planned for today."
                }
                .into(),
            );
            self.reload_today();
        }
    }

    fn go_to_selected_list(&mut self) {
        let Some(task) = self.selected().cloned() else {
            return;
        };
        self.select_list(&task.list_id);
        self.selected_task = Some(task.id);
        self.region = Region::Tasks;
    }

    fn move_selected_to_adjacent_list(&mut self, by: i64) {
        let Some(task) = self.selected().cloned() else {
            return;
        };
        let order = self.lists_in_sidebar_order();
        let Some(current) = order.iter().position(|id| *id == task.list_id) else {
            return;
        };
        let target = (current as i64 + by).rem_euclid(order.len() as i64) as usize;
        let target = order[target].clone();
        self.move_selected_to_list(&target);
        let name = self
            .list(&target)
            .map(|list| list.name.clone())
            .unwrap_or_default();
        self.message = Some(format!("Moved to {name}."));
    }

    // MARK: Find (`f`, `F`, `;`)

    pub fn find(&mut self, letter: char, forward: bool) {
        self.last_find = Some((letter, forward));
        let ids = self.visible_ids();
        if ids.is_empty() {
            return;
        }
        let start = self.selected_index().unwrap_or(0);
        let count = ids.len();
        let letter = letter.to_lowercase().next().unwrap_or(letter);
        for step in 1..=count {
            let ix = if forward {
                (start + step) % count
            } else {
                (start + count * 2 - step) % count
            };
            let starts = self
                .task(&ids[ix])
                .and_then(|task| task.title.chars().next())
                .is_some_and(|first| first.to_lowercase().next() == Some(letter));
            if starts {
                self.select_row(ix);
                return;
            }
        }
    }

    fn repeat_find(&mut self) {
        if let Some((letter, forward)) = self.last_find {
            self.find(letter, forward);
        }
    }

    // MARK: The sidebar's lists and folders

    fn begin_new_list(&mut self) {
        let folder_id = match &self.scope {
            Scope::Folder(id) => Some(id.clone()),
            Scope::List { list_id, .. } => self.list(list_id).and_then(|l| l.folder_id.clone()),
            _ => None,
        };
        self.prefs.sidebar_visible = true;
        self.region = Region::Sidebar;
        self.editing = Some(Editing::NewList { folder_id });
    }

    fn begin_new_folder(&mut self) {
        let parent_id = match &self.scope {
            Scope::Folder(id) => Some(id.clone()),
            _ => None,
        };
        self.prefs.sidebar_visible = true;
        self.region = Region::Sidebar;
        self.editing = Some(Editing::NewFolder { parent_id });
    }

    fn begin_sidebar_rename(&mut self) {
        let Some(row) = self.sidebar_cursor_row().cloned() else {
            return;
        };
        if matches!(
            row.kind,
            super::sidebar::RowKind::List(_) | super::sidebar::RowKind::Folder(_)
        ) {
            self.prefs.sidebar_visible = true;
            self.region = Region::Sidebar;
            self.editing = Some(Editing::SidebarName { row_id: row.id });
        }
    }

    /// Saves a name typed into the sidebar: a new list or folder, or a
    /// rename.
    pub fn commit_sidebar_name(&mut self, name: &str) {
        let name = name.trim().to_string();
        let editing = self.editing.take();
        if name.is_empty() {
            return;
        }
        let workspace = self.store.workspace_id.clone();
        match editing {
            Some(Editing::NewList { folder_id }) => {
                if let Some(created) = self.write(|store| {
                    store
                        .core
                        .create_list(workspace, name.clone(), folder_id, now_ms())
                }) {
                    self.reload_sidebar();
                    self.select_list(&created.id);
                    self.sidebar_cursor = None;
                }
            }
            Some(Editing::NewFolder { parent_id }) => {
                if let Some(created) = self.write(|store| {
                    store
                        .core
                        .create_folder(workspace, name.clone(), parent_id, now_ms())
                }) {
                    self.reload_sidebar();
                    self.set_folder_expanded(&created.id, true);
                    self.select_folder(&created.id);
                }
            }
            Some(Editing::SidebarName { row_id }) => {
                let kind = self
                    .sidebar_rows
                    .iter()
                    .find(|row| row.id == row_id)
                    .map(|row| row.kind.clone());
                let done = match kind {
                    Some(super::sidebar::RowKind::List(id)) => {
                        self.write(|store| store.core.rename_list(id, name.clone(), now_ms()))
                    }
                    Some(super::sidebar::RowKind::Folder(id)) => {
                        self.write(|store| store.core.rename_folder(id, name.clone(), now_ms()))
                    }
                    _ => None,
                };
                if done.is_some() {
                    self.reload_sidebar();
                }
            }
            other => self.editing = other,
        }
    }

    fn archive_current_list(&mut self, archived: bool) {
        let Scope::List {
            list_id,
            entered: None,
        } = self.scope.clone()
        else {
            return;
        };
        if self.inbox().is_some_and(|inbox| inbox.id == list_id) {
            self.message = Some("The inbox can't be archived.".into());
            return;
        }
        if self
            .write(|store| {
                store
                    .core
                    .set_list_archived(list_id.clone(), archived, now_ms())
            })
            .is_some()
        {
            self.reload_sidebar();
            self.select_everything();
        }
    }

    fn move_current_list(&mut self, by: i32) {
        let Scope::List { list_id, .. } = self.scope.clone() else {
            return;
        };
        if self
            .write(|store| {
                store
                    .core
                    .move_list_within_folder(list_id.clone(), by, now_ms())
            })
            .is_some()
        {
            self.reload_sidebar();
        }
    }

    fn move_current_folder(&mut self, by: i32) {
        let Scope::Folder(id) = self.scope.clone() else {
            return;
        };
        if self
            .write(|store| {
                store
                    .core
                    .move_folder_within_siblings(id.clone(), by, now_ms())
            })
            .is_some()
        {
            self.reload_sidebar();
        }
    }

    /// Whether a command can run from the palette now (motions cannot: they
    /// move from where the cursor is).
    pub fn palette_can_run(&self, id: &str) -> bool {
        self.bindings
            .entry(id)
            .is_some_and(|entry| !entry.motion && entry.surface != CommandSurface::Timeline)
    }
}

/// Where the cursor lands when a title opens for editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditCursor {
    #[default]
    SelectAll,
    Start,
    End,
}

/// Local midnight, `days` from today, in milliseconds.
pub fn start_of_local_day(days: i64) -> i64 {
    use chrono::{Duration, Local, TimeZone};
    let date = Local::now().date_naive() + Duration::days(days);
    let midnight = date.and_hms_opt(0, 0, 0).unwrap_or_default();
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map(|time| time.timestamp_millis())
        .unwrap_or_else(now_ms)
}

/// Every catalogue command the Rust app runs; the rest report themselves.
pub const PORTED: &[&str] = &[
    "goToday",
    "goBoard",
    "goOutline",
    "goMatrix",
    "goEverything",
    "goInbox",
    "goPreviousList",
    "goNextList",
    "goBack",
    "goForward",
    "goSidebarRegion",
    "goTaskRegion",
    "goInspectorRegion",
    "goCycleRegion",
    "goCommandPalette",
    "goListNavigator",
    "goSearch",
    "goKeyboardReference",
    "taskNew",
    "taskNewAbove",
    "taskNewChild",
    "taskComplete",
    "taskInvalidate",
    "taskDelete",
    "taskRename",
    "taskRenameAppend",
    "taskRenameInsert",
    "taskDueToday",
    "taskDueTomorrow",
    "taskClearDue",
    "taskClearNotes",
    "taskClearTags",
    "taskClearPriority",
    "taskSendToBack",
    "taskClearStart",
    "taskTogglePlannedToday",
    "taskToggleInspector",
    "taskGoToList",
    "taskIndent",
    "taskOutdent",
    "taskMoveUp",
    "taskMoveDown",
    "taskMoveToPreviousList",
    "taskMoveToNextList",
    "planEnterTask",
    "planLeaveTask",
    "planHideCompleted",
    "planToggleFold",
    "planBoardMoveCardLeft",
    "planBoardMoveCardRight",
    "planMatrixPlace",
    "motionBoardColumn",
    "planFoldAll",
    "planUnfoldAll",
    "listNew",
    "folderNew",
    "listRename",
    "listArchive",
    "folderCollapseAll",
    "folderExpandAll",
    "listMoveUp",
    "listMoveDown",
    "folderMoveUp",
    "folderMoveDown",
    "windowUndo",
    "windowRedo",
    "windowToggleSidebar",
    "windowToggleInspectorPane",
    "windowToggleRightDock",
    "windowCloseAllDocks",
    "motionSelectNext",
    "motionSelectPrevious",
    "motionSelectEnds",
    "motionSelectPage",
    "motionSidebarSelect",
    "motionSidebarExpand",
    "motionSidebarCollapse",
    "motionOutlineUnfold",
    "motionTodayUnfold",
    "motionOutlineFold",
    "motionTodayLeave",
    "motionFindForward",
    "motionFindBackward",
    "motionFindRepeat",
    "motionDismiss",
    "doneClose",
    "doneReopen",
    "focusFinish",
    "focusStop",
    "folderSelectNext",
    "folderSelectPrevious",
    "goFocus",
    "listComplete",
    "listDelete",
    "listIcon",
    "listNewTaskDestination",
    "listRestore",
    "motionGoToNumber",
    "motionInspectorClear",
    "motionInspectorEdit",
    "motionInspectorEnds",
    "motionInspectorLeave",
    "motionInspectorNext",
    "motionInspectorPrevious",
    "motionSetPriority",
    "planGrowTask",
    "planHideLater",
    "planShrinkTask",
    "taskAddBlocker",
    "taskAddLink",
    "taskBury",
    "taskConvertToList",
    "taskEditDue",
    "taskEditEstimate",
    "taskEditNotes",
    "taskEditRecurrence",
    "taskEditStart",
    "taskEditTags",
    "taskExtractBranch",
    "taskMove",
    "taskOpenLink",
    "taskPromoteList",
    "taskShowProgress",
    "taskStartFocus",
    "taskWaiting",
    "windowToggleDoneRail",
    "filterListEdit",
    "filterListNew",
    "planBoardAddColumnLeft",
    "planBoardAddColumnRight",
    "planBoardEditColumnRules",
    "planBoardMakeDefaultColumns",
    "planBoardMoveColumnLeft",
    "planBoardMoveColumnRight",
    "planBoardNewColumn",
    "planBoardRemoveColumn",
    "planBoardRenameColumn",
    "planBoardResetColumns",
    "conditionsAdjust",
    "conditionsArchive",
    "conditionsCategory",
    "conditionsClear",
    "conditionsClose",
    "conditionsEdit",
    "conditionsFold",
    "conditionsFoldAll",
    "conditionsMerge",
    "conditionsMoveCategoryDown",
    "conditionsMoveCategoryUp",
    "conditionsMoveDown",
    "conditionsMoveUp",
    "conditionsNew",
    "conditionsProvides",
    "conditionsRenameCategory",
    "conditionsShowTasks",
    "conditionsToggle",
    "conditionsTypeTime",
    "conditionsUseAgain",
    "doneReveal",
    "focusManageConditions",
    "focusSetAvailable",
    "focusSetPlace",
    "goTimeline",
    "motionConditionsSelect",
    "motionDoneSelect",
    "motionTimelineSelect",
    "taskConditions",
    "timelineClose",
    "timelineNextDay",
    "timelinePreviousDay",
    "timelineReveal",
    "timelineToday",
    "windowDockNextTab",
    "windowDockPreviousTab",
    "windowToggleProgressDock",
    "listSettings",
    "taskHabit",
    "taskToggleDaily",
    "todayStart",
    "windowExportTheme",
    "windowOpenKeymap",
    "windowOpenThemesFolder",
    "windowReloadKeymap",
    "windowReloadThemes",
    "windowShowDiagnostics",
    "taskAskAgent",
    "windowToggleAgentPanel",
    "windowUseClaudeCode",
    "windowUseCodex",
];

#[cfg(test)]
mod tests {
    use super::PORTED;

    #[test]
    fn every_ported_command_is_in_the_catalogue() {
        let catalogue: Vec<String> = takt_core::keymap::workspace_command_catalog()
            .into_iter()
            .map(|entry| entry.id)
            .collect();
        for id in PORTED {
            assert!(
                catalogue.iter().any(|known| known == id),
                "{id} is not a catalogue id"
            );
        }
    }
}
