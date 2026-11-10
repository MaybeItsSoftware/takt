import Foundation
import TaktCore

extension WorkspaceStore {
  public func activeFocusSession() throws -> FocusSession? {
    try Self.mappingCoreErrors { try core.activeFocusSession() }.map(FocusSession.init)
  }

  public func focusQueue(for sessionId: String) throws -> [FocusQueueTask] {
    try Self.mappingCoreErrors { try core.focusQueue(sessionId: sessionId) }.map(FocusQueueTask.init)
  }

  /// `plannedSeconds` is the estimate given on the focus screen. It becomes the
  /// session's work block, so the timer counts towards the number the user
  /// actually committed to rather than a fixed pomodoro they never chose.
  /// Starts a focus session on a task, or returns the one running: the Rust
  /// core's `focus::start_session`.
  public func startFocusSession(
    taskId: String,
    plannedSeconds: Int? = nil,
    workDurationSeconds: Int = 25 * 60,
    breakDurationSeconds: Int = 5 * 60,
    context: FocusContext? = nil,
    overrideAvailability: Bool = false,
    now: Date = .now
  ) throws -> FocusSession {
    let id = try coreWrite {
      try core.startFocusSession(
        taskId: taskId, plannedSeconds: plannedSeconds.map { Int64($0) },
        workSeconds: Int64(workDurationSeconds), breakSeconds: Int64(breakDurationSeconds),
        context: context?.core, overrideAvailability: overrideAvailability, nowMs: now.coreMilliseconds, zone: TimeZone.current.identifier)
    }
    guard let session = try Self.mappingCoreErrors({ try core.focusSession(id: id) }).map(FocusSession.init) else {
      throw WorkspaceStoreError.noActiveFocusTask
    }
    return session
  }

  /// Starts a timer on a task — a block with no length, counting up until it
  /// is finished — or returns the session already running: the Rust core's
  /// `focus::start_timer`.
  public func startFocusTimer(
    taskId: String,
    breakDurationSeconds: Int = 5 * 60,
    context: FocusContext? = nil,
    overrideAvailability: Bool = false,
    now: Date = .now
  ) throws -> FocusSession {
    let id = try coreWrite {
      try core.startFocusTimer(
        taskId: taskId, breakSeconds: Int64(breakDurationSeconds), context: context?.core,
        overrideAvailability: overrideAvailability, nowMs: now.coreMilliseconds, zone: TimeZone.current.identifier)
    }
    guard let session = try Self.mappingCoreErrors({ try core.focusSession(id: id) }).map(FocusSession.init) else {
      throw WorkspaceStoreError.noActiveFocusTask
    }
    return session
  }

  public func addToFocusQueue(
    sessionId: String, taskId: String, plannedSeconds: Int? = nil, now: Date = .now
  ) throws {
    // The Rust core's `focus::add_to_queue`.
    try coreWrite {
      try core.addToFocusQueue(
        sessionId: sessionId, taskId: taskId, plannedSeconds: plannedSeconds.map { Int64($0) },
        nowMs: now.coreMilliseconds)
    }
  }

  /// What finishing a focus block did to the underlying task. A daily's task
  /// survives the sitting — that is the whole point of modelling a daily as a
  /// contribution — so the caller needs to know which happened before it
  /// reports anything to the user.
  public enum FocusCompletionOutcome: Sendable, Equatable {
    case taskCompleted
    case progressLogged(seconds: Int)
    case contributionLogged(seconds: Int)
  }

  public struct FocusCompletion: Sendable, Equatable {
    public let session: FocusSession
    public let outcome: FocusCompletionOutcome
    /// The points the block earned, or nil when it was finished without a
    /// quality judgement — or took no measurable time.
    public let award: FocusAward?
  }

  /// Finishes the current focus block, crediting `elapsedSeconds` of work.
  ///
  /// When the active task has a daily expected today the task stays open and
  /// the time lands on today's contribution instead. Otherwise the task is
  /// completed, which is what the queue's original behaviour was.
  ///
  /// `qualityMultiplier` is how well the user says the block went. Supplying
  /// one scores the block; leaving it nil finishes the block without a score,
  /// which is what every caller that is not the user pressing Done does.
  @discardableResult
  public func completeActiveFocusTask(
    sessionId: String,
    elapsedSeconds: Int = 0,
    qualityMultiplier: Double? = nil,
    completeTask: Bool = true,
    expectedBlockId: String? = nil,
    context: FocusContext = FocusContext(),
    now: Date = .now,
    calendar: Calendar = .current
  ) throws -> FocusCompletion {
    // The Rust core's `focus::finish_block`.
    let finished = try coreWrite {
      try core.finishFocusBlock(
        sessionId: sessionId, elapsedSeconds: Int64(elapsedSeconds), qualityMultiplier: qualityMultiplier,
        completeTask: completeTask, expectedBlockId: expectedBlockId, context: context.core,
        nowMs: now.coreMilliseconds, zone: calendar.timeZone.identifier)
    }
    return try completion(
      outcome: finished.outcome, seconds: finished.seconds, awardId: finished.awardId, in: sessionId)
  }

