import SwiftUI
import TaktWorkspace
import UserNotifications

/// Keeps the iPhone's pending reminders in step with the workspace
/// (`docs/reminders.md`). The core decides what goes off; this sets it again
/// after every change the model hears about — a write here, a pull, a write
/// from the widget — and when the preferences change. An iPhone app cannot
/// run code when a notification fires, so each one's text is what it was the
/// last time the app was open; seven days are set ahead so a week away still
/// rings.
///
/// It is also the notification centre's delegate: reminders show while the
/// app is open, and a tapped due-time reminder opens its task.
@MainActor
final class ReminderService: NSObject, UNUserNotificationCenterDelegate {
  static let shared = ReminderService()

  private weak var model: WorkspaceModel?
  private var pending: Task<Void, Never>?

  /// Installed from `featureInstallers`.
  func install(on model: WorkspaceModel) {
    self.model = model
    UNUserNotificationCenter.current().delegate = self
    model.changeObservers.append { _ in ReminderService.shared.schedule() }
    NotificationCenter.default.addObserver(
      forName: ReminderPreferences.didChange, object: nil, queue: .main
    ) { _ in
      Task { @MainActor in ReminderService.shared.schedule(after: .zero) }
    }
    schedule()
  }

  /// Reads and sets the reminders `delay` from now, unless asked again first.
  func schedule(after delay: Duration = .seconds(1)) {
    pending?.cancel()
    pending = Task { [weak self] in
      try? await Task.sleep(for: delay)
      guard !Task.isCancelled, let store = self?.model?.store else { return }
      let reminders = (try? store.reminders(ReminderPreferences.load())) ?? []
      await ReminderScheduler.apply(reminders)
    }
  }

  nonisolated func userNotificationCenter(
    _ center: UNUserNotificationCenter, willPresent notification: UNNotification
  ) async -> UNNotificationPresentationOptions {
    [.banner, .sound, .list]
  }

  nonisolated func userNotificationCenter(
    _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse
  ) async {
    guard let taskID = response.notification.request.content.userInfo[ReminderScheduler.taskIDKey] as? String
    else { return }
    await MainActor.run {
      model?.navigation.inspect(taskID, isPad: UIDevice.current.userInterfaceIdiom == .pad)
    }
  }
}

/// Settings' Reminders section: three switches, each with its time.
struct ReminderSettingsSection: View {
  @Environment(\.theme) private var theme
  @Environment(\.openURL) private var openURL
  @State private var preferences = ReminderPreferences.load()
  @State private var isDenied = false

  var body: some View {
    Section {
      toggle("At a task's due time", \.dueTime, id: "settings.reminders.dueTime")
      toggle("Due today", \.dueToday, id: "settings.reminders.dueToday")
      if preferences.dueToday { time("Each morning at", \.dueTodayMinute) }
      toggle("Dailies left", \.dailiesLeft, id: "settings.reminders.dailiesLeft")
      if preferences.dailiesLeft { time("Each evening at", \.dailiesLeftMinute) }
      if isDenied {
        Button {
          if let url = URL(string: UIApplication.openNotificationSettingsURLString) { openURL(url) }
        } label: {
          Label("Notifications are off for Takt. Turn them on in Settings.", systemImage: "bell.slash")
            .font(theme.type.caption)
            .foregroundStyle(theme.danger)
        }
        .accessibilityIdentifier("settings.reminders.denied")
      }
    } header: {
      Text("Reminders").font(theme.type.caption).foregroundStyle(theme.muted).textCase(nil)
    } footer: {
      Text("Set on this phone only. A due day without a time is in the morning summary.")
        .font(theme.type.footnote).foregroundStyle(theme.muted)
    }
    .listRowBackground(theme.raised)
    .task { isDenied = await ReminderScheduler.isDenied() }
  }

  private func toggle(_ title: String, _ key: WritableKeyPath<ReminderPreferences, Bool>, id: String) -> some View {
    Toggle(
      title,
      isOn: Binding(
        get: { preferences[keyPath: key] },
        set: { isOn in
          withAnimation { preferences[keyPath: key] = isOn }
          let chosen = preferences
          Task {
            if isOn { await ReminderScheduler.requestAuthorization() }
            isDenied = await ReminderScheduler.isDenied()
            chosen.save()
          }
        })
    )
    .toggleStyle(ThemedToggleStyle())
    .font(theme.type.body)
    .accessibilityIdentifier(id)
  }

  private func time(_ title: String, _ key: WritableKeyPath<ReminderPreferences, Int>) -> some View {
    DatePicker(
      title,
      selection: Binding(
        get: {
          Calendar.current.startOfDay(for: .now).addingTimeInterval(TimeInterval(preferences[keyPath: key] * 60))
        },
        set: { date in
          let parts = Calendar.current.dateComponents([.hour, .minute], from: date)
          preferences[keyPath: key] = (parts.hour ?? 0) * 60 + (parts.minute ?? 0)
          preferences.save()
        }),
      displayedComponents: .hourAndMinute
    )
    .font(theme.type.body)
  }
}
