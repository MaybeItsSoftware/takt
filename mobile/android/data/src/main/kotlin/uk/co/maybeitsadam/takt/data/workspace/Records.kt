package uk.co.maybeitsadam.takt.data.workspace

import uk.co.maybeitsadam.takt.core.DailyContribution
import uk.co.maybeitsadam.takt.core.DailyItem
import uk.co.maybeitsadam.takt.core.FocusAward
import uk.co.maybeitsadam.takt.core.FocusQueueItem
import uk.co.maybeitsadam.takt.core.FocusQueueState
import uk.co.maybeitsadam.takt.core.FocusQueueTask
import uk.co.maybeitsadam.takt.core.FocusSession
import uk.co.maybeitsadam.takt.core.FocusSessionPhase
import uk.co.maybeitsadam.takt.core.FocusWorkBlock
import uk.co.maybeitsadam.takt.core.ListFolder
import uk.co.maybeitsadam.takt.core.TaskCondition
import uk.co.maybeitsadam.takt.core.TaskList
import uk.co.maybeitsadam.takt.core.TaskListRole
import uk.co.maybeitsadam.takt.core.TaskMetadata
import uk.co.maybeitsadam.takt.core.TaskStatus
import uk.co.maybeitsadam.takt.core.Workspace
import uk.co.maybeitsadam.takt.core.WorkspaceDaily
import uk.co.maybeitsadam.takt.core.WorkspaceItemKind
import uk.co.maybeitsadam.takt.core.WorkspaceTask
import uk.co.maybeitsadam.takt.data.db.Db
import uk.co.maybeitsadam.takt.data.db.Row
import uniffi.takt_core.AwardRow
import uniffi.takt_core.ConditionRow
import uniffi.takt_core.ContributionRow
import uniffi.takt_core.DailyRow
import uniffi.takt_core.DayDaily
import uniffi.takt_core.FolderRow
import uniffi.takt_core.MetadataRow
import uniffi.takt_core.QueueEntry
import uniffi.takt_core.SessionRow
import uniffi.takt_core.WorkBlockRow
import uniffi.takt_core.ListRow
import uniffi.takt_core.TaskRow
import uniffi.takt_core.WorkspaceRow
import java.time.Instant

// The GRDB records' fetch/insert/update, by hand. `update` writes every
// column like GRDB's `record.update(db)`; the triggers then record only what
// actually differs.

internal fun Row.toWorkspace() = Workspace(
    id = string("id"), name = string("name"), createdAt = instant("createdAt"), updatedAt = instant("updatedAt"),
)

internal fun Row.toFolder() = ListFolder(
    id = string("id"), workspaceId = string("workspaceId"), parentFolderId = stringOrNull("parentFolderId"),
    name = string("name"), sortOrder = int("sortOrder"), createdAt = instant("createdAt"),
    updatedAt = instant("updatedAt"),
)

internal fun Row.toList() = TaskList(
    id = string("id"), workspaceId = string("workspaceId"), folderId = stringOrNull("folderId"),
    name = string("name"), colorHex = stringOrNull("colorHex"), sortOrder = int("sortOrder"),
    isArchived = bool("isArchived"), systemRole = TaskListRole.of(stringOrNull("systemRole")),
    visibleRootTaskId = stringOrNull("visibleRootTaskId"), completedAt = instantOrNull("completedAt"),
    createdAt = instant("createdAt"), updatedAt = instant("updatedAt"),
)

internal fun Row.toTask() = WorkspaceTask(
    id = string("id"), listId = string("listId"), parentTaskId = stringOrNull("parentTaskId"),
    title = string("title"), notes = string("notes"), status = TaskStatus.of(string("status")),
    sortOrder = int("sortOrder"), dueAt = instantOrNull("dueAt"), estimateSeconds = intOrNull("estimateSeconds"),
    sourceSystem = stringOrNull("sourceSystem"), sourceId = stringOrNull("sourceId"),
    itemKind = WorkspaceItemKind.of(stringOrNull("itemKind")), isPromoted = boolOrNull("isPromoted"),
    archivedAt = instantOrNull("archivedAt"), completedAt = instantOrNull("completedAt"),
    createdAt = instant("createdAt"), updatedAt = instant("updatedAt"),
)

