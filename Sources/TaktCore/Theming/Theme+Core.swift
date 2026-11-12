import Foundation
import TaktRustCore

// The theme format is the Rust core's (core/src/theme), which Android calls
// too. These are the conversions between TaktCore's theme vocabulary, which
// the apps render through, and the core's records. Every one is a whole
// theme or file at a time: resolution happens when a theme loads, never per
// colour in a view body.

extension ThemeColorValue {
  var core: CoreThemeColor { CoreThemeColor(red: red, green: green, blue: blue, alpha: alpha) }

  init(_ core: CoreThemeColor) {
    self.init(red: core.red, green: core.green, blue: core.blue, alpha: core.alpha)
  }
}

extension ThemeAppearance {
  var core: CoreThemeAppearance { self == .light ? .light : .dark }

  init(_ core: CoreThemeAppearance) {
    self = core == .light ? .light : .dark
  }
}

extension ThemePlatform {
  var core: CoreThemePlatform {
    switch self {
    case .macos: .macos
    case .ios: .ios
    case .android: .android
    }
  }

  init(_ core: CoreThemePlatform) {
    switch core {
    case .macos: self = .macos
    case .ios: self = .ios
    case .android: self = .android
    }
  }
}

extension ThemeIssueSeverity {
  var core: CoreThemeIssueSeverity {
    switch self {
    case .error: .error
    case .warning: .warning
    case .note: .note
    }
  }

  init(_ core: CoreThemeIssueSeverity) {
    switch core {
    case .error: self = .error
    case .warning: self = .warning
    case .note: self = .note
    }
  }
}

// MARK: - Palette and structure

extension ThemePalette {
  var core: CoreThemePalette {
    func table(
      _ values: [ThemeColorRole: ThemeColorValue], _ chrome: [ThemeChromeRole: ThemeColorValue]
    ) -> [String: CoreThemeColor] {
      var table = Dictionary(uniqueKeysWithValues: values.map { ($0.key.rawValue, $0.value.core) })
      for (role, value) in chrome { table[role.rawValue] = value.core }
      return table
    }
    return CoreThemePalette(light: table(light, chromeLight), dark: table(dark, chromeDark))
  }

  init(_ core: CoreThemePalette) {
    func table(_ values: [String: CoreThemeColor]) -> [ThemeColorRole: ThemeColorValue] {
      Dictionary(
        uniqueKeysWithValues: values.compactMap { key, value in
          ThemeColorRole(rawValue: key).map { ($0, ThemeColorValue(value)) }
        })
    }
    func chrome(_ values: [String: CoreThemeColor]) -> [ThemeChromeRole: ThemeColorValue] {
      Dictionary(
        uniqueKeysWithValues: values.compactMap { key, value in
          ThemeChromeRole(rawValue: key).map { ($0, ThemeColorValue(value)) }
        })
    }
    self.init(
      light: table(core.light), dark: table(core.dark), chromeLight: chrome(core.light),
      chromeDark: chrome(core.dark))
  }

  /// A chrome role, opaque: as the theme states it, else derived the way Zed
  /// draws a theme that names none. The Rust core's `theme_chrome_color`.
  public func chrome(_ role: ThemeChromeRole, in appearance: ThemeAppearance) -> ThemeColorValue {
    ThemeColorValue(themeChromeColor(palette: core, role: role.rawValue, appearance: appearance.core))
  }

  /// Whether the theme states `role` itself rather than leaving it derived:
  /// a painted status bar, say, sets its words in ink.
  public func states(_ role: ThemeChromeRole) -> Bool {
    themeStatesRole(palette: core, role: role.rawValue)
  }
}

extension ThemeFontFace {
  var core: CoreThemeFontFace { CoreThemeFontFace(families: families, design: design.rawValue) }

  init(_ core: CoreThemeFontFace) {
    self.init(families: core.families, design: ThemeFontDesign(rawValue: core.design) ?? .sans)
  }
}

extension ThemeTypeScale {
  var core: CoreThemeTypeScale {
    CoreThemeTypeScale(caption: caption, body: body, title: title, display: display, hero: hero)
  }

