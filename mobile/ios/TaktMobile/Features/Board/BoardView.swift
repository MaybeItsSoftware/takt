import TaktWorkspace
import SwiftUI

/// The scope's kanban board: paging columns of cards, each card carrying its
/// foldable subtasks. Cards move by drag or by menu; columns can be added,
/// renamed, reordered and removed.
struct BoardView: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.isPadLayout) private var isPad
  @State private var board: BoardModel
  @State private var columnPrompt: ColumnPrompt?
  @State private var promptText = ""
  @State private var pendingRemoval: WorkspaceKanbanColumn?
  @FocusState private var composerFocused: Bool

  enum ColumnPrompt: Identifiable, Equatable {
    case add
    case rename(WorkspaceKanbanColumn)
    var id: String {
      switch self {
      case .add: "add"
      case .rename(let column): "rename-\(column.id)"
      }
    }
  }

  #if DEBUG
  @State private var debugColumnID: String?
  #endif

  init(scope: ListScope) {
    _board = State(initialValue: BoardModel(scope: scope))
  }

  var body: some View {
    GeometryReader { proxy in
      let width = columnWidth(for: proxy.size.width)
      ScrollView(.horizontal) {
        LazyHStack(alignment: .top, spacing: theme.mac(8)) {
          ForEach(board.columns) { column in
            BoardColumnView(
              column: column, cards: board.cards(in: column.id), board: board, width: width,
              composerFocused: $composerFocused,
              onRename: { promptText = column.title; columnPrompt = .rename(column) },
              onRemove: { pendingRemoval = column })
          }
          addColumnButton
            .frame(width: min(width, 200))
        }
        .scrollTargetLayout()
        .padding(.horizontal, theme.mac(8))
        .padding(.vertical, theme.mac(8))
      }
      .scrollTargetBehavior(.viewAligned)
      .scrollIndicators(.hidden)
      #if DEBUG
      // `-boardColumn <id>` scrolls to a column, for screenshots.
      .scrollPosition(id: $debugColumnID, anchor: .leading)
      .onChange(of: board.isLoaded) {
        debugColumnID = UserDefaults.standard.string(forKey: "boardColumn")
      }
      #endif
    }
    .background(theme.paper)
    .task(id: QueryKey(revision: model.revision, scope: board.scope)) {
      await board.load(model)
    }
    .alert(columnPromptTitle, isPresented: promptBinding, presenting: columnPrompt) { prompt in
      TextField("Column name", text: $promptText)
        .accessibilityIdentifier("board.columnName")
      Button("Cancel", role: .cancel) {}
      Button("Save") {
        switch prompt {
        case .add: board.addColumn(named: promptText, model: model)
        case .rename(let column): board.renameColumn(column.id, to: promptText, model: model)
        }
      }
    }
    .confirmationDialog(
      pendingRemoval.map { "Remove \($0.title)?" } ?? "", isPresented: removalBinding, titleVisibility: .visible,
      presenting: pendingRemoval
    ) { column in
      Button("Remove column", role: .destructive) { board.removeColumn(column.id, model: model) }
    } message: { _ in
      Text("Its cards move to the first remaining column. You can undo it.")
    }
    .accessibilityIdentifier("board")
  }

  /// One column fills an iPhone (less a peek of the next); an iPad shows
  /// several side by side.
  private func columnWidth(for width: CGFloat) -> CGFloat {
    let available = width - theme.mac(8) * 2
    if available < 500 { return max(240, available - 28) }
    // The Mac's 272, scaled.
    return theme.mac(272)
  }

  private var addColumnButton: some View {
    Button {
      promptText = ""
      columnPrompt = .add
    } label: {
      Text("+ Add column")
        .font(theme.type.body)
        .foregroundStyle(theme.muted)
        .frame(maxWidth: .infinity, minHeight: theme.mac(30))
        .background(RoundedRectangle(cornerRadius: theme.mac.radiusMenu).fill(theme.altRow))
        .overlay(RoundedRectangle(cornerRadius: theme.mac.radiusMenu).strokeBorder(theme.borderMuted, lineWidth: 1))
        .hitTarget()
    }
    .buttonStyle(.plain)
    .accessibilityIdentifier("board.addColumn")
  }

  private var columnPromptTitle: String {
    if case .rename = columnPrompt { return "Rename column" }
    return "New column"
  }

  private var promptBinding: Binding<Bool> {
    Binding(get: { columnPrompt != nil }, set: { if !$0 { columnPrompt = nil } })
  }

  private var removalBinding: Binding<Bool> {
    Binding(get: { pendingRemoval != nil }, set: { if !$0 { pendingRemoval = nil } })
  }
}