internal fun Row.toMetadata() = TaskMetadata(
    taskId = string("taskId"), priority = intOrNull("priority"), startAt = instantOrNull("startAt"),
    tagsJSON = string("tagsJSON"), recurrenceRule = stringOrNull("recurrenceRule"),
    matrixUrgency = intOrNull("matrixUrgency"), matrixImportance = intOrNull("matrixImportance"),
    kanbanColumn = stringOrNull("kanbanColumn"), externalLinksJSON = string("externalLinksJSON"),
    focusRank = intOrNull("focusRank"), updatedAt = instant("updatedAt"), planningJSON = stringOrNull("planningJSON"),
    waitingOn = stringOrNull("waitingOn"), waitingFollowUpAt = instantOrNull("waitingFollowUpAt"),
    waitingFollowUpTaskId = stringOrNull("waitingFollowUpTaskId"), followUpOfTaskId = stringOrNull("followUpOfTaskId"),
    matrixRank = intOrNull("matrixRank"),
    waitingFollowUpEveryDays = intOrNull("waitingFollowUpEveryDays"),
    waitingFollowUpCount = intOrNull("waitingFollowUpCount") ?: 0,
)

internal fun Row.toSession() = FocusSession(
    id = string("id"), startedAt = instant("startedAt"), endedAt = instantOrNull("endedAt"),
    phase = FocusSessionPhase.of(string("phase")), activeTaskId = stringOrNull("activeTaskId"),
    activeTaskStartedAt = instant("activeTaskStartedAt"), workDurationSeconds = int("workDurationSeconds"),
    breakDurationSeconds = int("breakDurationSeconds"), breakEndsAt = instantOrNull("breakEndsAt"),
    activeBlockId = stringOrNull("activeBlockId"), accumulatedSeconds = intOrNull("accumulatedSeconds"),
    pausedAt = instantOrNull("pausedAt"), checkpointAt = instantOrNull("checkpointAt"),
)

internal fun Row.toQueueItem() = FocusQueueItem(
    id = string("id"), sessionId = string("sessionId"), taskId = string("taskId"), sortOrder = int("sortOrder"),
    state = FocusQueueState.of(string("state")), plannedSeconds = intOrNull("plannedSeconds"),
    completedAt = instantOrNull("completedAt"), skippedAt = instantOrNull("skippedAt"),
    createdAt = instant("createdAt"),
)

internal fun Row.toDaily() = WorkspaceDaily(
    id = string("id"), taskId = string("taskId"), activeWeekdaysMask = int("activeWeekdaysMask"),
    intervalDays = intOrNull("intervalDays"), intervalAnchor = instantOrNull("intervalAnchor"),
    targetSeconds = intOrNull("targetSeconds"), sortOrder = int("sortOrder"), archivedAt = instantOrNull("archivedAt"),
    legacyDailyId = stringOrNull("legacyDailyId"), createdAt = instant("createdAt"), updatedAt = instant("updatedAt"),
    sourceTaskId = stringOrNull("sourceTaskId"), placementColumn = stringOrNull("placementColumn"),
    dropsAtDayEnd = bool("dropsAtDayEnd"), expiryRule = string("expiryRule"), expiresAt = instantOrNull("expiresAt"),
)

internal fun Row.toContribution() = DailyContribution(
    id = string("id"), dailyId = string("dailyId"), taskId = string("taskId"), dayKey = string("dayKey"),
    secondsLogged = int("secondsLogged"), completedAt = instantOrNull("completedAt"), createdAt = instant("createdAt"),
)

internal fun Row.toCondition() = TaskCondition(
    id = string("id"), workspaceId = string("workspaceId"), name = string("name"), isLocation = bool("isLocation"),
    isArchived = bool("isArchived"), createdAt = instant("createdAt"), updatedAt = instant("updatedAt"),
    category = stringOrNull("category"),
)

