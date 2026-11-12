import TaktCore
import TaktWorkspace
import SwiftUI

/// A one-pixel rule in the border colour. Borders do the work shadows would.
struct Hairline: View {
  @Environment(\.theme) private var theme
  var role: ThemeColorRole = .border
  var body: some View {
    Rectangle().fill(theme.color(role)).frame(height: theme.hairline)
  }
}

/// The theme's micro-label above a group. Chalk sets it sentence-case and
/// muted, the way Zed does; a theme can bring back the shouted capitals.
struct SectionLabel: View {
  @Environment(\.theme) private var theme
  let text: String
  init(_ text: String) { self.text = text }
  var body: some View {
    Text(text)
      .microLabel()
      .frame(maxWidth: .infinity, alignment: .leading)
  }
}

/// The quiet label over a group's blocked tasks — an outline parent's
/// children, a board column's cards — which sink below the rest. A
/// micro-label and a hairline, nothing to tap or focus: it is read as one
/// static line and the tasks either side of it are reached as if it were not
/// there.
struct BlockedDivider: View {
  @Environment(\.theme) private var theme
  /// Over tasks whose start is still to come rather than ones waiting on others.
  var isBuried = false

  var body: some View {
    HStack(spacing: theme.space.sm) {
      Text(isBuried ? "Buried" : "Blocked")
        .microLabel()
        .fixedSize()
      Hairline()
    }
    .allowsHitTesting(false)
    .focusable(false)
    .accessibilityElement(children: .ignore)
    .accessibilityLabel(isBuried ? "Buried: these tasks start later" : "Blocked: these tasks wait on others")
    .accessibilityAddTraits(.isStaticText)
  }
}

extension View {
  /// Sets text as the theme's micro-label: its size, weight, tracking, case
  /// and colour role.
  func microLabel() -> some View { modifier(MicroLabelModifier()) }
}

private struct MicroLabelModifier: ViewModifier {
  @Environment(\.theme) private var theme

  func body(content: Content) -> some View {
    let label = theme.type.microLabel
    content
      .font(label.font)
      .tracking(label.tracking)
      .textCase(label.isUppercased ? .uppercase : nil)
      .foregroundStyle(theme.color(label.role))
  }
}

/// A small squarish tag: tinted fill, hairline of the same hue, text in it.
/// Status is never a solid block and never a capsule.
struct Tag: View {
  @Environment(\.theme) private var theme
  let text: String
  var tint: Color?
  var systemImage: String?
  var mono = false

  var body: some View {
    HStack(spacing: 3) {
      if let systemImage { Image(systemName: systemImage).imageScale(.small) }
      Text(text).lineLimit(1)
    }
    .font(mono ? theme.type.secondaryMono : theme.type.secondary)
    .foregroundStyle(hue)
    .padding(.horizontal, 5)
    .padding(.vertical, 1)
    .background(RoundedRectangle(cornerRadius: theme.mac.radiusRow).fill(hue.opacity(0.10)))
    .overlay(RoundedRectangle(cornerRadius: theme.mac.radiusRow).strokeBorder(hue.opacity(0.35), lineWidth: 1))
  }

  private var hue: Color { tint ?? theme.muted }
}

/// A task's status mark, drawn as the Mac draws it (`widgets::check`): a
/// glyph in a 12px column, not a checkbox. A cancelled task's is ✕ in dim
/// text, a list's ≡ in muted; a completed task's is blank, because its
/// struck-through title already says so. An open task's is blank on the Mac
/// too; a phone has no hover to find the tap that completes it, so it draws
/// a hairline square in dim text instead — the one concession.
struct TaskCheckbox: View {
  @Environment(\.theme) private var theme
  let status: TaskStatus
  var isList = false
  /// The column's width; the glyph sits in it at the secondary size.
  var size: CGFloat?
  /// An open task's square. Off where the row itself completes the task.
  var showsBox = true

  var body: some View {
    let width = size ?? theme.mac.mark
    ZStack {
      switch status {
      case .cancelled:
        Text("✕").font(theme.type.secondary).foregroundStyle(theme.dim)
      case .open where isList:
        Text("≡").font(theme.type.secondary).foregroundStyle(theme.muted)
      case .open where showsBox:
        RoundedRectangle(cornerRadius: 2, style: .continuous)
          .strokeBorder(theme.dim, lineWidth: 1)
          .frame(width: 12, height: 12)
      default:
        Color.clear
      }
    }
    .frame(width: width, height: width)
    .accessibilityHidden(true)
  }
}

