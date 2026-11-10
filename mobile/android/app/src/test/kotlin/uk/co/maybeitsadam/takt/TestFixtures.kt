package uk.co.maybeitsadam.takt

import java.time.Instant
import uk.co.maybeitsadam.takt.core.FocusSession
import uk.co.maybeitsadam.takt.core.FocusSessionPhase
import uk.co.maybeitsadam.takt.core.TaskStatus
import uk.co.maybeitsadam.takt.core.WorkspaceTask

val T0: Instant = Instant.parse("2026-10-02T09:00:00Z")

fun task(id: String, title: String = id, estimate: Int? = null, listId: String = "L") = WorkspaceTask(
    id = id, listId = listId, parentTaskId = null, title = title, notes = "", status = TaskStatus.OPEN, sortOrder = 0,
    dueAt = null, estimateSeconds = estimate, sourceSystem = null, sourceId = null, itemKind = null, isPromoted = null,
    archivedAt = null, createdAt = T0, updatedAt = T0,
)

fun session(
    taskId: String?,
    startedAt: Instant = T0,
    accumulated: Int? = null,
    pausedAt: Instant? = null,
    planned: Int = 1500,
    phase: FocusSessionPhase = FocusSessionPhase.RUNNING,
    openEnded: Boolean = false,
) = FocusSession(
    id = "S", startedAt = startedAt, endedAt = null, phase = phase, activeTaskId = taskId, activeTaskStartedAt = startedAt,
    workDurationSeconds = planned, breakDurationSeconds = 300, breakEndsAt = null, activeBlockId = "B",
    accumulatedSeconds = accumulated, pausedAt = pausedAt, checkpointAt = startedAt, openEnded = openEnded,
)
