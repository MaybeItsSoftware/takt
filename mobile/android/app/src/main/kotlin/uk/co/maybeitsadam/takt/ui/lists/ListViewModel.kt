package uk.co.maybeitsadam.takt.ui.lists

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import java.time.LocalDate
import java.time.ZoneId
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.app.AppContainer
import uk.co.maybeitsadam.takt.app.CelebrationStyle
import uk.co.maybeitsadam.takt.app.FoldStore
import uk.co.maybeitsadam.takt.core.BoardColumnsRead
import uk.co.maybeitsadam.takt.core.TaskList
import uk.co.maybeitsadam.takt.core.TaskMatrixPosition
import uk.co.maybeitsadam.takt.core.TaskStatus
import uk.co.maybeitsadam.takt.core.WorkspaceKanbanColumn
import uk.co.maybeitsadam.takt.core.WorkspaceTask
import uk.co.maybeitsadam.takt.data.workspace.ListScopeData
import uk.co.maybeitsadam.takt.data.workspace.observeListScope

/** The three ways a list is drawn, as the Mac's Ctrl+2/3/4 switch them. */
enum class ListViewMode(val raw: String, val title: String) {
    OUTLINE("outline", "Outline"),
    BOARD("board", "Board"),
    MATRIX("matrix", "Matrix"),
    ;

    companion object {
        fun of(raw: String?): ListViewMode = entries.firstOrNull { it.raw == raw } ?: OUTLINE
    }

    /** The view a tap on the toggle moves to: outline, board, matrix, round again. */
    fun next(): ListViewMode = entries[(ordinal + 1) % entries.size]
}

/** A scope's data with its tasks indexed by id, built off the main thread. */
class ScopeSnapshot(val data: ListScopeData, val tasks: Map<String, WorkspaceTask>)

/**
 * One list scope (a list, or Everything when [listId] is null): its outline,
 * board and matrix, all shaped off the main thread from one observed read,
 * and every edit made there.
 */
