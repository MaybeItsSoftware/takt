import TaktWorkspace
import SwiftUI

/// The folder and list tree: Everything, the Inbox, pinned lists, folders
/// (nested), lists, and the lists nested inside lists. The iPhone Lists tab,
/// and the lower half of the iPad sidebar.
struct ListsTreeView: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.isPadLayout) private var isPad
  /// On iPad the tree is part of the sidebar's `List`, which owns selection.
  var embedded = false

  var body: some View {
    if embedded {
      ListsTreeRows()
    } else {
      List {
        ListsTreeRows()
      }
      .listStyle(.sidebar)
      .scrollContentBackground(.hidden)
      .background(theme.paper)
      .navigationTitle("Lists")
      .navigationBarTitleDisplayMode(.inline)
      .toolbar {
        ListsTreeToolbar()
        WorkspaceToolbar()
        ToolbarItem(placement: .topBarLeading) {
          Button { model.navigation.isSettingsPresented = true } label: { Image(systemName: "gearshape") }
            .accessibilityLabel("Settings")
            .accessibilityIdentifier("toolbar.settings")
        }
      }
      .overlay(alignment: .bottomTrailing) {
        if !isPad { AddButton() }
      }
      .refreshable { await SyncController.shared?.syncNow() }
      .accessibilityIdentifier("lists.tree")
    }
  }
}

/// The "New list / New folder" menu.
struct ListsTreeToolbar: ToolbarContent {
  @Environment(WorkspaceModel.self) private var model

  var body: some ToolbarContent {
    ToolbarItem(placement: .topBarTrailing) {
      Menu {
        Button { model.navigation.namePrompt = .newList(folderID: nil) } label: {
          Label("New list", systemImage: "list.bullet")
        }
        Button { model.navigation.namePrompt = .newFolder(parentID: nil) } label: {
          Label("New folder", systemImage: "folder.badge.plus")
        }
        Divider()
        Button { model.setAllFoldersExpanded(false) } label: {
          Label("Collapse every folder", systemImage: "chevron.up.chevron.down")
        }
        .disabled(model.structure.folders.isEmpty)
        Button { model.setAllFoldersExpanded(true) } label: {
          Label("Expand every folder", systemImage: "chevron.down")
        }
        .disabled(model.structure.folders.isEmpty)
        Button { model.restoreLastArchivedList() } label: {
          Label("Restore the last archived list", systemImage: "tray.and.arrow.up")
        }
        .disabled(model.structure.archivedLists.isEmpty)
        Button { model.navigation.joinInvite = JoinInvite(text: "") } label: {
          Label("Join a shared list…", systemImage: "person.badge.plus")
        }
        #if DEBUG
        Divider()
        Button { model.seedTasks() } label: { Label("Seed 5,000 tasks", systemImage: "hammer") }
        #endif
      } label: {
        Image(systemName: "folder.badge.plus")
      }
      .accessibilityLabel("New list or folder")
      .accessibilityIdentifier("lists.new")
    }
  }
}

/// What a name prompt is for. One alert serves every create and rename.
enum NamePrompt: Identifiable, Equatable {
  case newList(folderID: String?)
  case newFolder(parentID: String?)
  case newNestedList(listID: String, parentTaskID: String?)
  case renameList(String)
  case renameFolder(String)
  case moveToNewList(taskID: String)

  var id: String {
    switch self {
    case .newList(let id): "newList-\(id ?? "")"
    case .newFolder(let id): "newFolder-\(id ?? "")"
    case .newNestedList(let list, let parent): "nested-\(list)-\(parent ?? "")"
    case .renameList(let id): "renameList-\(id)"
    case .renameFolder(let id): "renameFolder-\(id)"
    case .moveToNewList(let id): "moveNew-\(id)"
    }
  }

  var title: String {
    switch self {
    case .newList, .moveToNewList: "New list"
    case .newFolder: "New folder"
    case .newNestedList: "New nested list"
    case .renameList: "Rename list"
    case .renameFolder: "Rename folder"
    }
  }
}