internal fun Row.toWorkBlock() = FocusWorkBlock(
    id = string("id"), sessionId = stringOrNull("sessionId"), taskId = stringOrNull("taskId"),
    taskTitle = string("taskTitle"), seconds = int("seconds"), recordedAt = instant("recordedAt"),
    originalTaskId = stringOrNull("originalTaskId"),
)

internal fun Row.toAward() = FocusAward(
    id = string("id"), sessionId = stringOrNull("sessionId"), taskId = stringOrNull("taskId"),
    taskTitle = string("taskTitle"), seconds = int("seconds"), minutes = double("minutes"),
    multiplier = double("multiplier"), points = double("points"), awardedAt = instant("awardedAt"),
)

// The Rust core's rows (core/src/records.rs) as the records the screens use.

private fun Long.instant(): Instant = Instant.ofEpochMilli(this)

internal fun WorkspaceRow.toWorkspace() = Workspace(
    id = id, name = name, createdAt = createdAtMs.instant(), updatedAt = updatedAtMs.instant(),
)

internal fun FolderRow.toFolder() = ListFolder(
    id = id, workspaceId = workspaceId, parentFolderId = parentFolderId, name = name, sortOrder = sortOrder.toInt(),
    createdAt = createdAtMs.instant(), updatedAt = updatedAtMs.instant(),
)

internal fun ListRow.toList() = TaskList(
    id = id, workspaceId = workspaceId, folderId = folderId, name = name, colorHex = colorHex,
    sortOrder = sortOrder.toInt(), isArchived = isArchived, systemRole = TaskListRole.of(systemRole),
    visibleRootTaskId = visibleRootTaskId, completedAt = completedAtMs?.instant(),
    createdAt = createdAtMs.instant(), updatedAt = updatedAtMs.instant(),
)

internal fun TaskRow.toTask() = WorkspaceTask(
    id = id, listId = listId, parentTaskId = parentTaskId, title = title, notes = notes,
    status = TaskStatus.of(status), sortOrder = sortOrder.toInt(), dueAt = dueAtMs?.instant(),
    estimateSeconds = estimateSeconds?.toInt(), sourceSystem = sourceSystem, sourceId = sourceId,
    itemKind = WorkspaceItemKind.of(itemKind), isPromoted = isPromoted, archivedAt = archivedAtMs?.instant(),
    completedAt = completedAtMs?.instant(), createdAt = createdAtMs.instant(), updatedAt = updatedAtMs.instant(),
)

internal fun DailyRow.toDaily() = WorkspaceDaily(
    id = id, taskId = taskId, activeWeekdaysMask = activeWeekdaysMask.toInt(), intervalDays = intervalDays?.toInt(),
    intervalAnchor = intervalAnchorMs?.instant(), targetSeconds = targetSeconds?.toInt(), sortOrder = sortOrder.toInt(),
    archivedAt = archivedAtMs?.instant(), legacyDailyId = legacyDailyId, createdAt = createdAtMs.instant(),
    updatedAt = updatedAtMs.instant(), sourceTaskId = sourceTaskId, placementColumn = placementColumn,
    dropsAtDayEnd = dropsAtDayEnd, expiryRule = expiryRule, expiresAt = expiresAtMs?.instant(),
)

internal fun ContributionRow.toContribution() = DailyContribution(
    id = id, dailyId = dailyId, taskId = taskId, dayKey = dayKey, secondsLogged = secondsLogged.toInt(),
    completedAt = completedAtMs?.instant(), createdAt = createdAtMs.instant(),
)

internal fun DayDaily.toItem() = DailyItem(daily.toDaily(), task.toTask(), contribution?.toContribution())

internal fun ConditionRow.toCondition() = TaskCondition(
    id = id, workspaceId = workspaceId, name = name, isLocation = isLocation, isArchived = isArchived,
    createdAt = createdAtMs.instant(), updatedAt = updatedAtMs.instant(), category = category,
    sortOrder = sortOrder, provides = provides,
)

