import Foundation
import TaktCore
import XCTest

final class TaskConditionsFocusTests: XCTestCase {
  private var calendar: Calendar {
    var result = Calendar(identifier: .gregorian)
    result.timeZone = TimeZone(identifier: "Europe/London")!
    return result
  }
  private let now = Date(timeIntervalSince1970: 1_789_560_000)

  private func evaluate(_ tasks: [NextUpCandidate], ids: Set<String> = [], seconds: Int? = nil,
                        mode: FocusTimeMode = .progress) -> FocusRanking {
    NextUpSelector.evaluate(tasks, now: now, calendar: calendar,
      context: FocusContext(conditionIDs: ids, endsAt: seconds.map { now.addingTimeInterval(Double($0)) }, mode: mode))
  }

  func testEmptyContextAdmitsOnlyUnconditionalWorkAndReturnsMissingRequirements() {
    let tasks = [NextUpCandidate(id: "laptop", title: "Laptop"),
                 NextUpCandidate(id: "campus", title: "Campus", requirementGroups: [["campus"]])]
    let result = evaluate(tasks)
    XCTAssertEqual(result.ranked.map(\.id), ["laptop"])
    XCTAssertEqual(result.blocked.first?.reasons, [.missingConditions([["campus"]])])
    XCTAssertEqual(evaluate(tasks, ids: ["campus"]).ranked.map(\.id), ["campus", "laptop"])
    XCTAssertEqual(evaluate(tasks, ids: ["home"]).ranked.map(\.id), ["laptop"])
  }

  func testAllGroupsAndAnyAlternativesMustMatch() {
    let task = NextUpCandidate(id: "exercise", title: "Exercise", requirementGroups: [["home", "campus"], ["private", "floor"]])
    XCTAssertTrue(evaluate([task], ids: ["home"]).ranked.isEmpty)
    XCTAssertEqual(evaluate([task], ids: ["campus", "floor"]).ranked.map(\.id), ["exercise"])
    XCTAssertEqual(evaluate([task], ids: ["home", "private"]).ranked.map(\.id), ["exercise"])
  }

  func testConditionsAndPinsCannotOvertakeOlderOverdueWork() {
    let tasks = [NextUpCandidate(id: "old", title: "Old", dueAt: now.addingTimeInterval(-90 * 86400)),
                 NextUpCandidate(id: "new", title: "New", dueAt: now.addingTimeInterval(-86400), focusRank: 0),
                 NextUpCandidate(id: "campus", title: "Campus", focusRank: 0, requirementGroups: [["campus"]])]
    XCTAssertEqual(evaluate(tasks, ids: ["campus"]).ranked.map(\.id), ["old", "new", "campus"])
    XCTAssertEqual(evaluate(tasks, ids: ["campus"]).ranked.first?.explanation, "overdue by 90 days")
  }

  func testDueTodayBreaksEqualDeadlineTiesByCreationAge() {
    let day = TaskCalendarDate.string(now, calendar: calendar)
    let tasks = [NextUpCandidate(id: "new", title: "New", createdAt: now, dueDate: day),
                 NextUpCandidate(id: "old", title: "Old", createdAt: now.addingTimeInterval(-90 * 86400), dueDate: day),
                 NextUpCandidate(id: "daily", title: "Daily", isDailyDueToday: true)]
    XCTAssertEqual(evaluate(tasks).ranked.map(\.id), ["old", "new", "daily"])
    XCTAssertEqual(evaluate(tasks).ranked.first?.reason, .dueToday)
  }

  func testStartBoundaryAndReachedStartPromotion() {
    let task = NextUpCandidate(id: "scheduled", title: "Scheduled", startAt: now.addingTimeInterval(1))
    XCTAssertTrue(evaluate([task]).ranked.isEmpty)
    XCTAssertEqual(evaluate([task]).nextEvaluationAt, task.startAt)
    let ready = NextUpCandidate(id: "ready", title: "Ready", startAt: now)
    XCTAssertEqual(evaluate([NextUpCandidate(id: "plain", title: "Plain"), ready]).ranked.first?.id, "ready")
    // A daily due today counts as Today, the column furthest right, and a
    // start reached orders work only within a column.
    XCTAssertEqual(evaluate([NextUpCandidate(id: "daily", title: "Daily", isDailyDueToday: true), ready]).ranked.first?.id, "daily")
  }

