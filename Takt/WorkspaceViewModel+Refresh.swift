import Foundation
import TaktCore
import TaktWorkspace

/// What a change made stale. A mutation says which of these it dirtied, and
/// the refresh reloads those and nothing else, once, however many times it
/// was asked.
struct WorkspaceRefresh: OptionSet {
  let rawValue: Int

  /// The sidebar's nested lists and per-list counts. Every list is read.
  static let sidebar = WorkspaceRefresh(rawValue: 1 << 0)
  /// The main pane's scope: outline, board and matrix, and the done rail.
  static let outline = WorkspaceRefresh(rawValue: 1 << 1)
  /// The board alone, for a change that cannot have moved anything in the
  /// outline — a card's column, its matrix square.
  static let board = WorkspaceRefresh(rawValue: 1 << 2)
  static let dailies = WorkspaceRefresh(rawValue: 1 << 3)
  /// The day and the focus ladder. Ranked off the main thread.
  static let nextUp = WorkspaceRefresh(rawValue: 1 << 4)
}

/// One refresh per user action.
///
/// Every mutation used to reload, in line, everything it could have touched:
/// the sidebar walked every list, then the outline walked it again, then the
/// board re-read its lists, then next-up read the entire workspace — and
/// `reloadDailies` and `reloadNextUp` each wrapped themselves in `perform`, so
/// one keystroke also refreshed the editor's drafts and scheduled a Google
/// Tasks push two or three times.
///
/// Now a reload called inside `perform` (or `batchingRefreshes`) only marks
/// what it would have reloaded. When the outermost block ends, the marks are
/// served once, in dependency order, the side effects run once, and the
/// expensive next-up ranking is handed to a background read. A reload called
/// outside any block still runs straight away, so a caller that reads the
/// result on the next line keeps working.
///
/// Reading `visibleNavigationTasks` mid-block serves the marks first, so code
/// that asks "what is on screen now" after changing scope sees the new scope.
@MainActor
extension WorkspaceViewModel {
  /// Marks `dirty` and, outside any block, serves it now.
  func refresh(_ dirty: WorkspaceRefresh) {
    pendingRefresh.formUnion(dirty)
    if refreshDepth == 0 { flushPendingRefresh() }
  }

  /// Runs `body` with its reloads coalesced into one refresh at the end.
  /// For navigation that reloads more than once and writes nothing.
  func batchingRefreshes(_ body: () -> Void) {
    refreshDepth += 1
    body()
    refreshDepth -= 1
    if refreshDepth == 0 { flushPendingRefresh() }
  }

  /// The one funnel every local write passes through.
  ///
  /// - Parameter mirrors: whether the write is one the Google Tasks mirror
  ///   has to hear about. A focus clock checkpoint is a write, but not to
  ///   anything the mirror carries.
  func perform(mirrors: Bool = true, _ work: () throws -> Void) {
    if refreshDepth == 0 { performPriorErrorMessage = errorMessage }
    refreshDepth += 1
    do {
      try work()
      if mirrors { performShouldMirror = true }
    } catch {
      performFailed = true
      errorMessage = error.localizedDescription
    }
    refreshDepth -= 1
    guard refreshDepth == 0 else { return }

    let failed = performFailed
    let shouldMirror = performShouldMirror
    performFailed = false
    performShouldMirror = false
    // A write that went through clears the message a failed one left, and
    // only that: a message the work itself put up is the work's to show.
    if !failed, errorMessage == performPriorErrorMessage { errorMessage = nil }
    performPriorErrorMessage = nil
    // After clearing the error, so a reload that fails still says so.
    flushPendingRefresh(afterWrite: true)
    if let store { taskEditor.refresh(store: store) }
    // Every local write funnels through here, which makes it the one place
    // the Google Tasks mirror has to be told about. It coalesces.
    if shouldMirror { onLocalWrite?() }
  }

  /// Serves whatever has been marked. Safe to call at any depth.
  func flushPendingRefresh(afterWrite: Bool = false) {
    let wrote = afterWrite
    guard !pendingRefresh.isEmpty || wrote else { return }
    // Any in-flight ranking was read before whatever just happened.
    writeEpoch += 1
    // What was just written has to be on screen: no kept scope stands in.
    if wrote { scopeReadMayBeStale = false }
    var reloadedTasks = false
    // What an open draft row is drawn among, so it can follow the rows it
    // sat beside rather than leave with them.
    let draft = taskDraftSurroundings()
    // A reload that asks for another only marks it, and the loop serves it,
    // rather than starting a second refresh inside this one.
    refreshDepth += 1
    defer { refreshDepth -= 1 }
    while !pendingRefresh.isEmpty {
      let dirty = pendingRefresh
      pendingRefresh = []
      if dirty.contains(.outline) {
        reloadOutlineNow()
        reloadedTasks = true
      } else if dirty.contains(.board) {
        reloadBoardNow()
        reloadedTasks = true
      }
      if dirty.contains(.sidebar) {
        reloadNestedListsNow()
        reloadedTasks = true
      }
      if dirty.contains(.dailies) { reloadDailiesNow() }
      if dirty.contains(.nextUp) { scheduleNextUp() }
    }
    if wrote { waitingDetailsAreStale = true }
    if reloadedTasks || wrote {
      rebuildTaskCache(afterWrite: wrote)
      refreshHistoryLabels()
      if let draft { reanchorTaskDraft(from: draft) }
    }
    // The day and the ladder are read from the whole workspace, so any write
    // can change them — a task added on Today most of all, which otherwise
    // vanished from the screen it was typed on until something else asked for
    // a ranking. Off the main thread, and coalesced with any other request.
    if wrote { scheduleNextUp() }
    // What is due may have changed, and a reminder's text is fixed when set.
    if wrote { ReminderCoordinator.shared.schedule() }
    // The outermost refresh: a switch's leave to draw kept state ends here.
    if refreshDepth == 1 { scopeReadMayBeStale = false }
  }

