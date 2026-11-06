import AppKit
import Foundation
import UserNotifications
import os
import TaktCore
import TaktWorkspace

/// Dispatches focus session local notifications via `UNUserNotificationCenter`.
/// Safe degradation: checks and requests authorization, silently degrading when
/// permission is denied or unavailable.
@MainActor
final class FocusNotificationManager: NSObject, UNUserNotificationCenterDelegate {
  static let shared = FocusNotificationManager()

  private let center: UNUserNotificationCenter
  private let logger = Logger(subsystem: "uk.co.maybeitssoftware.takt", category: "notifications")

  init(center: UNUserNotificationCenter = .current()) {
    self.center = center
    super.init()
    center.delegate = self
  }

  /// Dispatches a focus notification asynchronously.
  func dispatch(_ notification: FocusNotification) {
    Task {
      await post(notification)
    }
  }

  /// Posts a notification to `UNUserNotificationCenter`, requesting permission if needed.
  func post(_ notification: FocusNotification) async {
    do {
      let settings = await center.notificationSettings()
      let authorized: Bool
      switch settings.authorizationStatus {
      case .notDetermined:
        authorized = (try? await center.requestAuthorization(options: [.alert, .sound])) ?? false
      case .authorized, .provisional:
        authorized = true
      case .denied:
        authorized = false
      @unknown default:
        authorized = false
      }

      guard authorized else {
        logger.debug("Focus notification skipped: permission not granted.")
        return
      }

      let content = UNMutableNotificationContent()
      content.title = notification.title
      if let subtitle = notification.subtitle, !subtitle.isEmpty {
        content.subtitle = subtitle
      }
      content.body = notification.body
      if notification.playSound {
        content.sound = .default
      }

      let request = UNNotificationRequest(
        identifier: "\(notification.id)-\(UUID().uuidString)",
        content: content,
        trigger: nil
      )
      try await center.add(request)
      logger.debug("Dispatched focus notification: \(notification.title)")
    } catch {
      logger.warning("Focus notification dispatch degraded safely: \(error.localizedDescription)")
    }
  }

  // MARK: - UNUserNotificationCenterDelegate

  nonisolated func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    willPresent notification: UNNotification,
    withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
  ) {
    completionHandler([.banner, .sound])
  }

  nonisolated func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    didReceive response: UNNotificationResponse,
    withCompletionHandler completionHandler: @escaping () -> Void
  ) {
    // A due-time reminder carries its task (`ReminderScheduler.taskIDKey`).
    let taskID = response.notification.request.content.userInfo[ReminderScheduler.taskIDKey] as? String
    Task { @MainActor in
      NSApp.activate(ignoringOtherApps: true)
      if let taskID { ReminderCoordinator.shared.openTask(taskID) }
    }
    completionHandler()
  }
}
