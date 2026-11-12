import TaktCore
import SwiftUI
import UIKit

/// The theme every view draws through, read as `@Environment(\.theme)`.
///
/// A projection of a `ThemeSpecification` resolved for the iPhone: colour
/// roles as dynamic colours (they flip with the trait collection, so light and
/// dark need no redraw of their own), the structure's radii, rules, spacing and
/// type, and the minimum hit target. `ThemeStore` builds a new one whenever
/// the chosen theme changes, and the environment carries it to every view, so
/// a change of theme redraws live.
struct Theme {
  /// How far something that cannot be acted on yet steps back — a blocked
  /// task's whole row. The Mac's value, so the two read alike.
  static let disabledOpacity = 0.45

  let specification: ThemeSpecification
  let structure: ThemeStructure
  let touchTarget: CGFloat
  let radius: Radii
  let space: Spacing
  let type: TypeSet
  /// The default rule. A theme's hairline is in pixels, the way the house
  /// style states it ("1px borders"), so 1 is one device pixel.
  let hairline: CGFloat
  /// The same rule in points, for an outline that has to read at a glance —
  /// a card, a drop target, a stepper's frame.
  let stroke: CGFloat
  /// A selected or active edge.
  let emphasis: CGFloat
  /// The focus ring, drawn in `primary`.
  let focusRing: CGFloat
  /// The Mac's measurements, scaled to the phone's type.
  let mac: MacMetrics
  /// The theme paints its own status bar, so words on it are set in ink
  /// rather than muted text (Zed's rule).
  let statesStatusBar: Bool

  private let colors: [ThemeColorRole: Color]
  private let uiColors: [ThemeColorRole: UIColor]
  private let chromeColors: [ThemeChromeRole: Color]
  private let chromeUIColors: [ThemeChromeRole: UIColor]

  /// `specification` must already be resolved for `.ios` — the built-ins
  /// through `BuiltInThemeSpecifications.all(for: .ios)`, files through
  /// `ThemeFileLoader.load(_:platform: .ios)` — so its structure is the
  /// phone's.
  init(specification: ThemeSpecification, displayScale: CGFloat) {
    self.specification = specification
    let structure = specification.structure
    self.structure = structure
    self.touchTarget = structure.touchTarget
    radius = Radii(
      panel: structure.radius.panel, row: structure.radius.row, control: structure.radius.control,
      pill: structure.radius.pill)
    space = Spacing(structure.spacing)
    type = TypeSet(structure.typography)
    let scale = max(1, displayScale)
    hairline = structure.border.hairline / scale
    stroke = structure.border.hairline
    emphasis = structure.border.emphasis
    focusRing = structure.border.focusRing
    var uiColors: [ThemeColorRole: UIColor] = [:]
    for role in ThemeColorRole.allCases {
      uiColors[role] = Self.dynamic(role, of: specification)
    }
    self.uiColors = uiColors
    colors = uiColors.mapValues { Color(uiColor: $0) }
    var chromeUIColors: [ThemeChromeRole: UIColor] = [:]
    for role in ThemeChromeRole.allCases {
      chromeUIColors[role] = Self.dynamic(role, of: specification)
    }
    self.chromeUIColors = chromeUIColors
    chromeColors = chromeUIColors.mapValues { Color(uiColor: $0) }
    statesStatusBar = specification.palette.states(.statusBar)
    mac = MacMetrics(body: structure.typography.scale.body)
  }

  /// The default theme, as the iPhone draws it: what the environment holds
  /// before a store has said otherwise, and what previews and tests see.
  static let standard = Theme(
    specification: BuiltInThemeSpecifications.defaultTheme(for: .ios), displayScale: 3)

  /// The appearance this theme insists on, if it is a one-appearance theme
  /// such as Chalk Dark.
  var lockedColorScheme: ColorScheme? {
    switch specification.lockedAppearance {
    case .light: .light
    case .dark: .dark
    case nil: nil
    }
  }

