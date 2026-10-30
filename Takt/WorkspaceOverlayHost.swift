import TaktCore
import TaktWorkspace
import SwiftUI

/// The one place anything is drawn over the workspace: top-anchored, instant,
/// untitled, on the raised surface with a hairline round it.
///
/// It replaced twelve sheets. A sheet slid down from the title bar, took the
/// window's key monitor down with it, and could not be replaced by another —
/// so a command palette that could not be reached from search was the normal
/// state of affairs. Here, whatever `activeOverlay` names is what is up, and
/// clicking anywhere outside it puts it away.
///
/// That click is caught by the window's mouse monitor, not by a SwiftUI layer
/// here. A transparent tap-catcher behind the card never saw clicks over the
/// sidebar or the outline: those are AppKit tables inside the hosting view,
/// and AppKit hands a mouse-down to the deepest `NSView` under it before
/// SwiftUI's gestures get a look. So the card reports its frame, and
/// `MainWindowController` compares each mouse-down against it.
struct WorkspaceOverlayHost: View {
  @Environment(WorkspaceViewModel.self) private var model
  @Environment(\.theme) private var theme
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    // No scrim: the overlay is a tool held over the work, not a modal that
    // asks you to stop looking at it.
    ZStack(alignment: .top) {
      if let overlay = model.windowOverlay {
        WorkspaceOverlayContent(overlay: overlay)
          .frame(width: WorkspaceOverlayMetrics.width)
          .themedSurface(theme, fill: theme.raised, radius: theme.panelRadius)
          .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { frame in
            model.overlayPanelFrame = frame
          }
          .padding(.top, theme.space.xl)
          .id(overlay.id)
          .transition(.opacity)
      }
    }
    .animation(
      reduceMotion ? nil : .easeOut(duration: WorkspaceOverlayMetrics.fade),
      value: model.windowOverlay?.id)
  }
}

/// What an overlay draws, whichever window it is up in: the main window's
/// host, or the focus panel's (`FocusPanelView`).
struct WorkspaceOverlayContent: View {
  @Environment(WorkspaceViewModel.self) private var model
  let overlay: WorkspaceOverlay

  var body: some View {
    switch overlay {
    case .commandPalette:
      WorkspaceCommandPalette(overlayID: overlay.id)
    case .search:
      WorkspaceSearchOverlay(overlayID: overlay.id)
    case .listNavigator:
      WorkspaceListNavigator(overlayID: overlay.id)
    case .keyboardReference:
      WorkspaceKeyboardReference(overlayID: overlay.id)
    case .move(let request):
      WorkspaceMoveOverlay(overlayID: overlay.id, request: request)
    case .quickEdit(let request):
      WorkspaceTaskQuickEditOverlay(overlayID: overlay.id, request: request)
    case .habit(let request):
      WorkspaceHabitOverlay(overlayID: overlay.id, request: request)
    case .commitment(let request):
      WorkspaceCommitmentOverlay(overlayID: overlay.id, request: request)
    case .waiting(let request):
      WorkspaceWaitingOverlay(overlayID: overlay.id, request: request)
    case .blocker(let request):
      WorkspaceBlockerFinder(overlayID: overlay.id, request: request)
    case .link(let request):
      WorkspaceLinkPicker(overlayID: overlay.id, request: request)
    case .quickAsk(let request):
      WorkspaceQuickAskOverlay(overlayID: overlay.id, request: request)
    case .conditions(let request):
      WorkspaceConditionPicker(overlayID: overlay.id, request: request)
    case .conditionCategory(let request):
      WorkspaceConditionCategoryFinder(overlayID: overlay.id, request: request)
    case .conditionFinder(let request):
      WorkspaceConditionFinder(overlayID: overlay.id, request: request)
    case .renameConditionCategory(let request):
      WorkspaceNameOverlay(
        overlayID: overlay.id, title: "Rename category", prompt: "Category name",
        context: "Every condition under \(request.title) moves with it; a name in use merges the two",
        initialName: request.category ?? "", submitHint: "↩ rename · esc cancel"
      ) { model.renameConditionCategory(request.category, to: $0) }
    case .create(let kind):
      WorkspaceNameOverlay(
        overlayID: overlay.id, title: kind.title, prompt: "Name",
        context: creationContext(kind)
      ) { name in
        switch kind {
        case .list:
          if model.creationIsNested {
            model.createNestedList(named: name)
          } else {
            model.createList(named: name, in: model.creationParentFolderID)
          }
        case .folder: model.createFolder(named: name, in: model.creationParentFolderID)
        }
      }
    case .newBoardColumn:
      WorkspaceNameOverlay(
        overlayID: overlay.id, title: "New board column", prompt: "Column name",
        context: model.pendingColumnInsertion.flatMap { insertion in
          model.boardColumnsByID[insertion.anchorID].map { "\(insertion.after ? "Right" : "Left") of \($0.title)" }
        }
      ) { model.addKanbanColumn(named: $0) }
    case .renameBoardColumn(let id):
      WorkspaceNameOverlay(
        overlayID: overlay.id, title: "Rename column", prompt: "Column name",
        context: model.boardColumnsByID[id]?.title, initialName: model.boardColumnsByID[id]?.title ?? "",
        submitHint: "↩ rename · esc cancel"
      ) { model.renameKanbanColumn(id, to: $0) }
    case .deleteBoardColumn(let id):
      WorkspaceDeleteColumnOverlay(overlayID: overlay.id, columnID: id)
    case .recurrenceClash(let request):
      WorkspaceRecurrenceClashOverlay(overlayID: overlay.id, request: request)
    case .seriesRename(let request):
      WorkspaceSeriesRenameOverlay(overlayID: overlay.id, request: request)
    case .startLater(let request):
      WorkspaceStartLaterOverlay(overlayID: overlay.id, request: request)
    }
  }