/// A chip as the Mac sets one beside a title: plain secondary-size text in
/// its colour, no box (`widgets::detail_chips`).
struct Chip: View {
  @Environment(\.theme) private var theme
  let text: String
  var tint: Color?
  var mono = false

  var body: some View {
    Text(text)
      .font(mono ? theme.type.secondaryMono : theme.type.secondary)
      .foregroundStyle(tint ?? theme.muted)
      .lineLimit(1)
      .fixedSize()
  }
}

/// A list's section heading as the Mac draws one: "Title · n" at the
/// secondary size in dim text, sentence case, no rule.
struct SectionHeading: View {
  @Environment(\.theme) private var theme
  let title: String
  var count: Int?
  var trailing: String?

  init(_ title: String, count: Int? = nil, trailing: String? = nil) {
    self.title = title
    self.count = count
    self.trailing = trailing
  }

  var body: some View {
    HStack(spacing: theme.space.xs) {
      Text(text).lineLimit(1)
      Spacer(minLength: 0)
      if let trailing { Text(trailing).lineLimit(1) }
    }
    .font(theme.type.secondary)
    .foregroundStyle(theme.dim)
    .textCase(nil)
    .padding(.horizontal, theme.mac(12))
    .padding(.top, theme.mac(12))
    .padding(.bottom, theme.mac(4))
    .frame(maxWidth: .infinity, alignment: .leading)
    .accessibilityAddTraits(.isHeader)
  }

  private var text: String {
    guard let count else { return title }
    return "\(title) · \(count)"
  }
}

/// The Mac's 10px bold capitals in dim text, over a settings group or a
/// matrix quadrant.
struct GroupLabel: View {
  @Environment(\.theme) private var theme
  let text: String
  init(_ text: String) { self.text = text }

  var body: some View {
    Text(text.uppercased())
      .font(theme.type.groupLabel)
      .foregroundStyle(theme.dim)
      .lineLimit(1)
      .frame(maxWidth: .infinity, alignment: .leading)
      .accessibilityAddTraits(.isHeader)
  }
}

extension View {
  /// A row's selection as the Mac draws it: the `selected` fill at radius 4,
  /// and a 1px `focus` edge where the row has the keyboard — on the phone,
  /// the row the inspector is showing.
  func rowSelection(_ isSelected: Bool, focused: Bool = false) -> some View {
    modifier(RowSelection(isSelected: isSelected, isFocused: focused))
  }
}

private struct RowSelection: ViewModifier {
  @Environment(\.theme) private var theme
  let isSelected: Bool
  let isFocused: Bool

  func body(content: Content) -> some View {
    let shape = RoundedRectangle(cornerRadius: theme.mac.radiusRow, style: .continuous)
    content
      .background(shape.fill(isSelected ? theme.selected : .clear))
      .overlay(shape.strokeBorder(isSelected && isFocused ? theme.focus : .clear, lineWidth: 1))
  }
}

/// A switch as the Mac's settings draw one: a 34×20 track with a 14 knob.
/// On, the accent fills it and the knob is paper; off, it is the hover fill
/// in a border, its knob muted.
struct ThemedToggleStyle: ToggleStyle {
  @Environment(\.theme) private var theme
  func makeBody(configuration: Configuration) -> some View {
    Button {
      configuration.isOn.toggle()
    } label: {
      HStack {
        configuration.label
          .font(theme.type.body)
          .foregroundStyle(theme.ink)
        Spacer(minLength: theme.space.sm)
        ThemedSwitch(isOn: configuration.isOn)
      }
      .contentShape(HitArea(minimum: theme.touchTarget))
    }
    .buttonStyle(.plain)
    .accessibilityValue(configuration.isOn ? "On" : "Off")
    .accessibilityAddTraits(.isToggle)
  }
}

/// The switch's painted part, for a row that lays its own label out.
struct ThemedSwitch: View {
  @Environment(\.theme) private var theme
  let isOn: Bool

  var body: some View {
    ZStack(alignment: isOn ? .trailing : .leading) {
      Capsule(style: .continuous)
        .fill(isOn ? theme.primary : theme.hover)
        .overlay(Capsule(style: .continuous).strokeBorder(isOn ? theme.primary : theme.border, lineWidth: 1))
        .frame(width: 34, height: 20)
      Circle().fill(isOn ? theme.paper : theme.muted).frame(width: 14, height: 14).padding(3)
    }
    .animation(.snappy(duration: 0.18), value: isOn)
    .accessibilityHidden(true)
  }
}

