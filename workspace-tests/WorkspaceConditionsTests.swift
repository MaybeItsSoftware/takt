import Foundation
import TestSQLite
import TaktCore
@testable import TaktWorkspace
import XCTest

final class WorkspaceConditionsTests: XCTestCase {
  private var directory: URL!
  private var url: URL!
  private var store: WorkspaceStore!
  private var workspace: Workspace!
  private var inbox: TaskList!
  private let now = Date(timeIntervalSince1970: 1_789_560_000)

  override func setUpWithError() throws {
    directory = FileManager.default.temporaryDirectory.appendingPathComponent("Conditions-\(UUID().uuidString)")
    url = directory.appendingPathComponent("workspace.sqlite")
    store = try WorkspaceStore(databaseURL: url)
    workspace = try store.bootstrapIfNeeded(now: now)
    inbox = try XCTUnwrap(store.inbox(in: workspace.id))
  }

  override func tearDownWithError() throws {
    store = nil
    try? FileManager.default.removeItem(at: directory)
  }

  func testFocusTimelineIncludesUnscoredWorkAndKeepsTitlesAfterDeletion() throws {
    let first = try task("Reading")
    let session = try store.startFocusSession(taskId: first.id, now: now)
    let loggedAt = now.addingTimeInterval(600)
    _ = try store.completeActiveFocusTask(sessionId: session.id, elapsedSeconds: 420,
      completeTask: false, now: loggedAt)
    try store.deleteTask(id: first.id)
    let before = try store.focusWorkBlocks(in: DateInterval(start: now, end: loggedAt))
    XCTAssertTrue(before.isEmpty)
    let included = try store.focusWorkBlocks(in: DateInterval(start: loggedAt, end: loggedAt.addingTimeInterval(60)))
    XCTAssertEqual(included.count, 1)
    XCTAssertEqual(included.first?.taskTitle, "Reading")
    XCTAssertEqual(included.first?.seconds, 420)
    XCTAssertNil(included.first?.taskId)
    XCTAssertEqual(included.first?.originalTaskId, first.id)
  }

  private func task(_ title: String = "Task", estimate: Int? = 3600) throws -> WorkspaceTask {
    let task = try store.createTask(listId: inbox.id, title: title, now: now)
    try store.updateTask(id: task.id, title: title, notes: "", dueAt: nil, estimateSeconds: estimate, now: now)
    return task
  }

  private func edit(_ id: String) throws -> TaskEditorDraft {
    TaskEditorDraft(snapshot: try store.taskEditorSnapshot(for: id))
  }

  func testDefaultConditionsSeedOnceAndRenamingRetainsTaskIdentityThroughUndo() throws {
    let conditions = try store.conditions(in: workspace.id)
    XCTAssertEqual(Set(conditions.map(\.name)), ["Home", "Campus", "Private", "Floor space"])
    try store.bootstrapIfNeeded()
    XCTAssertEqual(try store.conditions(in: workspace.id).count, 4)
    let campus = try XCTUnwrap(conditions.first { $0.name == "Campus" })
    let task = try task()
    var draft = try edit(task.id); draft.values.requirementGroups = [[campus.id]]
    try store.saveTaskEditor(draft)
    try store.saveCondition(id: campus.id, name: "On campus", isLocation: true, isArchived: false)
    XCTAssertEqual(try store.nextUpCandidates(now: now).first?.requirementGroups, [[campus.id]])
    try store.undo()
    XCTAssertEqual(try store.conditions(in: workspace.id).first { $0.id == campus.id }?.name, "Campus")
    try store.redo()
    XCTAssertEqual(try store.conditions(in: workspace.id).first { $0.id == campus.id }?.name, "On campus")
  }

