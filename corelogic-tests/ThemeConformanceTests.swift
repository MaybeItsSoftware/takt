import Foundation
import TaktCore
import XCTest

/// Holds `shared/themes/` to the Swift definitions.
///
/// The built-ins and the conformance cases there are generated from Swift, and
/// the other apps read them instead of restating Chalk or reimplementing the
/// resolver by eye. A difference fails here; run with
/// `TAKT_REGENERATE_THEMES=1` to rewrite the files from Swift.
final class ThemeConformanceTests: XCTestCase {
  private static let sharedThemes = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .appending(path: "shared/themes", directoryHint: .isDirectory)

  private var regenerating: Bool {
    ProcessInfo.processInfo.environment["TAKT_REGENERATE_THEMES"] == "1"
  }

  /// Every case, by file name. The inputs are written the way a person writes
  /// a theme, so a case is also an example.
  static let cases: [(name: String, files: [(String, String)], selected: String)] = [
    (
      "palette-only",
      [("violet.json", ##"{ "name": "Chalk, violet", "palette": { "light": { "primary": "#7a4de8" }, "dark": { "primary": "#9b7bf0" } } }"##)],
      "user.violet"
    ),
    (
      "structure-override",
      [("square.json", ##"{ "structure": { "radius": { "control": 0 }, "border": { "hairline": 2 }, "spacing": { "md": 10 } } }"##)],
      "user.square"
    ),
    (
      "platform-override",
      [(
        "dusk.json",
        """
        {
          "name": "Dusk",
          "palette": { "dark": { "paper": "#15131c" } },
          "structure": { "radius": { "control": 2 } },
          "platforms": {
            "ios": { "structure": { "typography": { "bodySize": 18 } } },
            "android": { "structure": { "spacing": { "md": 18 }, "touchTarget": 52 } }
          }
        }
        """
      )],
      "user.dusk"
    ),
    (
      "platform-palette-warning",
      [("tinted.json", ##"{ "platforms": { "android": { "palette": { "light": { "paper": "#000000" } }, "structure": { "radius": { "row": 4 } } } } }"##)],
      "user.tinted"
    ),
    (
      "extends-chain",
      [
        ("base.json", ##"{ "identifier": "mine.base", "palette": { "light": { "ink": "#222222" } }, "platforms": { "ios": { "structure": { "radius": { "panel": 12 } } } } }"##),
        ("middle.json", ##"{ "identifier": "mine.middle", "extends": "mine.base", "structure": { "border": { "emphasis": 3 } } }"##),
        ("top.json", ##"{ "extends": "mine.middle", "name": "Top", "palette": { "dark": { "primary": "#ff6b2b" } } }"##),
      ],
      "user.top"
    ),
    (
      "extends-chalk-dark",
      [("big-dark.json", ##"{ "name": "Big Dark", "extends": "native.theme.chalk.dark", "structure": { "radius": { "panel": 8, "row": 6, "control": 6 }, "typography": { "bodySize": 15 } } }"##)],
      "user.big-dark"
    ),
    (
      "locked-appearance-cleared",
      [("follows.json", ##"{ "extends": "native.theme.chalk.dark", "lockedAppearance": null }"##)],
      "user.follows"
    ),
    (
      "locked-appearance-light",
      [("bright.json", ##"{ "lockedAppearance": "light" }"##)],
      "user.bright"
    ),
    (
      "body-size-reproportions",
      [("roomy.json", ##"{ "structure": { "typography": { "bodySize": 15, "scale": { "hero": 80 } } } }"##)],
      "user.roomy"
    ),
    (
      "misspelt-key",
      [("typo.json", ##"{ "pallete": { "light": { "paper": "#ffffff" } }, "palette": { "light": { "backgroud": "#ffffff" } }, "structure": { "radius": { "pannel": 3 } } }"##)],
      "user.typo"
    ),
    (
      "bad-hex",
      [("bad.json", ##"{ "palette": { "light": { "paper": "#ggg", "ink": "#123456" } }, "structure": { "radius": { "panel": -1 }, "typography": { "microLabel": { "weight": "heavy" } } } }"##)],
      "user.bad"
    ),
    (
      "extends-null-missing-roles",
      [(
        "bare.json",
        """
        { "extends": null, "palette": { "light": {
          "paper": "#ffffff", "raised": "#ffffff", "altRow": "#f7f7f7", "hover": "#f0f0f0", "well": "#eeeeee",
          "border": "#dddddd", "borderMuted": "#eeeeee", "inputBorder": "#cccccc", "ink": "#111111",
          "mutedText": "#555555", "dimText": "#999999", "primary": "#0055cc", "success": "#118844",
          "danger": "#cc2233", "warning": "#aa7700", "categoricalPurple": "#7a4de8",
          "categoricalPink": "#ff88dc", "categoricalOrange": "#ff6b2b", "mediaLetterbox": "#000",
          "mediaScrim": "#000000b3", "mediaScrimInk": "#fff"
        }, "dark": { "paper": "#101010", "ink": "#eeeeee" } } }
        """
      ), ("hollow.json", ##"{ "extends": null, "palette": { "light": { "paper": "#ffffff" } } }"##)],
      "user.bare"
    ),
    (
      "extends-null-unpainted-falls-back",
      [("hollow.json", ##"{ "extends": null, "palette": { "light": { "paper": "#ffffff" } } }"##)],
      "user.hollow"
    ),
    (
      "cycle",
      [
        ("a.json", ##"{ "identifier": "mine.a", "extends": "mine.b" }"##),
        ("b.json", ##"{ "identifier": "mine.b", "extends": "mine.a" }"##),
        ("c.json", ##"{ "extends": "mine.nowhere" }"##),
      ],
      "mine.a"
    ),
    (
      "duplicate-and-builtin-identifiers",
      [
        ("one.json", ##"{ "identifier": "mine.same", "name": "One" }"##),
        ("two.json", ##"{ "identifier": "mine.same", "name": "Two" }"##),
        ("three.json", ##"{ "identifier": "native.theme.chalk" }"##),
        ("four.json", "not json"),
      ],
      "mine.same"
    ),
    ("builtin-chalk-dark", [], BuiltInThemeSpecifications.chalkDarkIdentifier),
    ("builtin-priority", [], BuiltInThemeSpecifications.priorityIdentifier),
    (
      "seeds-only",
      [
        (
          "sea.json",
          ##"{ "name": "Sea", "seeds": { "light": { "background": "#f4f8f9", "foreground": "#14303a", "accent": "#0b7a8c" }, "##
            + ##""dark": { "background": "#0e1d22", "foreground": "#dcecef", "accent": "#4fc3d4" } } }"##
        )
      ],
      "user.sea"
    ),
    (
      "seeds-with-overrides",
      [
        (
          "sea-tuned.json",
          ##"{ "name": "Sea, tuned", "seeds": { "light": { "background": "#f4f8f9", "foreground": "#14303a", "##
            + ##""accent": "#0b7a8c", "danger": "#c4314b" } }, "palette": { "light": { "border": "#c9d9dd" } } }"##
        )
      ],
      "user.sea-tuned"
    ),
    (
      "seeds-one-sided",
      [("half.json", ##"{ "name": "Half", "seeds": { "light": { "background": "#ffffff", "accent": "#ff0000", "glow": "#00ff00" } } }"##)],
      "user.half"
    ),
    (
      "extends-zed",
      [("zed-teal.json", ##"{ "name": "Zed, teal", "extends": "native.theme.chalk", "palette": { "light": { "primary": "#0b7a8c" } } }"##)],
      "user.zed-teal"
    ),
  ]

  func testTheSharedBuiltInFilesMatchSwift() throws {
    for (name, data) in try ThemeConformance.sharedFiles() {
      try check(Self.sharedThemes.appending(path: name), data)
    }
  }

  func testTheConformanceCasesMatchSwift() throws {
    let folder = Self.sharedThemes.appending(path: "conformance", directoryHint: .isDirectory)
    var expectedNames: Set<String> = []
    for testCase in Self.cases {
      let built = ThemeConformance.Case(
        files: testCase.files.map { ThemeConformance.File(name: $0.0, json: $0.1) },
        selected: testCase.selected)
      let name = "\(testCase.name).json"
      expectedNames.insert(name)
      try check(folder.appending(path: name), try built.encoded())
    }
    let present = Set(
      (try? FileManager.default.contentsOfDirectory(atPath: folder.path))?.filter { $0.hasSuffix(".json") } ?? [])
    for stale in present.subtracting(expectedNames) {
      if regenerating {
        try FileManager.default.removeItem(at: folder.appending(path: stale))
      } else {
        XCTFail("shared/themes/conformance/\(stale) is not a case any more; regenerate")
      }
    }
  }

  /// The cases say what they are meant to say, so a resolver change that
  /// quietly empties one is caught here rather than shipped as the truth.
  func testTheCasesExerciseWhatTheirNamesSay() throws {
    func built(_ name: String) throws -> ThemeConformance.Case {
      let testCase = try XCTUnwrap(Self.cases.first { $0.name == name })
      return ThemeConformance.Case(
        files: testCase.files.map { ThemeConformance.File(name: $0.0, json: $0.1) },
        selected: testCase.selected)
    }
    let dusk = try built("platform-override")
    XCTAssertEqual(dusk.expected["ios"]?["light"]?.structure.typography.bodySize, 18)
    XCTAssertEqual(dusk.expected["android"]?["light"]?.structure.spacing["md"], 18)
    XCTAssertEqual(dusk.expected["macos"]?["light"]?.structure.radius["control"], 2)

    let typo = try built("misspelt-key")
    XCTAssertTrue(typo.issues.contains { $0.severity == "warning" && $0.message.hasPrefix("pallete") })

    let bad = try built("bad-hex")
    XCTAssertTrue(bad.issues.contains { $0.severity == "error" && $0.message.contains("#ggg") })
    XCTAssertEqual(
      bad.expected["macos"]?["light"]?.colors["paper"],
      BuiltInThemeSpecifications.priority.palette.light[.paper]?.hexString.lowercased(),
      "the default's value is kept")
    XCTAssertEqual(bad.expected["macos"]?["light"]?.colors["ink"], "#123456")

    let cycle = try built("cycle")
    XCTAssertEqual(cycle.expected["macos"]?["light"]?.identifier, BuiltInThemeSpecifications.defaultIdentifier)

    let sea = try built("seeds-only")
    XCTAssertEqual(sea.expected["macos"]?["light"]?.colors["paper"], "#f4f8f9", "background is the page")
    XCTAssertEqual(sea.expected["macos"]?["light"]?.colors["ink"], "#14303a", "foreground is the text")
    XCTAssertEqual(sea.expected["macos"]?["dark"]?.colors["primary"], "#4fc3d4")
    XCTAssertNotEqual(sea.expected["macos"]?["light"]?.colors["border"], sea.expected["macos"]?["light"]?.colors["paper"])
    XCTAssertTrue(sea.issues.filter { $0.severity == "error" }.isEmpty)

    let tuned = try built("seeds-with-overrides")
    XCTAssertEqual(tuned.expected["macos"]?["light"]?.colors["border"], "#c9d9dd", "palette beats seeds")
    XCTAssertEqual(tuned.expected["macos"]?["light"]?.colors["danger"], "#c4314b")

    let half = try built("seeds-one-sided")
    XCTAssertEqual(
      half.expected["macos"]?["light"]?.colors["ink"],
      BuiltInThemeSpecifications.priority.palette.light[.ink]?.hexString.lowercased(),
      "a missing seed comes from the theme it extends")
    XCTAssertTrue(half.issues.contains { $0.severity == "warning" && $0.message.contains("glow") })

    let zedTeal = try built("extends-zed")
    XCTAssertEqual(zedTeal.expected["macos"]?["light"]?.colors["paper"], "#faf8f4", "Zed's paper")
    XCTAssertEqual(zedTeal.expected["ios"]?["light"]?.structure.typography.bodySize, 15)

    let cleared = try built("locked-appearance-cleared")
    XCTAssertNil(cleared.expected["ios"]?["light"]?.lockedAppearance)
    XCTAssertEqual(cleared.expected["ios"]?["light"]?.appearance, "light")

    let bare = try built("extends-null-missing-roles")
    XCTAssertEqual(bare.expected["android"]?["dark"]?.colors["raised"], "#ffffff", "borrowed from light")
  }

  private func check(_ url: URL, _ expected: Data) throws {
    if regenerating {
      try FileManager.default.createDirectory(
        at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
      let current = try? Data(contentsOf: url)
      if current != expected { try expected.write(to: url, options: .atomic) }
      return
    }
    let current = try? Data(contentsOf: url)
    XCTAssertEqual(
      current.flatMap { String(bytes: $0, encoding: .utf8) }, String(bytes: expected, encoding: .utf8),
      "\(url.lastPathComponent) differs from Swift; run TAKT_REGENERATE_THEMES=1 swift test --filter ThemeConformanceTests")
  }
}
