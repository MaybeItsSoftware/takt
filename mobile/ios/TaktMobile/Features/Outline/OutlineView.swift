import TaktWorkspace
import SwiftUI

/// A list scope as a Checkvist-style outline: folding in place, swipe to
/// complete, a long-press menu with every task command, drag to reorder, and
/// an inline composer for new tasks and subtasks.
struct OutlineView: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.isPadLayout) private var isPad
  @State private var outline: OutlineModel
  /// A revealed row to scroll to once the outline has drawn it.
  @State private var pendingScrollID: String?
  @FocusState private var focusedField: Field?

  enum Field: Hashable { case composer, rename }

  init(scope: ListScope) {
    _outline = State(initialValue: OutlineModel(scope: scope))
  }

  var body: some View {
    @Bindable var navigation = model.navigation
    ScrollViewReader { proxy in
      // On the iPad a tap selects and the inspector column follows; on the
      // iPhone a tap opens the inspector (each row's own tap, below).
      List(selection: isPad ? $navigation.selectedTaskID : .constant(nil)) {
        if outline.composer?.anchorRowID == nil, outline.composer != nil {
          composerRow
        }
        let entries = OutlineEntry.entries(
          rows: outline.rows, blocked: model.blockedTaskIDs, buried: Set(model.buriedStarts.keys))
        ForEach(entries) { entry in
          switch entry {
          case .row(let row):
            rowView(row)
            if outline.composer?.anchorRowID == row.id {
              composerRow
            }
          case .blockedLabel(_, let depth, let isBuried):
            blockedLabelRow(depth: depth, isBuried: isBuried)
          }
        }
        .onMove { source, destination in
          guard let move = OutlineEntry.rowMove(in: entries, source: source, destination: destination) else { return }
          outline.move(from: move.source, to: move.destination, model: model)
        }
        .moveDisabled(!outline.scope.isSingleTree)
      }
      .listStyle(.plain)
      .scrollContentBackground(.hidden)
      .environment(\.defaultMinListRowHeight, theme.touchTarget)
      .overlay {
        if outline.isLoaded && outline.rows.isEmpty && outline.composer == nil {
          VStack(spacing: theme.space.md) {
            EmptyState(
              title: outline.scope.isSingleTree ? "Nothing here yet" : "Nothing to do",
              message: outline.scope.isSingleTree
                ? "Add a task to start the list."
                : outline.scope.filterListID != nil
                  ? "No task matches this filter list." : "Every open task in these lists shows here.",
              systemImage: "list.bullet")
            if outline.scope.isSingleTree {
              Button("Add a task") { startComposing() }
                .buttonStyle(ThemedButtonStyle(kind: .primary))
                .accessibilityIdentifier("outline.addFirst")
            }
          }
        }
      }
      .task(id: QueryKey(revision: model.revision, scope: OutlineScopeKey(scope: outline.scope, token: outline.reloadToken))) {
        await outline.load(model)
      }
      .onChange(of: model.navigation.outlineCommand) { _, command in
        guard let command else { return }
        model.navigation.outlineCommand = nil
        handle(command)
      }
      .onChange(of: model.navigation.revealRequest, initial: true) { _, request in
        guard let request, request.scope == outline.scope else { return }
        model.navigation.revealRequest = nil
        outline.apply(request)
        pendingScrollID = request.taskID
        scrollToPending(proxy)
      }
      .onChange(of: outline.rows) { _, _ in scrollToPending(proxy) }
      .accessibilityIdentifier("outline.list")
    }
    .toolbar {
      ToolbarItemGroup(placement: .secondaryAction) {
        Toggle(isOn: $outline.hidesCompleted) { Label("Hide completed", systemImage: "eye.slash") }
        if outline.scope.isSingleTree {
          Button { outline.setAllFolded(true) } label: { Label("Fold all", systemImage: "chevron.right.2") }
          Button { outline.setAllFolded(false) } label: { Label("Unfold all", systemImage: "chevron.down.2") }
        }
      }
    }
  }

  // MARK: - Rows

  @ViewBuilder
  private func rowView(_ row: OutlineRow) -> some View {
    let isSelected = model.navigation.selectedTaskID == row.id
    Group {
      if outline.editingTaskID == row.id {
        renameRow(row)
      } else {
        OutlineRowView(
          row: row, isSelected: isSelected, isBlocked: model.isBlocked(row.id, status: row.status),
          starts: model.startsLabel(row.id, status: row.status),
          waiting: model.waitingBadge(row.id, status: row.status)
        ) {
          outline.toggleFold(row.id)
        }
          .equatable()
      }
    }
    .tag(row.id)
    .contentShape(Rectangle())
    // Below the row's own buttons (the fold, the check), and switched off on
    // the iPad, where the list's selection does the work.
    .gesture(
      TapGesture().onEnded {
        if outline.editingTaskID != row.id { model.navigation.inspect(row.id, isPad: isPad) }
      },
      including: isPad ? .subviews : .all)
    .listRowInsets(EdgeInsets(top: 0, leading: theme.mac(4), bottom: 0, trailing: theme.mac(4)))
    // The selection is painted by the row, a box round its title as the
    // Mac's is; the list's own row stays the paper.
    .listRowBackground(theme.paper)
    .listRowSeparator(.hidden)
    .swipeActions(edge: .leading, allowsFullSwipe: true) {
      Button { model.completeCelebrating(row.id) } label: {
        Label(row.status == .open ? "Complete" : "Reopen", systemImage: row.status == .open ? "checkmark" : "arrow.uturn.backward")
      }
      .tint(theme.success)
    }
    // The same two swipes as Today and as Android (docs/mobile-ux.md): right
    // completes, left puts it on today. Cancel and delete are in the menu.
    .swipeActions(edge: .trailing, allowsFullSwipe: true) {
      if row.status == .open {
        Button { model.togglePlannedToday(row.id) } label: {
          Label(row.isPlanned ? "Take off today" : "Today", systemImage: "sun.max")
        }
        .tint(theme.primary)
      }
    }
    .contextMenu {
      TaskContextMenu(
        context: menuContext(row),
        onNewChild: outline.scope.isSingleTree ? { compose { outline.composeChild(of: row) } } : nil,
        onNewAbove: outline.scope.isSingleTree ? { compose { outline.composeAbove(row) } } : nil,
        onRename: { beginRenaming(row) })
    }
    .accessibilityAction(named: row.status == .open ? "Complete" : "Reopen") { model.toggleComplete(row.id) }
    .accessibilityAction(named: row.isFolded ? "Unfold" : "Fold") { outline.toggleFold(row.id) }
  }

  private func menuContext(_ row: OutlineRow) -> TaskMenuContext {
    var context = row.menuContext
    context.allowsStructure = outline.scope.isSingleTree
    return context
  }

  /// The label over a sibling group's blocked tail, its text in the title
  /// column of the rows it heads: past their indent, disclosure and mark.
  /// A row of its own that cannot be selected, dragged or swiped.
  private func blockedLabelRow(depth: Int, isBuried: Bool = false) -> some View {
    BlockedDivider(isBuried: isBuried)
      .padding(.leading, OutlineRowView.titleInset(depth: depth, theme: theme))
      .padding(.top, theme.space.sm)
      .padding(.bottom, theme.space.xxs)
      .listRowInsets(EdgeInsets(top: 0, leading: theme.mac(4), bottom: 0, trailing: theme.mac(4)))
      .listRowBackground(theme.paper)
      .listRowSeparator(.hidden)
      .selectionDisabled()
      .moveDisabled(true)
      .deleteDisabled(true)
  }

  private var composerRow: some View {
    let depth = outline.composer?.depth ?? 0
    return HStack(spacing: theme.mac(8)) {
      TaskCheckbox(status: .open)
      TextField("New task", text: Binding(
        get: { outline.composer?.text ?? "" },
        set: { outline.composer?.text = $0 }))
        .font(theme.type.body)
        .foregroundStyle(theme.ink)
        .focused($focusedField, equals: .composer)
        .submitLabel(.next)
        .onSubmit {
          if outline.commitComposer(model) != nil {
            focusedField = .composer
          } else {
            outline.composer = nil
          }
        }
        .accessibilityIdentifier("outline.composer")
      Button {
        outline.composer = nil
        focusedField = nil
      } label: {
        Image(systemName: "xmark.circle.fill").foregroundStyle(theme.dim)
      }
      .buttonStyle(.plain)
      .accessibilityLabel("Cancel new task")
    }
    .padding(.leading, OutlineRowView.markInset(depth: depth, theme: theme))
    .padding(.trailing, theme.mac(12))
    .frame(minHeight: theme.mac.row)
    .background(
      RoundedRectangle(cornerRadius: theme.mac.radiusRow).strokeBorder(theme.focus, lineWidth: 1)
        .padding(.leading, OutlineRowView.markInset(depth: depth, theme: theme) + theme.mac.mark + theme.mac(2)))
    .frame(minHeight: theme.touchTarget)
    .listRowInsets(EdgeInsets(top: 0, leading: theme.mac(4), bottom: 0, trailing: theme.mac(4)))
    .listRowBackground(theme.paper)
    .listRowSeparator(.hidden)
    .moveDisabled(true)
    .selectionDisabled()
  }

  private func renameRow(_ row: OutlineRow) -> some View {
    HStack(spacing: theme.mac(8)) {
      TaskCheckbox(status: row.status, isList: row.isList)
      TextField("Title", text: $outline.editingText)
        .font(theme.type.body)
        .focused($focusedField, equals: .rename)
        .submitLabel(.done)
        .onSubmit { outline.commitRename(model) }
        .accessibilityIdentifier("outline.rename")
        .padding(.horizontal, theme.mac(6))
        .frame(minHeight: theme.mac.row)
        .background(RoundedRectangle(cornerRadius: theme.mac.radiusRow).fill(theme.selected))
        .overlay(RoundedRectangle(cornerRadius: theme.mac.radiusRow).strokeBorder(theme.focus, lineWidth: 1))
    }
    .padding(.leading, OutlineRowView.markInset(depth: row.depth, theme: theme))
    .padding(.trailing, theme.mac(12))
    .frame(minHeight: theme.touchTarget)
  }

  // MARK: - Commands

  private func compose(_ open: () -> Void) {
    open()
    focusedField = .composer
  }

  /// The toolbar's add button: a sibling below the selection, or a new task
  /// at the end.
  func startComposing() {
    let selected = outline.rows.first { $0.id == model.navigation.selectedTaskID }
    compose { outline.composeSibling(after: selected) }
  }

  private func beginRenaming(_ row: OutlineRow) {
    outline.beginRenaming(row)
    focusedField = .rename
  }

  private func handle(_ command: OutlineCommand) {
    let selected = outline.rows.first { $0.id == model.navigation.selectedTaskID }
    switch command {
    case .add: startComposing()
    case .addChild: if let selected { compose { outline.composeChild(of: selected) } }
    case .addAbove: if let selected { compose { outline.composeAbove(selected) } }
    case .rename: if let selected { beginRenaming(selected) }
    case .toggleFold: if let selected, selected.hasChildren { outline.toggleFold(selected.id) }
    case .expand: if let selected, selected.isFolded { outline.toggleFold(selected.id) }
    case .collapse:
      guard let selected else { return }
      if selected.hasChildren, !selected.isFolded {
        outline.toggleFold(selected.id)
      } else if let parent = selected.parentID, outline.rows.contains(where: { $0.id == parent }) {
        model.navigation.selectedTaskID = parent
      }
    case .foldAll: outline.setAllFolded(true)
    case .unfoldAll: outline.setAllFolded(false)
    case .toggleHideCompleted: outline.hidesCompleted.toggle()
    case .selectNext: select(offset: 1)
    case .selectPrevious: select(offset: -1)
    }
  }

  /// Scrolls to the revealed row once it is among the rows drawn.
  private func scrollToPending(_ proxy: ScrollViewProxy) {
    guard let id = pendingScrollID, outline.rows.contains(where: { $0.id == id }) else { return }
    pendingScrollID = nil
    Task { @MainActor in
      withAnimation { proxy.scrollTo(id, anchor: .center) }
    }
  }

  private func select(offset: Int) {
    let rows = outline.rows
    guard !rows.isEmpty else { return }
    guard let current = rows.firstIndex(where: { $0.id == model.navigation.selectedTaskID }) else {
      model.navigation.selectedTaskID = offset > 0 ? rows.first?.id : rows.last?.id
      return
    }
    let next = min(max(0, current + offset), rows.count - 1)
    model.navigation.selectedTaskID = rows[next].id
  }
}

