import Foundation
import TaktCore
import TaktWorkspace
import XCTest

final class WorkspaceReminderTests: XCTestCase {
  private var directoryURL: URL!
  private var store: WorkspaceStore!
  private var list: TaskList!
  private let london = TimeZone(identifier: "Europe/London")!
  /// 2026-11-06 08:00 in London.
  private let now = Date(timeIntervalSince1970: 1_793_952_000)

  override func setUpWithError() throws {
    directoryURL = FileManager.default.temporaryDirectory
      .appendingPathComponent("TaktReminderTests-\(UUID().uuidString)", isDirectory: true)
    store = try WorkspaceStore(databaseURL: directoryURL.appendingPathComponent("priority.sqlite"))
    let workspace = try store.bootstrapIfNeeded()
    list = try XCTUnwrap(store.lists(in: workspace.id).first)
  }

  override func tearDownWithError() throws {
    try? FileManager.default.removeItem(at: directoryURL)
  }

  func testNothingIsSetWhileEveryReminderIsOff() throws {
    _ = try store.createTask(listId: list.id, title: "Call", dueAt: now.addingTimeInterval(3_600))
    XCTAssertEqual(try store.reminders(ReminderPreferences(), now: now, timeZone: london), [])
  }

  func testADueTimeComesBackAsAReminderForItsTask() throws {
    let due = now.addingTimeInterval(7 * 3_600)
    let task = try store.createTask(listId: list.id, title: "Call the bank", dueAt: due)

    let got = try store.reminders(ReminderPreferences(dueTime: true), now: now, timeZone: london)

    XCTAssertEqual(
      got, [WorkspaceReminder(id: "due.\(task.id)", fireAt: due, title: "Due now", body: "Call the bank", taskID: task.id)])
  }

  func testDueTodaySummarisesAtItsMinute() throws {
    _ = try store.createTask(listId: list.id, title: "Call", dueAt: now.addingTimeInterval(7 * 3_600))

    let got = try store.reminders(
      ReminderPreferences(dueToday: true, dueTodayMinute: 9 * 60 + 30), now: now, timeZone: london, days: 1)

    XCTAssertEqual(got.map(\.id), ["today.2026-11-06"])
    XCTAssertEqual(got.first?.fireAt, now.addingTimeInterval(90 * 60))
    XCTAssertEqual(got.first?.title, "1 due today")
  }

  func testPreferencesRoundTripThroughDefaults() throws {
    let defaults = try XCTUnwrap(UserDefaults(suiteName: "TaktReminderTests-\(UUID().uuidString)"))
    XCTAssertEqual(ReminderPreferences.load(from: defaults), ReminderPreferences())
    let chosen = ReminderPreferences(dueTime: true, dailiesLeft: true, dailiesLeftMinute: 21 * 60 + 15)
    chosen.save(to: defaults)
    XCTAssertEqual(ReminderPreferences.load(from: defaults), chosen)
  }
}
