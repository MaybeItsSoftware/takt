import Foundation

/// Resolving a key press to a catalogue entry, which is the whole of what the
/// desktop window's key router does.
///
/// The router used to be a 400-line `switch` over key codes beside the
/// catalogue it was meant to agree with, plus a second hand-written copy of the
/// two-letter sequences. Nothing held the three equal, so the catalogue
/// advertised keys the switch never ran — `⌘⌃C` to remove a board column, `o`
/// to reset the focus ladder — and the switch ran keys the catalogue never
/// mentioned. The rules for which key means what on which surface live here
/// instead, where they can be tested, and the router only asks.
extension WorkspaceCommandSurface {
  /// The timeline takes the keyboard outright: a key it does not answer to
  /// must not reach the task surface beside it, where `⌫` would delete a task
  /// the keyboard is not on. It was a full-pane screen when this was written,
  /// and is a tab of the right dock now; the reasoning held.
  public var ownsKeyboard: Bool {
    self == .timeline
  }
}

extension WorkspaceCommandCatalog {

  // MARK: - Which keys reach which surface

  /// The only `.anywhere` commands that stay live on a screen that owns the
  /// keyboard. Each either leaves the screen (the view keys), puts something
  /// in front of it (the palette, the reference, search), or belongs to the
  /// window rather than to the pane.
  ///
  /// `goListNavigator` is deliberately absent. One of its keys is `gl`, and
  /// admitting it would make `g` a sequence starter on full-pane screens that
  /// have no other use for one.
  public static let reachableFromFullPaneScreens: Set<WorkspaceCommandID> = [
    .goToday, .goBoard, .goOutline, .goMatrix, .goEverything, .goInbox, .goFocus, .goTimeline,
    .goSearch, .goCommandPalette, .goKeyboardReference,
    .windowUndo, .windowRedo, .windowToggleSidebar, .windowToggleInspectorPane,
    .windowToggleRightDock, .windowToggleDoneRail, .windowToggleAgentPanel, .windowToggleProgressDock,
    .windowCloseAllDocks, .windowSyncNow,
    // A running block can be ended from anywhere, the timeline included.
    .focusFinish, .focusStop,
    // The timeline sits in the right dock beside the work, so the keys that
    // move between regions have to get out of it as they get out of the rest.
    .goSidebarRegion, .goTaskRegion, .goInspectorRegion, .goCycleRegion,
  ]

  /// Commands whose chord still works with the caret in a text field, because
  /// each is how you leave the field to do something else. Undo is not one of
  /// them: inside a field `⌘Z` belongs to the text being typed.
  public static let reachableFromTextField: Set<WorkspaceCommandID> = [
    .goToday, .goBoard, .goOutline, .goMatrix, .goEverything, .goInbox, .goFocus, .goTimeline,
    .goSidebarRegion, .goTaskRegion, .goInspectorRegion, .goCycleRegion,
    .taskNew, .listNew, .folderNew, .listNewTaskDestination,
    .goKeyboardReference, .goCommandPalette, .goSearch, .goListNavigator,
    // The agent's own field is a text field, and this is how you leave it.
    .windowToggleAgentPanel,
    // ⌘B, ⌘J, ⌥⌘B and ⌥⌘Y, as in Zed, where they work from inside the editor.
    .windowToggleSidebar, .windowToggleProgressDock, .windowToggleRightDock,
    .windowCloseAllDocks,
    // ⌘{ and ⌘} in the inspector, whose keyboard is nearly always in a field.
    .windowDockNextTab, .windowDockPreviousTab,
    // ⌘R syncs, as a reload does in a browser: no reason to stop typing.
    .windowSyncNow,
    // Ending a block is not something to put the typing down for first.
    .focusFinish, .focusStop,
    // ⌥⌘8, beside ⌘8: saying where you are is part of opening the panel.
    .focusSetAvailable,
  ]

  /// Surfaces where Esc in a text field is the surface's Esc rather than the
  /// field's. The inspector is a column of fields and opens with the caret in
  /// its title, so an Esc the field kept was an Esc that did nothing, and the
  /// way back to the tasks was ⌃Tab or the mouse. What was typed stays in the
  /// draft; leaving the field does not throw it away.
  public static let escapeLeavesTextField: Set<WorkspaceCommandSurface> = [.inspector]

  /// Bare keys a region (sidebar, inspector, done rail) takes from `.anywhere`.
  /// None of them acts on a task row: they open something, or leave — and
  /// Return adds a task, which it does from everywhere, row or no row. `r`
  /// is the right dock's, so the key that opened it puts it away from inside.
  static let regionBareKeys: Set<String> = ["/", "?", "i", "r", "escape", "enter"]

  /// The command a key means on the surface on screen, or `nil` when it means
  /// nothing there.
  ///
  /// A surface's own rows win over `.anywhere` ones. What a surface takes from
  /// `.anywhere` at all depends on what it is:
  ///
  /// - The four planning panes take everything: `.anywhere` is written from
  ///   their point of view.
  /// - The sidebar, the done rail and the conditions tab hold a cursor of
  ///   their own, so a bare key that acts on "the selected task" — Space,
  ///   `x`, `⌫`, a digit — would act on a row in a pane that does not have
  ///   the keyboard. They take chords
  ///   (anything with `⌘` or `⌃`), the two-letter sequences, and
  ///   `regionBareKeys`.
  /// - The inspector is the same minus the sequences, because its controls
  ///   are where your typing goes.
  /// - The timeline takes only `reachableFromFullPaneScreens`.
  ///
  /// A row's `surfaceKeys` count as that surface's own, and a keymap can take
  /// a key away from one surface without taking it from the rest — see
  /// `WorkspaceKeyBindings`. Everything here reads the bindings in force, so
  /// a user keymap changes what a key does and what every reader prints.
  public static func command(
    forKey key: String,
    on surface: WorkspaceCommandSurface
  ) -> WorkspaceCommand? {
    bindings.command(forKey: key, on: surface)
  }

