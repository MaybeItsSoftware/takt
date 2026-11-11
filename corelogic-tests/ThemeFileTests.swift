import XCTest

@testable import TaktCore

final class ThemeFileTests: XCTestCase {
  private func source(_ name: String, _ json: String) -> ThemeFileSource {
    ThemeFileSource(name: name, data: Data(json.utf8))
  }

  private func load(_ sources: ThemeFileSource...) -> ThemeFileLibrary {
    ThemeFileLoader.load(sources)
  }

  private func messages(_ outcome: ThemeFileOutcome?) -> [String] {
    outcome?.issues.map(\.message) ?? []
  }

  // MARK: - Round trip

  /// Exporting a built-in and loading the export back gives the same theme,
  /// to the precision a hex string carries.
  func testExportedBuiltInsLoadBackAsTheSameTheme() throws {
    for builtIn in BuiltInThemeSpecifications.all {
      var file = ThemeFile(specification: builtIn)
      file.identifier = "user.copy"
      let data = try file.encoded()

      let (decoded, decodeIssues) = ThemeFileLoader.decode(data, source: "copy.json")
      XCTAssertEqual(decodeIssues, [])
      XCTAssertEqual(decoded, file)

      let library = ThemeFileLoader.load([ThemeFileSource(name: "copy.json", data: data)])
      let loaded = try XCTUnwrap(library.themes.first)
      XCTAssertEqual(loaded.name, builtIn.name)
      XCTAssertEqual(loaded.lockedAppearance, builtIn.lockedAppearance)
      XCTAssertEqual(loaded.structure, builtIn.structure)
      for appearance in ThemeAppearance.allCases {
        for role in ThemeColorRole.allCases {
          XCTAssertEqual(
            loaded.color(role, in: appearance).hexString,
            builtIn.color(role, in: appearance).hexString,
            "\(builtIn.name) \(appearance.rawValue).\(role.rawValue)")
        }
      }
      XCTAssertFalse(library.issues.contains { $0.severity == .error }, "\(library.issues)")
    }
  }

  func testTheTriStateFieldsSurviveEncoding() throws {
    let file = ThemeFile(identifier: "x", lockedAppearance: .unlocked, extends: .nothing)
    let json = try XCTUnwrap(String(bytes: try file.encoded(), encoding: .utf8))
    XCTAssertTrue(json.contains("\"extends\" : null"), json)
    XCTAssertTrue(json.contains("\"lockedAppearance\" : null"), json)
    XCTAssertEqual(ThemeFileLoader.decode(Data(json.utf8), source: "x.json").0, file)

    let bare = ThemeFileLoader.decode(Data("{}".utf8), source: "bare.json").0
    XCTAssertEqual(bare?.extends, .defaultTheme)
    XCTAssertEqual(bare?.lockedAppearance, .inherit)
  }

  // MARK: - extends and merging

  func testAFewOverridesInheritEverythingElseFromTheDefault() throws {
    let library = load(
      source(
        "dusk.json",
        """
        {
          "name": "Dusk",
          "palette": { "light": { "primary": "#7a4de8" }, "dark": { "paper": "#101014" } },
          "structure": { "radius": { "panel": 10 }, "typography": { "bodySize": 15 } }
        }
        """))
    let dusk = try XCTUnwrap(library.themes.first)
    let chalk = BuiltInThemeSpecifications.priority

    XCTAssertEqual(dusk.identifier, "user.dusk", "no identifier: named after the file")
    XCTAssertEqual(dusk.name, "Dusk")
    XCTAssertEqual(dusk.color(.primary, in: .light).hexString, "#7A4DE8")
    XCTAssertEqual(dusk.color(.primary, in: .dark), chalk.color(.primary, in: .dark))
    XCTAssertEqual(dusk.color(.paper, in: .dark).hexString, "#101014")
    XCTAssertEqual(dusk.color(.ink, in: .light), chalk.color(.ink, in: .light))
    XCTAssertEqual(dusk.structure.radius.panel, 10)
    XCTAssertEqual(dusk.structure.radius.control, chalk.structure.radius.control)
    XCTAssertEqual(dusk.structure.border, chalk.structure.border)
    XCTAssertEqual(dusk.structure.typography.bodySize, 15)
    XCTAssertEqual(
      dusk.structure.typography.scale, .proportioned(fromBody: 15),
      "a new body size with no scale re-proportions the scale")
  }