/// The rows themselves, shared by the phone's tab and the pad's sidebar.
struct ListsTreeRows: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.isPadLayout) private var isPad
  @State private var showsArchived = false

  private var structure: WorkspaceStructure { model.structure }

  var body: some View {
    Section {
      scopeRow(.everything, title: "Everything", symbol: "tray.full", count: nil)
      if let inbox = structure.inbox {
        listRow(inbox, depth: 0)
      }
      // Filter lists sit under Everything and the Inbox, as on the Mac. Read
      // only here: they are made and edited on the desktop.
      ForEach(structure.filterLists) { filter in
        scopeRow(
          .filter(filter.id), title: filter.name, symbol: filter.icon ?? WorkspaceFilterList.defaultIcon,
          count: nil, subtitle: filter.definition.summary)
      }
      ForEach(structure.promotedLists) { task in
        scopeRow(.nested(listID: task.listId, taskID: task.id), title: task.title, symbol: "pin", count: structure.sidebar.taskCounts[task.id])
          .contextMenu {
            Button { model.togglePromoted(task.id) } label: { Label("Unpin", systemImage: "pin.slash") }
          }
      }
    }
    Section {
      ForEach(rootFolders) { folder in
        folderBranch(folder, depth: 0)
      }
      ForEach(rootLists) { list in
        listRow(list, depth: 0)
      }
      .onMove { source, destination in reorder(rootLists, source: source, destination: destination, folderID: nil) }
    } header: {
      Text("Lists").font(theme.type.caption).foregroundStyle(theme.muted).textCase(nil)
    }
    if !structure.archivedLists.isEmpty {
      Section {
        DisclosureGroup(isExpanded: $showsArchived) {
          ForEach(structure.archivedLists) { list in
            HStack {
              Text(list.name).font(theme.type.body).foregroundStyle(theme.muted)
              Spacer()
              Button("Restore") { model.setArchived(false, list: list.id) }
                .buttonStyle(ThemedButtonStyle(kind: .quiet, compact: true))
            }
            .contextMenu {
              Button { model.setArchived(false, list: list.id) } label: { Label("Restore", systemImage: "tray.and.arrow.up") }
              Button(role: .destructive) {
                model.navigation.pendingDeletion = PendingDeletion(id: list.id, isFolder: false, name: list.name)
              } label: { Label("Delete…", systemImage: "trash") }
            }
          }
        } label: {
          Label("Archived", systemImage: "archivebox").font(theme.type.body).foregroundStyle(theme.muted)
        }
      }
    }
    Section {
      SyncStatusLine()
    }
  }


  private var rootFolders: [ListFolder] { structure.folders.filter { $0.parentFolderId == nil } }
  private var rootLists: [TaskList] { structure.lists.filter { $0.folderId == nil && $0.systemRole == nil } }

  // MARK: - Rows

  private func folderBranch(_ folder: ListFolder, depth: Int) -> AnyFolderBranch {
    AnyFolderBranch(folder: folder, depth: depth, rows: self)
  }

  fileprivate func folderContents(_ folder: ListFolder, depth: Int) -> some View {
    let childFolders = structure.folders.filter { $0.parentFolderId == folder.id }
    let lists = structure.lists.filter { $0.folderId == folder.id }
    return Group {
      ForEach(childFolders) { child in
        folderBranch(child, depth: depth + 1)
      }
      ForEach(lists) { list in
        listRow(list, depth: depth + 1)
      }
      .onMove { source, destination in reorder(lists, source: source, destination: destination, folderID: folder.id) }
    }
  }

  fileprivate func folderLabel(_ folder: ListFolder) -> some View {
    Button {
      model.navigation.open(.folder(folder.id), isPad: isPad)
    } label: {
      Label(folder.name, systemImage: "folder")
        .font(theme.type.body)
        .foregroundStyle(theme.ink)
    }
    .buttonStyle(.plain)
    .accessibilityIdentifier("lists.folder.\(folder.name)")
    .contextMenu { folderMenu(folder) }
  }

  @ViewBuilder
  private func listRow(_ list: TaskList, depth: Int) -> some View {
    let nested = structure.sidebar.nestedLists.filter { $0.task.listId == list.id && !($0.task.isPromoted ?? false) }
    scopeRow(
      .list(list.id), title: list.name,
      symbol: list.systemRole == .inbox ? "tray" : structure.sharedListIDs.contains(list.id) ? "person.2" : "list.bullet",
      count: structure.sidebar.taskCounts[list.id], tint: ListColor.color(hex: list.colorHex),
      completed: list.completedAt != nil
    )
    .contextMenu { listMenu(list) }
    ForEach(nested) { item in
      scopeRow(
        .nested(listID: list.id, taskID: item.id), title: item.task.title, symbol: "list.bullet.indent",
        count: structure.sidebar.taskCounts[item.id], indent: CGFloat(item.depth + 1) * theme.space.lg
      )
      .contextMenu {
        Button { model.togglePromoted(item.id) } label: { Label("Pin to lists", systemImage: "pin") }
        Button { model.toggleListKind(item.id) } label: { Label("Convert to task", systemImage: "checkmark.square") }
        Button { model.setNestedListArchived(true, taskID: item.id) } label: { Label("Archive", systemImage: "archivebox") }
        Button { _ = model.extractBranch(item.id) } label: {
          Label("Extract to its own list", systemImage: "arrow.up.right.square")
        }
      }
    }
  }

  private func scopeRow(
    _ scope: ListScope, title: String, symbol: String, count: Int?, tint: Color? = nil, completed: Bool = false,
    indent: CGFloat = 0, subtitle: String? = nil
  ) -> some View {
    let label = HStack(spacing: theme.space.sm) {
      Image(systemName: symbol)
        .foregroundStyle(tint ?? theme.muted)
        .frame(width: 22)
      VStack(alignment: .leading, spacing: 1) {
        Text(title)
          .font(theme.type.body)
          .foregroundStyle(completed ? theme.muted : theme.ink)
          .strikethrough(completed, color: theme.dim)
          .lineLimit(1)
        if let subtitle {
          Text(subtitle)
            .font(theme.type.footnote)
            .foregroundStyle(theme.muted)
            .lineLimit(1)
        }
      }
      Spacer(minLength: theme.space.sm)
      if let count, count > 0 {
        Text("\(count)").font(theme.type.numeral).foregroundStyle(theme.muted)
      }
    }
    .padding(.leading, indent)
    .contentShape(Rectangle())
    return Group {
      if isPad {
        // The sidebar's `List(selection:)` reads the tag.
        label.tag(SidebarItem.scope(scope))
      } else {
        NavigationLink(value: scope) { label }
      }
    }
    .accessibilityIdentifier("lists.row.\(title)")
  }

  // MARK: - Menus

  @ViewBuilder
  private func listMenu(_ list: TaskList) -> some View {
    if list.systemRole == nil {
      Button { model.navigation.namePrompt = .renameList(list.id) } label: { Label("Rename", systemImage: "pencil") }
      Button { model.navigation.listSettingsID = list.id } label: { Label("List settings…", systemImage: "paintpalette") }
      Button { model.navigation.namePrompt = .newNestedList(listID: list.id, parentTaskID: nil) } label: {
        Label("New nested list", systemImage: "list.bullet.indent")
      }
      Menu {
        Button("Top level") { model.moveList(list.id, toFolder: nil) }
        ForEach(structure.folders) { folder in
          Button(folder.name) { model.moveList(list.id, toFolder: folder.id) }
        }
      } label: { Label("Move to folder", systemImage: "folder") }
      Button { model.moveList(list.id, by: -1) } label: { Label("Move up", systemImage: "arrow.up") }
      Button { model.moveList(list.id, by: 1) } label: { Label("Move down", systemImage: "arrow.down") }
      Divider()
      Button { model.toggleCompleted(list: list.id) } label: {
        Label(list.completedAt == nil ? "Complete list" : "Reopen list", systemImage: "checkmark.square")
      }
      Button { model.setArchived(true, list: list.id) } label: { Label("Archive", systemImage: "archivebox") }
      Button { model.navigation.sharingListID = list.id } label: {
        Label(structure.sharedListIDs.contains(list.id) ? "Sharing…" : "Share…", systemImage: "person.2")
      }
      if structure.sharedListIDs.contains(list.id) {
        // A member leaves and an owner stops sharing first, from the sheet.
      } else {
        Button { model.convertListToTask(list.id) } label: { Label("Convert to task", systemImage: "checkmark.square") }
        Button(role: .destructive) {
          model.navigation.pendingDeletion = PendingDeletion(id: list.id, isFolder: false, name: list.name)
        } label: { Label("Delete…", systemImage: "trash") }
      }
    } else {
      Button { model.navigation.listSettingsID = list.id } label: { Label("List settings…", systemImage: "paintpalette") }
    }
  }

  @ViewBuilder
  private func folderMenu(_ folder: ListFolder) -> some View {
    Button { model.navigation.namePrompt = .newList(folderID: folder.id) } label: { Label("New list here", systemImage: "plus") }
    Button { model.navigation.namePrompt = .newFolder(parentID: folder.id) } label: {
      Label("New folder here", systemImage: "folder.badge.plus")
    }
    Button { model.navigation.namePrompt = .renameFolder(folder.id) } label: { Label("Rename", systemImage: "pencil") }
    Menu {
      Button("Top level") { model.moveFolder(folder.id, toFolder: nil) }
      ForEach(structure.folders.filter { $0.id != folder.id }) { other in
        Button(other.name) { model.moveFolder(folder.id, toFolder: other.id) }
      }
    } label: { Label("Move to folder", systemImage: "folder") }
    Button { model.moveFolder(folder.id, by: -1) } label: { Label("Move up", systemImage: "arrow.up") }
    Button { model.moveFolder(folder.id, by: 1) } label: { Label("Move down", systemImage: "arrow.down") }
    Button(role: .destructive) {
      model.navigation.pendingDeletion = PendingDeletion(id: folder.id, isFolder: true, name: folder.name)
    } label: { Label("Delete…", systemImage: "trash") }
  }

  private func reorder(_ lists: [TaskList], source: IndexSet, destination: Int, folderID: String?) {
    guard let index = source.first, lists.indices.contains(index) else { return }
    let before = destination < lists.count ? lists[destination].id : nil
    guard before != lists[index].id else { return }
    model.placeList(lists[index].id, before: before, inFolder: folderID)
  }
}

