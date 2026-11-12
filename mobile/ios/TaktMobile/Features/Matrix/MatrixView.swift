import TaktWorkspace
import SwiftUI

/// The Eisenhower matrix: four quadrants over the scope's cards, with an
/// unplaced tray beneath. Cards are placed by drag or from their menu.
struct MatrixView: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @State private var matrix: MatrixModel

  init(scope: ListScope) {
    _matrix = State(initialValue: MatrixModel(scope: scope))
  }

  var body: some View {
    ScrollView {
      VStack(spacing: theme.mac(8)) {
        plane
        MatrixTrayView(matrix: matrix)
      }
      .padding(theme.mac(8))
    }
    .background(theme.paper)
    .task(id: QueryKey(revision: model.revision, scope: matrix.scope)) {
      await matrix.load(model)
    }
    .accessibilityIdentifier("matrix")
  }

  /// The Mac's one plane: a bordered field split by the two axes, each
  /// quadrant named in a micro-label at its outer corner, the axes' ends
  /// named in muted secondary text where they meet the edge.
  private var plane: some View {
    Grid(horizontalSpacing: 0, verticalSpacing: 0) {
      GridRow {
        MatrixCellView(cell: .doNow, matrix: matrix, number: 1, alignment: .topLeading)
        MatrixCellView(cell: .schedule, matrix: matrix, number: 2, alignment: .topTrailing)
      }
      GridRow {
        MatrixCellView(cell: .delegate, matrix: matrix, number: 3, alignment: .bottomLeading)
        MatrixCellView(cell: .eliminate, matrix: matrix, number: 4, alignment: .bottomTrailing)
      }
    }
    .overlay {
      // The axes, through the middle.
      ZStack {
        Rectangle().fill(theme.border).frame(width: 1)
        Rectangle().fill(theme.border).frame(height: 1)
      }
      .allowsHitTesting(false)
    }
    .overlay {
      GeometryReader { proxy in
        let mid = proxy.size.height / 2
        let centre = proxy.size.width / 2
        Group {
          axisLabel("Urgent").position(x: 0, y: 0).offset(x: theme.mac(6), y: mid - theme.mac(3))
          axisLabel("Not urgent", trailing: true).position(x: proxy.size.width, y: 0)
            .offset(x: -theme.mac(6), y: mid - theme.mac(3))
          axisLabel("Important").position(x: centre, y: 0).offset(x: theme.mac(6), y: theme.mac(4))
          axisLabel("Not important").position(x: centre, y: proxy.size.height)
            .offset(x: theme.mac(6), y: -theme.mac(4))
        }
        .allowsHitTesting(false)
      }
    }
    .background(RoundedRectangle(cornerRadius: theme.mac.radiusMenu).fill(theme.paper))
    .clipShape(RoundedRectangle(cornerRadius: theme.mac.radiusMenu))
    .overlay(RoundedRectangle(cornerRadius: theme.mac.radiusMenu).strokeBorder(theme.borderMuted, lineWidth: 1))
  }

  /// An axis end's name, anchored by its leading (or trailing) edge and its
  /// foot (or head) so it sits beside the hairline it names.
  private func axisLabel(_ text: String, trailing: Bool = false) -> some View {
    Text(text)
      .font(theme.type.secondary)
      .foregroundStyle(theme.muted)
      .fixedSize()
      .alignmentGuide(HorizontalAlignment.center) { trailing ? $0[.trailing] : $0[.leading] }
      .alignmentGuide(VerticalAlignment.center) { $0[.bottom] }
  }
}

/// One quadrant of the plane: its micro-label in the outer corner, and its
/// tasks as the Mac's chips — a dot and the title in a bordered box at
/// radius 4.
struct MatrixCellView: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  let cell: MatrixCell
  let matrix: MatrixModel
  let number: Int
  let alignment: Alignment
  @State private var isTargeted = false

  private var isTop: Bool { alignment == .topLeading || alignment == .topTrailing }
  private var isLeading: Bool { alignment == .topLeading || alignment == .bottomLeading }

  var body: some View {
    let cards = matrix.cards(in: cell)
    VStack(alignment: isLeading ? .leading : .trailing, spacing: theme.mac(4)) {
      if isTop { label(count: cards.count) }
      VStack(alignment: .leading, spacing: theme.mac(4)) {
        ForEach(cards) { card in
          MatrixRow(card: card, matrix: matrix)
        }
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(.vertical, theme.mac(14))
      Spacer(minLength: 0)
      if !isTop { label(count: cards.count) }
    }
    .padding(.horizontal, theme.mac(8))
    .padding(.vertical, theme.mac(6))
    .frame(maxWidth: .infinity, minHeight: 190, alignment: alignment)
    .background(isTargeted ? theme.hover : Color.clear)
    .contentShape(Rectangle())
    .dropDestination(for: String.self) { ids, _ in
      guard let id = ids.first else { return false }
      matrix.place(id, in: cell, model: model)
      return true
    } isTargeted: { isTargeted = $0 }
    .accessibilityElement(children: .contain)
    .accessibilityLabel("\(cell.title), \(cell.detail)")
    .accessibilityIdentifier("matrix.cell.\(cell.rawValue)")
  }

  private func label(count: Int) -> some View {
    Text("\(number)  \(cell.title.uppercased())  \(count)")
      .font(theme.type.groupLabel)
      .foregroundStyle(theme.dim)
      .lineLimit(1)
      .frame(maxWidth: .infinity, alignment: isLeading ? .leading : .trailing)
      .accessibilityAddTraits(.isHeader)
  }
}

