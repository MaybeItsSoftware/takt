import XCTest

@testable import TaktCore

/// The key router asks the catalogue what a key means and runs the answer —
/// it has no list of its own. These pin that the answer is the one the
/// catalogue advertises, which is the property the old hand-written router
/// lost: its reference listed `⌘⌃C` and the ladder's `o`, and neither ran.
///
/// The other half — that every command id the catalogue resolves to actually
/// does something — is the compiler's: `WorkspaceViewModel.run(_:key:)` is an
/// exhaustive `switch` over `WorkspaceCommandID`, and the router has no other
/// way to act.
final class WorkspaceCommandDispatchTests: XCTestCase {
  private static let planningPanes: [WorkspaceCommandSurface] = [.today, .board, .outline, .matrix]

  /// A row that prints a key on a surface is a row that key runs there.
  func testEverySurfaceRowIsWhatItsKeysMeanOnItsSurface() {
    for command in WorkspaceCommandCatalog.all where command.surface != .anywhere {
      for key in command.keys {
        XCTAssertEqual(
          WorkspaceCommandCatalog.command(forKey: key, on: command.surface)?.id, command.id,
          "\(key) on \(command.surface) does not run \(command.id)")
      }
    }
    for command in WorkspaceCommandCatalog.all {
      for (surface, keys) in command.surfaceKeys {
        for key in keys {
          XCTAssertEqual(
            WorkspaceCommandCatalog.command(forKey: key, on: surface)?.id, command.id,
            "\(key) on \(surface) does not run \(command.id)")
        }
      }
    }
  }

  /// An `.anywhere` row is written from a planning pane's point of view, so
  /// on the outline — which overrides nothing — every one of its keys is live.
  func testEveryAnywhereRowIsLiveOnTheOutline() {
    // ←/→ are the outline's own: they fold, as Checkvist's do.
    let outlineOwn: Set<String> = ["left", "right"]
    for command in WorkspaceCommandCatalog.all where command.surface == .anywhere {
      for key in command.keys where !outlineOwn.contains(key) {
        XCTAssertEqual(
          WorkspaceCommandCatalog.command(forKey: key, on: .outline)?.id, command.id,
          "\(key) on the outline does not run \(command.id)")
      }
    }
  }

