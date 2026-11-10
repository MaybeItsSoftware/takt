import Foundation
import TaktRustCore

// The rest of the workspace's rows as the Rust core reads them
// (core/src/rows.rs). Each initialiser sets the stored properties directly,
// as decoding a row did, so a normalising initialiser cannot change what was
// stored on the way out.

private extension Optional where Wrapped == Int64 {
  var int: Int? { map { Int($0) } }
  var date: Date? { map(Date.init(coreMilliseconds:)) }
}

extension WorkspaceDaily {
  init(_ row: DailyRow) {
    id = row.id
    taskId = row.taskId
    activeWeekdaysMask = Int(row.activeWeekdaysMask)
    intervalDays = row.intervalDays.int
    intervalAnchor = row.intervalAnchorMs.date
    targetSeconds = row.targetSeconds.int
    sortOrder = Int(row.sortOrder)
    archivedAt = row.archivedAtMs.date
    legacyDailyId = row.legacyDailyId
    createdAt = Date(coreMilliseconds: row.createdAtMs)
    updatedAt = Date(coreMilliseconds: row.updatedAtMs)
    sourceTaskId = row.sourceTaskId
    placementColumn = row.placementColumn
    dropsAtDayEnd = row.dropsAtDayEnd
    expiryRule = row.expiryRule
    expiresAt = row.expiresAtMs.date
  }
}

extension DailyContribution {
  init(_ row: ContributionRow) {
    id = row.id
    dailyId = row.dailyId
    taskId = row.taskId
    dayKey = row.dayKey
    secondsLogged = Int(row.secondsLogged)
    completedAt = row.completedAtMs.date
    createdAt = Date(coreMilliseconds: row.createdAtMs)
    skippedAt = row.skippedAtMs.date
  }
}

extension DailyItem {
  init(_ day: DayDaily) {
    self.init(
      daily: WorkspaceDaily(day.daily), task: WorkspaceTask(day.task),
      contribution: day.contribution.map(DailyContribution.init))
  }
}

extension TaskCondition {
  init(_ row: ConditionRow) {
    id = row.id
    workspaceId = row.workspaceId
    name = row.name
    isLocation = row.isLocation
    isArchived = row.isArchived
    createdAt = Date(coreMilliseconds: row.createdAtMs)
    updatedAt = Date(coreMilliseconds: row.updatedAtMs)
    category = row.category
    sortOrder = row.sortOrder.map(Int.init)
    provides = row.provides
  }

  /// The row the core's condition rules read.
  var core: ConditionRow {
    ConditionRow(
      id: id, workspaceId: workspaceId, name: name, isLocation: isLocation, isArchived: isArchived,
      createdAtMs: createdAt.coreMilliseconds, updatedAtMs: updatedAt.coreMilliseconds, category: category,
      sortOrder: sortOrder.map(Int64.init), provides: provides)
  }
}

extension TaskMetadata {
  init(_ row: MetadataRow) {
    self.init(
      taskId: row.taskId, priority: row.priority.int, startAt: row.startAtMs.date, tagsJSON: row.tagsJson,
      recurrenceRule: row.recurrenceRule, matrixUrgency: row.matrixUrgency.int,
      matrixImportance: row.matrixImportance.int, kanbanColumn: row.kanbanColumn,
      externalLinksJSON: row.externalLinksJson, focusRank: row.focusRank.int,
      updatedAt: Date(coreMilliseconds: row.updatedAtMs))
    planningJSON = row.planningJson
    waitingOn = row.waitingOn
    waitingFollowUpAt = row.waitingFollowUpAtMs.date
    waitingFollowUpTaskId = row.waitingFollowUpTaskId
    followUpOfTaskId = row.followUpOfTaskId
  }
}

extension FocusSession {
  init(_ row: SessionRow) {
    id = row.id
    startedAt = Date(coreMilliseconds: row.startedAtMs)
    endedAt = row.endedAtMs.date
    phase = FocusSessionPhase(rawValue: row.phase) ?? .finished
    activeTaskId = row.activeTaskId
    activeTaskStartedAt = Date(coreMilliseconds: row.activeTaskStartedAtMs)
    workDurationSeconds = Int(row.workDurationSeconds)
    breakDurationSeconds = Int(row.breakDurationSeconds)
    breakEndsAt = row.breakEndsAtMs.date
    activeBlockId = row.activeBlockId
    accumulatedSeconds = row.accumulatedSeconds.int
    pausedAt = row.pausedAtMs.date
    checkpointAt = row.checkpointAtMs.date
    openEnded = row.openEnded
  }
}

extension FocusQueueTask {
  init(_ entry: QueueEntry) {
    let row = entry.item
    let item = FocusQueueItem(
      id: row.id, sessionId: row.sessionId, taskId: row.taskId, sortOrder: Int(row.sortOrder),
      state: FocusQueueState(rawValue: row.state) ?? .queued, plannedSeconds: row.plannedSeconds.int,
      completedAt: row.completedAtMs.date, skippedAt: row.skippedAtMs.date,
      createdAt: Date(coreMilliseconds: row.createdAtMs), openEnded: row.openEnded)
    self.init(item: item, task: WorkspaceTask(entry.task))
  }
}

extension FocusWorkBlock {
  init(_ row: WorkBlockRow) {
    id = row.id
    sessionId = row.sessionId
    taskId = row.taskId
    taskTitle = row.taskTitle
    seconds = Int(row.seconds)
    recordedAt = Date(coreMilliseconds: row.recordedAtMs)
    originalTaskId = row.originalTaskId
  }
}

extension FocusAward {
  init(_ row: AwardRow) {
    id = row.id
    sessionId = row.sessionId
    taskId = row.taskId
    taskTitle = row.taskTitle
    seconds = Int(row.seconds)
    minutes = row.minutes
    multiplier = row.multiplier
    points = row.points
    awardedAt = Date(coreMilliseconds: row.awardedAtMs)
  }
}
