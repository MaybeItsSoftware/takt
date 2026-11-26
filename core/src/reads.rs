//! The handle's reads of whole rows, beyond tasks and lists: dailies,
//! conditions, metadata, focus, work, awards, themes and preferences
//! (`rows.rs`). Kept apart from `workspace.rs`, whose methods write.

use crate::CoreError;
use crate::planning::{self, TaskPlanningEntry};
use crate::records::TaskRow;
use crate::rows::{
    self, AwardRow, CompletionContext, ConditionRow, ContributionRow, DailyRow, DayDaily,
    LoggedWork, MetadataRow, PointsSummary, PreferenceRow, QueueEntry, SessionRow, ThemeRow,
    WorkBlockRow,
};
use crate::workspace::CoreWorkspace;

#[uniffi::export]
impl CoreWorkspace {
    /// The live dailies in order.
    pub fn all_dailies(&self) -> Result<Vec<DailyRow>, CoreError> {
        rows::all_dailies(&self.lock())
    }

    /// One daily by id, archived or not.
    pub fn daily(&self, id: String) -> Result<Option<DailyRow>, CoreError> {
        rows::daily(&self.lock(), &id)
    }

    /// A task's live daily, if it has one.
    pub fn daily_for_task(&self, task_id: String) -> Result<Option<DailyRow>, CoreError> {
        rows::daily_for_task(&self.lock(), &task_id)
    }

    /// One contribution by id.
    pub fn contribution(&self, id: String) -> Result<Option<ContributionRow>, CoreError> {
        rows::contribution(&self.lock(), &id)
    }

    /// A daily's contributions on the given days, oldest first.
    pub fn contributions(
        &self,
        daily_id: String,
        day_keys: Vec<String>,
    ) -> Result<Vec<ContributionRow>, CoreError> {
        rows::contributions(&self.lock(), &daily_id, &day_keys)
    }

    /// The dailies the local day `day_ms` falls on shows, with their tasks and
    /// that day's contributions.
    pub fn dailies_on(&self, day_ms: i64, zone: String) -> Result<Vec<DayDaily>, CoreError> {
        rows::dailies_on(&self.lock(), day_ms, &zone)
    }

    /// Where a completion now falls in its day and in a run of days.
    pub fn completion_context(
        &self,
        now_ms: i64,
        zone: String,
    ) -> Result<CompletionContext, CoreError> {
        rows::completion_context(&self.lock(), now_ms, &zone)
    }

    /// Whether any task is pinned in Today's order.
    pub fn has_manual_focus_order(&self) -> Result<bool, CoreError> {
        rows::has_manual_focus_order(&self.read())
    }

    /// A workspace's conditions, oldest first.
    pub fn conditions(&self, workspace_id: String) -> Result<Vec<ConditionRow>, CoreError> {
        rows::conditions(&self.read(), &workspace_id)
    }

    /// The conditions an open task in the workspace requires, by id, sorted.
    pub fn required_condition_ids(&self, workspace_id: String) -> Result<Vec<String>, CoreError> {
        crate::conditions::required_condition_ids(&self.read(), &workspace_id)
    }

    /// The lists a task inherits requirements from, outermost first.
    pub fn inherited_requirements(
        &self,
        task_id: String,
    ) -> Result<Vec<crate::conditions::InheritedRequirement>, CoreError> {
        crate::conditions::inherited_requirements(&self.read(), &task_id)
    }

    /// What a list, standalone or nested, requires of its tasks.
    pub fn list_requirement_groups(&self, list_id: String) -> Result<Vec<Vec<String>>, CoreError> {
        crate::conditions::list_requirement_groups(&self.read(), &list_id)
    }

    /// How many open tasks require each condition, for those any does.
    pub fn condition_usage(
        &self,
        workspace_id: String,
    ) -> Result<Vec<crate::conditions::ConditionUsage>, CoreError> {
        crate::conditions::condition_usage(&self.read(), &workspace_id)
    }

    /// The open tasks that require a condition.
    pub fn tasks_requiring_condition(
        &self,
        workspace_id: String,
        condition_id: String,
    ) -> Result<Vec<crate::conditions::RequiringTask>, CoreError> {
        crate::conditions::tasks_requiring(&self.read(), &workspace_id, &condition_id)
    }

    /// What holds now, as last saved, or none if it never was.
    pub fn focus_context(
        &self,
        workspace_id: String,
    ) -> Result<Option<crate::focus_context::StoredFocusContext>, CoreError> {
        crate::focus_context::load(&self.read(), &workspace_id)
    }

    /// One condition by id.
    pub fn condition(&self, id: String) -> Result<Option<ConditionRow>, CoreError> {
        rows::condition(&self.lock(), &id)
    }

    /// A task's metadata row, if it has one.
    pub fn metadata(&self, task_id: String) -> Result<Option<MetadataRow>, CoreError> {
        rows::metadata(&self.lock(), &task_id)
    }

