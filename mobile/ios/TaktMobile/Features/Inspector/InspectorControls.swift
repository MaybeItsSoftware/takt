import SwiftUI

/// A row of squarish chips, one of which is chosen — the Mac's
/// `ThemedSegmentedPicker`, at touch size. Selection is a border and a tint,
/// never elevation.
struct ChoiceChips<Value: Hashable>: View {
  @Environment(\.theme) private var theme
  let options: [(title: String, value: Value)]
  @Binding var selection: Value
  var tint: Color?
  var identifier: String?

  var body: some View {
    HStack(spacing: theme.space.xs) {
      ForEach(Array(options.enumerated()), id: \.offset) { index, option in
        let selected = option.value == selection
        Button {
          selection = option.value
        } label: {
          Text(option.title)
            .font(theme.type.callout)
            .foregroundStyle(selected ? hue : theme.ink)
            .lineLimit(1)
            .minimumScaleFactor(0.8)
            .frame(maxWidth: .infinity, minHeight: 36)
            .background(
              RoundedRectangle(cornerRadius: theme.radius.control, style: .continuous)
                .fill(selected ? hue.opacity(0.10) : theme.raised))
            .overlay(
              RoundedRectangle(cornerRadius: theme.radius.control, style: .continuous)
                .strokeBorder(selected ? hue.opacity(0.6) : theme.inputBorder, lineWidth: selected ? theme.stroke : theme.hairline))
            .hitTarget()
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier(identifier.map { "\($0).\(index)" } ?? "")
      }
    }
    .frame(minHeight: theme.touchTarget)
  }

  private var hue: Color { tint ?? theme.primary }
}

/// A labelled row: the muted label on the left, the control on the right.
struct InspectorRow<Content: View>: View {
  @Environment(\.theme) private var theme
  let label: String
  @ViewBuilder var content: Content

  init(_ label: String, @ViewBuilder content: () -> Content) {
    self.label = label
    self.content = content()
  }

  var body: some View {
    HStack(alignment: .center, spacing: theme.space.md) {
      Text(label)
        .font(theme.type.callout)
        .foregroundStyle(theme.muted)
        .frame(width: 92, alignment: .leading)
      content
        .frame(maxWidth: .infinity, alignment: .leading)
    }
    .frame(minHeight: theme.touchTarget)
  }
}

/// A group of rows under a sentence-case label, closed by a hairline.
struct InspectorSection<Content: View, Accessory: View>: View {
  @Environment(\.theme) private var theme
  let title: String
  @ViewBuilder var accessory: Accessory
  @ViewBuilder var content: Content

  init(_ title: String, @ViewBuilder accessory: () -> Accessory, @ViewBuilder content: () -> Content) {
    self.title = title
    self.accessory = accessory()
    self.content = content()
  }

  var body: some View {
    VStack(alignment: .leading, spacing: theme.space.sm) {
      HStack {
        SectionLabel(title)
        Spacer()
        accessory
      }
      content
    }
    .padding(.horizontal, theme.space.lg)
    .padding(.vertical, theme.space.md)
    .overlay(alignment: .bottom) { Hairline(role: .borderMuted) }
  }
}

extension InspectorSection where Accessory == EmptyView {
  init(_ title: String, @ViewBuilder content: () -> Content) {
    self.init(title, accessory: { EmptyView() }, content: content)
  }
}

/// A menu drawn as a themed control rather than a stock pop-up button.
struct ThemedMenuLabel: View {
  @Environment(\.theme) private var theme
  let title: String
  var systemImage: String = "chevron.up.chevron.down"

  var body: some View {
    HStack {
      Text(title).lineLimit(1)
      Spacer(minLength: theme.space.xs)
      Image(systemName: systemImage).font(theme.type.glyph(9.5, .semibold)).foregroundStyle(theme.muted)
    }
    .controlFrame()
    .contentShape(Rectangle())
  }
}
