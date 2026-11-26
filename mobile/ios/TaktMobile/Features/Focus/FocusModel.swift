import Foundation
import Observation
import TaktCore
import TaktWorkspace

/// When a deferred task comes back. The Mac's `WorkspaceDeferral`.
enum FocusDeferral: String, CaseIterable, Identifiable {
  case anHour, thisAfternoon, tomorrow, nextWeek

  var id: String { rawValue }

  var title: String {
    switch self {
    case .anHour: "In an hour"
    case .thisAfternoon: "This afternoon"
    case .tomorrow: "Tomorrow morning"
    case .nextWeek: "Next week"
    }
  }

  /// The core's arithmetic (`FocusDeferralTime`), shared with Android.
  func date(from now: Date, calendar: Calendar = .current) -> Date {
    // Same raw values on both enums.
    FocusDeferralTime(rawValue: rawValue)!.date(from: now, calendar: calendar)
  }
}

/// Everything the focus screen draws, read in one go off the main actor.
struct FocusSnapshot: Sendable {
  var ladder: [ScoredNextUp] = []
  var blocked: [BlockedFocusTask] = []
  var conditions: [TaskCondition] = []
  /// The conditions an open task requires: only these are offered as chips.
  var requiredConditionIDs: Set<String> = []
  var hasManualOrder = false
  var session: FocusSession?
  var activeTask: WorkspaceTask?
  var queue: [FocusQueueTask] = []
  var points: FocusPointsSummary = .zero
  var dailyTaskIDs: Set<String> = []
  var nextEvaluationAt: Date?

  static func load(store: WorkspaceStore, workspaceID: String, context: FocusContext, now: Date = .now) throws -> FocusSnapshot {
    let session = try store.activeFocusSession()
    let next = try store.nextUpSnapshot(workspaceId: workspaceID, context: context, runningID: session?.activeTaskId, now: now)
    var snapshot = FocusSnapshot()
    // The running task is the block, not a rung to choose again.
    snapshot.ladder = next.ranking.ranked.filter { $0.id != session?.activeTaskId }
    snapshot.blocked = next.ranking.blocked
    snapshot.conditions = next.conditions
    snapshot.requiredConditionIDs = next.requiredConditionIDs
    snapshot.hasManualOrder = next.hasManualFocusOrder
    snapshot.nextEvaluationAt = next.ranking.nextEvaluationAt
    snapshot.session = session
    if let session {
      snapshot.activeTask = try session.activeTaskId.flatMap { try store.task(id: $0) }
      snapshot.queue = try store.focusQueue(for: session.id)
    }
    snapshot.points = try store.focusPointsSummary(now: now)
    snapshot.dailyTaskIDs = Set(try store.dailies(on: now).filter { !$0.isDoneToday }.map(\.task.id))
    return snapshot
  }
}

/// A start the ranking would not have chosen, held for the user to confirm.
struct FocusStartOverride: Identifiable, Equatable {
  let taskID: String
  let plannedSeconds: Int
  let explanation: String
  var id: String { taskID }
}

/// The focus screen's state: the context you are in, the ladder ranked for
/// it, the task staged, and the running block. The Mac's
/// `WorkspaceViewModel+Focus/+Dailies/+Conditions`, for one screen.
@MainActor
@Observable
final class FocusModel {
  let model: WorkspaceModel
  private(set) var snapshot = FocusSnapshot()
  private(set) var isLoaded = false

  // MARK: Context

