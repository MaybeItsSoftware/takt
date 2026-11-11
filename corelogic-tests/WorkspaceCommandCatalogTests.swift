import XCTest

@testable import TaktCore

/// The catalogue is the only place the workspace's keys are written down, so
/// these are the properties that make that claim worth anything.
final class WorkspaceCommandCatalogTests: XCTestCase {

  func testEveryCommandHasAnEntry() {
    let missing = WorkspaceCommandID.allCases.filter { WorkspaceCommandCatalog.byID[$0] == nil }
    XCTAssertTrue(missing.isEmpty, "no catalogue entry for: \(missing.map(\.rawValue))")
    XCTAssertEqual(WorkspaceCommandCatalog.all.count, WorkspaceCommandID.allCases.count)
  }

  /// The entries are the core's (`core/src/keymap/catalogue.rs`); one with no
  /// `WorkspaceCommandID` case could never run on the Mac.
  func testEveryCoreEntryHasACommandID() {
    let unknown = WorkspaceCommandCatalog.coreEntries.map(\.id).filter { WorkspaceCommandID(rawValue: $0) == nil }
    XCTAssertTrue(unknown.isEmpty, "core entries with no WorkspaceCommandID case: \(unknown)")
  }

  func testNoCommandIsListedTwice() {
    var seen: Set<WorkspaceCommandID> = []
    for command in WorkspaceCommandCatalog.all {
      XCTAssertTrue(seen.insert(command.id).inserted, "\(command.id.rawValue) listed twice")
    }
  }

  /// Two rows on the same surface claiming one key is a contradiction: the
  /// reference would be telling you a key does two things at once.
  ///
  /// A surface-specific row *shadowing* an `anywhere` row is not that — it is
  /// how the router already works, since the timeline takes the keyboard
  /// before the workspace's own keys are consulted.
  func testNoKeyIsClaimedTwiceOnOneSurface() {
    var owners: [String: [String]] = [:]
    for command in WorkspaceCommandCatalog.defaults {
      for key in command.keys {
        owners["\(command.surface.rawValue)/\(key)", default: []].append(command.title)
      }
      for (surface, keys) in command.surfaceKeys {
        for key in keys { owners["\(surface.rawValue)/\(key)", default: []].append(command.title) }
      }
    }
    let shared = owners.filter { $0.value.count > 1 }
    XCTAssertTrue(shared.isEmpty, "keys claimed by more than one command: \(shared)")
  }

  func testEveryRowPrintsSomethingReadable() {
    for command in WorkspaceCommandCatalog.all {
      XCTAssertFalse(command.title.isEmpty, "\(command.id.rawValue) has no title")
      XCTAssertFalse(command.group.isEmpty, "\(command.id.rawValue) has no group")
      XCTAssertEqual(
        command.displayKeys.count, command.allKeys.count,
        "\(command.id.rawValue) has a key that renders to nothing: \(command.allKeys)")
    }
  }

  /// Few rows are allowed to have no shortcut: the handful of things done
  /// rarely enough that the palette is the right way to reach them. Anything
  /// else with an empty binding is a row nobody can reach from the keyboard.
  func testOnlyPaletteOnlyRowsLackAKey() {
    let keyless = WorkspaceCommandCatalog.defaults.filter(\.allKeys.isEmpty).map(\.id)
    XCTAssertEqual(
      keyless,
      [
        .focusManageConditions,
        // A column header's menu, done rarely: the palette and the header's
        // right-click reach them.
        .planBoardRenameColumn,
        .planBoardAddColumnLeft, .planBoardAddColumnRight, .planBoardEditColumnRules,
        .planBoardMakeDefaultColumns, .planBoardResetColumns,
        // Sharing a list, and handing a task in one to someone: rare, and
        // done from the palette, the list's menu or the inspector.
        .listShare, .listJoin, .listMembers, .listLeave, .listStopSharing, .taskAssign,
        .windowUseClaudeCode, .windowUseCodex,
        .windowOpenKeymap, .windowReloadKeymap, .windowShowDiagnostics,
        .windowOpenThemesFolder, .windowReloadThemes, .windowExportTheme,
      ])
  }

  func testTheTaskMenuCarriesEveryTaskAction() {
    let listed = WorkspaceCommandCatalog.taskMenu.flatMap { $0 }
    XCTAssertEqual(listed.count, Set(listed).count, "the Task menu lists a command twice")
    let expected = WorkspaceCommandCatalog.defaults
      .filter { $0.group == "Task" && $0.kind == .action }
      .map(\.id)
      .filter { $0 != .taskNew && $0 != .taskClearPriority && $0 != .taskSendToBack }
    XCTAssertEqual(Set(listed), Set(expected))
  }

