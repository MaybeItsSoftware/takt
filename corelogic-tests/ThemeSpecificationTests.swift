import XCTest

@testable import TaktCore

final class ThemeSpecificationTests: XCTestCase {
  private func value(_ hex: String) -> ThemeColorValue {
    ThemeColorValue(hex: hex)!
  }

  // MARK: - The built-ins

  func testBothBuiltInThemesAreRegisteredUnderDistinctIdentifiers() {
    let identifiers = BuiltInThemeSpecifications.all.map(\.identifier)
    XCTAssertEqual(identifiers.count, Set(identifiers).count)
    XCTAssertEqual(
      BuiltInThemeSpecifications.specification(withIdentifier: "native.theme.chalk")?.name,
      "Zed"
    )
    XCTAssertEqual(BuiltInThemeSpecifications.all.first?.identifier, BuiltInThemeSpecifications.defaultIdentifier)
    XCTAssertEqual(BuiltInThemeSpecifications.defaultIdentifier, "native.theme.priority")
    XCTAssertNil(BuiltInThemeSpecifications.specification(withIdentifier: "native.theme.nope"))
  }

  func testNoBuiltInRoleResolvesToTheDebugColour() {
    for specification in BuiltInThemeSpecifications.all {
      for appearance in ThemeAppearance.allCases {
        for role in ThemeColorRole.allCases {
          XCTAssertNotEqual(
            specification.color(role, in: appearance),
            .unresolved,
            "\(specification.identifier).\(appearance.rawValue).\(role.rawValue) is unresolved — "
              + "either the hex is a typo or the role was never given a value"
          )
        }
      }
    }
  }

  func testChalkReproducesTheHouseStyleExactly() {
    let chalk = BuiltInThemeSpecifications.chalk
    XCTAssertEqual(chalk.color(.paper, in: .light).hexString, "#FAF8F4")
    XCTAssertEqual(chalk.color(.raised, in: .light).hexString, "#FFFFFF")
    XCTAssertEqual(chalk.color(.ink, in: .light).hexString, "#444054")
    XCTAssertEqual(chalk.color(.border, in: .light).hexString, "#E6E4EA")
    XCTAssertEqual(chalk.color(.mutedText, in: .light).hexString, "#6E6B7C")
    XCTAssertEqual(chalk.color(.dimText, in: .light).hexString, "#B6B3BF")
    XCTAssertEqual(chalk.color(.primary, in: .light).hexString, "#007FFF")
    XCTAssertEqual(chalk.color(.success, in: .light).hexString, "#4CC38E")
    XCTAssertEqual(chalk.color(.danger, in: .light).hexString, "#D62246")
    XCTAssertEqual(chalk.color(.warning, in: .light).hexString, "#FFBF00")
    XCTAssertEqual(chalk.color(.paper, in: .dark).hexString, "#1C1A23")
    XCTAssertEqual(chalk.color(.raised, in: .dark).hexString, "#25232F")
    XCTAssertEqual(chalk.color(.border, in: .dark).hexString, "#34313F")
  }

  /// "Warm at the paper end, cool at the ink end", stated as a property rather
  /// than as six hex strings: the page is warmer than neutral, and the ink is
  /// cooler than neutral.
  func testChalkNeutralsRunWarmAtThePaperEndAndCoolAtTheInkEnd() {
    let chalk = BuiltInThemeSpecifications.chalk
    let paper = chalk.color(.paper, in: .light)
    XCTAssertGreaterThan(paper.red, paper.green)
    XCTAssertGreaterThan(paper.green, paper.blue)

    let ink = chalk.color(.ink, in: .light)
    XCTAssertGreaterThan(ink.blue, ink.green)
    XCTAssertGreaterThan(ink.red, ink.green, "grape, not blue-grey")
  }

  func testChalkAccentsKeepTheirHexAcrossTheFlip() {
    let chalk = BuiltInThemeSpecifications.chalk
    for role in [ThemeColorRole.primary, .success, .danger, .warning,
      .categoricalPurple, .categoricalPink, .categoricalOrange] {
      XCTAssertEqual(
        chalk.color(role, in: .light),
        chalk.color(role, in: .dark),
        "\(role.rawValue) should keep its hex in dark mode and be used at low alpha instead"
      )
    }
  }