/// A folder and its contents, as a disclosure group. A struct of its own so
/// the recursion has a concrete type.
struct AnyFolderBranch: View {
  let folder: ListFolder
  let depth: Int
  let rows: ListsTreeRows
  @AppStorage private var isExpanded: Bool

  init(folder: ListFolder, depth: Int, rows: ListsTreeRows) {
    self.folder = folder
    self.depth = depth
    self.rows = rows
    _isExpanded = AppStorage(wrappedValue: true, WorkspaceModel.folderExpandedKey(folder.id))
  }

  var body: some View {
    DisclosureGroup(isExpanded: $isExpanded) {
      rows.folderContents(folder, depth: depth)
    } label: {
      rows.folderLabel(folder)
    }
  }
}

/// Presents the name alert for whatever `navigation.namePrompt` asks.
struct NamePromptHost: ViewModifier {
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.isPadLayout) private var isPad
  @State private var text = ""

  func body(content: Content) -> some View {
    content
      .alert(model.navigation.namePrompt?.title ?? "", isPresented: binding, presenting: model.navigation.namePrompt) { prompt in
        TextField("Name", text: $text)
          .accessibilityIdentifier("namePrompt.field")
        Button("Cancel", role: .cancel) {}
        Button("Save") { commit(prompt) }
          .accessibilityIdentifier("namePrompt.save")
      }
      .onChange(of: model.navigation.namePrompt) { _, prompt in
        text = prompt.map(initialText) ?? ""
      }
  }

  private var binding: Binding<Bool> {
    Binding(get: { model.navigation.namePrompt != nil }, set: { if !$0 { model.navigation.namePrompt = nil } })
  }

  private func initialText(_ prompt: NamePrompt) -> String {
    switch prompt {
    case .renameList(let id): model.structure.list(id)?.name ?? ""
    case .renameFolder(let id): model.structure.folder(id)?.name ?? ""
    default: ""
    }
  }

  private func commit(_ prompt: NamePrompt) {
    let name = text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !name.isEmpty else { return }
    switch prompt {
    case .newList(let folderID):
      if let list = model.createList(named: name, inFolder: folderID) { model.navigation.open(.list(list.id), isPad: isPad) }
    case .newFolder(let parentID): model.createFolder(named: name, inFolder: parentID)
    case .newNestedList(let listID, let parentTaskID):
      model.createNestedList(named: name, inList: listID, parentTaskID: parentTaskID)
    case .renameList(let id): model.renameList(id, to: name)
    case .renameFolder(let id): model.renameFolder(id, to: name)
    case .moveToNewList(let taskID): model.move(taskID, toNewListNamed: name)
    }
  }
}