  /// Where the new list or folder will land, which the sheet never said.
  private func creationContext(_ kind: WorkspaceCreationKind) -> String? {
    if kind == .list, model.creationIsNested {
      return model.creationTaskParentID.flatMap { model.task(withID: $0)?.title }
        .map { "In \($0)" } ?? "Nested"
    }
    return model.creationParentFolderID
      .flatMap { id in model.folders.first { $0.id == id }?.name }
      .map { "In \($0)" }
  }
}

/// The overlay's figures, named once. Layout constants rather than theme
/// tokens: the theme owns spacing, radius and type, not how wide a finder is.
enum WorkspaceOverlayMetrics {
  static let width: CGFloat = 600
  /// The height of a result list. Fixed so the panel does not jump as the
  /// matches narrow under the caret.
  static let listHeight: CGFloat = 340
  /// At most a frame or five. Reduce Motion takes it to nothing.
  static let fade: Double = 0.08
}

extension View {
  /// Claims the keys an overlay consumes. The host asks the model, the model
  /// asks this; see `WorkspaceViewModel.handleOverlayKey(_:)`.
  func overlayKeys(
    _ model: WorkspaceViewModel,
    id: String,
    handle: @escaping (String) -> Bool
  ) -> some View {
    onAppear {
      model.overlayKeyHandler = WorkspaceOverlayKeyHandler(overlayID: id, handle: handle)
    }
    .onDisappear {
      if model.overlayKeyHandler?.overlayID == id { model.overlayKeyHandler = nil }
    }
  }
}

// MARK: - Shared chrome

/// The row every overlay opens with: a glyph, a field that has the caret the
/// moment the overlay appears, and a micro-label saying what it is acting on.
struct WorkspaceOverlayField: View {
  @Environment(\.theme) private var theme
  let symbol: String
  let prompt: String
  @Binding var text: String
  var context: String?
  @FocusState private var isFocused: Bool

  var body: some View {
    HStack(spacing: theme.space.sm) {
      Image(systemName: symbol)
        .foregroundStyle(theme.muted)
      TextField(prompt, text: $text)
        .textFieldStyle(.plain)
        .font(theme.bodyFont())
        .foregroundStyle(theme.ink)
        .focused($isFocused)
      if let context {
        MicroLabel(context)
          .lineLimit(1)
          .truncationMode(.middle)
      }
    }
    .padding(.horizontal, theme.listGutter)
    .padding(.vertical, theme.space.sm)
    // A frame late: on the pass that inserts the field it has no window to
    // become first responder in.
    .onAppear { DispatchQueue.main.async { isFocused = true } }
  }
}

/// The strip along the foot of an overlay: which keys do what, and a count.
struct WorkspaceOverlayFooter: View {
  @Environment(\.theme) private var theme
  let hints: String
  var trailing: String?

