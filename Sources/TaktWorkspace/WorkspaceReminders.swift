import Foundation
import TaktRustCore
import UserNotifications
import os

/// What this device reminds about, and when: the Rust core's
/// `ReminderSettings`, kept in `UserDefaults` because it is about which
/// device buzzes and does not sync. The Mac and the iPhone share it.
public struct ReminderPreferences: Equatable, Sendable {
  public var dueTime: Bool
  public var dueToday: Bool
  /// Minutes after local midnight.
  public var dueTodayMinute: Int
  public var dailiesLeft: Bool
  public var dailiesLeftMinute: Int

  public init(
    dueTime: Bool = false, dueToday: Bool = false, dueTodayMinute: Int = 9 * 60,
    dailiesLeft: Bool = false, dailiesLeftMinute: Int = 20 * 60
  ) {
    self.dueTime = dueTime
    self.dueToday = dueToday
    self.dueTodayMinute = dueTodayMinute
    self.dailiesLeft = dailiesLeft
    self.dailiesLeftMinute = dailiesLeftMinute
  }

  public var anyEnabled: Bool { dueTime || dueToday || dailiesLeft }

  /// Posted on the default centre whenever `save` changes something, so
  /// whoever schedules can set the reminders again.
  public static let didChange = Notification.Name("TaktReminderPreferencesDidChange")

  private enum Key {
    static let dueTime = "reminders.dueTime.enabled"
    static let dueToday = "reminders.dueToday.enabled"
    static let dueTodayMinute = "reminders.dueToday.minute"
    static let dailiesLeft = "reminders.dailiesLeft.enabled"
    static let dailiesLeftMinute = "reminders.dailiesLeft.minute"
  }

  public static func load(from defaults: UserDefaults = .standard) -> ReminderPreferences {
    let fallback = ReminderPreferences()
    func minute(_ key: String, _ value: Int) -> Int {
      defaults.object(forKey: key) == nil ? value : defaults.integer(forKey: key)
    }
    return ReminderPreferences(
      dueTime: defaults.bool(forKey: Key.dueTime),
      dueToday: defaults.bool(forKey: Key.dueToday),
      dueTodayMinute: minute(Key.dueTodayMinute, fallback.dueTodayMinute),
      dailiesLeft: defaults.bool(forKey: Key.dailiesLeft),
      dailiesLeftMinute: minute(Key.dailiesLeftMinute, fallback.dailiesLeftMinute)
    )
  }

  public func save(to defaults: UserDefaults = .standard) {
    guard self != Self.load(from: defaults) else { return }
    defaults.set(dueTime, forKey: Key.dueTime)
    defaults.set(dueToday, forKey: Key.dueToday)
    defaults.set(dueTodayMinute, forKey: Key.dueTodayMinute)
    defaults.set(dailiesLeft, forKey: Key.dailiesLeft)
    defaults.set(dailiesLeftMinute, forKey: Key.dailiesLeftMinute)
    NotificationCenter.default.post(name: Self.didChange, object: nil)
  }

  var core: ReminderSettings {
    ReminderSettings(
      dueTime: dueTime, dueToday: dueToday, dueTodayMinute: Int32(dueTodayMinute),
      dailiesLeft: dailiesLeft, dailiesLeftMinute: Int32(dailiesLeftMinute))
  }
}

/// One notification to set: the core's `Reminder`.
public struct WorkspaceReminder: Equatable, Sendable {
  public let id: String
  public let fireAt: Date
  public let title: String
  public let body: String
  public let taskID: String?

  public init(id: String, fireAt: Date, title: String, body: String, taskID: String?) {
    self.id = id
    self.fireAt = fireAt
    self.title = title
    self.body = body
    self.taskID = taskID
  }
}