  // The context is the workspace's (`focus_contexts`), shared with the Mac,
  // Android, the CLI and the agent through sync: every change is saved there,
  // and `restoreContext` reads it back after a write anywhere.
  /// The conditions said to hold: a place chosen, a chip tapped.
  var conditionIDs: Set<String> = [] { didSet { if conditionIDs != oldValue { contextChanged() } } }
  /// The conditions a setting provides that were switched off for now —
  /// the housemates are in, so Quiet is off at Home — kept until whatever
  /// provides them is turned off or replaced.
  var offIDs: Set<String> = [] { didSet { if offIDs != oldValue { contextChanged() } } }
  /// Every condition in the workspace, for working out what the ones said
  /// provide: read with the context, so the first ranking already has it.
  @ObservationIgnored private(set) var allConditions: [TaskCondition] = []
  /// The end of the free time you have, or nil for no limit.
  var availableUntil: Date? { didSet { if availableUntil != oldValue { contextChanged() } } }
  /// When the conditions you said were true stop being trusted — "at the
  /// office until 5". Past it, the context is set aside and the last one is
  /// offered back rather than assumed. The Mac's `contextExpiresAt`.
  var contextExpiresAt: Date? { didSet { if contextExpiresAt != oldValue { contextChanged() } } }
  /// The conditions an expired context held, offered back with "Use again".
  var suggestedContextIDs: Set<String> = [] { didSet { if suggestedContextIDs != oldValue { contextChanged() } } }
  var mode: FocusTimeMode = .progress { didSet { if mode != oldValue { contextChanged() } } }
  /// When the conditions on were said, on whichever device said them.
  private(set) var conditionsSetAt: Date?
  /// Moves when the context does, so the ladder is ranked again.
  private(set) var contextVersion = 0
  /// Set while the stored context is being put back, so putting it back is
  /// not saved again as a new answer, and while several of its parts change
  /// as one.
  @ObservationIgnored private var isRestoringContext = false

  /// Every condition said, provided or switched off for now, by id: the
  /// core's `condition_holdings`, as on the Mac.
  var holdings: [String: FocusConditionHolding] {
    FocusConditionHoldings.holdings(conditionsKnown, set: conditionIDs, off: offIDs)
  }

  /// What holds now — said, plus what that provides, less what is off for
  /// now — which every task is checked against.
  var heldIDs: Set<String> {
    FocusConditionHoldings.held(conditionsKnown, set: conditionIDs, off: offIDs)
  }

  /// "from Home" on a condition a setting provides, "off for now" on one
  /// switched off, "this device" or "not this device" on a device condition;
  /// nil on one said directly or not at all.
  func holdingNote(_ id: String) -> String? {
    if holdings[id]?.isDevice == true { return "this device" }
    if conditionsKnown.first(where: { $0.id == id })?.isDevice == true { return "not this device" }
    guard let holding = holdings[id], !holding.isSet, !holding.providedBy.isEmpty else { return nil }
    if holding.isOff { return "off for now" }
    let names = holding.providedBy.compactMap { provider in conditionsKnown.first { $0.id == provider }?.name }
    return names.isEmpty ? nil : "from " + names.joined(separator: ", ")
  }

  private var conditionsKnown: [TaskCondition] {
    snapshot.conditions.isEmpty ? allConditions : snapshot.conditions
  }

  /// Whether the conditions on were said on an earlier day: a place left on
  /// overnight is probably not where you are. The core's rule, as the Mac's.
  var isContextStale: Bool {
    FocusContextSelection.isStale(conditionIDs, setAt: conditionsSetAt, now: .now)
  }

  /// The context the ranking sees. The time you have runs to whichever
  /// comes first, your stated end or the context's expiry: a context that
  /// lapses mid-block should not be trusted past it.
  var context: FocusContext {
    let ends = [availableUntil, contextExpiresAt].compactMap { $0 }.min()
    return FocusContext(conditionIDs: heldIDs, endsAt: ends, mode: mode)
  }

  // MARK: Ladder

  private(set) var stagedTaskID: String?
  var estimateMinutes: Double = 25
  var startOverride: FocusStartOverride?
  @ObservationIgnored private var generation = 0
  @ObservationIgnored private var expiryPromptedBlockID: String?
  @ObservationIgnored private var lastCheckpointAt = Date.distantPast
  /// When the ranking next changes on its own — a start time arriving, a
  /// window closing — so the ladder is ranked again then without a write.
  @ObservationIgnored private(set) var nextEvaluationAt: Date?
  /// The wall clock and uptime at the last tick: a gap between how far each
  /// moved is the device clock being changed under a running block.
  @ObservationIgnored private var lastClockAt = Date.now
  @ObservationIgnored private var lastUptime = ProcessInfo.processInfo.systemUptime
  @ObservationIgnored private var lastTimeZone = TimeZone.current.identifier

  /// Where the conditions on lived before they were the workspace's.
  private var legacyContextKey: String { "focus-context-\(model.workspace.id)" }

  init(model: WorkspaceModel) {
    self.model = model
    restoreContext()
  }

  var lastAward: FocusAward? { model.lastBlockResult?.award }
  var lastOutcome: WorkspaceStore.FocusCompletionOutcome? { model.lastBlockResult?.outcome }
  var pendingCompletion: PendingBlockCompletion? { model.pendingBlock }