  var body: some View {
    VStack(spacing: 0) {
      FocusRule()
      HStack(spacing: theme.space.md) {
        Text(hints)
        Spacer(minLength: theme.space.sm)
        if let trailing { Text(trailing) }
      }
      .font(theme.monoCaptionFont)
      .foregroundStyle(theme.dim)
      .padding(.horizontal, theme.listGutter)
      .padding(.vertical, theme.space.xs)
    }
  }
}

/// A quiet line in place of results.
struct WorkspaceOverlayHint: View {
  @Environment(\.theme) private var theme
  let text: String

  var body: some View {
    Text(text)
      .font(theme.bodyFont())
      .foregroundStyle(theme.muted)
      .frame(maxWidth: .infinity, maxHeight: .infinity)
  }
}

/// One result row's selection, flat to the panel's edges so the list reads as
/// a list rather than a stack of chips.
extension View {
  func overlayRow(isSelected: Bool) -> some View {
    modifier(WorkspaceOverlayRowStyle(isSelected: isSelected))
  }
}

private struct WorkspaceOverlayRowStyle: ViewModifier {
  @Environment(\.theme) private var theme
  let isSelected: Bool

  func body(content: Content) -> some View {
    content
      .padding(.horizontal, theme.listGutter)
      .padding(.vertical, theme.rowVerticalPadding)
      .frame(maxWidth: .infinity, alignment: .leading)
      .contentShape(Rectangle())
      .workspaceSelection(isSelected: isSelected, hasKeyboard: isSelected)
  }
}

// MARK: - Naming something new

/// A new list, folder or board column: one field, Return makes it.
struct WorkspaceNameOverlay: View {
  @Environment(WorkspaceViewModel.self) private var model
  let overlayID: String
  let title: String
  let prompt: String
  let context: String?
  var initialName = ""
  var submitHint = "↩ create · esc cancel"
  let onSubmit: (String) -> Void
  @State private var name = ""

  var body: some View {
    VStack(spacing: 0) {
      WorkspaceOverlayField(symbol: "plus", prompt: "\(title) — \(prompt.lowercased())", text: $name, context: context)
      WorkspaceOverlayFooter(hints: submitHint)
    }
    .onAppear { if name.isEmpty { name = initialName } }
    .overlayKeys(model, id: overlayID) { key in
      guard key == "enter" else { return false }
      submit()
      return true
    }
  }

  private func submit() {
    let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return }
    model.dismissOverlay(restoringFocus: false)
    onSubmit(trimmed)
  }
}

// MARK: - The list finder

/// Every list and nested list, filtered as you type. The workspace's "go to
/// file": ⌘P, then a few letters, then Return.
struct WorkspaceListNavigator: View {
  @Environment(WorkspaceViewModel.self) private var model
  @Environment(\.theme) private var theme
  let overlayID: String
  @State private var query = ""
  @State private var selection: Int?

  private struct Destination: Identifiable {
    let id: String
    let title: String
    let task: WorkspaceTask?
    /// Where it lives — a list's folder, a nested list's list — so two lists
    /// with one name can be told apart, and a folder's name finds its lists.
    let context: String?
  }

  /// Every list then every nested list, in sidebar order with an empty query;
  /// otherwise ranked by how well the letters match, the way the palette and
  /// Zed's file finder match — `wsr` finds "Write the spring report". The
  /// title counts for more than where it lives.
  private var destinations: [Destination] {
    let folderNames = Dictionary(model.folders.map { ($0.id, $0.name) }, uniquingKeysWith: { first, _ in first })
    let listNames = Dictionary(model.lists.map { ($0.id, $0.name) }, uniquingKeysWith: { first, _ in first })
    let lists = model.lists.map {
      Destination(id: $0.id, title: $0.name, task: nil, context: $0.folderId.flatMap { folderNames[$0] })
    }
    let nested = model.nestedLists.map {
      Destination(id: $0.task.id, title: $0.task.title, task: $0.task, context: listNames[$0.task.listId])
    }
    let all = lists + nested
    let query = query.trimmingCharacters(in: .whitespaces)
    // With nothing typed, the lists you were last in come first — the one
    // before this at the top, as Zed's file finder puts the last file — so
    // ⌘P Return goes back. Then everything else in sidebar order.
    guard !query.isEmpty else {
      let rank = Dictionary(
        model.recentListIDs.filter { $0 != currentID }.enumerated().map { ($1, $0) },
        uniquingKeysWith: { first, _ in first })
      let recent = all.filter { rank[$0.id] != nil }.sorted { rank[$0.id]! < rank[$1.id]! }
      return recent + all.filter { rank[$0.id] == nil }
    }
    let scored: [(Destination, Int)] = all.compactMap { destination in
      let title = WorkspaceCommandQuery.fuzzyScore(query, in: destination.title).map { $0 * 2 }
      let context = destination.context.flatMap { WorkspaceCommandQuery.fuzzyScore(query, in: $0) }
      guard let best = [title, context].compactMap({ $0 }).max() else { return nil }
      return (destination, best)
    }
    // Stable on ties, so equal matches keep sidebar order.
    return scored.enumerated()
      .sorted { $0.element.1 != $1.element.1 ? $0.element.1 > $1.element.1 : $0.offset < $1.offset }
      .map(\.element.0)
  }