/// One column: its header, its cards, and an inline add field.
struct BoardColumnView: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  let column: WorkspaceKanbanColumn
  let cards: [BoardCard]
  let board: BoardModel
  let width: CGFloat
  var composerFocused: FocusState<Bool>.Binding
  let onRename: () -> Void
  let onRemove: () -> Void
  @State private var isTargeted = false

  var body: some View {
    VStack(spacing: 0) {
      header
      Hairline(role: .borderMuted)
      ScrollView(.vertical) {
        let blockedStart = BlockedSections.boardLabel(cards: cards, blocked: model.blockedTaskIDs)
        LazyVStack(spacing: theme.mac(4)) {
          ForEach(cards) { card in
            // The label over the column's blocked cards, which sink to its
            // foot, rides with the first of them.
            VStack(spacing: theme.mac(4)) {
              if card.id == blockedStart {
                BlockedDivider()
                  .padding(.top, theme.space.xs)
                  .padding(.horizontal, theme.space.xxs)
              }
              BoardCardView(card: card, board: board, column: column)
            }
          }
          composer
        }
        .padding(theme.mac(4))
      }
      .scrollIndicators(.hidden)
    }
    .frame(width: width)
    .frame(maxHeight: .infinity, alignment: .top)
    .background(isTargeted ? theme.hover : theme.altRow)
    .clipShape(RoundedRectangle(cornerRadius: theme.mac.radiusMenu, style: .continuous))
    .overlay(
      RoundedRectangle(cornerRadius: theme.mac.radiusMenu, style: .continuous)
        .strokeBorder(isTargeted ? theme.focus : theme.borderMuted, lineWidth: 1))
    .dropDestination(for: String.self) { ids, _ in
      guard let id = ids.first, board.snapshot.allCards.contains(where: { $0.id == id }) else { return false }
      board.move(id, toColumn: column.id, model: model)
      return true
    } isTargeted: { isTargeted = $0 }
    .accessibilityIdentifier("board.column.\(column.title)")
  }

  private var header: some View {
    HStack(spacing: theme.mac(8)) {
      Text(column.rules != nil ? "\(column.title) ⚙" : column.title)
        .font(theme.type.body)
        .foregroundStyle(theme.muted)
        .lineLimit(1)
      Spacer(minLength: 0)
      Text("\(cards.count)")
        .font(theme.type.secondary)
        .foregroundStyle(theme.dim)
      Button {
        board.composerText = ""
        board.composingColumnID = column.id
        composerFocused.wrappedValue = true
      } label: {
        Image(systemName: "plus").font(theme.type.glyph(12)).frame(width: 24, height: theme.mac(24)).hitTarget()
      }
      .buttonStyle(.plain)
      .foregroundStyle(theme.icon)
      .accessibilityLabel("Add a card to \(column.title)")
      .accessibilityIdentifier("board.add.\(column.title)")
      Menu {
        Button { onRename() } label: { Label("Rename", systemImage: "pencil") }
        Button { board.moveColumn(column.id, by: -1, model: model) } label: { Label("Move left", systemImage: "arrow.left") }
          .disabled(board.columns.first?.id == column.id)
        Button { board.moveColumn(column.id, by: 1, model: model) } label: { Label("Move right", systemImage: "arrow.right") }
          .disabled(board.columns.last?.id == column.id)
        Divider()
        Button(role: .destructive) { onRemove() } label: { Label("Remove column…", systemImage: "trash") }
          .disabled(board.columns.count <= 1)
      } label: {
        Image(systemName: "ellipsis").font(theme.type.glyph(12)).frame(width: 24, height: theme.mac(24)).hitTarget()
      }
      .foregroundStyle(theme.icon)
      .accessibilityLabel("\(column.title) options")
    }
    .padding(.leading, theme.mac(8))
    .padding(.trailing, theme.mac(4))
    .frame(minHeight: theme.mac(30))
  }

  @ViewBuilder
  private var composer: some View {
    if board.composingColumnID == column.id {
      HStack(spacing: theme.space.sm) {
        TextField("New card", text: Binding(get: { board.composerText }, set: { board.composerText = $0 }))
          .font(theme.type.body)
          .focused(composerFocused)
          .submitLabel(.next)
          .onSubmit {
            if board.addCard(board.composerText, toColumn: column.id, model: model) != nil {
              board.composerText = ""
              composerFocused.wrappedValue = true
            } else {
              board.composingColumnID = nil
            }
          }
          .accessibilityIdentifier("board.composer")
        Button {
          board.composingColumnID = nil
        } label: {
          Image(systemName: "xmark.circle.fill").foregroundStyle(theme.dim)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Cancel new card")
      }
      .padding(.horizontal, theme.mac(8))
      .padding(.vertical, theme.mac(4))
      .background(RoundedRectangle(cornerRadius: theme.mac.radiusRow).fill(theme.paper))
      .overlay(RoundedRectangle(cornerRadius: theme.mac.radiusRow).strokeBorder(theme.focus, lineWidth: 1))
    }
  }
}