internal fun TaskCondition.toRow() = ConditionRow(
    id = id, workspaceId = workspaceId, name = name, isLocation = isLocation, isArchived = isArchived,
    createdAtMs = createdAt.toEpochMilli(), updatedAtMs = updatedAt.toEpochMilli(), category = category,
    sortOrder = sortOrder, provides = provides,
)

internal fun MetadataRow.toMetadata() = TaskMetadata(
    taskId = taskId, priority = priority?.toInt(), startAt = startAtMs?.instant(), tagsJSON = tagsJson,
    recurrenceRule = recurrenceRule, matrixUrgency = matrixUrgency?.toInt(), matrixImportance = matrixImportance?.toInt(),
    kanbanColumn = kanbanColumn, externalLinksJSON = externalLinksJson, focusRank = focusRank?.toInt(),
    updatedAt = updatedAtMs.instant(), planningJSON = planningJson, waitingOn = waitingOn,
    waitingFollowUpAt = waitingFollowUpAtMs?.instant(), waitingFollowUpTaskId = waitingFollowUpTaskId,
    followUpOfTaskId = followUpOfTaskId, matrixRank = matrixRank?.toInt(),
    waitingFollowUpEveryDays = waitingFollowUpEveryDays?.toInt(), waitingFollowUpCount = waitingFollowUpCount.toInt(),
)

internal fun SessionRow.toSession() = FocusSession(
    id = id, startedAt = startedAtMs.instant(), endedAt = endedAtMs?.instant(), phase = FocusSessionPhase.of(phase),
    activeTaskId = activeTaskId, activeTaskStartedAt = activeTaskStartedAtMs.instant(),
    workDurationSeconds = workDurationSeconds.toInt(), breakDurationSeconds = breakDurationSeconds.toInt(),
    breakEndsAt = breakEndsAtMs?.instant(), activeBlockId = activeBlockId, accumulatedSeconds = accumulatedSeconds?.toInt(),
    pausedAt = pausedAtMs?.instant(), checkpointAt = checkpointAtMs?.instant(), openEnded = openEnded,
)

internal fun QueueEntry.toQueueTask() = FocusQueueTask(
    FocusQueueItem(
        id = item.id, sessionId = item.sessionId, taskId = item.taskId, sortOrder = item.sortOrder.toInt(),
        state = FocusQueueState.of(item.state), plannedSeconds = item.plannedSeconds?.toInt(),
        completedAt = item.completedAtMs?.instant(), skippedAt = item.skippedAtMs?.instant(),
        createdAt = item.createdAtMs.instant(), openEnded = item.openEnded,
    ),
    task.toTask(),
)

internal fun WorkBlockRow.toWorkBlock() = FocusWorkBlock(
    id = id, sessionId = sessionId, taskId = taskId, taskTitle = taskTitle, seconds = seconds.toInt(),
    recordedAt = recordedAtMs.instant(), originalTaskId = originalTaskId,
)

internal fun AwardRow.toAward() = FocusAward(
    id = id, sessionId = sessionId, taskId = taskId, taskTitle = taskTitle, seconds = seconds.toInt(),
    minutes = minutes, multiplier = multiplier, points = points, awardedAt = awardedAtMs.instant(),
)

// Fetch by key.

internal fun Db.workspace(id: String) = queryOne("SELECT * FROM workspaces WHERE id = ?", id) { it.toWorkspace() }
internal fun Db.folder(id: String) = queryOne("SELECT * FROM list_folders WHERE id = ?", id) { it.toFolder() }
internal fun Db.list(id: String) = core.list(id)?.toList()
internal fun Db.task(id: String) = core.task(id)?.toTask()
internal fun Db.metadata(taskId: String) = core.metadata(taskId)?.toMetadata()
internal fun Db.session(id: String) = core.focusSession(id)?.toSession()
internal fun Db.daily(id: String) = core.daily(id)?.toDaily()
internal fun Db.condition(id: String) = core.condition(id)?.toCondition()
internal fun Db.workBlockExists(id: String) = exists("SELECT 1 FROM focus_work_blocks WHERE id = ?", id)