  func testProgressAndFinishModesHandleLongUnknownAndOneSittingWork() {
    let tasks = [NextUpCandidate(id: "long", title: "Long", estimateSeconds: 3600),
                 NextUpCandidate(id: "short", title: "Short", estimateSeconds: 720),
                 NextUpCandidate(id: "single", title: "Single", estimateSeconds: 3600, requiresSingleSitting: true),
                 NextUpCandidate(id: "unknown", title: "Unknown"),
                 NextUpCandidate(id: "zero", title: "Zero", estimateSeconds: 0)]
    XCTAssertEqual(Set(evaluate(tasks, seconds: 1200).ranked.map(\.id)), ["long", "short", "unknown", "zero"])
    XCTAssertEqual(evaluate(tasks, seconds: 1200, mode: .finish).ranked.map(\.id), ["short"])
    let context = FocusContext(endsAt: now.addingTimeInterval(1200))
    XCTAssertEqual(TaskAvailabilityPolicy.suggestedSeconds(for: tasks[0], context: context, now: now), 1200)
    XCTAssertTrue(evaluate(tasks, seconds: 59).ranked.isEmpty)
  }

  func testRemainingWorkAndMinimumBlocksDetermineFit() {
    let task = NextUpCandidate(id: "work", title: "Work", estimateSeconds: 3600, loggedSeconds: 900, minimumBlockSeconds: 600)
    XCTAssertEqual(task.remainingSeconds, 2700)
    XCTAssertTrue(evaluate([task], seconds: 599).ranked.isEmpty)
    XCTAssertEqual(evaluate([task], seconds: 2700, mode: .finish).ranked.map(\.id), ["work"])
    let exhausted = NextUpCandidate(id: "exhausted", title: "Exhausted", estimateSeconds: 300, loggedSeconds: 600)
    XCTAssertEqual(exhausted.remainingSeconds, 0)
    XCTAssertEqual(evaluate([exhausted], mode: .finish).blocked.first?.reasons, [.needsEstimate])
  }

  func testFutureDeadlineRiskOutranksOrdinaryContextWork() {
    let tomorrow = now.addingTimeInterval(86400)
    let tasks = [NextUpCandidate(id: "risk", title: "Risk", dueAt: tomorrow, estimateSeconds: 90000),
                 NextUpCandidate(id: "campus", title: "Campus", requirementGroups: [["campus"]])]
    XCTAssertEqual(evaluate(tasks, ids: ["campus"]).ranked.first?.id, "risk")
    XCTAssertEqual(evaluate(tasks, ids: ["campus"]).ranked.first?.reason, .deadlineRisk)
  }

  func testDateOnlyDeadlineUsesCalendarDayAcrossDSTAndRejectsInvalidDates() throws {
    XCTAssertNil(TaskCalendarDate.date("2026-02-30", calendar: calendar))
    let day = try XCTUnwrap(TaskCalendarDate.date("2026-03-29", calendar: calendar))
    let task = NextUpCandidate(id: "day", title: "Day", dueDate: "2026-03-29")
    let boundary = try XCTUnwrap(task.effectiveDeadline(calendar: calendar))
    XCTAssertEqual(boundary.timeIntervalSince(day), 23 * 3600)
    XCTAssertEqual(NextUpSelector.score(task, now: boundary.addingTimeInterval(-1), calendar: calendar).reason, .dueToday)
    XCTAssertEqual(NextUpSelector.score(task, now: boundary, calendar: calendar).reason, .overdue)
    var otherZone = calendar; otherZone.timeZone = TimeZone(secondsFromGMT: 0)!
    XCTAssertNotEqual(task.effectiveDeadline(calendar: otherZone), boundary)
  }

  func testBlockedUrgentTaskIsNotLostAndRankingIsIndependentOfInputOrder() {
    let task = NextUpCandidate(id: "urgent", title: "Urgent", dueAt: now.addingTimeInterval(-86400), requirementGroups: [["home"]])
    let laptop = NextUpCandidate(id: "laptop", title: "Laptop")
    let first = evaluate([task, laptop]), second = evaluate([laptop, task])
    XCTAssertEqual(first.ranked, second.ranked)
    XCTAssertEqual(first.blocked, second.blocked)
    XCTAssertEqual(first.blocked.first?.id, "urgent")
  }
}

final class FocusClockPolicyTests: XCTestCase {
  func testWallClockAdjustmentsDoNotFabricateOrRemoveActiveWork() {
    XCTAssertEqual(FocusClockPolicy.adjustedElapsed(previousElapsed: 600, wallDelta: 7201, uptimeDelta: 1), 601)
    XCTAssertEqual(FocusClockPolicy.adjustedElapsed(previousElapsed: 600, wallDelta: -7199, uptimeDelta: 1), 601)
    XCTAssertNil(FocusClockPolicy.adjustedElapsed(previousElapsed: 600, wallDelta: 1, uptimeDelta: 1))
  }

  func testInvalidClockReadingsDoNotChangeElapsedTime() {
    XCTAssertNil(FocusClockPolicy.adjustedElapsed(previousElapsed: 600, wallDelta: .nan, uptimeDelta: 1))
    XCTAssertNil(FocusClockPolicy.adjustedElapsed(previousElapsed: 600, wallDelta: 3600, uptimeDelta: -1))
    XCTAssertEqual(FocusClockPolicy.adjustedElapsed(previousElapsed: 600, wallDelta: 3600, uptimeDelta: 0), 600)
  }
}
