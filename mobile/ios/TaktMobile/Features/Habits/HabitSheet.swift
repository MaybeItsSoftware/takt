import TaktCore
import TaktWorkspace
import SwiftUI

/// The habit form, as a sheet: the Mac's `gb` overlay for a phone, and
/// Android's `HabitSheet`. Opened on a task it makes a habit from that task;
/// on a habit it edits it; with no task it makes a standalone habit. A habit
/// lands in a board column each day it is due, disappears at the end of a
/// missed day or stays until done, and ends with its task, on a date, or never.
struct HabitSheet: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.dismiss) private var dismiss
  let request: HabitRequest
  @State private var context: HabitFormContext?

  var body: some View {
    NavigationStack {
      Group {
        if let context {
          HabitFormBody(context: context) { draft in
            if model.saveHabit(draft, habitTaskID: context.habitTaskId) { dismiss() }
          }
        } else {
          Color.clear
        }
      }
      .background(theme.paper)
      .navigationTitle(context?.habitTaskId == nil ? "New habit" : "Edit habit")
      .navigationBarTitleDisplayMode(.inline)
      .toolbar {
        ToolbarItem(placement: .cancellationAction) {
          Button("Cancel") { dismiss() }
            .accessibilityIdentifier("habit.cancel")
        }
      }
    }
    .presentationDetents([.large])
    .task(id: request.id) {
      context = model.habitFormContext(forTaskID: request.taskID)
      if context == nil { dismiss() }
    }
  }
}

private struct HabitFormBody: View {
  @Environment(\.theme) private var theme
  let save: (HabitDraft) -> Void
  let sourceTitle: String?
  @State private var draft: HabitDraft
  @State private var estimateText: String
  @State private var expiryDateText: String
  @State private var error: HabitForm.Invalid?

