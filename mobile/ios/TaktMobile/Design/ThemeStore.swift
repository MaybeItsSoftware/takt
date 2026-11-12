import Foundation
import OSLog
import TaktCore
import TaktWorkspace
import SwiftUI
import UIKit
import WidgetKit

/// Which theme is chosen, which appearance, and the `Theme` they resolve to.
///
/// Owned by the app root and read through `@Environment(\.theme)` (the
/// resolved value) or `@Environment(ThemeStore.self)` (the choices and the
/// imported themes, for Settings). Every change rebuilds `theme`, so the
/// window redraws in the new theme at once; it also re-dresses the UIKit
/// chrome and writes the resolved palette to the app group for the widgets.
///
/// Imported themes and the choice live in the workspace, so they follow the
/// user's other devices (docs/themes.md, "The chosen theme follows you"):
/// each theme is a row in the synced `themes` table, and the choice is the
/// `theme.selected` and `theme.appearance` preferences. "Use a different theme
/// on this device" and the choice made under it stay in UserDefaults and never
/// leave the phone.
///
/// The store is re-read after every change the workspace model hears of —
/// local writes, a sync pull and another process's commit (`data_version`) —
/// so a theme imported or chosen on the Mac shows up here without a relaunch.
@MainActor
@Observable
final class ThemeStore {
  /// The store the app root owns, for the UIKit and widget side effects that
  /// cannot read the environment.
  private(set) static var shared: ThemeStore?

  private(set) var theme: Theme
  /// Every stored theme and what became of it on this phone: the theme, or
  /// why not, and the issues found on the way. A theme that cannot load here
  /// is still a row, and still reaches the other devices intact.
  private(set) var library: ThemeFileLibrary = .empty

  /// The synced choice, as last read from the workspace.
  private(set) var selectedIdentifier: String
  private(set) var appearance: AppearanceChoice

  /// "Use a different theme on this device". Turning it on keeps what is on
  /// screen as this device's own choice; turning it off follows the others
  /// again.
  var usesDeviceTheme: Bool {
    didSet {
      guard usesDeviceTheme != oldValue else { return }
      if usesDeviceTheme {
        deviceSelectedIdentifier = selectedIdentifier
        deviceAppearance = appearance
      }
      defaults.set(usesDeviceTheme, forKey: Keys.deviceEnabled)
      refresh()
    }
  }

  private(set) var deviceSelectedIdentifier: String {
    didSet { defaults.set(deviceSelectedIdentifier, forKey: Keys.deviceSelected) }
  }

  private(set) var deviceAppearance: AppearanceChoice {
    didSet { defaults.set(deviceAppearance.rawValue, forKey: Keys.deviceAppearance) }
  }

  @ObservationIgnored private let defaults: UserDefaults
  @ObservationIgnored private weak var model: WorkspaceModel?
  @ObservationIgnored private var rows: [StoredTheme] = []
  @ObservationIgnored private var lastWidgetTheme: WidgetTheme?
  @ObservationIgnored private let logger = Logger(subsystem: "uk.co.maybeitssoftware.takt", category: "ThemeStore")

  enum Keys {
    /// The choice as it was kept before it synced, and is still mirrored:
    /// what the first run with a workspace seeds the synced choice from.
    static let selected = "theme.selected"
    static let appearance = AppearanceChoice.storageKey
    static let deviceEnabled = "theme.device.enabled"
    static let deviceSelected = "theme.device.selected"
    static let deviceAppearance = "theme.device.appearance"
  }

  init(defaults: UserDefaults = .standard) {
    self.defaults = defaults
    let standard = BuiltInThemeSpecifications.defaultIdentifier
    selectedIdentifier = defaults.string(forKey: Keys.selected) ?? standard
    appearance = defaults.string(forKey: Keys.appearance).flatMap(AppearanceChoice.init(rawValue:)) ?? .system
    usesDeviceTheme = defaults.bool(forKey: Keys.deviceEnabled)
    deviceSelectedIdentifier = defaults.string(forKey: Keys.deviceSelected) ?? standard
    deviceAppearance =
      defaults.string(forKey: Keys.deviceAppearance).flatMap(AppearanceChoice.init(rawValue:)) ?? .system
    theme = .standard
    theme = resolveTheme()
  }

  /// Reads themes and the choice from the workspace and follows its changes.
  /// The first time, a choice made before it synced becomes the synced one,
  /// rather than every device starting on Chalk.
  func attach(to model: WorkspaceModel) {
    self.model = model
    migrateLocalFolder(into: model.store)
    seedChoiceIfUnset(in: model.store)
    model.changeObservers.append { [weak self] _ in self?.reloadFromStore() }
    reloadFromStore()
  }