  /// Whether the key should stop at the window even though nothing on the
  /// surface answers to it.
  ///
  /// On a screen that owns the keyboard, everything does except a chord the
  /// catalogue has never heard of — `⌘Q`, `⌘W`, `⌘,` belong to the app, not
  /// to the pane. A chord the catalogue *does* know is swallowed rather than
  /// passed on, because the main menu carries many of them and would run the
  /// command on the hidden workspace anyway.
  public static func swallowsUnhandledKey(_ key: String, on surface: WorkspaceCommandSurface) -> Bool {
    bindings.swallowsUnhandledKey(key, on: surface)
  }

  /// Whether a chord is one of the handful that work from inside a text field.
  public static func reachesIntoTextField(_ key: String, on surface: WorkspaceCommandSurface) -> Bool {
    bindings.reachesIntoTextField(key, on: surface)
  }

  /// The two-letter sequences live on a surface — derived from the catalogue,
  /// so a sequence exists on a surface exactly when a row there prints it.
  public static func sequences(on surface: WorkspaceCommandSurface) -> Set<String> {
    bindings.sequences(on: surface)
  }

  // MARK: - Key shapes

  /// Carries `⌘` or `⌃`, and so cannot be typed into a field by accident.
  /// `⌥` and `⇧` alone do not count: `⌥↩` and `⇧↩` are task-pane gestures.
  public static func isChord(_ key: String) -> Bool {
    key.hasPrefix("cmd+") || key.hasPrefix("ctrl+")
  }

  /// `ee`, `gh` — two bare letters, Checkvist's spelling. Not `up`, which is
  /// two bare letters too but names a key. Or `-2`: a minus and a digit, the
  /// priorities behind the back, as `!-2` writes them.
  public static func isSequence(_ key: String) -> Bool {
    isLetterSequence(key) || (key.count == 2 && key.first == "-" && key.last!.isASCII && key.last!.isNumber)
  }

  static func isLetterSequence(_ key: String) -> Bool {
    key.count == 2 && key.allSatisfy { $0.isLetter && $0.isLowercase } && !keyNames.contains(key)
  }

  /// Every key the spelling names rather than taking from its character.
  static let keyNames = Set(ShortcutKeyToken.nameByKeyCode.values)
    .union(extraNameByKeyCode.values)

  static func inherits(
    _ command: WorkspaceCommand,
    key: String,
    on surface: WorkspaceCommandSurface
  ) -> Bool {
    switch surface {
    case .anywhere, .today, .board, .outline, .matrix:
      return true
    case .timeline:
      return reachableFromFullPaneScreens.contains(command.id)
    case .sidebar, .done, .conditions:
      return isChord(key) || isLetterSequence(key) || regionBareKeys.contains(key)
    case .inspector:
      return isChord(key) || regionBareKeys.contains(key)
    }
  }

  // MARK: - Spelling a key press

  /// Keys the catalogue names but `ShortcutKeyToken` does not, because the
  /// menu bar's bindings never needed them.
  private static let extraNameByKeyCode: [UInt16: String] = [
    76: "enter",  // keypad Enter, the same key as far as the workspace cares
    115: "home", 119: "end", 116: "pageup", 121: "pagedown",
  ]

  /// The full stop's key code. In a chord it is spelled by the key rather
  /// than its character, so ⇧⌘. reads `cmd+shift+.` and not the `>` Shift
  /// makes of it. Only in a chord: a bare key is still its character, so a
  /// layout that puts `:` there keeps its `:`.
  private static let fullStopKeyCode: UInt16 = 47

  /// A key press spelled the way the catalogue spells its keys: modifiers in
  /// the order `cmd`, `ctrl`, `option`, `shift`, then the key.
  ///
  /// Shift is dropped from a printable character when no other modifier is
  /// held, because it is already in the character — `?` is Shift-/, and the
  /// catalogue writes `?`. A letter is the exception: `⇧D` is `shift+d`,
  /// because Zed's project panel gives `d` and `⇧D` different jobs (a new
  /// folder, and delete). A `shift+` letter nothing binds falls back to the
  /// letter, so `⇧X` is still `x` run at once rather than held for a
  /// sequence, which is the way to skip the wait.
  ///
  /// `nil` for a key with nothing to name, such as a bare modifier.
  public static func key(
    keyCode: UInt16,
    charactersIgnoringModifiers rawCharacters: String,
    shift: Bool,
    ctrl: Bool,
    cmd: Bool,
    option: Bool
  ) -> String? {
    let characters = rawCharacters.trimmingCharacters(in: .whitespacesAndNewlines)
    let chordedFullStop = keyCode == fullStopKeyCode && (cmd || ctrl) ? "." : nil
    let named = chordedFullStop ?? extraNameByKeyCode[keyCode] ?? ShortcutKeyToken.nameByKeyCode[keyCode]
    guard let base = named ?? (characters.isEmpty ? nil : characters.lowercased()) else {
      return nil
    }
    let isLetter = named == nil && base.count == 1 && base.first?.isLetter == true
    let keepsShift = shift && (named != nil || cmd || ctrl || option || isLetter)
    var parts: [String] = []
    if cmd { parts.append("cmd") }
    if ctrl { parts.append("ctrl") }
    if option { parts.append("option") }
    if keepsShift { parts.append("shift") }
    parts.append(base)
    return parts.joined(separator: "+")
  }
}