  func color(_ role: ThemeColorRole) -> Color { colors[role] ?? .clear }
  func uiColor(_ role: ThemeColorRole) -> UIColor { uiColors[role] ?? .clear }
  func chrome(_ role: ThemeChromeRole) -> Color { chromeColors[role] ?? .clear }
  func uiChrome(_ role: ThemeChromeRole) -> UIColor { chromeUIColors[role] ?? .clear }

  // MARK: Roles

  var paper: Color { color(.paper) }
  var raised: Color { color(.raised) }
  var altRow: Color { color(.altRow) }
  var hover: Color { color(.hover) }
  var well: Color { color(.well) }
  var border: Color { color(.border) }
  var borderMuted: Color { color(.borderMuted) }
  var inputBorder: Color { color(.inputBorder) }
  var ink: Color { color(.ink) }
  var muted: Color { color(.mutedText) }
  var dim: Color { color(.dimText) }
  var primary: Color { color(.primary) }
  var success: Color { color(.success) }
  var danger: Color { color(.danger) }
  var warning: Color { color(.warning) }
  var purple: Color { color(.categoricalPurple) }
  var pink: Color { color(.categoricalPink) }
  var orange: Color { color(.categoricalOrange) }
  /// Text and glyphs set on a filled accent (a primary button, a done
  /// checkbox). Theme-invariant, like the media roles it is read from.
  var onAccent: Color { color(.mediaScrimInk) }

  // MARK: Chrome — Zed's, stated by the theme or derived by the core

  /// The selected row's fill.
  var selected: Color { chrome(.selected) }
  /// The hairlines under a tree's levels.
  var indentGuide: Color { chrome(.indentGuide) }
  /// The status bar's fill — on the phone, the tab bar's.
  var statusBar: Color { chrome(.statusBar) }
  /// The outline round the row that has the keyboard (the inspector's row).
  var focus: Color { chrome(.focus) }
  /// An icon at rest.
  var icon: Color { chrome(.icon) }
  /// Words on the status bar: ink on a painted one, else muted.
  var statusBarText: Color { statesStatusBar ? ink : muted }

  private static func dynamic(_ role: ThemeColorRole, of specification: ThemeSpecification) -> UIColor {
    // Media roles are read from the light table whatever is in force.
    let invariant = ThemeColorRole.themeInvariant.contains(role)
    let light = specification.palette.color(role, in: .light)
    let dark = invariant ? light : specification.palette.color(role, in: .dark)
    return UIColor { traits in
      let value = traits.userInterfaceStyle == .dark ? dark : light
      return UIColor(red: value.red, green: value.green, blue: value.blue, alpha: value.alpha)
    }
  }

  private static func dynamic(_ role: ThemeChromeRole, of specification: ThemeSpecification) -> UIColor {
    let light = specification.palette.chrome(role, in: .light)
    let dark = specification.palette.chrome(role, in: .dark)
    return UIColor { traits in
      let value = traits.userInterfaceStyle == .dark ? dark : light
      return UIColor(red: value.red, green: value.green, blue: value.blue, alpha: value.alpha)
    }
  }

  // MARK: Structure

  /// The Mac app's measurements (desktop/src/ui), scaled by the phone's body
  /// against the Mac's 13: a 26px row is a 30pt row here. Painted sizes only;
  /// a hit area is still the touch target.
  struct MacMetrics: Equatable {
    /// Phone points per Mac pixel.
    let k: CGFloat

    init(body: CGFloat) { k = max(0.5, body / 13) }

    /// A Mac size in phone points, to the half point.
    func callAsFunction(_ px: CGFloat) -> CGFloat { (px * k * 2).rounded() / 2 }

    /// A row: the outline's, the sidebar's, a menu's.
    var row: CGFloat { self(26) }
    /// A control: a button, a dropdown, a field.
    var control: CGFloat { self(26) }
    /// A settings row's least height.
    var settingsRow: CGFloat { self(38) }
    /// A menu item.
    var menuItem: CGFloat { self(24) }
    /// The title bar, which the navigation bar stands for.
    var titleBar: CGFloat { self(34) }
    /// The status bar, which the tab bar stands for.
    var statusBar: CGFloat { self(30) }
    /// One outline level.
    var indent: CGFloat { self(18) }
    /// The fold arrow's column.
    var disclosure: CGFloat { self(10) }
    /// The status mark's column.
    var mark: CGFloat { self(12) }