  init(_ core: CoreThemeTypeScale) {
    self.init(caption: core.caption, body: core.body, title: core.title, display: core.display, hero: core.hero)
  }
}

extension ThemeStructure {
  var core: CoreThemeStructure {
    let type = typography
    return CoreThemeStructure(
      radius: CoreThemeRadius(
        panel: radius.panel, row: radius.row, control: radius.control, pill: radius.pill, shell: radius.shell),
      border: CoreThemeBorder(hairline: border.hairline, emphasis: border.emphasis, focusRing: border.focusRing),
      spacing: CoreThemeSpacing(
        xxs: spacing.xxs, xs: spacing.xs, sm: spacing.sm, md: spacing.md, lg: spacing.lg, xl: spacing.xl),
      typography: CoreThemeTypography(
        display: type.display.core, body: type.body.core, mono: type.mono.core, bodySize: type.bodySize,
        scale: type.scale.core,
        microLabel: CoreThemeMicroLabel(
          size: type.microLabel.size, weight: type.microLabel.weight.rawValue, tracking: type.microLabel.tracking,
          isUppercased: type.microLabel.isUppercased, role: type.microLabel.role.rawValue)),
      touchTarget: touchTarget, usesShadows: usesShadows, usesGradientsOnChrome: usesGradientsOnChrome)
  }

  init(_ core: CoreThemeStructure) {
    let type = core.typography
    self.init(
      radius: ThemeRadiusScale(
        panel: core.radius.panel, row: core.radius.row, control: core.radius.control, pill: core.radius.pill,
        shell: core.radius.shell),
      border: ThemeBorderScale(
        hairline: core.border.hairline, emphasis: core.border.emphasis, focusRing: core.border.focusRing),
      spacing: ThemeSpacingScale(
        xxs: core.spacing.xxs, xs: core.spacing.xs, sm: core.spacing.sm, md: core.spacing.md,
        lg: core.spacing.lg, xl: core.spacing.xl),
      typography: ThemeTypography(
        display: ThemeFontFace(type.display), body: ThemeFontFace(type.body), mono: ThemeFontFace(type.mono),
        bodySize: type.bodySize, scale: ThemeTypeScale(type.scale),
        microLabel: ThemeMicroLabel(
          size: type.microLabel.size, weight: ThemeFontWeight(rawValue: type.microLabel.weight) ?? .regular,
          tracking: type.microLabel.tracking, isUppercased: type.microLabel.isUppercased,
          role: ThemeColorRole(rawValue: type.microLabel.role) ?? .mutedText)),
      touchTarget: core.touchTarget, usesShadows: core.usesShadows,
      usesGradientsOnChrome: core.usesGradientsOnChrome)
  }
}

extension ThemeSpecification {
  var core: CoreThemeSpecification {
    CoreThemeSpecification(
      identifier: identifier, name: name, summary: summary, lockedAppearance: lockedAppearance?.core,
      palette: palette.core, structure: structure.core)
  }

  init(_ core: CoreThemeSpecification) {
    self.init(
      identifier: core.identifier, name: core.name, summary: core.summary,
      lockedAppearance: core.lockedAppearance.map(ThemeAppearance.init), palette: ThemePalette(core.palette),
      structure: ThemeStructure(core.structure))
  }
}

// MARK: - Issues

