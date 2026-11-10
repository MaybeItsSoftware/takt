import Foundation
import TaktCore
import TaktWorkspace
import XCTest

final class WorkspaceDailyTests: XCTestCase {
  private var directoryURL: URL!
  private var store: WorkspaceStore!
  private var workspace: Workspace!
  private var list: TaskList!
  private let calendar = Calendar(identifier: .gregorian)

  override func setUpWithError() throws {
    directoryURL = FileManager.default.temporaryDirectory
      .appendingPathComponent("TaktDailyTests-\(UUID().uuidString)", isDirectory: true)
    store = try WorkspaceStore(databaseURL: directoryURL.appendingPathComponent("priority.sqlite"))
    workspace = try store.bootstrapIfNeeded()
    list = try XCTUnwrap(store.lists(in: workspace.id).first)
  }

  override func tearDownWithError() throws {
    try? FileManager.default.removeItem(at: directoryURL)
  }

  func testMakingATaskDailyIsIdempotentAndArchivingLeavesTheTaskAlone() throws {
    let task = try store.createTask(listId: list.id, title: "Write 500 words")

    let first = try store.makeDaily(taskId: task.id)
    let second = try store.makeDaily(taskId: task.id)

    XCTAssertEqual(first.id, second.id)
    XCTAssertEqual(try store.allDailies().count, 1)

    try store.archiveDaily(taskId: task.id)

    XCTAssertEqual(try store.allDailies(), [])
    XCTAssertEqual(try store.task(id: task.id)?.status, .open)
  }

  func testTickingADailyLogsAContributionWithoutCompletingTheTask() throws {
    let task = try store.createTask(listId: list.id, title: "Write 500 words")
    let daily = try store.makeDaily(taskId: task.id)

    try store.logContribution(dailyId: daily.id, seconds: 1_500)

    let items = try store.dailies()
    XCTAssertEqual(items.map(\.task.id), [task.id])
    XCTAssertTrue(items[0].isDoneToday)
    XCTAssertEqual(items[0].secondsLoggedToday, 1_500)
    XCTAssertEqual(try store.task(id: task.id)?.status, .open, "a daily's task survives the day it is done")
  }

  func testContributionsOnTheSameDayAccumulateIntoOneRow() throws {
    let task = try store.createTask(listId: list.id, title: "Practice")
    let daily = try store.makeDaily(taskId: task.id)
    let noon = Date(timeIntervalSince1970: 1_750_000_000)

    try store.logContribution(dailyId: daily.id, seconds: 600, complete: false, now: noon, calendar: calendar)
    try store.logContribution(dailyId: daily.id, seconds: 900, complete: true, now: noon.addingTimeInterval(3_600), calendar: calendar)

    let items = try store.dailies(on: noon, calendar: calendar)
    XCTAssertEqual(items[0].secondsLoggedToday, 1_500)
    XCTAssertTrue(items[0].isDoneToday)
    XCTAssertEqual(try store.contributionHistory(dailyId: daily.id, days: 2, endingOn: noon, calendar: calendar).count, 1)
  }

  func testClearingAContributionKeepsTheTimeAlreadyLogged() throws {
    let task = try store.createTask(listId: list.id, title: "Practice")
    let daily = try store.makeDaily(taskId: task.id)
    try store.logContribution(dailyId: daily.id, seconds: 900)

    try store.clearContribution(dailyId: daily.id)

    let items = try store.dailies()
    XCTAssertFalse(items[0].isDoneToday)
    XCTAssertEqual(items[0].secondsLoggedToday, 900)
  }

  func testAWeekdayScheduleOnlyShowsOnItsDays() throws {
    let task = try store.createTask(listId: list.id, title: "Weekday review")
    // 2 = Monday through 6 = Friday in Calendar's numbering.
    _ = try store.makeDaily(taskId: task.id, weekdays: [2, 3, 4, 5, 6])
    let monday = try XCTUnwrap(DateComponents(calendar: calendar, year: 2_025, month: 6, day: 16).date)
    let sunday = try XCTUnwrap(DateComponents(calendar: calendar, year: 2_025, month: 6, day: 15).date)

    XCTAssertEqual(try store.dailies(on: monday, calendar: calendar).count, 1)
    XCTAssertEqual(try store.dailies(on: sunday, calendar: calendar).count, 0)
  }