/// The frame every themed control sits in: a hairline at the control radius
/// on the raised surface.
struct ControlFrame: ViewModifier {
  @Environment(\.theme) private var theme
  var expands = true
  func body(content: Content) -> some View {
    let shape = RoundedRectangle(cornerRadius: theme.mac.radiusRow, style: .continuous)
    content
      .font(theme.type.body)
      .foregroundStyle(theme.ink)
      .padding(.horizontal, theme.mac(8))
      .padding(.vertical, theme.mac(4))
      .frame(minHeight: theme.mac.control)
      .frame(maxWidth: expands ? .infinity : nil, alignment: .leading)
      .background(shape.fill(theme.paper))
      .overlay(shape.strokeBorder(theme.border, lineWidth: 1))
  }
}

extension View {
  func controlFrame(expands: Bool = true) -> some View { modifier(ControlFrame(expands: expands)) }

  /// A bordered card as the Mac's board draws one: paper at radius 4 in the
  /// muted border; selected, the `selected` fill in the focus colour.
  func cardSurface(selected: Bool = false) -> some View { modifier(CardSurface(selected: selected)) }

  /// A panel at the menu radius: the alternate row fill in a border — a
  /// board column, a popover, a settings control's box.
  func panelSurface(radius: CGFloat? = nil) -> some View { modifier(PanelSurface(radius: radius)) }

  /// Paper behind a whole screen, including under the safe areas.
  func paperBackground() -> some View { modifier(PaperBackground()) }
}

private struct CardSurface: ViewModifier {
  @Environment(\.theme) private var theme
  let selected: Bool

  func body(content: Content) -> some View {
    let shape = RoundedRectangle(cornerRadius: theme.mac.radiusRow, style: .continuous)
    content
      .background(shape.fill(selected ? theme.selected : theme.paper))
      .overlay(shape.strokeBorder(selected ? theme.focus : theme.borderMuted, lineWidth: 1))
  }
}

private struct PanelSurface: ViewModifier {
  @Environment(\.theme) private var theme
  let radius: CGFloat?

  func body(content: Content) -> some View {
    let shape = RoundedRectangle(cornerRadius: radius ?? theme.mac.radiusMenu, style: .continuous)
    content
      .background(shape.fill(theme.altRow))
      .overlay(shape.strokeBorder(theme.border, lineWidth: 1))
  }
}

private struct PaperBackground: ViewModifier {
  @Environment(\.theme) private var theme

  func body(content: Content) -> some View {
    content.background(theme.paper.ignoresSafeArea())
  }
}

/// A button as the Mac draws one: 26px high at radius 6 in a 1px border,
/// on the alternate row fill, the hover fill while pressed. The primary one
/// is accent-bordered on the selection fill; a danger one is the error hue.
struct ThemedButtonStyle: ButtonStyle {
  @Environment(\.theme) private var theme
  enum Kind { case primary, quiet, danger }
  var kind: Kind = .quiet
  var compact = false

  func makeBody(configuration: Configuration) -> some View {
    let shape = RoundedRectangle(cornerRadius: theme.mac.radiusMenu, style: .continuous)
    configuration.label
      .font(compact ? theme.type.secondaryMedium : theme.type.callout.weight(.medium))
      .foregroundStyle(foreground)
      .padding(.horizontal, compact ? theme.mac(8) : theme.mac(12))
      .frame(minHeight: compact ? theme.mac(22) : theme.mac.control)
      .background(shape.fill(configuration.isPressed ? theme.hover : background))
      .overlay(shape.strokeBorder(border, lineWidth: 1))
      .contentShape(HitArea(minimum: theme.touchTarget))
  }

  private var foreground: Color {
    switch kind {
    case .primary: theme.primary
    case .quiet: theme.ink
    case .danger: theme.danger
    }
  }

  private var background: Color {
    switch kind {
    case .primary: theme.selected
    case .quiet: theme.altRow
    case .danger: theme.danger.opacity(0.10)
    }
  }

  private var border: Color {
    switch kind {
    case .primary: theme.primary
    case .quiet: theme.border
    case .danger: theme.danger.opacity(0.4)
    }
  }
}

/// What an empty surface says instead of being blank.
struct EmptyState: View {
  @Environment(\.theme) private var theme
  let title: String
  var message: String?
  var systemImage: String = "tray"