extension ThemeIssue {
  var core: CoreThemeIssue {
    switch self {
    case .missingRole(let role, let appearance):
      .missingRole(role: role.rawValue, appearance: appearance.core)
    case .bodyTextBelowAA(let role, let appearance, let ratio):
      .bodyTextBelowAa(role: role.rawValue, appearance: appearance.core, ratio: ratio)
    case .largeTextOnly(let role, let appearance, let ratio):
      .largeTextOnly(role: role.rawValue, appearance: appearance.core, ratio: ratio)
    case .accentBelowUIMinimum(let role, let appearance, let ratio):
      .accentBelowUiMinimum(role: role.rawValue, appearance: appearance.core, ratio: ratio)
    case .raisedIndistinctFromPaper(let appearance, let ratio):
      .raisedIndistinctFromPaper(appearance: appearance.core, ratio: ratio)
    case .shadowsUsed: .shadowsUsed
    case .gradientsOnChrome: .gradientsOnChrome
    case .radiusScaleOutOfOrder: .radiusScaleOutOfOrder
    case .shellRadiusOffScale(let value): .shellRadiusOffScale(value: value)
    case .hairlineTooHeavy(let value): .hairlineTooHeavy(value: value)
    case .touchTargetTooSmall(let value): .touchTargetTooSmall(value: value)
    }
  }

  init(_ core: CoreThemeIssue) {
    func role(_ raw: String) -> ThemeColorRole { ThemeColorRole(rawValue: raw) ?? .paper }
    switch core {
    case .missingRole(let raw, let appearance):
      self = .missingRole(role: role(raw), appearance: ThemeAppearance(appearance))
    case .bodyTextBelowAa(let raw, let appearance, let ratio):
      self = .bodyTextBelowAA(role: role(raw), appearance: ThemeAppearance(appearance), ratio: ratio)
    case .largeTextOnly(let raw, let appearance, let ratio):
      self = .largeTextOnly(role: role(raw), appearance: ThemeAppearance(appearance), ratio: ratio)
    case .accentBelowUiMinimum(let raw, let appearance, let ratio):
      self = .accentBelowUIMinimum(role: role(raw), appearance: ThemeAppearance(appearance), ratio: ratio)
    case .raisedIndistinctFromPaper(let appearance, let ratio):
      self = .raisedIndistinctFromPaper(appearance: ThemeAppearance(appearance), ratio: ratio)
    case .shadowsUsed: self = .shadowsUsed
    case .gradientsOnChrome: self = .gradientsOnChrome
    case .radiusScaleOutOfOrder: self = .radiusScaleOutOfOrder
    case .shellRadiusOffScale(let value): self = .shellRadiusOffScale(value: value)
    case .hairlineTooHeavy(let value): self = .hairlineTooHeavy(value: value)
    case .touchTargetTooSmall(let value): self = .touchTargetTooSmall(value: value)
    }
  }
}

extension ThemeFileIssue {
  init(_ core: CoreThemeFileIssue) {
    self.init(
      source: core.source, severity: ThemeIssueSeverity(core.severity), message: core.message,
      isAudit: core.isAudit)
  }
}

extension ThemeFileOutcome {
  init(_ core: CoreThemeFileOutcome) {
    self.init(
      source: core.source, specification: core.specification.map(ThemeSpecification.init),
      skippedReason: core.skippedReason, issues: core.issues.map(ThemeFileIssue.init))
  }
}

// MARK: - The file

extension ThemeFile.Palette {
  var core: CoreThemeFilePalette { CoreThemeFilePalette(light: light, dark: dark) }
  init(_ core: CoreThemeFilePalette) { self.init(light: core.light, dark: core.dark) }
}

extension ThemeFile.Face {
  var core: CoreThemeFileFace { CoreThemeFileFace(families: families, design: design) }
  init(_ core: CoreThemeFileFace) { self.init(families: core.families, design: core.design) }
}

extension ThemeFile.Structure {
  var core: CoreThemeFileStructure {
    CoreThemeFileStructure(
      radius: radius.map {
        CoreThemeFileRadius(panel: $0.panel, row: $0.row, control: $0.control, pill: $0.pill, shell: $0.shell)
      },
      border: border.map { CoreThemeFileBorder(hairline: $0.hairline, emphasis: $0.emphasis, focusRing: $0.focusRing) },
      spacing: spacing.map {
        CoreThemeFileSpacing(xxs: $0.xxs, xs: $0.xs, sm: $0.sm, md: $0.md, lg: $0.lg, xl: $0.xl)
      },
      typography: typography.map { type in
        CoreThemeFileTypography(
          display: type.display?.core, body: type.body?.core, mono: type.mono?.core, bodySize: type.bodySize,
          scale: type.scale.map {
            CoreThemeFileTypeScale(
              caption: $0.caption, body: $0.body, title: $0.title, display: $0.display, hero: $0.hero)
          },
          microLabel: type.microLabel.map {
            CoreThemeFileMicroLabel(
              size: $0.size, weight: $0.weight, tracking: $0.tracking, uppercase: $0.uppercase, role: $0.role)
          })
      },
      touchTarget: touchTarget, usesShadows: usesShadows, usesGradientsOnChrome: usesGradientsOnChrome)
  }