class ListViewModel(
    private val container: AppContainer,
    val listId: String?,
    revealTaskId: String?,
) : ViewModel() {
    val isEverything: Boolean get() = listId == null
    private val foldKey = listId ?: FoldStore.EVERYTHING
    private val boardKey = BoardShaping.boardKey(listId)
    private val viewKey = "listView.${listId ?: "everything"}"
    private val commands get() = container.commands
    private val undo get() = container.undo

    private val snapshot: StateFlow<ScopeSnapshot?> = container.withSession { session ->
        session.repository.observeListScope(session.workspace.id, listId)
    }.map { data ->
        val tasks = HashMap<String, WorkspaceTask>()
        for (tree in data.trees.values) for (task in tree.tasks) tasks[task.id] = task
        ScopeSnapshot(data, tasks)
    }.flowOn(Dispatchers.Default)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    private val folded: Flow<Set<String>> = container.folds.folded(foldKey)

    val hideCompleted: StateFlow<Boolean> = container.settings.string(HIDE_COMPLETED).map { it == "true" }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    val celebration: StateFlow<CelebrationStyle> = container.settings.celebrationStyle
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), CelebrationStyle.STRIKE)

    val viewMode: StateFlow<ListViewMode> = container.settings.string(viewKey).map { ListViewMode.of(it) }
        .stateIn(viewModelScope, SharingStarted.Eagerly, ListViewMode.OUTLINE)

    /** Rows the Fold celebration has folded away this visit. */
    private val foldedAway = MutableStateFlow<Set<String>>(emptySet())

    private val _selectedId = MutableStateFlow<String?>(null)
    val selectedId: StateFlow<String?> = _selectedId.asStateFlow()

    /** A row to scroll to once it is drawn; consumed by the outline. */
    private val _scrollTo = MutableStateFlow<String?>(null)
    val scrollTo: StateFlow<String?> = _scrollTo.asStateFlow()

    val list: StateFlow<TaskList?> = snapshot.map { s -> if (listId == null) null else s?.data?.lists?.firstOrNull() }
        .distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** Every unarchived list, for the move-to-list picker. */
    val allLists: StateFlow<List<TaskList>> = snapshot.map { it?.data?.allLists.orEmpty() }.distinctUntilChanged()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    val outline: StateFlow<OutlineShape> = combine(snapshot.filterNotNull(), folded, hideCompleted, foldedAway) { s, f, h, away ->
        OutlineShaping.shape(s.data, isEverything, f, h, away, ZoneId.systemDefault(), LocalDate.now())
    }.flowOn(Dispatchers.Default)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), OutlineShape.Empty)

    /**
     * Each snapshot with the core's read of its board, taken after it, so a
     * card's rule-column placement never lags the card. A layout edit, which
     * the snapshot does not watch, reads the board again too.
     */
    @OptIn(ExperimentalCoroutinesApi::class)
    private val boardInputs: Flow<Pair<ScopeSnapshot, BoardColumnsRead>> = container.withSession { session ->
        combine(snapshot.filterNotNull(), session.repository.observeKanbanBoards()) { s, _ -> s }
            .mapLatest { s ->
                val read = runCatching { session.repository.boardColumns(boardKey, s.data.lists.map { it.id }) }
                    .getOrElse { BoardColumnsRead.builtin(boardKey) }
                s to read
            }
    }

    val board: StateFlow<BoardShape> = combine(boardInputs, folded, hideCompleted) { (s, read), f, h ->
        BoardShaping.shape(s.data, isEverything, read, f, h, ZoneId.systemDefault(), LocalDate.now())
    }.flowOn(Dispatchers.Default)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), BoardShape.Empty)

    val matrix: StateFlow<MatrixShape> = board.map { BoardShaping.matrix(it) }.flowOn(Dispatchers.Default)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), MatrixShape.Empty)

    init {
        if (revealTaskId != null) reveal(revealTaskId)
    }

    fun task(id: String): WorkspaceTask? = snapshot.value?.tasks?.get(id)

    // region Selection and folding

    fun select(id: String?) {
        _selectedId.value = id
    }

    /** Selects the row [by] steps from the selection, over [rows]. */
    fun selectAdjacent(rows: List<OutlineRow>, by: Int) {
        if (rows.isEmpty()) return
        val index = rows.indexOfFirst { it.id == _selectedId.value }
        val next = if (index < 0) (if (by > 0) 0 else rows.size - 1) else (index + by).coerceIn(0, rows.size - 1)
        _selectedId.value = rows[next].id
        _scrollTo.value = rows[next].id
    }

    fun consumeScroll() {
        _scrollTo.value = null
    }

    /**
     * Unfolds [taskId]'s ancestors, shows completed tasks when it or one of
     * them is finished (as the iPhone's `revealInList` does), then selects it
     * and asks the outline to scroll to it.
     */
    fun reveal(taskId: String) {
        viewModelScope.launch {
            val s = snapshot.filterNotNull().first()
            val ancestors = OutlineShaping.ancestors(s.data, taskId)
            val folds = folded.first()
            if (folds.any { it in ancestors }) container.folds.set(foldKey, folds - ancestors)
            val hidden = (ancestors + taskId).any { id -> s.tasks[id]?.let { it.status != TaskStatus.OPEN } == true }
            if (hidden && container.settings.string(HIDE_COMPLETED).first() == "true") {
                container.settings.putString(HIDE_COMPLETED, "false")
            }
            _selectedId.value = taskId
            _scrollTo.value = taskId
        }
    }

    fun toggleFold(taskId: String) {
        container.scope.launch { container.folds.toggle(foldKey, taskId) }
    }

    fun foldAll() {
        val parents = outline.value.parentIds + board.value.columns.flatMap { c -> c.cards.filter { it.row.hasChildren }.map { it.id } }
        container.scope.launch { container.folds.set(foldKey, parents) }
    }

    fun unfoldAll() {
        container.scope.launch { container.folds.set(foldKey, emptySet()) }
    }

    fun setHideCompleted(hide: Boolean) {
        container.scope.launch { container.settings.putString(HIDE_COMPLETED, hide.toString()) }
    }

    fun setViewMode(mode: ListViewMode) {
        container.scope.launch { container.settings.putString(viewKey, mode.raw) }
    }

    // endregion

    // region Task commands

    /** Completes or reopens; with the Fold celebration a finished row then folds out of the outline. */
    fun toggleComplete(id: String) {
        val task = task(id) ?: return
        commands.toggleComplete(task)
        if (task.status == TaskStatus.OPEN) {
            if (celebration.value == CelebrationStyle.FOLD) {
                viewModelScope.launch {
                    // A habit is ticked, not finished: it stays in the outline.
                    if (container.repository().daily(id)?.isHabit == true) return@launch
                    delay(FOLD_DELAY_MILLIS)
                    foldedAway.value = foldedAway.value + id
                }
            }
        } else {
            foldedAway.value = foldedAway.value - id
        }
    }

    fun toggleInvalidate(id: String) {
        task(id)?.let { commands.toggleInvalidate(it) }
    }

    fun toggleList(id: String) {
        task(id)?.let { commands.toggleList(it) }
    }

    /**
     * Adds a task from the inline field: after [afterId] when given, else at
     * the end of the scope's top level (the Inbox's, in Everything). Returns
     * the new task's id so the next one goes after it.
     */
    suspend fun addTask(text: String, afterId: String?): String? {
        if (text.isBlank()) return null
        val session = container.awaitSession()
        val data = snapshot.value?.data
        val after = afterId?.let { task(it) }
        val targetList = after?.listId ?: listId ?: session.inboxId
        val parent = if (after != null) {
            after.parentTaskId
        } else {
            data?.let { d -> OutlineShaping.sections(d).firstOrNull { it.list.id == targetList }?.rootParentId }
        }
        var created: String? = null
        undo.performNow { repo ->
            created = repo.createTask(
                capturing = text.trim(), listId = targetList, parentTaskId = parent, adjacentTaskId = after?.id,
            ).id
        }
        return created
    }

    fun drop(rows: List<OutlineRow>, from: Int, to: Int) {
        val moved = rows.getOrNull(from) ?: return
        when (val drop = OutlineShaping.planDrop(rows, from, to)) {
            is OutlineDrop.Before -> undo.perform { it.moveTaskBefore(moved.id, drop.targetId) }
            OutlineDrop.ToEnd -> undo.perform { it.moveTaskWithinSiblings(moved.id, Int.MAX_VALUE / 2) }
            is OutlineDrop.Reparent -> {
                // A list's top level is its visible root's children.
                val parent = drop.parentId ?: snapshot.value?.data?.let { d ->
                    OutlineShaping.sections(d).firstOrNull { it.list.id == drop.listId }?.rootParentId
                }
                undo.perform { repo ->
                    repo.moveTask(moved.id, drop.listId, parent)
                    drop.beforeId?.let { repo.moveTaskBefore(moved.id, it) }
                }
            }
            null -> Unit
        }
    }

    // endregion

    // region Board and matrix

    /** Through the core, which edits a card dropped into a rule column to match, or refuses with its reason. */
    fun moveCard(cardId: String, columnId: String) {
        if (board.value.columnOf(cardId)?.id == columnId) return
        undo.perform { it.moveToBoardColumn(boardKey, listOf(cardId), columnId) }
    }

    fun moveCardBy(cardId: String, offset: Int) {
        val columns = board.value.columns
        val index = columns.indexOfFirst { c -> c.cards.any { it.id == cardId } }
        if (index < 0) return
        val destination = (index + offset).coerceIn(0, columns.size - 1)
        if (destination != index) moveCard(cardId, columns[destination].id)
    }

    fun addColumn(title: String) {
        val name = title.trim()
        // The layout the core read, so its rule columns are saved back with their rules.
        val shape = board.value
        if (name.isEmpty() || !shape.isLoaded) return
        val layout = shape.layout
        val taken = layout + shape.columns.map { WorkspaceKanbanColumn(it.id, it.title) }
        val updated = layout + WorkspaceKanbanColumn(BoardShaping.uniqueColumnId(name, taken), name)
        undo.perform { it.setKanbanBoardColumns(updated, boardKey, label = "Add Board Column") }
    }

    fun renameColumn(columnId: String, title: String) {
        val shape = board.value
        if (!shape.isLoaded) return
        val updated = BoardShaping.renamingColumn(shape.layout, columnId, title) ?: return
        undo.perform { it.setKanbanBoardColumns(updated, boardKey, label = "Rename Board Column") }
    }

    /** Moves a column [offset] places along the drawn board. */
    fun moveColumn(columnId: String, offset: Int) {
        val shape = board.value
        if (!shape.isLoaded) return
        val updated = BoardShaping.movingColumn(
            shape.layout, shape.columns.map { it.id }, columnId, offset, shape.columns.associate { it.id to it.title },
        ) ?: return
        undo.perform { it.setKanbanBoardColumns(updated, boardKey, label = "Move Board Column") }
    }

    /**
     * Removes a column, moving the cards filed in it into the first remaining
     * column that is filed into, in the same undo step. A column drawn only
     * because cards are filed in it is not in the layout: its cards move.
     */
    fun removeColumn(columnId: String) {
        val shape = board.value
        if (shape.columns.size <= 1) return
        val fallback = shape.layout.firstOrNull { it.id != columnId && !it.isRuleColumn }?.id
            ?: shape.columns.firstOrNull { it.id != columnId && !it.isRuleColumn }?.id
        val listIds = snapshot.value?.data?.lists?.map { it.id } ?: listOfNotNull(listId)
        if (shape.layout.any { it.id == columnId }) {
            undo.perform { it.deleteBoardColumn(boardKey, listIds, columnId, fallback) }
        } else {
            val moving = shape.columns.firstOrNull { it.id == columnId }?.cards?.map { it.id }.orEmpty()
            if (fallback != null && moving.isNotEmpty()) undo.perform { it.setKanbanColumn(fallback, moving) }
        }
    }

    fun place(cardId: String, cell: MatrixCell?) {
        val position = cell?.position ?: TaskMatrixPosition(null, null)
        undo.perform { it.setMatrixPosition(position, cardId) }
    }

    // endregion

    // region The list itself

    fun renameList(name: String) {
        val id = listId ?: return
        undo.perform { it.renameList(id, name) }
    }

    fun setArchived(archived: Boolean) {
        val id = listId ?: return
        undo.perform { it.setListArchived(archived, id) }
    }

    fun toggleCompleted() {
        val list = list.value ?: return
        undo.perform { it.setListCompleted(list.completedAt == null, list.id) }
    }

    fun deleteList() {
        val id = listId ?: return
        undo.perform { it.deleteList(id) }
    }

    // endregion

    companion object {
        const val HIDE_COMPLETED = "outlineHidesCompleted"
        /** How long a finished row stays before the Fold celebration folds it away. */
        internal const val FOLD_DELAY_MILLIS = 450L
    }
}