  var ladder: [ScoredNextUp] { snapshot.ladder }
  var session: FocusSession? { snapshot.session }
  var activeTask: WorkspaceTask? { snapshot.activeTask }
  var isRunning: Bool { snapshot.session?.activeTaskId != nil }

  var stagedRung: ScoredNextUp? {
    guard let stagedTaskID else { return nil }
    return ladder.first { $0.id == stagedTaskID }
  }

  /// The title of the staged task, whether or not it is on the ladder — a
  /// task asked for from a context menu may be one the ranking ruled out.
  var stagedTitle: String? {
    guard let stagedTaskID else { return nil }
    return stagedRung?.candidate.title ?? model.task(stagedTaskID)?.title
  }

  // MARK: - Loading

  /// Ranks off the main actor and lands the result if nothing newer was asked.
  func load() async {
    restoreContext()
    generation &+= 1
    let mine = generation
    let store = model.store
    let workspaceID = model.workspace.id
    let context = context
    let result = await Task.detached(priority: .userInitiated) {
      try? FocusSnapshot.load(store: store, workspaceID: workspaceID, context: context)
    }.value
    guard mine == generation, let result else { return }
    apply(result)
  }

  /// Ranks on the spot. Tests, and callers that read the ladder next line.
  func loadNow() {
    restoreContext()
    generation &+= 1
    if let result = try? FocusSnapshot.load(store: model.store, workspaceID: model.workspace.id, context: context) {
      apply(result)
    }
  }

  private func apply(_ result: FocusSnapshot) {
    snapshot = result
    nextEvaluationAt = result.nextEvaluationAt
    isLoaded = true
    if let stagedTaskID, result.session?.activeTaskId == stagedTaskID { self.stagedTaskID = nil }
  }

  // MARK: - Context

  /// Puts back the context as the workspace holds it — said here, on another
  /// device or by the agent. The first time, conditions kept only in this
  /// phone's own settings move into the workspace, unless it already holds
  /// an answer: the phone's had no time on it, so it never beats one that does.
  func restoreContext() {
    let store = model.store
    let workspaceID = model.workspace.id
    var saved = (try? store.focusContext(in: workspaceID)) ?? nil
    if let legacy = AppGroup.defaults.stringArray(forKey: legacyContextKey) {
      AppGroup.defaults.removeObject(forKey: legacyContextKey)
      if saved == nil, !legacy.isEmpty {
        let moved = SavedFocusContext(conditionIDs: Set(legacy))
        _ = try? store.saveFocusContext(moved, in: workspaceID)
        saved = (try? store.focusContext(in: workspaceID)) ?? moved
      }
    }
    allConditions = (try? store.conditions(in: workspaceID)) ?? allConditions
    guard let saved else { return }
    isRestoringContext = true
    defer { isRestoringContext = false }
    let before = contextVersion
    conditionIDs = saved.conditionIDs
    offIDs = saved.offIDs
    availableUntil = saved.availableUntil
    contextExpiresAt = saved.expiresAt
    suggestedContextIDs = saved.suggestedIDs
    mode = saved.mode
    if conditionsSetAt != saved.conditionsSetAt {
      conditionsSetAt = saved.conditionsSetAt
      if contextVersion == before { contextVersion &+= 1 }
    }
  }

  /// Every change to the context: the ladder is ranked again and, unless it
  /// came from the workspace, the change is saved there for every device.
  private func contextChanged() {
    contextVersion &+= 1
    guard !isRestoringContext else { return }
    let context = SavedFocusContext(
      conditionIDs: conditionIDs, availableUntil: availableUntil, expiresAt: contextExpiresAt,
      mode: mode, suggestedIDs: suggestedContextIDs, offIDs: offIDs)
    let workspaceID = model.workspace.id
    _ = try? model.store.saveFocusContext(context, in: workspaceID)
    conditionsSetAt = ((try? model.store.focusContext(in: workspaceID)) ?? nil)?.conditionsSetAt
  }

