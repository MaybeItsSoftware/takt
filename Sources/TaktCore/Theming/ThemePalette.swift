import Foundation

/// Which of a palette's two tables is in force.
///
/// Not "the user's preference" — that is a three-way choice including *system*,
/// and it is resolved before it gets here. A palette only ever knows light or
/// dark.
public enum ThemeAppearance: String, CaseIterable, Sendable, Codable {
  case light
  case dark

  public var opposite: ThemeAppearance { self == .light ? .dark : .light }
}

/// The semantic roles a component is allowed to name.
///
/// The palette layer is literal hex; this is the layer components touch. The
/// split is the point: a view asks for `.border`, never for `#e6e4ea`, so a
/// theme swap and an appearance flip are the same mechanism.
public enum ThemeColorRole: String, CaseIterable, Sendable, Codable {
  // MARK: Neutral spine

  /// The page.
  case paper
  /// Cards and panels, half a step above the paper.
  case raised
  /// Alternating rows in a list or table.
  case altRow
  /// Hover feedback on a row or control.
  case hover
  /// Wells, chips and inset containers.
  case well
  /// The hairline that does the separating.
  case border
  /// A softer hairline, for dividers inside a surface.
  case borderMuted
  /// The stronger hairline a text field or input needs to read as editable.
  case inputBorder
  /// Body text and iconography.
  case ink
  /// Secondary text: captions, micro-labels, status lines.
  case mutedText
  /// Tertiary text: placeholders, disabled glyphs. Not for anything you have
  /// to read.
  case dimText

  // MARK: Status — the fixed four-way convention

  /// Actions, links, focus rings, selection, "info".
  case primary
  case success
  case danger
  case warning

  // MARK: Categorical extras — identity colour only, never chrome

  case categoricalPurple
  case categoricalPink
  case categoricalOrange

  // MARK: Theme-invariant media surfaces

  /// Letterbox behind a photo. Fixed, because the content underneath isn't
  /// ours to theme.
  case mediaLetterbox
  /// The scrim under chrome that sits on an image.
  case mediaScrim
  /// Text on that scrim.
  case mediaScrimInk

  /// Roles that do not flip with the appearance.
  ///
  /// Rule 5 of the house style, expressed as data so it cannot be observed in
  /// one component and forgotten in the next.
  public static let themeInvariant: Set<ThemeColorRole> = [
    .mediaLetterbox, .mediaScrim, .mediaScrimInk,
  ]

  /// Roles a paragraph of running text may be set in, and which therefore have
  /// to clear 4.5:1 against the surface behind them.
  public static let bodyTextRoles: [ThemeColorRole] = [.ink, .mutedText]

  public var isThemeInvariant: Bool { Self.themeInvariant.contains(self) }
}

/// Two tables of literal colour, one per appearance, plus the rules for
/// reading them.
public struct ThemePalette: Equatable, Sendable {
  public let light: [ThemeColorRole: ThemeColorValue]
  public let dark: [ThemeColorRole: ThemeColorValue]
  /// The chrome roles the theme states, per appearance. Read them through
  /// `chrome(_:in:)`, which derives the ones left out.
  public let chromeLight: [ThemeChromeRole: ThemeColorValue]
  public let chromeDark: [ThemeChromeRole: ThemeColorValue]

  public init(
    light: [ThemeColorRole: ThemeColorValue], dark: [ThemeColorRole: ThemeColorValue],
    chromeLight: [ThemeChromeRole: ThemeColorValue] = [:], chromeDark: [ThemeChromeRole: ThemeColorValue] = [:]
  ) {
    self.light = light
    self.dark = dark
    self.chromeLight = chromeLight
    self.chromeDark = chromeDark
  }

  /// Resolution is total, and in this order:
  ///
  /// 1. A theme-invariant role always answers from the `light` table, whatever
  ///    the appearance — that is what invariant means.
  /// 2. Otherwise the appearance's own table.
  /// 3. Otherwise the other table, so a half-written theme degrades to
  ///    something visible rather than to nothing.
  /// 4. Otherwise `ThemeColorValue.unresolved`, which `validate()` exists to
  ///    stop anyone ever seeing.
  public func color(_ role: ThemeColorRole, in appearance: ThemeAppearance) -> ThemeColorValue {
    if role.isThemeInvariant {
      return light[role] ?? dark[role] ?? .unresolved
    }
    return table(appearance)[role] ?? table(appearance.opposite)[role] ?? .unresolved
  }

  public func table(_ appearance: ThemeAppearance) -> [ThemeColorRole: ThemeColorValue] {
    appearance == .light ? light : dark
  }

  /// Roles absent from `appearance`'s own table. Invariant roles are only
  /// expected in the light table, so they are never reported missing from the
  /// dark one.
  public func missingRoles(in appearance: ThemeAppearance) -> [ThemeColorRole] {
    ThemeColorRole.allCases
      .filter { role in
        if role.isThemeInvariant { return appearance == .light && light[role] == nil }
        return table(appearance)[role] == nil
      }
      .sorted { $0.rawValue < $1.rawValue }
  }
}

/// Zed's chrome, which a theme may state and need not: the selected row, the
/// hairlines under a tree's levels, the status bar, the outline round the row
/// that has the keyboard, and an icon at rest. A role a theme leaves out is
/// derived from the others by the Rust core (`CoreThemePalette::chrome`).
public enum ThemeChromeRole: String, CaseIterable, Sendable, Codable {
  case selected
  case indentGuide
  case statusBar
  case focus
  case icon
}