  /// Chalk Dark is deliberately *not* a second palette. It shares Chalk's
  /// outright, so the two cannot drift; what makes it a different theme is
  /// that it has an identity of its own and fixes the appearance.
  func testChalkDarkIsChalkWithTheAppearanceFixed() {
    let chalk = BuiltInThemeSpecifications.chalk
    let dark = BuiltInThemeSpecifications.chalkDark
    XCTAssertEqual(chalk.palette, dark.palette)
    XCTAssertEqual(chalk.structure, dark.structure)
    XCTAssertNotEqual(chalk.identifier, dark.identifier)
    XCTAssertNil(chalk.lockedAppearance, "Chalk follows the system")
    XCTAssertEqual(dark.lockedAppearance, .dark)
  }

  /// The point of locking it: the colours you get are the dark ones whatever
  /// the desktop is set to.
  func testChalkDarkResolvesToTheDarkTable() {
    let dark = BuiltInThemeSpecifications.chalkDark
    XCTAssertEqual(dark.color(.paper, in: .dark).hexString.lowercased(), "#1c1a23")
    XCTAssertEqual(
      dark.color(.paper, in: .dark),
      BuiltInThemeSpecifications.chalk.color(.paper, in: .dark))
  }

  func testBuiltInThemesRaiseNoErrorsOrWarnings() {
    for specification in BuiltInThemeSpecifications.all {
      let blocking = specification.validate().filter { $0.severity != .note }
      XCTAssertTrue(
        blocking.isEmpty,
        "\(specification.identifier): \(blocking.map(\.message).joined(separator: "; "))"
      )
    }
  }

  /// Azure on chalk is 3.6:1. That is not a bug to fix, it is a constraint to
  /// record — and the audit records it as a note rather than a warning.
  func testAzureOnChalkIsReportedAsLargeTextOnly() {
    let issues = BuiltInThemeSpecifications.chalk.validate()
    let largeTextOnly = issues.compactMap { issue -> (ThemeColorRole, ThemeAppearance)? in
      if case .largeTextOnly(let role, let appearance, _) = issue { return (role, appearance) }
      return nil
    }
    XCTAssertTrue(largeTextOnly.contains { $0.0 == .primary && $0.1 == .light })
    XCTAssertTrue(issues.allSatisfy { $0.severity == .note })
  }

  /// Chalk's own dark table has to clear AA for body text, which it does and
  /// which the light one does not have to be asked twice about. Checked
  /// separately from the whole-theme audit so a regression here names itself.
  func testChalkDarkClearsBodyTextAA() {
    let palette = BuiltInThemeSpecifications.chalkDark.palette
    for role in ThemeColorRole.bodyTextRoles {
      let ratio = ThemeContrastAudit.ratio(role, on: .paper, in: .dark, of: palette)
      XCTAssertGreaterThanOrEqual(
        ratio, ThemeContrastAudit.bodyTextMinimum,
        "\(role.rawValue) on the dark page is only \(ratio):1")
    }
  }

  // MARK: - The audit itself

  private func palette(
    paper: String,
    ink: String,
    muted: String,
    accent: String,
    raised: String
  ) -> ThemePalette {
    var table: [ThemeColorRole: ThemeColorValue] = Dictionary(
      uniqueKeysWithValues: ThemeColorRole.allCases.map { ($0, value("#808080")) }
    )
    table[.paper] = value(paper)
    table[.ink] = value(ink)
    table[.mutedText] = value(muted)
    table[.primary] = value(accent)
    table[.raised] = value(raised)
    return ThemePalette(light: table, dark: table)
  }

  private func specification(_ palette: ThemePalette) -> ThemeSpecification {
    ThemeSpecification(
      identifier: "test",
      name: "Test",
      summary: "",
      palette: palette,
      structure: BuiltInThemeSpecifications.chalk.structure
    )
  }