  init(_ core: CoreThemeFileStructure) {
    self.init(
      radius: core.radius.map {
        ThemeFile.Radius(panel: $0.panel, row: $0.row, control: $0.control, pill: $0.pill, shell: $0.shell)
      },
      border: core.border.map {
        ThemeFile.Border(hairline: $0.hairline, emphasis: $0.emphasis, focusRing: $0.focusRing)
      },
      spacing: core.spacing.map {
        ThemeFile.Spacing(xxs: $0.xxs, xs: $0.xs, sm: $0.sm, md: $0.md, lg: $0.lg, xl: $0.xl)
      },
      typography: core.typography.map { type in
        ThemeFile.Typography(
          display: type.display.map(ThemeFile.Face.init), body: type.body.map(ThemeFile.Face.init),
          mono: type.mono.map(ThemeFile.Face.init), bodySize: type.bodySize,
          scale: type.scale.map {
            ThemeFile.TypeScale(
              caption: $0.caption, body: $0.body, title: $0.title, display: $0.display, hero: $0.hero)
          },
          microLabel: type.microLabel.map {
            ThemeFile.MicroLabel(
              size: $0.size, weight: $0.weight, tracking: $0.tracking, uppercase: $0.uppercase, role: $0.role)
          })
      },
      touchTarget: core.touchTarget, usesShadows: core.usesShadows,
      usesGradientsOnChrome: core.usesGradientsOnChrome)
  }
}

extension ThemeFile {
  var core: CoreThemeFile {
    func entry(_ entry: PlatformOverride?) -> CoreThemeFilePlatformOverride? {
      entry.map { CoreThemeFilePlatformOverride(structure: $0.structure?.core) }
    }
    let base: CoreThemeFileBase =
      switch extends {
      case .defaultTheme: .defaultTheme
      case .theme(let identifier): .theme(identifier: identifier)
      case .nothing: .nothing
      }
    let lock: CoreThemeFileLock =
      switch lockedAppearance {
      case .inherit: .inherit
      case .unlocked: .unlocked
      case .locked(let raw): .locked(raw: raw)
      }
    return CoreThemeFile(
      identifier: identifier, name: name, summary: summary, lockedAppearance: lock, extends: base,
      seeds: seeds?.core, palette: palette?.core, structure: structure?.core,
      platforms: platforms.map {
        CoreThemeFilePlatforms(macos: entry($0.macos), ios: entry($0.ios), android: entry($0.android))
      })
  }

  init(_ core: CoreThemeFile) {
    func entry(_ entry: CoreThemeFilePlatformOverride?) -> PlatformOverride? {
      entry.map { PlatformOverride(structure: $0.structure.map(Structure.init)) }
    }
    let base: Base =
      switch core.extends {
      case .defaultTheme: .defaultTheme
      case .theme(let identifier): .theme(identifier)
      case .nothing: .nothing
      }
    let lock: Lock =
      switch core.lockedAppearance {
      case .inherit: .inherit
      case .unlocked: .unlocked
      case .locked(let raw): .locked(raw)
      }
    self.init(
      identifier: core.identifier, name: core.name, summary: core.summary, lockedAppearance: lock,
      extends: base, seeds: core.seeds.map(Palette.init), palette: core.palette.map(Palette.init),
      structure: core.structure.map(Structure.init),
      platforms: core.platforms.map {
        Platforms(macos: entry($0.macos), ios: entry($0.ios), android: entry($0.android))
      })
  }
}