  /// Where a task command's only key was a two-letter sequence, it has a
  /// chord as well, so the Task menu can print one beside it.
  func testTheFieldEditorsHaveAChordForTheMenu() {
    for id: WorkspaceCommandID in [
      .taskRename, .taskEditDue, .taskEditNotes, .taskEditTags, .taskEditRecurrence,
      .taskDueToday, .taskDueTomorrow, .taskMove, .taskOpenLink,
    ] {
      XCTAssertTrue(
        WorkspaceCommandCatalog.defaults.first { $0.id == id }!.keys
          .contains(where: WorkspaceCommandCatalog.isChord),
        "\(id) has no chord")
    }
  }

  func testNamedKeysRenderAsSymbols() {
    XCTAssertEqual(WorkspaceCommandCatalog[.goToday].displayKeys, ["⌘T"])
    XCTAssertEqual(WorkspaceCommandCatalog[.taskRename].displayKeys, ["E E", "F2", "⌘⌃E"])
    XCTAssertEqual(
      WorkspaceCommandCatalog[.motionSelectEnds].displayKeys, ["Home", "End", "⌘↑", "⌘↓", "G G", "⇧G"])
    XCTAssertEqual(WorkspaceCommandCatalog[.taskDelete].displayKeys, ["⌫", "⌘⌫", "⌘⇧K"])
  }
}

final class WorkspaceCommandQueryTests: XCTestCase {

  func testAnEmptyQueryShowsEverything() {
    let matches = WorkspaceCommandQuery.matches(query: "", surface: .anywhere)
    XCTAssertEqual(matches.count, WorkspaceCommandCatalog.all.count)
  }

  func testTheCurrentSurfaceSortsFirst() {
    let matches = WorkspaceCommandQuery.matches(query: "", surface: .board)
    let firstOtherSurface = matches.firstIndex { $0.command.surface != .board }
    let lastBoard = matches.lastIndex { $0.command.surface == .board }
    XCTAssertNotNil(firstOtherSurface)
    XCTAssertNotNil(lastBoard)
    XCTAssertLessThan(lastBoard!, firstOtherSurface!)
  }

  /// A key you cannot press here is still worth knowing about, so rows for
  /// other surfaces sort last rather than disappearing.
  func testOtherSurfacesAreKeptButSortLast() {
    let matches = WorkspaceCommandQuery.matches(query: "", surface: .board)
    XCTAssertTrue(matches.contains { $0.command.surface == .timeline })
    XCTAssertEqual(matches.last?.command.surface.rawValue.isEmpty, false)
  }

  func testAPrefixBeatsAMatchInTheMiddle() {
    let matches = WorkspaceCommandQuery.matches(query: "add", surface: .anywhere)
    XCTAssertEqual(matches.first?.command.title, "Add a task")
  }

  func testTypingTheKeyFindsTheCommand() {
    let matches = WorkspaceCommandQuery.matches(query: "cmd+shift+d", surface: .anywhere)
    XCTAssertEqual(matches.first?.id, .taskToggleDaily)
  }

  func testInitialsFindACommand() {
    let matches = WorkspaceCommandQuery.matches(query: "adt", surface: .anywhere)
    XCTAssertTrue(matches.contains { $0.id == .taskNew }, "initials should reach 'Add a task'")
  }

  func testNonsenseMatchesNothing() {
    XCTAssertTrue(WorkspaceCommandQuery.matches(query: "zzqx", surface: .anywhere).isEmpty)
  }

  // MARK: - Fuzzy scoring

  func testAWordStartBeatsTheSameLettersMidWord() {
    let atWord = WorkspaceCommandQuery.fuzzyScore("col", in: "Add a board column")!
    let midWord = WorkspaceCommandQuery.fuzzyScore("col", in: "Recollect things")!
    XCTAssertGreaterThan(atWord, midWord)
  }

  func testContiguousLettersBeatScatteredOnes() {
    let together = WorkspaceCommandQuery.fuzzyScore("due", in: "Edit the due date")!
    let scattered = WorkspaceCommandQuery.fuzzyScore("due", in: "Drop your own order of the ladder")
    XCTAssertNotNil(scattered)
    XCTAssertGreaterThan(together, scattered!)
  }

