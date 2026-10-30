import Foundation
import SwiftUI
import TaktCore
import TaktWorkspace

/// A simple fullscreen text editor for a task's notes.
struct TaskNotesEditor: View {
  @Environment(\.theme) private var theme
  @Environment(\.dismiss) private var dismiss
  @Environment(WorkspaceModel.self) private var model

  @State private var inspector: InspectorModel
  private let ownsInspector: Bool
  private let initialTitle: String?

  @FocusState private var isEditorFocused: Bool

  init(taskID: String, taskTitle: String? = nil) {
    self._inspector = State(initialValue: InspectorModel(taskID: taskID))
    self.ownsInspector = true
    self.initialTitle = taskTitle
  }

  init(inspector: InspectorModel, taskTitle: String? = nil) {
    self._inspector = State(initialValue: inspector)
    self.ownsInspector = false
    self.initialTitle = taskTitle
  }

  private var taskTitle: String? {
    inspector.task?.title ?? initialTitle
  }

  private var notesBinding: Binding<String> {
    Binding(
      get: { inspector.values?.notes ?? "" },
      set: { value in inspector.edit(model) { $0.notes = value } }
    )
  }

  private var wordCount: Int {
    let text = inspector.values?.notes.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    if text.isEmpty { return 0 }
    return text.components(separatedBy: .whitespacesAndNewlines).filter { !$0.isEmpty }.count
  }

  var body: some View {
    VStack(spacing: 0) {
      if let title = taskTitle, !title.isEmpty {
        HStack(spacing: theme.space.xs) {
          Text(title)
            .font(theme.type.bodyMedium)
            .foregroundStyle(theme.ink)
            .lineLimit(1)
          Spacer()
          if wordCount > 0 {
            Text("\(wordCount) \(wordCount == 1 ? "word" : "words")")
              .font(theme.type.mono(12))
              .foregroundStyle(theme.dim)
          }
        }
        .padding(.horizontal, theme.space.lg)
        .padding(.vertical, theme.space.sm)
        .background(theme.raised)
        .overlay(alignment: .bottom) { Hairline(role: .borderMuted) }
      }

      ZStack(alignment: .topLeading) {
        TextEditor(text: notesBinding)
          .font(theme.type.body)
          .foregroundStyle(theme.ink)
          .scrollContentBackground(.hidden)
          .focused($isEditorFocused)
          .padding(.horizontal, theme.space.md)
          .padding(.vertical, theme.space.sm)
          .accessibilityIdentifier("notesEditor.text")

        if (inspector.values?.notes ?? "").isEmpty {
          Text("Add notes…")
            .font(theme.type.body)
            .foregroundStyle(theme.dim)
            .padding(.horizontal, theme.space.md + 5)
            .padding(.vertical, theme.space.sm + 8)
            .allowsHitTesting(false)
        }
      }
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .background(theme.paper)
    }
    .navigationTitle("Notes")
    .navigationBarTitleDisplayMode(.inline)
    .toolbar {
      ToolbarItem(placement: .confirmationAction) {
        Button("Done") {
          inspector.save(model)
          dismiss()
        }
        .accessibilityIdentifier("notesEditor.done")
      }
      ToolbarItemGroup(placement: .keyboard) {
        if wordCount > 0 {
          Text("\(wordCount) \(wordCount == 1 ? "word" : "words")")
            .font(theme.type.caption)
            .foregroundStyle(theme.dim)
        }
        Spacer()
        Button {
          isEditorFocused = false
        } label: {
          Image(systemName: "keyboard.chevron.compact.down")
        }
        .accessibilityLabel("Dismiss keyboard")
      }
    }
    .task {
      if ownsInspector {
        inspector.load(model)
      }
      isEditorFocused = true
    }
    .onChange(of: model.revision) { _, _ in
      if ownsInspector { inspector.refresh(model) }
    }
    .onDisappear {
      inspector.save(model)
    }
  }
}
