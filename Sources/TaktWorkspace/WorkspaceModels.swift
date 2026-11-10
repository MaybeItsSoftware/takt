import Foundation
import TaktCore
import TaktRustCore

public struct Workspace: Codable, Identifiable, Sendable, Equatable {
  public let id: String
  public var name: String
  public let createdAt: Date
  public var updatedAt: Date
}

public struct ListFolder: Codable, Identifiable, Sendable, Equatable {
  public let id: String
  public let workspaceId: String
  public var parentFolderId: String?
  public var name: String
  public var sortOrder: Int
  public let createdAt: Date
  public var updatedAt: Date
}

/// A job the workspace itself relies on a list to do, as opposed to a list the
/// user made for their own reasons.
///
/// The role is what the app navigates by, so renaming Inbox to "Capture" keeps
/// it the inbox — the name is the user's, the role is the workspace's.
public enum TaskListRole: String, Codable, Sendable, CaseIterable {
  case inbox
}

public struct TaskList: Codable, Identifiable, Sendable, Equatable {
  public let id: String
  public let workspaceId: String
  public var folderId: String?
  public var name: String
  public var colorHex: String?
  public var sortOrder: Int
  public var isArchived: Bool
  public var systemRole: TaskListRole?
  /// An imported wrapper whose children form the list's visible root.
  /// Stored by identity so renaming either record cannot change placement.
  public var visibleRootTaskId: String?
  public var completedAt: Date?
  public let createdAt: Date
  public var updatedAt: Date

  public var isSystemList: Bool { systemRole != nil }
}

public enum TaskStatus: String, Codable, Sendable, CaseIterable {
  case open
  case completed
  case cancelled
}

public enum WorkspaceItemKind: String, Codable, Sendable, CaseIterable {
  case task
  case list
}

public struct WorkspaceTask: Codable, Identifiable, Sendable, Equatable {
  public let id: String
  public var listId: String
  public var parentTaskId: String?
  public var title: String
  public var notes: String
  public var status: TaskStatus
  public var sortOrder: Int
  public var dueAt: Date?
  public var estimateSeconds: Int?
  /// Where an imported task came from, and what that service calls it.
  ///
  /// Both are nil for a task typed into Priority, and immutable once set: they
  /// are what lets a second import run recognise a task it has already brought
  /// across instead of making another copy of it.
  public var sourceSystem: String?
  public var sourceId: String?
  /// Lists share the task tree so conversion preserves identity and descendants.
  /// Nil is a pre-existing ordinary task, not an inferred container.
  public var itemKind: WorkspaceItemKind?
  public var isPromoted: Bool?
  public var archivedAt: Date?
  /// When the task was closed, kept separately from `updatedAt` because
  /// editing a finished task must not move the day it was finished on.
  /// Nil whenever `status` is open, and cleared on reopening.
  public var completedAt: Date?
  public let createdAt: Date
  public var updatedAt: Date

  public var isList: Bool { itemKind == .list }
}

public struct TaskOutlineItem: Identifiable, Sendable, Equatable {
  public let task: WorkspaceTask
  public let depth: Int

  public var id: String { task.id }

  public init(task: WorkspaceTask, depth: Int) {
    self.task = task
    self.depth = depth
  }
}

public struct TaskMetadata: Codable, Sendable, Equatable {
  public let taskId: String
  public var priority: Int?
  public var startAt: Date?
  public var tagsJSON: String
  public var recurrenceRule: String?
  public var matrixUrgency: Int?
  public var matrixImportance: Int?
  public var kanbanColumn: String?
  public var externalLinksJSON: String
  /// Hand-placed position in the focus ladder; nil means ranked by score.
  public var focusRank: Int?
  public var updatedAt: Date
  public var planningJSON: String?
  /// Who or what a waiting task waits on: "Sam", "Legal", "invoice".
  public var waitingOn: String?
  /// When to chase a waiting task. See `WaitingFollowUp`.
  public var waitingFollowUpAt: Date?
  /// The follow-up task already made for this one, so it is made once.
  public var waitingFollowUpTaskId: String?
  /// On a follow-up task: the waiting task it chases.
  public var followUpOfTaskId: String?