  /// A greedy reading of "ta" in "xat ta" takes the first `t`, mid-word, and
  /// then an `a` two letters on. The best reading is the word "ta" itself.
  func testTheBestAlignmentIsChosenNotTheFirst() {
    let chosen = WorkspaceCommandQuery.fuzzyScore("ta", in: "xat ta")!
    let wordOnly = WorkspaceCommandQuery.fuzzyScore("ta", in: "xyz ta")!
    XCTAssertEqual(chosen, wordOnly)
  }

  func testAGapCostsMoreTheWiderItIs() {
    let near = WorkspaceCommandQuery.fuzzyScore("ab", in: "a-b")!
    let far = WorkspaceCommandQuery.fuzzyScore("ab", in: "a------b")!
    XCTAssertGreaterThan(near, far)
  }

  func testSpacesInTheQueryAreIgnored() {
    XCTAssertEqual(
      WorkspaceCommandQuery.fuzzyScore("due tom", in: "Due tomorrow"),
      WorkspaceCommandQuery.fuzzyScore("duetom", in: "Due tomorrow"))
  }

  func testWordInitialsRankTheirCommandFirst() {
    let matches = WorkspaceCommandQuery.matches(query: "dtom", surface: .outline)
    XCTAssertEqual(matches.first?.id, .taskDueTomorrow)
  }

  // MARK: - Recently run

  func testARecentCommandRisesAboveAnEqualMatch() {
    let plain = WorkspaceCommandQuery.matches(query: "add", surface: .outline)
    XCTAssertEqual(plain.first?.id, .taskNew)
    let recent = WorkspaceCommandQuery.matches(
      query: "add", surface: .outline, recents: [.taskNewChild])
    XCTAssertEqual(recent.first?.id, .taskNewChild)
  }

  func testRecentsLeadAnEmptyQueryMostRecentFirst() {
    let matches = WorkspaceCommandQuery.matches(
      query: "", surface: .outline, recents: [.listArchive, .goTimeline])
    XCTAssertEqual(matches.prefix(2).map(\.id), [.listArchive, .goTimeline])
  }

  func testRecencyDoesNotLiftAPoorMatchOverAGoodOne() {
    // "Open the keymap file" is a scattered match for "rename"; the rename
    // commands match it outright.
    let matches = WorkspaceCommandQuery.matches(
      query: "rename", surface: .outline, recents: [.windowOpenKeymap])
    XCTAssertTrue([.taskRename, .listRename].contains(matches.first?.id))
  }

  func testRecordingMovesToTheFrontAndCaps() {
    var recents: [WorkspaceCommandID] = []
    for id in WorkspaceCommandID.allCases.prefix(25) {
      recents = WorkspaceCommandQuery.recording(id, in: recents)
    }
    XCTAssertEqual(recents.count, WorkspaceCommandQuery.recentLimit)
    recents = WorkspaceCommandQuery.recording(recents[5], in: recents)
    XCTAssertEqual(recents.count, WorkspaceCommandQuery.recentLimit)
    XCTAssertEqual(Set(recents).count, recents.count)
  }

  func testSubsequenceIsNotSubstring() {
    XCTAssertTrue(WorkspaceCommandQuery.isSubsequence("adt", of: "add a task"))
    XCTAssertFalse(WorkspaceCommandQuery.isSubsequence("tda", of: "add a task"))
  }
}

/// The catalogue is now the only place the keys are written down: the router
/// reads it, the palette lists it, the tooltips quote it, and the main menu
/// derives its `keyboardShortcut` from it. That last one fails *silently* —
/// a token the parser does not recognise yields no menu key rather than an
/// error — so the shape of every token needs a test of its own.
final class WorkspaceCommandTokenShapeTests: XCTestCase {
  private static let modifiers: Set<String> = ["cmd", "shift", "option", "ctrl"]

  func testEveryModifierInEveryTokenIsOneTheAppKnows() {
    for command in WorkspaceCommandCatalog.all {
      for token in command.keys {
        let parts = token.split(separator: "+").map(String.init)
        guard parts.count > 1 else { continue }
        for modifier in parts.dropLast() {
          XCTAssertTrue(
            Self.modifiers.contains(modifier),
            "\(command.id) binds \(token), whose modifier \"\(modifier)\" is not one of \(Self.modifiers.sorted())")
        }
      }
    }
  }

  func testTokensAreLowercaseAndUnpadded() {
    for command in WorkspaceCommandCatalog.all {
      for token in command.keys {
        XCTAssertEqual(token, token.lowercased(), "\(command.id) binds \(token) with capitals")
        XCTAssertEqual(
          token, token.trimmingCharacters(in: .whitespaces),
          "\(command.id) binds \(token) with surrounding space")
      }
    }
  }

