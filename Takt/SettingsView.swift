import AppKit
import TaktCore
import SwiftUI
// MARK: - Carbon modifier constants (avoid importing Carbon in SwiftUI file)
private let carbonCmdKey = 0x0100
private let carbonShiftKey = 0x0200
private let carbonOptionKey = 0x0800
private let carbonControlKey = 0x1000

// MARK: - Hotkey Recorder

class HotkeyNSTextField: NSTextField {
  var isRecording = false
  var onRecord: ((Int, Int) -> Void)?
  var displayString: String = ""

  override var acceptsFirstResponder: Bool { true }

  override func mouseDown(with event: NSEvent) {
    window?.makeFirstResponder(self)
  }

  override func becomeFirstResponder() -> Bool {
    isRecording = true
    stringValue = "Type shortcut\u{2026}"
    return super.becomeFirstResponder()
  }

  override func resignFirstResponder() -> Bool {
    isRecording = false
    stringValue = displayString
    return super.resignFirstResponder()
  }

  override func keyDown(with event: NSEvent) {
    guard isRecording else { return }

    if event.keyCode == 53 {  // Escape = cancel
      window?.makeFirstResponder(nil)
      return
    }

    let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
    var carbonMods = 0
    if flags.contains(.command) { carbonMods |= carbonCmdKey }
    if flags.contains(.shift) { carbonMods |= carbonShiftKey }
    if flags.contains(.option) { carbonMods |= carbonOptionKey }
    if flags.contains(.control) { carbonMods |= carbonControlKey }

    // Require at least one modifier for a global hotkey
    guard carbonMods != 0 else { return }

    onRecord?(Int(event.keyCode), carbonMods)
    window?.makeFirstResponder(nil)
  }
}

struct HotkeyRecorderField: NSViewRepresentable {
  @Binding var keyCode: Int
  @Binding var modifiers: Int

  func makeNSView(context: Context) -> HotkeyNSTextField {
    let tf = HotkeyNSTextField()
    tf.focusRingType = .none
    tf.isEditable = false
    tf.isSelectable = false
    tf.alignment = .center
    // A shortcut is a key, and keys are set in the theme's monospace
    // everywhere else they are drawn (`KeyCap`).
    let theme = context.environment.theme
    tf.font = Theme.nsFont(theme.type.mono, size: theme.scale.caption)
    // Flat: the themed frame round it is drawn by SwiftUI, not AppKit's bezel.
    tf.isBezeled = false
    tf.drawsBackground = false
    tf.textColor = NSColor(theme.ink)
    tf.displayString = Self.displayString(keyCode: keyCode, modifiers: modifiers)
    tf.stringValue = tf.displayString
    tf.onRecord = { code, mods in
      keyCode = code
      modifiers = mods
    }
    return tf
  }

  func updateNSView(_ tf: HotkeyNSTextField, context: Context) {
    tf.displayString = Self.displayString(keyCode: keyCode, modifiers: modifiers)
    if !tf.isRecording {
      tf.stringValue = tf.displayString
    }
    tf.onRecord = { code, mods in
      keyCode = code
      modifiers = mods
    }
  }

  static func displayString(keyCode: Int, modifiers: Int) -> String {
    var mods = ""
    if modifiers & carbonControlKey != 0 { mods += "\u{2303}" }
    if modifiers & carbonOptionKey != 0 { mods += "\u{2325}" }
    if modifiers & carbonShiftKey != 0 { mods += "\u{21E7}" }
    if modifiers & carbonCmdKey != 0 { mods += "\u{2318}" }

    let keyNames: [Int: String] = [
      0: "A", 1: "S", 2: "D", 3: "F", 4: "H", 5: "G", 6: "Z", 7: "X",
      8: "C", 9: "V", 11: "B", 12: "Q", 13: "W", 14: "E", 15: "R",
      16: "Y", 17: "T", 18: "1", 19: "2", 20: "3", 21: "4", 22: "6",
      23: "5", 24: "=", 25: "9", 26: "7", 27: "-", 28: "8", 29: "0",
      30: "]", 31: "O", 32: "U", 33: "[", 34: "I", 35: "P",
      36: "\u{21A9}", 37: "L", 38: "J", 39: "'", 40: "K", 41: ";",
      43: ",", 44: "/", 45: "N", 46: "M", 47: ".",
      48: "\u{21E5}", 49: "Space", 50: "`", 51: "\u{232B}",
      96: "F5", 97: "F6", 98: "F7", 99: "F3", 100: "F8", 101: "F9",
      103: "F11", 109: "F10", 111: "F12", 118: "F4", 120: "F2", 122: "F1",
      123: "\u{2190}", 124: "\u{2192}", 125: "\u{2193}", 126: "\u{2191}",
    ]

    let keyName = keyNames[keyCode] ?? "Key\(keyCode)"
    return mods + keyName
  }
}

