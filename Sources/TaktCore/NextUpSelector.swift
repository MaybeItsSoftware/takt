import Foundation
import TaktRustCore

public enum NextUpReason: String, Sendable, Equatable, CaseIterable {
  case daily, overdue, dueToday, dueSoon, today, importance, priority, order
  case condition, started, deadlineRisk

  public var explanation: String {
    switch self {
    case .daily: "today's contribution is still outstanding"
    case .overdue: "it is past its due date"
    case .dueToday: "it is due today"
    case .dueSoon: "it is due shortly"
    case .today: "you put it in Today"
    case .importance: "it is important on the matrix"
    case .priority: "it carries a high priority"
    case .order: "it is next in order"
    case .condition: "its required conditions are available now"
    case .started: "its start time has arrived"
    case .deadlineRisk: "its remaining work puts the deadline at risk"
    }
  }
}

public enum FocusTimeMode: String, Codable, CaseIterable, Sendable {
  case progress, finish
}

public struct FocusContext: Equatable, Sendable {
  public var conditionIDs: Set<String>
  public var endsAt: Date?
  public var mode: FocusTimeMode

  public init(conditionIDs: Set<String> = [], endsAt: Date? = nil, mode: FocusTimeMode = .progress) {
    self.conditionIDs = conditionIDs
    self.endsAt = endsAt
    self.mode = mode
  }
}

public struct NextUpCandidate: Sendable, Equatable, Identifiable {
  public let id: String
  public let title: String
  public let isDailyDueToday: Bool
  public let dueAt: Date?
  public let startAt: Date?
  public let matrixUrgency: Int?
  public let matrixImportance: Int?
  public let priority: Int?
  public let estimateSeconds: Int?
  public let kanbanColumn: String?
  public let focusRank: Int?
  public let sortOrder: Int
  public let createdAt: Date
  public let dueDate: String?
  public let requirementGroups: [[String]]
  public let loggedSeconds: Int
  public let minimumBlockSeconds: Int?
  public let requiresSingleSitting: Bool
  public let dailyRemainingSeconds: Int?
  public let dailyUnavailable: TaskUnavailableReason?
  /// Its list's place among the prioritised lists, 1 the most important.
  public let listPriority: Int?
  /// How committed its board column is, 0 for Today: the core's `column_rank`.
  public let columnRank: Int?

  public init(
    id: String, title: String, isDailyDueToday: Bool = false, dueAt: Date? = nil,
    startAt: Date? = nil, matrixUrgency: Int? = nil, matrixImportance: Int? = nil,
    priority: Int? = nil, estimateSeconds: Int? = nil, kanbanColumn: String? = nil,
    focusRank: Int? = nil, sortOrder: Int = 0, createdAt: Date = .distantPast,
    dueDate: String? = nil, requirementGroups: [[String]] = [], loggedSeconds: Int = 0,
    minimumBlockSeconds: Int? = nil, requiresSingleSitting: Bool = false,
    dailyRemainingSeconds: Int? = nil, dailyUnavailable: TaskUnavailableReason? = nil,
    listPriority: Int? = nil, columnRank: Int? = nil
  ) {
    self.id = id; self.title = title; self.isDailyDueToday = isDailyDueToday
    self.dueAt = dueAt; self.startAt = startAt; self.matrixUrgency = matrixUrgency
    self.matrixImportance = matrixImportance; self.priority = priority
    self.estimateSeconds = estimateSeconds; self.kanbanColumn = kanbanColumn
    self.focusRank = focusRank; self.sortOrder = sortOrder; self.createdAt = createdAt
    self.dueDate = dueDate; self.requirementGroups = requirementGroups
    self.loggedSeconds = loggedSeconds; self.minimumBlockSeconds = minimumBlockSeconds
    self.requiresSingleSitting = requiresSingleSitting; self.dailyRemainingSeconds = dailyRemainingSeconds
    self.dailyUnavailable = dailyUnavailable
    self.listPriority = listPriority; self.columnRank = columnRank
  }

  public var remainingSeconds: Int? {
    if let dailyRemainingSeconds { return max(0, dailyRemainingSeconds) }
    return estimateSeconds.map { max(0, $0 - max(0, loggedSeconds)) }
  }

  public func effectiveDeadline(calendar: Calendar) -> Date? {
    if let dueDate, let day = TaskCalendarDate.date(dueDate, calendar: calendar) {
      return calendar.date(byAdding: .day, value: 1, to: day)
    }
    return dueAt
  }
}

/// Calendar dates stay dates across daylight-saving and timezone changes.
public enum TaskCalendarDate {
  public static func string(_ date: Date, calendar: Calendar = .current) -> String {
    let parts = calendar.dateComponents([.year, .month, .day], from: date)
    return String(format: "%04d-%02d-%02d", parts.year ?? 0, parts.month ?? 0, parts.day ?? 0)
  }

