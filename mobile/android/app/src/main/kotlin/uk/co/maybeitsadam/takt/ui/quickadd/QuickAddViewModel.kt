package uk.co.maybeitsadam.takt.ui.quickadd

import uk.co.maybeitsadam.takt.data.workspace.TaskEditorDraft
import uk.co.maybeitsadam.takt.core.NextUpSelector
import androidx.compose.runtime.Immutable
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import uk.co.maybeitsadam.takt.app.AppContainer
import uk.co.maybeitsadam.takt.core.TaskList

/** A list quick add can file into. */
@Immutable
data class QuickAddTarget(val id: String, val name: String, val isInbox: Boolean)

@Immutable
data class QuickAddTargets(
    val inboxId: String? = null,
    val lists: ImmutableList<QuickAddTarget> = persistentListOf(),
) {
    fun named(id: String?): QuickAddTarget? = lists.firstOrNull { it.id == id }
}

/** Where a capture can go: every live list, the Inbox first. */
class QuickAddViewModel(private val container: AppContainer) : ViewModel() {
    val targets: StateFlow<QuickAddTargets> = container.withSession { session ->
        session.repository.observeLists(session.workspace.id).map { lists -> targets(lists, session.inboxId) }
    }.flowOn(Dispatchers.Default)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), QuickAddTargets())

    /** Files [text] (its trailing tokens included) as one undo step. */
    fun add(text: String, listId: String, parentTaskId: String?, plansToday: Boolean = false, needs: List<List<String>>? = null) {
        container.undo.perform { repo ->
            val task = repo.createTask(
                capturing = text,
                listId = listId,
                parentTaskId = parentTaskId,
                kanbanColumn = if (plansToday) NextUpSelector.todayColumnID else null,
            )
            // The conditions it needs have no token: saved as the inspector
            // saves them, in the same undo step.
            if (!needs.isNullOrEmpty()) {
                val snapshot = repo.taskEditorSnapshot(task.id)
                repo.saveTaskEditor(TaskEditorDraft(snapshot, snapshot.values.copy(requirementGroups = needs)))
            }
        }
    }

    companion object {
        fun targets(lists: List<TaskList>, inboxId: String): QuickAddTargets {
            val live = lists.filter { !it.isArchived && it.completedAt == null }
            val ordered = live.filter { it.id == inboxId } + live.filter { it.id != inboxId }
            return QuickAddTargets(
                inboxId = inboxId,
                lists = ordered.map { QuickAddTarget(it.id, it.name, it.id == inboxId) }.toImmutableList(),
            )
        }
    }
}