  /// `cmd` first is the repository's convention, and a tooltip renders tokens in
  /// the order they are written.
  func testCommandIsWrittenFirstAmongModifiers() {
    for command in WorkspaceCommandCatalog.all {
      for token in command.keys {
        let parts = token.split(separator: "+").map(String.init)
        guard let index = parts.firstIndex(of: "cmd") else { continue }
        XCTAssertEqual(index, 0, "\(command.id) binds \(token); cmd goes first")
      }
    }
  }

  func testEveryCommandRendersAtLeastOneKeyOrDeclaresItHasNone() {
    for command in WorkspaceCommandCatalog.all where !command.keys.isEmpty {
      XCTAssertFalse(
        command.displayKeys.isEmpty,
        "\(command.id) binds \(command.keys) but renders nothing, so no tooltip or palette row can show it")
    }
  }
}

/// Two commands that can both hear the same key on the same screen is a bug the
/// router resolves silently, by order of declaration — so the catalogue is asked
/// to be unambiguous instead.
final class WorkspaceCommandCollisionTests: XCTestCase {
  /// Only commands declared on the *same* surface are ambiguous. A surface
  /// command sharing a key with an `.anywhere` one is the intended arrangement,
  /// not a clash: `.anywhere` is the fallback and the surface overrides it, which
  /// is how `l` means "go in" everywhere and "log the block" while one is
  /// running. The scoring in `WorkspaceCommandQuery` is what makes that work,
  /// and a test that forbade it would forbid the design.
  func testNoTwoCommandsOnTheSameSurfaceShareAKey() {
    let all = WorkspaceCommandCatalog.defaults
    for (index, command) in all.enumerated() {
      for other in all[(index + 1)...] where command.surface == other.surface {
        let shared = Set(command.keys).intersection(other.keys)
        XCTAssertTrue(
          shared.isEmpty,
          "\(command.id) and \(other.id) both answer to \(shared.sorted()) on \(command.surface)")
      }
    }
  }