    /// Rows, cards, buttons, tabs, key hints, inline fields.
    let radiusRow: CGFloat = 4
    /// Board columns, menus, tooltips, settings controls, popovers.
    let radiusMenu: CGFloat = 6
    /// Dialogs and palettes.
    let radiusDialog: CGFloat = 8
  }

  struct Radii: Equatable {
    /// Cards, panels, popovers and sheets' inner panels.
    let panel: CGFloat
    /// A list row's selection and hover.
    let row: CGFloat
    /// Buttons, inputs, chips, tooltips.
    let control: CGFloat
    /// Genuinely round things.
    let pill: CGFloat

    /// A tag or badge set inside a control: a step tighter than the control.
    var tag: CGFloat { max(0, control - 2) }
  }

  struct Spacing: Equatable {
    let xxs: CGFloat
    let xs: CGFloat
    let sm: CGFloat
    let md: CGFloat
    let lg: CGFloat
    let xl: CGFloat

    init(_ scale: ThemeSpacingScale) {
      xxs = scale.xxs
      xs = scale.xs
      sm = scale.sm
      md = scale.md
      lg = scale.lg
      xl = scale.xl
    }
  }
}

extension Theme: Equatable {
  static func == (lhs: Theme, rhs: Theme) -> Bool {
    lhs.specification == rhs.specification && lhs.structure == rhs.structure
      && lhs.touchTarget == rhs.touchTarget && lhs.hairline == rhs.hairline
  }
}

// MARK: - Type

extension Theme {
  /// The theme's faces and its type scale, as the fonts views ask for by role.
  ///
  /// The named fonts are the iPhone's interface roles, each taken from the
  /// theme's scale: `body`, `caption`, `title`, `display` and `hero` are the
  /// scale itself; `callout` and `footnote` sit a step either side. Every font
  /// is relative to a Dynamic Type text style, so the interface still follows
  /// the user's text size. `sans(_:)`, `mono(_:)` and `glyph(_:)` are for the
  /// few sizes that belong to one layout: they are stated against the
  /// phone's 15pt body and scale with the theme's. `secondary` and
  /// `groupLabel` are the Mac's 11 and 10, at the phone's scale.
  struct TypeSet: Equatable {
    let body: Font
    let bodyMedium: Font
    let callout: Font
    let caption: Font
    let footnote: Font
    let title: Font
    let largeTitle: Font
    let numeral: Font
    let numeralBody: Font
    let display: Font
    let hero: Font
    let microLabel: MicroLabel
    /// The Mac's 11px secondary text — chips, counts, status lines, section
    /// headings — at the phone's scale.
    let secondary: Font
    let secondaryMedium: Font
    let secondaryMono: Font
    /// The Mac's 10px bold capitals over a settings group or a matrix
    /// quadrant.
    let groupLabel: Font
    let scale: ThemeTypeScale

    private let bodyFace: ResolvedFace
    private let displayFace: ResolvedFace
    private let monoFace: ResolvedFace
    /// The theme's body size against the reference.
    private let factor: CGFloat

    /// The iPhone's body size in the default theme, which layout-specific
    /// sizes are stated against.
    static let referenceBody: CGFloat = 15
    /// The Mac's body, which its secondary and label sizes are stated
    /// against.
    static let macBody: CGFloat = 13

