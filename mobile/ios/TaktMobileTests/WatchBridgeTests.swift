import TaktCore
import TaktWorkspace
import XCTest
@testable import Takt

/// What the Apple Watch is told about the day, and what its taps do.
@MainActor
final class WatchBridgeTests: XCTestCase {
  private var model: WorkspaceModel!
  private var listID: String!

  override func setUp() async throws {
    model = try WorkspaceModel.temporary()
    listID = try XCTUnwrap(model.inbox?.id)
  }

  private func add(_ title: String, estimate: Int? = nil, due: Date? = nil) throws -> WorkspaceTask {
    try model.store.createTask(listId: listID, title: title, dueAt: due, estimateSeconds: estimate)
  }

  private func snapshot(now: Date = .now) throws -> WatchSnapshot {
    try WatchSnapshotBuilder.load(
      store: model.store, workspaceID: model.workspace.id,
      lists: model.structure.lists + model.structure.archivedLists, now: now)
  }

  private func apply(_ kind: WatchAction.Kind, _ taskID: String? = nil, issuedAt: Date = .now) async {
    await WatchBridge.shared.apply(WatchAction(kind: kind, taskId: taskID, issuedAt: issuedAt), model: model)
  }

  // MARK: - The wire

  func testSnapshotSurvivesTheApplicationContext() throws {
    let start = Date(timeIntervalSince1970: 1_800_000_000.250)
    let snapshot = WatchSnapshot(
      tasks: [
        .init(id: "a", title: "Write", detail: "Overdue", isRunning: true, estimateSeconds: 1_800),
        .init(id: "b", title: "Stretch", detail: "Daily", isDaily: true, dailyDone: false),
      ],
      session: .init(
        taskId: "a", title: "Write", phase: .running, startedAt: start, accumulatedSeconds: 90, plannedSeconds: 1_500),
      generatedAt: start.addingTimeInterval(5))
    let context = try snapshot.context()
    XCTAssertTrue(context[WatchSnapshot.contextKey] is Data, "carried as plain data")
    XCTAssertEqual(WatchSnapshot(context: context), snapshot)
    XCTAssertNil(WatchSnapshot(context: [:]))
    XCTAssertNil(WatchSnapshot(context: [WatchSnapshot.contextKey: Data("{}".utf8)]))
  }