  private var currentID: String? {
    model.scopeTask?.isList == true ? model.scopeTaskID : model.selectedListID
  }

  var body: some View {
    let items = destinations
    VStack(spacing: 0) {
      WorkspaceOverlayField(symbol: "list.bullet", prompt: "Go to a list", text: $query)
      FocusRule()
      if items.isEmpty {
        WorkspaceOverlayHint(text: "No list matches. ⌘↩ makes one.")
          .frame(height: WorkspaceOverlayMetrics.listHeight)
      } else {
        ScrollViewReader { proxy in
          ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
              ForEach(Array(items.enumerated()), id: \.element.id) { index, destination in
                HStack(spacing: theme.space.sm) {
                  Label(destination.title, systemImage: destination.task == nil ? "list.bullet" : "list.bullet.indent")
                    .foregroundStyle(theme.ink)
                    .lineLimit(1)
                  if let context = destination.context {
                    Text(context)
                      .foregroundStyle(theme.muted)
                      .lineLimit(1)
                      .truncationMode(.middle)
                  }
                  Spacer(minLength: theme.space.sm)
                  if destination.id == currentID {
                    Text("Current").font(theme.captionFont).foregroundStyle(theme.dim)
                  }
                }
                  .font(theme.bodyFont())
                  .overlayRow(isSelected: index == (selection ?? 0))
                  .id(destination.id)
                  .onTapGesture { open(destination) }
              }
            }
          }
          .frame(height: WorkspaceOverlayMetrics.listHeight)
          .onChange(of: selection) { _, index in
            if let index, items.indices.contains(index) { proxy.scrollTo(items[index].id) }
          }
        }
      }
      WorkspaceOverlayFooter(hints: "↑↓ choose · ↩ open · ⌘↩ new list · esc close", trailing: "\(items.count)")
    }
    .onChange(of: query) { _, _ in selection = nil }
    .overlayKeys(model, id: overlayID) { key in
      let items = destinations
      if let step = WorkspaceOverlayStep.offset(for: key) {
        selection = WorkspaceOverlayStep.index(from: selection ?? 0, by: step, count: items.count)
        return true
      }
      switch key {
      case "enter":
        let index = selection ?? 0
        if items.indices.contains(index) { open(items[index]) }
        return true
      case "cmd+enter":
        model.dismissOverlay(restoringFocus: false)
        model.requestListCreationForSelection()
        return true
      default:
        return false
      }
    }
  }

  private func open(_ destination: Destination) {
    model.dismissOverlay(restoringFocus: false)
    if let task = destination.task { model.selectNestedList(task) } else { model.selectList(destination.id) }
    model.requestKeyboardFocus(.tasks)
  }
}

// MARK: - Moving something

/// Where to move a task or list: every list, nested list and folder it could
/// go to, filtered as you type. It was a pop-up menu in a sheet, which is a
/// mouse control you had to Tab to.
///
/// Below the lists, a task being moved is also offered every open task the
/// query matches, from any list, to become the last subtask of. The lists
/// stay first, so typing a list's name and pressing Return still lands where
/// it always did.
struct WorkspaceMoveOverlay: View {
  @Environment(WorkspaceViewModel.self) private var model
  @Environment(\.theme) private var theme
  let overlayID: String
  let request: WorkspaceItemMoveRequest
  @State private var query = ""
  @State private var selection: Int?
  /// Read once when the overlay opens. It needs the outline beneath the task
  /// being moved, which is a store read, and a body is no place for one.
  @State private var all: [Destination] = []
  /// The task being moved and everything below it: no place it can go.
  @State private var excluded: Set<String> = []
  /// The open tasks the query matches, searched as you type.
  @State private var taskMatches: [Destination] = []

