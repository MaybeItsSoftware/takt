import TaktWorkspace
import SwiftUI

/// Manage conditions: add, rename, file under a category, archive and
/// restore. The Mac's Conditions tab. Conditions show under their category's
/// heading; Location is the category whose conditions are places, so
/// choosing one in Focus replaces the last.
///
/// Tasks name conditions by identity, so a rename here is a rename
/// everywhere and every requirement survives it. Archiving takes a condition
/// out of the pickers and the Focus chips without touching the tasks that
/// still need it — restore it and they are as they were.
struct ConditionsSheet: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.dismiss) private var dismiss
  @State private var catalogue = StoreQuery(ConditionsCatalogue.empty)
  @State private var newName = ""
  /// The category a new condition is filed under; nil is none.
  @State private var newCategory: String?
  /// Typing a new category: for this condition, or (nil) for the new one.
  @State private var namingCategoryFor: TaskCondition?
  @State private var isNamingCategory = false
  @State private var categoryText = ""
  @State private var renaming: TaskCondition?
  @State private var renameText = ""
  @FocusState private var addFocused: Bool

  var body: some View {
    NavigationStack {
      List {
        Section {
          Text("Tasks refer to conditions by identity. Renaming one keeps every requirement on it intact.")
            .font(theme.type.caption)
            .foregroundStyle(theme.muted)
            .listRowBackground(Color.clear)
            .listRowInsets(EdgeInsets(top: 0, leading: theme.space.xs, bottom: 0, trailing: theme.space.xs))
        }
        if catalogue.isLoaded, catalogue.value.active.isEmpty {
          Section {
            Text("No conditions yet").font(theme.type.body).foregroundStyle(theme.muted)
          } header: {
            label("Available")
          }
        }
        ForEach(catalogue.value.activeSections) { section in
          Section {
            ForEach(section.conditions) { condition in row(condition) }
          } header: {
            label(section.title)
          }
        }
        Section {
          HStack(spacing: theme.space.sm) {
            TextField("New condition", text: $newName)
              .font(theme.type.body)
              .focused($addFocused)
              .submitLabel(.done)
              .onSubmit(add)
              .accessibilityIdentifier("conditions.newName")
            Menu {
              categoryChoices(current: newCategory, choose: { newCategory = $0 }, for: nil)
            } label: {
              Tag(text: newCategory ?? "No category", tint: newCategory == nil ? theme.dim : theme.primary,
                  systemImage: "folder")
            }
            .accessibilityLabel("Category")
            .accessibilityValue(newCategory ?? "None")
            .accessibilityIdentifier("conditions.newCategory")
            Button("Add", action: add)
              .buttonStyle(ThemedButtonStyle(kind: .primary, compact: true))
              .disabled(newName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
              .accessibilityIdentifier("conditions.add")
          }
          .listRowBackground(theme.raised)
        } header: {
          label("Add")
        } footer: {
          Text("Location is where you are: choosing one in Focus replaces the last. Any other category — Equipment, Energy — combines. Focus offers only what an open task needs. "
            + "A condition marked with a device (hold it for its menu) holds on that kind of device by itself.")
            .font(theme.type.footnote)
            .foregroundStyle(theme.dim)
        }
        if !catalogue.value.archived.isEmpty {
          Section {
            ForEach(catalogue.value.archived) { condition in row(condition) }
          } header: {
            label("Archived")
          }
        }
      }
      .scrollContentBackground(.hidden)
      .background(theme.paper)
      .navigationTitle("Conditions")
      .navigationBarTitleDisplayMode(.inline)
      .toolbar {
        ToolbarItem(placement: .confirmationAction) {
          Button("Done") { dismiss() }
            .accessibilityIdentifier("conditions.done")
        }
      }
      .task(id: model.revision) {
        let workspaceID = model.workspace.id
        await catalogue.load(model.store) { store in try ConditionsCatalogue.load(store: store, workspaceID: workspaceID) }
      }
      .alert("New category", isPresented: $isNamingCategory) {
        TextField("Category", text: $categoryText)
        Button("Cancel", role: .cancel) { namingCategoryFor = nil }
        Button("File") {
          let category = TaskConditionSection.normalizedCategory(categoryText)
          if let condition = namingCategoryFor {
            model.setConditionCategory(condition, category: category)
          } else {
            newCategory = category
          }
          namingCategoryFor = nil
        }
      }
      .alert("Rename condition", isPresented: renameBinding, presenting: renaming) { condition in
        TextField("Name", text: $renameText)
        Button("Cancel", role: .cancel) { renaming = nil }
        Button("Rename") {
          let name = renameText.trimmingCharacters(in: .whitespacesAndNewlines)
          if !name.isEmpty, name != condition.name { model.saveCondition(condition, name: name) }
          renaming = nil
        }
      }
    }
    .presentationDetents([.medium, .large])
  }

  private var renameBinding: Binding<Bool> {
    Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })
  }

  private func label(_ text: String) -> some View {
    Text(text).font(theme.type.caption).foregroundStyle(theme.muted).textCase(nil)
  }

  private func row(_ condition: TaskCondition) -> some View {
    let uses = catalogue.value.usage[condition.id] ?? 0
    return Button {
      startRename(condition)
    } label: {
      HStack(spacing: theme.space.sm) {
        Image(systemName: condition.device?.symbol ?? (condition.isLocation ? "mappin" : "circle.dotted"))
          .foregroundStyle(condition.isArchived ? theme.dim : theme.purple)
          .frame(width: 20)
        VStack(alignment: .leading, spacing: 1) {
          Text(condition.name)
            .font(theme.type.body)
            .foregroundStyle(condition.isArchived ? theme.muted : theme.ink)
          Text(detail(condition, uses: uses))
            .font(theme.type.footnote)
            .foregroundStyle(theme.dim)
        }
        Spacer(minLength: 0)
        Image(systemName: "pencil").foregroundStyle(theme.dim).imageScale(.small)
      }
      .frame(minHeight: theme.touchTarget - 8)
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .listRowBackground(theme.raised)
    .swipeActions(edge: .trailing, allowsFullSwipe: true) {
      if condition.isArchived {
        Button { model.saveCondition(condition, isArchived: false) } label: {
          Label("Restore", systemImage: "arrow.uturn.backward")
        }
        .tint(theme.success)
      } else {
        Button { model.saveCondition(condition, isArchived: true) } label: {
          Label("Archive", systemImage: "archivebox")
        }
        .tint(theme.warning)
      }
    }
    .contextMenu {
      Button { startRename(condition) } label: { Label("Rename", systemImage: "pencil") }
      Menu {
        categoryChoices(current: condition.category, choose: { model.setConditionCategory(condition, category: $0) },
                        for: condition)
      } label: {
        Label("Category", systemImage: "folder")
      }
      Menu {
        deviceChoices(condition)
      } label: {
        Label("Device", systemImage: "laptopcomputer.and.iphone")
      }
      if condition.isArchived {
        Button { model.saveCondition(condition, isArchived: false) } label: {
          Label("Restore", systemImage: "arrow.uturn.backward")
        }
      } else {
        Button { model.saveCondition(condition, isArchived: true) } label: {
          Label("Archive", systemImage: "archivebox")
        }
      }
    }
    .accessibilityIdentifier("conditions.row.\(condition.name)")
    .accessibilityActions {
      Button(condition.isArchived ? "Restore" : "Archive") {
        model.saveCondition(condition, isArchived: !condition.isArchived)
      }
      Button("New category") { startNamingCategory(for: condition) }
      ForEach(ConditionDeviceKind.allCases) { kind in
        Button(condition.device == kind ? "Not a \(kind.rawValue) condition" : "Means a \(kind.rawValue)") {
          model.setConditionDevice(condition, kind: condition.device == kind ? nil : kind)
        }
      }
    }
  }

  private func detail(_ condition: TaskCondition, uses: Int) -> String {
    var parts = [condition.categoryTitle]
    parts.append(uses == 0 ? "No tasks" : uses == 1 ? "1 task" : "\(uses) tasks")
    if condition.isArchived { parts.append("Archived") }
    return parts.joined(separator: " · ")
  }

  /// None, or the kind of device it means: a device condition holds on a
  /// device of that kind by itself and is never said to hold.
  @ViewBuilder
  private func deviceChoices(_ condition: TaskCondition) -> some View {
    Button { model.setConditionDevice(condition, kind: nil) } label: {
      if condition.isDevice { Text("None") } else { Label("None", systemImage: "checkmark") }
    }
    ForEach(ConditionDeviceKind.allCases) { kind in
      Button { model.setConditionDevice(condition, kind: kind) } label: {
        if condition.device == kind { Label(kind.title, systemImage: "checkmark") } else { Text(kind.title) }
      }
    }
  }

  /// The categories in use (Location first), "No category", and "New
  /// category…", which asks for a name.
  @ViewBuilder
  private func categoryChoices(current: String?, choose: @escaping (String?) -> Void, for condition: TaskCondition?) -> some View {
    ForEach(catalogue.value.categories, id: \.self) { name in
      Button { choose(name) } label: {
        if name == current { Label(name, systemImage: "checkmark") } else { Text(name) }
      }
    }
    Button { choose(nil) } label: {
      if current == nil { Label("No category", systemImage: "checkmark") } else { Text("No category") }
    }
    Divider()
    Button("New category…") { startNamingCategory(for: condition) }
  }

  private func startNamingCategory(for condition: TaskCondition?) {
    namingCategoryFor = condition
    categoryText = ""
    isNamingCategory = true
  }

  private func startRename(_ condition: TaskCondition) {
    renameText = condition.name
    renaming = condition
  }

  private func add() {
    let name = newName.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !name.isEmpty else { return }
    // The category stays chosen, for the next one in it.
    if model.createCondition(named: name, category: newCategory) != nil {
      newName = ""
    }
  }
}