// MARK: - Settings View

/// The settings window: a sidebar of pages and the page beside it.
///
/// Every page is a header and a `Form` of `Section`s drawn by
/// `SettingsFormStyle`, so the app's own pages, the sync page and each
/// plugin's page read as one surface. Integrations are enumerated from
/// `AppCoordinator.activePluginSettingsPages` — never named here.
struct SettingsView: View {
  /// An integration's page, as the sidebar lists it.
  struct IntegrationPage: Identifiable {
    let id: String
    let title: String
    let summary: String
    let status: String
    let systemImage: String
    let plugin: any PluginSettingsPageProviding
  }

  @Environment(AppCoordinator.self) var checkvistManager
  @Environment(SettingsNavState.self) var navState
  @Environment(WorkspaceViewModel.self) var workspace
  // Internal rather than private: the panes are extensions in files of their
  // own, and every one of them draws from it.
  @Environment(\.theme) var theme
  @State private var filter = ""
  @FocusState private var focus: SidebarFocus?
  @State var exportStatus: (message: String, isError: Bool)?
  @State var fontSearch = ""

  private enum SidebarFocus: Hashable {
    case search
    case list
  }

  var preferences: PreferencesManager {
    checkvistManager.preferences
  }

  func preferenceBinding<T>(_ keyPath: ReferenceWritableKeyPath<PreferencesManager, T>)
    -> Binding<T>
  {
    Binding(
      get: { preferences[keyPath: keyPath] },
      set: { preferences[keyPath: keyPath] = $0 }
    )
  }

  var body: some View {
    HStack(spacing: 0) {
      sidebar
        .frame(width: 200)
        .background(theme.paper.ignoresSafeArea())
      Rectangle().fill(theme.border).frame(width: theme.hairline).ignoresSafeArea()
      detail
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(theme.paper.ignoresSafeArea())
    }
    // The theme's primary and body face, like every other window.
    .tint(theme.primary)
    .font(theme.bodyFont())
    .foregroundStyle(theme.ink)
    .onAppear { focus = .list }
    .background {
      // ⌘1…⌘9 jump straight to a page, in sidebar order.
      ForEach(Array(allDestinations.prefix(9).enumerated()), id: \.offset) { index, destination in
        Button("") { navState.destination = destination }
          .keyboardShortcut(KeyEquivalent(Character(String(index + 1))), modifiers: .command)
          .opacity(0)
          .accessibilityHidden(true)
      }
    }
  }

  // MARK: - Sidebar

  var integrationPages: [IntegrationPage] {
    checkvistManager.activePluginSettingsPages
      // Themes have a page of their own kind: Appearance draws the gallery.
      .filter { !($0 is any ThemePlugin) }
      .map { page in
        // "Native Google Calendar Integration" reads as "Google Calendar" in a sidebar.
        var name = page.displayName
        for prefix in ["Native "] where name.hasPrefix(prefix) { name.removeFirst(prefix.count) }
        for suffix in [" Integration", " Plugin"] where name.hasSuffix(suffix) {
          name.removeLast(suffix.count)
        }
        return IntegrationPage(
          id: page.settingsCardIdentifier,
          title: name,
          summary: page.pluginDescription,
          status: page.sidebarStatusLabel(manager: checkvistManager),
          systemImage: page.settingsIconSystemName,
          plugin: page)
      }
  }

  private var appPanes: [SettingsNavState.Pane] { SettingsNavState.Pane.appPanes.filter(matches) }
  private var accountPanes: [SettingsNavState.Pane] {
    SettingsNavState.Pane.accountPanes.filter(matches)
  }
  private var visibleIntegrations: [IntegrationPage] {
    integrationPages.filter { matches($0.title) || matches($0.summary) }
  }
  private var showsPluginsPane: Bool { matches(.plugins) }

  /// Every page in sidebar order, after the filter — what the arrow keys walk.
  private var visibleDestinations: [SettingsNavState.Destination] {
    appPanes.map { .pane($0) } + visibleIntegrations.map { .integration($0.id) }
      + (showsPluginsPane ? [.pane(.plugins)] : []) + accountPanes.map { .pane($0) }
  }

  private var allDestinations: [SettingsNavState.Destination] {
    SettingsNavState.Pane.appPanes.map { .pane($0) } + integrationPages.map { .integration($0.id) }
      + [.pane(.plugins)] + SettingsNavState.Pane.accountPanes.map { .pane($0) }
  }

  private func matches(_ pane: SettingsNavState.Pane) -> Bool {
    matches(pane.title) || matches(pane.summary) || pane.keywords.contains(where: matches)
  }