  private static let newListRowID = "new-list"

  struct Destination: Identifiable {
    let id: String
    let name: String
    let listID: String
    let parentTaskID: String?
    var folderID: String?
    /// The list a task destination lives in, drawn beside it; nil for a
    /// list, nested list or folder.
    var listName: String?
  }

  enum Row: Identifiable {
    case destination(Destination)
    case newList(String)

    var id: String {
      switch self {
      case .destination(let destination): destination.id
      case .newList: WorkspaceMoveOverlay.newListRowID
      }
    }
  }

  private var destinations: [Destination] {
    all.filter { query.isEmpty || $0.name.localizedCaseInsensitiveContains(query) }
  }

  /// The lists, then the new list row, then the tasks to move under.
  private var rows: [Row] {
    destinations.map(Row.destination) + (newListName.map { [Row.newList($0)] } ?? [])
      + taskMatches.map(Row.destination)
  }

  /// The list the new list row would create: what you typed, when no list is
  /// called that already. Moving somewhere that does not exist yet is the
  /// same gesture as moving somewhere that does, not a trip to the sidebar.
  private var newListName: String? {
    let name = query.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !name.isEmpty,
      !model.lists.contains(where: { $0.name.compare(name, options: .caseInsensitive) == .orderedSame })
    else { return nil }
    return name
  }