  public init(
    taskId: String,
    priority: Int?,
    startAt: Date?,
    tagsJSON: String,
    recurrenceRule: String?,
    matrixUrgency: Int?,
    matrixImportance: Int?,
    kanbanColumn: String?,
    externalLinksJSON: String,
    focusRank: Int? = nil,
    updatedAt: Date
  ) {
    self.taskId = taskId
    self.priority = priority
    self.startAt = startAt
    self.tagsJSON = tagsJSON
    self.recurrenceRule = recurrenceRule
    self.matrixUrgency = matrixUrgency
    self.matrixImportance = matrixImportance
    self.kanbanColumn = kanbanColumn
    self.externalLinksJSON = externalLinksJSON
    self.focusRank = focusRank
    self.updatedAt = updatedAt
  }
}

public struct TaskEditorMetadata: Codable, Sendable, Equatable {
  public var priority: Int?
  public var tags: [String]
  public var recurrenceRule: String?
  public var externalLinks: [String]

  public init(priority: Int? = nil, tags: [String] = [], recurrenceRule: String? = nil, externalLinks: [String] = []) {
    self.priority = priority
    self.tags = tags
    self.recurrenceRule = recurrenceRule
    self.externalLinks = externalLinks
  }
}

/// A local board is scoped to a list or to one project's immediate children.
/// The task's column is deliberately independent of its parent's column: a
/// project can be in “In progress” while its next section is in “Today”.
///
/// A column with `rules` is a rule column: it holds the tasks its rules
/// match rather than the ones filed in it. See the core's `board_columns`.
public struct WorkspaceKanbanColumn: Codable, Identifiable, Sendable, Hashable {
  /// Coded as the id and title alone: the rules are the core's to read and
  /// write (`WorkspaceStore.boardColumns`), not a decoder's.
  enum CodingKeys: String, CodingKey { case id, title }

  public let id: String
  public var title: String
  public var rules: WorkspaceFilterDefinition?

  public init(id: String, title: String, rules: WorkspaceFilterDefinition? = nil) {
    self.id = id
    self.title = title
    self.rules = rules
  }

  /// Whether its tasks are the ones its rules match.
  public var isRuleColumn: Bool { rules != nil }

  /// Today and Waiting on: ids other features read, so deleting one moves
  /// nothing — its tasks stay planned or waiting.
  public var isBuiltIn: Bool { id == "today" || id == "waiting-on" }

  public static let blitzitDefaults = [
    WorkspaceKanbanColumn(id: "backlog", title: "Backlog"),
    WorkspaceKanbanColumn(id: "in-progress", title: "In progress"),
    WorkspaceKanbanColumn(id: "this-week", title: "This week"),
    WorkspaceKanbanColumn(id: "waiting-on", title: "Waiting on"),
    WorkspaceKanbanColumn(id: "today", title: "Today"),
  ]
}

public struct TaskMatrixPosition: Sendable, Equatable {
  public let urgency: Int?
  public let importance: Int?
  /// Where the task sits within its quadrant, lowest first; nil for a task
  /// never dropped at a point, which reads after the ranked ones
  /// (`v22_matrix_order`).
  public let rank: Int?

  public init(urgency: Int?, importance: Int?, rank: Int? = nil) {
    self.urgency = urgency
    self.importance = importance
    self.rank = rank
  }

  /// Whether two positions name the same quadrant, whatever their ranks.
  public func isSameQuadrant(as other: TaskMatrixPosition) -> Bool {
    urgency == other.urgency && importance == other.importance
  }
}

public enum FocusSessionPhase: String, Codable, Sendable, CaseIterable {
  case running
  case onBreak
  case finished
}