  private func matches(_ text: String) -> Bool {
    let needle = filter.trimmingCharacters(in: .whitespaces)
    return needle.isEmpty || text.localizedCaseInsensitiveContains(needle)
  }

  private var sidebar: some View {
    VStack(alignment: .leading, spacing: 0) {
      HStack(spacing: theme.space.xs) {
        Image(systemName: "magnifyingglass")
          .font(theme.captionFont)
          .foregroundStyle(theme.muted)
        TextField("Search settings", text: $filter)
          .textFieldStyle(.plain)
          .font(theme.bodyFont())
          .focused($focus, equals: .search)
          .onSubmit {
            if let first = visibleDestinations.first { navState.destination = first }
            focus = .list
          }
          .onKeyPress(.downArrow) {
            focus = .list
            return .handled
          }
          .help("↩ opens the first match, ↓ moves to the list")
      }
      .themedControlFrame(expands: true)
      .padding(.horizontal, theme.space.sm)
      .padding(.top, theme.space.xs)
      .padding(.bottom, theme.space.sm)

      ScrollView {
        VStack(alignment: .leading, spacing: theme.space.md) {
          sidebarGroup(nil, panes: appPanes)
          if !visibleIntegrations.isEmpty || showsPluginsPane {
            VStack(alignment: .leading, spacing: 1) {
              sidebarEyebrow("Integrations")
              ForEach(visibleIntegrations) { page in
                sidebarRow(
                  .integration(page.id), title: page.title, systemImage: page.systemImage,
                  status: page.status)
              }
              if showsPluginsPane {
                sidebarRow(
                  .pane(.plugins), title: SettingsNavState.Pane.plugins.title,
                  systemImage: SettingsNavState.Pane.plugins.systemImage, status: nil)
              }
            }
          }
          sidebarGroup("Account", panes: accountPanes)
          if visibleDestinations.isEmpty {
            Text("No settings match “\(filter)”.")
              .font(theme.captionFont)
              .foregroundStyle(theme.muted)
              .padding(.horizontal, theme.space.md)
          }
        }
        .padding(.horizontal, theme.space.sm)
        .padding(.bottom, theme.space.md)
      }
      .focusable()
      .focused($focus, equals: .list)
      .focusEffectDisabled()
      .onKeyPress(.upArrow) { step(-1) }
      .onKeyPress(.downArrow) { step(1) }
      .onKeyPress(characters: .alphanumerics, phases: .down) { press in
        // Typing while the list has focus starts a search, the way a source
        // list's type-select would.
        guard press.modifiers.isEmpty else { return .ignored }
        filter = press.characters
        focus = .search
        return .handled
      }
    }
  }

  private func step(_ delta: Int) -> KeyPress.Result {
    let destinations = visibleDestinations
    guard !destinations.isEmpty else { return .ignored }
    let index = destinations.firstIndex(of: navState.destination) ?? (delta > 0 ? -1 : destinations.count)
    let next = min(max(index + delta, 0), destinations.count - 1)
    navState.destination = destinations[next]
    return .handled
  }

  @ViewBuilder
  private func sidebarGroup(_ title: String?, panes: [SettingsNavState.Pane]) -> some View {
    if !panes.isEmpty {
      VStack(alignment: .leading, spacing: 1) {
        if let title { sidebarEyebrow(title) }
        ForEach(panes) { pane in
          sidebarRow(.pane(pane), title: pane.title, systemImage: pane.systemImage, status: nil)
        }
      }
    }
  }

  private func sidebarEyebrow(_ title: String) -> some View {
    MicroLabel(title)
      .padding(.horizontal, theme.space.sm)
      .padding(.top, theme.space.xs)
      .padding(.bottom, theme.space.xxs)
  }

  private func sidebarRow(
    _ destination: SettingsNavState.Destination, title: String, systemImage: String, status: String?
  ) -> some View {
    let isSelected = navState.destination == destination
    let showsFocus = isSelected && focus == .list
    // The ⌘1…⌘9 the window's background binds, by the same sidebar order.
    let shortcut = allDestinations.prefix(9).firstIndex(of: destination).map { "\(title) · ⌘\($0 + 1)" }
    return Button {
      navState.destination = destination
      focus = .list
    } label: {
      // The main window's sidebar row, geometry and selection band alike, so
      // the two sidebars read as one kind of thing.
      HStack(spacing: theme.space.sm) {
        Image(systemName: systemImage)
          .foregroundStyle(isSelected ? theme.primary : theme.muted)
          .frame(width: WorkspaceSidebarMetrics.iconWidth)
        Text(title)
          .foregroundStyle(theme.ink)
          .lineLimit(1)
        Spacer(minLength: theme.space.xs)
        if let status, !status.isEmpty {
          Text(status)
            .font(theme.captionFont)
            .foregroundStyle(theme.dim)
            .lineLimit(1)
        }
      }
      .sidebarRowPadding(theme)
      .background(WorkspaceSelectionBackground(isSelected: isSelected, hasKeyboard: showsFocus))
    }
    .buttonStyle(.plain)
    .help(shortcut ?? "")
    .accessibilityAddTraits(isSelected ? .isSelected : [])
  }