/** Siblings in GRDB's `.order(sortOrder, createdAt, id)`. */
internal fun Db.taskSiblings(listId: String, parentTaskId: String?, withId: Boolean = true): List<WorkspaceTask> =
    query(
        "SELECT * FROM tasks WHERE listId = ? AND parentTaskId IS ? ORDER BY sortOrder, createdAt" +
            if (withId) ", id" else "",
        listId, parentTaskId,
    ) { it.toTask() }

// Insert / update.

internal fun Db.insert(w: Workspace) = execute(
    "INSERT INTO workspaces (id, name, createdAt, updatedAt) VALUES (?, ?, ?, ?)",
    w.id, w.name, w.createdAt, w.updatedAt,
)

internal fun Db.insert(f: ListFolder) = execute(
    "INSERT INTO list_folders (id, workspaceId, parentFolderId, name, sortOrder, createdAt, updatedAt) " +
        "VALUES (?, ?, ?, ?, ?, ?, ?)",
    f.id, f.workspaceId, f.parentFolderId, f.name, f.sortOrder, f.createdAt, f.updatedAt,
)

internal fun Db.update(f: ListFolder) = execute(
    "UPDATE list_folders SET workspaceId = ?, parentFolderId = ?, name = ?, sortOrder = ?, createdAt = ?, " +
        "updatedAt = ? WHERE id = ?",
    f.workspaceId, f.parentFolderId, f.name, f.sortOrder, f.createdAt, f.updatedAt, f.id,
)

internal fun Db.insert(l: TaskList) = execute(
    "INSERT INTO task_lists (id, workspaceId, folderId, name, colorHex, sortOrder, isArchived, systemRole, " +
        "visibleRootTaskId, completedAt, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    l.id, l.workspaceId, l.folderId, l.name, l.colorHex, l.sortOrder, l.isArchived, l.systemRole?.raw,
    l.visibleRootTaskId, l.completedAt, l.createdAt, l.updatedAt,
)

internal fun Db.update(l: TaskList) = execute(
    "UPDATE task_lists SET workspaceId = ?, folderId = ?, name = ?, colorHex = ?, sortOrder = ?, isArchived = ?, " +
        "systemRole = ?, visibleRootTaskId = ?, completedAt = ?, createdAt = ?, updatedAt = ? WHERE id = ?",
    l.workspaceId, l.folderId, l.name, l.colorHex, l.sortOrder, l.isArchived, l.systemRole?.raw,
    l.visibleRootTaskId, l.completedAt, l.createdAt, l.updatedAt, l.id,
)

internal fun Db.insert(t: WorkspaceTask) = execute(
    "INSERT INTO tasks (id, listId, parentTaskId, title, notes, status, sortOrder, dueAt, estimateSeconds, " +
        "createdAt, updatedAt, sourceSystem, sourceId, itemKind, isPromoted, archivedAt, completedAt) " +
        "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    t.id, t.listId, t.parentTaskId, t.title, t.notes, t.status.raw, t.sortOrder, t.dueAt, t.estimateSeconds,
    t.createdAt, t.updatedAt, t.sourceSystem, t.sourceId, t.itemKind?.raw, t.isPromoted, t.archivedAt, t.completedAt,
)

internal fun Db.update(t: WorkspaceTask) = execute(
    "UPDATE tasks SET listId = ?, parentTaskId = ?, title = ?, notes = ?, status = ?, sortOrder = ?, dueAt = ?, " +
        "estimateSeconds = ?, createdAt = ?, updatedAt = ?, sourceSystem = ?, sourceId = ?, itemKind = ?, " +
        "isPromoted = ?, archivedAt = ?, completedAt = ? WHERE id = ?",
    t.listId, t.parentTaskId, t.title, t.notes, t.status.raw, t.sortOrder, t.dueAt, t.estimateSeconds,
    t.createdAt, t.updatedAt, t.sourceSystem, t.sourceId, t.itemKind?.raw, t.isPromoted, t.archivedAt,
    t.completedAt, t.id,
)

