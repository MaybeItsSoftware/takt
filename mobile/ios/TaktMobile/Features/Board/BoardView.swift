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
        LazyHStack(alignment: .top, spacing: theme.space.md) {
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
        .padding(.horizontal, theme.space.lg)
        .padding(.vertical, theme.space.md)
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
    let available = width - theme.space.lg * 2
    if available < 500 { return max(240, available - 28) }
    return min(340, max(260, (available - theme.space.md * 2) / 3))
  }

  private var addColumnButton: some View {
    Button {
      promptText = ""
      columnPrompt = .add
    } label: {
      Label("Add column", systemImage: "plus")
        .font(theme.type.callout)
        .foregroundStyle(theme.muted)
        .frame(maxWidth: .infinity, minHeight: theme.touchTarget)
        .overlay(
          RoundedRectangle(cornerRadius: theme.radius.panel)
            .strokeBorder(theme.border, style: StrokeStyle(lineWidth: theme.stroke, dash: [4, 3])))
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
        LazyVStack(spacing: theme.space.sm) {
          ForEach(cards) { card in
            // The label over the column's blocked cards, which sink to its
            // foot, rides with the first of them.
            VStack(spacing: theme.space.sm) {
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
        .padding(theme.space.sm)
      }
      .scrollIndicators(.hidden)
    }
    .frame(width: width)
    .frame(maxHeight: .infinity, alignment: .top)
    .background(isTargeted ? theme.primary.opacity(0.06) : theme.altRow)
    .clipShape(RoundedRectangle(cornerRadius: theme.radius.panel, style: .continuous))
    .overlay(
      RoundedRectangle(cornerRadius: theme.radius.panel, style: .continuous)
        .strokeBorder(isTargeted ? theme.primary : theme.border, lineWidth: isTargeted ? 1.5 : 1))
    .dropDestination(for: String.self) { ids, _ in
      guard let id = ids.first, board.snapshot.allCards.contains(where: { $0.id == id }) else { return false }
      board.move(id, toColumn: column.id, model: model)
      return true
    } isTargeted: { isTargeted = $0 }
    .accessibilityIdentifier("board.column.\(column.title)")
  }

  private var header: some View {
    HStack(spacing: theme.space.sm) {
      Text(column.title)
        .font(theme.type.bodyMedium)
        .foregroundStyle(theme.ink)
        .lineLimit(1)
      Text("\(cards.count)")
        .font(theme.type.numeral)
        .foregroundStyle(theme.muted)
      Spacer(minLength: 0)
      Button {
        board.composerText = ""
        board.composingColumnID = column.id
        composerFocused.wrappedValue = true
      } label: {
        Image(systemName: "plus").frame(width: 32, height: 36).hitTarget()
      }
      .buttonStyle(.plain)
      .foregroundStyle(theme.muted)
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
        Image(systemName: "ellipsis").frame(width: 32, height: 36).hitTarget()
      }
      .foregroundStyle(theme.muted)
      .accessibilityLabel("\(column.title) options")
    }
    .padding(.leading, theme.space.md)
    .padding(.trailing, theme.space.xs)
    .frame(minHeight: theme.touchTarget)
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
      .padding(theme.space.md)
      .cardSurface()
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

  var body: some View {
    VStack(alignment: .leading, spacing: theme.space.xs) {
      if let parentTitle = card.parentTitle {
        Text(parentTitle).font(theme.type.footnote).foregroundStyle(theme.muted).lineLimit(1)
      }
      HStack(alignment: .top, spacing: theme.space.sm) {
        Button { model.completeCelebrating(card.id) } label: {
          TaskCheckbox(status: card.status, isList: card.isList)
            .celebrationIcon(card.id)
            .frame(width: 28, height: 28)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(card.status == .open ? "Complete \(card.title)" : "Reopen \(card.title)")
        Text(card.title)
          .font(card.isList ? theme.type.bodyMedium : theme.type.body)
          .foregroundStyle(card.status == .open ? theme.ink : theme.muted)
          .strikethrough(card.status != .open, color: theme.dim)
          .celebrationStrike(card.id)
          .frame(maxWidth: .infinity, alignment: .leading)
          .padding(.top, 3)
        if !card.subtasks.isEmpty {
          foldButton(card.id, title: card.title, folded: board.folded.contains(card.id))
        }
      }
      badges
      subtasks
    }
    // Blocked, the whole card steps back — mark, title, badges and the
    // subtasks that wait with it — while its frame and selection keep theirs.
    .opacity(isBlocked ? Theme.disabledOpacity : 1)
    .padding(theme.space.sm)
    .celebrationRow(card.id)
    .background(
      RoundedRectangle(cornerRadius: theme.radius.panel, style: .continuous)
        .fill(isSelected ? theme.primary.opacity(0.08) : theme.raised))
    .overlay(
      RoundedRectangle(cornerRadius: theme.radius.panel, style: .continuous)
        .strokeBorder(isSelected || isTargeted ? theme.primary : theme.border, lineWidth: theme.stroke))
    .contentShape(RoundedRectangle(cornerRadius: theme.radius.panel))
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

  @ViewBuilder
  private var badges: some View {
    let waiting = model.waitingBadge(card.id, status: card.status)
    let hasBadges = card.isPlanned || card.dueAt != nil || (card.estimateSeconds ?? 0) > 0 || card.listName != nil
      || waiting != nil
    if hasBadges {
      HStack(spacing: theme.space.xs) {
        if card.isPlanned {
          Image(systemName: "sun.max").font(theme.type.glyph(10.5)).foregroundStyle(theme.warning)
        }
        if let waiting { WaitingBadgeView(badge: waiting) }
        if let estimate = card.estimateSeconds, estimate > 0 {
          Text(Format.duration(estimate)).font(theme.type.numeral).foregroundStyle(theme.muted)
        }
        if let due = card.dueAt {
          Tag(
            text: Format.due(due),
            tint: card.status != .open ? theme.muted
              : Format.isOverdue(due) ? theme.danger : Format.isToday(due) ? theme.primary : theme.muted)
        }
        Spacer(minLength: 0)
        if let listName = card.listName {
          Text(listName).font(theme.type.footnote).foregroundStyle(theme.muted).lineLimit(1)
        }
      }
      .padding(.leading, 36)
    }
  }

  @ViewBuilder
  private var subtasks: some View {
    let rows = board.subtaskRows(of: card)
    if !rows.isEmpty {
      let parents = board.subtaskParentIDs(of: card)
      let shown = rows.prefix(BoardModel.visibleSubtaskRows)
      VStack(alignment: .leading, spacing: 0) {
        Hairline(role: .borderMuted).padding(.vertical, theme.space.xxs)
        ForEach(Array(shown), id: \.id) { item in
          HStack(spacing: theme.space.xs) {
            Color.clear.frame(width: CGFloat(item.depth) * Metrics.indent, height: 1)
            Button { model.toggleComplete(item.id) } label: {
              TaskCheckbox(status: item.task.status, isList: item.task.isList, size: 16)
                .frame(width: 28, height: 32)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(item.task.status == .open ? "Complete \(item.task.title)" : "Reopen \(item.task.title)")
            Text(item.task.title)
              .font(theme.type.callout)
              .foregroundStyle(item.task.status == .open ? theme.ink : theme.muted)
              .strikethrough(item.task.status != .open, color: theme.dim)
              .lineLimit(2)
              .frame(maxWidth: .infinity, alignment: .leading)
              .contentShape(Rectangle())
              .onTapGesture { model.navigation.inspect(item.id, isPad: isPad) }
            if parents.contains(item.id) {
              foldButton(item.id, title: item.task.title, folded: board.folded.contains(item.id))
            }
          }
          // A blocked subtask on a card that is not: the row steps back as a
          // blocked card does. On a blocked card it already has.
          .opacity(model.isBlocked(item.task.id, status: item.task.status) && !isBlocked ? Theme.disabledOpacity : 1)
          .contextMenu {
            TaskContextMenu(context: TaskMenuContext(
              taskID: item.id, title: item.task.title, status: item.task.status, isList: item.task.isList,
              isPromoted: item.task.isPromoted == true,
              isPlanned: board.snapshot.plannedTaskIDs.contains(item.id), allowsStructure: false))
          }
        }
        if rows.count > shown.count {
          Button("+\(rows.count - shown.count) more") { model.navigation.inspect(card.id, isPad: isPad) }
            .font(theme.type.footnote)
            .foregroundStyle(theme.muted)
            .buttonStyle(.plain)
            .padding(.leading, 32)
            .frame(minHeight: 28)
        }
      }
      .padding(.leading, theme.space.xs)
    }
  }

  private func foldButton(_ id: String, title: String, folded: Bool) -> some View {
    Button {
      withAnimation(.snappy(duration: 0.2)) { board.toggleFold(id) }
    } label: {
      Image(systemName: "chevron.right")
        .font(theme.type.glyph(9.5, .semibold))
        .foregroundStyle(theme.muted)
        .rotationEffect(.degrees(folded ? 0 : 90))
        .frame(width: 28, height: 28)
        .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .accessibilityLabel(folded ? "Unfold \(title)" : "Fold \(title)")
  }
}