  public static func date(_ value: String, calendar: Calendar = .current) -> Date? {
    let pieces = value.split(separator: "-").compactMap { Int($0) }
    guard pieces.count == 3 else { return nil }
    let parts = DateComponents(year: pieces[0], month: pieces[1], day: pieces[2])
    guard let date = calendar.date(from: parts), string(date, calendar: calendar) == value else { return nil }
    return date
  }
}

public enum TaskUnavailableReason: Equatable, Sendable {
  case startsLater(Date), missingConditions([[String]]), insufficientTime(Int), needsEstimate, expiredWindow
  case dailyNotScheduled, dailyAlreadyMet
}

public struct BlockedFocusTask: Identifiable, Equatable, Sendable {
  public let candidate: NextUpCandidate
  public let reasons: [TaskUnavailableReason]
  public var id: String { candidate.id }
}

public struct FocusRanking: Sendable {
  public let ranked: [ScoredNextUp]
  public let blocked: [BlockedFocusTask]
  public let nextEvaluationAt: Date?
}

/// Whether a task can be worked on now, and for how long: the Rust core's
/// `focus::reasons`, `planned_seconds` and `suggested_seconds`.
public enum TaskAvailabilityPolicy {
  public static func reasons(
    for task: NextUpCandidate, context: FocusContext, now: Date
  ) -> [TaskUnavailableReason] {
    availabilityReasons(candidate: task.core, context: context.coreContext, nowMs: now.rankingMilliseconds)
      .map(TaskUnavailableReason.init)
  }

  public static func plannedSeconds(for task: NextUpCandidate, requested: Int?, context: FocusContext, now: Date) -> Int {
    Int(plannedBlockSeconds(
      candidate: task.core, requested: requested.map { Int64($0) }, context: context.coreContext,
      nowMs: now.rankingMilliseconds))
  }

  public static func suggestedSeconds(for task: NextUpCandidate, context: FocusContext, now: Date) -> Int {
    Int(suggestedBlockSeconds(candidate: task.core, context: context.coreContext, nowMs: now.rankingMilliseconds))
  }
}

public struct ScoredNextUp: Sendable, Equatable, Identifiable {
  public let candidate: NextUpCandidate
  public let score: Double
  public let reason: NextUpReason
  public let explanation: String
  public var id: String { candidate.id }

  public init(candidate: NextUpCandidate, score: Double, reason: NextUpReason, explanation: String? = nil) {
    self.candidate = candidate; self.score = score; self.reason = reason
    self.explanation = explanation ?? reason.explanation
  }
}

/// Availability is evaluated before a deterministic precedence tuple. Numeric
/// scores are retained for compatibility; they never override deadline ordering.
public enum NextUpSelector {
  public static let todayColumnID = "today"
  public static let dueHorizonDays = 14
  public static let minimumDeadlineBuffer: Double = 300
  public static let deadlineBufferFraction: Double = 0.2

  public static func next(from candidates: [NextUpCandidate], now: Date = .now,
                          calendar: Calendar = .current) -> ScoredNextUp? {
    rank(candidates, now: now, calendar: calendar).first
  }

  public static func rank(_ candidates: [NextUpCandidate], now: Date = .now,
                          calendar: Calendar = .current, context: FocusContext = FocusContext()) -> [ScoredNextUp] {
    evaluate(candidates, now: now, calendar: calendar, context: context).ranked
  }

  /// The day's order: the Rust core's `ranking::evaluate`.
  public static func evaluate(_ candidates: [NextUpCandidate], now: Date = .now,
                              calendar: Calendar = .current, context: FocusContext = FocusContext()) -> FocusRanking {
    let byID = Dictionary(candidates.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
    let ranking = rankNextUp(
      candidates: candidates.map(\.core), nowMs: now.rankingMilliseconds, zone: calendar.timeZone.identifier,
      context: context.coreContext)
    return FocusRanking(
      ranked: ranking.ranked.compactMap { scored in
        byID[scored.candidate.id].map { ScoredNextUp(scored, original: $0) }
      },
      blocked: ranking.blocked.compactMap { blocked in
        byID[blocked.candidate.id].map {
          BlockedFocusTask(candidate: $0, reasons: blocked.reasons.map(TaskUnavailableReason.init))
        }
      },
      nextEvaluationAt: ranking.nextEvaluationAtMs.map { Date(rankingMilliseconds: $0) })
  }

  /// Why one task ranks where it does: the Rust core's `ranking::score`.
  public static func score(_ task: NextUpCandidate, now: Date = .now,
                           calendar: Calendar = .current) -> ScoredNextUp {
    ScoredNextUp(
      scoreNextUp(candidate: task.core, nowMs: now.rankingMilliseconds, zone: calendar.timeZone.identifier),
      original: task)
  }
}
