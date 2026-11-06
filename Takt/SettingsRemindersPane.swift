import SwiftUI
import TaktWorkspace

/// Which reminders this Mac sets, and when (`docs/reminders.md`).
///
/// Kept on this Mac rather than synced: they are about which device buzzes,
/// so a phone and a Mac can remind at different times or not at all.
/// Switching one on asks for notification permission the first time.
struct SettingsRemindersPane: View {
  @Environment(\.theme) private var theme
  @State private var preferences = ReminderPreferences.load()
  @State private var isDenied = false

  var body: some View {
    Section {
      SettingsToggleRow(
        "At a task's due time",
        detail: "A task due at a time goes off then. A task due on a day with no time is left to the summary below.",
        isOn: binding(\.dueTime))
      SettingsToggleRow(
        "Due today",
        detail: "One notification each morning naming what is due that day. None on a day with nothing due.",
        isOn: binding(\.dueToday))
      if preferences.dueToday {
        SettingsRow("Due today at") { timePicker(\.dueTodayMinute) }
      }
      SettingsToggleRow(
        "Dailies left",
        detail: "One notification each evening naming the dailies not yet done. None once they all are.",
        isOn: binding(\.dailiesLeft))
      if preferences.dailiesLeft {
        SettingsRow("Dailies left at") { timePicker(\.dailiesLeftMinute) }
      }
    } header: {
      SettingsHeader("Reminders")
    } footer: {
      if isDenied {
        Text("Notifications for Takt are off in System Settings → Notifications, so none of these will show.")
          .foregroundStyle(theme.danger)
      }
    }
    .task { isDenied = await ReminderScheduler.isDenied() }
  }

  private func binding(_ key: WritableKeyPath<ReminderPreferences, Bool>) -> Binding<Bool> {
    Binding(
      get: { preferences[keyPath: key] },
      set: { isOn in
        preferences[keyPath: key] = isOn
        let chosen = preferences
        Task {
          if isOn { await ReminderScheduler.requestAuthorization() }
          isDenied = await ReminderScheduler.isDenied()
          chosen.save()
        }
      })
  }

  private func timePicker(_ key: WritableKeyPath<ReminderPreferences, Int>) -> some View {
    DatePicker(
      "",
      selection: Binding(
        get: {
          Calendar.current.startOfDay(for: .now).addingTimeInterval(TimeInterval(preferences[keyPath: key] * 60))
        },
        set: { date in
          let parts = Calendar.current.dateComponents([.hour, .minute], from: date)
          preferences[keyPath: key] = (parts.hour ?? 0) * 60 + (parts.minute ?? 0)
          preferences.save()
        }),
      displayedComponents: [.hourAndMinute]
    )
    .datePickerStyle(.field)
    .labelsHidden()
    .fixedSize()
  }
}