internal fun Db.insert(m: TaskMetadata) = execute(
    "INSERT INTO task_metadata (taskId, priority, startAt, tagsJSON, recurrenceRule, matrixUrgency, " +
        "matrixImportance, kanbanColumn, externalLinksJSON, updatedAt, focusRank, planningJSON, waitingOn, " +
        "waitingFollowUpAt, waitingFollowUpTaskId, followUpOfTaskId) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    m.taskId, m.priority, m.startAt, m.tagsJSON, m.recurrenceRule, m.matrixUrgency, m.matrixImportance,
    m.kanbanColumn, m.externalLinksJSON, m.updatedAt, m.focusRank, m.planningJSON, m.waitingOn, m.waitingFollowUpAt,
    m.waitingFollowUpTaskId, m.followUpOfTaskId,
)

internal fun Db.update(m: TaskMetadata) = execute(
    "UPDATE task_metadata SET priority = ?, startAt = ?, tagsJSON = ?, recurrenceRule = ?, matrixUrgency = ?, " +
        "matrixImportance = ?, kanbanColumn = ?, externalLinksJSON = ?, updatedAt = ?, focusRank = ?, " +
        "planningJSON = ?, waitingOn = ?, waitingFollowUpAt = ?, waitingFollowUpTaskId = ?, followUpOfTaskId = ? " +
        "WHERE taskId = ?",
    m.priority, m.startAt, m.tagsJSON, m.recurrenceRule, m.matrixUrgency, m.matrixImportance, m.kanbanColumn,
    m.externalLinksJSON, m.updatedAt, m.focusRank, m.planningJSON, m.waitingOn, m.waitingFollowUpAt,
    m.waitingFollowUpTaskId, m.followUpOfTaskId, m.taskId,
)

/** GRDB's `save`: update when the row exists, insert otherwise. */
internal fun Db.save(m: TaskMetadata) {
    if (exists("SELECT 1 FROM task_metadata WHERE taskId = ?", m.taskId)) update(m) else insert(m)
}

internal fun Db.insert(s: FocusSession) = execute(
    "INSERT INTO focus_sessions (id, startedAt, endedAt, phase, activeTaskId, workDurationSeconds, " +
        "breakDurationSeconds, breakEndsAt, activeTaskStartedAt, activeBlockId, accumulatedSeconds, pausedAt, " +
        "checkpointAt) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    s.id, s.startedAt, s.endedAt, s.phase.raw, s.activeTaskId, s.workDurationSeconds, s.breakDurationSeconds,
    s.breakEndsAt, s.activeTaskStartedAt, s.activeBlockId, s.accumulatedSeconds, s.pausedAt, s.checkpointAt,
)

internal fun Db.update(s: FocusSession) = execute(
    "UPDATE focus_sessions SET startedAt = ?, endedAt = ?, phase = ?, activeTaskId = ?, workDurationSeconds = ?, " +
        "breakDurationSeconds = ?, breakEndsAt = ?, activeTaskStartedAt = ?, activeBlockId = ?, " +
        "accumulatedSeconds = ?, pausedAt = ?, checkpointAt = ? WHERE id = ?",
    s.startedAt, s.endedAt, s.phase.raw, s.activeTaskId, s.workDurationSeconds, s.breakDurationSeconds,
    s.breakEndsAt, s.activeTaskStartedAt, s.activeBlockId, s.accumulatedSeconds, s.pausedAt, s.checkpointAt, s.id,
)

internal fun Db.insert(q: FocusQueueItem) = execute(
    "INSERT INTO focus_queue_items (id, sessionId, taskId, sortOrder, state, completedAt, skippedAt, createdAt, " +
        "plannedSeconds) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    q.id, q.sessionId, q.taskId, q.sortOrder, q.state.raw, q.completedAt, q.skippedAt, q.createdAt, q.plannedSeconds,
)

internal fun Db.update(q: FocusQueueItem) = execute(
    "UPDATE focus_queue_items SET sessionId = ?, taskId = ?, sortOrder = ?, state = ?, completedAt = ?, " +
        "skippedAt = ?, createdAt = ?, plannedSeconds = ? WHERE id = ?",
    q.sessionId, q.taskId, q.sortOrder, q.state.raw, q.completedAt, q.skippedAt, q.createdAt, q.plannedSeconds, q.id,
)

