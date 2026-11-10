import Foundation
import TaktCore
import TaktWorkspace

/// Builds what the widgets draw from the store. Pure over the store and safe
/// off the main actor: it only reads.
///
/// The day is resolved the way the Mac's `rebuildDayItems()` resolves it:
/// the planned day in order, or — when nothing has a claim on today — the
/// head of the focus ranking, each with the ranking's short "why" (Overdue,
/// High priority…). A task that is only next in order carries no reason,
/// because nothing chose it.
enum WidgetSnapshotBuilder {
  static let itemLimit = 6

  static func make(store: WorkspaceStore, workspaceID: String, now: Date = .now) throws -> WidgetSnapshot {
    let session = try store.activeFocusSession()
    let runningID = session?.activeTaskId
    let snapshot = try store.nextUpSnapshot(
      workspaceId: workspaceID, context: FocusContext(), runningID: runningID, now: now,
      ladderLimit: WorkspaceNextUpSnapshot.fallbackDayLength)
    let listNames = Dictionary(
      try store.lists(in: workspaceID, includingArchived: true).map { ($0.id, $0.name) },
      uniquingKeysWith: { first, _ in first })

    func item(_ task: WorkspaceTask, reason: String?) -> WidgetSnapshot.Item {
      WidgetSnapshot.Item(
        id: task.id, title: task.title, reason: reason, estimateSeconds: task.estimateSeconds,
        listName: listNames[task.listId])
    }

    let planned = snapshot.todayPlan.compactMap { entry in
      snapshot.dayTasks[entry.id].map { item($0, reason: entry.reason.label) }
    }
    let items = planned.isEmpty
      ? snapshot.ranking.ranked.prefix(WorkspaceNextUpSnapshot.fallbackDayLength)
        .compactMap { rung in snapshot.dayTasks[rung.candidate.id].map { item($0, reason: Self.why(rung.reason)) } }
      : planned

    let remaining = snapshot.todayPlan.reduce(0) { total, entry in
      guard let task = snapshot.dayTasks[entry.id], let estimate = task.estimateSeconds else { return total }
      return total + max(0, estimate - (snapshot.loggedSeconds[task.id] ?? 0))
    }

    var running: WidgetSnapshot.Running?
    if let session, let taskID = runningID, let task = try store.task(id: taskID) {
      let elapsed = session.elapsedSeconds(now: now)
      running = WidgetSnapshot.Running(
        taskID: taskID, title: task.title, timerStart: now.addingTimeInterval(-TimeInterval(elapsed)),
        isPaused: session.pausedAt != nil, elapsedSeconds: elapsed)
    }

    return WidgetSnapshot(
      generatedAt: now, items: Array(items.prefix(itemLimit)), todayCount: snapshot.todayPlan.count,
      remainingSeconds: remaining, completedToday: snapshot.workProgress.today.completed,
      loggedTodaySeconds: snapshot.workProgress.today.seconds, running: running)
  }

  /// The ranking's reason in a widget's few characters. The Focus ladder
  /// spells the same reasons out in full.
  static func why(_ reason: NextUpReason) -> String? {
    switch reason {
    case .daily: "Daily"
    case .overdue: "Overdue"
    case .dueToday: "Due today"
    case .dueSoon: "Due soon"
    case .today: "Planned"
    case .importance: "Important"
    case .priority: "High priority"
    case .condition: "Conditions met"
    case .started: "Started"
    case .deadlineRisk: "Deadline at risk"
    case .order: nil
    }
  }

  /// The Live Activity's state for the running block, or nil when nothing is
  /// running.
  static func focusState(store: WorkspaceStore, now: Date = .now) throws
    -> (sessionID: String, state: FocusActivityAttributes.ContentState)?
  {
    guard let session = try store.activeFocusSession(), session.phase != .finished,
      let taskID = session.activeTaskId, let task = try store.task(id: taskID)
    else { return nil }
    let elapsed = session.elapsedSeconds(now: now)
    let planned = try store.focusQueue(for: session.id)
      .first { $0.task.id == taskID && $0.item.state == .queued }?.item.plannedSeconds
    let state = FocusActivityAttributes.ContentState(
      taskID: taskID, taskTitle: task.title,
      timerStart: now.addingTimeInterval(-TimeInterval(elapsed)), isPaused: session.pausedAt != nil,
      // A timer commits to no length, so it carries none: the activity then
      // draws no progress bar towards an end that isn't there.
      elapsedSeconds: elapsed, plannedSeconds: session.openEnded ? nil : planned ?? session.workDurationSeconds)
    return (session.id, state)
  }
}
