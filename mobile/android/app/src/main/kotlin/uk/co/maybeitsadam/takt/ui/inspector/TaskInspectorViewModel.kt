package uk.co.maybeitsadam.takt.ui.inspector

import androidx.compose.runtime.Immutable
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import java.time.ZoneId
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.app.AppContainer
import uk.co.maybeitsadam.takt.ui.components.Format
import uk.co.maybeitsadam.takt.core.MatrixQuadrant
import uk.co.maybeitsadam.takt.core.TaskCondition
import uk.co.maybeitsadam.takt.core.TaskPlanningException
import uk.co.maybeitsadam.takt.core.WorkspaceKanbanColumn
import uk.co.maybeitsadam.takt.core.WorkspaceTask
import uk.co.maybeitsadam.takt.data.workspace.FieldEdit
import uk.co.maybeitsadam.takt.data.workspace.TaskDependencies
import uk.co.maybeitsadam.takt.data.workspace.TaskSearchResult
import uk.co.maybeitsadam.takt.data.workspace.assignTask
import uk.co.maybeitsadam.takt.data.workspace.taskSharing
import uk.co.maybeitsadam.takt.data.workspace.addBlocker
import uk.co.maybeitsadam.takt.data.workspace.observeTaskDependencies
import uk.co.maybeitsadam.takt.data.workspace.removeBlocker
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorDraft
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorError
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorException
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorField
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorSnapshot
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorValues
import uk.co.maybeitsadam.takt.data.workspace.TaskInspectorFacts
import uk.co.maybeitsadam.takt.data.workspace.TaskRecurrenceClash
import uk.co.maybeitsadam.takt.data.workspace.WorkspaceStoreException
import uk.co.maybeitsadam.takt.data.workspace.observeTaskInspectorFacts
import uk.co.maybeitsadam.takt.data.workspace.recurrenceClash
import uk.co.maybeitsadam.takt.data.workspace.setWaiting
import uniffi.takt_core.InheritedRequirement

/** Everything around the draft: the task row, its list, the catalogues, its placement. */
@Immutable
data class InspectorSurroundings(
    val task: WorkspaceTask? = null,
    val listName: String = "",
    val conditions: ImmutableList<TaskCondition> = persistentListOf(),
    val boardColumns: ImmutableList<WorkspaceKanbanColumn> = WorkspaceKanbanColumn.blitzitDefaults.toImmutableList(),
    val facts: TaskInspectorFacts = TaskInspectorFacts(),
    /** What it waits on and what waits on it. */
    val dependencies: TaskDependencies = TaskDependencies(),
    /**
     * The lists it inherits requirements from, outermost first, with what
     * each requires: shown with its own, but edited on the list.
     */
    val inherited: ImmutableList<InheritedRequirement> = persistentListOf(),
) {
    fun conditionName(id: String): String = conditions.firstOrNull { it.id == id }?.name ?: "Missing condition"
}

/** The save's progress and the reason it last failed, shown in the unsaved bar. */
@Immutable
data class SaveState(val saving: Boolean = false, val error: String? = null, val attempted: Boolean = false)

