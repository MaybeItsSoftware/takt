import Observation

/// Which page of the settings window is showing.
///
/// The window is a sidebar of pages rather than a toolbar of tabs: the app's
/// own pages first, then one page per integration — enumerated from the
/// plugins that vend one, never named here — then sync and the advanced page.
@MainActor
@Observable final class SettingsNavState {
  enum Pane: String, CaseIterable, Identifiable {
    case general
    case focus
    case reminders
    case appearance
    case keyboard
    case agent
    case plugins
    case sync
    case advanced

    var id: String { rawValue }

    var title: String {
      switch self {
      case .general: "General"
      case .focus: "Focus"
      case .reminders: "Reminders"
      case .appearance: "Appearance"
      case .keyboard: "Keyboard"
      case .agent: "Agent"
      case .plugins: "Installed plugins"
      case .sync: "Sync"
      case .advanced: "Advanced"
      }
    }

    var summary: String {
      switch self {
      case .general:
        "How the app starts, confirms and celebrates."
      case .focus:
        "Where a focus block runs and how it is scored."
      case .reminders:
        "What this Mac reminds you about: due times, what is due today, dailies left."
      case .appearance:
        "Theme, light or dark, and the faces and size the app is set in."
      case .keyboard:
        "Global hotkeys, where Quick Add captures, and the window's keymap."
      case .agent:
        "Which agent the panel runs, and what it is told in every thread."
      case .plugins:
        "Plugins you installed yourself, as packages in the plugins folder."
      case .sync:
        "One workspace across this Mac, your iPhone and your Android phone."
      case .advanced:
        "Export your workspace, diagnostics and the files behind the app."
      }
    }

    var systemImage: String {
      switch self {
      case .general: "gearshape"
      case .focus: "scope"
      case .reminders: "bell"
      case .appearance: "paintpalette"
      case .keyboard: "keyboard"
      case .agent: "sparkles"
      case .plugins: "puzzlepiece.extension"
      case .sync: "arrow.triangle.2.circlepath"
      case .advanced: "wrench.and.screwdriver"
      }
    }

    /// Extra words the sidebar's filter matches, so typing "font" finds
    /// Appearance and "hotkey" finds Keyboard.
    var keywords: [String] {
      switch self {
      case .general: ["login", "launch", "delete", "confirm", "celebration", "sound", "completing"]
      case .focus: ["panel", "menu bar", "score", "quality", "multiplier", "block", "timer"]
      case .reminders: ["notification", "notifications", "alert", "due", "deadline", "dailies", "remind"]
      case .appearance:
        ["theme", "dark", "light", "font", "typeface", "text size", "colour", "color", "zed", "grape", "priority"]
      case .keyboard: ["hotkey", "shortcut", "quick add", "capture", "keymap", "keybinding"]
      case .agent: ["claude", "codex", "client", "assistant", "instructions", "context", "prompt", "ai"]
      case .plugins: ["install", "package", "extension"]
      case .sync: ["account", "devices", "iphone", "android", "server", "password"]
      case .advanced: ["export", "markdown", "json", "backup", "diagnostics", "debug", "support"]
      }
    }

    static let appPanes: [Pane] = [.general, .focus, .reminders, .appearance, .keyboard, .agent]
    static let accountPanes: [Pane] = [.sync, .advanced]
  }

  /// A page in the sidebar: one of the app's, or an integration's by its
  /// settings card identifier.
  enum Destination: Hashable {
    case pane(Pane)
    case integration(String)

    /// A name for the page that outlives a launch, for what it remembers —
    /// which of its sections are open.
    var storageKey: String {
      switch self {
      case .pane(let pane): pane.rawValue
      case .integration(let id): "integration.\(id)"
      }
    }
  }

  var destination: Destination = .pane(.general)

  func select(pane: Pane) {
    destination = .pane(pane)
  }
}