  /// Makes this the store the chrome and widgets follow, and dresses them.
  func install() {
    Self.shared = self
    ThemeChrome.apply(theme)
    publishToWidgets()
  }

  // MARK: What is in force

  /// The identifier actually being followed on this device.
  var effectiveIdentifier: String { usesDeviceTheme ? deviceSelectedIdentifier : selectedIdentifier }
  var effectiveAppearance: AppearanceChoice { usesDeviceTheme ? deviceAppearance : appearance }

  /// The scheme to force: the theme's lock if it has one, else the choice.
  var colorScheme: ColorScheme? { theme.lockedColorScheme ?? effectiveAppearance.colorScheme }

  /// The built-ins, then every stored theme that loaded, as the iPhone
  /// resolves them.
  var available: [ThemeSpecification] { BuiltInThemeSpecifications.all(for: .ios) + library.themes }

  /// The chosen theme is unknown here (it failed to load, or has not
  /// arrived), so the default is standing in.
  var isFallingBack: Bool { theme.specification.identifier != effectiveIdentifier }

  /// Selects a theme for wherever the choice is being made: this device if it
  /// has opted out, every device otherwise.
  func select(_ identifier: String) {
    if usesDeviceTheme {
      deviceSelectedIdentifier = identifier
      refresh()
    } else {
      write { try $0.setPreference(WorkspacePreferenceKey.themeSelected, identifier) }
    }
  }

  func setAppearance(_ choice: AppearanceChoice) {
    if usesDeviceTheme {
      deviceAppearance = choice
      refresh()
    } else {
      write { try $0.setPreference(WorkspacePreferenceKey.themeAppearance, choice.rawValue) }
    }
  }

  // MARK: Imported themes

  /// What importing a file came to.
  struct ImportResult {
    let fileName: String
    /// Nil when the file was not stored: it could not be read as a theme.
    let identifier: String?
    let issues: [ThemeFileIssue]
  }

  /// Stores a theme file as a synced row and reloads. A file that cannot be
  /// read as a theme at all is not stored, and its problems are returned.
  func importTheme(from url: URL) -> ImportResult {
    let fileName = url.lastPathComponent
    let accessing = url.startAccessingSecurityScopedResource()
    defer { if accessing { url.stopAccessingSecurityScopedResource() } }
    func failed(_ message: String) -> ImportResult {
      ImportResult(
        fileName: fileName, identifier: nil,
        issues: [ThemeFileIssue(source: fileName, severity: .error, message: message)])
    }
    guard let data = try? Data(contentsOf: url) else { return failed("could not be opened") }
    guard let json = String(data: data, encoding: .utf8) else { return failed("is not UTF-8 text") }
    let (decoded, decodeIssues) = ThemeFileLoader.decode(data, source: fileName)
    guard let file = decoded else {
      return ImportResult(fileName: fileName, identifier: nil, issues: decodeIssues)
    }
    let identifier = ThemeFileLoader.identifier(of: file, source: fileName)
    if BuiltInThemeSpecifications.all.contains(where: { $0.identifier == identifier }) {
      return failed("identifier \"\(identifier)\" is a built-in theme's; give it one of its own")
    }
    guard write({ try $0.upsertTheme(id: identifier, json: json) }) else {
      return failed("could not be saved to the workspace")
    }
    let source = ThemeFolderMirror.fileName(for: identifier, json: json)
    let issues = library.outcomes.first { $0.source == source }?.issues ?? decodeIssues
    return ImportResult(fileName: fileName, identifier: identifier, issues: issues)
  }

  /// The row an outcome was loaded from.
  func identifier(ofSource source: String) -> String? {
    rows.first { ThemeFolderMirror.fileName(for: $0.id, json: $0.json) == source }?.id
  }

  /// Deletes a stored theme everywhere. A device using it falls back to Chalk.
  func removeTheme(source: String) {
    guard let identifier = identifier(ofSource: source) else { return }
    write { try $0.deleteTheme(id: identifier) }
  }

  // MARK: The workspace

  /// Writes through the model, so the change is seen like any other: the
  /// observers run, which re-reads this store.
  @discardableResult
  private func write(_ work: (WorkspaceStore) throws -> Void) -> Bool {
    guard let model else { return false }
    return model.perform(work)
  }