/// One card. Equatable on its value so an unchanged card is not redrawn.
struct BoardCardView: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.isPadLayout) private var isPad
  let card: BoardCard
  let board: BoardModel
  let column: WorkspaceKanbanColumn
  @State private var isTargeted = false

  private var isSelected: Bool { model.navigation.selectedTaskID == card.id }
  private var isBlocked: Bool { model.isBlocked(card.id, status: card.status) }

  /// The Mac's card (`views::card`): the status mark, the title and its
  /// chips; under them the list and the due day in dim secondary text; then
  /// the subtasks as compact `└` rows. Paper at radius 4 in the muted border,
  /// 8 by 4 inside; selected, the selection fill in the focus colour.
  var body: some View {
    VStack(alignment: .leading, spacing: theme.mac(2)) {
      HStack(spacing: theme.mac(8)) {
        Button { model.completeCelebrating(card.id) } label: {
          TaskCheckbox(status: card.status, isList: card.isList)
            .celebrationIcon(card.id)
            .hitTarget()
        }
        .buttonStyle(.plain)
        .accessibilityLabel(card.status == .open ? "Complete \(card.title)" : "Reopen \(card.title)")
        Text(card.title)
          .font(theme.type.body)
          .foregroundStyle(card.status != .open || isBlocked ? theme.dim : theme.ink)
          .strikethrough(card.status != .open, color: theme.dim)
          .lineLimit(2)
          .celebrationStrike(card.id)
          .frame(maxWidth: .infinity, alignment: .leading)
        TaskChips(
          status: card.status, isBlocked: isBlocked, waitingOn: model.waitingBadge(card.id, status: card.status)?.waitingOn)
        if !card.subtasks.isEmpty {
          foldButton(card.id, title: card.title, folded: board.folded.contains(card.id))
        }
      }
      .frame(minHeight: theme.mac(20))
      secondLine
      if let parentTitle = card.parentTitle {
        Text(parentTitle)
          .font(theme.type.secondary).foregroundStyle(theme.dim).lineLimit(1)
          .padding(.leading, theme.mac.mark + theme.mac(8))
      }
      subtasks
    }
    .padding(.horizontal, theme.mac(8))
    .padding(.vertical, theme.mac(4))
    .celebrationRow(card.id)
    .background(
      RoundedRectangle(cornerRadius: theme.mac.radiusRow, style: .continuous)
        .fill(isSelected ? theme.selected : theme.paper))
    .overlay(
      RoundedRectangle(cornerRadius: theme.mac.radiusRow, style: .continuous)
        .strokeBorder(isSelected || isTargeted ? theme.focus : theme.borderMuted, lineWidth: 1))
    .contentShape(RoundedRectangle(cornerRadius: theme.mac.radiusRow))
    .onTapGesture { model.navigation.inspect(card.id, isPad: isPad) }
    .draggable(card.id) {
      Text(card.title).font(theme.type.body).padding(theme.space.sm).background(theme.raised)
    }
    .dropDestination(for: String.self) { ids, _ in
      guard let id = ids.first, id != card.id, board.snapshot.allCards.contains(where: { $0.id == id }) else {
        return false
      }
      board.move(id, before: card.id, model: model)
      return true
    } isTargeted: { isTargeted = $0 }
    .contextMenu {
      Menu {
        ForEach(board.columns) { other in
          Button(other.title) { board.move(card.id, toColumn: other.id, model: model) }
            .disabled(other.id == column.id)
        }
      } label: { Label("Move to column", systemImage: "rectangle.split.3x1") }
      Button { board.moveToAdjacentColumn(card.id, by: -1, model: model) } label: {
        Label("Move left", systemImage: "arrow.left")
      }
      .disabled(board.columns.first?.id == column.id)
      Button { board.moveToAdjacentColumn(card.id, by: 1, model: model) } label: {
        Label("Move right", systemImage: "arrow.right")
      }
      .disabled(board.columns.last?.id == column.id)
      TaskContextMenu(context: card.menuContext)
    }
    .accessibilityIdentifier("board.card.\(card.title)")
  }

  /// The list and the due day, flush with the title.
  @ViewBuilder
  private var secondLine: some View {
    let estimate = card.estimateSeconds ?? 0
    if card.listName != nil || card.dueAt != nil || estimate > 0 {
      HStack(spacing: theme.mac(8)) {
        if let listName = card.listName {
          Text(listName).lineLimit(1)
        }
        if let due = card.dueAt {
          Text(Format.day(due))
            .foregroundStyle(card.status == .open && Format.isOverdue(due) ? theme.danger : theme.dim)
        }
        if estimate > 0 {
          Text(Format.duration(estimate)).font(theme.type.secondaryMono)
        }
      }
      .font(theme.type.secondary)
      .foregroundStyle(theme.dim)
      .padding(.leading, theme.mac.mark + theme.mac(8))
    }
  }

  @ViewBuilder
  private var subtasks: some View {
    let rows = board.subtaskRows(of: card)
    if !rows.isEmpty {
      let parents = board.subtaskParentIDs(of: card)
      let shown = rows.prefix(BoardModel.visibleSubtaskRows)
      VStack(alignment: .leading, spacing: 0) {
        ForEach(Array(shown), id: \.id) { item in
          let open = item.task.status == .open
          HStack(spacing: theme.mac(4)) {
            Color.clear.frame(width: CGFloat(item.depth) * theme.mac(10), height: 1)
            Text("└").foregroundStyle(theme.dim)
            Button { model.toggleComplete(item.id) } label: {
              Text(item.task.status == .completed ? "●" : item.task.status == .cancelled ? "✕" : "○")
                .foregroundStyle(open ? theme.dim : theme.muted)
                .hitTarget()
            }
            .buttonStyle(.plain)
            .accessibilityLabel(open ? "Complete \(item.task.title)" : "Reopen \(item.task.title)")
            Text(item.task.title)
              .foregroundStyle(open && !model.isBlocked(item.task.id, status: item.task.status) ? theme.muted : theme.dim)
              .strikethrough(!open, color: theme.dim)
              .lineLimit(1)
              .frame(maxWidth: .infinity, alignment: .leading)
              .contentShape(Rectangle())
              .onTapGesture { model.navigation.inspect(item.id, isPad: isPad) }
            if parents.contains(item.id) {
              foldButton(item.id, title: item.task.title, folded: board.folded.contains(item.id))
            }
          }
          .font(theme.type.callout)
          .frame(height: theme.mac(20))
          .rowSelection(model.navigation.selectedTaskID == item.id)
          .contextMenu {
            TaskContextMenu(context: TaskMenuContext(
              taskID: item.id, title: item.task.title, status: item.task.status, isList: item.task.isList,
              isPromoted: item.task.isPromoted == true,
              isPlanned: board.snapshot.plannedTaskIDs.contains(item.id), allowsStructure: false))
          }
        }
        if rows.count > shown.count {
          Button("+\(rows.count - shown.count) more") { model.navigation.inspect(card.id, isPad: isPad) }
            .font(theme.type.secondary)
            .foregroundStyle(theme.dim)
            .buttonStyle(.plain)
            .frame(minHeight: theme.mac(20))
        }
      }
      .padding(.top, theme.mac(4))
    }
  }

  private func foldButton(_ id: String, title: String, folded: Bool) -> some View {
    Button {
      withAnimation(.snappy(duration: 0.2)) { board.toggleFold(id) }
    } label: {
      Text(folded ? "▸" : "▾")
        .font(theme.type.secondary)
        .foregroundStyle(theme.dim)
        .frame(width: theme.mac(12))
        .hitTarget()
    }
    .buttonStyle(.plain)
    .accessibilityLabel(folded ? "Unfold \(title)" : "Fold \(title)")
  }
}
