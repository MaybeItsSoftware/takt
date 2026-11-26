package uk.co.maybeitsadam.takt.data.workspace

import uniffi.takt_core.CoreException
import java.time.Clock
import java.time.Instant
import java.time.ZoneId
import kotlinx.coroutines.flow.Flow
import uk.co.maybeitsadam.takt.core.BoardColumnsRead
import uk.co.maybeitsadam.takt.core.DailyContribution
import uk.co.maybeitsadam.takt.core.DailyItem
import uk.co.maybeitsadam.takt.core.DayBoundary
import uk.co.maybeitsadam.takt.core.FocusAward
import uk.co.maybeitsadam.takt.core.FocusContext
import uk.co.maybeitsadam.takt.core.FocusPointsSummary
import uk.co.maybeitsadam.takt.core.FocusQueueItem
import uk.co.maybeitsadam.takt.core.FocusQueueState
import uk.co.maybeitsadam.takt.core.FocusQueueTask
import uk.co.maybeitsadam.takt.core.FocusSession
import uk.co.maybeitsadam.takt.core.FocusSessionPhase
import uk.co.maybeitsadam.takt.core.FocusWorkBlock
import uk.co.maybeitsadam.takt.core.NextUpCandidate
import uk.co.maybeitsadam.takt.core.NextUpSelector
import uk.co.maybeitsadam.takt.core.PeriodicSchedule
import uk.co.maybeitsadam.takt.core.resolution
import uk.co.maybeitsadam.takt.core.StaleFocusResolution
import uk.co.maybeitsadam.takt.core.TaskAvailabilityPolicy
import uk.co.maybeitsadam.takt.core.TaskCapture
import uk.co.maybeitsadam.takt.core.TaskCondition
import uk.co.maybeitsadam.takt.core.TaskPlanning
import uk.co.maybeitsadam.takt.core.TaskPlanningError
import uk.co.maybeitsadam.takt.core.WorkProgress
import uk.co.maybeitsadam.takt.core.WorkspaceDaily
import uk.co.maybeitsadam.takt.core.WorkspaceNextUpSnapshot
import uk.co.maybeitsadam.takt.core.secondsBetween
import uk.co.maybeitsadam.takt.core.defaultFirstWeekday
import uk.co.maybeitsadam.takt.core.ListFolder
import uk.co.maybeitsadam.takt.core.TaskEditorMetadata
import uk.co.maybeitsadam.takt.core.TaskList
import uk.co.maybeitsadam.takt.core.TaskMatrixPosition
import uk.co.maybeitsadam.takt.core.TaskMetadata
import uk.co.maybeitsadam.takt.core.TaskOutlineItem
import uk.co.maybeitsadam.takt.core.TaskStatus
import uk.co.maybeitsadam.takt.core.Workspace
import uk.co.maybeitsadam.takt.core.WorkspaceItemKind
import uk.co.maybeitsadam.takt.core.WorkspaceKanbanColumn
import uk.co.maybeitsadam.takt.core.WorkspaceListTree
import uk.co.maybeitsadam.takt.core.WorkspaceTask
import uk.co.maybeitsadam.takt.core.coreMillis
import uk.co.maybeitsadam.takt.core.coreName
import uk.co.maybeitsadam.takt.data.db.Db
import uk.co.maybeitsadam.takt.data.db.SqlDates
import uk.co.maybeitsadam.takt.data.db.WorkspaceDatabase
import uk.co.maybeitsadam.takt.data.db.WorkspaceSchema

/**
 * The local source of truth for the workspace: a port of the Swift
 * `WorkspaceStore` and its extensions onto the same SQLite schema.
 *
 * Every method is a suspend function on the database's dispatcher. Writes that
 * touch the user's work go through [journalledWrite], so each is one undo
 * step, exactly as on the Mac. Reads have `observe…` flow variants that re-query
 * when a relevant table changes, which is what a ViewModel collects.
 *
 * `now` defaults to [clock] truncated to milliseconds (what GRDB stores), and
 * day arithmetic uses [zone], the Swift `Calendar.current`'s time zone.
 */