  /// Re-reads the themes and the choice. Cheap when nothing moved: the
  /// library is only rebuilt when a row's text changed.
  func reloadFromStore() {
    guard let store = model?.store else { return }
    if let stored = try? store.themes(), stored.map(\.json) != rows.map(\.json) || stored.map(\.id) != rows.map(\.id) {
      rows = stored
      library = ThemeFileLoader.load(
        stored.map {
          ThemeFileSource(name: ThemeFolderMirror.fileName(for: $0.id, json: $0.json), data: Data($0.json.utf8))
        },
        platform: .ios)
    }
    if let preferences = try? store.preferences() {
      let selected =
        (preferences[WorkspacePreferenceKey.themeSelected] ?? nil) ?? BuiltInThemeSpecifications.defaultIdentifier
      let chosen =
        (preferences[WorkspacePreferenceKey.themeAppearance] ?? nil).flatMap(AppearanceChoice.init(rawValue:))
        ?? .system
      if selected != selectedIdentifier { selectedIdentifier = selected }
      if chosen != appearance { appearance = chosen }
      defaults.set(selected, forKey: Keys.selected)
      defaults.set(chosen.rawValue, forKey: Keys.appearance)
    }
    refresh()
  }

  /// `theme.selected` has never been written: this phone's earlier, local
  /// choice becomes everyone's.
  private func seedChoiceIfUnset(in store: WorkspaceStore) {
    guard let preferences = try? store.preferences(),
      preferences[WorkspacePreferenceKey.themeSelected] == nil
    else { return }
    do {
      try store.setPreference(WorkspacePreferenceKey.themeSelected, selectedIdentifier)
      if preferences[WorkspacePreferenceKey.themeAppearance] == nil {
        try store.setPreference(WorkspacePreferenceKey.themeAppearance, appearance.rawValue)
      }
    } catch {
      logger.error("Seeding the theme choice failed: \(error.localizedDescription, privacy: .public)")
    }
  }

  /// Themes imported before they synced were files in the app group. They
  /// become rows, once, and the folder goes.
  private func migrateLocalFolder(into store: WorkspaceStore) {
    let folder = AppGroup.containerURL.appending(path: "Priority/themes", directoryHint: .isDirectory)
    guard let files = try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)
    else { return }
    for url in files where url.pathExtension == ThemeFileLoader.fileExtension {
      guard let data = try? Data(contentsOf: url), let json = String(data: data, encoding: .utf8),
        let file = ThemeFileLoader.decode(data, source: url.lastPathComponent).0
      else { continue }
      let identifier = ThemeFileLoader.identifier(of: file, source: url.lastPathComponent)
      _ = try? store.upsertTheme(id: identifier, json: json)
    }
    try? FileManager.default.removeItem(at: folder)
  }

  // MARK: Resolution

  private func resolveTheme() -> Theme {
    let identifier = effectiveIdentifier
    let specification =
      available.first { $0.identifier == identifier } ?? BuiltInThemeSpecifications.defaultTheme(for: .ios)
    return Theme(specification: specification, displayScale: UIScreen.main.scale)
  }

  private func refresh() {
    let resolved = resolveTheme()
    guard resolved != theme else { return }
    theme = resolved
    guard Self.shared === self else { return }
    ThemeChrome.apply(resolved)
    ThemeChrome.redrawWindows()
    publishToWidgets()
  }

  // MARK: Widgets

  /// Writes the resolved palette for both appearances where the widgets and
  /// the Live Activity read it, and reloads their timelines.
  private func publishToWidgets() {
    let widgetTheme = WidgetTheme(theme)
    guard widgetTheme != lastWidgetTheme else { return }
    if lastWidgetTheme == nil, WidgetTheme.load() == widgetTheme {
      lastWidgetTheme = widgetTheme
      return
    }
    do {
      try widgetTheme.write()
      lastWidgetTheme = widgetTheme
      WidgetCenter.shared.reloadAllTimelines()
    } catch {
      logger.error("Widget theme write failed: \(error.localizedDescription, privacy: .public)")
    }
  }
}

extension WidgetTheme {
  init(_ theme: Theme) {
    let specification = theme.specification
    func table(_ appearance: ThemeAppearance) -> [String: String] {
      Dictionary(
        uniqueKeysWithValues: ThemeColorRole.allCases.map {
          ($0.rawValue, specification.palette.color($0, in: appearance).hexString)
        })
    }
    let typography = theme.structure.typography
    let installed = Set(UIFont.familyNames)
    self.init(
      identifier: specification.identifier,
      lockedAppearance: specification.lockedAppearance?.rawValue,
      light: table(.light), dark: table(.dark),
      bodyFamily: typography.body.families.first { installed.contains($0) },
      monoFamily: typography.mono.families.first { installed.contains($0) },
      controlRadius: theme.radius.control, hairline: theme.structure.border.hairline)
  }
}

