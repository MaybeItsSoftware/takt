import SwiftUI
import TaktWorkspace

/// Settings' row for Google Calendar: a link to its page, with its state.
struct CalendarSettingsRow: View {
  @Environment(\.theme) private var theme

  var body: some View {
    if let calendar = GoogleCalendarController.shared, calendar.isConfigured {
      Section {
        NavigationLink {
          CalendarSettingsView(calendar: calendar)
        } label: {
          HStack {
            Label("Google Calendar", systemImage: "calendar").font(theme.type.body)
            Spacer()
            Text(calendar.isConnected ? "Connected" : "Off")
              .font(theme.type.caption)
              .foregroundStyle(theme.muted)
          }
        }
        .accessibilityIdentifier("settings.calendar")
      } header: {
        // The integrations under one heading, as the Mac's Settings →
        // Plugins; Google Calendar is the one this phone has.
        Text("Plugins").font(theme.type.caption).foregroundStyle(theme.muted).textCase(nil)
      }
      .listRowBackground(theme.raised)
    }
  }
}

/// Connecting Google Calendar and choosing what it does
/// (docs/google-calendar.md). The choices are the workspace's synced
/// preferences, so they follow the account to the Mac and Android.
struct CalendarSettingsView: View {
  @Environment(\.theme) private var theme
  var calendar: GoogleCalendarController

  private var writable: [GoogleCalendarChoice] { calendar.calendars.filter(\.canWrite) }

  var body: some View {
    Form {
      Section {
        if calendar.isConnected {
          Button("Disconnect", role: .destructive) { calendar.disconnect() }
            .font(theme.type.body)
        } else {
          Button {
            Task { await calendar.connect() }
          } label: {
            HStack {
              Text("Connect Google Calendar").font(theme.type.body)
              if calendar.isBusy { Spacer(); ProgressView() }
            }
          }
          .accessibilityIdentifier("calendar.connect")
        }
        if let error = calendar.errorMessage {
          Label(error, systemImage: "exclamationmark.triangle")
            .font(theme.type.caption)
            .foregroundStyle(theme.danger)
        }
      } footer: {
        Text("Takt asks Google to see your calendars and edit their events.")
          .font(theme.type.caption).foregroundStyle(theme.muted)
      }
      .listRowBackground(theme.raised)

      if calendar.isConnected {
        Section {
          Picker("Calendar", selection: twoWayBinding) {
            Text("Off").tag("")
            ForEach(writable) { Text($0.summary).tag($0.id) }
          }
          .font(theme.type.body)
          if calendar.preferences.twoWayCalendarID != nil {
            Toggle("Turn new events into tasks", isOn: importBinding)
              .toggleStyle(ThemedToggleStyle())
              .font(theme.type.body)
          }
        } header: {
          header("Two-way")
        } footer: {
          Text("Timed tasks become events in this calendar. Moving or renaming one there moves or renames the task.")
            .font(theme.type.caption).foregroundStyle(theme.muted)
        }
        .listRowBackground(theme.raised)

        Section {
          // An account can hold dozens of calendars; folded, the row counts
          // the ones Today shows.
          SettingsDisclosure("Calendars", value: shownSummary, key: "calendar.agenda") {
            ForEach(calendar.calendars) { choice in
              Toggle(isOn: agendaBinding(choice.id)) {
                HStack(spacing: theme.space.sm) {
                  CalendarSwatch(hex: choice.colorHex)
                  Text(choice.summary).font(theme.type.body)
                }
              }
              .toggleStyle(ThemedToggleStyle())
            }
          }
        } header: {
          header("Shown in Today")
        }
        .listRowBackground(theme.raised)
      }
    }
    .scrollContentBackground(.hidden)
    .background(theme.paper)
    .navigationTitle("Google Calendar")
    .navigationBarTitleDisplayMode(.inline)
    .task { await calendar.loadCalendars() }
  }

  private var twoWayBinding: Binding<String> {
    Binding(
      get: { calendar.preferences.twoWayCalendarID ?? "" },
      set: { id in calendar.update { $0.twoWayCalendarID = id.isEmpty ? nil : id } })
  }

  private var importBinding: Binding<Bool> {
    Binding(
      get: { calendar.preferences.importEvents },
      set: { value in calendar.update { $0.importEvents = value } })
  }

  private var shownSummary: String {
    let shown = calendar.calendars.filter { calendar.preferences.agendaCalendarIDs.contains($0.id) }.count
    return shown == 0 ? "None" : "\(shown) of \(calendar.calendars.count)"
  }

  private func agendaBinding(_ id: String) -> Binding<Bool> {
    Binding(
      get: { calendar.preferences.agendaCalendarIDs.contains(id) },
      set: { on in
        calendar.update { preferences in
          preferences.agendaCalendarIDs.removeAll { $0 == id }
          if on { preferences.agendaCalendarIDs.append(id) }
        }
      })
  }

  private func header(_ text: String) -> some View {
    Text(text).font(theme.type.caption).foregroundStyle(theme.muted).textCase(nil)
  }
}

/// A calendar's own colour, as Google gives it: identity, not chrome.
struct CalendarSwatch: View {
  @Environment(\.theme) private var theme
  let hex: String?

  var body: some View {
    RoundedRectangle(cornerRadius: 2)
      .fill(color)
      .frame(width: 4, height: 18)
  }

  private var color: Color {
    guard let hex, hex.hasPrefix("#"), hex.count == 7, let value = UInt32(hex.dropFirst(), radix: 16) else {
      return theme.primary
    }
    return Color(
      red: Double((value >> 16) & 0xFF) / 255, green: Double((value >> 8) & 0xFF) / 255,
      blue: Double(value & 0xFF) / 255)
  }
}