  func testCategoriesGroupConditionsAndAvailableNowOffersOnlyWhatOpenTasksNeed() throws {
    let seeded = try store.conditions(in: workspace.id)
    let home = try XCTUnwrap(seeded.first { $0.name == "Home" })
    XCTAssertEqual(home.category, "Location", "the seeded places are filed under Location")
    let mat = try store.createCondition(workspaceId: workspace.id, name: "Mat", category: "Equipment", now: now)
    XCTAssertEqual(mat.category, "Equipment")
    XCTAssertFalse(mat.isLocation)
    try store.setConditionCategory(id: mat.id, category: "location", now: now)
    var moved = try XCTUnwrap(store.conditions(in: workspace.id).first { $0.id == mat.id })
    XCTAssertEqual(moved.category, "Location")
    XCTAssertTrue(moved.isLocation, "Location is what makes it a place")
    try store.undo()
    moved = try XCTUnwrap(store.conditions(in: workspace.id).first { $0.id == mat.id })
    XCTAssertEqual(moved.category, "Equipment")

    let all = try store.conditions(in: workspace.id)
    XCTAssertEqual(TaskConditionSection.sections(all).map(\.title), ["Location", "Equipment", "Other"])
    XCTAssertEqual(TaskConditionSection.categories(all), ["Location", "Equipment"])

    XCTAssertEqual(try store.requiredConditionIDs(in: workspace.id), [])
    XCTAssertTrue(TaskConditionSection.available(all, required: [], active: []).isEmpty)
    let gym = try task("Gym")
    var draft = try edit(gym.id); draft.values.requirementGroups = [[mat.id]]
    try store.saveTaskEditor(draft)
    let required = try store.requiredConditionIDs(in: workspace.id)
    XCTAssertEqual(required, [mat.id])
    let available = TaskConditionSection.available(all, required: required, active: [home.id])
    XCTAssertEqual(available.map(\.title), ["Location", "Equipment"])
    XCTAssertEqual(available.map { $0.conditions.map(\.id) }, [[home.id], [mat.id]])
    XCTAssertTrue(available[0].isExclusive)

    try store.setStatus(.completed, for: gym.id, now: now)
    XCTAssertEqual(try store.requiredConditionIDs(in: workspace.id), [], "a done task needs nothing")
  }

  /// A device condition holds on a device of its kind by itself, under its
  /// own heading first, and nothing said can switch it.
  func testDeviceConditionHoldsOnlyOnItsKindAndCannotBeSwitched() throws {
    // The kind is process-wide: put it back for the tests after this one.
    let before = ConditionDeviceKind.thisDevice
    defer { ConditionDeviceKind.setThisDevice(before) }
    let laptop = try store.createCondition(workspaceId: workspace.id, name: "Laptop", category: nil, now: now)
    XCTAssertTrue(try store.setConditionDeviceKind(id: laptop.id, kind: "laptop", now: now))
    XCTAssertFalse(try store.setConditionDeviceKind(id: laptop.id, kind: "laptop", now: now))
    let conditions = try store.conditions(in: workspace.id)
    let marked = try XCTUnwrap(conditions.first { $0.id == laptop.id })
    XCTAssertEqual(marked.device, .laptop)
    XCTAssertEqual(TaskConditionSection.sections(conditions).first?.isDevice, true)
    XCTAssertEqual(TaskConditionSection.sections(conditions).first?.conditions.map(\.id), [laptop.id])

    ConditionDeviceKind.setThisDevice(.phone)
    XCTAssertFalse(FocusConditionHoldings.held(conditions, set: [laptop.id], off: []).contains(laptop.id))
    XCTAssertEqual(
      FocusConditionHoldings.switching(laptop.id, on: true, in: conditions, set: [], off: []).set, [])
    ConditionDeviceKind.setThisDevice(.laptop)
    XCTAssertTrue(FocusConditionHoldings.held(conditions, set: [], off: []).contains(laptop.id))
    XCTAssertEqual(FocusConditionHoldings.holdings(conditions, set: [], off: [laptop.id])[laptop.id]?.isDevice, true)

    XCTAssertTrue(try store.setConditionDeviceKind(id: laptop.id, kind: nil, now: now))
    XCTAssertNil(try store.conditions(in: workspace.id).first { $0.id == laptop.id }?.deviceKind)
  }

