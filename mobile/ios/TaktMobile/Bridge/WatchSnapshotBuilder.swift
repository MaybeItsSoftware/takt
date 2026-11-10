import Foundation
import TaktWorkspace

/// Shapes what Today has read into what the watch draws. Pure over the day,
/// so the watch's rows are Today's rows by construction rather than by a
/// second ranking.
enum WatchSnapshotBuilder {
  /// A watch list longer than this is scrolled past, not read.
  static let taskLimit = 30

  /// The day as the watch shows it: the running block first, then Today's
  /// cards in Today's order (planned, overdue, due today, starts today — or
  /// the head of the ranking when nothing was planned), then the dailies
  /// still owed today that the day did not already hold.
  ///
  /// `plannedSeconds` is the block's commitment when the queue knows it;
  /// the session's own work length stands in otherwise, and a timer has none.
  static func make(day: DaySnapshot, plannedSeconds: Int? = nil, runningTitle: String? = nil, now: Date = .now)
    -> WatchSnapshot
  {
    let session = day.session.flatMap { session -> WatchSnapshot.Session? in
      guard session.phase != .finished, let taskID = session.activeTaskId else { return nil }
      let title = day.cards.first { $0.id == taskID }?.title
        ?? day.dailies.first { $0.taskID == taskID }?.title ?? runningTitle ?? ""
      return WatchSnapshot.Session(
        taskId: taskID, title: title, phase: session.pausedAt == nil ? .running : .paused,
        startedAt: session.pausedAt ?? session.activeTaskStartedAt,
        accumulatedSeconds: max(0, session.accumulatedSeconds ?? 0),
        // A timer has no length: with none, the watch counts up.
        plannedSeconds: session.openEnded ? nil : plannedSeconds ?? session.workDurationSeconds)
    }
    let runningID = session?.taskId

    var seen = Set<String>()
    var tasks: [WatchSnapshot.Task] = []
    let cards = day.cards.filter { $0.id == runningID } + day.cards.filter { $0.id != runningID }
    for card in cards where !card.isList && seen.insert(card.id).inserted {
      // A daily already logged today is done for the day, and goes.
      if card.dailyID != nil, card.isDailyDoneToday { continue }
      tasks.append(
        WatchSnapshot.Task(
          id: card.id, title: card.title, detail: card.detail ?? card.listName, isDaily: card.dailyID != nil,
          dailyDone: card.isDailyDoneToday, isRunning: card.id == runningID, estimateSeconds: card.estimateSeconds))
    }
    for daily in day.dailies where !daily.isDone && seen.insert(daily.taskID).inserted {
      tasks.append(
        WatchSnapshot.Task(
          id: daily.taskID, title: daily.title, detail: "Daily", isDaily: true, dailyDone: false,
          isRunning: daily.taskID == runningID, estimateSeconds: daily.targetSeconds))
    }
    // The running block keeps its row even when the day did not hold it — a
    // task started from a list.
    if let session, !seen.contains(session.taskId) {
      tasks.insert(WatchSnapshot.Task(id: session.taskId, title: session.title, isRunning: true), at: 0)
    }
    return WatchSnapshot(tasks: Array(tasks.prefix(taskLimit)), session: session, generatedAt: now)
  }

  /// Today's day, read as the Today screen reads it, with what the block in
  /// hand was planned to take. Pure over the store; safe off the main actor.
  static func load(store: WorkspaceStore, workspaceID: String, lists: [TaskList], now: Date = .now) throws
    -> WatchSnapshot
  {
    let day = try DaySnapshot.load(store: store, workspaceID: workspaceID, lists: lists, now: now)
    var planned: Int?
    var title: String?
    if let session = day.session, let taskID = session.activeTaskId {
      planned = try store.focusQueue(for: session.id)
        .first { $0.task.id == taskID && $0.item.state == .queued }?.item.plannedSeconds
      title = try store.task(id: taskID)?.title
    }
    return make(day: day, plannedSeconds: planned, runningTitle: title, now: now)
  }
}