  init(context: HabitFormContext, save: @escaping (HabitDraft) -> Void) {
    self.save = save
    sourceTitle = context.sourceTitle
    _draft = State(initialValue: context.draft)
    _estimateText = State(initialValue: HabitForm.estimateText(context.draft.estimateSeconds))
    _expiryDateText = State(initialValue: context.draft.expiry.date.map { HabitForm.dayText($0) } ?? "")
  }

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: theme.space.md) {
        row("Habit") {
          TextField("Practise drums", text: titleBinding)
            .textInputAutocapitalization(.sentences)
            .submitLabel(.done)
            .controlFrame()
            .overlay(errorBorder(.title))
            .accessibilityIdentifier("habit.title")
        }

        row("How often") {
          ChoiceChips(
            options: HabitForm.FrequencyKind.allCases.map { ($0.label, $0) },
            selection: frequencyKind, identifier: "habit.frequency")
          frequencyDetail
        }

        row("Appears in") {
          ChoiceChips(
            options: HabitPlacement.allCases.map { ($0.label, $0) }, selection: $draft.placement,
            identifier: "habit.placement")
        }

        row("End of day") {
          ChoiceChips(
            options: [("Disappears", true), ("Stays until done", false)], selection: $draft.dropsAtDayEnd,
            identifier: "habit.dayEnd")
        }

        row("Estimate") {
          TextField("30m", text: $estimateText)
            .keyboardType(.asciiCapable)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .controlFrame()
            .overlay(errorBorder(.estimate))
            .accessibilityIdentifier("habit.estimate")
        }

        row("Ends") {
          ChoiceChips(
            options: HabitForm.expiryKinds(hasSource: draft.sourceTaskId != nil).map { ($0.label, $0) },
            selection: expiryKind, identifier: "habit.ends")
          switch HabitForm.expiryKind(draft.expiry) {
          case .source:
            hint(HabitForm.sourceHint(sourceTitle))
          case .date:
            TextField("2026-12-31", text: $expiryDateText)
              .keyboardType(.asciiCapable)
              .textInputAutocapitalization(.never)
              .autocorrectionDisabled()
              .controlFrame()
              .overlay(errorBorder(.expiryDate))
              .accessibilityIdentifier("habit.endDate")
            hint("A date, 3w, or a weekday.")
          case .never:
            EmptyView()
          }
        }

        if let error {
          Text(error.message)
            .font(theme.type.callout)
            .foregroundStyle(theme.danger)
            .accessibilityIdentifier("habit.error")
        }

        Button(action: submit) {
          Text("Save").frame(maxWidth: .infinity)
        }
        .buttonStyle(ThemedButtonStyle(kind: .primary))
        .accessibilityIdentifier("habit.save")
      }
      .padding(.horizontal, theme.space.lg)
      .padding(.vertical, theme.space.md)
    }
    .scrollDismissesKeyboard(.interactively)
  }

  // MARK: - How often

  @ViewBuilder
  private var frequencyDetail: some View {
    switch draft.frequency {
    case .weekdays(let days):
      HStack(spacing: theme.space.xs) {
        ForEach(HabitForm.weekdayOrder, id: \.self) { weekday in
          weekdayChip(weekday, selected: days.contains(weekday))
        }
      }
    case .everyNDays(let days):
      HStack(spacing: theme.space.sm) {
        stepButton("minus", label: "Fewer days", enabled: days > 2) { draft.frequency = HabitForm.step(draft.frequency, by: -1) }
        Text(draft.frequency.label)
          .font(theme.type.body)
          .foregroundStyle(theme.ink)
          .frame(maxWidth: .infinity)
          .accessibilityIdentifier("habit.interval")
        stepButton("plus", label: "More days", enabled: days < 366) { draft.frequency = HabitForm.step(draft.frequency, by: 1) }
      }
    default:
      EmptyView()
    }
  }

  private func weekdayChip(_ weekday: Int, selected: Bool) -> some View {
    let shape = RoundedRectangle(cornerRadius: theme.radius.control, style: .continuous)
    return Button {
      draft.frequency = HabitForm.toggle(weekday, in: draft.frequency)
    } label: {
      Text(HabitForm.weekdayName(weekday))
        .font(theme.type.callout)
        .foregroundStyle(selected ? theme.primary : theme.muted)
        .frame(maxWidth: .infinity, minHeight: 36)
        .background(shape.fill(selected ? theme.primary.opacity(0.10) : theme.raised))
        .overlay(shape.strokeBorder(selected ? theme.primary.opacity(0.6) : theme.inputBorder, lineWidth: theme.hairline))
        .hitTarget()
    }
    .buttonStyle(.plain)
    .accessibilityLabel(Calendar.current.weekdaySymbols[weekday - 1])
    .accessibilityAddTraits(selected ? .isSelected : [])
    .accessibilityIdentifier("habit.weekday.\(weekday)")
  }

  private func stepButton(_ symbol: String, label: String, enabled: Bool, action: @escaping () -> Void) -> some View {
    Button(action: action) {
      Image(systemName: symbol)
        .font(theme.type.glyph(12.5, .semibold))
        .frame(width: theme.touchTarget, height: theme.touchTarget)
    }
    .buttonStyle(ThemedButtonStyle())
    .disabled(!enabled)
    .accessibilityLabel(label)
  }

  // MARK: - Bindings

  private var titleBinding: Binding<String> {
    Binding(get: { draft.title }, set: { draft.title = $0.replacingOccurrences(of: "\n", with: " ") })
  }

  private var frequencyKind: Binding<HabitForm.FrequencyKind> {
    Binding(
      get: { HabitForm.kind(draft.frequency) },
      set: { draft.frequency = HabitForm.with(draft.frequency, kind: $0) })
  }

  private var expiryKind: Binding<HabitForm.ExpiryKind> {
    Binding(
      get: { HabitForm.expiryKind(draft.expiry) },
      set: { kind in
        switch kind {
        case .source: draft.expiry = .whenSourceCompleted
        case .never: draft.expiry = .never
        case .date:
          let date = HabitForm.defaultEndDate()
          if expiryDateText.isEmpty { expiryDateText = HabitForm.dayText(date) }
          draft.expiry = .on(date)
        }
      })
  }

  // MARK: - Pieces

  private func row(_ label: String, @ViewBuilder content: () -> some View) -> some View {
    VStack(alignment: .leading, spacing: theme.space.xs) {
      SectionLabel(label)
      content()
    }
  }

  private func hint(_ text: String) -> some View {
    Text(text).font(theme.type.footnote).foregroundStyle(theme.muted)
  }

  @ViewBuilder
  private func errorBorder(_ field: HabitForm.Field) -> some View {
    if error?.field == field {
      RoundedRectangle(cornerRadius: theme.radius.control, style: .continuous)
        .strokeBorder(theme.danger, lineWidth: theme.stroke)
    }
  }

  private func submit() {
    switch HabitForm.validate(draft, estimateText: estimateText, expiryDateText: expiryDateText) {
    case .failure(let invalid): error = invalid
    case .success(let valid):
      error = nil
      save(valid)
    }
  }
}