/// A list's name and colour.
struct ListSettingsSheet: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.dismiss) private var dismiss
  let listID: String
  @State private var name = ""
  @State private var colorHex: String?

  var body: some View {
    NavigationStack {
      Form {
        Section("Name") {
          TextField("Name", text: $name).font(theme.type.body)
        }
        Section("Colour") {
          LazyVGrid(columns: [GridItem(.adaptive(minimum: 44))], spacing: theme.space.md) {
            swatch(nil, name: "None")
            ForEach(ListColor.choices, id: \.hex) { entry in
              swatch(entry.hex, name: entry.name)
            }
          }
          .padding(.vertical, theme.space.sm)
        }
      }
      .scrollContentBackground(.hidden)
      .background(theme.paper)
      .navigationTitle("List settings")
      .navigationBarTitleDisplayMode(.inline)
      .toolbar {
        ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
        ToolbarItem(placement: .confirmationAction) {
          Button("Save") {
            model.saveList(listID, name: name, colorHex: colorHex)
            dismiss()
          }
          .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
        }
      }
      .onAppear {
        let list = model.structure.list(listID)
        name = list?.name ?? ""
        colorHex = list?.colorHex
      }
    }
    .presentationDetents([.medium])
  }

  private func swatch(_ hex: String?, name: String) -> some View {
    let selected = colorHex == hex
    return Button {
      colorHex = hex
    } label: {
      ZStack {
        Circle().fill(ListColor.color(hex: hex) ?? theme.well)
        Circle().strokeBorder(selected ? theme.ink : theme.border, lineWidth: selected ? theme.emphasis : theme.stroke)
        if hex == nil { Image(systemName: "slash.circle").foregroundStyle(theme.muted) }
      }
      .frame(width: 32, height: 32)
      .frame(width: 44, height: theme.touchTarget)
    }
    .buttonStyle(.plain)
    .accessibilityLabel(name)
    .accessibilityAddTraits(selected ? .isSelected : [])
  }
}