/**
 * The inspector's state machine. One per activity (the sheet and the pane are
 * both outside the nav graph), re-pointed at a task with [bind].
 *
 * Edits go into a [TaskEditorDraft] and only reach the store on [save]. Each
 * snapshot the store emits is folded in with `reconciled`, so a sync or an
 * edit elsewhere updates fields left alone here and turns a field edited on
 * both sides into a conflict. Placement (Today, matrix, board column) and the
 * daily schedule are not in the draft and are written straight away.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class TaskInspectorViewModel(private val container: AppContainer) : ViewModel() {
    private val zone: ZoneId get() = ZoneId.systemDefault()
    private val taskId = MutableStateFlow<String?>(null)
    private val _draft = MutableStateFlow<TaskEditorDraft?>(null)
    private val _save = MutableStateFlow(SaveState())

    val draft: StateFlow<TaskEditorDraft?> = _draft.asStateFlow()
    val saveState: StateFlow<SaveState> = _save.asStateFlow()

    val surroundings: StateFlow<InspectorSurroundings> = taskId.flatMapLatest { id ->
        if (id == null) {
            flowOf(InspectorSurroundings())
        } else {
            container.withSession { session ->
                val repo = session.repository
                val listName = combine(
                    repo.observeTask(id),
                    repo.observeLists(session.workspace.id, includingArchived = true),
                    repo.observeInheritedRequirements(id),
                ) { task, lists, inherited ->
                    Triple(task, lists.firstOrNull { it.id == task?.listId }?.name ?: "", inherited)
                }
                // The board the task is a card on, as the core resolves it: its own
                // layout, else its list's, the default or the stock five. Rule
                // columns are left out, since nothing is filed in one.
                val boardColumns = listName.map { (task, _) -> task?.let { "${it.listId}/${it.parentTaskId ?: "root"}" } }
                    .distinctUntilChanged()
                    .flatMapLatest { key ->
                        if (key == null) flowOf(WorkspaceKanbanColumn.blitzitDefaults)
                        else repo.observeBoardColumns(key, emptyList()).map { read -> read.layout.filter { !it.isRuleColumn } }
                    }
                combine(
                    listName,
                    repo.observeConditions(session.workspace.id),
                    boardColumns,
                    repo.observeTaskInspectorFacts(id),
                    repo.observeTaskDependencies(id),
                ) { (task, name, inherited), conditions, filedColumns, facts, dependencies ->
                    val columns = filedColumns.ifEmpty { WorkspaceKanbanColumn.blitzitDefaults }
                    InspectorSurroundings(
                        task = task,
                        listName = name,
                        conditions = conditions.toImmutableList(),
                        boardColumns = columns.toImmutableList(),
                        facts = facts,
                        dependencies = dependencies,
                        inherited = inherited.toImmutableList(),
                    )
                }
            }
        }
    }.flowOn(Dispatchers.Default)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), InspectorSurroundings())

    init {
        viewModelScope.launch {
            taskId.flatMapLatest { id ->
                if (id == null) emptyFlow() else container.withSession { it.repository.observeTaskEditorSnapshot(id) }.map { id to it }
            }.collect { (id, saved) -> fold(id, saved) }
        }
    }

    private fun fold(id: String, saved: TaskEditorSnapshot?) {
        _draft.update { current ->
            when {
                saved == null -> current?.takeIf { it.baseline.taskId == id }?.copy(isUnavailable = true)
                current == null || current.baseline.taskId != id -> TaskEditorDraft(saved)
                else -> current.reconciled(saved)
            }
        }
    }

    /** Points the inspector at [id]. A draft left on another task is saved first if it can be. */
    fun bind(id: String) {
        if (taskId.value == id) return
        val outgoing = _draft.value
        if (outgoing != null && outgoing.baseline.taskId != id) {
            // A released inspector already flushed (and reported) its draft.
            if (taskId.value != null) flush(outgoing)
            _draft.value = null
            _save.value = SaveState()
        }
        taskId.value = id
    }

    /** The inspector went away (sheet swiped down, task deselected): save what can be saved. */
    fun release() {
        _draft.value?.let(::flush)
        taskId.value = null
    }

    private fun flush(draft: TaskEditorDraft) {
        if (!draft.isDirty || draft.isUnavailable) return
        val problems = InspectorValidation.problems(draft, zone)
        if (draft.conflicts.isNotEmpty() || problems.isNotEmpty()) {
            container.undo.say("Edits to “${draft.baseline.title}” were not saved: ${problems.firstOrNull()?.message ?: TaskEditorError.CONFLICTING_CHANGES.message}")
            return
        }
        // The app's scope: asked as the inspector goes, the answer outlives it.
        container.scope.launch {
            // A move onto a repeating task's next turn asks first; Cancel keeps the draft to reopen.
            val choice = container.clashes.choose(draft.values.title, clashOf(draft))
            if (!choice.saves) return@launch
            val saving = container.undo.performNow { repo ->
                container.clashes.keepingBoth(choice, draft.baseline.taskId, repo) { it.saveTaskEditor(draft) }
            }
            if (!saving) return@launch
            // Reopening the task now starts from the saved values.
            _draft.update { current -> if (current === draft) null else current }
        }
    }

    // region Editing the draft

    fun edit(change: (TaskEditorValues) -> TaskEditorValues) {
        _draft.update { draft -> draft?.copy(values = change(draft.values)) }
        if (_save.value.error != null) _save.update { it.copy(error = null) }
    }

    fun resolve(field: TaskEditorField, useSaved: Boolean) {
        _draft.update { it?.resolved(field, useSaved) }
    }

    fun discard() {
        _draft.update { draft -> draft?.let { TaskEditorDraft(it.baseline, isUnavailable = it.isUnavailable) } }
        _save.value = SaveState()
    }

    /** Saves the draft as one "Edit Task" step. Returns whether it is clean afterwards. */
    suspend fun save(): Boolean {
        val draft = _draft.value ?: return true
        if (!draft.isDirty) return true
        val problems = InspectorValidation.problems(draft, zone)
        val blocker = when {
            draft.isUnavailable -> "This task was deleted elsewhere. Your edits can't be saved."
            draft.conflicts.isNotEmpty() -> TaskEditorError.CONFLICTING_CHANGES.message
            problems.isNotEmpty() -> problems.first().message
            else -> null
        }
        if (blocker != null) {
            _save.value = SaveState(error = blocker, attempted = true)
            return false
        }
        // A move onto a repeating task's next turn asks first. Cancel saves
        // nothing and leaves the draft as it is, to change.
        val choice = container.clashes.choose(draft.values.title, clashOf(draft))
        if (!choice.saves) return false
        _save.value = SaveState(saving = true, attempted = true)
        var saved: TaskEditorSnapshot? = null
        var failure: Throwable? = null
        container.undo.performNow { repo ->
            container.clashes.keepingBoth(choice, draft.baseline.taskId, repo) {
                try {
                    saved = it.saveTaskEditor(draft)
                } catch (cancelled: CancellationException) {
                    throw cancelled
                } catch (error: Exception) {
                    failure = error
                    throw error
                }
            }
        }
        val committed = saved
        if (committed == null) {
            _save.value = SaveState(error = failure?.let(::message) ?: "Couldn't save.", attempted = true)
            return false
        }
        _draft.update { current ->
            when {
                current == null || current.baseline.taskId != committed.taskId -> current
                current.values == draft.values -> TaskEditorDraft(committed)
                // Typed while saving: keep the newer values on top of what was saved.
                else -> TaskEditorDraft(committed, values = current.values)
            }
        }
        _save.value = SaveState()
        return true
    }

    /** Whether saving [draft] lands a repeating task on its next turn; the core decides. */
    private suspend fun clashOf(draft: TaskEditorDraft): TaskRecurrenceClash? =
        runCatching { container.repository().recurrenceClash(draft) }.getOrNull()

    fun saveAsync(then: (Boolean) -> Unit = {}) {
        viewModelScope.launch { then(save()) }
    }

    private fun message(error: Throwable): String = when (error) {
        is TaskEditorException -> error.error.message
        is TaskPlanningException -> error.error.message
        is WorkspaceStoreException -> error.text
        else -> error.message ?: "Couldn't save."
    }

    // endregion

    // region Written straight away

    fun setMatrix(quadrant: MatrixQuadrant?) {
        val id = taskId.value ?: return
        container.undo.perform { it.setMatrixPosition(MatrixPicker.position(quadrant), id) }
    }

    fun setColumn(column: String?) {
        val id = taskId.value ?: return
        container.undo.perform { it.setKanbanColumn(column, id) }
    }

    /**
     * Files the task in Waiting on with [tag] and [followUpAt] (either may be null), following up again
     * every [everyDays] days after each follow-up is done (null for once), as one undo step.
     */
    fun setWaiting(tag: String?, followUpAt: java.time.Instant?, everyDays: Int?) {
        val id = taskId.value ?: return
        container.undo.perform { it.setWaiting(id, tag, followUpAt, everyDays) }
    }

    /** Makes the task wait on [blockerId], as one "Add Blocker" step; a cycle is refused with a message. */
    fun addBlocker(blockerId: String) {
        val id = taskId.value ?: return
        container.undo.perform { it.addBlocker(id, blockerId) }
    }

    /** Stops the task waiting on [blockerId], as one "Remove Blocker" step. */
    /** Who added, finished and holds the task, when it is in a shared list. */
    suspend fun sharing(id: String): uniffi.takt_core.TaskSharing? =
        container.repository().taskSharing(id)

    /** Assigns the task to a member of its shared list, or to nobody. */
    fun assign(accountId: String?, then: () -> Unit = {}) {
        val id = taskId.value ?: return
        container.undo.perform {
            it.assignTask(id, accountId)
            then()
        }
    }

    fun removeBlocker(blockerId: String) {
        val id = taskId.value ?: return
        container.undo.perform { it.removeBlocker(id, blockerId) }
    }

    /** Open tasks matching [query] the task could wait on, for the "Blocked by" picker. */
    suspend fun blockerCandidates(query: String): List<TaskSearchResult> {
        val id = taskId.value ?: return emptyList()
        if (query.isBlank()) return emptyList()
        val session = container.awaitSession()
        val results = try {
            session.repository.searchTasks(session.workspace.id, query.trim())
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            emptyList()
        }
        return BlockerPicker.candidates(results, id, surroundings.value.dependencies)
    }

    fun setPlannedToday(planned: Boolean) {
        val id = taskId.value ?: return
        container.undo.perform { it.setPlannedForToday(planned, listOf(id)) }
    }

    fun setDailyWeekdays(dailyId: String, weekdays: Set<Int>) {
        if (weekdays.isEmpty()) return
        container.undo.perform { it.updateDaily(dailyId, weekdays = weekdays, intervalDays = FieldEdit.To(null)) }
    }

    fun setDailyInterval(dailyId: String, days: Int?) {
        container.undo.perform { it.updateDaily(dailyId, intervalDays = FieldEdit.To(days?.coerceIn(1, 366))) }
    }

    /** Records time worked on the task without a focus session or finishing it. */
    fun logWork(seconds: Int) {
        val id = taskId.value ?: return
        container.undo.perform(message = "Logged ${Format.duration(seconds)}") { it.logTaskWork(id, seconds) }
    }

    fun setDailyTarget(dailyId: String, seconds: Int?) {
        container.undo.perform { it.updateDaily(dailyId, targetSeconds = FieldEdit.To(seconds?.takeIf { it > 0 })) }
    }

    /** Saves first if needed, since the store copies the saved planning, not the draft. */
    fun applyPlanningToSubtasks() {
        val id = taskId.value ?: return
        viewModelScope.launch {
            if (!save()) return@launch
            container.undo.performNow { it.applyPlanningToDescendants(id) }
        }
    }

    // endregion
}