extension WorkspaceStore {
  /// What to set now: the next `days` days, soonest first, at most `limit`.
  /// iOS keeps 64 pending notifications per app, and the focus timer's need
  /// room too, so the default stays well under.
  public func reminders(
    _ preferences: ReminderPreferences, now: Date = .now, timeZone: TimeZone = .current,
    days: UInt32 = 7, limit: UInt32 = 48
  ) throws -> [WorkspaceReminder] {
    guard preferences.anyEnabled else { return [] }
    return try Self.mappingCoreErrors {
      try core.reminders(
        settings: preferences.core, nowMs: now.coreMilliseconds, zone: timeZone.identifier,
        days: days, limit: limit)
    }.map {
      WorkspaceReminder(
        id: $0.id, fireAt: Date(timeIntervalSince1970: TimeInterval($0.fireAtMs) / 1000),
        title: $0.title, body: $0.body, taskID: $0.taskId)
    }
  }
}

/// Hands the core's reminders to `UNUserNotificationCenter`, replacing every
/// one set before, and leaves every other notification (the focus timer's)
/// alone. Text is fixed when set, so callers set them again after each write
/// and each pull.
public enum ReminderScheduler {
  /// Prefixes every request this sets, so a replace touches only its own.
  public static let identifierPrefix = "takt.reminder."
  /// The `userInfo` key holding the task a due-time reminder is for.
  public static let taskIDKey = "taskID"

  private static let logger = Logger(
    subsystem: "uk.co.maybeitssoftware.takt", category: "ReminderScheduler")

  /// Asks for permission if it has not been asked; whether it is granted.
  /// Called when the user switches a reminder on, not at launch.
  @discardableResult
  public static func requestAuthorization(
    center: UNUserNotificationCenter = .current()
  ) async -> Bool {
    let settings = await center.notificationSettings()
    if allows(settings.authorizationStatus) { return true }
    guard settings.authorizationStatus == .notDetermined else { return false }
    return (try? await center.requestAuthorization(options: [.alert, .sound])) ?? false
  }

  private static func allows(_ status: UNAuthorizationStatus) -> Bool {
    #if os(iOS)
      if status == .ephemeral { return true }
    #endif
    return status == .authorized || status == .provisional
  }

  /// Whether the system has turned this app's notifications off, to say so
  /// beside the switches.
  public static func isDenied(center: UNUserNotificationCenter = .current()) async -> Bool {
    await center.notificationSettings().authorizationStatus == .denied
  }

  /// Replaces what is pending with `reminders`. Sets nothing without
  /// permission, and never asks for it.
  public static func apply(
    _ reminders: [WorkspaceReminder], center: UNUserNotificationCenter = .current()
  ) async {
    let pending = await center.pendingNotificationRequests()
      .map(\.identifier)
      .filter { $0.hasPrefix(identifierPrefix) }
    let wanted = Set(reminders.map { identifierPrefix + $0.id })
    let stale = pending.filter { !wanted.contains($0) }
    if !stale.isEmpty { center.removePendingNotificationRequests(withIdentifiers: stale) }
    guard !reminders.isEmpty else { return }

    guard allows(await center.notificationSettings().authorizationStatus) else {
      logger.debug("Reminders not set: notifications are not allowed.")
      return
    }
    let calendar = Calendar.current
    for reminder in reminders {
      let content = UNMutableNotificationContent()
      content.title = reminder.title
      content.body = reminder.body
      content.sound = .default
      content.threadIdentifier = "takt.reminders"
      if let taskID = reminder.taskID { content.userInfo = [taskIDKey: taskID] }
      let components = calendar.dateComponents(
        [.year, .month, .day, .hour, .minute, .second], from: reminder.fireAt)
      let request = UNNotificationRequest(
        identifier: identifierPrefix + reminder.id, content: content,
        trigger: UNCalendarNotificationTrigger(dateMatching: components, repeats: false))
      do {
        // Adding with an identifier already pending replaces it, text and all.
        try await center.add(request)
      } catch {
        logger.warning("Reminder \(reminder.id, privacy: .public) not set: \(error.localizedDescription)")
      }
    }
  }
}
