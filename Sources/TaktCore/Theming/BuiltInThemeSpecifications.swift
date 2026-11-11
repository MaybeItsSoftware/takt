import Foundation
import TaktRustCore

/// The themes that ship with the app.
///
/// They are defined once, in the Rust core (`core/src/theme/builtins.rs`),
/// which the Mac, the iPhone and Android all read them from, and written out
/// whole to `shared/themes/`. These are their Swift values, built once per
/// platform. `Takt/Plugins/Native/Theme/` wraps them as plugins.
public enum BuiltInThemeSpecifications {
  /// The default (shown as "Takt", stored as Priority): what a fresh install shows, what a theme file
  /// extends unless it says otherwise, and what stands in for a theme that
  /// will not load.
  public static let priorityIdentifier = "native.theme.priority"
  /// The Zed look. The identifiers still say Chalk, its old name, because
  /// they are stored as people's choice and synced between devices.
  public static let chalkIdentifier = "native.theme.chalk"
  public static let chalkDarkIdentifier = "native.theme.chalk.dark"
  /// Grape: the house design language, whole — Zed's palette with its own
  /// type, radii and labels rather than Zed's.
  public static let grapeIdentifier = "native.theme.grape"

  /// The identifier a device uses when nothing has been chosen.
  public static let defaultIdentifier = priorityIdentifier

  /// The built-ins as the Mac resolves them, the default first.
  public static var all: [ThemeSpecification] { all(for: .macos) }

  /// The built-ins as `platform` resolves them: the same palettes, with each
  /// theme's per-platform structure. Default first, then Zed, Zed Dark and Grape.
  public static func all(for platform: ThemePlatform) -> [ThemeSpecification] {
    switch platform {
    case .macos: macos
    case .ios: ios
    case .android: android
    }
  }

  private static let macos = themeBuiltins(platform: .macos).map(ThemeSpecification.init)
  private static let ios = themeBuiltins(platform: .ios).map(ThemeSpecification.init)
  private static let android = themeBuiltins(platform: .android).map(ThemeSpecification.init)

  /// The default theme, resolved for `platform`.
  public static func defaultTheme(for platform: ThemePlatform) -> ThemeSpecification {
    priority(for: platform)
  }

  /// The default, resolved for `platform`. `priority` is the macOS one.
  public static func priority(for platform: ThemePlatform) -> ThemeSpecification {
    all(for: platform)[0]
  }

  /// Zed, resolved for `platform`. `chalk` is the macOS one.
  public static func chalk(for platform: ThemePlatform) -> ThemeSpecification {
    all(for: platform)[1]
  }

  /// Zed Dark, resolved for `platform`. It extends Zed, so it takes the same
  /// per-platform structure.
  public static func chalkDark(for platform: ThemePlatform) -> ThemeSpecification {
    all(for: platform)[2]
  }

  /// Grape, resolved for `platform`. `grape` is the macOS one.
  public static func grape(for platform: ThemePlatform) -> ThemeSpecification {
    all(for: platform)[3]
  }

  public static func specification(withIdentifier identifier: String) -> ThemeSpecification? {
    all.first { $0.identifier == identifier }
  }

  public static func specification(
    withIdentifier identifier: String, for platform: ThemePlatform
  ) -> ThemeSpecification? {
    all(for: platform).first { $0.identifier == identifier }
  }

  /// The Mac app's structure (IBM Plex Sans, square panels) in the house colours: the default.
  public static var priority: ThemeSpecification { priority(for: .macos) }
  /// Zed: IBM Plex Sans and Lilex, square panels, hairlines, warm paper and grape ink.
  public static var chalk: ThemeSpecification { chalk(for: .macos) }
  /// Zed with the appearance fixed to dark.
  public static var chalkDark: ThemeSpecification { chalkDark(for: .macos) }
  /// The house style whole: Arvo and Geist Mono, 8 and 6 radii, small capitals.
  public static var grape: ThemeSpecification { grape(for: .macos) }

  // MARK: - Per platform

  /// What each built-in lays over its own structure on the phones: the table
  /// in `docs/themes.md`. The Mac has no entry, because a built-in's own
  /// structure already *is* the Mac's.
  public static let platformStructures: [String: [ThemePlatform: ThemeFile.Structure]] = {
    let identifiers = [priorityIdentifier, chalkIdentifier, chalkDarkIdentifier, grapeIdentifier]
    return Dictionary(
      uniqueKeysWithValues: identifiers.map { identifier in
        let entries = themeBuiltinPlatformStructures(identifier: identifier).map {
          (ThemePlatform($0.platform), ThemeFile.Structure($0.structure))
        }
        return (identifier, Dictionary(uniqueKeysWithValues: entries))
      })
  }()

  /// Zed's per-platform structure: the platform's body size, panels and
  /// controls rounded a little, hit areas grown to the platform minimum.
  public static var chalkPlatformStructures: [ThemePlatform: ThemeFile.Structure] {
    platformStructures[chalkIdentifier] ?? [:]
  }

  /// The default's: the same as Zed's, so a phone on the default is set as the Mac is.
  public static var priorityPlatformStructures: [ThemePlatform: ThemeFile.Structure] {
    platformStructures[priorityIdentifier] ?? [:]
  }

  /// Grape's: the platform body sizes and hit areas, the Mac's radius scale.
  public static var grapePlatformStructures: [ThemePlatform: ThemeFile.Structure] {
    platformStructures[grapeIdentifier] ?? [:]
  }
}