  /// Says `condition` holds now, or no longer does. `conditions` are the
  /// ones to find the places and what they provide among — the ladder's,
  /// unless the caller has read its own, as the conditions sheet on Today
  /// does. One place at a time: choosing a location replaces the last one,
  /// and brings back what the last one's overrides switched off. Turning
  /// off one a setting provides switches it off for now.
  func toggleCondition(_ condition: TaskCondition, among conditions: [TaskCondition]? = nil) {
    // A device condition holds by the device in your hand, not by anything said.
    guard !condition.isDevice else { return }
    if let conditions { allConditions = conditions }
    let known = conditions ?? conditionsKnown
    let next = FocusConditionHoldings.switching(
      condition.id, in: known.contains { $0.id == condition.id } ? known : known + [condition],
      set: conditionIDs, off: offIDs)
    setSaid(next.set, off: next.off, suggested: [])
  }

  /// The conditions said and those off for now, changed as one answer and
  /// saved once.
  private func setSaid(_ set: Set<String>, off: Set<String>, suggested: Set<String>) {
    let changed = set != conditionIDs || off != offIDs || suggested != suggestedContextIDs
    isRestoringContext = true
    conditionIDs = set
    offIDs = off
    suggestedContextIDs = suggested
    isRestoringContext = false
    if changed { contextChanged() }
  }

  func setAvailable(minutes: Int?, now: Date = .now) {
    availableUntil = minutes.map { now.addingTimeInterval(Double($0 * 60)) }
  }

  /// Turns the context's expiry on (an hour from now) or off.
  func setContextExpires(_ expires: Bool, now: Date = .now) {
    contextExpiresAt = expires ? now.addingTimeInterval(3_600) : nil
  }

  /// The context has lapsed: its conditions are set aside, and offered back
  /// with "Use again" rather than assumed still true. The Mac does the same
  /// in `monitorFocus`.
  func expireContextIfDue(now: Date = .now) {
    guard let expires = contextExpiresAt, expires <= now else { return }
    let suggested = conditionIDs.isEmpty ? suggestedContextIDs : conditionIDs
    contextExpiresAt = nil
    setSaid([], off: [], suggested: suggested)
    model.showToast("Your context expired")
  }

  /// Nothing holds any more: every condition off.
  func clearConditions() {
    setSaid([], off: [], suggested: [])
  }

  /// Puts back the conditions an expired context held.
  func confirmSuggestedContext() {
    setSaid(suggestedContextIDs, off: [], suggested: [])
  }

  func dismissSuggestedContext() {
    suggestedContextIDs = []
  }

  func createCondition(named name: String, isLocation: Bool) {
    model.createCondition(named: name, isLocation: isLocation)
  }

  /// Makes a condition under a category (nil for none; "Location" makes it a place).
  func createCondition(named name: String, category: String?) {
    model.createCondition(named: name, category: category)
  }

  /// What "where you are" offers, under category headings, settings first:
  /// only the conditions an open task requires and the settings that
  /// provide them, plus any that is said, provided or off for now so it can
  /// be turned the other way. The rule is the Rust core's, so the Mac and
  /// Android agree.
  var conditionSections: [TaskConditionSection] {
    TaskConditionSection.available(
      snapshot.conditions, required: snapshot.requiredConditionIDs, active: Set(holdings.keys))
  }

  var visibleConditions: [TaskCondition] { conditionSections.flatMap(\.conditions) }

  func unavailableDescription(_ reason: TaskUnavailableReason) -> String {
    switch reason {
    case .startsLater(let date): "Starts \(date.formatted(date: .abbreviated, time: .shortened))"
    case .missingConditions(let groups): "Needs " + groups.map { group in
      TaskRequirement.label(group: group) { id in snapshot.conditions.first(where: { $0.id == id })?.name }
    }.joined(separator: " and ")
    case .insufficientTime(let seconds): "Needs at least \(Int(ceil(Double(seconds) / 60))) minutes"
    case .needsEstimate: "Needs a remaining estimate to check it fits"
    case .expiredWindow: "Your available time has run out"
    case .dailyNotScheduled: "This daily isn't scheduled today"
    case .dailyAlreadyMet: "Today's daily commitment is already met"
    }
  }

  /// The ranking's reason, with the conditions named when that is the reason.
  func explanation(for rung: ScoredNextUp) -> String {
    guard rung.reason == .condition else { return rung.explanation }
    let names = rung.candidate.requirementGroups.map { group in
      TaskRequirement.label(group: group.filter { TaskRequirement($0).isMet(available: heldIDs) }) { id in
        snapshot.conditions.first(where: { $0.id == id })?.name
      }
    }.joined(separator: " + ")
    return names + " available now"
  }

  // MARK: - The ladder