  /// The history menu's labels, read after a write rather than on every
  /// render of the menu that shows them.
  func refreshHistoryLabels() {
    let undo = store.flatMap { try? $0.undoableLabel() }
    let redo = store.flatMap { try? $0.redoableLabel() }
    if undoLabel != undo { undoLabel = undo }
    if redoLabel != redo { redoLabel = redo }
    // The journal may have moved even where the labels read the same, and
    // the agent panel's change cards ask it what Undo would do.
    historyVersion &+= 1
  }

  /// Everything a surface on screen names by id, so resolving one in a view
  /// body is a dictionary lookup rather than a query.
  ///
  /// Built from what the refresh has just loaded, then topped up in one read
  /// for whatever is named but not on the board or outline — the day's tasks,
  /// the running block, the selection.
  ///
  /// - Parameter wrote: whether a write preceded it. Without one, and with
  ///   the store's stamp where it was at the last rebuild, nothing a task
  ///   resolved by id could say has changed — a switch only changes which
  ///   tasks are held — so `taskContentRevision` is left alone, and the views
  ///   keyed to it (the progress dock's reload, the inspector's dependencies,
  ///   every body that resolved a task) are not redrawn by a switch.
  func rebuildTaskCache(afterWrite wrote: Bool = true) {
    var cache: [String: WorkspaceTask] = [:]
    cache.reserveCapacity(boardTreeTasks.count + outline.count + nestedLists.count)
    for task in completedTasks { cache[task.id] = task }
    for queued in focusQueue { cache[queued.task.id] = queued.task }
    for item in nestedLists { cache[item.id] = item.task }
    for item in outline { cache[item.id] = item.task }
    for task in boardTreeTasks { cache[task.id] = task }

    var named = todayPlan.map(\.id) + focusLadder.prefix(WorkspaceNextUpSnapshot.fallbackDayLength).map(\.candidate.id)
    named += [selectedTaskID, scopeTaskID, activeFocusSession?.activeTaskId].compactMap { $0 }
    let missing = named.filter { cache[$0] == nil }
    if let store, !missing.isEmpty, let fetched = try? store.tasks(ids: missing) {
      for (id, task) in fetched { cache[id] = task }
      for id in missing { dayTaskSnapshot[id] = fetched[id] }
    }
    taskCache = cache
    // Rereads only after a write, of any kind; see `reloadWaitingDetails`.
    reloadWaitingDetails()
    missingTaskIDs.removeAll(keepingCapacity: true)
    descendantCache.removeAll(keepingCapacity: true)
    dayChildrenCache.removeAll(keepingCapacity: true)
    if taskContentMayHaveChanged(afterWrite: wrote) { taskContentRevision += 1 }
    rebuildDayItems()
  }

  /// Whether anything has been written since the last rebuild, by this
  /// process or another, and notes the stamp for the next one.
  private func taskContentMayHaveChanged(afterWrite wrote: Bool) -> Bool {
    guard let store, let stamp = try? store.changeStamp() else {
      taskContentStamp = nil
      return true
    }
    let current = (store: ObjectIdentifier(store), stamp: stamp)
    defer { taskContentStamp = current }
    guard !wrote, let held = taskContentStamp else { return true }
    return held.store != current.store || held.stamp != current.stamp
  }

  /// The day, resolved to tasks. See `dayItems`.
  func rebuildDayItems() {
    func resolve(_ id: String) -> WorkspaceTask? { taskCache[id] ?? dayTaskSnapshot[id] }
    let planned = todayPlan.compactMap { entry -> DayItem? in
      resolve(entry.id).map { DayItem(task: $0, reason: entry.reason) }
    }
    let items = planned.isEmpty
      ? focusLadder.prefix(WorkspaceNextUpSnapshot.fallbackDayLength)
        .compactMap { resolve($0.candidate.id) }.map { DayItem(task: $0, reason: nil) }
      : planned
    if dayItems != items { dayItems = items }
    rebuildDayTree()
  }