  func testPlanningSaveAndUndoAreAtomicAndReopenRetainsAllFields() throws {
    let task = try task()
    let conditions = try store.conditions(in: workspace.id)
    var draft = try edit(task.id)
    draft.values.startAt = now.addingTimeInterval(3600)
    draft.values.dueDate = TaskCalendarDate.string(now.addingTimeInterval(86400))
    draft.values.minimumBlockMinutes = "10.5"
    draft.values.requiresSingleSitting = true
    draft.values.requirementGroups = [[conditions[0].id, conditions[1].id], [conditions[2].id]]
    draft.values.title = "Planned"
    let saved = try store.saveTaskEditor(draft)
    XCTAssertEqual(saved.planning?.minimumBlockSeconds, 630)
    XCTAssertEqual(try store.undo(), "Edit Task")
    XCTAssertEqual(try store.taskEditorSnapshot(for: task.id), draft.baseline)
    try store.redo()
    store = try WorkspaceStore(databaseURL: url)
    XCTAssertEqual(try store.taskEditorSnapshot(for: task.id), saved)
  }

  func testInvalidConditionAndLateInjectedFailureRollBackPlanningAndHistory() throws {
    let task = try task()
    var draft = try edit(task.id); draft.values.title = "Lost?"; draft.values.requirementGroups = [["missing"]]
    let label = try store.undoableLabel()
    XCTAssertThrowsError(try store.saveTaskEditor(draft))
    XCTAssertEqual(try store.taskEditorSnapshot(for: task.id), draft.baseline)
    XCTAssertEqual(try store.undoableLabel(), label)
    draft.values.requirementGroups = [[try XCTUnwrap(store.conditions(in: workspace.id).first).id]]
    let required = try XCTUnwrap(draft.values.requirementGroups?.first)
    draft.values.requirementGroups = [required, required]
    XCTAssertThrowsError(try store.saveTaskEditor(draft))
    XCTAssertEqual(try store.taskEditorSnapshot(for: task.id), draft.baseline)
    draft.values.requirementGroups = [required]
    draft.values.dailyProgress = true
    try DatabaseQueue(path: url.path).write { db in
      try db.execute(sql: "CREATE TRIGGER fail_conditions_daily BEFORE INSERT ON dailies BEGIN SELECT RAISE(ABORT, 'failure'); END")
    }
    XCTAssertThrowsError(try store.saveTaskEditor(draft))
    XCTAssertEqual(try store.taskEditorSnapshot(for: task.id), draft.baseline)
    XCTAssertEqual(try store.undoableLabel(), label)
  }

  func testInvalidScheduleMinimumAndOneSittingEstimateAreRejected() throws {
    let task = try task(estimate: nil)
    var draft = try edit(task.id)
    draft.values.dueAt = now; draft.values.startAt = now.addingTimeInterval(1)
    XCTAssertThrowsError(try store.saveTaskEditor(draft))
    draft.values.startAt = nil; draft.values.minimumBlockMinutes = "0.5"
    XCTAssertThrowsError(try store.saveTaskEditor(draft))
    draft.values.minimumBlockMinutes = nil; draft.values.requiresSingleSitting = true
    XCTAssertThrowsError(try store.saveTaskEditor(draft))
    XCTAssertEqual(try store.taskEditorSnapshot(for: task.id), draft.baseline)
  }

  func testArchivedRequirementsRemainButCannotBeNewlyAssigned() throws {
    let condition = try XCTUnwrap(store.conditions(in: workspace.id).first)
    let first = try task("First"), second = try task("Second")
    var draft = try edit(first.id); draft.values.requirementGroups = [[condition.id]]
    try store.saveTaskEditor(draft)
    try store.saveCondition(id: condition.id, name: condition.name, isLocation: condition.isLocation, isArchived: true)
    draft = try edit(first.id); draft.values.title = "Retained"
    XCTAssertNoThrow(try store.saveTaskEditor(draft))
    var another = try edit(second.id); another.values.requirementGroups = [[condition.id]]
    XCTAssertThrowsError(try store.saveTaskEditor(another))
  }

