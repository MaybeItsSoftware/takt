import TaktCore
import SwiftUI
import UniformTypeIdentifiers

/// Settings → Theme: the appearance, the theme list with a swatch of each
/// palette, the per-device opt-out, and the imported themes with everything
/// found wrong with them.
///
/// Themes and the choice are synced (see `ThemeStore`), so a theme imported
/// here, or removed, is imported or removed on every device.
struct ThemeSettingsView: View {
  @Environment(\.theme) private var theme
  @Environment(ThemeStore.self) private var themes
  @State private var isImporting = false
  @State private var lastImport: ThemeStore.ImportResult?

  var body: some View {
    @Bindable var themes = themes
    List {
      Section {
        Picker(
          "Appearance", selection: Binding(get: { themes.effectiveAppearance }, set: { themes.setAppearance($0) })
        ) {
          ForEach(AppearanceChoice.allCases) { choice in
            Text(choice.title).tag(choice)
          }
        }
        .pickerStyle(.segmented)
        .disabled(theme.lockedColorScheme != nil)
        .accessibilityIdentifier("settings.appearance")
      } header: {
        header("Appearance")
      } footer: {
        if let locked = theme.specification.lockedAppearance {
          footer("\(theme.specification.name) is always \(locked.rawValue).")
        }
      }
      .settingsRows()

      Section {
        // The whole library is a screen of swatches; folded, the row says
        // which theme is on and opens onto the rest.
        SettingsDisclosure("All themes", value: theme.specification.name, key: "themes") {
          ForEach(themes.available, id: \.identifier) { specification in
            ThemeRow(specification: specification, isSelected: specification.identifier == theme.specification.identifier) {
              themes.select(specification.identifier)
            }
          }
        }
      } header: {
        header("Theme")
      } footer: {
        if themes.isFallingBack {
          footer("The chosen theme isn't available on this phone yet, so the default theme is standing in.")
        }
      }
      .settingsRows()
      .accessibilityIdentifier("settings.theme")

      Section {
        Toggle("Use a different theme on this device", isOn: $themes.usesDeviceTheme)
          .toggleStyle(ThemedToggleStyle())
          .accessibilityIdentifier("settings.theme.device")
      } footer: {
        footer(
          themes.usesDeviceTheme
            ? "This phone keeps its own theme and appearance. Your other devices are unaffected."
            : "Your theme and appearance follow your other devices.")
      }
      .settingsRows()

      if let lastImport {
        Section {
          IssueList(issues: lastImport.issues, audit: [])
          if lastImport.issues.isEmpty {
            Text("No problems found.").font(theme.type.caption).foregroundStyle(theme.muted)
          }
        } header: {
          header(lastImport.identifier == nil ? "Couldn't import \(lastImport.fileName)" : "Imported \(lastImport.fileName)")
        }
        .settingsRows()
      }

      Section {
        ForEach(themes.library.outcomes, id: \.source) { outcome in
          ImportedThemeRow(outcome: outcome)
            .swipeActions {
              Button("Remove", role: .destructive) { themes.removeTheme(source: outcome.source) }
            }
        }
        Button {
          isImporting = true
        } label: {
          Label("Import a theme…", systemImage: "square.and.arrow.down")
            .font(theme.type.body)
            .frame(maxWidth: .infinity, minHeight: theme.touchTarget, alignment: .leading)
            .contentShape(Rectangle())
        }
        .accessibilityIdentifier("settings.theme.import")
      } header: {
        header("Your themes")
      } footer: {
        footer("A theme is a .json file, the same format the Mac uses. Imported themes sync to your other devices.")
      }
      .settingsRows()
    }
    .font(theme.type.body)
    .macSettingsList()
    .background(theme.paper)
    .navigationTitle("Theme")
    .navigationBarTitleDisplayMode(.inline)
    .fileImporter(isPresented: $isImporting, allowedContentTypes: [.json], allowsMultipleSelection: true) { result in
      guard case .success(let urls) = result else { return }
      for url in urls { lastImport = themes.importTheme(from: url) }
    }
  }

  private func header(_ text: String) -> some View {
    SettingsGroupLabel(text)
  }

  private func footer(_ text: String) -> some View {
    Text(text).font(theme.type.caption).foregroundStyle(theme.muted)
  }
}

/// One theme in the list: its swatch, its name and summary, and a check when
/// it is the one in force.
private struct ThemeRow: View {
  @Environment(\.theme) private var theme
  let specification: ThemeSpecification
  let isSelected: Bool
  let action: () -> Void

