import SwiftUI
import TaktCore
import TaktWorkspace

/// A waiting task's chips, after its other badges: who it waits on as a tag,
/// and `↻ Tue 14:00 · #2 · every 3d` in the numeral face.
struct WaitingBadgeView: View {
  @Environment(\.theme) private var theme
  let badge: WaitingBadge

  var body: some View {
    if let waitingOn = badge.waitingOn {
      Tag(text: waitingOn)
        .accessibilityLabel("Waiting on \(waitingOn)")
    }
    if let followUp = badge.followUp {
      Text(followUp)
        .font(theme.type.numeral)
        .foregroundStyle(theme.muted)
        .lineLimit(1)
        .accessibilityLabel("Follow up \(followUp.dropFirst(2))")
    }
  }
}

/// Waiting on: who or what the task waits on, when to chase it and how
/// often to again. Saved by its own button, which also files the task in
/// Waiting on; at the follow-up time a follow-up task lands in Today.
/// Android's `WaitingSection`. Waiting is task metadata rather than part of
/// the editor draft, so it is written straight away as its own undo step,
/// like the placement above it.
struct WaitingSection: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  let task: WorkspaceTask
  /// The task's board column, as the inspector last read it.
  let column: String?
  @State private var saved: TaskWaitingDetails?
  @State private var form = WaitingForm()
  @State private var hasLoaded = false
  @FocusState private var editsTag: Bool

  private struct ReloadKey: Hashable {
    let taskID: String
    let revision: Int
  }

  private var isWaiting: Bool { column == WaitingFollowUp.waitingColumnID }

  var body: some View {
    InspectorSection("Waiting on") {
      InspectorRow("Who") {
        TextField("Sam, Legal, the invoice", text: Binding(
          get: { form.tag },
          set: { form.tag = String($0.prefix(WaitingFollowUp.maximumTagLength)) }))
          .textInputAutocapitalization(.words)
          .focused($editsTag)
          .submitLabel(.done)
          .controlFrame()
          .accessibilityIdentifier("inspector.waitingOn")
      }
      Toggle("Follow up", isOn: Binding(
        get: { form.followUpAt != nil },
        set: { on in form.followUpAt = on ? (form.followUpAt ?? WaitingForm.defaultFollowUp()) : nil }))
        .toggleStyle(ThemedToggleStyle())
        .accessibilityIdentifier("inspector.followUp")
      if form.followUpAt != nil {
        followUpTime
        repeatControl
      }
      HStack(spacing: theme.space.sm) {
        if WaitingForm.hasWaiting(saved) {
          Button("Clear") { write(WaitingForm()) }
            .buttonStyle(ThemedButtonStyle(kind: .quiet))
            .accessibilityIdentifier("inspector.waitingClear")
        }
        Spacer(minLength: 0)
        Button(isWaiting ? "Save" : "Move to Waiting on") { write(form) }
          .buttonStyle(ThemedButtonStyle(kind: .primary))
          .disabled(isWaiting && !form.differs(from: saved))
          .accessibilityIdentifier("inspector.waitingSave")
      }
      .frame(minHeight: theme.touchTarget)
      Text(form.hint(title: task.title))
        .font(theme.type.footnote)
        .foregroundStyle(theme.dim)
    }
    .task(id: ReloadKey(taskID: task.id, revision: model.revision)) { reload() }
  }

  private var followUpTime: some View {
    VStack(alignment: .leading, spacing: theme.space.sm) {
      HStack(spacing: theme.space.xs) {
        ForEach(WaitingForm.quickChoices(), id: \.title) { choice in
          Button(choice.title) { form.followUpAt = choice.date }
            .buttonStyle(ThemedButtonStyle(kind: .quiet, compact: true))
        }
      }
      DatePicker(
        "Follow up at",
        selection: Binding(
          get: { form.followUpAt ?? .now },
          set: { form.followUpAt = WaitingForm.minute($0) }),
        displayedComponents: [.date, .hourAndMinute]
      )
      .font(theme.type.body)
      .foregroundStyle(theme.muted)
      .accessibilityIdentifier("inspector.followUpAt")
    }
  }

  /// Once, or every so many days: a day at a time up to a week, then a week
  /// at a time.
  @ViewBuilder
  private var repeatControl: some View {
    InspectorRow("Repeat") {
      ChoiceChips(
        options: [("Once", false), ("Every few days", true)],
        selection: Binding(
          get: { form.everyDays != nil },
          set: { repeats in form.everyDays = repeats ? (form.everyDays ?? WaitingForm.defaultEveryDays) : nil }),
        identifier: "inspector.followUpRepeat")
    }
    if let every = form.everyDays {
      HStack(spacing: theme.space.sm) {
        stepButton("minus", label: "Fewer days between follow-ups", enabled: every > WaitingFollowUp.everyDaysRange.lowerBound) {
          form.everyDays = WaitingForm.step(every, by: -1)
        }
        Text(WaitingFollowUp.repeatText(every))
          .font(theme.type.numeralBody)
          .foregroundStyle(theme.ink)
          .frame(maxWidth: .infinity)
          .accessibilityIdentifier("inspector.followUpEvery")
        stepButton("plus", label: "More days between follow-ups", enabled: every < WaitingFollowUp.everyDaysRange.upperBound) {
          form.everyDays = WaitingForm.step(every, by: 1)
        }
      }
    }
  }

  private func stepButton(_ symbol: String, label: String, enabled: Bool, action: @escaping () -> Void) -> some View {
    Button(action: action) {
      Image(systemName: symbol)
        .font(theme.type.glyph(11.5, .semibold))
        .frame(width: 24, height: 20)
    }
    .buttonStyle(ThemedButtonStyle(kind: .quiet, compact: true))
    .disabled(!enabled)
    .accessibilityLabel(label)
  }

  /// Reads what is saved; the form follows it when it changed, from here or
  /// elsewhere, and keeps what is being typed otherwise.
  private func reload() {
    let details = (try? model.store.waitingDetails())?[task.id]
    guard !hasLoaded || details != saved else { return }
    hasLoaded = true
    saved = details
    form = WaitingForm(saved: details)
  }

  private func write(_ form: WaitingForm) {
    editsTag = false
    let taskID = task.id
    model.perform { store in
      try store.setWaiting(
        taskId: taskID, waitingOn: form.savedTag, followUpAt: form.followUpAt.map(WaitingForm.minute),
        everyDays: form.savedEveryDays)
    }
  }
}