  func testStartChangedOutsideEditorConflictsWithoutLosingNotes() throws {
    let task = try task()
    var draft = try edit(task.id); draft.values.startAt = now; draft.values.notes = "Unsaved notes"
    try store.scheduleTask(id: task.id, startAt: now.addingTimeInterval(3600))
    XCTAssertThrowsError(try store.saveTaskEditor(draft))
    draft.reconcile(with: try store.taskEditorSnapshot(for: task.id))
    XCTAssertTrue(draft.conflicts.contains(.startAt))
    XCTAssertEqual(draft.values.notes, "Unsaved notes")
    draft.resolve(.startAt, useSaved: true)
    try store.saveTaskEditor(draft)
    XCTAssertEqual(try store.taskEditorSnapshot(for: task.id).planning?.startAt, now.addingTimeInterval(3600))
  }

  func testProgressCreditsUnscoredWorkAndKeepsTaskOpenWithRemainingEstimate() throws {
    let task = try task()
    let session = try store.startFocusSession(taskId: task.id, now: now)
    let result = try store.completeActiveFocusTask(sessionId: session.id, elapsedSeconds: 900,
      completeTask: false, expectedBlockId: session.activeBlockId, now: now.addingTimeInterval(900))
    XCTAssertEqual(result.outcome, .progressLogged(seconds: 900))
    XCTAssertEqual(try store.task(id: task.id)?.status, .open)
    XCTAssertEqual(try store.workBlocks(for: task.id).map(\.seconds), [900])
    XCTAssertEqual(try store.nextUpCandidates(now: now).first?.remainingSeconds, 2700)
    XCTAssertTrue(try store.focusAwards().isEmpty)
  }

  func testDuplicateCompletionCannotCreditTwiceOrCompleteNextQueuedTask() throws {
    let first = try task("First"), second = try task("Second")
    let session = try store.startFocusSession(taskId: first.id, plannedSeconds: 600, now: now)
    try store.addToFocusQueue(sessionId: session.id, taskId: second.id, plannedSeconds: 2400, now: now)
    let result = try store.completeActiveFocusTask(sessionId: session.id, elapsedSeconds: 600,
      qualityMultiplier: 1, expectedBlockId: session.activeBlockId, now: now.addingTimeInterval(600))
    XCTAssertEqual(result.session.workDurationSeconds, 2400)
    XCTAssertEqual(result.session.activeTaskId, second.id)
    let retry = try store.completeActiveFocusTask(sessionId: session.id, elapsedSeconds: 600,
      qualityMultiplier: 1, expectedBlockId: session.activeBlockId, now: now.addingTimeInterval(601))
    XCTAssertEqual(retry.session.activeTaskId, second.id)
    XCTAssertEqual(try store.workBlocks(for: first.id).count, 1)
    XCTAssertEqual(try store.focusAwards().count, 1)
    XCTAssertEqual(try store.task(id: second.id)?.status, .open)
  }

  func testPauseResumeAndInterruptedRecoveryExcludeInactiveTime() throws {
    let task = try task()
    let session = try store.startFocusSession(taskId: task.id, now: now)
    try store.pauseFocusSession(id: session.id, now: now.addingTimeInterval(300))
    XCTAssertEqual(try store.activeFocusSession()?.elapsedSeconds(now: now.addingTimeInterval(600)), 300)
    try store.resumeFocusSession(id: session.id, now: now.addingTimeInterval(600))
    try store.checkpointFocusSession(id: session.id, now: now.addingTimeInterval(720))
    try store.recoverInterruptedFocus()
    let recovered = try XCTUnwrap(store.activeFocusSession())
    XCTAssertNotNil(recovered.pausedAt)
    XCTAssertEqual(recovered.elapsedSeconds(now: now.addingTimeInterval(9000)), 420)
    try store.completeActiveFocusTask(sessionId: session.id, elapsedSeconds: recovered.elapsedSeconds(now: now),
      completeTask: false, now: now.addingTimeInterval(9000))
    XCTAssertEqual(try store.workBlocks(for: task.id).first?.seconds, 420)
  }