  /// A surface command that overrides an `.anywhere` one takes that key away on
  /// that screen, so each override should be a deliberate one. This records the
  /// set; a new entry in it means a key stopped working somewhere, and the
  /// question to ask is whether that was the intention.
  func testTheSurfaceOverridesAreTheOnesWeMeant() {
    var overrides: Set<String> = []
    let catalogue = WorkspaceCommandCatalog.defaults
    for command in catalogue {
      var own: [(WorkspaceCommandSurface, String)] = command.surfaceKeys.flatMap { surface, keys in
        keys.map { (surface, $0) }
      }
      if command.surface != .anywhere { own += command.keys.map { (command.surface, $0) } }
      for (surface, key) in own {
        for other in catalogue
        where other.surface == .anywhere && other.id != command.id && other.keys.contains(key) {
          overrides.insert("\(surface.rawValue):\(key)")
        }
      }
    }
    let expected: Set<String> = [
      // The timeline is a day at a time, so ←/→ and h/l step days rather than
      // entering and leaving a task.
      "timeline:left", "timeline:right", "timeline:escape",
      // In the sidebar the arrows walk rows and open folders, rename acts on the
      // row, ⌥↑/⌥↓ reorder the row rather than a task, and ⌘↑/⌘↓ go to the
      // sidebar's own ends.
      "sidebar:up", "sidebar:down", "sidebar:left", "sidebar:right",
      "sidebar:k", "sidebar:j", "sidebar:f2", "sidebar:cmd+up", "sidebar:cmd+down",
      "sidebar:option+up", "sidebar:option+down",
      // In the sidebar ⌥⌘↑/↓ reorder the folder; elsewhere they change list.
      "sidebar:cmd+option+up", "sidebar:cmd+option+down",
      "sidebar:home", "sidebar:end", "sidebar:pageup", "sidebar:pagedown",
      // : is the palette in the sidebar, as in Zed's vim project panel, and
      // vim's go-to-line over the rows everywhere else.
      "sidebar::",
      // - is Zed's select parent in the sidebar, and sends a task to the
      // back (priority -1) everywhere else.
      "sidebar:-",
      // The sidebar is Zed's project panel: ⌘N and ⌫ act on the row,
      // and so do the vim panel's h, l and gg, never on a task behind it.
      "sidebar:cmd+n", "sidebar:delete", "sidebar:cmd+delete", 
      "sidebar:gg",
      // On the board ←/→ change column instead of entering and leaving a task.
      "board:left", "board:right",
      // In the outline they fold and unfold a branch and step between a task
      // and its subtasks, as in Checkvist, so the subtasks can be walked
      // without opening the task. l and ] still open it.
      "outline:left", "outline:right",
      // The done rail is a list of its own, so the list keys walk it, Space
      // reopens the finished task, and escape gives the keyboard back to
      // the work rather than clearing a selection the rail does not hold.
      "done:up", "done:down", "done:k", "done:j", "done:space", "done:escape",
      "done:home", "done:end", "done:cmd+up", "done:cmd+down", "done:pageup", "done:pagedown",
      "done:left",
      // The timeline's breakdown is a list of its own too, walked with the
      // same keys.
      "timeline:up", "timeline:down", "timeline:k", "timeline:j", "timeline:home", "timeline:end",
      "timeline:cmd+up", "timeline:cmd+down", "timeline:pageup", "timeline:pagedown",
      // In the right dock ⌘{ ⌘} and [ ] change the dock's tab, Zed's previous
      // and next tab; on a task pane they change list and leave or open a task.
      "inspector:cmd+shift+{", "inspector:cmd+shift+}", "inspector:[", "inspector:]",
      // The inspector's task keys step through its fields, not the tasks.
      "inspector:down", "sidebar:shift+g", "sidebar:ctrl+u", "sidebar:ctrl+d", "inspector:up", "inspector:j", "inspector:k",
      // It is a vim buffer of rows: gg and ⇧G, Home and End are its ends,
      // Return and i edit the row, x and Delete clear it, ← leaves.
      "inspector:gg", "inspector:shift+g", "inspector:home", "inspector:end", "inspector:enter", "inspector:i",
      "inspector:x", "inspector:delete", "inspector:left",
      "done:cmd+shift+{", "done:cmd+shift+}", "done:[", "done:]",
      "timeline:cmd+shift+{", "timeline:cmd+shift+}", "timeline:[", "timeline:]",
      // The Conditions tab is a list of its own as well: the list keys walk
      // it, Space turns a condition on or off, Return renames it, and the
      // rest act on the row, never on a task behind it.
      "conditions:down", "conditions:up", "conditions:j", "conditions:k", "conditions:home", "conditions:end",
      "conditions:cmd+up", "conditions:cmd+down", "conditions:pageup", "conditions:pagedown",
      "conditions:space", "conditions:enter", "conditions:f2", "conditions:cmd+n", "conditions:escape",
      "conditions:left", "conditions:cmd+shift+{", "conditions:cmd+shift+}", "conditions:[", "conditions:]",
      // ⌥↑ ⌥↓ move the condition, and with ⇧ its category, as they move a task.
      "conditions:option+up", "conditions:option+down", "conditions:option+shift+up", "conditions:option+shift+down",
      // ← → change the setting under the cursor — where you are, the time you
      // have — and typing a number sets the time, rather than a task's
      // priority behind the tab.
      "conditions:right", "conditions:0", "conditions:1", "conditions:2", "conditions:3", "conditions:4",
      // Today is walked with the ordinary keys, but gf queues behind a
      // running block rather than replacing it, and → and ← open and fold a
      // task where it stands, as the outline's do, rather than entering and
      // leaving it.
      "today:right", "today:left", "today:gf",
      // R is the task's replace everywhere else; in the sidebar and on the
      // Conditions tab ⇧R renames the row the cursor is on.
      "sidebar:shift+r", "conditions:shift+r",
    ]
    XCTAssertEqual(
      overrides, expected,
      "the set of keys a surface takes over from .anywhere changed")
  }
}

final class WorkspaceCommandKeyMatchTests: XCTestCase {
  func testAKeyListsItselfThenItsSequencesThenItsChords() {
    let matches = WorkspaceCommandQuery.keyMatches(for: "m")
    XCTAssertFalse(matches.isEmpty)
    let firstKeys = matches.first?.allKeys ?? []
    XCTAssertTrue(
      firstKeys.contains("m") || firstKeys.contains("mm"),
      "m answers with what m (or a sequence starting with it) does, got \(firstKeys)")
    XCTAssertTrue(matches.contains { $0.allKeys.contains("mm") }, "mm, move, is offered for m")
  }

  func testAWordIsNotAKey() {
    XCTAssertTrue(WorkspaceCommandQuery.keyMatches(for: "move").isEmpty)
    XCTAssertTrue(WorkspaceCommandQuery.keyMatches(for: "  ").isEmpty)
  }
}