    init(_ typography: ThemeTypography) {
      let scale = typography.scale
      self.scale = scale
      bodyFace = ResolvedFace(typography.body)
      displayFace = ResolvedFace(typography.display)
      monoFace = ResolvedFace(typography.mono)
      factor = max(0.5, scale.body / Self.referenceBody)
      let callout = (scale.body - 1).rounded()
      body = bodyFace.font(scale.body, .regular, .body)
      bodyMedium = bodyFace.font(scale.body, .medium, .body)
      self.callout = bodyFace.font(callout, .regular, .callout)
      caption = bodyFace.font(scale.caption, .regular, .caption)
      footnote = bodyFace.font(scale.caption - 1, .regular, .footnote)
      // Pane headings, big numbers and the focus timer are the scale's
      // title, display and hero, as stated.
      title = displayFace.font(scale.title, .medium, .title3)
      largeTitle = displayFace.font(scale.display, .medium, .largeTitle)
      numeral = monoFace.font(scale.caption, .regular, .caption)
      numeralBody = monoFace.font(callout, .regular, .body)
      display = monoFace.font(scale.display, .medium, .largeTitle)
      hero = monoFace.font(scale.hero, .medium, .largeTitle)
      microLabel = MicroLabel(typography.microLabel, face: bodyFace)
      let k = scale.body / Self.macBody
      let secondarySize = ((11 * k) * 2).rounded() / 2
      secondary = bodyFace.font(secondarySize, .regular, .footnote)
      secondaryMedium = bodyFace.font(secondarySize, .medium, .footnote)
      secondaryMono = monoFace.font(secondarySize, .regular, .footnote)
      groupLabel = bodyFace.font(((10 * k) * 2).rounded() / 2, .bold, .caption2)
    }

    /// The body face at a size stated against the 15pt body.
    func sans(_ size: CGFloat, _ weight: Font.Weight = .regular, relativeTo style: Font.TextStyle = .body) -> Font {
      bodyFace.font(size * factor, weight, style)
    }

    /// The mono face at a size stated against the 15pt body.
    func mono(_ size: CGFloat, _ weight: Font.Weight = .regular, relativeTo style: Font.TextStyle = .body) -> Font {
      monoFace.font(size * factor, weight, style)
    }

    /// An SF Symbol at a size stated against the 15pt body.
    func glyph(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
      .system(size: size * factor, weight: weight)
    }

    /// The body or mono face as a UIKit font, scaled for Dynamic Type — for
    /// the UIKit chrome.
    func uiFont(_ size: CGFloat, weight: UIFont.Weight = .regular, mono: Bool = false) -> UIFont {
      (mono ? monoFace : bodyFace).uiFont(size, weight: weight)
    }

    static func == (lhs: TypeSet, rhs: TypeSet) -> Bool {
      lhs.scale == rhs.scale && lhs.microLabel == rhs.microLabel && lhs.bodyFace == rhs.bodyFace
        && lhs.displayFace == rhs.displayFace && lhs.monoFace == rhs.monoFace
    }
  }

  /// The small label on section headers, column heads and chips.
  struct MicroLabel: Equatable {
    let font: Font
    let tracking: CGFloat
    let isUppercased: Bool
    let role: ThemeColorRole
    private let specification: ThemeMicroLabel

    init(_ label: ThemeMicroLabel, face: ResolvedFace) {
      specification = label
      font = face.font(label.size, label.weight.fontWeight, .caption)
      tracking = label.trackingPoints
      isUppercased = label.isUppercased
      role = label.role
    }

    static func == (lhs: MicroLabel, rhs: MicroLabel) -> Bool { lhs.specification == rhs.specification }
  }

  /// A face request settled against what is installed: the first named
  /// family the app can draw, or the design's system face.
  struct ResolvedFace: Equatable {
    let family: String?
    let design: Font.Design

    private static let installedFamilies = Set(UIFont.familyNames)

    init(_ face: ThemeFontFace) {
      family = face.families.first { Self.installedFamilies.contains($0) }
      design = face.design.fontDesign
    }

    func font(_ size: CGFloat, _ weight: Font.Weight, _ style: Font.TextStyle) -> Font {
      if let family { return .custom(family, size: size, relativeTo: style).weight(weight) }
      return Font(UIFontMetrics(forTextStyle: style.uiTextStyle).scaledFont(for: systemFont(size, weight.uiWeight)))
        .weight(weight)
    }

    func uiFont(_ size: CGFloat, weight: UIFont.Weight) -> UIFont {
      let font: UIFont
      if let family {
        let descriptor = UIFontDescriptor(fontAttributes: [
          .family: family, .traits: [UIFontDescriptor.TraitKey.weight: weight],
        ])
        font = UIFont(descriptor: descriptor, size: size)
      } else {
        font = systemFont(size, weight)
      }
      return UIFontMetrics.default.scaledFont(for: font)
    }