  // MARK: - Detail

  private var detail: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: theme.space.xl) {
        SettingsPageHeader(title: pageTitle, summary: pageSummary)
        Form { pageContent }
          .settingsChrome()
          .environment(\.settingsPageKey, navState.destination.storageKey)
          .environment(\.settingsSearchQuery, filter)
      }
      .padding(.horizontal, theme.space.xl + theme.space.md)
      .padding(.top, theme.space.lg)
      .padding(.bottom, theme.space.xl * 2)
      .frame(maxWidth: 760, alignment: .leading)
      .frame(maxWidth: .infinity, alignment: .leading)
    }
    .scrollContentBackground(.hidden)
    // A fresh scroll position per page.
    .id(navState.destination)
  }

  private var selectedIntegration: IntegrationPage? {
    guard case .integration(let id) = navState.destination else { return nil }
    return integrationPages.first { $0.id == id }
  }

  private var pageTitle: String {
    switch navState.destination {
    case .pane(let pane): return pane.title
    case .integration: return selectedIntegration?.title ?? "Integration"
    }
  }

  private var pageSummary: String {
    switch navState.destination {
    case .pane(let pane): return pane.summary
    case .integration:
      return selectedIntegration?.summary ?? "This integration is no longer installed."
    }
  }

  @ViewBuilder
  private var pageContent: some View {
    switch navState.destination {
    case .pane(.general): generalPane
    case .pane(.focus): focusPane
    case .pane(.appearance): appearancePane
    case .pane(.keyboard): keyboardPane
    case .pane(.agent): agentPane
    case .pane(.plugins): pluginsPane
    case .pane(.sync): SettingsSyncPane()
    case .pane(.advanced): advancedPane
    case .integration:
      if let page = selectedIntegration {
        page.plugin.makeSettingsView(manager: checkvistManager)
      } else {
        Section { Text("Choose a page from the sidebar.").foregroundStyle(theme.muted) }
      }
    }
  }

  // MARK: - Installed plugins

  @ViewBuilder
  private var pluginsPane: some View {
    let manager = checkvistManager.userPluginManager
    Section {
      HStack(spacing: theme.space.xs) {
        Button {
          manager.installPluginPackageInteractively()
        } label: {
          Label("Install plugin…", systemImage: "plus")
        }
        Button {
          manager.openPluginsFolder()
        } label: {
          Label("Open plugins folder", systemImage: "folder")
        }
        Spacer(minLength: 0)
      }
      if manager.sortedInstalledPlugins.isEmpty {
        Text("No plugins installed. The built-in integrations are listed above them in the sidebar.")
          .font(theme.captionFont)
          .foregroundStyle(theme.muted)
      }
    } header: {
      SettingsHeader("Plugins folder")
    }
    ForEach(manager.sortedInstalledPlugins, id: \.manifest.id) { plugin in
      Section {
        SettingsToggleRow(
          "Enabled",
          detail: plugin.manifest.summary,
          isOn: Binding(
            get: { manager.isPluginEnabled(plugin.manifest.id) },
            set: { manager.setPluginEnabled($0, pluginIdentifier: plugin.manifest.id) }))
        LabeledContent("Version", value: plugin.manifest.version ?? "—")
        LabeledContent("Identifier") {
          Text(plugin.manifest.id).font(theme.monoFont(size: theme.scale.caption))
        }
        HStack(spacing: theme.space.xs) {
          Button("Reveal in Finder") { manager.revealPluginInFinder(plugin) }
          Spacer(minLength: 0)
          Button("Remove plugin", role: .destructive) { manager.removePlugin(plugin) }
        }
      } header: {
        SettingsHeader(plugin.manifest.name)
      }
    }
  }
}

extension View {
  /// A status notice in the house convention: a tint of the status hue behind
  /// it, a border of the same hue, on the control radius. Settings drew these
  /// as ad-hoc 8% fills with a 30% stroke at radius 8 or 10, one per page.
  func settingsStatusSurface(_ theme: Theme, tint: Color) -> some View {
    padding(theme.space.sm)
      .frame(maxWidth: .infinity, alignment: .leading)
      .themedSurface(
        theme,
        fill: tint.opacity(Theme.statusFillOpacity),
        radius: theme.controlRadius,
        stroke: tint.opacity(Theme.statusBorderOpacity))
  }
}