  /// Commits to a task: it becomes the thing about to be done, and the
  /// estimate is seeded from what it knows about itself.
  func stage(_ taskID: String) {
    stagedTaskID = taskID
    let seconds = ladder.first { $0.id == taskID }.map {
      TaskAvailabilityPolicy.suggestedSeconds(for: $0.candidate, context: context, now: .now)
    } ?? model.task(taskID)?.estimateSeconds ?? 1_500
    estimateMinutes = max(1, (Double(seconds) / 60).rounded())
  }

  func unstage() { stagedTaskID = nil }

  /// Begins the staged task. A start the context rules out is held as an
  /// override for the user to confirm, as on the Mac.
  func beginStaged(override: Bool = false) {
    guard let taskID = stagedTaskID else { return }
    begin(taskID, plannedSeconds: Int((max(1, estimateMinutes) * 60).rounded()), override: override)
  }

  func begin(_ taskID: String, plannedSeconds: Int, override: Bool = false) {
    let now = Date.now
    let context = context
    let candidate = (try? model.store.nextUpCandidates(now: now))?.first { $0.id == taskID }
    var explanations = candidate.map {
      TaskAvailabilityPolicy.reasons(for: $0, context: context, now: now).map(unavailableDescription)
    } ?? ["This task isn't currently available for focus."]
    if let end = context.endsAt, Double(plannedSeconds) > end.timeIntervalSince(now) {
      explanations.append("The block is longer than the time you have.")
    }
    if let candidate, plannedSeconds < max(60, candidate.minimumBlockSeconds ?? 60)
      || (candidate.requiresSingleSitting && plannedSeconds < (candidate.remainingSeconds ?? Int.max)) {
      explanations.append("The block is shorter than this task needs.")
    }
    if !override, !explanations.isEmpty {
      startOverride = FocusStartOverride(
        taskID: taskID, plannedSeconds: plannedSeconds, explanation: explanations.joined(separator: "\n"))
      return
    }
    startOverride = nil
    let started = model.perform { store in
      _ = try store.startFocusSession(
        taskId: taskID, plannedSeconds: plannedSeconds, context: context, overrideAvailability: override)
    }
    if started {
      stagedTaskID = nil
      model.lastBlockResult = nil
      loadNow()
    }
  }

  func confirmOverride() {
    guard let pending = startOverride else { return }
    begin(pending.taskID, plannedSeconds: pending.plannedSeconds, override: true)
  }

  /// Ticks a task off without a session: a daily logs today's contribution,
  /// anything else is completed.
  func completeWithoutSession(_ taskID: String) {
    let isDaily = snapshot.dailyTaskIDs.contains(taskID)
    let done = model.perform { store in
      if isDaily, let daily = try store.daily(forTaskId: taskID) {
        _ = try store.logContribution(dailyId: daily.id)
      } else {
        try store.setStatus(.completed, for: taskID)
      }
    }
    if done {
      model.noteCompletion()
      if stagedTaskID == taskID { stagedTaskID = nil }
    }
  }

  func deferTask(_ taskID: String, _ deferral: FocusDeferral, now: Date = .now) {
    let start = deferral.date(from: now)
    let move: @MainActor () -> Bool = { [weak model] in
      model?.perform { try $0.scheduleTask(id: taskID, startAt: start) } ?? false
    }
    let moved: @MainActor () -> Void = { [weak self] in
      guard let self else { return }
      if stagedTaskID == taskID { stagedTaskID = nil }
      model.announce("Deferred to \(deferral.title.lowercased())")
    }
    // A repeating task deferred onto its own next turn asks first.
    if model.askAboutRecurrenceClash(starting: taskID, at: start, save: move, saved: moved) { return }
    _ = move()
    moved()
  }

  /// Moves a rung by hand. Only the task that moved is pinned; its
  /// neighbours stay with the ranking, so a nudge stays a nudge.
  func moveRung(_ taskID: String, by offset: Int) {
    guard let index = ladder.firstIndex(where: { $0.id == taskID }) else { return }
    let target = index + offset
    guard ladder.indices.contains(target) else { return }
    model.perform { try $0.pinTask(id: taskID, atIndex: target) }
  }

  func moveRungs(from source: IndexSet, to destination: Int) {
    guard let index = source.first, ladder.indices.contains(index) else { return }
    let target = destination > index ? destination - 1 : destination
    guard target != index else { return }
    model.perform { try $0.pinTask(id: ladder[index].id, atIndex: target) }
  }