  var body: some View {
    VStack(spacing: theme.space.sm) {
      Image(systemName: systemImage)
        .font(theme.type.glyph(24.5, .light))
        .foregroundStyle(theme.dim)
      Text(title).font(theme.type.body).foregroundStyle(theme.muted)
      if let message {
        Text(message).font(theme.type.secondary).foregroundStyle(theme.dim).multilineTextAlignment(.center)
      }
    }
    .padding(theme.space.xl)
    .frame(maxWidth: .infinity)
  }
}

/// Lays its children out left to right, wrapping onto a new line when the
/// width runs out — chips that read as a sentence ("Lab or Office").
struct FlowLayout: Layout {
  var spacing: CGFloat = 4

  func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    let rows = arrange(width: proposal.width ?? .infinity, subviews: subviews)
    let height = rows.reduce(0) { $0 + $1.height } + spacing * CGFloat(max(0, rows.count - 1))
    let width = rows.map(\.width).max() ?? 0
    return CGSize(width: proposal.width ?? width, height: height)
  }

  func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    var y = bounds.minY
    for row in arrange(width: bounds.width, subviews: subviews) {
      var x = bounds.minX
      for index in row.indices {
        let size = subviews[index].sizeThatFits(.unspecified)
        subviews[index].place(
          at: CGPoint(x: x, y: y + (row.height - size.height) / 2), anchor: .topLeading, proposal: .unspecified)
        x += size.width + spacing
      }
      y += row.height + spacing
    }
  }

  private struct Row {
    var indices: [Int] = []
    var width: CGFloat = 0
    var height: CGFloat = 0
  }

  private func arrange(width: CGFloat, subviews: Subviews) -> [Row] {
    var rows: [Row] = []
    var current = Row()
    for index in subviews.indices {
      let size = subviews[index].sizeThatFits(.unspecified)
      let needed = current.indices.isEmpty ? size.width : current.width + spacing + size.width
      if needed > width, !current.indices.isEmpty {
        rows.append(current)
        current = Row()
      }
      current.width = current.indices.isEmpty ? size.width : current.width + spacing + size.width
      current.height = max(current.height, size.height)
      current.indices.append(index)
    }
    if !current.indices.isEmpty { rows.append(current) }
    return rows
  }
}

/// The chips the Mac sets after a task's title, in its order
/// (`widgets::detail_chips` then `task_chips`): blocked or later, ⌛ who,
/// up to three #tags, ↻ and ↗, the priority, the due day, done/total in mono,
/// and ¶ when there are notes. Plain secondary text, no boxes.
struct TaskChips: View {
  @Environment(\.theme) private var theme
  var status: TaskStatus = .open
  var isBlocked = false
  var isLater = false
  var waitingOn: String?
  var metadata: TaskChipMetadata?
  var dueAt: Date?
  var childrenDone = 0
  var childrenTotal = 0
  var hasNotes = false

  var body: some View {
    HStack(spacing: theme.mac(8)) {
      if status == .open, isBlocked {
        Chip(text: "blocked", tint: theme.warning)
      } else if status == .open, isLater {
        Chip(text: "later", tint: theme.dim)
      }
      if let who = waitingOn ?? metadata?.waitingOn {
        Chip(text: "⌛ \(who)", tint: theme.muted)
      }
      if let metadata {
        ForEach(metadata.tags.prefix(3), id: \.self) { tag in
          Chip(text: "#\(tag)", tint: theme.primary)
        }
        if metadata.isRecurring { Chip(text: "↻", tint: theme.dim) }
        if metadata.isLinked { Chip(text: "↗", tint: theme.dim) }
        if let priority = metadata.priority { Chip(text: Self.priorityLabel(priority), tint: priorityTint(priority)) }
      }
      if let dueAt {
        Chip(text: Format.day(dueAt), tint: status == .open && Format.isOverdue(dueAt) ? theme.danger : theme.dim)
      }
      if childrenTotal > 0 {
        Chip(text: "\(childrenDone)/\(childrenTotal)", tint: theme.dim, mono: true)
      }
      if hasNotes { Chip(text: "¶", tint: theme.dim) }
    }
  }

  static func priorityLabel(_ priority: Int) -> String {
    priority < 0 ? "↓\(-priority)" : "P\(priority)"
  }

  private func priorityTint(_ priority: Int) -> Color {
    switch priority {
    case 1: theme.danger
    case 2: theme.warning
    case ..<0: theme.dim
    default: theme.muted
    }
  }
}
