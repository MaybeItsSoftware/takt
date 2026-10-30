import AppKit
import Foundation
import TaktCore
import TaktWorkspace

/// Everything that used to be a sheet on the workspace window and is now drawn
/// by the one overlay host at the top of it.
///
/// A sheet is a second window: it slides, it steals the title bar, it takes
/// the window's key monitor down with it, and two of them cannot replace each
/// other — ⌘K from inside search had to be pressed after closing search. One
/// enum means one of these is up at a time by construction, and asking for a
/// different one simply replaces it.
enum WorkspaceOverlay: Identifiable {
  case commandPalette
  case search
  /// The list finder: every list and nested list, filtered as you type.
  case listNavigator
  case keyboardReference
  case move(WorkspaceItemMoveRequest)
  case quickEdit(WorkspaceTaskQuickEditRequest)
  /// The habit form, on a task, a habit, or nothing.
  case habit(WorkspaceHabitRequest)
  /// Committing to a task periodically: a name, a schedule and an end.
  case commitment(WorkspaceCommitmentRequest)
  /// Waiting on: who or what, and when to follow up.
  case waiting(WorkspaceWaitingRequest)
  /// Blocked by: the finder for a task to wait on.
  case blocker(WorkspaceBlockerRequest)
  /// Link…: the picker for a note, a Drive file or an address to link.
  case link(WorkspaceLinkRequest)
  /// The conditions a task needs, ticked: `cc`.
  case conditions(WorkspaceConditionPickerRequest)
  /// The category a condition is filed under, or the new-condition row's.
  case conditionCategory(WorkspaceConditionCategoryRequest)
  /// One condition chosen: the place you are in (`gw`), or what one merges
  /// into (`m` in the Conditions tab).
  case conditionFinder(WorkspaceConditionFinderRequest)
  /// A category's new name, for every condition under it: `⇧T`.
  case renameConditionCategory(WorkspaceConditionCategoryRenameRequest)
  /// A new list or folder. Where it goes is `creationParentFolderID` and
  /// `creationIsNested` on the model, set by whoever asked.
  case create(WorkspaceCreationKind)
  case newBoardColumn
  /// A board column's new name, by its id.
  case renameBoardColumn(String)
  /// Deleting a board column: where its tasks go, by its id.
  case deleteBoardColumn(String)
  /// Telling the agent what to do to a task: `aa`.
  case quickAsk(WorkspaceQuickAskRequest)
  /// A save moving a repeating task onto its next turn, held for a choice.
  case recurrenceClash(WorkspaceRecurrenceClashRequest)
  /// A rename of a repeating task, held for whether it is this turn's alone.
  case seriesRename(WorkspaceSeriesRenameRequest)
  /// A save starting a task on a later day while it has a deadline, held
  /// for what the deadline does.
  case startLater(WorkspaceStartLaterRequest)

  var id: String {
    switch self {
    case .commandPalette: "palette"
    case .search: "search"
    case .listNavigator: "navigator"
    case .keyboardReference: "keys"
    case .move(let request): "move:\(request.id)"
    case .quickEdit(let request): "edit:\(request.id)"
    case .habit(let request): "habit:\(request.id)"
    case .commitment(let request): "commitment:\(request.id)"
    case .waiting(let request): "waiting:\(request.id)"
    case .blocker(let request): "blocker:\(request.id)"
    case .link(let request): "link:\(request.id)"
    case .conditions(let request): "conditions:\(request.id)"
    case .conditionCategory(let request): "condition-category:\(request.id)"
    case .conditionFinder(let request): "condition-finder:\(request.id)"
    case .renameConditionCategory(let request): "rename-category:\(request.id)"
    case .create(let kind): "create:\(kind.rawValue)"
    case .newBoardColumn: "column"
    case .renameBoardColumn(let id): "column-rename:\(id)"
    case .deleteBoardColumn(let id): "column-delete:\(id)"
    case .quickAsk(let request): "quick-ask:\(request.id)"
    case .recurrenceClash(let request): "recurrence-clash:\(request.id)"
    case .seriesRename(let request): "series-rename:\(request.id)"
    case .startLater(let request): "start-later:\(request.id)"
    }
  }
}

/// The window an overlay is drawn in. See `WorkspaceViewModel.overlaySurface`.
enum WorkspaceOverlaySurface {
  case window
  case panel
}