  var body: some View {
    let rows = rows
    VStack(spacing: 0) {
      WorkspaceOverlayField(
        symbol: "arrow.right.doc.on.clipboard", prompt: "Move to a list or under a task… or name a new list",
        text: $query, context: request.title)
      FocusRule()
      if rows.isEmpty {
        WorkspaceOverlayHint(text: "Nowhere matches.")
          .frame(height: WorkspaceOverlayMetrics.listHeight)
      } else {
        ScrollViewReader { proxy in
          ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
              ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                rowView(row, isSelected: index == (selection ?? 0))
                  .id(row.id)
                  .onTapGesture { choose(row) }
              }
            }
          }
          .frame(height: WorkspaceOverlayMetrics.listHeight)
          // Rows are keyed by destination, not position: keyed by position,
          // a filtered list kept drawing the rows it had before you typed.
          .onChange(of: selection) { _, index in
            guard let index, rows.indices.contains(index) else { return }
            proxy.scrollTo(rows[index].id)
          }
        }
      }
      WorkspaceOverlayFooter(
        hints: "↑↓ choose · ↩ move · esc cancel", trailing: "\(destinations.count + taskMatches.count)")
    }
    .onAppear {
      excluded = Self.excludedTaskIDs(for: request, in: model)
      all = Self.destinations(for: request, excluding: excluded, in: model)
    }
    .onChange(of: query) { _, text in
      selection = nil
      taskMatches = Self.taskDestinations(matching: text, for: request, excluding: excluded, in: model)
    }
    .overlayKeys(model, id: overlayID) { key in
      // `self.rows`, not the `rows` above: this handler is installed once,
      // when the picker opens, and that one is the rows before it had read
      // anywhere to go — so Return found nothing to choose.
      let rows = self.rows
      if let step = WorkspaceOverlayStep.offset(for: key) {
        selection = WorkspaceOverlayStep.index(from: selection ?? 0, by: step, count: rows.count)
        return true
      }
      guard key == "enter" else { return false }
      let index = selection ?? 0
      if rows.indices.contains(index) { choose(rows[index]) }
      return true
    }
  }

  @ViewBuilder
  private func rowView(_ row: Row, isSelected: Bool) -> some View {
    switch row {
    case .newList(let name):
      Label("New list “\(name)”", systemImage: "plus")
        .font(theme.bodyFont())
        .foregroundStyle(theme.primary)
        .lineLimit(1)
        .truncationMode(.middle)
        .overlayRow(isSelected: isSelected)
    case .destination(let destination):
      if let listName = destination.listName {
        HStack(alignment: isSelected ? .firstTextBaseline : .center, spacing: theme.space.sm) {
          Label(destination.name, systemImage: "arrow.turn.down.right")
            .font(theme.bodyFont())
            .foregroundStyle(theme.ink)
            .expandsWhenSelected(isSelected)
          Spacer(minLength: theme.space.md)
          MicroLabel(listName)
            .lineLimit(1)
            .fixedSize(horizontal: isSelected, vertical: false)
        }
        .overlayRow(isSelected: isSelected)
        .help("Move under \(destination.name), as its last subtask")
      } else {
        Label(destination.name, systemImage: destination.folderID == nil ? "list.bullet" : "folder")
          .font(theme.bodyFont())
          .foregroundStyle(theme.ink)
          .lineLimit(1)
          .truncationMode(.middle)
          .overlayRow(isSelected: isSelected)
      }
    }
  }

  private func choose(_ row: Row) {
    switch row {
    case .destination(let destination): move(to: destination)
    case .newList(let name): moveToNewList(named: name)
    }
  }

  private func moveToNewList(named name: String) {
    model.dismissOverlay()
    model.moveDroppedItem(request.payload, toNewListNamed: name)
  }

  private func move(to destination: Destination) {
    model.dismissOverlay()
    if let folderID = destination.folderID {
      model.moveDroppedItem(request.payload, toFolderID: folderID)
    } else {
      model.moveDroppedItem(request.payload, toListID: destination.listID, parentTaskID: destination.parentTaskID)
    }
  }

  /// The task being moved and its whole subtree, which it cannot go under.
  static func excludedTaskIDs(for request: WorkspaceItemMoveRequest, in model: WorkspaceViewModel) -> Set<String> {
    guard let taskID = request.taskID else { return [] }
    var excluded: Set<String> = [taskID]
    for item in (try? model.store?.outline(in: request.sourceListID, parentTaskId: taskID)) ?? [] {
      excluded.insert(item.id)
    }
    return excluded
  }

  static func destinations(
    for request: WorkspaceItemMoveRequest, excluding blocked: Set<String>, in model: WorkspaceViewModel
  ) -> [Destination] {
    let roots = model.lists.filter { request.taskID != nil || $0.id != request.sourceListID }
    let listDestinations = roots.flatMap { list in
      [Destination(id: "root:\(list.id)", name: list.name, listID: list.id, parentTaskID: nil)]
        + model.nestedLists.filter { $0.task.listId == list.id && !blocked.contains($0.id) }.map { item in
          Destination(id: item.id, name: "\(list.name) › \(item.task.title)", listID: list.id, parentTaskID: item.id)
        }
    }
    return listDestinations + model.folders.map { folder in
      Destination(id: "folder:\(folder.id)", name: "Folder: \(folder.name)", listID: "", parentTaskID: nil, folderID: folder.id)
    }
  }

  /// Open tasks matching `query`, from any list, to move a task under. Only
  /// for a task: a list moves into lists and folders. Nested lists are
  /// already among the lists above, so they are not offered twice.
  static func taskDestinations(
    matching query: String, for request: WorkspaceItemMoveRequest, excluding blocked: Set<String>,
    in model: WorkspaceViewModel
  ) -> [Destination] {
    guard request.taskID != nil, !query.trimmingCharacters(in: .whitespaces).isEmpty else { return [] }
    return model.searchResults(matching: query)
      .filter { !$0.task.isList && !blocked.contains($0.id) }
      .map { result in
        Destination(
          id: "task:\(result.id)", name: result.task.title, listID: result.task.listId,
          parentTaskID: result.task.id, listName: result.list.name)
      }
  }
}

// MARK: - The keyboard reference

/// The full keyboard reference, read off the same catalogue the keys are.
///
/// It used to be seventy rows typed out by hand next to a switch statement
/// that did the actual work, so the two could disagree and did. Nothing here
/// is written twice: a row exists because a command exists, and prints the
/// key that command is bound to. ⌘K reaches the same list and can run what it
/// lands on; this stays for "show me everything", and filters as you type.
struct WorkspaceKeyboardReference: View {
  @Environment(WorkspaceViewModel.self) private var model
  @Environment(\.theme) private var theme
  let overlayID: String
  @State private var query = ""
  /// The row ↑↓ walk and ↩ runs. A reference you could filter but not act on
  /// left you typing a name and then having to go and press the key it named.
  @State private var selection: WorkspaceCommandID?