  /// Gives the ladder back to the ranking.
  func resetOrder() {
    model.perform { try $0.clearFocusOrder() }
  }

  /// Stages a task someone asked for elsewhere — a context menu, a day card.
  func takeRequest() {
    guard let taskID = model.focusRequestTaskID else { return }
    model.focusRequestTaskID = nil
    guard !isRunning else {
      model.showToast("Finish the running block first")
      return
    }
    stage(taskID)
  }

  // MARK: - The running block

  func togglePause(now: Date = .now) {
    guard let session else { return }
    model.perform { store in
      if session.pausedAt == nil {
        try store.pauseFocusSession(id: session.id, now: now)
      } else {
        try store.resumeFocusSession(id: session.id, now: now)
      }
    }
    loadNow()
  }

  /// Stops the clock and asks how the block went, through the one prompt
  /// the root presents. `completeTask` false is "log and keep": the time is
  /// credited and the task stays open.
  func requestCompletion(completeTask: Bool = true, now: Date = .now) {
    guard let session, let task = activeTask else { return }
    model.requestBlockCompletion(
      session: session, title: task.title, completeTask: completeTask, context: context, now: now)
    loadNow()
  }

  /// Ends the session, logging the running block's time without a score.
  func finish(now: Date = .now) {
    guard let session else { return }
    let context = context
    model.perform { store in
      if session.activeTaskId != nil {
        _ = try store.completeActiveFocusTask(
          sessionId: session.id, elapsedSeconds: session.elapsedSeconds(now: now), completeTask: false,
          expectedBlockId: session.activeBlockId, context: context)
      }
      try store.finishFocusSession(id: session.id, now: now)
    }
    model.pendingBlock = nil
    loadNow()
  }

  /// Called each second while the screen is up. Lapses an expired context,
  /// rebases a running block's clock after the device clock jumps, ranks
  /// again when the ranking's next boundary passes or the time zone changes,
  /// checkpoints the clock every thirty seconds so a crash keeps the time,
  /// and asks how it went once the planned block is up. The Mac's
  /// `monitorFocus`.
  func tick(now: Date = .now, uptime: TimeInterval = ProcessInfo.processInfo.systemUptime) {
    expireContextIfDue(now: now)
    let jumped = synchroniseClock(now: now, uptime: uptime)
    let zone = TimeZone.current.identifier
    let zoneChanged = zone != lastTimeZone
    lastTimeZone = zone
    if jumped || zoneChanged || (nextEvaluationAt.map { $0 <= now } ?? false) {
      nextEvaluationAt = nil
      contextVersion &+= 1
    }
    guard let session, session.pausedAt == nil, session.activeTaskId != nil else { return }
    if now.timeIntervalSince(lastCheckpointAt) >= 30 {
      lastCheckpointAt = now
      try? model.store.checkpointFocusSession(id: session.id, now: now)
    }
    // A timer never runs out: its stored length is only a stand-in.
    if !session.openEnded, session.elapsedSeconds(now: now) >= session.workDurationSeconds,
      expiryPromptedBlockID != session.activeBlockId, pendingCompletion == nil {
      expiryPromptedBlockID = session.activeBlockId
      requestCompletion(completeTask: false, now: now)
    }
  }

  /// Notices the wall clock moving by more than the time that passed — the
  /// user changing the clock, a manual time-zone fix — and rebases the
  /// running block so it counts the time actually worked rather than the
  /// jump. Returns whether the clock jumped.
  @discardableResult
  func synchroniseClock(now: Date, uptime: TimeInterval) -> Bool {
    let wallDelta = now.timeIntervalSince(lastClockAt)
    let uptimeDelta = uptime - lastUptime
    let jumped = abs(wallDelta - uptimeDelta) > 3
    if let session, session.pausedAt == nil, session.activeTaskId != nil,
      let elapsed = FocusClockPolicy.adjustedElapsed(
        previousElapsed: session.elapsedSeconds(now: lastClockAt), wallDelta: wallDelta, uptimeDelta: uptimeDelta) {
      model.perform { try $0.rebaseFocusClock(id: session.id, elapsedSeconds: elapsed, now: now) }
      loadNow()
    }
    lastClockAt = now
    lastUptime = uptime
    return jumped
  }

  func dismissAward() {
    model.lastBlockResult = nil
  }
}