/// What an overlay does with the keys it owns. Registered by the overlay's own
/// view, because the selection it moves is the view's state, and keyed by the
/// overlay's id so a view on its way out cannot answer for its replacement.
struct WorkspaceOverlayKeyHandler {
  let overlayID: String
  /// Returns whether the key was consumed. A key that is not — a letter, say —
  /// goes on to the overlay's field.
  let handle: (String) -> Bool
}

@MainActor
extension WorkspaceViewModel {
  /// The chords that swap one overlay for another rather than closing it:
  /// ⌘K from search opens the palette.
  static let overlaySwitchingCommands: Set<WorkspaceCommandID> = [
    .goCommandPalette, .goSearch, .goKeyboardReference, .goListNavigator,
  ]

  /// Asking for the overlay already up closes it, the way ⌘K twice does in
  /// every editor that has one.
  func presentOverlay(_ overlay: WorkspaceOverlay) {
    desktopShortcutSequence.reset()
    if activeOverlay?.id == overlay.id {
      dismissOverlay()
      return
    }
    overlaySurface = focusPanelHasKeyboard?() == true ? .panel : .window
    activeOverlay = overlay
  }

  /// The overlay up in the main window, if the one up is drawn there.
  var windowOverlay: WorkspaceOverlay? { overlaySurface == .window ? activeOverlay : nil }

  /// The overlay up in the focus panel, if the one up is drawn there.
  var panelOverlay: WorkspaceOverlay? { overlaySurface == .panel ? activeOverlay : nil }

  /// - Parameter restoringFocus: whether to hand the keyboard back to the
  ///   region that had it. An overlay that goes somewhere on its way out — a
  ///   search result, a palette command — says where itself.
  func dismissOverlay(restoringFocus: Bool = true) {
    guard activeOverlay != nil else { return }
    activeOverlay = nil
    overlayKeyHandler = nil
    overlayPanelFrame = nil
    // The panel puts its own keyboard back; the window's regions are not
    // where it went.
    if restoringFocus && overlaySurface == .window { requestKeyboardFocus(keyboardFocusArea) }
  }

  /// The window's key monitor asks this first whenever an overlay is up.
  ///
  /// Escape always closes. The chords you leave a text field with still work,
  /// and the four that open an overlay replace this one rather than closing it.
  /// Everything else is offered to the overlay, and what it does not consume
  /// goes to its field — never through to the workspace behind it.
  func handleOverlayKey(_ event: NSEvent) -> Bool {
    guard let overlay = activeOverlay, let key = event.workspaceCommandKey else { return false }
    if key == "escape" {
      if !cancelHeldSave(overlay) { dismissOverlay() }
      return true
    }
    if WorkspaceCommandCatalog.isChord(key),
      let command = WorkspaceCommandCatalog.command(forKey: key, on: .anywhere),
      WorkspaceCommandCatalog.reachableFromTextField.contains(command.id) {
      // A prompt holding a save is answered "cancel" before the chord runs,
      // as escape answers it, rather than closed with the edit inside it.
      cancelHeldSave(overlay)
      if !Self.overlaySwitchingCommands.contains(command.id) {
        dismissOverlay(restoringFocus: false)
      }
      run(command.id, key: key)
      return true
    }
    if let handler = overlayKeyHandler, handler.overlayID == overlay.id {
      return handler.handle(key)
    }
    return false
  }

  /// Cancels the save a clash or start-later prompt is holding, which hands
  /// the edit back to where it was made. Returns whether `overlay` was one.
  @discardableResult
  private func cancelHeldSave(_ overlay: WorkspaceOverlay) -> Bool {
    switch overlay {
    case .recurrenceClash(let request): resolveRecurrenceClash(request, choice: .cancel)
    case .startLater(let request): resolveStartLater(request, choice: .cancel)
    default: return false
    }
    return true
  }
}

/// How a list-shaped overlay reads the direction keys. `⌃N` and `⌃P` are
/// there because they are how a finder is driven in every editor that has one.
enum WorkspaceOverlayStep {
  static func offset(for key: String) -> Int? {
    switch key {
    case "down", "ctrl+n": 1
    case "up", "ctrl+p": -1
    case "pagedown": 8
    case "pageup": -8
    default: nil
    }
  }

  /// The index `offset` rows on from `current`, clamped to `count`.
  static func index(from current: Int?, by offset: Int, count: Int) -> Int? {
    guard count > 0 else { return nil }
    guard let current else { return offset > 0 ? 0 : count - 1 }
    return min(max(current + offset, 0), count - 1)
  }
}
