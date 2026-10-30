import SwiftUI
import TaktCore
import TaktWorkspace

/// An in-row rename of a repeating task, held until the person says whether
/// the new name is this turn's alone or the series'. Every turn is a task of
/// its own, and the next one is written from the one closed, so "just today"
/// keeps the old name for the next turn (`WorkspaceStore.keepSeriesTitle`).
struct WorkspaceSeriesRenameRequest: Identifiable {
  let id = UUID()
  let task: WorkspaceTask
  /// The edit as typed, trailing tokens and all.
  let capture: TaskCapture
  let schedule: PeriodicSchedule
}

/// The ways out of a series rename: this turn's alone, or every turn's.
enum SeriesRenameChoice: CaseIterable {
  case thisTurn
  case everyTurn
  case cancel

  /// "Just today" / "Every day" for a daily task, and the words that fit
  /// the rest: "Just this Monday" / "Every Monday", "Just this time".
  func title(for schedule: PeriodicSchedule) -> String {
    switch (self, schedule.cadence) {
    case (.cancel, _): "Cancel"
    case (.thisTurn, .days(1)), (.thisTurn, .weekdays): "Just today"
    case (.thisTurn, .weeks(1)): "Just this week"
    case (.thisTurn, .weekday(let day)): "Just this \(PeriodicSchedule.weekdayName(day))"
    case (.thisTurn, _): "Just this time"
    case (.everyTurn, .days(1)): "Every day"
    case (.everyTurn, .weekdays): "Every weekday"
    case (.everyTurn, .weeks(1)): "Every week"
    case (.everyTurn, .weekday(let day)): "Every \(PeriodicSchedule.weekdayName(day))"
    case (.everyTurn, _): "Every time"
    }
  }

  var detail: String? {
    switch self {
    case .thisTurn: "The next one keeps the old name"
    case .everyTurn: "The next one takes the new name too"
    case .cancel: nil
    }
  }
}

@MainActor
extension WorkspaceViewModel {
  func resolveSeriesRename(_ request: WorkspaceSeriesRenameRequest, choice: SeriesRenameChoice) {
    dismissOverlay()
    let previousTitle = request.task.title
    let taskID = request.task.id
    switch choice {
    case .cancel:
      return
    case .thisTurn:
      saveTitleEdit(request.capture, of: request.task) {
        try $0.keepSeriesTitle(for: taskID, previousTitle: previousTitle)
      }
    case .everyTurn:
      // Lets go of a name an earlier "just today" kept.
      saveTitleEdit(request.capture, of: request.task) {
        try $0.keepSeriesTitle(for: taskID, previousTitle: nil)
      }
    }
  }
}

/// The choice as rows: ↩ or 1 for this turn, 2 for every turn, and escape —
/// the host's — cancels.
struct WorkspaceSeriesRenameOverlay: View {
  @Environment(WorkspaceViewModel.self) private var model
  @Environment(\.theme) private var theme
  let overlayID: String
  let request: WorkspaceSeriesRenameRequest
  @State private var selection = 0

  private let choices = SeriesRenameChoice.allCases

  var body: some View {
    VStack(alignment: .leading, spacing: 0) {
      VStack(alignment: .leading, spacing: theme.space.xxs) {
        Text("Rename “\(request.task.title)”")
          .font(theme.bodyFont().weight(.semibold))
          .foregroundStyle(theme.ink)
        Text("It repeats \(request.schedule.displayLabel.lowercased()). To “\(request.capture.title)” for:")
          .font(theme.captionFont)
          .foregroundStyle(theme.muted)
      }
      .fixedSize(horizontal: false, vertical: true)
      .padding(.horizontal, theme.listGutter)
      .padding(.vertical, theme.space.sm)
      FocusRule()
      ForEach(Array(choices.enumerated()), id: \.offset) { index, choice in
        Button {
          model.resolveSeriesRename(request, choice: choice)
        } label: {
          HStack(spacing: theme.space.sm) {
            Text(choice.title(for: request.schedule)).foregroundStyle(choice == .cancel ? theme.muted : theme.ink)
            if let detail = choice.detail {
              Text(detail).font(theme.captionFont).foregroundStyle(theme.dim)
            }
            Spacer()
            Text(Self.key(for: choice)).font(theme.monoCaptionFont).foregroundStyle(theme.dim)
          }
          .overlayRow(isSelected: index == selection)
        }
        .buttonStyle(.plain)
      }
      WorkspaceOverlayFooter(hints: "↑↓ choose · ↩ or 1 just this one · 2 every one · esc cancel")
    }
    .overlayKeys(model, id: overlayID) { key in
      if let offset = WorkspaceOverlayStep.offset(for: key) {
        selection = WorkspaceOverlayStep.index(from: selection, by: offset, count: choices.count) ?? 0
        return true
      }
      switch key {
      case "enter": model.resolveSeriesRename(request, choice: choices[selection])
      case "1": model.resolveSeriesRename(request, choice: .thisTurn)
      case "2": model.resolveSeriesRename(request, choice: .everyTurn)
      default: break
      }
      // Nothing behind it takes a key while it is up.
      return true
    }
  }

  private static func key(for choice: SeriesRenameChoice) -> String {
    switch choice {
    case .thisTurn: "1 ↩"
    case .everyTurn: "2"
    case .cancel: "esc"
    }
  }
}