  func testAnIntervalScheduleRotatesThroughTheWeek() throws {
    let task = try store.createTask(listId: list.id, title: "Every third day")
    let anchor = try XCTUnwrap(DateComponents(calendar: calendar, year: 2_025, month: 6, day: 16).date)
    _ = try store.makeDaily(taskId: task.id, intervalDays: 3, now: anchor)

    func dueCount(dayOffset: Int) throws -> Int {
      let day = try XCTUnwrap(calendar.date(byAdding: .day, value: dayOffset, to: anchor))
      return try store.dailies(on: day, calendar: calendar).count
    }

    XCTAssertEqual(try dueCount(dayOffset: 0), 1)
    XCTAssertEqual(try dueCount(dayOffset: 1), 0)
    XCTAssertEqual(try dueCount(dayOffset: 3), 1)
    XCTAssertEqual(try dueCount(dayOffset: 6), 1)
  }

  func testFinishingAFocusBlockOnADailyCreditsTimeInsteadOfCompletingTheTask() throws {
    let task = try store.createTask(listId: list.id, title: "Write 500 words")
    let daily = try store.makeDaily(taskId: task.id)
    let session = try store.startFocusSession(taskId: task.id, plannedSeconds: 1_500)

    let completion = try store.completeActiveFocusTask(sessionId: session.id, elapsedSeconds: 1_320)

    XCTAssertEqual(completion.outcome, .contributionLogged(seconds: 1_320))
    XCTAssertEqual(try store.task(id: task.id)?.status, .open)
    XCTAssertEqual(try store.dailies()[0].secondsLoggedToday, 1_320)
    XCTAssertEqual(try store.daily(forTaskId: task.id)?.id, daily.id)
  }

  func testAnEstimateGivenOnTheFocusScreenBecomesTheSessionsWorkBlock() throws {
    let task = try store.createTask(listId: list.id, title: "Draft proposal")

    let session = try store.startFocusSession(taskId: task.id, plannedSeconds: 45 * 60)

    XCTAssertEqual(session.workDurationSeconds, 45 * 60)
    XCTAssertEqual(try store.focusQueue(for: session.id).first?.item.plannedSeconds, 45 * 60)
  }

  func testATimerReadsAsOpenEndedAndAPomodoroDoesNot() throws {
    let timed = try store.createTask(listId: list.id, title: "Timed")
    let timer = try store.startFocusTimer(taskId: timed.id)
    XCTAssertTrue(timer.openEnded)
    XCTAssertTrue(try XCTUnwrap(store.activeFocusSession()).openEnded)
    try store.finishFocusSession(id: timer.id)

    let blocked = try store.createTask(listId: list.id, title: "Blocked")
    let pomodoro = try store.startFocusSession(taskId: blocked.id, plannedSeconds: 25 * 60)
    XCTAssertFalse(pomodoro.openEnded)
    XCTAssertEqual(pomodoro.workDurationSeconds, 25 * 60)
    XCTAssertFalse(try XCTUnwrap(store.focusQueue(for: pomodoro.id).first).item.openEnded)
  }

  // MARK: - Next up

  func testNextUpKeepsDeadlinePrecedenceAndDropsContributedDailies() throws {
    let daily = try store.createTask(listId: list.id, title: "Write 500 words")
    let due = try store.createTask(listId: list.id, title: "File return")
    try store.updateTask(id: due.id, title: due.title, notes: "", dueAt: Date(), estimateSeconds: nil)
    let record = try store.makeDaily(taskId: daily.id)

    XCTAssertEqual(NextUpSelector.next(from: try store.nextUpCandidates())?.candidate.id, due.id)

    try store.logContribution(dailyId: record.id)
    XCTAssertFalse(try store.nextUpCandidates().contains { $0.id == daily.id })

    XCTAssertEqual(NextUpSelector.next(from: try store.nextUpCandidates())?.candidate.id, due.id)
  }