/// Cards the matrix has not placed yet: the Mac's "Not placed" tray, the
/// chips wrapping under it.
struct MatrixTrayView: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  let matrix: MatrixModel
  @State private var isTargeted = false

  var body: some View {
    let cards = matrix.unplaced
    VStack(alignment: .leading, spacing: 0) {
      Text("Not placed  \(cards.count)")
        .font(theme.type.body)
        .foregroundStyle(theme.muted)
        .padding(.horizontal, theme.mac(8))
        .padding(.vertical, theme.mac(4))
      if cards.isEmpty, matrix.isLoaded {
        Text("Everything is placed.").font(theme.type.secondary).foregroundStyle(theme.dim)
          .padding(.horizontal, theme.mac(8))
          .frame(minHeight: theme.mac(26))
      }
      FlowLayout(spacing: theme.mac(4)) {
        ForEach(cards) { card in
          MatrixRow(card: card, matrix: matrix)
        }
      }
      .padding(theme.mac(4))
    }
    .frame(maxWidth: .infinity, minHeight: theme.mac(120), alignment: .topLeading)
    .background(
      RoundedRectangle(cornerRadius: theme.mac.radiusMenu, style: .continuous)
        .fill(isTargeted ? theme.hover : theme.paper))
    .overlay(
      RoundedRectangle(cornerRadius: theme.mac.radiusMenu, style: .continuous)
        .strokeBorder(theme.borderMuted, lineWidth: 1))
    .dropDestination(for: String.self) { ids, _ in
      guard let id = ids.first else { return false }
      matrix.place(id, in: nil, model: model)
      return true
    } isTargeted: { isTargeted = $0 }
    .accessibilityIdentifier("matrix.tray")
  }
}

struct MatrixRow: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  @Environment(\.isPadLayout) private var isPad
  let card: BoardCard
  let matrix: MatrixModel

  private var isSelected: Bool { model.navigation.selectedTaskID == card.id }

  /// The Mac's matrix chip: 20 high at radius 4 in the muted border, a 6pt
  /// dot (accent when selected, muted when open, dim when done) and the
  /// title. The dot completes it; the rest opens it.
  var body: some View {
    HStack(spacing: theme.mac(4)) {
      Button { model.completeCelebrating(card.id) } label: {
        Circle()
          .fill(isSelected ? theme.primary : card.status == .open ? theme.muted : theme.dim)
          .frame(width: 6, height: 6)
          .celebrationIcon(card.id)
          .hitTarget()
      }
      .buttonStyle(.plain)
      .accessibilityLabel(card.status == .open ? "Complete \(card.title)" : "Reopen \(card.title)")
      Text(card.title)
        .font(theme.type.callout)
        .foregroundStyle(card.status == .open && !model.isBlocked(card.id) ? theme.ink : theme.dim)
        .strikethrough(card.status != .open, color: theme.dim)
        .lineLimit(1)
        .celebrationStrike(card.id)
    }
    .padding(.horizontal, theme.mac(4))
    .frame(height: theme.mac(20))
    .background(
      RoundedRectangle(cornerRadius: 3).fill(isSelected ? theme.selected : Color.clear).padding(1))
    .background(RoundedRectangle(cornerRadius: theme.mac.radiusRow).fill(theme.paper))
    .overlay(
      RoundedRectangle(cornerRadius: theme.mac.radiusRow)
        .strokeBorder(isSelected ? theme.focus : theme.borderMuted, lineWidth: 1))
    .contentShape(Rectangle())
    .onTapGesture { model.navigation.inspect(card.id, isPad: isPad) }
    .draggable(card.id) {
      Text(card.title).font(theme.type.callout).padding(theme.mac(4)).background(theme.paper)
    }
    .contextMenu {
      ForEach(MatrixCell.allCases) { cell in
        Button { matrix.place(card.id, in: cell, model: model) } label: {
          Label(cell.title, systemImage: matrix.cell(of: card.id) == cell ? "checkmark" : "square.grid.2x2")
        }
      }
      Button { matrix.place(card.id, in: nil, model: model) } label: { Label("Unplace", systemImage: "tray") }
        .disabled(matrix.cell(of: card.id) == nil)
      TaskContextMenu(context: card.menuContext)
    }
    .accessibilityIdentifier("matrix.row.\(card.title)")
  }
}