public struct FocusSession: Codable, Identifiable, Sendable, Equatable {
  public let id: String
  public let startedAt: Date
  public var endedAt: Date?
  public var phase: FocusSessionPhase
  public var activeTaskId: String?
  /// When the *current* task's block began, as distinct from the session's own
  /// start. The queue moves on without ending the session, so timing a second
  /// task from `startedAt` would credit it with the first one's sitting too.
  public var activeTaskStartedAt: Date
  public var workDurationSeconds: Int
  public let breakDurationSeconds: Int
  public var breakEndsAt: Date?
  public var activeBlockId: String?
  public var accumulatedSeconds: Int?
  public var pausedAt: Date?
  public var checkpointAt: Date?
  /// A timer: the block in hand has no length and counts up until it is
  /// finished. `workDurationSeconds` then holds only a day, a stand-in for
  /// clients that predate the flag, and is never a length to count down from.
  public var openEnded = false

  public func elapsedSeconds(now: Date) -> Int {
    max(0, accumulatedSeconds ?? 0) + (pausedAt == nil ? Int(max(0, now.timeIntervalSince(activeTaskStartedAt))) : 0)
  }
}

public enum FocusQueueState: String, Codable, Sendable, CaseIterable {
  case queued
  case completed
  case skipped
}

public struct FocusQueueItem: Codable, Identifiable, Sendable, Equatable {
  public let id: String
  public let sessionId: String
  public let taskId: String
  public var sortOrder: Int
  public var state: FocusQueueState
  /// What you committed to when you started this sitting, as distinct from the
  /// task's standing estimate. Nil when the queue item was added without one.
  public var plannedSeconds: Int?
  public var completedAt: Date?
  public var skippedAt: Date?
  public let createdAt: Date
  /// Runs as a timer, counting up, rather than as a block of a length.
  public var openEnded = false
}

public struct FocusQueueTask: Identifiable, Sendable, Equatable {
  public let item: FocusQueueItem
  public let task: WorkspaceTask

  public var id: String { item.id }
}

/// A task offered to the local workspace by an outside service.
///
/// The source ID rebuilds the hierarchy within one import run, and is then
/// kept on the local task, so running the same import again recognises what it
/// already brought across. Local records still carry their own UUID identity —
/// the source ID names the task abroad, never here.
public struct ImportedTaskSeed: Sendable, Equatable {
  public let sourceId: String
  public let parentSourceId: String?
  public let title: String
  public let notes: String
  public let status: TaskStatus
  public let sortOrder: Int

  public init(
    sourceId: String,
    parentSourceId: String?,
    title: String,
    notes: String = "",
    status: TaskStatus,
    sortOrder: Int
  ) {
    self.sourceId = sourceId
    self.parentSourceId = parentSourceId
    self.title = title
    self.notes = notes
    self.status = status
    self.sortOrder = sortOrder
  }
}

/// What one import run actually did, so a caller can say so rather than
/// reporting that something unspecified happened.
public struct TaskImportOutcome: Sendable, Equatable {
  public let list: TaskList
  public let insertedTaskIDs: [String]
  public let updatedTaskIDs: [String]
  /// True when the run created the destination list rather than merging into
  /// a list a previous run had already made.
  public let createdList: Bool

  public var insertedCount: Int { insertedTaskIDs.count }
  public var updatedCount: Int { updatedTaskIDs.count }

  public init(list: TaskList, insertedTaskIDs: [String], updatedTaskIDs: [String], createdList: Bool) {
    self.list = list
    self.insertedTaskIDs = insertedTaskIDs
    self.updatedTaskIDs = updatedTaskIDs
    self.createdList = createdList
  }
}