  /// The built-ins moved to Zed's faces and quiet labels; a theme file can
  /// still put back the slab and the tracked capitals, in full.
  func testAThemeCanRestoreTheSlabFaceAndTheTrackedCapitals() throws {
    let library = load(
      source(
        "slab.json",
        """
        {
          "structure": { "typography": {
            "body": { "families": ["Arvo", "Rockwell"], "design": "serif" },
            "microLabel": { "size": 10, "weight": "bold", "tracking": 0.15, "uppercase": true }
          } }
        }
        """))
    let slab = try XCTUnwrap(library.themes.first)
    let type = slab.structure.typography
    XCTAssertEqual(type.body, ThemeFontFace(families: ["Arvo", "Rockwell"], design: .serif))
    XCTAssertEqual(type.mono, BuiltInThemeSpecifications.priority.structure.typography.mono)
    XCTAssertEqual(
      type.microLabel,
      ThemeMicroLabel(size: 10, weight: .bold, tracking: 0.15, isUppercased: true, role: .mutedText))
  }

  func testExtendingChalkDarkInheritsItsLockUnlessClearedWithNull() throws {
    let library = load(
      source("a.json", ##"{ "extends": "native.theme.chalk.dark" }"##),
      source("b.json", ##"{ "extends": "native.theme.chalk.dark", "lockedAppearance": null }"##),
      source("c.json", ##"{ "lockedAppearance": "light" }"##))
    XCTAssertEqual(library.themes.map(\.lockedAppearance), [.dark, nil, .light])
  }

  func testAThemeCanExtendAnotherUserThemeWhateverTheFileOrder() throws {
    let library = load(
      source("a-child.json", ##"{ "extends": "mine.base", "palette": { "light": { "ink": "#111111" } } }"##),
      source("z-base.json", ##"{ "identifier": "mine.base", "palette": { "light": { "paper": "#fefefe" } } }"##))
    let child = try XCTUnwrap(library.themes.first { $0.identifier == "user.a-child" })
    XCTAssertEqual(child.color(.paper, in: .light).hexString, "#FEFEFE")
    XCTAssertEqual(child.color(.ink, in: .light).hexString, "#111111")
  }

  func testCyclesAndUnknownParentsAreSkippedWithAReason() {
    let library = load(
      source("a.json", ##"{ "identifier": "a", "extends": "b" }"##),
      source("b.json", ##"{ "identifier": "b", "extends": "a" }"##),
      source("c.json", ##"{ "extends": "nope" }"##))
    XCTAssertEqual(library.themes, [])
    XCTAssertEqual(library.outcomes.count, 3)
    XCTAssertTrue(library.outcomes.allSatisfy { $0.skippedReason != nil })
    XCTAssertEqual(library.outcomes[2].skippedReason, "it extends \"nope\", which is not a theme")
  }

  func testIdentifiersCannotShadowABuiltInOrEachOther() {
    let library = load(
      source("a.json", ##"{ "identifier": "native.theme.chalk" }"##),
      source("b.json", ##"{ "identifier": "mine" }"##),
      source("c.json", ##"{ "identifier": "mine" }"##))
    XCTAssertEqual(library.themes.map(\.identifier), ["mine"])
    XCTAssertNotNil(library.outcomes[0].skippedReason)
    XCTAssertEqual(library.outcomes[2].skippedReason, "identifier \"mine\" is already used by b.json")
  }

  // MARK: - Bad values are reported, not fatal

  func testABadHexIsAnErrorButTheThemeStillLoadsWithTheInheritedValue() throws {
    let library = load(
      source("bad.json", ##"{ "palette": { "light": { "paper": "#nothex", "ink": "#222" } } }"##))
    let outcome = try XCTUnwrap(library.outcomes.first)
    let theme = try XCTUnwrap(outcome.specification)
    XCTAssertEqual(
      theme.color(.paper, in: .light), BuiltInThemeSpecifications.priority.color(.paper, in: .light))
    XCTAssertEqual(theme.color(.ink, in: .light).hexString, "#222222")
    XCTAssertEqual(outcome.issues.first?.severity, .error)
    XCTAssertTrue(messages(outcome).contains { $0.contains("palette.light.paper \"#nothex\"") })
  }

  func testAnUnknownRoleIsAWarningAndIgnored() throws {
    let library = load(source("x.json", ##"{ "palette": { "dark": { "backgroundd": "#000000" } } }"##))
    let outcome = try XCTUnwrap(library.outcomes.first)
    XCTAssertNotNil(outcome.specification)
    let issue = try XCTUnwrap(outcome.issues.first { $0.message.contains("backgroundd") })
    XCTAssertEqual(issue.severity, .warning)
    XCTAssertEqual(issue.source, "x.json")
  }

  func testUnknownKeysAndBadEnumeratedValuesAreReported() throws {
    let library = load(
      source(
        "x.json",
        """
        {
          "pallete": {},
          "lockedAppearance": "dusk",
          "structure": {
            "radius": { "pannel": 3 },
            "typography": { "body": { "design": "comic" }, "microLabel": { "weight": "heavy", "role": "nope" } }
          }
        }
        """))
    let outcome = try XCTUnwrap(library.outcomes.first)
    XCTAssertNotNil(outcome.specification, "every one of those is recoverable")
    let text = messages(outcome).joined(separator: "\n")
    for fragment in ["pallete", "structure.radius.pannel", "\"dusk\"", "\"comic\"", "\"heavy\"", "\"nope\""] {
      XCTAssertTrue(text.contains(fragment), "missing \(fragment) in:\n\(text)")
    }
  }

  func testAFileThatIsNotJSONIsSkippedNotThrown() throws {
    let library = load(source("broken.json", "{ \"name\": "), source("ok.json", "{}"))
    XCTAssertEqual(library.themes.map(\.identifier), ["user.ok"])
    let broken = try XCTUnwrap(library.outcomes.first)
    XCTAssertEqual(broken.skippedReason, "could not be read")
    XCTAssertTrue(messages(broken).contains("not valid JSON"))
  }

  func testAWrongTypeNamesThePath() throws {
    let library = load(source("x.json", ##"{ "structure": { "radius": { "panel": "big" } } }"##))
    XCTAssertTrue(
      messages(library.outcomes.first).contains("structure.radius.panel should be a number"),
      "\(messages(library.outcomes.first))")
  }

  func testANegativeSizeKeepsTheInheritedValue() throws {
    let library = load(source("x.json", ##"{ "structure": { "border": { "hairline": -1 } } }"##))
    let theme = try XCTUnwrap(library.themes.first)
    XCTAssertEqual(theme.structure.border.hairline, 1)
    XCTAssertEqual(library.issues.first?.severity, .error)
  }

  // MARK: - Missing roles

  func testAStandaloneThemeMissingARoleEverywhereIsSkipped() throws {
    var light: [String: String] = [:]
    for role in ThemeColorRole.allCases where role != .categoricalPink {
      light[role.rawValue] = "#808080"
    }
    let file = ThemeFile(extends: .nothing, palette: .init(light: light))
    let library = ThemeFileLoader.load([
      ThemeFileSource(name: "partial.json", data: try file.encoded())
    ])
    let outcome = try XCTUnwrap(library.outcomes.first)
    XCTAssertNil(outcome.specification)
    XCTAssertEqual(outcome.skippedReason, "no colour for categoricalPink")
  }

  func testAStandaloneThemeWithOneTableLoadsWithMissingRoleErrors() throws {
    var light: [String: String] = [:]
    for role in ThemeColorRole.allCases { light[role.rawValue] = "#808080" }
    light[ThemeColorRole.ink.rawValue] = "#000000"
    light[ThemeColorRole.paper.rawValue] = "#ffffff"
    let file = ThemeFile(extends: .nothing, palette: .init(light: light))
    let outcome = ThemeFileLoader.resolve(file, source: "light-only.json", base: nil)
    let theme = try XCTUnwrap(outcome.specification, "every role resolves, via the light table")
    XCTAssertEqual(theme.color(.ink, in: .dark).hexString, "#000000")
    XCTAssertTrue(outcome.issues.contains { $0.severity == .error && $0.message.contains("no dark value") })
  }

  // MARK: - Platforms

  private func load(for platform: ThemePlatform, _ sources: ThemeFileSource...) -> ThemeFileLibrary {
    ThemeFileLoader.load(sources, platform: platform)
  }

  /// The table in docs/themes.md, "Chalk's defaults per platform".
  func testChalksPerPlatformStructureIsTheDocumentedTable() {
    struct Row {
      let bodySize: Double
      let scale: ThemeTypeScale
      let microLabel: Double
      let radius: (panel: Double, row: Double, control: Double)
      let touchTarget: Double
    }
    let table: [ThemePlatform: Row] = [
      .macos: Row(
        bodySize: 13, scale: .init(caption: 12, body: 13, title: 15, display: 28, hero: 64),
        microLabel: 12, radius: (0, 0, 4), touchTarget: 0),
      .ios: Row(
        bodySize: 17, scale: .init(caption: 13, body: 17, title: 20, display: 34, hero: 72),
        microLabel: 13, radius: (8, 0, 6), touchTarget: 44),
      .android: Row(
        bodySize: 16, scale: .init(caption: 12, body: 16, title: 20, display: 32, hero: 72),
        microLabel: 12, radius: (8, 0, 6), touchTarget: 48),
    ]
    for (platform, row) in table {
      for builtIn in [
        BuiltInThemeSpecifications.chalk(for: platform), BuiltInThemeSpecifications.chalkDark(for: platform),
      ] {
        let structure = builtIn.structure
        let label = "\(builtIn.name) on \(platform.rawValue)"
        XCTAssertEqual(structure.typography.bodySize, row.bodySize, label)
        XCTAssertEqual(structure.typography.scale, row.scale, label)
        XCTAssertEqual(structure.typography.microLabel.size, row.microLabel, label)
        XCTAssertEqual(structure.radius.panel, row.radius.panel, label)
        XCTAssertEqual(structure.radius.row, row.radius.row, label)
        XCTAssertEqual(structure.radius.control, row.radius.control, label)
        XCTAssertEqual(
          structure.spacing, ThemeSpacingScale(xxs: 2, xs: 4, sm: 8, md: 12, lg: 16, xl: 24), label)
        XCTAssertEqual(structure.touchTarget, row.touchTarget, label)
        XCTAssertEqual(builtIn.palette, BuiltInThemeSpecifications.chalk.palette, "\(label): one palette")
        XCTAssertEqual(builtIn.validate().filter { $0.severity == .error }, [], label)
      }
    }
  }

  func testPrioritysPerPlatformStructureIsTheDocumentedTable() {
    let table: [ThemePlatform: (bodySize: Double, radius: [Double], touchTarget: Double)] = [
      .macos: (13, [0, 0, 4], 0),
      .ios: (17, [8, 0, 6], 44),
      .android: (16, [8, 0, 6], 48),
    ]
    for (platform, row) in table {
      let structure = BuiltInThemeSpecifications.priority(for: platform).structure
      XCTAssertEqual(structure.typography.bodySize, row.bodySize, platform.rawValue)
      XCTAssertEqual(
        [structure.radius.panel, structure.radius.row, structure.radius.control], row.radius, platform.rawValue)
      XCTAssertEqual(structure.touchTarget, row.touchTarget, platform.rawValue)
      XCTAssertEqual(
        BuiltInThemeSpecifications.priority(for: platform).palette, BuiltInThemeSpecifications.priority.palette)
    }
  }

  /// The Mac is what it was before platforms existed.
  func testTheMacResolvesExactlyAsBefore() {
    XCTAssertEqual(BuiltInThemeSpecifications.chalk(for: .macos), BuiltInThemeSpecifications.chalk)
    XCTAssertEqual(BuiltInThemeSpecifications.chalkDark(for: .macos), BuiltInThemeSpecifications.chalkDark)
    XCTAssertEqual(BuiltInThemeSpecifications.all(for: .macos), BuiltInThemeSpecifications.all)
    let file = source("dusk.json", ##"{ "structure": { "radius": { "control": 2 } } }"##)
    XCTAssertEqual(ThemeFileLoader.load([file]), ThemeFileLoader.load([file], platform: .macos))
  }

  func testChalkDarkInheritsChalksPlatformStructure() {
    for platform in ThemePlatform.allCases {
      XCTAssertEqual(
        BuiltInThemeSpecifications.chalkDark(for: platform).structure,
        BuiltInThemeSpecifications.chalk(for: platform).structure)
      XCTAssertEqual(BuiltInThemeSpecifications.chalkDark(for: platform).lockedAppearance, .dark)
    }
  }

  /// The example in docs/themes.md: base for the platform, then the theme's
  /// structure, then its entry for the platform.
  func testStructureResolvesBaseThenStructureThenPlatform() throws {
    let dusk = source(
      "dusk.json",
      """
      {
        "name": "Dusk",
        "palette": { "dark": { "paper": "#15131c" } },
        "structure": { "radius": { "control": 2 } },
        "platforms": {
          "ios": { "structure": { "typography": { "bodySize": 18 } } },
          "android": { "structure": { "spacing": { "md": 18 } } }
        }
      }
      """)
    let mac = try XCTUnwrap(load(for: .macos, dusk).themes.first)
    let ios = try XCTUnwrap(load(for: .ios, dusk).themes.first)
    let android = try XCTUnwrap(load(for: .android, dusk).themes.first)

    for theme in [mac, ios, android] {
      XCTAssertEqual(theme.structure.radius.control, 2, "structure applies everywhere")
      XCTAssertEqual(theme.color(.paper, in: .dark).hexString, "#15131C", "one palette")
    }
    XCTAssertEqual(mac.structure.typography.bodySize, 13)
    XCTAssertEqual(mac.structure.radius.panel, 0, "inherits the Mac's default")
    XCTAssertEqual(ios.structure.typography.bodySize, 18)
    XCTAssertEqual(ios.structure.typography.scale, .proportioned(fromBody: 18))
    XCTAssertEqual(ios.structure.radius.panel, 8, "inherits the iPhone's default")
    XCTAssertEqual(ios.structure.touchTarget, 44)
    XCTAssertEqual(ios.structure.spacing.md, BuiltInThemeSpecifications.priority(for: .ios).structure.spacing.md)
    XCTAssertEqual(android.structure.spacing.md, 18)
    XCTAssertEqual(android.structure.typography.bodySize, 16)
    XCTAssertEqual(android.structure.touchTarget, 48)
  }

  func testAStructureValueAppliesOnEveryPlatformUnlessAPlatformSaysOtherwise() throws {
    let big = source(
      "big.json",
      ##"{ "structure": { "typography": { "bodySize": 15 } }, "platforms": { "android": { "structure": { "typography": { "bodySize": 19 } } } } }"##
    )
    XCTAssertEqual(load(for: .macos, big).themes.first?.structure.typography.bodySize, 15)
    XCTAssertEqual(load(for: .ios, big).themes.first?.structure.typography.bodySize, 15)
    XCTAssertEqual(load(for: .ios, big).themes.first?.structure.typography.scale, .proportioned(fromBody: 15))
    XCTAssertEqual(load(for: .android, big).themes.first?.structure.typography.bodySize, 19)
  }

  /// A parent's own `platforms` entry is part of what a child extends.
  func testExtendingAUserThemeTakesItsPlatformEntry() throws {
    let parent = source(
      "parent.json",
      ##"{ "identifier": "mine.parent", "platforms": { "ios": { "structure": { "radius": { "panel": 12 } } } } }"##)
    let child = source("child.json", ##"{ "extends": "mine.parent", "structure": { "border": { "emphasis": 3 } } }"##)
    let ios = load(for: .ios, child, parent)
    let resolved = try XCTUnwrap(ios.themes.first { $0.identifier == "user.child" })
    XCTAssertEqual(resolved.structure.radius.panel, 12)
    XCTAssertEqual(resolved.structure.border.emphasis, 3)
    XCTAssertEqual(
      load(for: .macos, child, parent).themes.first { $0.identifier == "user.child" }?.structure.radius.panel, 0)
  }

  func testAPaletteUnderPlatformsIsAWarningAndIgnored() throws {
    let file = source(
      "x.json",
      ##"{ "platforms": { "ios": { "palette": { "light": { "paper": "#000000" } } }, "windows": {} } }"##)
    let outcome = try XCTUnwrap(load(for: .ios, file).outcomes.first)
    let theme = try XCTUnwrap(outcome.specification)
    XCTAssertEqual(theme.color(.paper, in: .light), BuiltInThemeSpecifications.priority.color(.paper, in: .light))
    let warnings = outcome.issues.filter { $0.severity == .warning }.map(\.message)
    XCTAssertTrue(
      warnings.contains("platforms.ios.palette is not allowed: colour is the same on every platform; ignored"),
      "\(warnings)")
    XCTAssertTrue(warnings.contains("platforms.windows is not a theme setting; ignored"), "\(warnings)")
  }

  /// A bad value for the phone is worth knowing about on the Mac, where the
  /// file is edited, even though the Mac never uses it.
  func testABadValueInAnyPlatformEntryIsReportedOnEveryPlatform() throws {
    let file = source(
      "x.json", ##"{ "platforms": { "android": { "structure": { "radius": { "panel": -2 } } } } }"##)
    for platform in ThemePlatform.allCases {
      let outcome = try XCTUnwrap(load(for: platform, file).outcomes.first)
      XCTAssertNotNil(outcome.specification)
      XCTAssertTrue(
        messages(outcome).contains("platforms.android.structure.radius.panel -2 should be zero or more"),
        "\(platform): \(messages(outcome))")
    }
    XCTAssertEqual(load(for: .android, file).themes.first?.structure.radius.panel, 8)
  }

  func testATouchTargetIsDecodedAndAudited() throws {
    let file = source("x.json", ##"{ "structure": { "touchTarget": 30 } }"##)
    let outcome = try XCTUnwrap(load(for: .ios, file).outcomes.first)
    XCTAssertEqual(outcome.specification?.structure.touchTarget, 30)
    let audit = try XCTUnwrap(outcome.issues.first { $0.isAudit && $0.message.contains("touch target") })
    XCTAssertEqual(audit.severity, .warning)
    XCTAssertEqual(
      ThemeStructureAudit.findings(for: BuiltInThemeSpecifications.chalk(for: .android).structure), [])
  }

  /// An export from any platform carries the other platforms' differences,
  /// so the file resolves to the same theme wherever it is read.
  func testAnExportWithPlatformVariantsResolvesTheSameEverywhere() throws {
    let dusk = source(
      "dusk.json",
      ##"{ "structure": { "radius": { "control": 2 } }, "platforms": { "ios": { "structure": { "typography": { "bodySize": 18 } } } } }"##)
    var variants: [ThemePlatform: ThemeSpecification] = [:]
    for platform in ThemePlatform.allCases {
      variants[platform] = try XCTUnwrap(load(for: platform, dusk).themes.first)
    }
    var file = ThemeFile(specification: try XCTUnwrap(variants[.macos]), platformVariants: variants)
    file.identifier = "user.copy"
    XCTAssertNil(file.platforms?.macos)
    let copy = ThemeFileSource(name: "copy.json", data: try file.encoded())
    for platform in ThemePlatform.allCases {
      let reloaded = try XCTUnwrap(ThemeFileLoader.load([copy], platform: platform).themes.first)
      XCTAssertEqual(reloaded.structure, variants[platform]?.structure, platform.rawValue)
    }
  }

  func testTheSharedChalkFileResolvesToTheBuiltInOnEveryPlatform() throws {
    for builtIn in BuiltInThemeSpecifications.all {
      let data = try ThemeConformance.sharedFile(for: builtIn).encoded()
      XCTAssertEqual(ThemeFileLoader.decode(data, source: "shared.json").1, [])
      for platform in ThemePlatform.allCases {
        let library = ThemeFileLoader.load(
          [ThemeFileSource(name: "shared.json", data: data)], platform: platform, builtIns: [])
        let loaded = try XCTUnwrap(library.themes.first, "\(library.issues)")
        let expected = try XCTUnwrap(
          BuiltInThemeSpecifications.specification(withIdentifier: builtIn.identifier, for: platform))
        XCTAssertEqual(loaded.identifier, expected.identifier)
        XCTAssertEqual(loaded.lockedAppearance, expected.lockedAppearance)
        XCTAssertEqual(loaded.structure, expected.structure, "\(builtIn.name) on \(platform.rawValue)")
        for appearance in ThemeAppearance.allCases {
          XCTAssertEqual(
            ThemeConformance.Resolved(loaded, requested: appearance).colors,
            ThemeConformance.Resolved(expected, requested: appearance).colors)
        }
      }
    }
  }
}