  func testBlockedQueueRetainsEntriesAndResumesAfterConditionChange() throws {
    let first = try task("Laptop"), second = try task("Home")
    let home = try XCTUnwrap(store.conditions(in: workspace.id).first { $0.name == "Home" })
    var draft = try edit(second.id); draft.values.requirementGroups = [[home.id]]; try store.saveTaskEditor(draft)
    let session = try store.startFocusSession(taskId: first.id, now: now)
    try store.addToFocusQueue(sessionId: session.id, taskId: second.id, plannedSeconds: 1200, now: now)
    let result = try store.completeActiveFocusTask(sessionId: session.id, now: now)
    XCTAssertNil(result.session.activeTaskId)
    XCTAssertEqual(result.session.phase, .running)
    XCTAssertEqual(try store.focusQueue(for: session.id).last?.item.state, .queued)
    try store.resumeEligibleFocusQueue(context: FocusContext(conditionIDs: [home.id]), now: now)
    XCTAssertEqual(try store.activeFocusSession()?.activeTaskId, second.id)
    XCTAssertEqual(try store.activeFocusSession()?.workDurationSeconds, 1200)
  }

  /// The app asks before it writes: a resume that resumes nothing is still a
  /// write, and a write re-ranks the day, which asked again — a loop for as
  /// long as the queue stayed blocked.
  func testBlockedQueueReportsNothingResumableAndResumingWritesNothing() throws {
    let first = try task("Laptop"), second = try task("Home")
    let home = try XCTUnwrap(store.conditions(in: workspace.id).first { $0.name == "Home" })
    var draft = try edit(second.id); draft.values.requirementGroups = [[home.id]]; try store.saveTaskEditor(draft)
    let session = try store.startFocusSession(taskId: first.id, now: now)
    try store.addToFocusQueue(sessionId: session.id, taskId: second.id, plannedSeconds: 1200, now: now)
    _ = try store.completeActiveFocusTask(sessionId: session.id, now: now)
    let blocked = try XCTUnwrap(store.activeFocusSession())
    XCTAssertNil(blocked.activeTaskId)

    let elsewhere = FocusContext()
    XCTAssertFalse(try store.hasResumableFocusQueueTask(context: elsewhere, now: now))
    let later = now.addingTimeInterval(60)
    try store.resumeEligibleFocusQueue(context: elsewhere, now: later)
    XCTAssertEqual(try store.activeFocusSession(), blocked)
    XCTAssertEqual(try store.focusQueue(for: session.id).last?.item.state, .queued)

    XCTAssertTrue(try store.hasResumableFocusQueueTask(context: FocusContext(conditionIDs: [home.id]), now: now))
    XCTAssertNil(try store.activeFocusSession()?.activeTaskId, "asking is a read")
  }

  func testDailyPartialProgressUsesTodaysTargetWithoutDoubleCountingAwards() throws {
    let task = try task()
    let daily = try store.makeDaily(taskId: task.id, targetSeconds: 1800, now: now)
    let session = try store.startFocusSession(taskId: task.id, now: now)
    try store.completeActiveFocusTask(sessionId: session.id, elapsedSeconds: 600, qualityMultiplier: 1,
      completeTask: false, now: now.addingTimeInterval(600))
    XCTAssertEqual(try store.nextUpCandidates(now: now).first?.remainingSeconds, 1200)
    XCTAssertEqual(try store.workBlocks(for: task.id).reduce(0) { $0 + $1.seconds }, 600)
    XCTAssertEqual(try store.contributionHistory(dailyId: daily.id, days: 1, endingOn: now).first?.secondsLogged, 600)
    let second = try store.startFocusSession(taskId: task.id, now: now.addingTimeInterval(600))
    try store.completeActiveFocusTask(sessionId: second.id, elapsedSeconds: 1200,
      completeTask: false, now: now.addingTimeInterval(1800))
    XCTAssertTrue(try store.nextUpCandidates(now: now).isEmpty)
    XCTAssertEqual(try store.task(id: task.id)?.status, .open)
    XCTAssertEqual(try store.workBlocks(for: task.id).reduce(0) { $0 + $1.seconds }, 1800)
  }