    /// The metadata rows of the given tasks.
    pub fn metadata_for_tasks(&self, task_ids: Vec<String>) -> Result<Vec<MetadataRow>, CoreError> {
        rows::metadata_for_tasks(&self.lock(), &task_ids)
    }

    /// Every metadata row.
    pub fn all_metadata(&self) -> Result<Vec<MetadataRow>, CoreError> {
        rows::all_metadata(&self.read())
    }

    /// The metadata rows of waiting tasks and their follow-ups.
    pub fn waiting_metadata(&self) -> Result<Vec<MetadataRow>, CoreError> {
        rows::waiting_metadata(&self.read())
    }

    /// The metadata rows that carry planning or a start time.
    pub fn planning_metadata(&self) -> Result<Vec<MetadataRow>, CoreError> {
        rows::planning_metadata(&self.read())
    }

    /// Every task's planning, decoded and normalised in the core, so only
    /// the tasks that have one cross, already parsed.
    pub fn task_planning_values(&self) -> Result<Vec<TaskPlanningEntry>, CoreError> {
        planning::planning_values(&self.read())
    }

    /// The newest session that has not finished.
    pub fn active_focus_session(&self) -> Result<Option<SessionRow>, CoreError> {
        rows::active_session(&self.lock())
    }

    /// One session by id.
    pub fn focus_session(&self, id: String) -> Result<Option<SessionRow>, CoreError> {
        rows::session(&self.lock(), &id)
    }

    /// A session's queue in order, with each task.
    pub fn focus_queue(&self, session_id: String) -> Result<Vec<QueueEntry>, CoreError> {
        rows::queue(&self.lock(), &session_id)
    }

    /// Seconds logged per task.
    pub fn logged_work(&self) -> Result<Vec<LoggedWork>, CoreError> {
        rows::logged_work(&self.read())
    }

    /// A task's work blocks, oldest first.
    pub fn work_blocks_for_task(&self, task_id: String) -> Result<Vec<WorkBlockRow>, CoreError> {
        rows::work_blocks_for_task(&self.lock(), &task_id)
    }

    /// Work blocks recorded in `[from, to)`.
    pub fn work_blocks_between(
        &self,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<WorkBlockRow>, CoreError> {
        rows::work_blocks_between(&self.read(), from_ms, to_ms)
    }

    /// One award by id.
    pub fn focus_award(&self, id: String) -> Result<Option<AwardRow>, CoreError> {
        rows::award(&self.lock(), &id)
    }

    /// The newest awards.
    pub fn recent_focus_awards(&self, limit: i64) -> Result<Vec<AwardRow>, CoreError> {
        rows::recent_awards(&self.lock(), limit)
    }

    /// Awards in `[from, to)`, newest first.
    pub fn focus_awards_between(
        &self,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<AwardRow>, CoreError> {
        rows::awards_between(&self.lock(), from_ms, to_ms)
    }

    /// Points today, over the last seven days and ever.
    pub fn focus_points_summary(
        &self,
        today_ms: i64,
        tomorrow_ms: i64,
        week_start_ms: i64,
    ) -> Result<PointsSummary, CoreError> {
        rows::points_summary(&self.lock(), today_ms, tomorrow_ms, week_start_ms)
    }

    /// When tasks were completed in `[from, to)`.
    pub fn task_completions_between(
        &self,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<i64>, CoreError> {
        rows::completions_between(&self.read(), from_ms, to_ms)
    }

    /// When tasks were created in `[from, to)`.
    pub fn task_creations_between(&self, from_ms: i64, to_ms: i64) -> Result<Vec<i64>, CoreError> {
        rows::creations_between(&self.lock(), from_ms, to_ms)
    }

    /// Tasks completed since `since`, newest first.
    pub fn completed_tasks_since(
        &self,
        since_ms: i64,
        limit: i64,
    ) -> Result<Vec<TaskRow>, CoreError> {
        rows::completed_since(&self.lock(), since_ms, limit)
    }

    /// Tasks closed in `[from, to)`, oldest first.
    pub fn tasks_closed_between(
        &self,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<TaskRow>, CoreError> {
        rows::closed_between(&self.lock(), from_ms, to_ms)
    }

    /// Every saved board's columns.
    pub fn kanban_boards(&self) -> Result<Vec<crate::imports::BoardBaseline>, CoreError> {
        rows::kanban_boards(&self.lock())
    }

    /// Open and closed task counts.
    pub fn task_counts(&self) -> Result<rows::TaskCounts, CoreError> {
        rows::task_counts(&self.lock())
    }

    /// The stored themes by id.
    pub fn themes(&self) -> Result<Vec<ThemeRow>, CoreError> {
        rows::themes(&self.lock())
    }

    /// Every preference.
    pub fn preferences(&self) -> Result<Vec<PreferenceRow>, CoreError> {
        rows::preferences(&self.lock())
    }
}