  func testSnapshotJSONIsTheSharedShape() throws {
    let snapshot = WatchSnapshot(
      tasks: [.init(id: "a", title: "Write")],
      session: .init(
        taskId: "a", title: "Write", phase: .paused, startedAt: Date(timeIntervalSince1970: 2),
        accumulatedSeconds: 60, plannedSeconds: nil),
      generatedAt: Date(timeIntervalSince1970: 1))
    let json = try XCTUnwrap(String(data: snapshot.encoded(), encoding: .utf8))
    XCTAssertEqual(
      json,
      #"{"generatedAt":1000,"session":{"accumulatedSeconds":60,"phase":"paused","startedAt":2000,"taskId":"a","title":"Write"},"#
        + #""tasks":[{"dailyDone":false,"id":"a","isDaily":false,"isRunning":false,"title":"Write"}]}"#)
  }

  func testActionsRoundTripAndRejectWhatTheyCannotDo() throws {
    for kind in WatchAction.Kind.allCases {
      let action = WatchAction(kind: kind, taskId: "t", issuedAt: Date(timeIntervalSince1970: 1_000))
      XCTAssertEqual(WatchAction(message: action.message), action)
    }
    XCTAssertNil(WatchAction(message: ["action": "complete"]), "a completion names its task")
    XCTAssertNil(WatchAction(message: ["action": "explode", "taskId": "t"]))
    XCTAssertEqual(WatchAction(message: ["action": "pause"])?.kind, .pause, "the bare shape is enough")
  }

  func testQueuedTimerActionsGoStaleButTicksDoNot() {
    let tapped = Date(timeIntervalSince1970: 0)
    let later = tapped.addingTimeInterval(WatchAction.timerActionLifetime + 1)
    XCTAssertFalse(WatchAction(kind: .pause, issuedAt: tapped).isCurrent(at: later))
    XCTAssertTrue(WatchAction(kind: .pause, issuedAt: tapped).isCurrent(at: tapped.addingTimeInterval(60)))
    XCTAssertTrue(WatchAction(kind: .complete, taskId: "t", issuedAt: tapped).isCurrent(at: later))
    XCTAssertTrue(WatchAction(kind: .logDaily, taskId: "t", issuedAt: tapped).isCurrent(at: later))
  }

  func testTheClockIsDrawnFromTheSessionsTwoNumbers() {
    let start = Date(timeIntervalSince1970: 1_000)
    let running = WatchSnapshot.Session(
      taskId: "a", title: "A", phase: .running, startedAt: start, accumulatedSeconds: 60, plannedSeconds: 600)
    XCTAssertEqual(running.elapsedSeconds(at: start.addingTimeInterval(30)), 90)
    XCTAssertEqual(running.timerStart, start.addingTimeInterval(-60))
    XCTAssertEqual(running.plannedEnd, start.addingTimeInterval(540))
    let paused = running.paused(at: start.addingTimeInterval(30))
    XCTAssertEqual(paused.elapsedSeconds(at: start.addingTimeInterval(500)), 90)
    XCTAssertNil(paused.plannedEnd)
    XCTAssertEqual(paused.resumed(at: start.addingTimeInterval(100)).elapsedSeconds(at: start.addingTimeInterval(110)), 100)
  }

  // MARK: - Shaping the day

  func testTheDayIsTodaysCardsInOrderThenOwedDailies() throws {
    let planned = try add("Planned", estimate: 1_800)
    let overdue = try add("Overdue", due: Calendar.current.date(byAdding: .day, value: -2, to: .now))
    _ = try add("Unrelated")
    let practice = try add("Practice")
    try model.store.makeDaily(taskId: practice.id)
    try model.store.setPlannedForToday(true, taskIds: [planned.id])
    model.reloadStructureNow()

    let snapshot = try snapshot()
    XCTAssertEqual(snapshot.tasks.prefix(2).map(\.id), [planned.id, overdue.id])
    XCTAssertEqual(snapshot.tasks.first?.detail, model.inbox?.name, "nothing more pressing than its list")
    XCTAssertEqual(snapshot.tasks.first?.estimateSeconds, 1_800)
    XCTAssertEqual(snapshot.tasks[1].detail, "Overdue")
    let daily = try XCTUnwrap(snapshot.tasks.first { $0.id == practice.id })
    XCTAssertTrue(daily.isDaily)
    XCTAssertFalse(snapshot.tasks.contains { $0.title == "Unrelated" })
    XCTAssertNil(snapshot.session)
  }

  func testTheRunningBlockLeadsAndCarriesItsClock() async throws {
    let first = try add("First", estimate: 600)
    let second = try add("Second", estimate: 1_200)
    try model.store.setPlannedForToday(true, taskIds: [first.id, second.id])
    await apply(.start, second.id)

    let snapshot = try snapshot()
    XCTAssertEqual(snapshot.tasks.first?.id, second.id)
    XCTAssertEqual(snapshot.tasks.filter(\.isRunning).map(\.id), [second.id])
    let session = try XCTUnwrap(snapshot.session)
    XCTAssertEqual(session.taskId, second.id)
    XCTAssertEqual(session.title, "Second")
    XCTAssertEqual(session.phase, .running)
    XCTAssertNotNil(session.plannedSeconds)
    XCTAssertLessThan(session.elapsedSeconds(at: .now), 5)
  }

  func testATimerReachesTheWatchWithNoLengthSoItCountsUp() throws {
    let task = try add("Timed", estimate: 600)
    try model.store.setPlannedForToday(true, taskIds: [task.id])
    _ = try model.store.startFocusTimer(taskId: task.id, overrideAvailability: true)

    let session = try XCTUnwrap(try snapshot().session)
    XCTAssertEqual(session.taskId, task.id)
    XCTAssertNil(session.plannedSeconds, "not the day a timer stores for older clients")
    XCTAssertNil(session.plannedEnd)
  }

  func testADailyLoggedTodayLeavesTheWatch() throws {
    let practice = try add("Practice")
    let daily = try model.store.makeDaily(taskId: practice.id)
    try model.store.logContribution(dailyId: daily.id)
    XCTAssertFalse(try snapshot().tasks.contains { $0.id == practice.id })
  }

  func testShapingCapsTheList() {
    var day = DaySnapshot()
    day.cards = (0..<50).map {
      DayCard(
        id: "\($0)", title: "Task \($0)", listName: "Inbox", listColorHex: nil, reason: .planned, estimateSeconds: nil,
        loggedSeconds: 0, dueAt: nil, isList: false, dailyID: nil, isDailyDoneToday: false, isRunning: false)
    }
    XCTAssertEqual(WatchSnapshotBuilder.make(day: day).tasks.count, WatchSnapshotBuilder.taskLimit)
  }

  // MARK: - Applying the watch's taps

  func testCompletingFromTheWatchIsAnUndoableCompletion() async throws {
    let task = try add("Write")
    await apply(.complete, task.id)
    XCTAssertEqual(try model.store.task(id: task.id)?.status, .completed)
    XCTAssertNotNil(model.undoLabel)
    model.undo()
    XCTAssertEqual(try model.store.task(id: task.id)?.status, .open)
  }

  func testCompletingADailyLogsTodayAndKeepsItOpen() async throws {
    let practice = try add("Practice")
    try model.store.makeDaily(taskId: practice.id)
    await apply(.complete, practice.id)
    XCTAssertEqual(try model.store.task(id: practice.id)?.status, .open)
    XCTAssertTrue(try model.store.dailies(on: .now).first { $0.task.id == practice.id }?.isDoneToday ?? false)
    await apply(.logDaily, practice.id)
    XCTAssertEqual(try model.store.dailies(on: .now).count, 1, "logging twice is still one day")
  }

  func testPauseResumeAndStopDriveThePhonesBlock() async throws {
    let task = try add("Write", estimate: 900)
    await apply(.start, task.id)
    XCTAssertEqual(try model.store.activeFocusSession()?.activeTaskId, task.id)

    await apply(.pause)
    XCTAssertNotNil(try model.store.activeFocusSession()?.pausedAt)
    await apply(.pause)
    XCTAssertNotNil(try model.store.activeFocusSession()?.pausedAt, "pausing a paused block does not resume it")
    await apply(.resume)
    XCTAssertNil(try model.store.activeFocusSession()?.pausedAt)
    await apply(.resume)
    XCTAssertNil(try model.store.activeFocusSession()?.pausedAt, "resuming a running block does not pause it")

    await apply(.stop)
    XCTAssertNil(try model.store.activeFocusSession())
    XCTAssertEqual(try model.store.task(id: task.id)?.status, .open, "stopping keeps the task")
  }

  func testDoneCompletesTheTaskAndEndsTheBlock() async throws {
    let task = try add("Write", estimate: 900)
    await apply(.start, task.id)
    await apply(.done)
    XCTAssertEqual(try model.store.task(id: task.id)?.status, .completed)
    XCTAssertNil(try model.store.activeFocusSession()?.activeTaskId)
    XCTAssertNil(model.pendingBlock)
  }

  func testATimerActionForABlockNoLongerRunningIsIgnored() async throws {
    let first = try add("First", estimate: 900)
    let second = try add("Second", estimate: 900)
    await apply(.start, first.id)
    await apply(.stop, first.id)
    await apply(.start, second.id)

    await apply(.done, first.id)
    await apply(.pause, first.id)
    await apply(.stop, first.id)
    XCTAssertEqual(try model.store.activeFocusSession()?.activeTaskId, second.id, "the new block runs on")
    XCTAssertNil(try model.store.activeFocusSession()?.pausedAt)
    XCTAssertEqual(try model.store.task(id: second.id)?.status, .open)

    await apply(.done, second.id)
    XCTAssertEqual(try model.store.task(id: second.id)?.status, .completed, "naming the running task acts")
  }

  func testDoneTakesNothingFromAQuestionAboutAnotherSession() async throws {
    let task = try add("Write", estimate: 900)
    await apply(.start, task.id)
    // Its block id names no block of this session, so borrowing it would
    // leave the running block unfinished.
    model.pendingBlock = PendingBlockCompletion(
      sessionID: "elsewhere", taskID: task.id, title: "Write", seconds: 7_200, completeTask: false,
      blockID: "stale", wasPaused: false)
    await apply(.done, task.id)
    XCTAssertEqual(try model.store.task(id: task.id)?.status, .completed)
    XCTAssertNil(try model.store.activeFocusSession()?.activeTaskId)
    XCTAssertNil(model.pendingBlock)
  }

  func testDoneAnswersAQuestionAlreadyUpOnThePhone() async throws {
    let task = try add("Write", estimate: 900)
    await apply(.start, task.id)
    let session = try XCTUnwrap(model.store.activeFocusSession())
    model.requestBlockCompletion(session: session, title: "Write", completeTask: false)
    XCTAssertNotNil(model.pendingBlock)
    await apply(.complete, task.id)
    XCTAssertNil(model.pendingBlock)
    XCTAssertEqual(try model.store.task(id: task.id)?.status, .completed)
  }

  func testResumingWhileThePhoneAsksHowItWentLetsTheQuestionGo() async throws {
    let task = try add("Write", estimate: 900)
    await apply(.start, task.id)
    let session = try XCTUnwrap(model.store.activeFocusSession())
    model.requestBlockCompletion(session: session, title: "Write", completeTask: true)
    await apply(.resume)
    XCTAssertNil(model.pendingBlock)
    XCTAssertNil(try model.store.activeFocusSession()?.pausedAt)
    XCTAssertEqual(try model.store.task(id: task.id)?.status, .open)
  }

  func testAnActionDeliveredTwiceLandsOnce() async throws {
    let task = try add("Write", estimate: 900)
    await apply(.start, task.id)
    await apply(.pause)
    let resume = WatchAction(kind: .resume)
    await WatchBridge.shared.receive(resume.message, model: model, reply: nil)
    XCTAssertNil(try model.store.activeFocusSession()?.pausedAt)
    await apply(.pause)
    await WatchBridge.shared.receive(resume.message, model: model, reply: nil)
    XCTAssertNotNil(try model.store.activeFocusSession()?.pausedAt, "the second delivery is not a second resume")
  }
}