  func testMissingRoleIsAnError() {
    let spec = ThemeSpecification(
      identifier: "test",
      name: "Test",
      summary: "",
      palette: ThemePalette(light: [:], dark: [:]),
      structure: BuiltInThemeSpecifications.chalk.structure
    )
    let issues = spec.validate()
    XCTAssertTrue(issues.contains { $0.severity == .error })
    XCTAssertEqual(issues.first?.severity, .error, "worst first")
  }

  func testUnreadableBodyTextIsAWarning() {
    let spec = specification(
      palette(
        paper: "#ffffff", ink: "#cccccc", muted: "#dddddd", accent: "#0032c8", raised: "#f0f0f3"))
    let belowAA = spec.validate().compactMap { issue -> ThemeColorRole? in
      if case .bodyTextBelowAA(let role, _, _) = issue { return role }
      return nil
    }
    XCTAssertTrue(belowAA.contains(.ink))
    XCTAssertTrue(belowAA.contains(.mutedText))
  }

  func testAccentBelowThreeToOneIsAWarningNotANote() {
    let spec = specification(
      palette(
        paper: "#ffffff", ink: "#000000", muted: "#2f2f3a", accent: "#fff4c8", raised: "#f0f0f3"))
    let issue = spec.validate().first { issue in
      if case .accentBelowUIMinimum(let role, _, _) = issue { return role == .primary }
      return false
    }
    XCTAssertNotNil(issue)
    XCTAssertEqual(issue?.severity, .warning)
  }

  func testACardYouCannotSeeAgainstThePageIsReported() {
    let spec = specification(
      palette(
        paper: "#ffffff", ink: "#000000", muted: "#2f2f3a", accent: "#0032c8", raised: "#ffffff"))
    XCTAssertTrue(
      spec.validate().contains { issue in
        if case .raisedIndistinctFromPaper = issue { return true }
        return false
      }
    )
  }

  // MARK: - Structure

  func testShadowsAndGradientsOnChromeAreReported() {
    let chalk = BuiltInThemeSpecifications.chalk.structure
    let loud = ThemeStructure(
      radius: chalk.radius,
      border: chalk.border,
      spacing: chalk.spacing,
      typography: chalk.typography,
      usesShadows: true,
      usesGradientsOnChrome: true
    )
    let issues = ThemeStructureAudit.findings(for: loud)
    XCTAssertTrue(issues.contains(.shadowsUsed))
    XCTAssertTrue(issues.contains(.gradientsOnChrome))
  }

  func testAPillThatIsNotAPillBreaksTheRadiusScale() {
    let chalk = BuiltInThemeSpecifications.chalk.structure
    let broken = ThemeStructure(
      radius: ThemeRadiusScale(panel: 8, row: 0, control: 6, pill: 12, shell: 20),
      border: chalk.border,
      spacing: chalk.spacing,
      typography: chalk.typography
    )
    XCTAssertTrue(ThemeStructureAudit.findings(for: broken).contains(.radiusScaleOutOfOrder))
  }

  func testShellRadiusOffTheReservedRangeIsReportedButZeroIsNot() {
    let chalk = BuiltInThemeSpecifications.chalk.structure
    func structure(shell: Double) -> ThemeStructure {
      ThemeStructure(
        radius: ThemeRadiusScale(panel: 8, row: 0, control: 6, pill: 9999, shell: shell),
        border: chalk.border,
        spacing: chalk.spacing,
        typography: chalk.typography
      )
    }
    XCTAssertTrue(
      ThemeStructureAudit.findings(for: structure(shell: 12))
        .contains(.shellRadiusOffScale(value: 12)))
    XCTAssertEqual(ThemeStructureAudit.findings(for: structure(shell: 0)), [])
    XCTAssertEqual(ThemeStructureAudit.findings(for: structure(shell: 22)), [])
  }