/// A list or folder waiting on delete confirmation.
struct PendingDeletion: Identifiable, Equatable {
  let id: String
  let isFolder: Bool
  let name: String
}

/// The tree's alerts, dialogs and sheets, hung on the root view so they work
/// whether the tree is a tab or part of the iPad sidebar.
struct ListsPresenters: ViewModifier {
  @Environment(WorkspaceModel.self) private var model

  func body(content: Content) -> some View {
    content
      .modifier(NamePromptHost())
      .confirmationDialog(
        model.navigation.pendingDeletion.map { "Delete \($0.name)?" } ?? "", isPresented: deletionBinding,
        titleVisibility: .visible, presenting: model.navigation.pendingDeletion
      ) { item in
        Button("Delete", role: .destructive) {
          if item.isFolder { model.deleteFolder(item.id) } else { model.deleteList(item.id) }
        }
      } message: { item in
        Text(item.isFolder
          ? "Lists remain, but move to the top level. Nested folders are deleted."
          : "This permanently deletes the list and all of its tasks. You can undo it.")
      }
      .sheet(item: settingsBinding) { item in
        ListSettingsSheet(listID: item.id).environment(model)
      }
      .sheet(item: sharingBinding) { item in
        SharedListSheet(listID: item.id).environment(model)
      }
      .sheet(item: joinBinding) { invite in
        JoinListSheet(initialText: invite.text).environment(model)
      }
  }

  private var deletionBinding: Binding<Bool> {
    Binding(get: { model.navigation.pendingDeletion != nil }, set: { if !$0 { model.navigation.pendingDeletion = nil } })
  }

  private var sharingBinding: Binding<IdentifiedString?> {
    Binding(
      get: { model.navigation.sharingListID.map(IdentifiedString.init) },
      set: { model.navigation.sharingListID = $0?.id })
  }

  private var joinBinding: Binding<JoinInvite?> {
    Binding(get: { model.navigation.joinInvite }, set: { model.navigation.joinInvite = $0 })
  }

  private var settingsBinding: Binding<IdentifiedString?> {
    Binding(
      get: { model.navigation.listSettingsID.map(IdentifiedString.init) },
      set: { model.navigation.listSettingsID = $0?.id })
  }
}
