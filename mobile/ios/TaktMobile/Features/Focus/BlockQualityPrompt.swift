import TaktCore
import TaktWorkspace
import SwiftUI

/// The one question asked at the end of a block: how did that go? The Mac's
/// `WorkspaceFocusQualityPrompt`.
///
/// Presented once, at the root, for whichever surface ended the block — the
/// Focus screen, a running day card, the timer running out — so Today and
/// Focus ask it the same way. A block is worth the minutes it took multiplied
/// by the answer, and the running total is shown while choosing, so the cost
/// of flattering yourself is visible at the moment you would do it. On a
/// hardware keyboard 1–5 choose, Return logs and Escape keeps working.
struct BlockQualityPrompt: View {
  @Environment(\.theme) private var theme
  @Environment(WorkspaceModel.self) private var model
  let pending: PendingBlockCompletion

  @State private var quality: FocusQuality = .solid
  @State private var customMultiplier: Double = FocusQuality.solid.multiplier
  @State private var isCustom = false
  @State private var today = StoreQuery(0.0)

  private var multiplier: Double {
    isCustom ? FocusPoints.clamped(multiplier: customMultiplier) : quality.multiplier
  }

  private var points: Double {
    FocusPoints.score(seconds: pending.seconds, multiplier: multiplier)
  }

  var body: some View {
    NavigationStack {
      ScrollView {
        VStack(alignment: .leading, spacing: theme.space.lg) {
          header
          choices
          customRow
          Hairline()
          tally
          Button("Log the time without a score") { model.confirmBlockCompletion(multiplier: nil) }
            .font(theme.type.callout)
            .foregroundStyle(theme.muted)
            .accessibilityIdentifier("quality.skip")
        }
        .padding(theme.space.lg)
      }
      .background(theme.paper)
      .navigationTitle("How did that go?")
      .navigationBarTitleDisplayMode(.inline)
      .toolbar {
        ToolbarItem(placement: .cancellationAction) {
          Button("Keep working") { model.cancelBlockCompletion() }
            .keyboardShortcut(.cancelAction)
            .accessibilityIdentifier("quality.keepWorking")
        }
        ToolbarItem(placement: .confirmationAction) {
          Button("Log it") { model.confirmBlockCompletion(multiplier: multiplier) }
            .keyboardShortcut(.defaultAction)
            .accessibilityIdentifier("quality.logIt")
        }
      }
    }
    .presentationDetents([.medium, .large])
    .interactiveDismissDisabled()
    .task {
      await today.load(model.store) { store in try store.focusPointsSummary(now: .now).today }
    }
  }

  private var header: some View {
    VStack(alignment: .leading, spacing: theme.space.xs) {
      Text(pending.completeTask ? "Complete task" : "Log progress, task stays open")
        .font(theme.type.caption)
        .foregroundStyle(theme.muted)
      Text(pending.title)
        .font(theme.type.title)
        .foregroundStyle(theme.ink)
        .lineLimit(2)
      Text("\(FocusPoints.formatted(Double(pending.seconds) / 60)) minutes of focused work")
        .font(theme.type.numeral)
        .foregroundStyle(theme.muted)
    }
  }

  private var choices: some View {
    VStack(spacing: 0) {
      ForEach(Array(FocusQuality.allCases.enumerated()), id: \.element) { index, option in
        let isChosen = !isCustom && quality == option
        Button {
          quality = option
          isCustom = false
        } label: {
          HStack(spacing: theme.space.md) {
            KeyCap("\(index + 1)")
            VStack(alignment: .leading, spacing: 1) {
              Text(option.title).font(theme.type.body).foregroundStyle(theme.ink)
              Text(option.detail).font(theme.type.footnote).foregroundStyle(theme.muted)
            }
            Spacer(minLength: theme.space.sm)
            Text("×\(FocusPoints.formatted(option.multiplier))")
              .font(theme.type.numeralBody)
              .foregroundStyle(isChosen ? theme.primary : theme.muted)
          }
          .padding(.vertical, theme.space.sm + 2)
          .padding(.horizontal, theme.space.md)
          .frame(minHeight: theme.touchTarget)
          .background(isChosen ? theme.primary.opacity(0.10) : Color.clear)
          .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .keyboardShortcut(KeyEquivalent(Character("\(index + 1)")), modifiers: [])
        .accessibilityLabel("\(option.title), \(option.detail)")
        .accessibilityAddTraits(isChosen ? .isSelected : [])
        .accessibilityIdentifier("quality.\(option.rawValue)")
        if index < FocusQuality.allCases.count - 1 { Hairline(role: .borderMuted) }
      }
    }
    .clipShape(RoundedRectangle(cornerRadius: theme.radius.panel))
    .cardSurface()
  }

  private var customRow: some View {
    VStack(alignment: .leading, spacing: theme.space.sm) {
      Toggle("Something else", isOn: $isCustom)
        .toggleStyle(ThemedToggleStyle())
        .accessibilityIdentifier("quality.customToggle")
      if isCustom {
        HStack(spacing: theme.space.md) {
          Text("×\(FocusPoints.formatted(customMultiplier))")
            .font(theme.type.numeralBody)
            .foregroundStyle(theme.primary)
            .frame(minWidth: 56, alignment: .leading)
          Stepper("Multiplier", value: $customMultiplier, in: FocusPoints.multiplierRange, step: 0.25)
            .labelsHidden()
            .accessibilityIdentifier("quality.custom")
          Spacer(minLength: 0)
        }
      }
    }
  }

  private var tally: some View {
    HStack(alignment: .firstTextBaseline) {
      VStack(alignment: .leading, spacing: theme.space.xxs) {
        Text("This block").font(theme.type.caption).foregroundStyle(theme.muted)
        Text("\(FocusPoints.formatted(points)) pts")
          .font(theme.type.mono(24.5, .medium, relativeTo: .title))
          .foregroundStyle(theme.primary)
          .contentTransition(.numericText())
          .animation(.snappy, value: points)
      }
      Spacer()
      VStack(alignment: .trailing, spacing: theme.space.xxs) {
        Text("Today").font(theme.type.caption).foregroundStyle(theme.muted)
        Text("\(FocusPoints.formatted(today.value + points)) pts")
          .font(theme.type.numeralBody)
          .foregroundStyle(theme.ink)
      }
    }
    .accessibilityElement(children: .combine)
    .accessibilityIdentifier("quality.tally")
  }
}

/// A key drawn as a key: the hardware shortcut that picks a row, which on a
/// touch screen still numbers the choices.
struct KeyCap: View {
  @Environment(\.theme) private var theme
  let text: String
  init(_ text: String) { self.text = text }

  var body: some View {
    Text(text)
      .font(theme.type.numeral)
      .foregroundStyle(theme.muted)
      .frame(minWidth: 22, minHeight: 22)
      .overlay(RoundedRectangle(cornerRadius: theme.radius.control).strokeBorder(theme.border, lineWidth: theme.stroke))
      .accessibilityHidden(true)
  }
}
