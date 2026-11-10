import TaktWorkspace
import XCTest
@testable import Takt

/// What the widgets and the Live Activity are told about the workspace.
final class WidgetSnapshotTests: XCTestCase {
  private var store: WorkspaceStore!
  private var workspaceID: String!
  private var listID: String!

  override func setUpWithError() throws {
    let url = FileManager.default.temporaryDirectory
      .appending(path: "widget-\(UUID().uuidString)/priority.sqlite")
    store = try WorkspaceStore(databaseURL: url)
    workspaceID = try store.bootstrapIfNeeded().id
    listID = try store.createList(workspaceId: workspaceID, name: "Work").id
  }

  func testPlannedTasksAreTheDayInOrderWithWhatIsLeft() throws {
    let first = try store.createTask(listId: listID, title: "Write", estimateSeconds: 1_800)
    let second = try store.createTask(listId: listID, title: "Review", estimateSeconds: 600)
    _ = try store.createTask(listId: listID, title: "Not today")
    try store.setPlannedForToday(true, taskIds: [first.id, second.id])
    try store.arrangeDay(orderedTaskIds: [second.id, first.id])

    let snapshot = try WidgetSnapshotBuilder.make(store: store, workspaceID: workspaceID)

    XCTAssertEqual(snapshot.items.map(\.title), ["Review", "Write"])
    XCTAssertEqual(snapshot.items.first?.reason, "Planned")
    XCTAssertEqual(snapshot.items.first?.listName, "Work")
    XCTAssertEqual(snapshot.todayCount, 2)
    XCTAssertEqual(snapshot.remainingSeconds, 2_400)
    XCTAssertEqual(snapshot.nextUp?.title, "Review")
    XCTAssertNil(snapshot.running)
  }

  func testTheRankingFallbackSaysWhyEachTaskIsUpNext() throws {
    _ = try store.createTask(listId: listID, title: "Urgent", dueAt: Date.now.addingTimeInterval(-86_400 * 2))
    let snapshot = try WidgetSnapshotBuilder.make(store: store, workspaceID: workspaceID)
    XCTAssertEqual(snapshot.items.first?.title, "Urgent")
    XCTAssertEqual(snapshot.items.first?.reason, "Overdue")
    XCTAssertEqual(WidgetSnapshotBuilder.why(.priority), "High priority")
    XCTAssertNil(WidgetSnapshotBuilder.why(.order))
  }

  func testAnUnplannedDayFallsBackToTheRankingWithoutReasons() throws {
    _ = try store.createTask(listId: listID, title: "Something")
    let snapshot = try WidgetSnapshotBuilder.make(store: store, workspaceID: workspaceID)
    XCTAssertEqual(snapshot.todayCount, 0)
    XCTAssertEqual(snapshot.items.map(\.title), ["Something"])
    XCTAssertNil(snapshot.items.first?.reason)
  }

  func testARunningBlockIsReportedAndIsNotNextUp() throws {
    let task = try store.createTask(listId: listID, title: "Deep work", estimateSeconds: 3_000)
    let other = try store.createTask(listId: listID, title: "After")
    try store.setPlannedForToday(true, taskIds: [task.id, other.id])
    let started = Date().addingTimeInterval(-120)
    let session = try store.startFocusSession(
      taskId: task.id, plannedSeconds: 1_500, overrideAvailability: true, now: started)

    let snapshot = try WidgetSnapshotBuilder.make(store: store, workspaceID: workspaceID)
    XCTAssertEqual(snapshot.running?.title, "Deep work")
    XCTAssertEqual(snapshot.running?.isPaused, false)
    XCTAssertEqual(snapshot.nextUp?.title, "After")

    let focus = try XCTUnwrap(WidgetSnapshotBuilder.focusState(store: store))
    XCTAssertEqual(focus.sessionID, session.id)
    XCTAssertEqual(focus.state.taskTitle, "Deep work")
    XCTAssertEqual(focus.state.plannedSeconds, 1_500)
    XCTAssertEqual(focus.state.elapsedSeconds, 120, accuracy: 3)

    try store.pauseFocusSession(id: session.id)
    let paused = try XCTUnwrap(WidgetSnapshotBuilder.focusState(store: store))
    XCTAssertTrue(paused.state.isPaused)
    XCTAssertTrue(FocusLiveActivityController.differs(focus.state, paused.state))
  }

  func testATimerCommitsTheLiveActivityToNoLength() throws {
    let task = try store.createTask(listId: listID, title: "Open ended", estimateSeconds: 3_000)
    let session = try store.startFocusTimer(
      taskId: task.id, overrideAvailability: true, now: Date().addingTimeInterval(-90))
    XCTAssertTrue(session.openEnded)

    let focus = try XCTUnwrap(WidgetSnapshotBuilder.focusState(store: store))
    XCTAssertNil(focus.state.plannedSeconds, "a timer has no end to draw progress towards")
    XCTAssertNil(focus.state.plannedEnd)
    XCTAssertEqual(focus.state.elapsedSeconds, 90, accuracy: 3)
  }

  func testTheLiveActivityIsNotUpdatedJustBecauseTimePassed() {
    let start = Date()
    let state = FocusActivityAttributes.ContentState(
      taskID: "t", taskTitle: "T", timerStart: start, isPaused: false, elapsedSeconds: 10, plannedSeconds: 600)
    var later = state
    later.elapsedSeconds = 40
    later.timerStart = start.addingTimeInterval(0.5)
    XCTAssertFalse(FocusLiveActivityController.differs(state, later))
    later.taskTitle = "Renamed"
    XCTAssertTrue(FocusLiveActivityController.differs(state, later))
  }

  func testTheSnapshotRoundTripsAndComparesWithoutItsTimestamp() throws {
    let url = FileManager.default.temporaryDirectory.appending(path: "snap-\(UUID().uuidString).json")
    try WidgetSnapshot.sample.write(to: url)
    let loaded = try XCTUnwrap(WidgetSnapshot.load(from: url))
    XCTAssertTrue(loaded.hasSameContent(as: WidgetSnapshot.sample))
    var older = loaded
    older.generatedAt = .distantPast
    XCTAssertTrue(older.hasSameContent(as: loaded))
    older.todayCount += 1
    XCTAssertFalse(older.hasSameContent(as: loaded))
  }
}
