import Foundation
import TaktRustCore

// The next-up ranking and the availability rules are the Rust core's
// (core/src/ranking.rs, core/src/focus.rs), which Android calls too. These are
// the conversions between TaktCore's vocabulary, which the screens use, and
// the core's records.

extension Date {
  /// Whole milliseconds since 1970, rounded to the nearest, as the core takes them.
  var rankingMilliseconds: Int64 { Int64((timeIntervalSince1970 * 1000).rounded()) }

  init(rankingMilliseconds ms: Int64) {
    self.init(timeIntervalSince1970: Double(ms) / 1000)
  }
}

extension NextUpCandidate {
  var core: Candidate {
    Candidate(
      id: id, title: title, isDailyDueToday: isDailyDueToday, dueAtMs: dueAt?.rankingMilliseconds,
      startAtMs: startAt?.rankingMilliseconds, matrixUrgency: matrixUrgency.map { Int64($0) },
      matrixImportance: matrixImportance.map { Int64($0) }, priority: priority.map { Int64($0) },
      estimateSeconds: estimateSeconds.map { Int64($0) }, kanbanColumn: kanbanColumn,
      focusRank: focusRank.map { Int64($0) }, sortOrder: Int64(sortOrder),
      createdAtMs: createdAt == .distantPast ? Int64.min / 2 : createdAt.rankingMilliseconds,
      dueDate: dueDate, requirementGroups: requirementGroups, loggedSeconds: Int64(loggedSeconds),
      minimumBlockSeconds: minimumBlockSeconds.map { Int64($0) }, requiresSingleSitting: requiresSingleSitting,
      dailyRemainingSeconds: dailyRemainingSeconds.map { Int64($0) },
      dailyUnavailable: dailyUnavailable.flatMap { reason -> String? in
        switch reason {
        case .dailyNotScheduled: "dailyNotScheduled"
        case .dailyAlreadyMet: "dailyAlreadyMet"
        default: nil
        }
      },
      listPriority: listPriority.map { Int64($0) }, columnRank: columnRank.map { Int64($0) })
  }
}

extension TaktCore.FocusContext {
  var coreContext: TaktRustCore.FocusContext {
    TaktRustCore.FocusContext(
      conditionIds: conditionIDs.sorted(), endsAtMs: endsAt?.rankingMilliseconds, mode: mode.rawValue)
  }
}

extension TaskUnavailableReason {
  init(_ core: Unavailable) {
    switch core {
    case .startsLater(let at): self = .startsLater(Date(rankingMilliseconds: at))
    case .missingConditions(let groups): self = .missingConditions(groups)
    case .insufficientTime(let seconds): self = .insufficientTime(Int(seconds))
    case .needsEstimate: self = .needsEstimate
    case .expiredWindow: self = .expiredWindow
    case .dailyNotScheduled: self = .dailyNotScheduled
    case .dailyAlreadyMet: self = .dailyAlreadyMet
    }
  }
}

extension ScoredNextUp {
  init(_ scored: Scored, original: NextUpCandidate) {
    let reason = NextUpReason(rawValue: scored.reason) ?? .order
    self.init(candidate: original, score: scored.score, reason: reason, explanation: scored.explanation)
  }
}

extension NextUpCandidate {
  /// A candidate the core read, in TaktCore's vocabulary.
  public init(_ core: Candidate) {
    let unavailable: TaskUnavailableReason? = switch core.dailyUnavailable {
    case "dailyNotScheduled": .dailyNotScheduled
    case "dailyAlreadyMet": .dailyAlreadyMet
    default: nil
    }
    self.init(
      id: core.id, title: core.title, isDailyDueToday: core.isDailyDueToday,
      dueAt: core.dueAtMs.map { Date(rankingMilliseconds: $0) },
      startAt: core.startAtMs.map { Date(rankingMilliseconds: $0) },
      matrixUrgency: core.matrixUrgency.map { Int($0) }, matrixImportance: core.matrixImportance.map { Int($0) },
      priority: core.priority.map { Int($0) }, estimateSeconds: core.estimateSeconds.map { Int($0) },
      kanbanColumn: core.kanbanColumn, focusRank: core.focusRank.map { Int($0) }, sortOrder: Int(core.sortOrder),
      createdAt: Date(rankingMilliseconds: core.createdAtMs), dueDate: core.dueDate,
      requirementGroups: core.requirementGroups, loggedSeconds: Int(core.loggedSeconds),
      minimumBlockSeconds: core.minimumBlockSeconds.map { Int($0) },
      requiresSingleSitting: core.requiresSingleSitting,
      dailyRemainingSeconds: core.dailyRemainingSeconds.map { Int($0) }, dailyUnavailable: unavailable,
      listPriority: core.listPriority.map { Int($0) }, columnRank: core.columnRank.map { Int($0) })
  }
}

extension TaktCore.FocusContext {
  /// The context as the core takes it, for a read that ranks inside the core.
  public var core: TaktRustCore.FocusContext { coreContext }
}

extension FocusRanking {
  /// A ranking the core made, every candidate converted from the core's
  /// record: for a read that ranked inside the core and returned only part of
  /// the ladder.
  public init(ranked: [Scored], blocked: [Blocked], nextEvaluationAtMs: Int64?) {
    self.init(
      ranked: ranked.map { ScoredNextUp($0, original: NextUpCandidate($0.candidate)) },
      blocked: blocked.map {
        BlockedFocusTask(candidate: NextUpCandidate($0.candidate), reasons: $0.reasons.map(TaskUnavailableReason.init))
      },
      nextEvaluationAt: nextEvaluationAtMs.map { Date(rankingMilliseconds: $0) })
  }
}

extension DayPlanEntry {
  /// An entry the core planned. An unknown reason, which only a newer core
  /// could send, reads as planned.
  public init(_ core: DayEntry) {
    self.init(id: core.id, reason: DayPlanReason(rawValue: core.reason) ?? .planned)
  }
}