  func testNextUpOffersLeavesRatherThanProjectsAndSkipsArchivedLists() throws {
    let project = try store.createTask(listId: list.id, title: "Ship release")
    let step = try store.createTask(listId: list.id, title: "Cut the tag", parentTaskId: project.id)
    let shelved = try store.createList(workspaceId: workspace.id, name: "Someday")
    _ = try store.createTask(listId: shelved.id, title: "Learn the cello")
    try store.setListArchived(true, id: shelved.id)

    let ids = Set(try store.nextUpCandidates().map(\.id))

    XCTAssertEqual(ids, [step.id])
  }

  func testSchedulingATaskForLaterRemovesItFromNextUpUntilThatTime() throws {
    let task = try store.createTask(listId: list.id, title: "Call the bank")
    let later = Date().addingTimeInterval(4 * 3_600)

    try store.scheduleTask(id: task.id, startAt: later)

    // The store still reports the task and its start time; deciding that
    // "later" means "not yet" is the selector's job.
    let stored = try XCTUnwrap(try store.nextUpCandidates().first?.startAt)
    XCTAssertEqual(stored.timeIntervalSince1970, later.timeIntervalSince1970, accuracy: 1)
    XCTAssertNil(NextUpSelector.next(from: try store.nextUpCandidates()))
    XCTAssertEqual(
      NextUpSelector.next(from: try store.nextUpCandidates(), now: later.addingTimeInterval(60))?.candidate.id,
      task.id)

    try store.scheduleTask(id: task.id, startAt: nil)

    XCTAssertEqual(NextUpSelector.next(from: try store.nextUpCandidates())?.candidate.id, task.id)
  }

  // MARK: - Legacy import

  func testLegacyDailiesBecomeHabitTasksAndImportingTwiceChangesNothing() throws {
    let existing = try store.createTask(listId: list.id, title: "Already a workspace task")
    let seeds = [
      LegacyDailySeed(id: "daily-1", title: "Spanish flashcards", activeWeekdays: [2, 3, 4, 5, 6]),
      LegacyDailySeed(id: "daily-2", title: "Stretch", intervalDays: 2),
    ]

    let imported = try store.importLegacyDailies(seeds, progressTaskIDs: [existing.id])

    XCTAssertEqual(imported, 3)
    let habits = try XCTUnwrap(store.lists(in: workspace.id).first { $0.name == "Habits" })
    XCTAssertEqual(try store.tasks(in: habits.id).map(\.title).sorted(), ["Spanish flashcards", "Stretch"])
    XCTAssertEqual(try store.daily(forTaskId: existing.id)?.taskId, existing.id)

    XCTAssertEqual(try store.importLegacyDailies(seeds, progressTaskIDs: [existing.id]), 0)
    XCTAssertEqual(try store.allDailies().count, 3)
    XCTAssertEqual(try store.lists(in: workspace.id).filter { $0.name == "Habits" }.count, 1)
  }

  // MARK: - Completion context

  func testTheFirstCompletionOfTheDayIsOrdinalOne() throws {
    _ = try store.createTask(listId: list.id, title: "Anything")

    XCTAssertEqual(try store.completionContext().ordinalToday, 1)
  }

  func testOrdinalCountsBothFinishedTasksAndTickedDailies() throws {
    let done = try store.createTask(listId: list.id, title: "Done")
    try store.setStatus(.completed, for: done.id)
    let habit = try store.createTask(listId: list.id, title: "Habit")
    let daily = try store.makeDaily(taskId: habit.id)
    try store.logContribution(dailyId: daily.id)

    XCTAssertEqual(try store.completionContext().ordinalToday, 3, "two already done, so the next is the third")
  }

  func testAStreakCountsBackThroughConsecutiveDaysAndStopsAtAGap() throws {
    let habit = try store.createTask(listId: list.id, title: "Habit")
    let daily = try store.makeDaily(taskId: habit.id)
    let today = calendar.startOfDay(for: Date())

    for offset in [1, 2, 4] {
      let day = try XCTUnwrap(calendar.date(byAdding: .day, value: -offset, to: today))
      try store.logContribution(dailyId: daily.id, now: day, calendar: calendar)
    }

    // Today (about to be extended) plus the run at -1 and -2; the gap at -3 ends it.
    XCTAssertEqual(try store.completionContext().streakDays, 3)
  }

  func testAnEmptyTodayStillCountsBecauseTheCompletionIsAboutToLandOnIt() throws {
    XCTAssertEqual(try store.completionContext().streakDays, 1)
  }
}