  /// Logs the block in hand as progress, as `completeActiveFocusTask` does
  /// with `completeTask: false`, then rests: the task stays in hand on a
  /// stopped clock and the session is on a break for its break length.
  @discardableResult
  public func takeFocusBreak(
    sessionId: String,
    elapsedSeconds: Int,
    qualityMultiplier: Double? = nil,
    expectedBlockId: String? = nil,
    now: Date = .now,
    calendar: Calendar = .current
  ) throws -> FocusCompletion {
    // The Rust core's `focus::take_break`.
    let finished = try coreWrite {
      try core.takeFocusBreak(
        sessionId: sessionId, elapsedSeconds: Int64(elapsedSeconds), qualityMultiplier: qualityMultiplier,
        expectedBlockId: expectedBlockId, nowMs: now.coreMilliseconds, zone: calendar.timeZone.identifier)
    }
    return try completion(
      outcome: finished.outcome, seconds: finished.seconds, awardId: finished.awardId, in: sessionId)
  }

  /// Back from a break: the task in hand starts a fresh block, or the queue
  /// moves on if it was finished meanwhile. The Rust core's `focus::end_break`.
  public func endFocusBreak(sessionId: String, context: FocusContext = FocusContext(), now: Date = .now) throws {
    try coreWrite {
      try core.endFocusBreak(
        sessionId: sessionId, context: context.core, nowMs: now.coreMilliseconds,
        zone: TimeZone.current.identifier)
    }
  }

  /// What a recorded block did, read back as the app's types. Takes the
  /// core's `BlockFinished` field by field: naming it here would mean
  /// importing the bindings, whose `FocusContext` shadows the app's.
  private func completion(
    outcome: String, seconds: Int64, awardId: String?, in sessionId: String
  ) throws -> FocusCompletion {
    try Self.mappingCoreErrors {
      guard let session = try core.focusSession(id: sessionId).map(FocusSession.init) else {
        throw WorkspaceStoreError.noActiveFocusTask
      }
      let award = try awardId.flatMap { try core.focusAward(id: $0) }.map(FocusAward.init)
      let seconds = Int(seconds)
      let result: FocusCompletionOutcome = switch outcome {
      case "taskCompleted": .taskCompleted
      case "contributionLogged": .contributionLogged(seconds: seconds)
      default: .progressLogged(seconds: seconds)
      }
      return FocusCompletion(session: session, outcome: result, award: award)
    }
  }

  /// Whether a running session that has handed off to nothing — every queued
  /// task was blocked at the last handoff — now has a queued task that is
  /// eligible under `context`. A read, so the app can ask before writing:
  /// `resumeEligibleFocusQueue` is a write whether or not it resumes anything,
  /// and a write re-ranks the day, which asked this again, forever, while the
  /// queue stayed blocked.
  public func hasResumableFocusQueueTask(context: FocusContext, now: Date = .now) throws -> Bool {
    // The Rust core's `focus::has_resumable`.
    try Self.mappingCoreErrors {
      try core.hasResumableFocusQueueTask(context: context.core, nowMs: now.coreMilliseconds, zone: TimeZone.current.identifier)
    }
  }

  public func finishFocusSession(id: String, now: Date = .now) throws {
    try coreWrite { try core.finishFocusSession(id: id, nowMs: now.coreMilliseconds) }
  }

  static func nonEmptyName(_ raw: String) throws -> String {
    let value = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !value.isEmpty else { throw WorkspaceStoreError.emptyName }
    return value
  }

  static func normalizedStrings(_ values: [String]) -> [String] {
    var seen = Set<String>()
    return values.compactMap { value in
      let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
      guard !trimmed.isEmpty, seen.insert(trimmed.localizedLowercase).inserted else { return nil }
      return trimmed
    }
  }

  static func decodeStringArray(_ json: String) -> [String] {
    guard let data = json.data(using: .utf8), let values = try? JSONDecoder().decode([String].self, from: data) else {
      return []
    }
    return normalizedStrings(values)
  }

  /// Internal rather than private so `WorkspaceStore+Import.swift` — the same
  /// type, split only for size — can reach it.
}
