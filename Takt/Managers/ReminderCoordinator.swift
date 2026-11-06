import AppKit
import Foundation
import TaktWorkspace

/// Keeps this Mac's pending reminders in step with the workspace.
///
/// The core decides what goes off (`reminders.rs`); this asks it again after
/// every write and pull (`WorkspaceViewModel.flushPendingRefresh` calls
/// `schedule`), when the preferences change, and when the app comes forward,
/// so a day that turned over while the Mac slept sets the days after it.
/// Requests coalesce: a burst of writes reads once, a second later.
@MainActor
final class ReminderCoordinator {
  static let shared = ReminderCoordinator()

  /// The store to read; set by the workspace once it has one.
  var store: () -> WorkspaceStore? = { nil }
  /// Opens a task, for a tapped due-time reminder.
  var openTask: (String) -> Void = { _ in }

  private var pending: Task<Void, Never>?
  private var observers: [NSObjectProtocol] = []

  private init() {
    let center = NotificationCenter.default
    observers.append(
      center.addObserver(forName: ReminderPreferences.didChange, object: nil, queue: .main) { _ in
        Task { @MainActor in ReminderCoordinator.shared.schedule(after: .zero) }
      })
    observers.append(
      center.addObserver(forName: NSApplication.didBecomeActiveNotification, object: nil, queue: .main) { _ in
        Task { @MainActor in ReminderCoordinator.shared.schedule() }
      })
  }

  /// Reads the reminders and sets them, `delay` from now unless asked again
  /// first.
  func schedule(after delay: Duration = .seconds(1)) {
    pending?.cancel()
    pending = Task { [weak self] in
      try? await Task.sleep(for: delay)
      guard !Task.isCancelled, let self else { return }
      let preferences = ReminderPreferences.load()
      let reminders: [WorkspaceReminder]
      if let store = self.store() {
        reminders = (try? store.reminders(preferences)) ?? []
      } else {
        reminders = []
      }
      await ReminderScheduler.apply(reminders)
    }
  }
}
