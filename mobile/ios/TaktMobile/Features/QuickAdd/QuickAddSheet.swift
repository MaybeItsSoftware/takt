import TaktCore
import TaktWorkspace
import SwiftUI

/// Quick add: one field that reads `45m #work @fri !1` off the end of what
/// you type and shows it as chips before you add, a destination, and "plan
/// for today". Stays open after Add so a run of tasks can go in one sitting.
struct QuickAddSheet: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.dismiss) private var dismiss
  @State private var quickAdd = QuickAddModel()
  @FocusState private var fieldFocused: Bool

  var body: some View {
    NavigationStack {
      VStack(alignment: .leading, spacing: theme.space.md) {
        field
        chips
        Hairline(role: .borderMuted)
        destinationRow
        Toggle("Plan for today", isOn: $quickAdd.plansToday)
          .toggleStyle(ThemedToggleStyle())
          .accessibilityIdentifier("quickAdd.today")
        Text(TaskCapture.syntaxHintText)
          .font(theme.type.footnote)
          .foregroundStyle(theme.dim)
        if !quickAdd.added.isEmpty {
          Hairline(role: .borderMuted)
          SectionLabel("Added")
          ForEach(Array(quickAdd.added.prefix(5).enumerated()), id: \.offset) { _, title in
            Label(title, systemImage: "checkmark")
              .font(theme.type.callout)
              .foregroundStyle(theme.muted)
              .lineLimit(1)
          }
        }
        Spacer(minLength: 0)
      }
      .padding(theme.space.lg)
      .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
      .background(theme.paper)
      .navigationTitle("Add task")
      .navigationBarTitleDisplayMode(.inline)
      .toolbar {
        ToolbarItem(placement: .cancellationAction) {
          Button("Done") { close() }
            .accessibilityIdentifier("quickAdd.done")
        }
        ToolbarItem(placement: .confirmationAction) {
          Button("Add") { add() }
            .disabled(!quickAdd.canAdd)
            .accessibilityIdentifier("quickAdd.add")
        }
      }
    }
    .presentationDetents([.medium, .large])
    .presentationDragIndicator(.visible)
    .onAppear {
      quickAdd.prepare(model)
      #if DEBUG
      // `-quickAddText <text>` types a capture for screenshots.
      if quickAdd.text.isEmpty, let text = UserDefaults.standard.string(forKey: "quickAddText") { quickAdd.text = text }
      #endif
      fieldFocused = true
    }
    .onDisappear {
      model.navigation.quickAddListID = nil
      model.navigation.quickAddParentTaskID = nil
    }
  }

  private var field: some View {
    TextField("What needs doing?", text: $quickAdd.text, axis: .vertical)
      .font(theme.type.sans(16, relativeTo: .body))
      .lineLimit(1...4)
      .focused($fieldFocused)
      .submitLabel(.next)
      .onSubmit(add)
      .controlFrame()
      .accessibilityIdentifier("quickAdd.field")
  }

  @ViewBuilder
  private var chips: some View {
    let capture = quickAdd.capture()
    let labels = quickAdd.chips()
    HStack(spacing: theme.space.xs) {
      if labels.isEmpty {
        Text(" ").font(theme.type.numeral)
      } else {
        Text(capture.title).font(theme.type.caption).foregroundStyle(theme.muted).lineLimit(1)
        ForEach(Array(labels.enumerated()), id: \.offset) { _, label in
          Tag(text: label, tint: theme.primary, mono: true)
        }
        // A start on or after the due day: not filed until one changes.
        if let problem = capture.problem {
          Tag(text: problem, tint: theme.danger)
        }
      }
    }
    .animation(.snappy(duration: 0.15), value: labels)
    .accessibilityElement(children: .ignore)
    .accessibilityLabel(
      labels.isEmpty
        ? "" : (["Will set \(labels.joined(separator: ", "))", capture.problem].compactMap { $0 }.joined(separator: ". ")))
    .accessibilityIdentifier("quickAdd.chips")
  }

  private var destinationRow: some View {
    InspectorRow("Into") {
      Menu {
        ForEach(QuickAddModel.destinations(in: model.structure)) { destination in
          Button {
            quickAdd.destinationID = destination.id
          } label: {
            Text(String(repeating: "   ", count: destination.depth) + destination.title)
          }
        }
      } label: {
        ThemedMenuLabel(title: quickAdd.destination(in: model)?.path ?? "Inbox")
      }
      .accessibilityIdentifier("quickAdd.destination")
    }
  }

  private func add() {
    if quickAdd.add(model) != nil {
      fieldFocused = true
    } else if !quickAdd.canAdd {
      close()
    }
  }

  private func close() {
    if quickAdd.canAdd { quickAdd.add(model) }
    model.navigation.quickAddListID = nil
    model.navigation.quickAddParentTaskID = nil
    dismiss()
  }
}

extension TaskCapture {
  /// The syntax, in one line under the field.
  static let syntaxHintText =
    "End with 30m, @fri, ^mon, #tag or !1 to set an estimate, due day, start day, tag or priority."
}
