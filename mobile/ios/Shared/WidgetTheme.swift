import Foundation
import SwiftUI
import UIKit

/// The chosen theme as the widgets and the Live Activity draw it.
///
/// The extension does not link TaktCore, so it cannot resolve a theme
/// file itself. The app resolves the chosen theme and writes the result here —
/// both appearances' colours as hex, the lock if the theme has one, and the
/// faces and control radius — whenever the choice changes, then reloads the
/// timelines. With no file, or one that does not decode, the widgets draw
/// Chalk.
struct WidgetTheme: Codable, Equatable, Sendable {
  var identifier: String
  /// `"light"` or `"dark"` for a one-appearance theme such as Chalk Dark.
  var lockedAppearance: String?
  /// Role name to `#rrggbbaa`, per appearance.
  var light: [String: String]
  var dark: [String: String]
  var bodyFamily: String?
  var monoFamily: String?
  var controlRadius: Double
  var hairline: Double

  static var url: URL {
    AppGroup.containerURL.appending(path: "Priority/widget-theme.json", directoryHint: .notDirectory)
  }

  static func load() -> WidgetTheme? {
    guard let data = try? Data(contentsOf: url) else { return nil }
    return try? JSONDecoder().decode(WidgetTheme.self, from: data)
  }

  func write() throws {
    let url = Self.url
    try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    try encoder.encode(self).write(to: url, options: .atomic)
  }

  /// The default (Zed One, as on the Mac) and the fallback: the hexes of
  /// `one_palette()` in core/src/theme/builtins.rs, its faces, and its
  /// control radius.
  static let fallback = WidgetTheme(
    identifier: "native.theme.one",
    lockedAppearance: nil,
    light: [
      "paper": "#fafafa", "raised": "#ebebec", "well": "#dcdcdd", "border": "#c9c9ca",
      "ink": "#242529", "mutedText": "#58585a", "dimText": "#7e8086",
      "primary": "#5c78e2", "success": "#669f59", "danger": "#d36151", "warning": "#a48819",
    ],
    dark: [
      "paper": "#282c33", "raised": "#2f343e", "well": "#3b414d", "border": "#464b57",
      "ink": "#dce0e5", "mutedText": "#a9afbc", "dimText": "#878a98",
      "primary": "#74ade8", "success": "#a1c181", "danger": "#d07277", "warning": "#dec184",
    ],
    bodyFamily: "IBM Plex Sans", monoFamily: "Lilex", controlRadius: 4, hairline: 1)
}

/// The colours, faces and radius a widget view draws with, read from the
/// file the app wrote.
struct WidgetPalette {
  let theme: WidgetTheme

  /// The theme the app last wrote, or the default.
  static var current: WidgetPalette { WidgetPalette(theme: WidgetTheme.load() ?? .fallback) }

  var paper: Color { color("paper") }
  var raised: Color { color("raised") }
  var well: Color { color("well") }
  var border: Color { color("border") }
  var ink: Color { color("ink") }
  var muted: Color { color("mutedText") }
  var dim: Color { color("dimText") }
  var primary: Color { color("primary") }
  var success: Color { color("success") }
  var danger: Color { color("danger") }
  var warning: Color { color("warning") }
  var controlRadius: CGFloat { theme.controlRadius }
  var hairline: CGFloat { theme.hairline }

  /// A role, flipping with the appearance unless the theme is locked to one.
  func color(_ role: String) -> Color {
    let fallback = WidgetTheme.fallback
    let light = Self.parse(theme.light[role]) ?? Self.parse(fallback.light[role]) ?? .clear
    let dark = Self.parse(theme.dark[role]) ?? Self.parse(fallback.dark[role]) ?? light
    switch theme.lockedAppearance {
    case "light": return Color(uiColor: light)
    case "dark": return Color(uiColor: dark)
    default:
      return Color(uiColor: UIColor { $0.userInterfaceStyle == .dark ? dark : light })
    }
  }

  func sans(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
    face(theme.bodyFamily, size, weight, design: .default)
  }

  func mono(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
    face(theme.monoFamily, size, weight, design: .monospaced).monospacedDigit()
  }

  private func face(_ family: String?, _ size: CGFloat, _ weight: Font.Weight, design: Font.Design) -> Font {
    if let family, UIFont.familyNames.contains(family) {
      return .custom(family, size: size).weight(weight)
    }
    return .system(size: size, weight: weight, design: design)
  }

  /// `#rgb`, `#rrggbb` or `#rrggbbaa`.
  static func parse(_ hex: String?) -> UIColor? {
    guard var digits = hex?.trimmingCharacters(in: .whitespaces) else { return nil }
    if digits.hasPrefix("#") { digits.removeFirst() }
    if digits.count == 3 { digits = digits.map { "\($0)\($0)" }.joined() }
    if digits.count == 6 { digits += "ff" }
    guard digits.count == 8, let value = UInt64(digits, radix: 16) else { return nil }
    return UIColor(
      red: CGFloat((value >> 24) & 0xff) / 255, green: CGFloat((value >> 16) & 0xff) / 255,
      blue: CGFloat((value >> 8) & 0xff) / 255, alpha: CGFloat(value & 0xff) / 255)
  }
}

enum WidgetType {
  /// `25m`, `1h 30m`.
  static func duration(_ seconds: Int) -> String {
    let minutes = max(0, seconds) / 60
    if minutes < 60 { return "\(minutes)m" }
    let hours = minutes / 60
    let rest = minutes % 60
    return rest == 0 ? "\(hours)h" : "\(hours)h \(rest)m"
  }
}

private struct WidgetPaletteKey: EnvironmentKey {
  static let defaultValue = WidgetPalette(theme: .fallback)
}

extension EnvironmentValues {
  /// The palette a widget view draws with, set at the top of each widget from
  /// the file the app wrote.
  var widgetPalette: WidgetPalette {
    get { self[WidgetPaletteKey.self] }
    set { self[WidgetPaletteKey.self] = newValue }
  }
}