  var body: some View {
    Button(action: action) {
      HStack(spacing: theme.space.md) {
        PaletteSwatch(specification: specification)
        VStack(alignment: .leading, spacing: theme.space.xxs) {
          Text(specification.name).font(theme.type.body).foregroundStyle(theme.ink)
          if !specification.summary.isEmpty {
            Text(specification.summary).font(theme.type.caption).foregroundStyle(theme.muted).lineLimit(2)
          }
        }
        Spacer(minLength: theme.space.sm)
        if isSelected {
          Image(systemName: "checkmark").font(theme.type.glyph(13, .semibold)).foregroundStyle(theme.primary)
        }
      }
      .frame(minHeight: theme.touchTarget)
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .accessibilityAddTraits(isSelected ? .isSelected : [])
    .accessibilityIdentifier("settings.theme.\(specification.identifier)")
  }
}

/// A theme's palette at a glance: its paper with ink and the four status
/// hues on it, light half and dark half — or one half, for a theme locked to
/// an appearance. Drawn in the theme's own hexes, not the one in force.
struct PaletteSwatch: View {
  @Environment(\.theme) private var theme
  let specification: ThemeSpecification

  private static let roles: [ThemeColorRole] = [.ink, .primary, .success, .danger, .warning]

  var body: some View {
    let appearances = specification.lockedAppearance.map { [$0] } ?? ThemeAppearance.allCases
    let shape = RoundedRectangle(cornerRadius: theme.radius.control, style: .continuous)
    HStack(spacing: 0) {
      ForEach(appearances, id: \.self) { appearance in
        VStack(alignment: .leading, spacing: 3) {
          HStack(spacing: 3) {
            ForEach(Self.roles, id: \.self) { role in
              Rectangle().fill(color(role, appearance)).frame(width: 5, height: 12)
            }
          }
          Rectangle().fill(color(.border, appearance)).frame(height: 1)
        }
        .padding(theme.space.xs + 1)
        .frame(maxHeight: .infinity)
        .background(color(.paper, appearance))
      }
    }
    .frame(height: 32)
    .fixedSize(horizontal: true, vertical: false)
    .clipShape(shape)
    .overlay(shape.strokeBorder(theme.border, lineWidth: theme.hairline))
    .accessibilityHidden(true)
  }

  private func color(_ role: ThemeColorRole, _ appearance: ThemeAppearance) -> Color {
    let value = specification.palette.color(role, in: appearance)
    return Color(red: value.red, green: value.green, blue: value.blue, opacity: value.alpha)
  }
}

/// An imported theme: its name, why it did not load if it did not, and every
/// issue from the file and from the audit.
private struct ImportedThemeRow: View {
  @Environment(\.theme) private var theme
  let outcome: ThemeFileOutcome

  var body: some View {
    VStack(alignment: .leading, spacing: theme.space.sm) {
      HStack(spacing: theme.space.md) {
        if let specification = outcome.specification {
          PaletteSwatch(specification: specification)
        }
        VStack(alignment: .leading, spacing: theme.space.xxs) {
          Text(outcome.specification?.name ?? outcome.source).font(theme.type.body).foregroundStyle(theme.ink)
          Text(outcome.specification?.identifier ?? "Not loaded")
            .font(theme.type.numeral)
            .foregroundStyle(outcome.specification == nil ? theme.danger : theme.muted)
        }
      }
      IssueList(issues: outcome.issues, audit: outcome.specification?.validate() ?? [])
    }
    .padding(.vertical, theme.space.xs)
  }
}

/// Errors, warnings and notes, worst first, each in its status hue.
private struct IssueList: View {
  @Environment(\.theme) private var theme
  let issues: [ThemeFileIssue]
  let audit: [ThemeIssue]

  var body: some View {
    let lines = issues.map { ($0.severity, $0.message, "File") } + audit.map { ($0.severity, $0.message, "Audit") }
    if !lines.isEmpty {
      VStack(alignment: .leading, spacing: theme.space.xs) {
        ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
          HStack(alignment: .firstTextBaseline, spacing: theme.space.sm) {
            Tag(text: label(line.0), tint: tint(line.0))
            Text("\(line.2): \(line.1)")
              .font(theme.type.caption)
              .foregroundStyle(theme.ink)
              .fixedSize(horizontal: false, vertical: true)
          }
        }
      }
    }
  }

  private func label(_ severity: ThemeIssueSeverity) -> String {
    switch severity {
    case .error: "Error"
    case .warning: "Warning"
    case .note: "Note"
    }
  }

  private func tint(_ severity: ThemeIssueSeverity) -> Color {
    switch severity {
    case .error: theme.danger
    case .warning: theme.warning
    case .note: theme.muted
    }
  }
}