  /// What the typed text does as a key, ahead of every title it appears in:
  /// `m` should answer "what does m do" before it lists "Move", "Mark"…
  private var keyMatches: [WorkspaceCommand] {
    WorkspaceCommandQuery.keyMatches(for: query)
  }

  private var groups: [(name: String, commands: [WorkspaceCommand])] {
    var order: [String] = []
    var byGroup: [String: [WorkspaceCommand]] = [:]
    let needle = query.trimmingCharacters(in: .whitespaces)
    let byKey = Set(keyMatches.map(\.id))
    for command in WorkspaceCommandCatalog.all
    where !byKey.contains(command.id)
      && (needle.isEmpty || command.title.localizedCaseInsensitiveContains(needle)
        || command.group.localizedCaseInsensitiveContains(needle)) {
      if byGroup[command.group] == nil { order.append(command.group) }
      byGroup[command.group, default: []].append(command)
    }
    return order.map { ($0, byGroup[$0] ?? []) }
  }

  /// Every row in the order drawn, less the motions: as in the palette, ↑↓
  /// skip rows that ↩ would do nothing to.
  private var runnable: [WorkspaceCommandID] {
    (keyMatches + groups.flatMap(\.commands)).filter { $0.kind == .action }.map(\.id)
  }

  var body: some View {
    VStack(spacing: 0) {
      WorkspaceOverlayField(symbol: "keyboard", prompt: "Filter the keyboard reference", text: $query)
      FocusRule()
      ScrollViewReader { proxy in
        ScrollView {
          LazyVStack(alignment: .leading, spacing: theme.space.md) {
            let keyed = keyMatches
            if !keyed.isEmpty {
              VStack(alignment: .leading, spacing: theme.space.xs) {
                MicroLabel("Pressing \(query.trimmingCharacters(in: .whitespaces))")
                ForEach(keyed) { command in
                  row(command)
                }
              }
            }
            ForEach(groups, id: \.name) { group in
              VStack(alignment: .leading, spacing: theme.space.xs) {
                MicroLabel(group.name)
                ForEach(group.commands) { command in
                  row(command)
                }
              }
            }
          }
          .padding(theme.space.md)
        }
        .frame(height: WorkspaceOverlayMetrics.listHeight)
        .onChange(of: selection) { _, id in
          guard let id else { return }
          proxy.scrollTo(id, anchor: .center)
        }
      }
      WorkspaceOverlayFooter(hints: "↑↓ choose · ↩ run · esc close")
    }
    .onAppear { selection = runnable.first }
    .onChange(of: query) { _, _ in selection = runnable.first }
    .overlayKeys(model, id: overlayID) { key in
      if let step = WorkspaceOverlayStep.offset(for: key) {
        move(by: step)
        return true
      }
      guard key == "enter" else { return false }
      if let id = selection, let command = WorkspaceCommandCatalog.byID[id] {
        run(command)
      } else {
        model.dismissOverlay()
      }
      return true
    }
  }

  private func row(_ command: WorkspaceCommand) -> some View {
    HStack(alignment: .firstTextBaseline, spacing: theme.space.md) {
      VStack(alignment: .leading, spacing: theme.space.xxs) {
        Text(command.title)
          .font(theme.bodyFont())
          .foregroundStyle(command.kind == .motion ? theme.muted : theme.ink)
        if let note = command.note {
          Text(note).font(theme.captionFont).foregroundStyle(theme.dim)
        }
      }
      Spacer(minLength: theme.space.md)
      if command.surface != .anywhere {
        MicroLabel(command.surface.title)
      }
      KeyCapRow(keys: command.displayKeys)
    }
    .overlayRow(isSelected: selection == command.id)
    .id(command.id)
    .contentShape(Rectangle())
    .onTapGesture { run(command) }
  }

  private func move(by offset: Int) {
    let ids = runnable
    guard !ids.isEmpty else { return }
    guard let current = selection, let index = ids.firstIndex(of: current) else {
      selection = ids.first
      return
    }
    selection = ids[min(max(index + offset, 0), ids.count - 1)]
  }

  /// Closed first, for the palette's reason: what runs may open an overlay
  /// or move the caret, and this one still being up would undo that.
  private func run(_ command: WorkspaceCommand) {
    guard command.kind == .action else { return }
    model.dismissOverlay()
    model.run(command.id)
  }
}
