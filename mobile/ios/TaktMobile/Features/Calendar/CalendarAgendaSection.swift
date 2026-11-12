import SwiftUI
import TaktWorkspace

/// Today's schedule from Google Calendar: the day's events and timed tasks,
/// interleaved by time, all-day first (the core's `google_calendar_agenda`).
/// Nothing at all while Calendar is not connected or the day is empty, so a
/// Today without Calendar looks exactly as it did.
struct CalendarAgendaSection: View {
  @Environment(\.theme) private var theme
  @Environment(\.isPadLayout) private var isPad
  @Environment(WorkspaceModel.self) private var model

  var body: some View {
    if let calendar = GoogleCalendarController.shared, calendar.isConnected, !calendar.agenda.isEmpty {
      Section {
        ForEach(calendar.agenda) { item in
          let hex = calendar.calendars.first { $0.id == item.calendarID }?.colorHex
          CalendarAgendaRow(item: item, colorHex: hex) {
            if item.kind == .task { model.navigation.inspect(item.id, isPad: isPad) }
          }
          .listRowBackground(theme.paper)
          .listRowSeparatorTint(theme.borderMuted)
          .moveDisabled(true)
          .selectionDisabled()
        }
      } header: {
        Text("Schedule").font(theme.type.caption).foregroundStyle(theme.muted).textCase(nil)
      }
    }
  }
}

private struct CalendarAgendaRow: View {
  @Environment(\.theme) private var theme
  let item: GoogleCalendarAgendaItem
  let colorHex: String?
  let open: () -> Void

  var body: some View {
    Button(action: open) {
      HStack(alignment: .firstTextBaseline, spacing: theme.space.md) {
        Text(timeText)
          .font(theme.type.numeral)
          .foregroundStyle(theme.muted)
          .frame(minWidth: 88, alignment: .leading)
        if item.kind == .event {
          CalendarSwatch(hex: colorHex).alignmentGuide(.firstTextBaseline) { $0[.bottom] - 4 }
        } else {
          Image(systemName: item.isDone ? "checkmark.square" : "square")
            .font(theme.type.glyph(11.5))
            .foregroundStyle(theme.muted)
        }
        Text(item.title)
          .font(theme.type.body)
          .foregroundStyle(item.isDone ? theme.muted : theme.ink)
          .strikethrough(item.isDone)
          .lineLimit(2)
        Spacer(minLength: 0)
      }
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .disabled(item.kind == .event)
    .accessibilityElement(children: .combine)
  }

  private var timeText: String {
    if item.isAllDay { return "All day" }
    guard let start = item.start else { return "" }
    let format = Date.FormatStyle(date: .omitted, time: .shortened)
    guard let end = item.end else { return start.formatted(format) }
    return "\(start.formatted(format))–\(end.formatted(format))"
  }
}
