package uk.co.maybeitsadam.takt.ui.lists

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.app.AppContainer
import uk.co.maybeitsadam.takt.data.workspace.observeSharedListIds
import uk.co.maybeitsadam.takt.data.workspace.observeSidebar

/** The Lists tree: folders, lists and nested lists, and every edit to them. */
class ListsViewModel(private val container: AppContainer) : ViewModel() {
    private val undo get() = container.undo
    private val showArchived = MutableStateFlow(false)
    private val collapsed = container.settings.stringSet(COLLAPSED_FOLDERS)

    val tree: StateFlow<ListsTreeState> = combine(
        container.withSession { it.repository.observeSidebar(it.workspace.id) },
        collapsed,
        showArchived,
    ) { data, collapsedIds, archived ->
        ListsTreeShaping.shape(data, collapsedIds, archived)
    }.flowOn(Dispatchers.Default)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ListsTreeState.Empty)

    /** The lists shared with other accounts, for their icon and menu. */
    val sharedIds: StateFlow<Set<String>> = container.withSession { it.repository.observeSharedListIds() }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptySet())

    fun toggleFolder(id: String) {
        container.scope.launch {
            val current = container.settings.stringSet(COLLAPSED_FOLDERS).first()
            container.settings.putStringSet(COLLAPSED_FOLDERS, if (id in current) current - id else current + id)
        }
    }

    fun toggleArchived() {
        showArchived.value = !showArchived.value
    }

    fun createList(name: String, folderId: String?) = undo.perform { repo ->
        repo.createList(container.awaitSession().workspace.id, name, folderId)
    }

    fun createFolder(name: String, parentId: String?) = undo.perform { repo ->
        repo.createFolder(container.awaitSession().workspace.id, name, parentId)
    }

    fun renameList(id: String, name: String) = undo.perform { it.renameList(id, name) }

    fun renameFolder(id: String, name: String) = undo.perform { it.updateFolder(id, name) }

    fun setArchived(id: String, archived: Boolean) = undo.perform { it.setListArchived(archived, id) }

    fun setCompleted(id: String, completed: Boolean) = undo.perform { it.setListCompleted(completed, id) }

    fun deleteList(id: String) = undo.perform { it.deleteList(id) }

    fun deleteFolder(id: String) = undo.perform { it.deleteFolder(id) }

    /** Files a list at the end of [folderId] (null: the top level). */
    fun moveListToFolder(id: String, folderId: String?) = undo.perform { it.placeList(id, null, folderId) }

    fun moveListBy(id: String, by: Int) = undo.perform { it.moveListWithinFolder(id, by) }

    fun moveFolderTo(id: String, parentId: String?) = undo.perform { it.placeFolder(id, null, parentId) }

    fun moveFolderBy(id: String, by: Int) = undo.perform { it.moveFolderWithinSiblings(id, by) }

    fun convertListToTask(id: String) = undo.perform { it.convertListToTask(id) }

    fun setNestedArchived(taskId: String, archived: Boolean) = undo.perform { it.setNestedListArchived(archived, taskId) }

    fun setNestedPinned(taskId: String, pinned: Boolean) = undo.perform { it.setNestedListPromoted(pinned, taskId) }

    companion object {
        const val COLLAPSED_FOLDERS = "lists.collapsedFolders"
    }
}
