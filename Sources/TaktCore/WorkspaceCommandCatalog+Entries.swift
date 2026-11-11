import Foundation
import TaktRustCore

/// The catalogue: every key the desktop workspace answers to, once.
///
/// The entries are the Rust core's (`core/src/keymap/catalogue.rs`), shared
/// with the iPhone and Android, and read once, here, at launch: the key
/// router never crosses into the core, it reads `WorkspaceKeyBindings`, the
/// lookup table built from them. A new command is a `WorkspaceCommandID` case,
/// its `run` arm, and an entry in that file.
public enum WorkspaceCommandCatalog {

  /// The shipped keys. What is in force — these with any user keymap laid
  /// over them — is `all`; see `WorkspaceKeyBindings`.
  public static let defaults: [WorkspaceCommand] = coreEntries.compactMap(WorkspaceCommand.init(core:))

  /// The core's entries as read, including any this build's `WorkspaceCommandID`
  /// lacks, which `defaults` drops; a test holds the two to one another.
  static let coreEntries = workspaceCommandCatalog()

  public static var byID: [WorkspaceCommandID: WorkspaceCommand] { bindings.byID }

  public static subscript(id: WorkspaceCommandID) -> WorkspaceCommand {
    // Force-unwrapped deliberately: `everyCommandHasAnEntry` fails the build
    // before this can, and a catalogue with a hole in it is not a thing the
    // app should try to carry on with. A keymap changes keys, never rows.
    bindings.byID[id]!
  }

  /// The menu bar's Task menu, in sections. Every action in the Task group is
  /// in it except "Add a task", which the Workspace menu carries, and "Clear
  /// the priority" and "Send to the back", which sit under the menu's
  /// priority digits; a test holds
  /// the menu to that, so a new task command cannot be left out of it.
  public static let taskMenu: [[WorkspaceCommandID]] = [
    [.taskComplete, .taskInvalidate, .taskStartFocus, .taskDelete],
    [.taskNewAbove, .taskNewChild],
    [
      .taskRename, .taskRenameAppend, .taskRenameInsert, .taskReplace, .taskEditDue, .taskEditStart, .taskEditEstimate,
      .taskEditNotes, .taskMaximiseNotes, .taskEditTags, .taskEditRecurrence,
    ],
    [.taskDueToday, .taskDueTomorrow, .taskBury, .taskClearDue, .taskClearStart, .taskClearNotes, .taskClearTags],
    [.taskIndent, .taskOutdent, .taskMoveUp, .taskMoveDown],
    [.taskMoveToPreviousList, .taskMoveToNextList, .taskMove],
    [.taskConvertToList, .taskPromoteList, .taskExtractBranch],
    [
      .taskTogglePlannedToday, .taskToggleDaily, .taskHabit, .taskWaiting, .taskAddBlocker, .taskConditions,
      .taskAskAgent,
      .taskOpenLink, .taskAddLink, .taskShowProgress, .taskToggleInspector, .taskGoToList,
    ],
  ]

  // MARK: - Menus

  /// The menu bar's Task menu carries these as a section of their own, after
  /// starting a block, and greys them while nothing runs; the palette leaves
  /// them out altogether until a block is running.
  public static let focusMenu: [WorkspaceCommandID] = [.focusFinish, .focusStop]
}