/// A command for whichever outline is on screen, from the toolbar or a
/// hardware key.
enum OutlineCommand: Equatable {
  case add, addChild, addAbove, rename, toggleFold, expand, collapse, foldAll, unfoldAll, toggleHideCompleted
  case selectNext, selectPrevious
}

private struct OutlineScopeKey: Hashable {
  let scope: ListScope
  let token: Int
}

/// One outline row. Equatable on its value, so a list of five thousand rows
/// redraws only the rows whose content changed.
struct OutlineRowView: View, Equatable {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.isPadLayout) private var isPad
  let row: OutlineRow
  let isSelected: Bool
  /// Open and still waiting on an open task: the whole row steps back.
  var isBlocked = false
  /// Open with its start still to come ("starts Mon"): the row steps back
  /// too, and says when it comes back.
  var starts: String?
  /// Filed in Waiting on: who it waits on and when it is chased.
  var waiting: WaitingBadge?
  let onFold: () -> Void

  static func == (lhs: OutlineRowView, rhs: OutlineRowView) -> Bool {
    lhs.row == rhs.row && lhs.isSelected == rhs.isSelected && lhs.isBlocked == rhs.isBlocked
      && lhs.starts == rhs.starts && lhs.waiting == rhs.waiting
  }

  /// Where the status mark starts: past the indent, the fold arrow and the
  /// gap after it. The Mac's `2 + depth × 18`, then 10, then 4.
  static func markInset(depth: Int, theme: Theme) -> CGFloat {
    theme.mac(2) + CGFloat(depth) * theme.mac.indent + theme.mac.disclosure + theme.mac(4)
  }

  /// Where the title starts, inside its selection box.
  static func titleInset(depth: Int, theme: Theme) -> CGFloat {
    markInset(depth: depth, theme: theme) + theme.mac.mark + theme.mac(6)
  }

  /// The Mac's outline row (`views::outline_row`): the indent, a 10 fold
  /// arrow in dim text, 4, the 12 status mark, then the title and its chips
  /// in a box at radius 4 that is the selection. The painted row is 26 Mac
  /// pixels; the list row round it is the touch target.
  var body: some View {
    HStack(spacing: 0) {
      Color.clear.frame(width: theme.mac(2) + CGFloat(row.depth) * theme.mac.indent, height: 1)
      disclosure
      Color.clear.frame(width: theme.mac(4), height: 1)
      Button {
        model.completeCelebrating(row.id)
      } label: {
        TaskCheckbox(status: row.status, isList: row.isList)
          .celebrationIcon(row.id)
          .hitTarget()
      }
      .buttonStyle(.plain)
      .accessibilityLabel(row.status == .open ? "Complete \(row.title)" : "Reopen \(row.title)")
      .accessibilityIdentifier("outline.check.\(row.title)")
      HStack(spacing: theme.mac(8)) {
        Text(row.title)
          .font(theme.type.body)
          .foregroundStyle(titleColor)
          .strikethrough(row.status != .open, color: theme.dim)
          .lineLimit(1)
          .truncationMode(.tail)
          .celebrationStrike(row.id)
          .frame(maxWidth: .infinity, alignment: .leading)
        TaskChips(
          status: row.status, isBlocked: isBlocked, isLater: starts != nil, waitingOn: waiting?.waitingOn,
          metadata: row.chips, dueAt: row.dueAt, childrenDone: row.childrenDone, childrenTotal: row.childrenTotal,
          hasNotes: row.hasNotes)
        if let listName = row.listName {
          Chip(text: listName, tint: theme.dim)
        }
        if isSelected {
          Button {
            model.navigation.inspect(row.id, isPad: isPad)
          } label: {
            Image(systemName: "info.circle")
              .font(theme.type.glyph(13))
              .foregroundStyle(theme.icon)
              .hitTarget()
          }
          .buttonStyle(.plain)
          .accessibilityLabel("Details")
          .accessibilityIdentifier("outline.details")
        }
      }
      .padding(.leading, theme.mac(6))
      .padding(.trailing, theme.mac(12))
      .frame(height: theme.mac.row)
      .rowSelection(isSelected, focused: isPad)
    }
    .celebrationRow(row.id)
  }

  /// Done or cancelled, struck through in dim; blocked or buried, dim.
  private var titleColor: Color {
    if row.status != .open { return theme.dim }
    if isBlocked || starts != nil { return theme.dim }
    return theme.ink
  }

  @ViewBuilder
  private var disclosure: some View {
    if row.hasChildren {
      Button {
        withAnimation(.snappy(duration: 0.2)) { onFold() }
      } label: {
        Text(row.isFolded ? "▸" : "▾")
          .font(theme.type.groupLabel.weight(.regular))
          .foregroundStyle(theme.dim)
          .frame(width: theme.mac.disclosure)
          .hitTarget()
      }
      .buttonStyle(.plain)
      .accessibilityLabel(row.isFolded ? "Unfold \(row.title)" : "Fold \(row.title)")
      .accessibilityIdentifier("outline.fold.\(row.title)")
    } else {
      Color.clear.frame(width: theme.mac.disclosure, height: 1)
    }
  }
}