/// The appearance the user chose. Themes follow the system by default; the
/// other two pin it. A theme locked to one appearance overrides this.
enum AppearanceChoice: String, CaseIterable, Identifiable {
  case system, light, dark

  var id: String { rawValue }

  var title: String {
    switch self {
    case .system: "System"
    case .light: "Light"
    case .dark: "Dark"
    }
  }

  var colorScheme: ColorScheme? {
    switch self {
    case .system: nil
    case .light: .light
    case .dark: .dark
    }
  }

  static let storageKey = "appearanceChoice"
}

/// The UIKit chrome — navigation bars, tab bars, segmented controls — dressed
/// in the theme's faces and colours rather than San Francisco on material.
@MainActor
enum ThemeChrome {
  static func apply(_ theme: Theme) {
    let type = theme.type
    let well = theme.uiColor(.well)
    let border = theme.uiColor(.border)
    let ink = theme.uiColor(.ink)
    let primary = theme.uiColor(.primary)
    let icon = theme.uiChrome(.icon)
    let statusBar = theme.uiChrome(.statusBar)
    let statusText = theme.statesStatusBar ? ink : theme.uiColor(.mutedText)
    let body = theme.structure.typography.scale.body
    let secondary = theme.mac(11)

    // The title bar: the well, a border under it, the title in medium body.
    let navigation = UINavigationBarAppearance()
    navigation.configureWithOpaqueBackground()
    navigation.backgroundColor = well
    navigation.shadowColor = border
    navigation.shadowImage = nil
    navigation.titleTextAttributes = [.font: type.uiFont(body, weight: .medium), .foregroundColor: ink]
    navigation.largeTitleTextAttributes = [
      .font: type.uiFont(theme.structure.typography.scale.display - 4, weight: .medium), .foregroundColor: ink,
    ]
    let button = UIBarButtonItemAppearance()
    button.normal.titleTextAttributes = [.font: type.uiFont(body), .foregroundColor: ink]
    navigation.buttonAppearance = button
    navigation.doneButtonAppearance = button
    navigation.backButtonAppearance = button
    UINavigationBar.appearance().standardAppearance = navigation
    UINavigationBar.appearance().scrollEdgeAppearance = navigation
    UINavigationBar.appearance().compactAppearance = navigation
    UINavigationBar.appearance().compactScrollEdgeAppearance = navigation
    UINavigationBar.appearance().tintColor = icon

    // The status bar: its own fill, a border over it, secondary words; the
    // tab in front in the accent, the rest as icons at rest.
    let tab = UITabBarAppearance()
    tab.configureWithOpaqueBackground()
    tab.backgroundColor = statusBar
    tab.shadowColor = border
    tab.shadowImage = nil
    for item in [tab.stackedLayoutAppearance, tab.inlineLayoutAppearance, tab.compactInlineLayoutAppearance] {
      item.normal.titleTextAttributes = [.font: type.uiFont(secondary), .foregroundColor: statusText]
      item.normal.iconColor = icon
      item.selected.titleTextAttributes = [.font: type.uiFont(secondary, weight: .medium), .foregroundColor: primary]
      item.selected.iconColor = primary
    }
    UITabBar.appearance().standardAppearance = tab
    UITabBar.appearance().scrollEdgeAppearance = tab

    let toolbar = UIToolbarAppearance()
    toolbar.configureWithOpaqueBackground()
    toolbar.backgroundColor = statusBar
    toolbar.shadowColor = border
    UIToolbar.appearance().standardAppearance = toolbar
    UIToolbar.appearance().scrollEdgeAppearance = toolbar
    UIToolbar.appearance().compactAppearance = toolbar

    // Segments are the Mac's tabs: secondary words, the chosen one on the
    // selection fill.
    UISegmentedControl.appearance().setTitleTextAttributes(
      [.font: type.uiFont(secondary), .foregroundColor: theme.uiColor(.mutedText)], for: .normal)
    UISegmentedControl.appearance().setTitleTextAttributes(
      [.font: type.uiFont(secondary, weight: .medium), .foregroundColor: ink], for: .selected)
    UISegmentedControl.appearance().selectedSegmentTintColor = theme.uiChrome(.selected)
    UISegmentedControl.appearance().backgroundColor = theme.uiColor(.altRow)
  }

  /// Appearance proxies only dress views created after they are set, so a
  /// live theme change re-adds each window's views to pick them up.
  static func redrawWindows() {
    for scene in UIApplication.shared.connectedScenes {
      guard let windowScene = scene as? UIWindowScene else { continue }
      for window in windowScene.windows {
        for view in window.subviews {
          view.removeFromSuperview()
          window.addSubview(view)
        }
      }
    }
  }
}