internal fun Db.insert(d: WorkspaceDaily) = execute(
    "INSERT INTO dailies (id, taskId, activeWeekdaysMask, intervalDays, intervalAnchor, targetSeconds, sortOrder, " +
        "archivedAt, legacyDailyId, createdAt, updatedAt, sourceTaskId, placementColumn, dropsAtDayEnd, " +
        "expiryRule, expiresAt) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    d.id, d.taskId, d.activeWeekdaysMask, d.intervalDays, d.intervalAnchor, d.targetSeconds, d.sortOrder,
    d.archivedAt, d.legacyDailyId, d.createdAt, d.updatedAt, d.sourceTaskId, d.placementColumn, d.dropsAtDayEnd,
    d.expiryRule, d.expiresAt,
)

internal fun Db.update(d: WorkspaceDaily) = execute(
    "UPDATE dailies SET taskId = ?, activeWeekdaysMask = ?, intervalDays = ?, intervalAnchor = ?, " +
        "targetSeconds = ?, sortOrder = ?, archivedAt = ?, legacyDailyId = ?, createdAt = ?, updatedAt = ?, " +
        "sourceTaskId = ?, placementColumn = ?, dropsAtDayEnd = ?, expiryRule = ?, expiresAt = ? WHERE id = ?",
    d.taskId, d.activeWeekdaysMask, d.intervalDays, d.intervalAnchor, d.targetSeconds, d.sortOrder, d.archivedAt,
    d.legacyDailyId, d.createdAt, d.updatedAt, d.sourceTaskId, d.placementColumn, d.dropsAtDayEnd, d.expiryRule,
    d.expiresAt, d.id,
)

internal fun Db.insert(c: DailyContribution) = execute(
    "INSERT INTO daily_contributions (id, dailyId, taskId, dayKey, secondsLogged, completedAt, createdAt) " +
        "VALUES (?, ?, ?, ?, ?, ?, ?)",
    c.id, c.dailyId, c.taskId, c.dayKey, c.secondsLogged, c.completedAt, c.createdAt,
)

internal fun Db.update(c: DailyContribution) = execute(
    "UPDATE daily_contributions SET dailyId = ?, taskId = ?, dayKey = ?, secondsLogged = ?, completedAt = ?, " +
        "createdAt = ? WHERE id = ?",
    c.dailyId, c.taskId, c.dayKey, c.secondsLogged, c.completedAt, c.createdAt, c.id,
)

internal fun Db.insert(c: TaskCondition) = execute(
    "INSERT INTO task_conditions (id, workspaceId, name, isLocation, isArchived, createdAt, updatedAt, category) " +
        "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    c.id, c.workspaceId, c.name, c.isLocation, c.isArchived, c.createdAt, c.updatedAt, c.category,
)

internal fun Db.update(c: TaskCondition) = execute(
    "UPDATE task_conditions SET workspaceId = ?, name = ?, isLocation = ?, isArchived = ?, createdAt = ?, " +
        "updatedAt = ?, category = ? WHERE id = ?",
    c.workspaceId, c.name, c.isLocation, c.isArchived, c.createdAt, c.updatedAt, c.category, c.id,
)

internal fun Db.insert(b: FocusWorkBlock) = execute(
    "INSERT INTO focus_work_blocks (id, sessionId, taskId, taskTitle, seconds, recordedAt, originalTaskId) " +
        "VALUES (?, ?, ?, ?, ?, ?, ?)",
    b.id, b.sessionId, b.taskId, b.taskTitle, b.seconds, b.recordedAt, b.originalTaskId,
)

internal fun Db.insert(a: FocusAward) = execute(
    "INSERT INTO focus_awards (id, sessionId, taskId, taskTitle, seconds, minutes, multiplier, points, awardedAt) " +
        "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    a.id, a.sessionId, a.taskId, a.taskTitle, a.seconds, a.minutes, a.multiplier, a.points, a.awardedAt,
)