  // MARK: - Next up

  /// Asks for the day and the ladder to be ranked again, off the main thread.
  /// Requests made in the same turn of the run loop share one ranking.
  func scheduleNextUp() {
    guard store != nil else { return }
    nextUpRequested = true
    guard !nextUpLaunchQueued else { return }
    nextUpLaunchQueued = true
    Task { @MainActor [weak self] in self?.launchNextUp() }
  }

  /// Ranks now, on this thread, for the few callers that read the ladder on
  /// the next line. Supersedes anything in flight.
  func reloadNextUpNow() {
    guard let store else { return }
    nextUpRequested = false
    nextUpGeneration += 1
    do {
      let snapshot = try store.nextUpSnapshot(
        workspaceId: workspace?.id, context: effectiveFocusContext,
        runningID: activeFocusSession?.activeTaskId, ladderLimit: WorkspaceNextUpSnapshot.fallbackDayLength)
      applyNextUp(snapshot)
    } catch {
      errorMessage = error.localizedDescription
    }
  }

  private func launchNextUp() {
    nextUpLaunchQueued = false
    guard nextUpRequested, let store else { return }
    nextUpRequested = false
    nextUpGeneration += 1
    let generation = nextUpGeneration
    let epoch = writeEpoch
    let workspaceID = workspace?.id
    let context = effectiveFocusContext
    let runningID = activeFocusSession?.activeTaskId
    Task.detached(priority: .userInitiated) { [weak self] in
      let result = Result {
        try store.nextUpSnapshot(
          workspaceId: workspaceID, context: context, runningID: runningID,
          ladderLimit: WorkspaceNextUpSnapshot.fallbackDayLength)
      }
      await self?.finishNextUp(result, generation: generation, epoch: epoch)
    }
  }

  private func finishNextUp(_ result: Result<WorkspaceNextUpSnapshot, Error>, generation: Int, epoch: Int) {
    // A newer ranking is on its way; this one lost.
    guard generation == nextUpGeneration else { return }
    // Something was written while this was being read, so it may describe a
    // workspace that no longer exists. Read again rather than show it.
    guard epoch == writeEpoch else {
      scheduleNextUp()
      return
    }
    switch result {
    case .success(let snapshot): applyNextUp(snapshot)
    case .failure(let error): errorMessage = error.localizedDescription
    }
  }

  private func applyNextUp(_ snapshot: WorkspaceNextUpSnapshot) {
    if taskLoggedSeconds != snapshot.loggedSeconds { taskLoggedSeconds = snapshot.loggedSeconds }
    if taskPlanningByID != snapshot.planning { taskPlanningByID = snapshot.planning }
    if workspace != nil, focusConditions != snapshot.conditions { focusConditions = snapshot.conditions }
    if requiredConditionIDs != snapshot.requiredConditionIDs { requiredConditionIDs = snapshot.requiredConditionIDs }
    if conditionRequirementCounts != snapshot.conditionUsage { conditionRequirementCounts = snapshot.conditionUsage }
    if todayPlan != snapshot.todayPlan { todayPlan = snapshot.todayPlan }
    if workProgress != snapshot.workProgress { workProgress = snapshot.workProgress }
    let ranked = snapshot.ranking.ranked
    if focusLadder != ranked { focusLadder = ranked }
    if blockedFocusTasks != snapshot.ranking.blocked { blockedFocusTasks = snapshot.ranking.blocked }
    nextFocusEvaluationAt = snapshot.ranking.nextEvaluationAt
    if nextUp != ranked.first { nextUp = ranked.first }
    // Fresher than anything cached: the epoch check means nothing has been
    // written since these rows were read.
    dayTaskSnapshot = snapshot.dayTasks
    for (id, task) in snapshot.dayTasks { taskCache[id] = task }
    rebuildDayItems()
    resumeBlockedFocusQueueIfEligible()
  }

  /// Hands a session whose queue was blocked at the last handoff its next
  /// task, once one is eligible.
  ///
  /// Asked first, as a read. The resume is a write, and every write through
  /// `perform` re-ranks the day, which lands back here: while every queued task
  /// stayed blocked, that was a resume that resumed nothing, a refresh, a
  /// ranking, and round again for as long as the session lasted.
  private func resumeBlockedFocusQueueIfEligible() {
    guard allowsQueueResume, let session = activeFocusSession, session.activeTaskId == nil, let store else { return }
    let context = effectiveFocusContext
    let now = Date.now
    guard (try? store.hasResumableFocusQueueTask(context: context, now: now)) == true else { return }
    // One resume per permission. The next handoff picks its own successor,
    // and a context change or a queued task grants it again.
    allowsQueueResume = false
    perform(mirrors: false) {
      try store.resumeEligibleFocusQueue(context: context, now: now)
      reloadFocus()
    }
  }
}