/// A standing commitment to move one task forward today.
///
/// Every daily points at a task. Ticking one does *not* complete the task — it
/// records a contribution against it, so "write 500 words" can be done daily
/// while "draft chapter three" stays open until it is genuinely finished. That
/// is the whole difference from a recurring task, which has to be completed and
/// respawned and therefore can't represent partial progress at all.
///
/// The schedule mirrors the legacy `Daily`: fixed weekdays, or a rotating
/// interval, never both. `intervalDays` wins when set.
public struct WorkspaceDaily: Codable, Identifiable, Sendable, Equatable {
  public let id: String
  public var taskId: String
  /// `Calendar` weekday numbering (1 = Sunday) as a bitmask, bit `n - 1` per
  /// weekday. A set stored as JSON would need decoding before SQL could filter
  /// on it, and the schedule is read on every launch.
  public var activeWeekdaysMask: Int
  public var intervalDays: Int?
  public var intervalAnchor: Date?
  /// How much of the task you mean to do on a day this lands — the estimate the
  /// focus screen offers first. Nil simply means "some".
  public var targetSeconds: Int?
  public var sortOrder: Int
  /// Archived rather than deleted, so past contributions keep a parent.
  public var archivedAt: Date?
  /// Set only on dailies carried over from the plugin-era store.
  public var legacyDailyId: String?
  public let createdAt: Date
  public var updatedAt: Date
  /// The task this habit was made from — "learn drums" behind "practise
  /// drums". Nil for a standalone habit and for every plain daily.
  public var sourceTaskId: String?
  /// The board column each appearance lands in (`HabitPlacement`). Nil is a
  /// plain daily, which never moves its task.
  public var placementColumn: String?
  /// Whether an appearance not done by the end of its day is dropped (a gap)
  /// rather than carried until it is done.
  public var dropsAtDayEnd: Bool = true
  /// `HabitExpiry.rule`: "source", "date" or "never".
  public var expiryRule: String = "never"
  /// The day a "date" expiry takes effect.
  public var expiresAt: Date?

  public static let allWeekdaysMask = 0b111_1111

  public init(
    id: String,
    taskId: String,
    activeWeekdaysMask: Int = WorkspaceDaily.allWeekdaysMask,
    intervalDays: Int? = nil,
    intervalAnchor: Date? = nil,
    targetSeconds: Int? = nil,
    sortOrder: Int,
    archivedAt: Date? = nil,
    legacyDailyId: String? = nil,
    createdAt: Date,
    updatedAt: Date
  ) {
    self.id = id
    self.taskId = taskId
    self.activeWeekdaysMask = activeWeekdaysMask == 0 ? WorkspaceDaily.allWeekdaysMask : activeWeekdaysMask
    self.intervalDays = intervalDays.map { min(366, max(1, $0)) }
    self.intervalAnchor = intervalAnchor
    self.targetSeconds = targetSeconds
    self.sortOrder = sortOrder
    self.archivedAt = archivedAt
    self.legacyDailyId = legacyDailyId
    self.createdAt = createdAt
    self.updatedAt = updatedAt
  }

  public var isArchived: Bool { archivedAt != nil }

  /// A daily made through the habit form: it has a column to land in.
  public var isHabit: Bool { placement != nil }

  public var placement: HabitPlacement? { placementColumn.flatMap(HabitPlacement.init(rawValue:)) }

  public var expiry: HabitExpiry { HabitExpiry(rule: expiryRule, date: expiresAt) }

  public var frequency: HabitFrequency { HabitFrequency(weekdays: activeWeekdays, intervalDays: intervalDays) }

  /// The schedule and options as the pure policy reads them.
  public var habitRule: HabitRule {
    HabitRule(
      weekdays: activeWeekdays, intervalDays: intervalDays, anchor: intervalAnchor ?? createdAt,
      dropsAtDayEnd: dropsAtDayEnd, expiry: expiry, placement: placement ?? .today)
  }

  public static func mask(forWeekdays weekdays: Set<Int>) -> Int {
    weekdays.reduce(0) { $0 | (1 << ($1 - 1)) }
  }