    private func systemFont(_ size: CGFloat, _ weight: UIFont.Weight) -> UIFont {
      let base = UIFont.systemFont(ofSize: size, weight: weight)
      let systemDesign: UIFontDescriptor.SystemDesign =
        switch design {
        case .serif: .serif
        case .monospaced: .monospaced
        case .rounded: .rounded
        default: .default
        }
      guard let descriptor = base.fontDescriptor.withDesign(systemDesign) else { return base }
      return UIFont(descriptor: descriptor, size: size)
    }
  }
}

extension ThemeFontDesign {
  var fontDesign: Font.Design {
    switch self {
    case .serif: .serif
    case .sans: .default
    case .monospaced: .monospaced
    case .rounded: .rounded
    }
  }
}

extension ThemeFontWeight {
  var fontWeight: Font.Weight {
    switch self {
    case .regular: .regular
    case .medium: .medium
    case .semibold: .semibold
    case .bold: .bold
    case .black: .black
    }
  }
}

extension Font.Weight {
  var uiWeight: UIFont.Weight {
    switch self {
    case .ultraLight: .ultraLight
    case .thin: .thin
    case .light: .light
    case .medium: .medium
    case .semibold: .semibold
    case .bold: .bold
    case .heavy: .heavy
    case .black: .black
    default: .regular
    }
  }
}

extension Font.TextStyle {
  var uiTextStyle: UIFont.TextStyle {
    switch self {
    case .largeTitle: .largeTitle
    case .title: .title1
    case .title2: .title2
    case .title3: .title3
    case .headline: .headline
    case .subheadline: .subheadline
    case .callout: .callout
    case .footnote: .footnote
    case .caption: .caption1
    case .caption2: .caption2
    default: .body
    }
  }
}

// MARK: - Environment

private struct ThemeKey: EnvironmentKey {
  static let defaultValue = Theme.standard
}

extension EnvironmentValues {
  var theme: Theme {
    get { self[ThemeKey.self] }
    set { self[ThemeKey.self] = newValue }
  }
}

// MARK: - Layout constants

/// Sizes that belong to one layout rather than to the theme.
enum Metrics {
  /// Indent per outline level: the Mac's 18.
  static let indent: CGFloat = 18
}

/// The colours a list can be given, in the order the picker offers them.
/// These are data — a list keeps its hex — not theme roles.
enum ListColor {
  static let choices: [(name: String, hex: String)] = [
    ("Azure", "#007fff"), ("Emerald", "#4cc38e"), ("Raspberry", "#d62246"), ("Amber", "#ffbf00"),
    ("Purple", "#7a4de8"), ("Pink", "#ff88dc"), ("Orange", "#ff6b2b"), ("Grape", "#444054"),
  ]

  static func color(hex: String?) -> Color? {
    guard let hex, let value = ThemeColorValue(hex: hex) else { return nil }
    return Color(red: value.red, green: value.green, blue: value.blue, opacity: value.alpha)
  }
}

// MARK: - Hit targets

extension View {
  /// Grows the tappable area to the theme's touch target without growing
  /// what is painted or how it is laid out: only the content shape reaches
  /// past the control's edges.
  func hitTarget() -> some View { modifier(HitTargetModifier()) }
}

private struct HitTargetModifier: ViewModifier {
  @Environment(\.theme) private var theme

  func body(content: Content) -> some View {
    content.contentShape(HitArea(minimum: theme.touchTarget))
  }
}

/// A rectangle at least `minimum` on each side, centred on the view it
/// shapes. Zero (a pointer platform) leaves the view's own bounds.
struct HitArea: Shape {
  var minimum: CGFloat

  func path(in rect: CGRect) -> Path {
    Path(
      rect.insetBy(
        dx: -max(0, (minimum - rect.width) / 2), dy: -max(0, (minimum - rect.height) / 2)))
  }
}