  func testAHeavyHairlineIsNoLongerAHairline() {
    let chalk = BuiltInThemeSpecifications.chalk.structure
    let heavy = ThemeStructure(
      radius: chalk.radius,
      border: ThemeBorderScale(hairline: 4, emphasis: 6, focusRing: 6),
      spacing: chalk.spacing,
      typography: chalk.typography
    )
    XCTAssertTrue(ThemeStructureAudit.findings(for: heavy).contains(.hairlineTooHeavy(value: 4)))
  }

  /// Tracking is stated in em; SwiftUI wants points. One place does the
  /// conversion, so the old house micro-label — 10pt at 0.15em — tracks 1.5pt.
  func testMicroLabelTrackingConvertsFromEmToPoints() {
    let label = ThemeMicroLabel(
      size: 10, weight: .bold, tracking: 0.15, isUppercased: true, role: .mutedText)
    XCTAssertEqual(label.trackingPoints, 1.5, accuracy: 0.0001)
  }

  /// The default is set as the Mac app is, in IBM Plex Sans and Lilex with
  /// Zed's structure, and its palette is the one its seeds grow — apart from `raised`,
  /// which it names so the dark card is a clear step above the page.
  func testTheDefaultIsSetAsTheMacAppAndInTheHouseColours() {
    let priority = BuiltInThemeSpecifications.priority
    XCTAssertEqual(priority.name, "Takt")
    let type = priority.structure.typography
    XCTAssertEqual(type.body, ThemeFontFace(families: ["IBM Plex Sans"], design: .sans))
    XCTAssertEqual(type.mono, ThemeFontFace(families: ["Lilex"], design: .monospaced))
    // Zed's structure, the one the Mac draws, on every platform.
    for platform in ThemePlatform.allCases {
      XCTAssertEqual(
        BuiltInThemeSpecifications.priority(for: platform).structure,
        BuiltInThemeSpecifications.chalk(for: platform).structure, platform.rawValue)
    }
    // The house colours, exactly: every role Zed names, with the same hex,
    // in both appearances — the default differs from Zed in structure only.
    let zed = BuiltInThemeSpecifications.chalk
    for appearance in ThemeAppearance.allCases {
      for (role, value) in zed.palette.table(appearance) {
        XCTAssertEqual(priority.color(role, in: appearance), value, "\(appearance) \(role)")
      }
    }
    XCTAssertEqual(priority.color(.paper, in: .light), ThemeColorValue(hex: "#faf8f4"))
    XCTAssertEqual(priority.color(.ink, in: .light), ThemeColorValue(hex: "#444054"))
    XCTAssertEqual(priority.color(.primary, in: .light), ThemeColorValue(hex: "#007fff"))
    XCTAssertEqual(priority.color(.paper, in: .dark), ThemeColorValue(hex: "#1c1a23"))
    XCTAssertTrue(priority.validate().filter { $0.severity != .note }.isEmpty, "\(priority.validate())")
  }

  /// Zed is set the way Zed sets itself: Plex Sans for everything read,
  /// Lilex for code and numerals, and labels at caption size, regular, as
  /// written and untracked. A system design sits behind each face so a failed
  /// registration lands in a sans, never in a serif.
  func testZedUsesZedsFacesAndQuietLabels() {
    for builtIn in [BuiltInThemeSpecifications.chalk, BuiltInThemeSpecifications.chalkDark] {
      let type = builtIn.structure.typography
      XCTAssertEqual(type.body, ThemeFontFace(families: ["IBM Plex Sans"], design: .sans))
      XCTAssertEqual(type.display, ThemeFontFace(families: ["IBM Plex Sans"], design: .sans))
      XCTAssertEqual(type.mono, ThemeFontFace(families: ["Lilex"], design: .monospaced))
      XCTAssertEqual(type.scale.body, 13)
      XCTAssertEqual(type.scale.caption, 12)
      XCTAssertEqual(type.scale.title, 15)

      let label = type.microLabel
      XCTAssertEqual(label.size, type.scale.caption)
      XCTAssertEqual(label.weight, .regular)
      XCTAssertEqual(label.trackingPoints, 0)
      XCTAssertFalse(label.isUppercased)
      XCTAssertEqual(label.role, .mutedText)
    }
  }
}