class WorkspaceRepository(
    val database: WorkspaceDatabase,
    private val clock: Clock = Clock.systemUTC(),
    private val zoneProvider: () -> ZoneId = { ZoneId.systemDefault() },
) {
    internal fun now(): Instant = SqlDates.truncate(clock.instant())
    internal val zone: ZoneId get() = zoneProvider()

    // region Journal (WorkspaceStore+Undo.swift)

    /** What undo would take back, phrased for a menu item; nil when there is nothing. */
    suspend fun undoableLabel(): String? = database.coreRead { it.undoableLabel() }

    suspend fun redoableLabel(): String? = database.coreRead { it.redoableLabel() }

    /** Both labels, re-read whenever the journal changes. */
    fun observeHistoryLabels(): Flow<Pair<String?, String?>> = database.observe(setOf("change_log")) {
        it.string("SELECT label FROM change_log WHERE undone = 0 ORDER BY id DESC LIMIT 1") to
            it.string("SELECT label FROM change_log WHERE undone = 1 ORDER BY id ASC LIMIT 1")
    }

    /** The task and list the next undo (or redo) affects, so the UI can reveal restored work. */
    suspend fun historyTarget(forUndo: Boolean): HistoryTarget =
        database.coreRead { it.historyTarget(forUndo) }.let { HistoryTarget(it.taskId, it.listId) }

    /**
     * Reverses the most recent group of changes. Returns its label, or nil when there was nothing to undo.
     * The replay is the Rust core's (core/src/journal.rs), the same code the Mac and the CLI's steps use.
     */
    suspend fun undo(): String? = database.coreWrite(REPLAYED_TABLES) { it.undo() }

    suspend fun redo(): String? = database.coreWrite(REPLAYED_TABLES) { it.redo() }

    /** The journal, newest first, for a history pane. */
    suspend fun history(limit: Int = 100): List<HistoryEntry> =
        database.coreRead { it.undoHistory(limit.coerceAtLeast(0).toUInt()) }.map {
            HistoryEntry(it.id, it.label, it.isUndone, it.changeCount.toInt())
        }

    // endregion

    // region Workspaces, folders and lists (WorkspaceStore.swift)

    /** The workspace, created with its Inbox and default conditions on first launch. */
    suspend fun bootstrapIfNeeded(now: Instant = now()): Workspace {
        // The Rust core's `setup::bootstrap`, outside the journal.
        val id = coreWrite { it.bootstrap(now.toEpochMilli()) }
        return coreRead { core -> core.workspaces().firstOrNull { it.id == id }?.toWorkspace() }
            ?: fail(WorkspaceStoreError.MISSING_LIST)
    }

    /** The list quick capture lands in, found by its role rather than its name. */
    suspend fun inbox(workspaceId: String): TaskList? = database.read { inbox(it, workspaceId) }

    suspend fun workspaces(): List<Workspace> = database.read { db -> db.core.workspaces().map { it.toWorkspace() } }

    fun observeWorkspaces(): Flow<List<Workspace>> =
        database.observe(setOf("workspaces")) { db -> db.core.workspaces().map { it.toWorkspace() } }

    suspend fun folders(workspaceId: String): List<ListFolder> = database.read { foldersIn(it, workspaceId) }

    fun observeFolders(workspaceId: String): Flow<List<ListFolder>> =
        database.observe(setOf("list_folders")) { foldersIn(it, workspaceId) }

    suspend fun lists(workspaceId: String, includingArchived: Boolean = false): List<TaskList> =
        database.read { listsIn(it, workspaceId, includingArchived) }

    fun observeLists(workspaceId: String, includingArchived: Boolean = false): Flow<List<TaskList>> =
        database.observe(setOf("task_lists")) { listsIn(it, workspaceId, includingArchived) }

    /** Creates a folder after its siblings: the Rust core's `lists::create_folder`. */
    suspend fun createFolder(
        workspaceId: String,
        name: String,
        parentFolderId: String? = null,
        now: Instant = now(),
    ): ListFolder {
        val created = coreWrite { it.createFolder(workspaceId, name, parentFolderId, now.toEpochMilli()) }
        return ListFolder(created.id, workspaceId, parentFolderId, created.name, created.sortOrder.toInt(), now, now)
    }

    /** Creates a list after its siblings: the Rust core's `lists::create_list`. */
    suspend fun createList(
        workspaceId: String,
        name: String,
        folderId: String? = null,
        now: Instant = now(),
    ): TaskList {
        val created = coreWrite { it.createList(workspaceId, name, folderId, now.toEpochMilli()) }
        return TaskList(
            id = created.id, workspaceId = workspaceId, folderId = folderId, name = created.name, colorHex = null,
            sortOrder = created.sortOrder.toInt(), isArchived = false, systemRole = null, visibleRootTaskId = null,
            completedAt = null, createdAt = now, updatedAt = now,
        )
    }

    suspend fun task(id: String): WorkspaceTask? = database.read { it.task(id) }

    fun observeTask(id: String): Flow<WorkspaceTask?> = database.observe(setOf("tasks")) { it.task(id) }

    /** Renames a folder: the Rust core's `lists::rename_folder`. */
    suspend fun updateFolder(id: String, name: String, now: Instant = now()) {
        coreWrite { it.renameFolder(id, name, now.toEpochMilli()) }
    }

    /** Moves a folder into another or to the top: the Rust core's `lists::move_folder`. */
    suspend fun moveFolder(id: String, toParentFolderId: String?, now: Instant = now()) {
        coreWrite { it.moveFolder(id, toParentFolderId, now.toEpochMilli()) }
    }

    /** Lists inside are kept (SET NULL moves them to the root); child folders cascade. */
    /** Deletes a folder as one undo step; its lists move to the top. The Rust core's `lists::delete_folder`. */
    suspend fun deleteFolder(id: String) {
        coreWrite { it.deleteFolder(id) }
    }

    /** Sets a list's name and colour: the Rust core's `lists::update_list`. */
    suspend fun updateList(id: String, name: String, colorHex: String?, now: Instant = now()) {
        coreWrite { it.updateList(id, name, colorHex, now.toEpochMilli()) }
    }

    /** Renames a list and touches nothing else, so undo offers "Rename List": the Rust core's `lists::rename_list`. */
    suspend fun renameList(id: String, name: String, now: Instant = now()) {
        coreWrite { it.renameList(id, name, now.toEpochMilli()) }
    }

    /** Moves a list into a folder or to the top: the Rust core's `lists::move_list`. */
    suspend fun moveList(id: String, toFolderId: String?, now: Instant = now()) {
        coreWrite { it.moveList(id, toFolderId, now.toEpochMilli()) }
    }

    /** Moves a list among its siblings: the Rust core's `lists::move_list_within_folder`. */
    suspend fun moveListWithinFolder(id: String, by: Int, now: Instant = now()) {
        coreWrite { it.moveListWithinFolder(id, by, now.toEpochMilli()) }
    }

    /** Puts [id] before [beforeId] (nil: at the end) among its siblings in [inFolderId]. */
    suspend fun placeList(id: String, beforeId: String?, inFolderId: String?, now: Instant = now()) {
        coreWrite { it.placeList(id, beforeId, inFolderId, now.toEpochMilli()) }
    }

    /** The same placement for folders, refusing a drop inside the folder's own subtree. */
    suspend fun placeFolder(id: String, beforeId: String?, inParentFolderId: String?, now: Instant = now()) {
        coreWrite { it.placeFolder(id, beforeId, inParentFolderId, now.toEpochMilli()) }
    }

    /** Moves a folder among its siblings: the Rust core's `lists::move_folder_within_siblings`. */
    suspend fun moveFolderWithinSiblings(id: String, by: Int, now: Instant = now()) {
        coreWrite { it.moveFolderWithinSiblings(id, by, now.toEpochMilli()) }
    }

    /** Archives or restores a list; the Inbox stays. The Rust core's `lists::set_list_archived`. */
    suspend fun setListArchived(archived: Boolean, id: String, now: Instant = now()) {
        coreWrite { it.setListArchived(id, archived, now.toEpochMilli()) }
    }

    /**
     * Puts a list at [position] among the prioritised lists (1 the most
     * important; past the end, last), or takes it out of them with null, as
     * one undo step. An archived or completed list cannot be prioritised.
     * The Rust core's `lists::set_list_priority`.
     */
    suspend fun setListPriority(position: Int?, id: String, now: Instant = now()) {
        coreWrite { it.setListPriority(id, position?.toLong(), now.toEpochMilli()) }
    }

    /**
     * Sets what a list requires — a standalone list's id, or a nested list's
     * task id — in the same groups a task keeps, as one undo step; every task
     * in it and in the lists nested under it inherits them. Empty requires
     * nothing. The Rust core's `conditions::inherited::set_list_requirements`.
     * False when it already required exactly that.
     */
    suspend fun setListRequirements(listId: String, groups: List<List<String>>, now: Instant = now()): Boolean =
        coreWrite { it.setListRequirements(listId, groups, now.toEpochMilli()) }

    /**
     * What a list requires of every task in it, standalone or nested, again
     * whenever a list or a nested list's plan changes.
     */
    fun observeListRequirements(listId: String): Flow<List<List<String>>> =
        database.observe(setOf("task_lists", "task_metadata")) { it.core.listRequirementGroups(listId) }

    /**
     * The lists a task inherits requirements from, outermost first — its
     * list, then each nested list above it that requires anything — again
     * whenever one changes or the task moves.
     */
    fun observeInheritedRequirements(taskId: String): Flow<List<uniffi.takt_core.InheritedRequirement>> =
        database.observe(setOf("tasks", "task_lists", "task_metadata")) { it.core.inheritedRequirements(taskId) }

    /** Deletes a list and its tasks as one undo step; the Inbox is permanent. The Rust core's `lists::delete_list`. */
    suspend fun deleteList(id: String) {
        coreWrite { it.deleteList(id) }
    }

    // endregion

    // region Tasks (WorkspaceStore.swift)

    suspend fun outline(listId: String, parentTaskId: String? = null): List<TaskOutlineItem> =
        listTree(listId).outline(parentTaskId)

    fun observeOutline(listId: String, parentTaskId: String? = null): Flow<List<TaskOutlineItem>> =
        database.observe(TASK_ORDER_TABLES) { listTree(it, listId).outline(parentTaskId) }

    /** A parent's direct children, as a project's own board shows them. */
    suspend fun tasks(listId: String, parentTaskId: String? = null): List<WorkspaceTask> =
        database.read { db -> db.core.childTasks(listId, parentTaskId).map { it.toTask() } }

    fun observeTasks(listId: String, parentTaskId: String? = null): Flow<List<WorkspaceTask>> =
        database.observe(TASK_ORDER_TABLES) { db -> db.core.childTasks(listId, parentTaskId).map { it.toTask() } }

    /** The imported wrapper standing in for the list's roots, while it is still the only root. */
    suspend fun visibleRootParentTaskId(list: TaskList): String? =
        database.read { visibleRootParentTaskId(it, list.id) }

    /** The virtual Everything scope: every active list's visible roots. */
    suspend fun visibleRootTasks(workspaceId: String): List<WorkspaceTask> = database.read { db ->
        listsIn(db, workspaceId, false).flatMap { list ->
            db.taskSiblings(list.id, visibleRootParentTaskId(db, list.id), withId = false)
        }
    }

    /**
     * Creates a task, with whatever the add field read off its title, as one undo step:
     * the Rust core's `tasks::create_task`.
     */
    suspend fun createTask(
        listId: String,
        title: String,
        parentTaskId: String? = null,
        kind: WorkspaceItemKind = WorkspaceItemKind.TASK,
        kanbanColumn: String? = null,
        startAt: Instant? = null,
        atTop: Boolean = false,
        adjacentTaskId: String? = null,
        above: Boolean = false,
        dueAt: Instant? = null,
        estimateSeconds: Int? = null,
        tags: List<String> = emptyList(),
        priority: Int? = null,
        waitingOn: String? = null,
        externalLinks: List<String> = emptyList(),
        now: Instant = now(),
    ): WorkspaceTask {
        val new = uniffi.takt_core.NewTask(
            listId = listId, title = title, parentTaskId = parentTaskId, kind = kind.raw, notes = "",
            kanbanColumn = kanbanColumn, startAtMs = startAt?.toEpochMilli(), dueAtMs = dueAt?.toEpochMilli(),
            estimateSeconds = estimateSeconds?.toLong(), tags = tags, priority = priority?.toLong(),
            waitingOn = waitingOn, externalLinks = externalLinks, atTop = atTop, adjacentTaskId = adjacentTaskId,
            above = above,
        )
        val id = coreWrite { it.createTask(new, now.toEpochMilli()) }
        return task(id) ?: fail(WorkspaceStoreError.MISSING_TASK)
    }

    /** Closing one occurrence of a repeating task writes the next one. */
    /**
     * Opens, completes or cancels a task: the Rust core's `tasks::set_status`, which also writes a
     * repeating task's next occurrence, stepped in [zone], and ends the habits made from it.
     */
    suspend fun setStatus(status: TaskStatus, taskId: String, now: Instant = now()) {
        coreWrite { it.setStatus(taskId, status.raw, now.toEpochMilli(), zone.id) }
    }

    /** Sets a task's title, notes, due time and estimate: the Rust core's `editor::update_task`. */
    suspend fun updateTask(
        id: String,
        title: String,
        notes: String,
        dueAt: Instant?,
        estimateSeconds: Int?,
        now: Instant = now(),
    ) {
        coreWrite {
            it.updateTask(id, title, notes, dueAt?.toEpochMilli(), estimateSeconds?.toLong(), now.toEpochMilli(), zone.id)
        }
    }

    suspend fun taskEditorMetadata(taskId: String): TaskEditorMetadata =
        database.read { taskEditorSnapshot(it, taskId).metadata }

    /** Sets a task's priority, tags, links and repeat: the Rust core's `editor::update_editor_metadata`. */
    suspend fun updateTaskEditorMetadata(taskId: String, metadata: TaskEditorMetadata, now: Instant = now()) {
        coreWrite { it.updateEditorMetadata(taskId, metadata.toCore(), now.toEpochMilli()) }
    }

    suspend fun kanbanColumn(taskId: String): String? = database.read { it.metadata(taskId)?.kanbanColumn }

    /** Board placement for many cards in one read; missing metadata means unplaced. */
    suspend fun boardMetadata(taskIds: List<String>): BoardMetadata {
        if (taskIds.isEmpty()) return BoardMetadata(emptyMap(), emptyMap())
        return coreRead { core ->
            val columns = HashMap<String, String>()
            val positions = taskIds.toSet().associateWith { TaskMatrixPosition(null, null) }.toMutableMap()
            for (record in core.metadataForTasks(taskIds)) {
                record.kanbanColumn?.let { columns[record.taskId] = it }
                positions[record.taskId] = TaskMatrixPosition(record.matrixUrgency?.toInt(), record.matrixImportance?.toInt())
            }
            BoardMetadata(columns, positions)
        }
    }

    suspend fun setKanbanColumn(column: String?, taskId: String, now: Instant = now()) =
        setKanbanColumn(column, listOf(taskId), now)

    /** Moving a whole column is one transaction and one undo step. */
    suspend fun setKanbanColumn(column: String?, taskIds: List<String>, now: Instant = now()) {
        if (taskIds.isEmpty()) return
        coreWrite { it.setKanbanColumn(taskIds, column, now.toEpochMilli()) }
    }

    suspend fun matrixPosition(taskId: String): TaskMatrixPosition = database.read { db ->
        val metadata = db.metadata(taskId)
        TaskMatrixPosition(metadata?.matrixUrgency, metadata?.matrixImportance)
    }

    /** Places a task on the priority matrix: the Rust core's `tasks::set_matrix_position`. */
    suspend fun setMatrixPosition(position: TaskMatrixPosition, taskId: String, now: Instant = now()) {
        coreWrite {
            it.setMatrixPosition(taskId, position.urgency?.toLong(), position.importance?.toLong(), now.toEpochMilli())
        }
    }

    /** Moves a task and its subtree; the destination parent must be in the destination list and outside the subtree. */
    /** Moves a task and its subtree: the Rust core's `tasks::move_task`. */
    suspend fun moveTask(
        id: String,
        toListId: String,
        parentTaskId: String? = null,
        toVisibleRoot: Boolean = false,
        now: Instant = now(),
    ) {
        coreWrite { it.moveTask(id, toListId, parentTaskId, toVisibleRoot, now.toEpochMilli()) }
    }

    /** Moves a task among its siblings: the Rust core's `tasks::move_task_within_siblings`. */
    suspend fun moveTaskWithinSiblings(id: String, by: Int, now: Instant = now()) {
        coreWrite { it.moveTaskWithinSiblings(id, by, now.toEpochMilli()) }
    }

    /** Puts a task first among its siblings. */
    /** Moves a task to the top of its siblings: the Rust core's `tasks::move_task_to_start`. */
    suspend fun moveTaskToStart(id: String, now: Instant = now()) {
        coreWrite { it.moveTaskToStart(id, now.toEpochMilli()) }
    }

    /** Reorders a card before another card of the same parent, optionally filing it in [kanbanColumn]. */
    /** Drops a task before a sibling: the Rust core's `tasks::move_task_before`. */
    suspend fun moveTaskBefore(id: String, targetId: String, kanbanColumn: String? = null, now: Instant = now()) {
        coreWrite { it.moveTaskBefore(id, targetId, kanbanColumn, now.toEpochMilli()) }
    }

    /** Makes the task a child of its immediately preceding sibling. */
    /** Indents a task under the sibling above: the Rust core's `tasks::indent_task`. */
    suspend fun indentTask(id: String, now: Instant = now()) {
        coreWrite { it.indentTask(id, now.toEpochMilli()) }
    }

    /** Promotes a task one level, immediately after its former parent. */
    /** Outdents a task to follow its parent: the Rust core's `tasks::outdent_task`. */
    suspend fun outdentTask(id: String, now: Instant = now()) {
        coreWrite { it.outdentTask(id, now.toEpochMilli()) }
    }

    /** Deletes a task and its subtree as one undo step: the Rust core's `tasks::delete_task`. */
    suspend fun deleteTask(id: String) {
        coreWrite { it.deleteTask(id) }
    }

    /**
     * Runs a write the Rust core makes, announcing every table it could have
     * changed and turning the failures screens react to into the repository's
     * own errors, so they see the same cases whichever side made the write.
     */
    internal suspend fun <T> coreWrite(block: (uniffi.takt_core.CoreWorkspace) -> T): T =
        mappingCoreErrors { database.coreWrite(CORE_WRITTEN_TABLES, block) }

    /**
     * Runs [block]'s writes back to back, with no other write between them;
     * each is still its own transaction. See [WorkspaceDatabase.together].
     */
    suspend fun <T> writingTogether(block: suspend () -> T): T = database.together(block)

    /** Runs a read the Rust core makes, with the same errors as [coreWrite]. */
    internal suspend fun <T> coreRead(block: (uniffi.takt_core.CoreWorkspace) -> T): T =
        mappingCoreErrors { database.coreRead(block) }

    private inline fun <T> mappingCoreErrors(call: () -> T): T = try {
        call()
    } catch (error: CoreException) {
        when (error) {
            is CoreException.MissingTask -> fail(WorkspaceStoreError.MISSING_TASK)
            is CoreException.MissingDaily -> fail(WorkspaceStoreError.MISSING_DAILY)
            is CoreException.MissingList -> fail(WorkspaceStoreError.MISSING_LIST)
            is CoreException.MissingFolder -> fail(WorkspaceStoreError.MISSING_FOLDER)
            is CoreException.SystemListIsPermanent -> fail(WorkspaceStoreError.SYSTEM_LIST_IS_PERMANENT)
            is CoreException.EmptyName -> fail(WorkspaceStoreError.EMPTY_NAME)
            is CoreException.InvalidFolderMove -> fail(WorkspaceStoreError.INVALID_FOLDER_MOVE)
            is CoreException.InvalidCondition -> planningFail(TaskPlanningError.INVALID_CONDITION)
            is CoreException.InvalidSchedule -> planningFail(TaskPlanningError.INVALID_SCHEDULE)
            is CoreException.InvalidMinimum -> planningFail(TaskPlanningError.INVALID_MINIMUM)
            is CoreException.EstimateRequired -> planningFail(TaskPlanningError.ESTIMATE_REQUIRED)
            is CoreException.InvalidDate -> planningFail(TaskPlanningError.INVALID_DATE)
            is CoreException.EditorConflict -> throw TaskEditorException(TaskEditorError.CONFLICTING_CHANGES)
            is CoreException.InvalidVisibleRoot -> throw TaskEditorException(TaskEditorError.INVALID_VISIBLE_ROOT)
            is CoreException.NoActiveFocusTask -> fail(WorkspaceStoreError.NO_ACTIVE_FOCUS_TASK)
            is CoreException.Unavailable -> planningFail(TaskPlanningError.UNAVAILABLE)
            is CoreException.InvalidTaskMove -> fail(WorkspaceStoreError.INVALID_TASK_MOVE)
            is CoreException.DependencyCycle -> fail(WorkspaceStoreError.DEPENDENCY_CYCLE)
            is CoreException.MissingFilterList -> fail(WorkspaceStoreError.MISSING_FILTER_LIST)
            is CoreException.InvalidFilter -> fail(WorkspaceStoreError.INVALID_FILTER)
            is CoreException.ColumnRefused -> throw WorkspaceStoreException(WorkspaceStoreError.COLUMN_REFUSED, error.detail)
            else -> throw error
        }
    }

    // endregion

    // region Lists as tasks (WorkspaceStore+Lists.swift)

    /** Turns an item into a standalone list in [folderId] (nil: the top level), keeping its subtree. */
    suspend fun moveTaskToFolder(id: String, folderId: String?, now: Instant = now()): TaskList {
        val listId = coreWrite { it.moveTaskToFolder(id, folderId, now.toEpochMilli()) }
        return database.read { it.list(listId) } ?: fail(WorkspaceStoreError.MISSING_LIST)
    }

    /** A standalone list becomes one task in Inbox, keeping its children, metadata and hierarchy. */
    suspend fun convertListToTask(id: String, now: Instant = now()): WorkspaceTask {
        val taskId = coreWrite { it.convertListToTask(id, now.toEpochMilli()) }
        return task(taskId) ?: fail(WorkspaceStoreError.MISSING_TASK)
    }

    /** Drops a standalone list into another list as a nested list. */
    suspend fun nestList(id: String, inListId: String, parentTaskId: String? = null, now: Instant = now()): WorkspaceTask {
        val taskId = coreWrite { it.nestList(id, inListId, parentTaskId, now.toEpochMilli()) }
        return task(taskId) ?: fail(WorkspaceStoreError.MISSING_TASK)
    }

    suspend fun setItemKind(kind: WorkspaceItemKind, id: String, now: Instant = now()) {
        coreWrite { it.setItemKind(id, kind.raw, now.toEpochMilli()) }
    }

    suspend fun setNestedListPromoted(promoted: Boolean, id: String, now: Instant = now()) {
        coreWrite { it.setNestedListPromoted(id, promoted, now.toEpochMilli()) }
    }

    suspend fun setNestedListArchived(archived: Boolean, id: String, now: Instant = now()) {
        coreWrite { it.setNestedListArchived(id, archived, now.toEpochMilli()) }
    }

    suspend fun setListCompleted(completed: Boolean, id: String, now: Instant = now()) {
        coreWrite { it.setListCompleted(id, completed, now.toEpochMilli()) }
    }

    /** Open, doable tasks across the workspace's active lists, or only [limitedTo] in that order. */
    suspend fun actionableTasks(workspaceId: String, limitedTo: List<String>? = null): List<WorkspaceTask> =
        database.read { actionableTasks(it, workspaceId, limitedTo) }

    fun observeActionableTasks(workspaceId: String, limitedTo: List<String>? = null): Flow<List<WorkspaceTask>> =
        database.observe(TASK_ORDER_TABLES + "task_lists") { actionableTasks(it, workspaceId, limitedTo) }

    suspend fun visibleOutline(listId: String, parentTaskId: String? = null): List<TaskOutlineItem> =
        listTree(listId).visibleOutline(parentTaskId)

    fun observeVisibleOutline(listId: String, parentTaskId: String? = null): Flow<List<TaskOutlineItem>> =
        database.observe(TASK_ORDER_TABLES) { listTree(it, listId).visibleOutline(parentTaskId) }

    /** The board configurations, seeding [legacy] and the defaults for [currentKey] without an undo step. */
    suspend fun kanbanBoardConfigurations(
        legacy: Map<String, String> = emptyMap(),
        currentKey: String,
    ): Map<String, List<WorkspaceKanbanColumn>> {
        // The Rust core's `imports::kanban_board_baseline`.
        val boards = coreWrite {
            it.kanbanBoardBaseline(legacy.map { (key, json) -> uniffi.takt_core.BoardBaseline(key, json) }, currentKey)
        }
        return boards.mapNotNull { board -> board.key to decodeBoardColumns(board.columnsJson) }
            .filter { it.second.isNotEmpty() }.toMap()
    }

    /** Every stored layout, rules included, re-read whenever one changes. */
    fun observeKanbanBoards(): Flow<Map<String, List<WorkspaceKanbanColumn>>> =
        database.observe(setOf("kanban_boards")) { db ->
            db.core.kanbanBoards().map { board -> board.key to decodeBoardColumns(board.columnsJson) }
                .filter { it.second.isNotEmpty() }.toMap()
        }

    /**
     * Board [key]'s columns over [listIds] and where its tasks are drawn: the
     * Rust core's `board_columns`, which resolves a board without a layout of
     * its own to its list's, the default or the stock five, and places tasks
     * a rule column catches or an entered card's floor clamps.
     */
    suspend fun boardColumns(key: String, listIds: List<String>, now: Instant = now()): BoardColumnsRead =
        coreRead { it.boardColumns(key, listIds, now.toEpochMilli(), zone.id) }.toRead()

    /** [boardColumns], re-read whenever a task, its metadata or a layout changes. */
    fun observeBoardColumns(key: String, listIds: List<String>): Flow<BoardColumnsRead> =
        database.observe(BOARD_TABLES) { db -> db.core.boardColumns(key, listIds, now().toEpochMilli(), zone.id).toRead() }

    /**
     * Saves a board's layout, moving [movingTaskIds] to [toColumn] in the same
     * undo step: the Rust core's `set_board_columns`. A column's rules go with
     * it, so pass the layout read from the core rather than rebuilt by title.
     */
    suspend fun setKanbanBoardColumns(
        columns: List<WorkspaceKanbanColumn>,
        key: String,
        movingTaskIds: List<String> = emptyList(),
        toColumn: String? = null,
        label: String = "Edit Board",
        now: Instant = now(),
    ) {
        if (columns.isEmpty()) return
        val core = columns.map { it.toCore() }
        coreWrite { it.setBoardColumns(key, core, movingTaskIds, toColumn, label, now.toEpochMilli()) }
    }

    /**
     * Deletes a column from board [key], which keeps a layout of its own from
     * then on, moving the tasks of [listIds] filed in it to [toColumn], as one
     * "Delete Column" step. A rule column, Today and Waiting on move nothing.
     */
    suspend fun deleteBoardColumn(key: String, listIds: List<String>, columnId: String, toColumn: String?, now: Instant = now()) {
        coreWrite { it.deleteBoardColumn(key, listIds, columnId, toColumn, now.toEpochMilli()) }
    }

    /** Has board [key] follow its list's or the default columns again. */
    suspend fun resetBoardColumns(key: String) {
        coreWrite { it.resetBoardColumns(key) }
    }

    /** Makes board [key]'s columns the workspace's default, which it then follows. */
    suspend fun makeDefaultBoardColumns(key: String) {
        coreWrite { it.makeDefaultBoardColumns(key) }
    }

    /**
     * Moves tasks into [columnId] on board [key]. Into a rule column the first
     * is edited until the rules hold, or the move is refused with
     * [WorkspaceStoreError.COLUMN_REFUSED] and the core's reason.
     */
    suspend fun moveToBoardColumn(key: String, taskIds: List<String>, columnId: String, now: Instant = now()) {
        if (taskIds.isEmpty()) return
        coreWrite { it.moveToBoardColumn(key, taskIds, columnId, now.toEpochMilli(), zone.id) }
    }

    // endregion

    // region List trees (WorkspaceListTree.swift)

    suspend fun listTree(listId: String): WorkspaceListTree = database.read { listTree(it, listId) }

    /** Several lists' rows in one read transaction, so a combined scope sees one moment. */
    suspend fun listTrees(listIds: List<String>): Map<String, WorkspaceListTree> {
        if (listIds.isEmpty()) return emptyMap()
        return database.read { db -> listTrees(db, listIds) }
    }

    fun observeListTrees(listIds: List<String>): Flow<Map<String, WorkspaceListTree>> =
        database.observe(TASK_ORDER_TABLES) { db -> listTrees(db, listIds) }

    /** Tasks by id, in one read; missing ids are absent. */
    suspend fun tasks(ids: List<String>): Map<String, WorkspaceTask> {
        if (ids.isEmpty()) return emptyMap()
        return database.read { tasksById(it, ids) }
    }

    // endregion

    // region External writes (WorkspaceStore+ExternalWrites.swift)

    /**
     * `PRAGMA data_version` on the Rust core's connection, which every write
     * of this app goes through: it moves only when another connection or
     * process commits, never for the app's own writes.
     */
    suspend fun externalChangeToken(): Long = coreRead { it.dataVersion() }

    // endregion

    // region Editing (WorkspaceStore+Editing.swift)

    suspend fun taskEditorSnapshot(taskId: String): TaskEditorSnapshot = database.read { taskEditorSnapshot(it, taskId) }

    fun observeTaskEditorSnapshot(taskId: String): Flow<TaskEditorSnapshot?> =
        database.observe(setOf("tasks", "task_lists", "task_metadata", "dailies")) { db ->
            runCatching { taskEditorSnapshot(db, taskId) }.getOrNull()
        }

    /**
     * Saves the inspector's draft as one step; throws CONFLICTING_CHANGES if the task changed
     * underneath it. The Rust core's `editor::save_editor`.
     */
    suspend fun saveTaskEditor(draft: TaskEditorDraft, now: Instant = now()): TaskEditorSnapshot {
        val edit = draft.validatedSnapshot()
        coreWrite { it.saveEditor(edit.toCore(), draft.baseline.toCore(), now.toEpochMilli(), zone.id) }
        return taskEditorSnapshot(edit.taskId)
    }

    /** Folders [folderId] may move under: everything but itself and its subtree. */
    suspend fun validParentFolders(folderId: String): List<ListFolder> =
        coreRead { core -> core.validParentFolders(folderId).map { it.toFolder() } }

    /** Saves the folder settings sheet: the Rust core's `lists::save_folder_settings`. */
    suspend fun saveFolderSettings(id: String, name: String, parentFolderId: String?, now: Instant = now()) {
        coreWrite { it.saveFolderSettings(id, name, parentFolderId, now.toEpochMilli()) }
    }

    /** The only root that may serve as the list's visible root, if there is one. */
    suspend fun visibleRootCandidates(listId: String): List<WorkspaceTask> =
        coreRead { core -> core.visibleRootCandidates(listId).map { it.toTask() } }

    /** Saves the list settings sheet: the Rust core's `lists::save_list_settings`. */
    suspend fun saveListSettings(
        id: String,
        name: String,
        colorHex: String?,
        folderId: String?,
        isArchived: Boolean,
        visibleRootTaskId: String?,
        now: Instant = now(),
    ) {
        val settings = uniffi.takt_core.ListSettings(name, colorHex, folderId, isArchived, visibleRootTaskId)
        coreWrite { it.saveListSettings(id, settings, now.toEpochMilli()) }
    }

    // endregion

    // region Conditions (WorkspaceStore+Conditions.swift)

    suspend fun conditions(workspaceId: String): List<TaskCondition> = database.read { conditionsIn(it, workspaceId) }

    fun observeConditions(workspaceId: String): Flow<List<TaskCondition>> =
        database.observe(setOf("task_conditions")) { conditionsIn(it, workspaceId) }

    /** Creates a condition: the Rust core's `conditions::create_condition`. */
    suspend fun createCondition(
        workspaceId: String,
        name: String,
        isLocation: Boolean = false,
        now: Instant = now(),
    ): TaskCondition {
        val id = coreWrite { it.createCondition(workspaceId, name, isLocation, now.toEpochMilli()) }
        return database.read { it.condition(id) } ?: planningFail(TaskPlanningError.INVALID_CONDITION)
    }

    /**
     * Creates a condition under a category (null for none; "Location" makes it
     * a place): the Rust core's `conditions::create_condition_in_category`.
     */
    suspend fun createCondition(
        workspaceId: String,
        name: String,
        category: String?,
        now: Instant = now(),
    ): TaskCondition {
        val id = coreWrite { it.createConditionInCategory(workspaceId, name, category, now.toEpochMilli()) }
        return database.read { it.condition(id) } ?: planningFail(TaskPlanningError.INVALID_CONDITION)
    }

    /**
     * Sets what a condition provides, replacing what it did — while it holds,
     * they hold too: the Rust core's `conditions::set_condition_provides`,
     * one "Edit Condition" step. False when it already provided exactly that.
     */
    suspend fun setConditionProvides(id: String, provides: List<String>, now: Instant = now()): Boolean =
        coreWrite { it.setConditionProvides(id, provides, now.toEpochMilli()) }

    /**
     * Marks a condition as meaning a kind of device ("laptop", "phone",
     * "tablet"), or with null makes it an ordinary condition again: the Rust
     * core's `conditions::device::set_condition_device`, one "Edit Condition"
     * step. Marking it takes it out of what is said to hold and of what
     * others provide, since it now holds by itself on a device of its kind.
     * False when it already was that.
     */
    suspend fun setConditionDeviceKind(id: String, kind: String?, now: Instant = now()): Boolean =
        coreWrite { it.setConditionDeviceKind(id, kind, now.toEpochMilli()) }

    /** Files a condition under a category, or none: the Rust core's `conditions::set_condition_category`. */
    suspend fun setConditionCategory(id: String, category: String?, now: Instant = now()) {
        coreWrite { it.setConditionCategory(id, category, now.toEpochMilli()) }
    }

    /**
     * What holds now, as the workspace keeps it (`focus_contexts`), or null if
     * it never was said: the Rust core's `focus_context::load`. The row syncs,
     * so this is the same answer the Mac, the iPhone, the CLI and the agent see.
     */
    suspend fun focusContext(workspaceId: String): uniffi.takt_core.StoredFocusContext? =
        database.read { it.core.focusContext(workspaceId) }

    /** [focusContext], again whenever it changes here, on another device or through the CLI. */
    fun observeFocusContext(workspaceId: String): Flow<uniffi.takt_core.StoredFocusContext?> =
        database.observe(setOf("focus_contexts")) { it.core.focusContext(workspaceId) }

    /**
     * Saves what holds now: the Rust core's `focus_context::save`, which stamps
     * when the conditions on were said. Not an undo step. False when nothing changed.
     */
    suspend fun saveFocusContext(
        workspaceId: String,
        context: uniffi.takt_core.StoredFocusContext,
        now: Instant = now(),
    ): Boolean = coreWrite { it.saveFocusContext(workspaceId, context, now.toEpochMilli()) }

    /**
     * Moves a context this phone kept in its own settings, before the context
     * was the workspace's, into the workspace — once, and only when the
     * workspace has none: the phone's carried no time, so an answer the
     * workspace already holds (from this phone's sync or another device) is
     * the newer one. Returns whether it moved.
     */
    suspend fun adoptPhoneFocusContext(
        workspaceId: String,
        conditionIds: Set<String>,
        mode: String?,
        now: Instant = now(),
    ): Boolean {
        if (conditionIds.isEmpty() && mode == null) return false
        if (focusContext(workspaceId) != null) return false
        return saveFocusContext(workspaceId, emptyFocusContext().copy(conditionIds = conditionIds.sorted(), mode = mode ?: "progress"), now)
    }

    /** The conditions an open task requires: the Rust core's `conditions::required_condition_ids`. */
    suspend fun requiredConditionIds(workspaceId: String): Set<String> =
        database.read { it.core.requiredConditionIds(workspaceId).toSet() }

    /**
     * How many open tasks require each condition, by id — a condition no open
     * task needs is absent: the Rust core's `conditions::condition_usage`,
     * again whenever a task, its plan or a condition changes.
     */
    fun observeConditionUsage(workspaceId: String): Flow<Map<String, Int>> =
        database.observe(setOf("tasks", "task_metadata", "task_lists", "task_conditions")) { db ->
            db.core.conditionUsage(workspaceId).filter { it.openTasks > 0u }.associate { it.conditionId to it.openTasks.toInt() }
        }

    /** Saves a condition: the Rust core's `conditions::save_condition`. */
    suspend fun saveCondition(id: String, name: String, isLocation: Boolean, isArchived: Boolean, now: Instant = now()) {
        coreWrite { it.saveCondition(id, name, isLocation, isArchived, now.toEpochMilli()) }
    }

    /** Copies a task's requirements, start and block rules (not due dates) onto every descendant. */
    suspend fun applyPlanningToDescendants(taskId: String, now: Instant = now()) {
        coreWrite { it.applyPlanningToDescendants(taskId, now.toEpochMilli(), zone.id) }
    }

    /** Every task's planning, keyed by task id. */
    suspend fun taskPlanningValues(): Map<String, TaskPlanning> = database.read { taskPlanningValues(it) }

    // endregion

    // region Dailies (WorkspaceStore+Dailies.swift)

    /** Every non-archived daily due on [day], with its task and that day's contribution. */
    suspend fun dailies(day: Instant = now(), zone: ZoneId = this.zone): List<DailyItem> =
        database.read { dailiesOn(it, day, zone) }

    fun observeDailies(day: Instant = now(), zone: ZoneId = this.zone): Flow<List<DailyItem>> =
        database.observe(setOf("dailies", "daily_contributions", "tasks")) { dailiesOn(it, day, zone) }

    /** Every daily regardless of schedule. */
    suspend fun allDailies(): List<WorkspaceDaily> = coreRead { core -> core.allDailies().map { it.toDaily() } }

    suspend fun daily(taskId: String): WorkspaceDaily? = coreRead { it.dailyForTask(taskId)?.toDaily() }

    /** Makes the task a daily, or returns (and revives) the one it already has. */
    /** Makes a task a daily: the Rust core's `dailies::make_daily`. */
    suspend fun makeDaily(
        taskId: String,
        weekdays: Set<Int> = (1..7).toSet(),
        intervalDays: Int? = null,
        targetSeconds: Int? = null,
        now: Instant = now(),
    ): WorkspaceDaily {
        val id = coreWrite {
            it.makeDaily(
                taskId, weekdays.sorted().map { day -> day.toUInt() }, intervalDays?.toLong(),
                targetSeconds?.toLong(), now.toEpochMilli(),
            )
        }
        return coreRead { it.daily(id)?.toDaily() } ?: fail(WorkspaceStoreError.MISSING_DAILY)
    }

    /** Archives rather than deletes, so logged contributions keep a parent. */
    suspend fun archiveDaily(taskId: String, now: Instant = now()) {
        coreWrite { it.archiveDaily(taskId, now.toEpochMilli()) }
    }

    /**
     * Edits a daily's schedule. An argument left as [FieldEdit.Keep] keeps its value;
     * `intervalDays = FieldEdit.To(null)` switches back to weekdays.
     */
    /** Edits a daily: the Rust core's `dailies::update_daily`. */
    suspend fun updateDaily(
        id: String,
        weekdays: Set<Int>? = null,
        intervalDays: FieldEdit<Int?> = FieldEdit.Keep,
        targetSeconds: FieldEdit<Int?> = FieldEdit.Keep,
        now: Instant = now(),
    ) {
        val edit = uniffi.takt_core.DailyEdit(
            weekdays = weekdays?.sorted()?.map { it.toUInt() },
            setInterval = intervalDays is FieldEdit.To,
            intervalDays = (intervalDays as? FieldEdit.To)?.value?.toLong(),
            setTarget = targetSeconds is FieldEdit.To,
            targetSeconds = (targetSeconds as? FieldEdit.To)?.value?.toLong(),
        )
        coreWrite { it.updateDaily(id, edit, now.toEpochMilli()) }
    }

    /**
     * Records [seconds] worked on a task without a focus session and without
     * finishing it; a daily due today is credited as a timed block would be.
     */
    suspend fun logTaskWork(taskId: String, seconds: Int, now: Instant = now(), zone: ZoneId = this.zone) {
        coreWrite { it.logTaskWork(taskId, seconds.toLong(), now.toEpochMilli(), zone.id) }
    }

    /** Records progress for today, accumulating onto any contribution already logged. */
    /** Logs progress on a daily for the day [now] falls on in [zone]: the Rust core's `dailies::log_contribution`. */
    suspend fun logContribution(
        dailyId: String,
        seconds: Int = 0,
        complete: Boolean = true,
        now: Instant = now(),
        zone: ZoneId = this.zone,
    ): DailyContribution {
        val id = coreWrite { it.logContribution(dailyId, seconds.toLong(), complete, now.toEpochMilli(), zone.id) }
        return coreRead { it.contribution(id)?.toContribution() } ?: fail(WorkspaceStoreError.MISSING_DAILY)
    }

    /** Un-ticks a day without discarding the time already logged against it. */
    suspend fun clearContribution(dailyId: String, day: Instant = now(), zone: ZoneId = this.zone) {
        coreWrite { it.clearContribution(dailyId, day.toEpochMilli(), zone.id) }
    }

    /** Contributions over the last [days] days ending on [endingOn], oldest first. */
    suspend fun contributionHistory(
        dailyId: String,
        days: Int,
        endingOn: Instant = now(),
        zone: ZoneId = this.zone,
    ): List<DailyContribution> {
        val keys = (0 until maxOf(1, days)).map { offset ->
            DailyContribution.dayKey(endingOn.atZone(zone).minusDays(offset.toLong()).toInstant(), zone)
        }
        return coreRead { core -> core.contributions(dailyId, keys).map { it.toContribution() } }
    }

    /** How many things are finished today (this one included), and the streak of days ending today. */
    suspend fun completionContext(now: Instant = now(), zone: ZoneId = this.zone): CompletionContext =
        coreRead { core ->
            // The Rust core's `rows::completion_context`, streak walk included.
            val context = core.completionContext(now.coreMillis, zone.coreName)
            CompletionContext(context.ordinalToday.toInt(), context.streakDays.toInt())
        }

    // endregion

    // region Next up and the focus order (WorkspaceStore+Dailies.swift)

    /** Every open task that could be done now, shaped for `NextUpSelector`. */
    suspend fun nextUpCandidates(now: Instant = now(), zone: ZoneId = this.zone): List<NextUpCandidate> =
        database.coreRead { focusCandidates(it, now, zone) }

    /** Pins one task to [atIndex] in the ladder, leaving the rest to the ranking. */
    suspend fun pinTask(taskId: String, atIndex: Int, now: Instant = now()) {
        coreWrite { it.pinTask(taskId, atIndex.toLong(), now.toEpochMilli()) }
    }

    /** Releases one task back to the ranking. */
    suspend fun unpinTask(taskId: String, now: Instant = now()) {
        coreWrite { it.unpinTask(taskId, now.toEpochMilli()) }
    }

    /** Hands the ladder back to the ranking. */
    suspend fun clearFocusOrder(now: Instant = now()) {
        coreWrite { it.clearFocusOrder(now.toEpochMilli()) }
    }

    suspend fun hasManualFocusOrder(): Boolean = database.read { hasManualFocusOrder(it) }

    /** "Schedule it for later": sets the task's start. */
    suspend fun scheduleTask(id: String, startAt: Instant?, now: Instant = now()) {
        coreWrite { it.scheduleTask(id, startAt?.toEpochMilli(), now.toEpochMilli(), zone.id) }
    }

    // endregion

    // region Today (WorkspaceStore+Today.swift)

    /** Puts each task in the Today column, or takes it out (dropping its hand-placed rank). */
    suspend fun setPlannedForToday(planned: Boolean, taskIds: List<String>, now: Instant = now()) {
        if (taskIds.isEmpty()) return
        // The Rust core's `today::set_planned_for_today`.
        coreWrite { it.setPlannedForToday(planned, taskIds, now.toEpochMilli()) }
    }

    /** Writes the day's hand-made order: each task's position becomes its rank. */
    suspend fun arrangeDay(orderedTaskIds: List<String>, now: Instant = now()) {
        if (orderedTaskIds.isEmpty()) return
        // The Rust core's `today::arrange_day`.
        coreWrite { it.arrangeDay(orderedTaskIds, now.toEpochMilli()) }
    }

    // endregion

    // region Capture and repeats (WorkspaceStore+Capture.swift, WorkspaceStore+Periodic.swift)

    /** `createTask` for typed text: `45m #work ^mon @fri !1` on the end is filed with the task in one undo step. */
    suspend fun createTask(
        capturing: String,
        listId: String,
        parentTaskId: String? = null,
        kanbanColumn: String? = null,
        startAt: Instant? = null,
        atTop: Boolean = false,
        adjacentTaskId: String? = null,
        above: Boolean = false,
        now: Instant = now(),
    ): WorkspaceTask {
        val capture = TaskCapture.parse(capturing, now, zone)
        return createTask(
            listId = listId, title = capture.title, parentTaskId = parentTaskId, kanbanColumn = kanbanColumn,
            startAt = capture.startAt ?: startAt, atTop = atTop, adjacentTaskId = adjacentTaskId, above = above,
            dueAt = capture.dueAt,
            estimateSeconds = capture.estimateSeconds, tags = capture.tags, priority = capture.priority,
            waitingOn = capture.waitingOn, externalLinks = capture.links, now = now,
        )
    }

    /** Whether a task repeats, and how often. */
    suspend fun periodicSchedule(taskId: String): PeriodicSchedule? = database.read { db ->
        db.metadata(taskId)?.recurrenceRule?.let { PeriodicSchedule.parse(it) }
    }

    /** When a task is scheduled to be begun, if anything scheduled it. */
    suspend fun startAt(taskId: String): Instant? = database.read { it.metadata(taskId)?.startAt }

    // endregion

    // region Focus sessions (WorkspaceStore+Focus.swift, WorkspaceStore+Work.swift)

    suspend fun activeFocusSession(): FocusSession? = database.read { activeSession(it) }

    fun observeActiveFocusSession(): Flow<FocusSession?> = database.observe(setOf("focus_sessions")) { activeSession(it) }

    suspend fun focusQueue(sessionId: String): List<FocusQueueTask> = database.read { focusQueue(it, sessionId) }

    fun observeFocusQueue(sessionId: String): Flow<List<FocusQueueTask>> =
        database.observe(setOf("focus_queue_items", "tasks")) { focusQueue(it, sessionId) }

    /** Starts a session on [taskId], or returns the one already running. Not an undo step. */
    /** Starts a focus session on a task, or returns the one running: the Rust core's `focus::start_session`. */
    suspend fun startFocusSession(
        taskId: String,
        plannedSeconds: Int? = null,
        workDurationSeconds: Int = 25 * 60,
        breakDurationSeconds: Int = 5 * 60,
        context: FocusContext? = null,
        overrideAvailability: Boolean = false,
        now: Instant = now(),
    ): FocusSession {
        val id = coreWrite {
            it.startFocusSession(
                taskId, plannedSeconds?.toLong(), workDurationSeconds.toLong(), breakDurationSeconds.toLong(),
                context?.toCore(), overrideAvailability, now.toEpochMilli(), zone.id,
            )
        }
        return database.read { it.session(id) } ?: fail(WorkspaceStoreError.NO_ACTIVE_FOCUS_TASK)
    }

    /**
     * Starts a timer on a task — a block with no length, counting up until it
     * is finished — or returns the session running: `focus::start_timer`.
     */
    suspend fun startFocusTimer(
        taskId: String,
        breakDurationSeconds: Int = 5 * 60,
        context: FocusContext? = null,
        overrideAvailability: Boolean = false,
        now: Instant = now(),
    ): FocusSession {
        val id = coreWrite {
            it.startFocusTimer(
                taskId, breakDurationSeconds.toLong(), context?.toCore(), overrideAvailability, now.toEpochMilli(), zone.id,
            )
        }
        return database.read { it.session(id) } ?: fail(WorkspaceStoreError.NO_ACTIVE_FOCUS_TASK)
    }

    suspend fun addToFocusQueue(sessionId: String, taskId: String, plannedSeconds: Int? = null, now: Instant = now()) {
        coreWrite { it.addToFocusQueue(sessionId, taskId, plannedSeconds?.toLong(), now.toEpochMilli()) }
    }

    /**
     * Finishes the current block, crediting [elapsedSeconds]. A daily due today
     * keeps its task open and logs a contribution; otherwise the task completes
     * (when [completeTask]). [qualityMultiplier] scores the block.
     */
    /** Finishes the block in hand: the Rust core's `focus::finish_block`. */
    suspend fun completeActiveFocusTask(
        sessionId: String,
        elapsedSeconds: Int = 0,
        qualityMultiplier: Double? = null,
        completeTask: Boolean = true,
        expectedBlockId: String? = null,
        context: FocusContext = FocusContext(),
        now: Instant = now(),
        zone: ZoneId = this.zone,
    ): FocusCompletion {
        val finished = coreWrite {
            it.finishFocusBlock(
                sessionId, elapsedSeconds.toLong(), qualityMultiplier, completeTask, expectedBlockId,
                context.toCore(), now.toEpochMilli(), zone.id,
            )
        }
        return coreRead { core ->
            val session = core.focusSession(sessionId)?.toSession() ?: fail(WorkspaceStoreError.NO_ACTIVE_FOCUS_TASK)
            val award = finished.awardId?.let { id -> core.focusAward(id)?.toAward() }
            val seconds = finished.seconds.toInt()
            val outcome = when (finished.outcome) {
                "taskCompleted" -> FocusCompletionOutcome.TaskCompleted
                "contributionLogged" -> FocusCompletionOutcome.ContributionLogged(seconds)
                else -> FocusCompletionOutcome.ProgressLogged(seconds)
            }
            FocusCompletion(session, outcome, award)
        }
    }

    suspend fun finishFocusSession(id: String, now: Instant = now()) {
        coreWrite { it.finishFocusSession(id, now.toEpochMilli()) }
    }

    suspend fun pauseFocusSession(id: String, now: Instant = now()) {
        coreWrite { it.pauseFocusSession(id, now.toEpochMilli()) }
    }

    suspend fun resumeFocusSession(id: String, now: Instant = now()) {
        coreWrite { it.resumeFocusSession(id, now.toEpochMilli()) }
    }

    suspend fun checkpointFocusSession(id: String, now: Instant = now()) {
        coreWrite { it.checkpointFocusSession(id, now.toEpochMilli()) }
    }

    /** On reopening, keep only checkpointed seconds; the user resumes the paused block explicitly. */
    suspend fun recoverInterruptedFocus() {
        coreWrite { it.recoverInterruptedFocus() }
    }

    /** Resumes a queue whose remaining entries were blocked at the last handoff. */
    suspend fun resumeEligibleFocusQueue(context: FocusContext, now: Instant = now()) {
        coreWrite { it.resumeEligibleFocusQueue(context.toCore(), now.toEpochMilli(), zone.id) }
    }

    suspend fun rebaseFocusClock(id: String, elapsedSeconds: Int, now: Instant) {
        coreWrite { it.rebaseFocusClock(id, elapsedSeconds.toLong(), now.toEpochMilli()) }
    }

    /**
     * Settles a session left paused on an earlier logical day: closes (crediting
     * at the pause) or discards it. The Rust core's `progress::resolve_stale_session`
     * reads the session, applies `StaleFocusPolicy` and writes the outcome.
     */
    suspend fun resolveStaleFocusSession(
        now: Instant = now(),
        boundary: DayBoundary = DayBoundary(zone = zone),
        context: FocusContext = FocusContext(),
    ): StaleFocusResolution = coreWrite {
        it.resolveStaleFocusSession(now.coreMillis, boundary.zone.coreName, boundary.rolloverHour.toUByte(), context.toCore())
    }.resolution

    /** Seconds of focused work per task, across renames and deletions. */
    suspend fun loggedWorkTotals(): Map<String, Int> = database.read { loggedWorkTotals(it) }

    suspend fun workBlocks(taskId: String): List<FocusWorkBlock> =
        coreRead { core -> core.workBlocksForTask(taskId).map { it.toWorkBlock() } }

    /** Blocks recorded in `[start, end)`. */
    suspend fun focusWorkBlocks(start: Instant, end: Instant): List<FocusWorkBlock> =
        database.read { workBlocksIn(it, start, end) }

    /** When each (non-list) task in `[start, end)` was closed. */
    suspend fun taskCompletions(start: Instant, end: Instant): List<Instant> = database.read { completionsIn(it, start, end) }

    /** When each (non-list) task in `[start, end)` was added. */
    suspend fun taskCreations(start: Instant, end: Instant): List<Instant> =
        coreRead { it.taskCreationsBetween(start.coreMillis, end.coreMillis).map(Instant::ofEpochMilli) }

    /** Tasks closed since [since], newest first; lists left out, cancellations kept. */
    suspend fun completedTasks(since: Instant, limit: Int = 300): List<WorkspaceTask> =
        coreRead { core -> core.completedTasksSince(since.coreMillis, limit.toLong()).map { it.toTask() } }

    /** Today measured against the week it is part of. */
    /** Today against its week; [firstWeekday] (1 = Sunday) is Swift's `Calendar.firstWeekday`. */
    suspend fun workProgress(
        now: Instant = now(),
        zone: ZoneId = this.zone,
        firstWeekday: Int = defaultFirstWeekday(),
    ): WorkProgress = database.read { workProgress(it, now, zone, firstWeekday) }

    fun observeWorkProgress(zone: ZoneId = this.zone): Flow<WorkProgress> =
        database.observe(setOf("tasks", "focus_work_blocks")) { workProgress(it, now(), zone) }

    // endregion

    // region Points (WorkspaceStore+Points.swift)

    /** Most recent first. */
    suspend fun focusAwards(limit: Int = 50): List<FocusAward> =
        coreRead { core -> core.recentFocusAwards(limit.toLong()).map { it.toAward() } }

    suspend fun focusAwards(onDayOf: Instant, zone: ZoneId = this.zone): List<FocusAward> {
        val day = onDayOf.atZone(zone).toLocalDate()
        val start = day.atStartOfDay(zone).toInstant()
        val end = day.plusDays(1).atStartOfDay(zone).toInstant()
        return coreRead { core -> core.focusAwardsBetween(start.coreMillis, end.coreMillis).map { it.toAward() } }
    }

    /** Today, the trailing seven calendar days, and all time, from one read. */
    suspend fun focusPointsSummary(now: Instant = now(), zone: ZoneId = this.zone): FocusPointsSummary =
        database.read { pointsSummary(it, now, zone) }

    fun observeFocusPointsSummary(zone: ZoneId = this.zone): Flow<FocusPointsSummary> =
        database.observe(setOf("focus_awards")) { pointsSummary(it, now(), zone) }

    // endregion

    // region Search (WorkspaceStore+Search.swift)

    /** Prefix search over titles (weighted 10x) and notes, through FTS5. */
    suspend fun searchTasks(
        workspaceId: String,
        query: String,
        includingCompleted: Boolean = false,
        includingArchivedLists: Boolean = false,
        limit: Int = 60,
    ): List<TaskSearchResult> {
        // The Rust core's `search::search`, which the Mac and iPhone call too.
        return database.coreRead { core ->
            core.searchTasks(workspaceId, query, includingCompleted, includingArchivedLists, limit.toLong()).map {
                TaskSearchResult(it.task.toTask(), it.list.toList(), it.notesSnippet)
            }
        }
    }

    // endregion

    // region The day and the ladder (WorkspaceNextUpSnapshot.swift)

    /**
     * Everything the day and the focus ladder are drawn from, in one read.
     * With a [ladderLimit] the ladder holds only its first so many entries
     * plus every task in the day further down, as the iPhone's Today and
     * widget ask; the focus screen asks for all of it.
     */
    suspend fun nextUpSnapshot(
        workspaceId: String?,
        context: FocusContext,
        runningId: String?,
        now: Instant = now(),
        zone: ZoneId = this.zone,
        ladderLimit: Int? = null,
    ): WorkspaceNextUpSnapshot =
        database.read { nextUpSnapshot(it, database.core, workspaceId, context, runningId, now, zone, ladderLimit) }

    /** The snapshot, rebuilt whenever any of the tables behind it change. */
    fun observeNextUpSnapshot(
        workspaceId: String?,
        context: FocusContext,
        runningId: String?,
        zone: ZoneId = this.zone,
        ladderLimit: Int? = null,
    ): Flow<WorkspaceNextUpSnapshot> = database.observe(
        setOf(
            "tasks", "task_lists", "task_metadata", "dailies", "daily_contributions", "focus_work_blocks",
            "task_conditions", "task_dependencies",
        ),
    ) { nextUpSnapshot(it, database.core, workspaceId, context, runningId, now(), zone, ladderLimit) }

    // endregion

    // region Counts (for overview screens)

    /** Open and total task counts per list, for a sidebar or a home screen. */
    fun observeTaskCounts(): Flow<TaskCounts> = database.observe(setOf("tasks")) { db ->
        val counts = db.core.taskCounts()
        TaskCounts(
            open = counts.open.toInt(),
            completed = counts.completed.toInt(),
            byList = counts.byList.associate { it.listId to it.open.toInt() },
        )
    }

    // endregion

    companion object {
        const val JOURNAL_DEPTH = 100

        /** What an undo or redo can change, announced to observers since the core writes on its own connection. */
        private val REPLAYED_TABLES: Set<String> =
            WorkspaceSchema.journalledTables.map { it.first }.toSet() + setOf("change_log", "sync_outbox")

        /**
         * What a write the core makes can change, cascades included. Every table the
         * workspace syncs, plus the journal and the outbox: broader than any one write
         * needs, so an observer may re-read once for nothing, but never misses a change.
         */
        private val CORE_WRITTEN_TABLES: Set<String> =
            WorkspaceSchema.syncedTables.map { it.first }.toSet() + setOf("change_log", "sync_outbox")

        /** Opens the workspace database at [path] and wraps it. */
        fun open(path: String, clock: Clock = Clock.systemUTC()): WorkspaceRepository =
            WorkspaceRepository(WorkspaceDatabase.open(path), clock)
    }
}

data class HistoryTarget(val taskId: String?, val listId: String?)

data class HistoryEntry(val groupId: String, val label: String?, val isUndone: Boolean, val changeCount: Int)

data class BoardMetadata(val columns: Map<String, String>, val positions: Map<String, TaskMatrixPosition>)

internal fun emptyMetadata(taskId: String, now: Instant) = TaskMetadata(
    taskId = taskId, priority = null, startAt = null, tagsJSON = "[]", recurrenceRule = null, matrixUrgency = null,
    matrixImportance = null, kanbanColumn = null, externalLinksJSON = "[]", focusRank = null, updatedAt = now,
)

/** What holds when nothing has been said: no conditions, no limits, making progress. */
fun emptyFocusContext(): uniffi.takt_core.StoredFocusContext = uniffi.takt_core.StoredFocusContext(
    conditionIds = emptyList(),
    conditionsSetAtMs = null,
    availableUntilMs = null,
    expiresAtMs = null,
    mode = "progress",
    suggestedIds = emptyList(),
    offIds = emptyList(),
)