  func testTheKeysTheOldRouterLeftDeadNowRun() {
    XCTAssertEqual(
      WorkspaceCommandCatalog.command(forKey: "cmd+ctrl+c", on: .board)?.id, .planBoardRemoveColumn)
    // Each the first letter of a sequence, and so swallowed by it.
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "x", on: .outline)?.id, .taskComplete)
    // [ and ] open and leave; h and l are left free on the task panes.
    XCTAssertNil(WorkspaceCommandCatalog.command(forKey: "l", on: .outline))
    XCTAssertNil(WorkspaceCommandCatalog.command(forKey: "h", on: .outline))
  }

  func testTheOutlineArrowsFoldAndTheOtherPanesStillEnter() {
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "right", on: .outline)?.id, .motionOutlineUnfold)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "left", on: .outline)?.id, .motionOutlineFold)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "right", on: .matrix)?.id, .planEnterTask)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "right", on: .board)?.id, .motionBoardColumn)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "za", on: .board)?.id, .planToggleFold)
  }

  /// Today opens a task where it stands, as Checkvist does, with the arrows
  /// and with h and l; `]` still opens the task itself.
  func testTodaysArrowsAndHLFoldTheDay() {
    for key in ["right", "l"] {
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: key, on: .today)?.id, .motionTodayUnfold, key)
    }
    for key in ["left", "h"] {
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: key, on: .today)?.id, .motionTodayLeave, key)
    }
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "]", on: .today)?.id, .planEnterTask)
  }

  /// ⌘R shows and hides the right dock from every pane, the dock's own tabs
  /// included, and ⇧⌘R syncs; renaming the list is F2 and ⇧R in the
  /// sidebar, and F2 stays the task's rename on a task pane.
  func testCommandRTogglesTheRightDockFromEveryPane() {
    for surface in Self.planningPanes + [.sidebar, .inspector, .done, .timeline] {
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+r", on: surface)?.id, .windowToggleRightDock)
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+shift+r", on: surface)?.id, .windowSyncNow)
    }
    XCTAssertTrue(WorkspaceCommandCatalog.reachesIntoTextField("cmd+r", on: .inspector))
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "f2", on: .sidebar)?.id, .listRename)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "f2", on: .outline)?.id, .taskRename)
  }

  // MARK: - Surfaces that own the keyboard

  /// Keys a full-pane screen does not answer to used to fall through to the
  /// task surface hidden behind it: `⌫` deleted the selected outline task, a
  /// digit changed its priority, `i` opened its inspector.
  func testTheTimelineLetsNoTaskKeyThrough() {
    for surface: WorkspaceCommandSurface in [.timeline] {
      for key in ["delete", "1", "i", "tab", "shift+space", "cmd+n", "cmd+shift+delete", "ee"] {
        XCTAssertNil(
          WorkspaceCommandCatalog.command(forKey: key, on: surface),
          "\(key) reaches the hidden workspace from \(surface)")
        XCTAssertTrue(
          WorkspaceCommandCatalog.swallowsUnhandledKey(key, on: surface),
          "\(key) passes through \(surface) to the window")
      }
    }
  }

  func testTheWindowsOwnKeysStillWorkOnAScreenThatOwnsTheKeyboard() {
    for surface: WorkspaceCommandSurface in [.timeline] {
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+t", on: surface)?.id, .goToday)
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+k", on: surface)?.id, .goCommandPalette)
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "?", on: surface)?.id, .goKeyboardReference)
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+z", on: surface)?.id, .windowUndo)
      // Not the catalogue's, so the app's: ⌘Q and ⌘W must reach the menu.
      XCTAssertFalse(WorkspaceCommandCatalog.swallowsUnhandledKey("cmd+q", on: surface))
      XCTAssertFalse(WorkspaceCommandCatalog.swallowsUnhandledKey("cmd+w", on: surface))
    }
  }

  /// The timeline is a tab of the right dock, beside the work, so the keys
  /// that move between regions get out of it as they get out of the others.
  func testTheTimelineLetsTheRegionKeysThrough() {
    let expected: [String: WorkspaceCommandID] = [
      "ctrl+tab": .goCycleRegion, "ctrl+shift+tab": .goCycleRegion, "ctrl+1": .goSidebarRegion,
      "ctrl+2": .goTaskRegion, "ctrl+3": .goInspectorRegion, "cmd+r": .windowToggleRightDock,
      "escape": .timelineClose,
    ]
    for (key, id) in expected {
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: key, on: .timeline)?.id, id, key)
    }
  }

  /// The timeline's breakdown is walked and opened the way the done rail is.
  /// The Conditions tab: one cursor down both halves, Space turns the
  /// condition on or off, Return renames, Esc and ← go back to the work.
  func testTheConditionsTabHasItsOwnKeys() {
    for key in ["j", "k", "down", "up", "home", "end"] {
      XCTAssertEqual(
        WorkspaceCommandCatalog.command(forKey: key, on: .conditions)?.id, .motionConditionsSelect, key)
    }
    let expected: [String: WorkspaceCommandID] = [
      "space": .conditionsToggle, "enter": .conditionsEdit, "f2": .conditionsEdit, "a": .conditionsNew,
      "cmd+n": .conditionsNew, "t": .conditionsCategory, "p": .conditionsProvides, "e": .conditionsArchive,
      "c": .conditionsClear,
      "shift+u": .conditionsUseAgain, "escape": .conditionsClose, "left": .conditionsAdjust,
      "right": .conditionsAdjust, "h": .conditionsAdjust, "l": .conditionsAdjust, "4": .conditionsTypeTime,
    ]
    for (key, id) in expected {
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: key, on: .conditions)?.id, id, key)
    }
    // A bare key that acts on the selected task does not reach through it.
    XCTAssertNil(WorkspaceCommandCatalog.command(forKey: "x", on: .conditions))
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+r", on: .conditions)?.id, .windowToggleRightDock)
    // ⌥⌘8 and `ga` from inside the tab are the toggle that put it away.
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "ga", on: .conditions)?.id, .focusSetAvailable)
  }

  func testTheTimelineWalksItsTasks() {
    for key in ["j", "k", "down", "up", "home", "end"] {
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: key, on: .timeline)?.id, .motionTimelineSelect, key)
    }
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "o", on: .timeline)?.id, .timelineReveal)
  }

  // MARK: - The right dock's tabs

  /// ⌘{ ⌘} are Zed's previous / next tab. On a task pane a list is the tab;
  /// in the right dock its own tabs are, and `[` `]` do it where no field
  /// would take them.
  func testTheDocksTabsAreZedsPreviousAndNextTab() {
    for surface: WorkspaceCommandSurface in [.inspector, .done, .timeline, .conditions] {
      for key in ["cmd+shift+}", "]"] {
        XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: key, on: surface)?.id, .windowDockNextTab, "\(key) on \(surface)")
      }
      for key in ["cmd+shift+{", "["] {
        XCTAssertEqual(
          WorkspaceCommandCatalog.command(forKey: key, on: surface)?.id, .windowDockPreviousTab, "\(key) on \(surface)")
      }
    }
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+shift+}", on: .outline)?.id, .goNextList)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "]", on: .outline)?.id, .planEnterTask)
    XCTAssertNil(WorkspaceCommandCatalog.command(forKey: "]", on: .sidebar))
  }

  /// The inspector's keyboard is nearly always in a field, so the chords have
  /// to reach into one; the bare brackets are typed.
  func testTheDockTabChordsReachIntoTheInspectorsFields() {
    XCTAssertTrue(WorkspaceCommandCatalog.reachesIntoTextField("cmd+shift+}", on: .inspector))
    XCTAssertTrue(WorkspaceCommandCatalog.reachesIntoTextField("cmd+shift+{", on: .inspector))
    XCTAssertFalse(WorkspaceCommandCatalog.reachesIntoTextField("]", on: .inspector))
    // On a task pane the same chord changes list, which is not a way out of a field.
    XCTAssertFalse(WorkspaceCommandCatalog.reachesIntoTextField("cmd+shift+}", on: .outline))
  }

  /// Esc in one of the inspector's fields hands the keyboard back to the
  /// tasks; in any other field it is the field's.
  func testEscapeLeavesTheInspectorsFieldsOnly() {
    XCTAssertTrue(WorkspaceCommandCatalog.reachesIntoTextField("escape", on: .inspector))
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "escape", on: .inspector)?.id, .motionDismiss)
    for surface: WorkspaceCommandSurface in [.outline, .board, .today, .matrix, .sidebar, .done, .timeline, .conditions] {
      XCTAssertFalse(WorkspaceCommandCatalog.reachesIntoTextField("escape", on: surface), "\(surface)")
    }
  }

  func testOrdinarySurfacesPassUnknownKeysOn() {
    for surface in Self.planningPanes + [.sidebar, .inspector, .done] {
      XCTAssertFalse(WorkspaceCommandCatalog.swallowsUnhandledKey("z", on: surface))
    }
  }

  // MARK: - Regions

  /// The sidebar and the done rail hold cursors of their own, so a bare key
  /// that acts on "the selected task" must not act on a row in a pane that
  /// does not have the keyboard.
  func testRegionsDoNotTakeBareTaskKeys() {
    for surface: WorkspaceCommandSurface in [.sidebar, .done, .inspector] {
      // The inspector's x clears the row under its cursor, not the task.
      for key in ["x", "1", "0", "tab", "shift+enter", "option+enter"] where !(surface == .inspector && key == "x") {
        XCTAssertNil(
          WorkspaceCommandCatalog.command(forKey: key, on: surface),
          "\(key) acts on the selected task from \(surface)")
      }
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "?", on: surface)?.id, .goKeyboardReference)
    }
    for surface: WorkspaceCommandSurface in [.done, .inspector] {
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+n", on: surface)?.id, .taskNew)
      XCTAssertNil(WorkspaceCommandCatalog.command(forKey: "f", on: surface))
    }
    // The sidebar's f finds a list, as the task panes' finds a task.
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "f", on: .sidebar)?.id, .motionFindForward)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "escape", on: .sidebar)?.id, .motionDismiss)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "escape", on: .inspector)?.id, .motionDismiss)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "escape", on: .done)?.id, .doneClose)
  }

  /// The sidebar is Zed's project panel, with its vim panel's netrw keys:
  /// the keys act on the sidebar row, never on a task behind it.
  func testTheSidebarAnswersToZedsProjectPanelKeys() {
    let expected: [String: WorkspaceCommandID] = [
      "d": .folderNew, "shift+d": .listDelete, "delete": .listDelete, "cmd+delete": .listDelete,
      "shift+5": .listNew, "cmd+n": .listNew, "cmd+option+n": .folderNew,
      "shift+r": .listRename, "f2": .listRename,
      "h": .motionSidebarCollapse, "-": .motionSidebarCollapse,
      "l": .motionSidebarExpand,
      "gg": .motionSidebarSelect, "shift+g": .motionSidebarSelect,
      "{": .folderSelectPrevious, "}": .folderSelectNext, ":": .goCommandPalette,
      "cmd+left": .folderCollapseAll, "cmd+right": .folderExpandAll,
    ]
    for (key, id) in expected {
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: key, on: .sidebar)?.id, id, key)
    }
    // Only on the sidebar: on a task pane these still mean the task.
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "delete", on: .outline)?.id, .taskDelete)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+n", on: .outline)?.id, .taskNew)
  }

  /// `d` makes a folder at once rather than waiting to see whether it was the
  /// start of `dd`, and `⇧X`, which nothing binds, is still `x`.
  func testASurfacesOwnLetterIsNotHeldForAnInheritedSequence() {
    let sidebar = WorkspaceCommandCatalog.sequences(on: .sidebar)
    XCTAssertFalse(sidebar.contains("dd"))
    XCTAssertFalse(sidebar.contains("dr"))
    XCTAssertTrue(sidebar.contains("gh"))
    XCTAssertTrue(WorkspaceCommandCatalog.sequences(on: .outline).contains("dd"))
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "shift+x", on: .outline)?.id, .taskComplete)
    // `-` is the sidebar's select parent, so it isn't held for `-2` there,
    // and the minus priorities stay out of the panes with no task to set.
    XCTAssertFalse(sidebar.contains("-2"))
    for surface in [WorkspaceCommandSurface.done, .conditions, .inspector] {
      XCTAssertFalse(WorkspaceCommandCatalog.sequences(on: surface).contains("-2"), "\(surface)")
    }
  }

  /// A lone `-` still sends a task to the back once the hold runs out, and a
  /// digit straight after it sets one of the priorities behind the back.
  func testMinusAndADigitSetTheBackPriorities() {
    let outline = WorkspaceCommandCatalog.sequences(on: .outline)
    var sequence = DesktopShortcutSequence()
    XCTAssertEqual(sequence.advance("-", at: 0, sequences: outline), .pending)
    XCTAssertEqual(sequence.advance("2", at: 0.3, sequences: outline), .command("-2"))
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "-2", on: .outline)?.id, .motionSetPriority)

    XCTAssertEqual(sequence.advance("-", at: 5, sequences: outline), .pending)
    XCTAssertEqual(sequence.expire(heldAt: 5), "-")
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "-", on: .outline)?.id, .taskSendToBack)
  }

  // MARK: - Sequences

  /// These were a hand-written list beside the catalogue; now they are read
  /// out of it. The set is recorded so a change to it is a deliberate one.
  func testSequencesAreDerivedFromTheCatalogue() {
    XCTAssertEqual(
      WorkspaceCommandCatalog.sequences(on: .outline),
      [
        "ee", "ea", "ei", "dd", "ds", "nn", "nm", "tt", "mm", "gl", "uu", "td", "tm", "bu", "cd", "cs", "cn", "ct", "go",
        "dr", "tc", "tl", "gb", "ww", "bb", "cc", "aa", "gh", "sd", "oo", "pc", "pl", "ex", "gg", "za", "gf", "gx", "ga", "gw",
        "-1", "-2", "-3", "-4",
      ])
    let written = Set(
      WorkspaceCommandCatalog.all.flatMap(\.allKeys).filter(WorkspaceCommandCatalog.isSequence))
    XCTAssertEqual(WorkspaceCommandCatalog.sequences(on: .outline), written)
  }

  /// Its one sequence is vim's gg, to the top row; nothing a task pane
  /// holds for a sequence reaches it.
  func testTheInspectorsOnlySequenceIsGG() {
    XCTAssertEqual(WorkspaceCommandCatalog.sequences(on: .inspector), ["gg"])
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "gg", on: .inspector)?.id, .motionInspectorEnds)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "x", on: .inspector)?.id, .motionInspectorClear)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "enter", on: .inspector)?.id, .motionInspectorEdit)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "h", on: .inspector)?.id, .motionInspectorLeave)
  }

  /// Only the window's own sequences survive a screen that owns the keyboard,
  /// and none of them begins with a letter the screen uses on its own — that
  /// letter would wait out the hold on every press.
  func testFullPaneScreensHoldNoKeyTheyUseThemselves() {
    for surface: WorkspaceCommandSurface in [.timeline] {
      let starters = Set(WorkspaceCommandCatalog.sequences(on: surface).compactMap(\.first).map(String.init))
      let own = Set(WorkspaceCommandCatalog.all.filter { $0.surface == surface }.flatMap(\.keys))
      XCTAssertTrue(
        starters.isDisjoint(with: own),
        "\(surface) holds \(starters.intersection(own).sorted()) for a sequence")
    }
  }

  func testKeyNamesAreNotSequences() {
    XCTAssertFalse(WorkspaceCommandCatalog.isSequence("up"))
    XCTAssertFalse(WorkspaceCommandCatalog.isSequence("f2"))
    XCTAssertTrue(WorkspaceCommandCatalog.isSequence("gh"))
  }

  // MARK: - Text fields

  func testOnlyTheWayOutChordsReachIntoATextField() {
    XCTAssertTrue(WorkspaceCommandCatalog.reachesIntoTextField("cmd+k", on: .outline))
    XCTAssertTrue(WorkspaceCommandCatalog.reachesIntoTextField("cmd+2", on: .today))
    XCTAssertTrue(WorkspaceCommandCatalog.reachesIntoTextField("ctrl+shift+tab", on: .outline))
    XCTAssertFalse(WorkspaceCommandCatalog.reachesIntoTextField("cmd+z", on: .outline))
    // Bare, so typed: Return belongs to the field even though it adds a task.
    XCTAssertFalse(WorkspaceCommandCatalog.reachesIntoTextField("enter", on: .outline))
    XCTAssertFalse(WorkspaceCommandCatalog.reachesIntoTextField("/", on: .outline))
  }

  // MARK: - Spelling a key press

  private func key(
    _ code: UInt16, _ characters: String,
    shift: Bool = false, ctrl: Bool = false, cmd: Bool = false, option: Bool = false
  ) -> String? {
    WorkspaceCommandCatalog.key(
      keyCode: code, charactersIgnoringModifiers: characters,
      shift: shift, ctrl: ctrl, cmd: cmd, option: option)
  }

  func testKeyPressesAreSpelledTheWayTheCatalogueSpellsThem() {
    XCTAssertEqual(key(8, "c", ctrl: true, cmd: true), "cmd+ctrl+c")
    XCTAssertEqual(key(8, "C", shift: true, cmd: true), "cmd+shift+c")
    XCTAssertEqual(key(44, "?", shift: true), "?")
    XCTAssertEqual(key(7, "X", shift: true), "shift+x")
    XCTAssertEqual(key(23, "%", shift: true), "shift+5")
    XCTAssertEqual(key(27, "-"), "-")
    XCTAssertEqual(key(36, "\r", shift: true), "shift+enter")
    XCTAssertEqual(key(76, "\u{3}"), "enter")
    XCTAssertEqual(key(115, "\u{F729}"), "home")
    XCTAssertEqual(key(121, "\u{F72D}"), "pagedown")
    XCTAssertEqual(key(48, "\t", shift: true, ctrl: true), "ctrl+shift+tab")
    XCTAssertEqual(key(18, "1", option: true), "option+1")
    XCTAssertEqual(key(51, "\u{7F}", shift: true, cmd: true), "cmd+shift+delete")
    XCTAssertEqual(key(126, "\u{F700}", ctrl: true, option: true), "ctrl+option+up")
    XCTAssertNil(key(56, ""))
  }

  /// A catalogue key no key press can be spelled as is a dead key by
  /// construction. The spelling puts modifiers in one order and never writes
  /// Shift on a bare character but a letter or a named key, so the catalogue
  /// must not either.
  func testEveryCatalogueKeyIsOneAKeyPressCanProduce() {
    let order = ["cmd", "ctrl", "option", "shift"]
    for command in WorkspaceCommandCatalog.all {
      for token in command.allKeys {
        let parts = token.split(separator: "+").map(String.init)
        let modifiers = Array(parts.dropLast())
        XCTAssertEqual(
          modifiers, order.filter(modifiers.contains),
          "\(command.id) binds \(token), whose modifiers are out of order")
        if modifiers == ["shift"], let base = parts.last, base.count == 1,
          base.first?.isLetter != true, !WorkspaceCommandCatalog.keyNames.contains(base) {
          XCTFail("\(command.id) binds \(token); Shift on a character is already in the character")
        }
      }
    }
  }

  /// ⌃⌥⇧⌘D is quick add's global hotkey and ⌃⌥⇧⌘F the focus panel's. If
  /// either reached the window as a key, it must not be a command there —
  /// in particular not ⌘D's or `dd`'s due date.
  func testTheGlobalHotkeysAreNoWindowCommand() {
    for (code, character) in [(UInt16(2), "d"), (UInt16(3), "f")] {
      let hyper = key(code, character, shift: true, ctrl: true, cmd: true, option: true)
      XCTAssertEqual(hyper, "cmd+ctrl+option+shift+\(character)")
      for surface in WorkspaceCommandSurface.allCases {
        XCTAssertNil(WorkspaceCommandCatalog.command(forKey: hyper!, on: surface), "\(surface)")
      }
    }
  }

  /// Zed's previous and next tab move between lists. ⌘{ arrives as Shift-[
  /// with Command held, so it is spelled with the brace the key produces.
  func testZedsTabKeysMoveBetweenLists() {
    let brace = WorkspaceCommandCatalog.key(
      keyCode: 33, charactersIgnoringModifiers: "{", shift: true, ctrl: false, cmd: true, option: false)
    XCTAssertEqual(brace, "cmd+shift+{")
    for surface in [WorkspaceCommandSurface.outline, .board, .sidebar] {
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+shift+{", on: surface)?.id, .goPreviousList)
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+shift+}", on: surface)?.id, .goNextList)
    }
  }

  /// ⇧⌘. stops a focus block and ⇧⌘↩ finishes it, from every surface and
  /// from inside a field. ⇧⌘. is spelled by its key: Shift would make the
  /// character `>`.
  func testTheFocusBlockCanBeEndedFromAnywhere() {
    let stop = WorkspaceCommandCatalog.key(
      keyCode: 47, charactersIgnoringModifiers: ">", shift: true, ctrl: false, cmd: true, option: false)
    XCTAssertEqual(stop, "cmd+shift+.")
    let finish = WorkspaceCommandCatalog.key(
      keyCode: 36, charactersIgnoringModifiers: "\r", shift: true, ctrl: false, cmd: true, option: false)
    XCTAssertEqual(finish, "cmd+shift+enter")
    for surface in WorkspaceCommandSurface.allCases {
      XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+shift+.", on: surface)?.id, .focusStop, "\(surface)")
      XCTAssertEqual(
        WorkspaceCommandCatalog.command(forKey: "cmd+shift+enter", on: surface)?.id, .focusFinish, "\(surface)")
      XCTAssertTrue(WorkspaceCommandCatalog.reachesIntoTextField("cmd+shift+.", on: surface))
      XCTAssertTrue(WorkspaceCommandCatalog.reachesIntoTextField("cmd+shift+enter", on: surface))
    }
    XCTAssertEqual(WorkspaceCommandCatalog[.focusStop].displayKeys, ["⌘⇧."])
  }

  /// ⌥⌘8 sits beside ⌘8 (focus) and, like ending a block, reaches past a
  /// text field: saying where you are should not need the typing put down.
  /// Not from the timeline, which takes only its short list: its other key
  /// is `ga`, and admitting it would make `g` a sequence starter there.
  func testWhatIsAvailableNowIsOptionCommandEightFromAnywhere() {
    let key = WorkspaceCommandCatalog.key(
      keyCode: 28, charactersIgnoringModifiers: "8", shift: false, ctrl: false, cmd: true, option: true)
    XCTAssertEqual(key, "cmd+option+8")
    for surface in WorkspaceCommandSurface.allCases where surface != .timeline {
      XCTAssertEqual(
        WorkspaceCommandCatalog.command(forKey: "cmd+option+8", on: surface)?.id, .focusSetAvailable, "\(surface)")
      XCTAssertTrue(WorkspaceCommandCatalog.reachesIntoTextField("cmd+option+8", on: surface))
    }
    XCTAssertEqual(WorkspaceCommandCatalog[.focusSetAvailable].displayKeys.first, "⌘⌥8")
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "ga", on: .outline)?.id, .focusSetAvailable)
    XCTAssertTrue(WorkspaceCommandCatalog[.focusManageConditions].allKeys.isEmpty)
  }

  /// The full stop is spelled by its key only in a chord; bare, it is still
  /// whatever the layout types there.
  func testABareFullStopIsItsCharacter() {
    let bare = WorkspaceCommandCatalog.key(
      keyCode: 47, charactersIgnoringModifiers: ":", shift: false, ctrl: false, cmd: false, option: false)
    XCTAssertEqual(bare, ":")
  }

  /// ⌘← and ⌘→ move a card on the board, and still fold in the outline.
  func testCommandArrowsMoveACardOnlyOnTheBoard() {
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+left", on: .board)?.id, .planBoardMoveCardLeft)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+right", on: .board)?.id, .planBoardMoveCardRight)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+left", on: .outline)?.id, .planFoldAll)
  }

  func testEaAndEiEditAtTheEndAndTheStart() {
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "ea", on: .outline)?.id, .taskRenameAppend)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "ei", on: .outline)?.id, .taskRenameInsert)
  }

  /// Zed's back and forward. The minus key is named whatever Shift does, so
  /// ⌃⇧- arrives as `ctrl+shift+-`.
  func testZedsBackAndForwardWalkTheListsYouWereIn() {
    let back = WorkspaceCommandCatalog.key(
      keyCode: 27, charactersIgnoringModifiers: "-", shift: false, ctrl: true, cmd: false, option: false)
    let forward = WorkspaceCommandCatalog.key(
      keyCode: 27, charactersIgnoringModifiers: "_", shift: true, ctrl: true, cmd: false, option: false)
    XCTAssertEqual(back.flatMap { WorkspaceCommandCatalog.command(forKey: $0, on: .outline)?.id }, .goBack)
    XCTAssertEqual(forward.flatMap { WorkspaceCommandCatalog.command(forKey: $0, on: .outline)?.id }, .goForward)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+option+down", on: .board)?.id, .goNextList)
    XCTAssertEqual(WorkspaceCommandCatalog.command(forKey: "cmd+option+right", on: .outline)?.id, .taskIndent)
  }
}
