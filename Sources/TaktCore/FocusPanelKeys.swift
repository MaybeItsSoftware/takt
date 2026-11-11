import Foundation

/// What a key means in the focus panel's list once the keyboard has left its
/// field — Today's cards, or a list's outline.
///
/// The panel answers to the catalogue's own keys rather than a set of its
/// own: `dd` is the due date there because it is the due date in the window,
/// and a keymap that moves it moves it in both. What it does *not* do is run
/// the window's commands, because those act on the window's selection; each
/// one the panel takes is translated here into an action on the panel's own
/// cursor. A command with no translation is not the panel's, and its key
/// does nothing there — `mm` is not held waiting for, so `m` waits for nothing.
public enum FocusPanelAction: Equatable, Sendable {
  /// The quick-edit field a key opens, as the window's quick edit names them.
  public enum Field: String, Equatable, Sendable {
    case title, due, start, estimate, notes, tags, recurrence
  }

  /// Up and down by this many rows; a single step wraps, as in the window.
  case step(Int)
  case first, last
  /// → and `l`: show the subtasks, then step into them.
  case right
  /// ← and `h`: hide them, step out to the parent, and from the top leave.
  case left
  /// `:` — the next keys are a row's number.
  case numberJump
  case complete
  /// Start the task, finish the running one, or queue behind it.
  case start
  case edit(Field)
  /// 1–4, or 0 for none.
  case priority(Int)
  case conditions
  /// Due today (0) or tomorrow (1).
  case due(days: Int)
  case clearDue
  case clearStart
  /// `go`: hand over to the window, on the task's own list.
  case goToList
  case delete
  /// `/`: back to the field to search.
  case search
  /// Return: back to the field to add.
  case add
  case undo, redo
  case goToday
  /// The panel's list switcher.
  case chooseList
  /// The place before or after this one: Today, then the lists in order.
  case place(Int)
  case dismiss
  /// `⌃T`: plan the task for today, or take it off.
  case planToday
  /// `⌘O`: open the task's first link.
  case openLink
  /// `ga`: hand over to the window's Conditions tab, as ⌘⌥8 does here.
  case setAvailable
  /// `gw`: the place finder, over the panel.
  case setPlace
}

public enum FocusPanelKeys {
  /// The commands whose meaning in the panel does not depend on the key.
  private static let fixed: [WorkspaceCommandID: FocusPanelAction] = [
    .motionSelectNext: .step(1), .motionSelectPrevious: .step(-1),
    .motionOutlineUnfold: .right, .motionTodayUnfold: .right, .planEnterTask: .right,
    .motionOutlineFold: .left, .motionTodayLeave: .left, .planLeaveTask: .left,
    .motionGoToNumber: .numberJump, .taskComplete: .complete,
    .todayStart: .start, .taskStartFocus: .start,
    .taskRename: .edit(.title), .taskRenameAppend: .edit(.title), .taskRenameInsert: .edit(.title),
    .taskReplace: .edit(.title),
    .taskEditDue: .edit(.due), .taskEditStart: .edit(.start), .taskEditEstimate: .edit(.estimate),
    .taskEditNotes: .edit(.notes), .taskEditTags: .edit(.tags), .taskEditRecurrence: .edit(.recurrence),
    .taskClearPriority: .priority(0), .taskSendToBack: .priority(-1), .taskConditions: .conditions,
    .taskDueToday: .due(days: 0), .taskDueTomorrow: .due(days: 1), .taskClearDue: .clearDue,
    .taskClearStart: .clearStart, .taskGoToList: .goToList,
    .taskDelete: .delete, .goSearch: .search, .taskNew: .add,
    .windowUndo: .undo, .windowRedo: .redo,
    .goToday: .goToday, .goListNavigator: .chooseList,
    .goPreviousList: .place(-1), .goNextList: .place(1),
    .motionDismiss: .dismiss,
    .taskTogglePlannedToday: .planToday, .taskOpenLink: .openLink,
    .focusSetAvailable: .setAvailable, .focusSetPlace: .setPlace,
  ]

  /// The panel's meaning for a command, as the key that asked for it.
  public static func action(for command: WorkspaceCommandID, key: String) -> FocusPanelAction? {
    switch command {
    case .motionSelectPage: .step(["pageup", "ctrl+u"].contains(key) ? -8 : 8)
    case .motionSelectEnds: ["home", "cmd+up", "gg"].contains(key) ? .first : .last
    case .motionSetPriority: Int(key).flatMap { (1...4).contains(abs($0)) ? .priority($0) : nil }
    default: fixed[command]
    }
  }

  /// The panel's meaning for a key on `surface`, read through the bindings
  /// in force.
  public static func action(forKey key: String, on surface: WorkspaceCommandSurface) -> FocusPanelAction? {
    WorkspaceCommandCatalog.command(forKey: key, on: surface).flatMap { action(for: $0.id, key: key) }
  }

  /// The two-letter sequences the panel answers on `surface`: only those it
  /// has a meaning for, so a key that starts nothing here is not held.
  public static func sequences(on surface: WorkspaceCommandSurface) -> Set<String> {
    WorkspaceCommandCatalog.sequences(on: surface).filter { action(forKey: $0, on: surface) != nil }
  }

  /// Whether an action is reached from inside the panel's field as well.
  /// Only by a chord, an `⌥` key or a function key, which nothing typed into
  /// a search is: ⌘D for the due date, ⌥T for the estimate, F2 for the title,
  /// ⌘P for the lists. A bare key there is the text's.
  public static func reachesFromField(_ action: FocusPanelAction, key: String) -> Bool {
    let isFunctionKey = key.hasPrefix("f") && key.count > 1 && key.dropFirst().allSatisfy(\.isNumber)
    guard WorkspaceCommandCatalog.isChord(key) || key.hasPrefix("option+") || isFunctionKey else { return false }
    switch action {
    case .edit, .due, .clearDue, .clearStart, .goToList, .goToday, .chooseList, .place, .planToday, .openLink:
      return true
    default: return false
    }
  }

  /// Whether a key is a chord for a command that, run by the main menu, acts
  /// on the window's selection — a task's or a list's. The panel never lets
  /// one of those through to the menu: it is done on the panel's row, or not
  /// at all, but never on a task hidden behind the panel.
  public static func isSelectionChord(_ key: String, on surface: WorkspaceCommandSurface) -> Bool {
    guard WorkspaceCommandCatalog.isChord(key),
      let command = WorkspaceCommandCatalog.command(forKey: key, on: surface)
        ?? WorkspaceCommandCatalog.command(forKey: key, on: .anywhere)
    else { return false }
    return command.group == "Task" || command.group == "Lists"
  }

  /// The row `:` lands on for `number` among `count` rows: vim's `:n`, past
  /// the last row is the last row and `:0` the first.
  public static func numberedIndex(_ number: Int, count: Int) -> Int? {
    guard count > 0 else { return nil }
    return min(max(number, 1), count) - 1
  }
}