  func testHistorySurvivesTaskDeletion() throws {
    let task = try task()
    let session = try store.startFocusSession(taskId: task.id, now: now)
    try store.completeActiveFocusTask(sessionId: session.id, elapsedSeconds: 600, completeTask: false, now: now)
    try store.deleteTask(id: task.id)
    let blocks = try store.focusWorkBlocks(in: DateInterval(start: .distantPast, end: .distantFuture))
    XCTAssertEqual(blocks.first?.taskTitle, "Task")
    XCTAssertNil(blocks.first?.taskId)
    XCTAssertEqual(blocks.first?.seconds, 600)
    try store.undo()
    XCTAssertEqual(try store.nextUpCandidates(now: now).first?.remainingSeconds, 3000)
  }

  func testOldDraftPayloadMigratesWithoutDiscardingUnsavedNotes() throws {
    let task = try task()
    var draft = try edit(task.id); draft.values.notes = "Retained from v1"
    let disk = TaskEditorDraftStore(fileURL: directory.appendingPathComponent("drafts.json"))
    try disk.save([draft])
    var object = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: disk.fileURL)) as? [String: Any])
    object["version"] = 1
    try JSONSerialization.data(withJSONObject: object).write(to: disk.fileURL)
    let restored = try XCTUnwrap(disk.load().first)
    XCTAssertEqual(restored.values.notes, "Retained from v1")
    XCTAssertNil(restored.values.startAt)
    XCTAssertEqual(try store.saveTaskEditor(restored).notes, "Retained from v1")
  }

  func testCrossWorkspaceConditionAndNoOpHistoryAreHandled() throws {
    let task = try task()
    let foreign = try DatabaseQueue(path: url.path).write { db -> TaskCondition in
      let other = Workspace(id: UUID().uuidString, name: "Other", createdAt: now, updatedAt: now)
      try db.execute(
        sql: "INSERT INTO workspaces (id, name, createdAt, updatedAt) VALUES (?, ?, ?, ?)",
        arguments: [other.id, other.name, now, now])
      let record = TaskCondition(id: UUID().uuidString, workspaceId: other.id, name: "Other", isLocation: false,
        isArchived: false, createdAt: now, updatedAt: now)
      try db.execute(
        sql: """
          INSERT INTO task_conditions (id, workspaceId, name, isLocation, isArchived, createdAt, updatedAt)
          VALUES (?, ?, ?, 0, 0, ?, ?)
          """,
        arguments: [record.id, record.workspaceId, record.name, now, now])
      return record
    }
    var draft = try edit(task.id); draft.values.requirementGroups = [[foreign.id]]
    XCTAssertThrowsError(try store.saveTaskEditor(draft))
    draft.values.requirementGroups = [[try XCTUnwrap(store.conditions(in: workspace.id).first).id]]
    try store.saveTaskEditor(draft)
    try store.undo()
    try store.saveTaskEditor(try edit(task.id))
    XCTAssertEqual(try store.redoableLabel(), "Edit Task")
  }
  func testAutomaticStartRechecksConditionsAndRequestedDurationInTransaction() throws {
    let task = try task()
    let home = try XCTUnwrap(store.conditions(in: workspace.id).first { $0.name == "Home" })
    var draft = try edit(task.id); draft.values.requirementGroups = [[home.id]]
    draft.values.minimumBlockMinutes = "10"; try store.saveTaskEditor(draft)
    XCTAssertThrowsError(try store.startFocusSession(taskId: task.id, plannedSeconds: 600,
      context: FocusContext(), now: now))
    XCTAssertNil(try store.activeFocusSession())
    XCTAssertThrowsError(try store.startFocusSession(taskId: task.id, plannedSeconds: 300,
      context: FocusContext(conditionIDs: [home.id]), now: now))
    XCTAssertNoThrow(try store.startFocusSession(taskId: task.id, plannedSeconds: 600,
      context: FocusContext(conditionIDs: [home.id]), now: now))
  }

  func testSingleSittingQueueUsesFullRequiredDurationAndCanRequeuePartialWork() throws {
    let first = try task("First"), second = try task("Second")
    var draft = try edit(second.id); draft.values.requiresSingleSitting = true
    try store.saveTaskEditor(draft)
    let session = try store.startFocusSession(taskId: first.id, plannedSeconds: 600, now: now)
    try store.addToFocusQueue(sessionId: session.id, taskId: second.id, plannedSeconds: 300, now: now)
    let result = try store.completeActiveFocusTask(sessionId: session.id, elapsedSeconds: 600,
      completeTask: false, now: now)
    XCTAssertEqual(result.session.workDurationSeconds, 3600)
    try store.addToFocusQueue(sessionId: session.id, taskId: first.id, plannedSeconds: 600, now: now)
    XCTAssertEqual(try store.focusQueue(for: session.id).count, 3)
  }

  func testApplyingSavedPlanningToSubtasksDoesNotChangeTheirDeadlinesOrEstimates() throws {
    let parent = try task("Parent")
    let child = try store.createTask(listId: inbox.id, title: "Child", parentTaskId: parent.id, now: now)
    try store.updateTask(id: child.id, title: child.title, notes: "", dueAt: now.addingTimeInterval(7200), estimateSeconds: 900, now: now)
    let home = try XCTUnwrap(store.conditions(in: workspace.id).first)
    var draft = try edit(parent.id); draft.values.requirementGroups = [[home.id]]; draft.values.startAt = now
    draft.values.dueDate = TaskCalendarDate.string(now.addingTimeInterval(86400)); try store.saveTaskEditor(draft)
    try store.applyPlanningToDescendants(of: parent.id)
    let saved = try store.taskEditorSnapshot(for: child.id)
    XCTAssertEqual(saved.planning?.requirementGroups, [[home.id]])
    XCTAssertEqual(saved.planning?.startAt, now)
    XCTAssertNil(saved.planning?.dueDate)
    XCTAssertEqual(saved.dueAt, now.addingTimeInterval(7200))
    XCTAssertEqual(saved.estimateSeconds, 900)
    try store.undo()
    XCTAssertNil(try store.taskEditorSnapshot(for: child.id).planning)
  }

  func testNewSchemaReplaysPreUpgradeMetadataSnapshotsAndBackfillsOnlyKnownWork() throws {
    let task = try task()
    try store.updateTaskEditorMetadata(taskId: task.id, metadata: .init(tags: ["Before upgrade"]))
    let session = try store.startFocusSession(taskId: task.id, now: now)
    try store.completeActiveFocusTask(sessionId: session.id, elapsedSeconds: 600, qualityMultiplier: 1, completeTask: false, now: now)
    try DatabaseQueue(path: url.path).write { db in
      try db.rollBackSyncMigration()
      for table in ["task_conditions", "task_metadata"] {
        for operation in ["insert", "update", "delete"] { try db.execute(sql: "DROP TRIGGER IF EXISTS change_log_\(table)_\(operation)") }
      }
      try db.execute(sql: "DROP TABLE task_conditions")
      try db.execute(sql: "DROP TABLE focus_work_blocks")
      try db.execute(sql: "ALTER TABLE task_metadata DROP COLUMN planningJSON")
      for column in ["activeBlockId", "accumulatedSeconds", "pausedAt", "checkpointAt"] {
        try db.execute(sql: "ALTER TABLE focus_sessions DROP COLUMN \(column)")
      }
      try db.execute(sql: "UPDATE change_log SET beforeJSON = json_remove(beforeJSON, '$.planningJSON'), afterJSON = json_remove(afterJSON, '$.planningJSON') WHERE tableName = 'task_metadata'")
      try db.execute(sql: "DELETE FROM grdb_migrations WHERE identifier = 'v12_task_conditions_and_work'")
    }
    store = try WorkspaceStore(databaseURL: url)
    XCTAssertEqual(try store.workBlocks(for: task.id).map(\.seconds), [600])
    XCTAssertEqual(try store.conditions(in: workspace.id).count, 4)
    try store.undo()
    XCTAssertTrue(try store.taskEditorMetadata(for: task.id).tags.isEmpty)
    try store.redo()
    XCTAssertEqual(try store.taskEditorMetadata(for: task.id).tags, ["Before upgrade"])
  }

  func testMeetingDailyTargetDoesNotHideItsOutstandingDeadline() throws {
    let task = try task()
    try store.updateTask(id: task.id, title: task.title, notes: "", dueAt: now.addingTimeInterval(-86400), estimateSeconds: 3600)
    let daily = try store.makeDaily(taskId: task.id, targetSeconds: 600, now: now)
    try store.logContribution(dailyId: daily.id, seconds: 600, now: now)
    let ranking = NextUpSelector.evaluate(try store.nextUpCandidates(now: now), now: now)
    XCTAssertTrue(ranking.ranked.isEmpty)
    XCTAssertEqual(ranking.blocked.first?.id, task.id)
    XCTAssertEqual(ranking.blocked.first?.reasons, [.dailyAlreadyMet])
    XCTAssertEqual(NextUpSelector.score(try XCTUnwrap(ranking.blocked.first?.candidate), now: now).reason, .overdue)
  }

  func testRebasingWallClockKeepsActualWorkAndPausedTimeStable() throws {
    let task = try task()
    let session = try store.startFocusSession(taskId: task.id, now: now)
    let adjustedNow = now.addingTimeInterval(7201)
    try store.rebaseFocusClock(id: session.id, elapsedSeconds: 601, now: adjustedNow)
    XCTAssertEqual(try store.activeFocusSession()?.elapsedSeconds(now: adjustedNow.addingTimeInterval(3)), 604)
    try store.pauseFocusSession(id: session.id, now: adjustedNow.addingTimeInterval(3))
    try store.rebaseFocusClock(id: session.id, elapsedSeconds: 9000, now: adjustedNow.addingTimeInterval(9000))
    let paused = try XCTUnwrap(store.activeFocusSession())
    XCTAssertEqual(paused.elapsedSeconds(now: adjustedNow.addingTimeInterval(9000)), 604)
    try store.completeActiveFocusTask(sessionId: session.id, elapsedSeconds: 604, completeTask: false, now: adjustedNow)
    XCTAssertEqual(try store.workBlocks(for: task.id).first?.seconds, 604)
  }

  func testLegacyTaskUpdateSwitchesCalendarDeadlineToExactTimeWithoutLosingConditions() throws {
    let task = try task()
    let home = try XCTUnwrap(store.conditions(in: workspace.id).first)
    var draft = try edit(task.id); draft.values.dueDate = TaskCalendarDate.string(now.addingTimeInterval(86400))
    draft.values.requirementGroups = [[home.id]]; try store.saveTaskEditor(draft)
    let deadline = now.addingTimeInterval(7200)
    try store.updateTask(id: task.id, title: task.title, notes: "Changed", dueAt: deadline, estimateSeconds: 3600)
    let saved = try store.taskEditorSnapshot(for: task.id)
    XCTAssertNil(saved.planning?.dueDate)
    XCTAssertEqual(saved.dueAt, deadline)
    XCTAssertEqual(saved.planning?.requirementGroups, [[home.id]])
    try store.undo()
    XCTAssertNotNil(try store.taskEditorSnapshot(for: task.id).planning?.dueDate)
  }

  func testFocusDeferralCannotSilentlyScheduleAfterDeadlineAndNoOpKeepsRedo() throws {
    let task = try task()
    try store.updateTask(id: task.id, title: task.title, notes: "", dueAt: now.addingTimeInterval(3600), estimateSeconds: 3600)
    XCTAssertThrowsError(try store.scheduleTask(id: task.id, startAt: now.addingTimeInterval(7200)))
    XCTAssertNil(try store.taskEditorSnapshot(for: task.id).planning)
    try store.scheduleTask(id: task.id, startAt: now.addingTimeInterval(1800))
    try store.undo()
    try store.scheduleTask(id: task.id, startAt: nil)
    XCTAssertEqual(try store.redoableLabel(), "Schedule Task")
  }

}