  public var activeWeekdays: Set<Int> {
    Set((1...7).filter { activeWeekdaysMask & (1 << ($0 - 1)) != 0 })
  }

  /// Whether this daily is expected on `day`: the Rust core's rule
  /// (`dailies::workspace_daily_is_due`), the one its next-up read applies.
  public func isDue(on day: Date, calendar: Calendar = .current) -> Bool {
    guard !isArchived else { return false }
    return workspaceDailyIsDue(
      weekdaysMask: Int64(activeWeekdaysMask), intervalDays: intervalDays.map(Int64.init),
      anchorMs: (intervalAnchor ?? createdAt).coreMilliseconds, dayMs: day.coreMilliseconds,
      zone: calendar.timeZone.identifier)
  }
}

/// One day's worth of progress against a daily's task.
///
/// Keyed by day rather than timestamped, because the question a daily answers
/// is "did I do this today", and a row per day makes that a lookup instead of a
/// range scan. `secondsLogged` accumulates across several focus sessions on the
/// same day.
public struct DailyContribution: Codable, Identifiable, Sendable, Equatable {
  public let id: String
  public let dailyId: String
  public let taskId: String
  /// `yyyy-MM-dd` in the user's calendar, produced by `DailyContribution.dayKey`.
  public let dayKey: String
  public var secondsLogged: Int
  public var completedAt: Date?
  public let createdAt: Date
  /// When the day was set aside without being done: cancelling a
  /// commitment's subtask skips its day. Neither done nor owed.
  public var skippedAt: Date?

  public var isComplete: Bool { completedAt != nil }
  public var isSkipped: Bool { skippedAt != nil }

  private static let dayKeyFormatter: DateFormatter = {
    let formatter = DateFormatter()
    formatter.calendar = Calendar(identifier: .gregorian)
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.dateFormat = "yyyy-MM-dd"
    return formatter
  }()

  public static func dayKey(for date: Date, calendar: Calendar = .current) -> String {
    let formatter = dayKeyFormatter
    formatter.timeZone = calendar.timeZone
    return formatter.string(from: date)
  }
}

/// A daily joined to its task and to today's contribution, which is what every
/// caller actually wants — the daily alone can't render a row.
public struct DailyItem: Identifiable, Sendable, Equatable {
  public let daily: WorkspaceDaily
  public let task: WorkspaceTask
  public let contribution: DailyContribution?

  public var id: String { daily.id }
  public var isDoneToday: Bool { contribution?.isComplete ?? false }
  /// Set aside today without being done.
  public var isSkippedToday: Bool { !isDoneToday && (contribution?.isSkipped ?? false) }
  public var secondsLoggedToday: Int { contribution?.secondsLogged ?? 0 }

  public init(daily: WorkspaceDaily, task: WorkspaceTask, contribution: DailyContribution?) {
    self.daily = daily
    self.task = task
    self.contribution = contribution
  }
}

/// A plugin-era daily on its way into the workspace. Mirrors `TaktCore.Daily`
/// without making this module depend on how that file happens to be shaped.
public struct LegacyDailySeed: Sendable, Equatable {
  public let id: String
  public let title: String
  public let activeWeekdays: Set<Int>
  public let intervalDays: Int?
  public let intervalAnchor: Date?
  public let targetSeconds: Int?
  public let archivedAt: Date?
  public let createdAt: Date

  public init(
    id: String,
    title: String,
    activeWeekdays: Set<Int> = Set(1...7),
    intervalDays: Int? = nil,
    intervalAnchor: Date? = nil,
    targetSeconds: Int? = nil,
    archivedAt: Date? = nil,
    createdAt: Date = Date()
  ) {
    self.id = id
    self.title = title
    self.activeWeekdays = activeWeekdays
    self.intervalDays = intervalDays
    self.intervalAnchor = intervalAnchor
    self.targetSeconds = targetSeconds
    self.archivedAt = archivedAt
    self.createdAt = createdAt
  }
}
