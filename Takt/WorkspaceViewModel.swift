import AppKit
import Foundation
import Observation
import TaktCore
import TaktSync
import TaktWorkspace
import SwiftUI

enum WorkspaceCreationKind: String, Identifiable {
  case list
  case folder

  var id: String { rawValue }
  var title: String { self == .list ? "New list" : "New folder" }
}

struct WorkspaceItemMoveRequest: Identifiable {
  let payload: String
  let title: String
  let sourceListID: String
  let taskID: String?
  var id: String { payload }
}

/// One place the global capture field can file a task. A nested list remains
/// in its owning physical list and is represented by its task as the parent.
struct QuickCaptureDestination: Identifiable, Equatable {
  let id: String
  let listID: String
  let parentTaskID: String?
  let title: String
  let path: String
  let depth: Int
}

/// The three persistent regions of the desktop window. Keeping this state in
/// the model lets the AppKit key monitor and the SwiftUI focus system agree on
/// where a shortcut belongs.
enum WorkspaceFocusArea: Hashable {
  case sidebar
  case tasks
  case inspector
  /// The bottom dock's rail of finished work. A region rather than a scroll view
  /// with a click target, because the app's premise is that anything you can
  /// reach you can reach from the keyboard.
  case done
  /// The timeline, as a tab of the bottom dock.
  case timeline
  /// The right dock's Conditions tab: what is available now, and the
  /// conditions themselves, walked by one cursor.
  case conditions
}

/// Views of the same local task model, rather than separate applications.
enum WorkspaceViewMode: String, CaseIterable, Identifiable {
  case today
  case board
  case outline
  case matrix

  /// The places you can be. Today is first because it is the question the app
  /// exists to answer; the rest are projections of the same tasks, read when
  /// the answer looks wrong. Focus is an action on the day, not a place to
  /// navigate to, so it is not among them.
  ///
  /// There is deliberately no Dailies here. A daily is a requirement placed on
  /// an ordinary task — that it be contributed to each day — not a kind of item
  /// with a room of its own. Such a task appears in Today like anything else
  /// today has a claim on, badged for what it owes.
  ///
  /// Today is not among them either: it is a list — every task the day has a
  /// claim on — chosen in the sidebar, and drawn its own way.
  static let planningModes: [WorkspaceViewMode] = [.board, .outline, .matrix]

  var id: String { rawValue }
  var title: String {
    switch self {
    case .today: "Today"
    case .board: "Board"
    case .outline: "Outline"
    case .matrix: "Matrix"
    }
  }

  var symbolName: String {
    switch self {
    case .today: "sun.max"
    case .board: "rectangle.split.3x1"
    case .outline: "list.bullet.indent"
    case .matrix: "square.grid.2x2"
    }
  }

  /// Where the mode sits on the command-digit row, and what the View menu
  /// prints beside it.
  /// The catalogue entry that goes here, rather than the key itself. The digit
  /// used to be written out here as well as in the router, the toolbar tooltip
  /// and the menu — four copies of ⌘1.
  var command: WorkspaceCommandID? {
    switch self {
    case .today: .goToday
    case .board: .goBoard
    case .outline: .goOutline
    case .matrix: .goMatrix
    }
  }
}

enum WorkspaceSidebarEditor: Identifiable {
  case list(TaskList)
  case folder(ListFolder)

  var id: String {
    switch self {
    case .list(let list): "list-\(list.id)"
    case .folder(let folder): "folder-\(folder.id)"
    }
  }
}

/// A new list or folder, drawn as a name field where it will appear. Nothing
/// is written until the name is: Esc, or leaving the field empty, leaves no
/// "Untitled" behind to delete and nothing on the undo stack.
struct WorkspaceSidebarDraft: Equatable {
  let kind: WorkspaceCreationKind
  /// The folder it goes into; nil for the top level.
  let folderID: String?
}

enum WorkspaceSidebarItem: Identifiable {
  case list(TaskList)
  case folder(ListFolder)

  var id: String {
    switch self {
    case .list(let list): "list-\(list.id)"
    case .folder(let folder): "folder-\(folder.id)"
    }
  }

  var deletionTitle: String {
    switch self {
    case .list(let list): "Delete \(list.name)?"
    case .folder(let folder): "Delete \(folder.name)?"
    }
  }

  var deletionMessage: String {
    switch self {
    case .list: "This permanently deletes the list and all of its tasks."
    case .folder: "Lists remain, but move to the sidebar root. Nested folders are deleted."
    }
  }
}

/// Presentation state for the desktop-only local workspace. The menu bar and
/// its Checkvist compatibility panel deliberately do not read this state.
@MainActor
@Observable final class WorkspaceViewModel {
  static let everythingScopeKey = "localWorkspaceEverythingScopeV1"
  /// Names the service a task was imported from, and so which identifiers its
  /// `sourceId` values belong to. Stored on the task, hence not free to change.
  static let checkvistSourceSystem = "checkvist"
  static let legacyOfflineSourceSystem = "priority-offline"

  /// Internal rather than private so `WorkspaceViewModel+Dailies.swift` —
  /// the same type, split only for size — can reach it.
  @ObservationIgnored var store: WorkspaceStore?
  @ObservationIgnored let legacyStore: LegacyOfflineTaskStore
  /// Title, task id, list name, due date, all-day → the created event's id,
  /// which is what lets Priority notice later that it was cleared.
  @ObservationIgnored var googleCalendarEventCreator:
    ((String, String, String, Date?, Bool) async throws -> String?)?
  /// Called with a task id and the calendar event now standing for it.
  @ObservationIgnored var onGoogleCalendarEventCreated: ((String, String) -> Void)?
  /// Opens the Diagnostics sheet, which the coordinator owns rather than the
  /// workspace.
  @ObservationIgnored var onShowDiagnostics: (() -> Void)?
  /// Puts a passing line in the status bar, which reads the coordinator's
  /// `statusMessage` rather than anything on the workspace. For feedback a
  /// key owes when what it did is not visible where you are looking.
  @ObservationIgnored var onStatusMessage: ((String) -> Void)?
  /// Where the link picker searches: Obsidian vaults and Google Drive. The
  /// Links plugin, handed over by the app delegate (docs/links.md).
  @ObservationIgnored var linkSources: (any LinksIntegrationPlugin)?
  /// Called after any local write, so the Google Tasks mirror can push it.
  /// Coalesced on the far side — this fires far more often than it syncs.
  @ObservationIgnored var onLocalWrite: (() -> Void)?
  /// The watch for writes made by another process. See
  /// `WorkspaceViewModel+ExternalWrites.swift`.
  /// Nil until the store opens. Observed, so the status bar follows it.
  var syncSession: SyncSession?
  /// Tells `syncSession` whether Takt is in front. See `startSync()`.
  @ObservationIgnored var syncActivationObservers: [NSObjectProtocol] = []
  @ObservationIgnored var externalWriteToken: Int?
  @ObservationIgnored var externalWriteTimer: Timer?
  @ObservationIgnored var externalWriteCheckInFlight = false
  /// The focus clock's housekeeping, and the sleep and activation observers
  /// beside it. Owned here, started once from `init`, because it has to outlive
  /// the window: starting a block closes the window, and a checkpoint that
  /// stopped with it was a block that lost its time at the next quit. See
  /// `startFocusMonitor()`.
  @ObservationIgnored var focusMonitorTask: Task<Void, Never>?
  @ObservationIgnored var focusMonitorObservers: [NSObjectProtocol] = []
  /// Whether an ordinary window is up, so a prompt the clock raises goes to
  /// the panel when the window is away. Set by the app shell.
  @ObservationIgnored var hasOrdinaryWindow: (() -> Bool)?
  let taskEditor = WorkspaceTaskEditor()

  private(set) var workspace: Workspace?
  // `folders`, `lists`, `expandedFolderIDs` and `nestedLists` are what the
  // sidebar is drawn from, so each one drops the sidebar's memoised rows.
  private(set) var folders: [ListFolder] = [] { didSet { sidebarRowsMemo = nil } }
  private(set) var lists: [TaskList] = [] { didSet { sidebarRowsMemo = nil } }
  private(set) var archivedLists: [TaskList] = []
  /// The outline and board state below is written only by
  /// `WorkspaceViewModel+Loading.swift`; internal rather than `private(set)`
  /// so that file can.
  var outline: [TaskOutlineItem] = [] {
    didSet {
      outlineByList = Dictionary(grouping: outline) { $0.task.listId }
      outlineOpenCount = outline.reduce(0) { $0 + ($1.task.status == .open ? 1 : 0) }
      let parents = TaskOutlineFolding.parentIDs(outline)
      if outlineParentIDs != parents { outlineParentIDs = parents }
      refoldOutline()
    }
  }
  /// The outline as drawn: `outline` with every folded task's branch taken
  /// out. What the pane lists and what the arrow keys walk.
  private(set) var outlineRows: [TaskOutlineItem] = []
  /// The outline's rows that have subtasks, folded or not.
  private(set) var outlineParentIDs: Set<String> = []
  /// The outline grouped by list and its open count, derived once per reload
  /// rather than on every render of the combined outline.
  private(set) var outlineByList: [String: [TaskOutlineItem]] = [:]
  private(set) var outlineOpenCount = 0
  var boardTasks: [WorkspaceTask] = []
  /// Bumped whenever `taskCache` is rebuilt, so a view that resolved a task
  /// by id re-resolves it. Written by `rebuildTaskCache()`.
  var taskContentRevision = 0
  @ObservationIgnored var taskCache: [String: WorkspaceTask] = [:]
  @ObservationIgnored var missingTaskIDs: Set<String> = []
  @ObservationIgnored var descendantCache: [String: [TaskOutlineItem]] = [:]
  /// Each task's open subtasks, as Today draws them beneath it. Cleared with
  /// `descendantCache`.
  @ObservationIgnored var dayChildrenCache: [String: [WorkspaceTask]] = [:]
  /// The board's cards and everything beneath them, as the last refresh read
  /// them. Held so the task cache can be rebuilt without reading them again.
  @ObservationIgnored var boardTreeTasks: [WorkspaceTask] = []
  /// Board column layouts by scope key, as `kanbanBoardConfigurations`
  /// returned them when it imported the old preferences' layouts. Only
  /// whether it has run this load is read now; the columns come from
  /// `WorkspaceStore.boardColumns`.
  @ObservationIgnored var boardColumnConfigurations: [String: Data]?

  // The refresh machinery. See `WorkspaceViewModel+Refresh.swift`.
  @ObservationIgnored var pendingRefresh: WorkspaceRefresh = []
  @ObservationIgnored var refreshDepth = 0
  @ObservationIgnored var performFailed = false
  @ObservationIgnored var performShouldMirror = false
  /// `errorMessage` as the outermost `perform` found it, so a successful write
  /// clears only a message that was already there — never one its own work
  /// has just put up.
  @ObservationIgnored var performPriorErrorMessage: String?
  /// Moves on every refresh, so a ranking read before it knows it is stale.
  @ObservationIgnored var writeEpoch = 0
  @ObservationIgnored var nextUpRequested = false
  @ObservationIgnored var nextUpLaunchQueued = false
  @ObservationIgnored var nextUpGeneration = 0
  /// The scopes shown lately, resident between switches. See
  /// `WorkspaceViewModel+ScopeCache.swift`.
  @ObservationIgnored let scopeCache = WorkspaceScopeCache(capacity: 8, prefetchCapacity: 6)
  /// Set for the refresh a switch makes, which wrote nothing, so a kept
  /// combined scope may be drawn while it is read again.
  @ObservationIgnored var scopeReadMayBeStale = false
  /// Scopes being read off the main thread, so one is not read twice at once.
  @ObservationIgnored var revalidatingScopes: Set<WorkspaceScope> = []
  @ObservationIgnored var scopePrefetchQueued = false
  /// The board shape on screen, as `WorkspaceScopeCache` numbers it; nil
  /// for a board that was emptied or not read through the cache.
  @ObservationIgnored var boardShapeGeneration: Int?
  /// The board indexes built for the shapes shown lately, and the one
  /// applied now. See `rebuildBoardIndex()`.
  @ObservationIgnored var boardIndexMemos: [BoardIndexMemo] = []
  @ObservationIgnored var appliedBoardIndexKey: BoardIndexMemo.Key?
  /// What the done rail's rows were read at; see `reloadCompleted()`.
  @ObservationIgnored var completedTasksKey: (stamp: WorkspaceChangeStamp, day: Date)?
  /// Whether `waitingDetails` may be behind the database. It is read from
  /// every task's metadata, so it is read again after a write rather than on
  /// every refresh — moving between lists changes none of it.
  @ObservationIgnored var waitingDetailsAreStale = true
  /// The rows behind the day, as the last ranking read them.
  @ObservationIgnored var dayTaskSnapshot: [String: WorkspaceTask] = [:]
  /// The history menu's labels. Read after writes by `refreshHistoryLabels()`
  /// rather than queried each time the menu is drawn.
  var undoLabel: String?
  var redoLabel: String?
  /// Moves every time the history labels are refreshed: after each write,
  /// undo and reload. See `refreshHistoryLabels()`.
  var historyVersion = 0
  var listTaskCounts: [String: Int] = [:]
  var boardCrossColumnTasks: [WorkspaceTask] = []
  var boardDescendants: [String: [TaskOutlineItem]] = [:]
  /// Each board task's parent, by id; see `boardParent(of:)`.
  var boardTaskParents: [String: String] = [:]
  /// The actual parent whose children are visible on the board. A Checkvist
  /// import can contain one root project whose title is identical to its list;
  /// that project is a transport wrapper, not useful work to show as the
  /// board's only card.
  var boardParentTaskID: String?
  var boardColumns: [WorkspaceKanbanColumn] = WorkspaceKanbanColumn.blitzitDefaults
  var boardTaskColumns: [String: String] = [:]
  /// The board's columns as the core resolved them: the layout an edit
  /// saves, where it came from, and what an entered card hides.
  var boardColumnRead: WorkspaceBoardColumns?
  /// Tasks a rule column or an entered card's floor draws somewhere other
  /// than their filed column; see `column(for:)`.
  var boardColumnPlacements: [String: String] = [:]
  /// Where the next column added goes: beside a column, rather than last.
  @ObservationIgnored var pendingColumnInsertion: (anchorID: String, after: Bool)?
  /// A board column's rules sheet, when it is open.
  var boardColumnRulesEditor: WorkspaceBoardColumnRulesRequest?
  var boardTasksByColumn: [String: [WorkspaceTask]] = [:]
  var boardColumnsByID: [String: WorkspaceKanbanColumn] = [:]
  var boardVisibleTaskIDs: Set<String> = []
  var matrixPositions: [String: TaskMatrixPosition] = [:]
  /// The board's tasks by matrix quadrant, sorted once per board read by
  /// `rebuildBoardIndex()` so the matrix does not filter the board per render.
  var matrixQuadrants = MatrixQuadrantIndex<WorkspaceTask>()
  var viewMode: WorkspaceViewMode = .today {
    didSet { if viewMode != oldValue { viewModeDidChange() } }
  }
  /// Moves when you change the view mode yourself, which is the one change
  /// the main pane fades on. A switch of scope that changes the mode on the
  /// way — the inbox opening as an outline, Everything or a list as a board,
  /// Today from the sidebar — cuts instead, as Zed's tabs do: the new rows
  /// are drawn in the frame that asked for them rather than faded in over it.
  var viewModeFadeToken = 0
  /// What the task cache was last rebuilt at, so a rebuild that read nothing
  /// newer — a switch, which writes nothing — does not bump
  /// `taskContentRevision` and redraw every view that resolved a task by id.
  @ObservationIgnored var taskContentStamp: (store: ObjectIdentifier, stamp: WorkspaceChangeStamp)?
  /// The boards' columns read since the last write, by board, and what each
  /// was read for. See `readBoardColumns(store:)`.
  @ObservationIgnored var boardColumnsMemos: [String: (key: BoardColumnsMemoKey, read: WorkspaceBoardColumns)] = [:]
  /// Whether the board's state was emptied rather than read, because the
  /// combined scope it belongs to is showing something else. See
  /// `reloadBoardNow`.
  @ObservationIgnored var boardIsStale = true
  /// Columns of tasks off the board, read one at a time for the inspector and
  /// kept until the next refresh. See `kanbanColumnID(ofTaskID:)`.
  @ObservationIgnored var offBoardColumnCache: (revision: Int, columns: [String: String?]) = (-1, [:])
  /// Virtual parent of every active list. The tasks remain stored in their
  /// own lists; this flag only changes which roots the views present.
  var isEverythingSelected = UserDefaults.standard.bool(forKey: WorkspaceViewModel.everythingScopeKey)
  var newTaskListID: String?
  var listIcons = UserDefaults.standard.dictionary(forKey: "workspaceListIconsV1") as? [String: String] ?? [:]

  /// Whether the sidebar takes a column at all. Persisted, because a sidebar
  /// you collapsed is a decision about how you work rather than about this
  /// session — reopening the window to find it back is the annoying part.
  var isSidebarVisible = UserDefaults.standard.object(forKey: WorkspaceViewModel.sidebarVisibleKey) as? Bool ?? true {
    didSet { UserDefaults.standard.set(isSidebarVisible, forKey: Self.sidebarVisibleKey) }
  }

  /// The width the sidebar was last dragged to.
  ///
  /// `HSplitView` sizes a pane from its `idealWidth` and then forgets whatever
  /// you dragged it to, so the column reset to 185 on every launch. Recording
  /// the laid-out width here and feeding it back as the ideal is what makes
  /// the drag stick.
  var sidebarWidth: CGFloat = {
    let stored = UserDefaults.standard.double(forKey: WorkspaceViewModel.sidebarWidthKey)
    guard stored > 0 else { return WorkspaceViewModel.defaultSidebarWidth }
    return min(max(CGFloat(stored), WorkspaceViewModel.minSidebarWidth), WorkspaceViewModel.maxSidebarWidth)
  }() {
    didSet {
      guard sidebarWidth != oldValue else { return }
      UserDefaults.standard.set(Double(sidebarWidth), forKey: Self.sidebarWidthKey)
    }
  }

  /// Whether the right dock — the inspector and the done rail, as tabs — takes
  /// a column. Persisted for the sidebar's reason: whether you want it on
  /// screen is a way of working, not a decision to make again every launch.
  /// Until the dock existed, this was the done rail's own flag, and its value
  /// carries over.
  var isRightDockVisible = UserDefaults.standard.object(forKey: WorkspaceViewModel.rightDockVisibleKey) as? Bool
    ?? UserDefaults.standard.bool(forKey: WorkspaceViewModel.doneRailVisibleKey) {
    didSet { UserDefaults.standard.set(isRightDockVisible, forKey: Self.rightDockVisibleKey) }
  }

  /// Which of the dock's tabs is showing. Persisted with the dock. A dock
  /// last left on the done rail or the timeline, which have since moved to
  /// the bottom dock, opens on the inspector.
  var rightDockTab = WorkspaceDockTab(
    rawValue: UserDefaults.standard.string(forKey: WorkspaceViewModel.rightDockTabKey) ?? "") ?? .inspector {
    didSet { UserDefaults.standard.set(rightDockTab.rawValue, forKey: Self.rightDockTabKey) }
  }

  /// An explicit width with a handle of its own, for the sidebar's reason: a
  /// pane that takes a share of whatever the window has spare is a pane whose
  /// width you cannot set. Seeded from the done rail's old width.
  var rightDockWidth: CGFloat = {
    let defaults = UserDefaults.standard
    let stored = defaults.double(forKey: WorkspaceViewModel.rightDockWidthKey)
    let legacy = defaults.double(forKey: WorkspaceViewModel.doneRailWidthKey)
    let width = stored > 0 ? stored : legacy
    guard width > 0 else { return WorkspaceViewModel.defaultRightDockWidth }
    return min(max(CGFloat(width), WorkspaceViewModel.minRightDockWidth), WorkspaceViewModel.maxRightDockWidth)
  }() {
    didSet {
      guard rightDockWidth != oldValue else { return }
      UserDefaults.standard.set(Double(rightDockWidth), forKey: Self.rightDockWidthKey)
    }
  }

  static let minRightDockWidth: CGFloat = 210
  static let maxRightDockWidth: CGFloat = 480
  static let defaultRightDockWidth: CGFloat = 280
  private static let rightDockVisibleKey = "localWorkspaceRightDockVisibleV1"
  private static let rightDockTabKey = "localWorkspaceRightDockTabV1"
  private static let rightDockWidthKey = "localWorkspaceRightDockWidthV1"
  private static let doneRailWidthKey = "localWorkspaceDoneRailWidthV1"

  static let minSidebarWidth: CGFloat = 140
  static let maxSidebarWidth: CGFloat = 420
  static let defaultSidebarWidth: CGFloat = 185
  private static let sidebarVisibleKey = "localWorkspaceSidebarVisibleV1"
  private static let sidebarWidthKey = "localWorkspaceSidebarWidthV1"
  private static let leftDockTabKey = "localWorkspaceLeftDockTabV1"

  /// Which of the left dock's tabs is showing: the lists, or the agent.
  /// Persisted with the dock, for the right dock's reason. Whether the dock
  /// is open at all is still `isSidebarVisible`, a name kept from when the
  /// sidebar was all it held, so that setting carries over.
  var leftDockTab = WorkspaceLeftDockTab(
    rawValue: UserDefaults.standard.string(forKey: WorkspaceViewModel.leftDockTabKey) ?? "") ?? .lists {
    didSet { UserDefaults.standard.set(leftDockTab.rawValue, forKey: Self.leftDockTabKey) }
  }

  /// The agent panel's thread. See `WorkspaceAgentSession`.
  let agent = WorkspaceAgentSession()
  /// The quick ask's thread (`aa`), apart from the panel's. See
  /// `WorkspaceViewModel+QuickAsk.swift`.
  let quickAgent = WorkspaceAgentSession()
  /// The task the last quick ask was about.
  var quickAskTask: WorkspaceTask?
  /// The agent's standing instructions for the account, folders and lists.
  let agentInstructions = WorkspaceAgentInstructions()
  /// Asks the agent panel's message field for the keyboard.
  var agentInputFocusRequest = 0
  /// The agent panel's field or one of its approval cards has the keyboard.
  /// The window's key monitor reads it to leave keys to the panel — Return on
  /// a card approves it, and must not open a task behind it instead.
  @ObservationIgnored var agentHoldsKeyboard = false

  var dailyProgressRevision = 0
  var selectedListID: String?
  var selectedFolderID: String?
  /// The filter list in scope, drawn over Everything: while it is set,
  /// `isEverythingSelected` is too, and the pane shows only the tasks the
  /// filter list's rules match. See `WorkspaceViewModel+FilterLists.swift`.
  var selectedFilterListID: String?
  /// The workspace's own filter lists, in sidebar order.
  var filterLists: [WorkspaceFilterList] = [] { didSet { sidebarRowsMemo = nil } }
  /// The filter list sheet, when it is open.
  var filterListEditor: WorkspaceFilterListEditorRequest?
  /// Where the sidebar's keyboard cursor is, as a row id from
  /// `WorkspaceSidebarOutline`.
  ///
  /// Separate from `selectedListID` because two of the rows — Focus and the
  /// timeline — are not lists and landing on one must not change what the
  /// main pane is showing. It also fixes the pinned-list duplicate: the same
  /// list is drawn twice, and only a row id can say which of the two you are
  /// standing on.
  var sidebarCursorID: String?
  var expandedFolderIDs: Set<String> = [] { didSet { sidebarRowsMemo = nil } }
  var scopeTaskID: String?
  var nestedLists: [TaskOutlineItem] = [] { didSet { sidebarRowsMemo = nil } }
  /// The sidebar's rows and the cursor's row, worked out once per change of
  /// what they are drawn from rather than once per row per render. See
  /// `WorkspaceViewModel+SidebarCursor.swift`.
  @ObservationIgnored var sidebarRowsMemo: [WorkspaceSidebarRow]? {
    didSet { sidebarCursorMemo = nil }
  }
  @ObservationIgnored var sidebarCursorMemo: SidebarCursorMemo?
  var archivedNestedLists: [WorkspaceTask] = []
  var creationIsNested = false
  var creationTaskParentID: String?
  var creationTaskListID: String?
  var selectedTaskID: String? {
    didSet {
      let has = selectedTaskID != nil
      if hasSelectedTask != has { hasSelectedTask = has }
    }
  }
  /// Whether anything is selected, which changes far less often than what is.
  /// The window shell reads this, so moving the selection does not redraw it.
  private(set) var hasSelectedTask = false
  /// Stands for where every list, folder and nested list sits, so the sidebar
  /// can animate a move without building arrays to compare on each render.
  /// Set by `reloadNestedListsNow()`.
  var sidebarLayoutKey = 0
  var focusedBoardColumnID: String?
  /// The bottom dock, which holds the progress graph, the done rail and the
  /// timeline. Persisted like the other docks, so it is only ever put away on
  /// purpose.
  var isBottomDockVisible = UserDefaults.standard.bool(forKey: WorkspaceViewModel.bottomDockVisibleKey) {
    didSet { UserDefaults.standard.set(isBottomDockVisible, forKey: Self.bottomDockVisibleKey) }
  }
  /// Which of the bottom dock's tabs is showing. Persisted with the dock. A
  /// right dock last left on the done rail or the timeline hands that tab
  /// down here, so the next ⌘J opens on what was being read.
  var bottomDockTab: WorkspaceBottomDockTab = {
    let defaults = UserDefaults.standard
    return WorkspaceBottomDockTab(rawValue: defaults.string(forKey: WorkspaceViewModel.bottomDockTabKey) ?? "")
      ?? WorkspaceBottomDockTab(rawValue: defaults.string(forKey: WorkspaceViewModel.rightDockTabKey) ?? "")
      .flatMap { $0 == .progress ? nil : $0 }
      ?? .progress
  }() {
    didSet { UserDefaults.standard.set(bottomDockTab.rawValue, forKey: Self.bottomDockTabKey) }
  }
  var bottomDockHeight: CGFloat = {
    let stored = UserDefaults.standard.double(forKey: WorkspaceViewModel.bottomDockHeightKey)
    return stored > 0
      ? min(max(CGFloat(stored), WorkspaceViewModel.minBottomDockHeight), WorkspaceViewModel.maxBottomDockHeight)
      : 180
  }() {
    didSet {
      guard bottomDockHeight != oldValue else { return }
      UserDefaults.standard.set(Double(bottomDockHeight), forKey: Self.bottomDockHeightKey)
    }
  }
  var progressPeriod = TaskProgressPeriod(
    rawValue: UserDefaults.standard.string(forKey: WorkspaceViewModel.progressPeriodKey) ?? "") ?? .month {
    didSet { UserDefaults.standard.set(progressPeriod.rawValue, forKey: Self.progressPeriodKey) }
  }
  static let minBottomDockHeight: CGFloat = 120
  static let maxBottomDockHeight: CGFloat = 420
  private static let bottomDockVisibleKey = "localWorkspaceBottomDockVisibleV1"
  private static let bottomDockHeightKey = "localWorkspaceBottomDockHeightV1"
  private static let bottomDockTabKey = "localWorkspaceBottomDockTabV1"
  private static let progressPeriodKey = "localWorkspaceProgressPeriodV1"
  /// Tasks whose subtasks are folded away — in the outline, on a board card,
  /// and on the subtask rows drawn on one; one set, so a branch put away in
  /// one view stays put away in the other. On the model rather than the row,
  /// because the arrow keys walk the rows that are showing and have to know
  /// which ones are not. Persisted, as Checkvist keeps a fold.
  var foldedTaskIDs = Set(UserDefaults.standard.stringArray(forKey: WorkspaceViewModel.foldedTasksKey) ?? []) {
    didSet {
      guard foldedTaskIDs != oldValue else { return }
      UserDefaults.standard.set(Array(foldedTaskIDs).sorted(), forKey: Self.foldedTasksKey)
      refoldOutline()
      rebuildBoardRowIndex()
    }
  }
  private static let foldedTasksKey = "localWorkspaceFoldedTasksV1"
  /// Each board column's rows and the card each is drawn on. Rebuilt by
  /// `rebuildBoardRowIndex()` when the board is read or a fold changes.
  var boardRowsByColumn: [String: BoardColumnRows] = [:]
  /// Which cards are subtasks filed in another column than the card they
  /// hang from. Rebuilt with the row index.
  var boardLinks = BoardLinks.empty
  /// The end of a link under the pointer — a subtask's row or card, or a
  /// card subtasks further on hang from — whose links are lit and drawn as
  /// the selection's are.
  var hoveredBoardLinkID: String?

  private func refoldOutline() {
    let rows = TaskOutlineFolding.visible(outline, folded: foldedTaskIDs)
    guard outlineRows != rows else { return }
    let draft = viewMode == .outline ? taskDraftSurroundings() : nil
    outlineRows = rows
    if let draft { reanchorTaskDraft(from: draft) }
  }

  /// The column the arrow keys are in. A subtask row counts as being in the
  /// column of the card it is drawn on, not the column its own card would
  /// sit in; and the column the keys were last walking wins, since a subtask
  /// can be both a row on its parent's card and a card elsewhere.
  var activeBoardColumnID: String? {
    let focused = boardColumns.first(where: { $0.id == focusedBoardColumnID })
    if let selectedTaskID {
      if let focused, boardRows(in: focused).contains(selectedTaskID) { return focused.id }
      if let columnID = boardColumnID(forTaskID: selectedTaskID) { return columnID }
      if let owner = boardCardID(owning: selectedTaskID), let columnID = boardColumnID(forTaskID: owner) {
        return columnID
      }
    }
    return focused?.id ?? boardColumns.first?.id
  }
  var dragDestinationListID: String?
  /// Asks the app shell for the always-on-top companion. A counter rather than
  /// a flag: the button and the F key both just want it shown, again.
  ///
  /// The shell, not the window: the companion's whole job is to be the thing
  /// still on screen once the window is not, so it cannot be owned by a view
  /// that closes with it.
  var focusFloatRequest = 0 {
    didSet { onFocusFloatRequested?() }
  }
  /// Called when the floating companion is asked for.
  @ObservationIgnored var onFocusFloatRequested: (() -> Void)?
  /// Bumped when a block is started deliberately, to hand the session over to
  /// the tray and put the window away.
  ///
  /// Focus is for working somewhere else. Leaving the window up in front of
  /// the thing you just committed to doing means the first act of every block
  /// is getting rid of it — so the app does that itself, and the tray becomes
  /// the surface the session runs on.
  var focusHandoffRequest = 0 {
    didSet { onFocusHandoffRequested?() }
  }
  /// Called when a deliberate start should close the window and raise the tray.
  @ObservationIgnored var onFocusHandoffRequested: (() -> Void)?
  /// Whether finishing a block should stop and ask how it went. Read at the
  /// moment of asking rather than stored, so changing the preference takes
  /// effect on the next block rather than the next launch.
  @ObservationIgnored var asksHowEachBlockWent: () -> Bool = { true }
  /// Whether focus session notifications are enabled. Read at the moment of
  /// dispatching so preference changes take effect immediately.
  @ObservationIgnored var focusNotificationsEnabled: () -> Bool = { true }
  /// Whether focus notifications should play a sound.
  @ObservationIgnored var focusNotificationSoundEnabled: () -> Bool = { true }
  /// Custom notification dispatcher for tests or custom handling. When nil,
  /// `FocusNotificationManager.shared` handles dispatch.
  @ObservationIgnored var focusNotificationDispatcher: ((FocusNotification) -> Void)?
  /// The list the quick-add hotkey captures into, by id. Empty, or a list
  /// that no longer exists, is the inbox. Read at the moment of capture, like
  /// the preference above.
  @ObservationIgnored var preferredQuickCaptureListID: () -> String = { "" }
  /// Called when something is finished, wherever it was finished from. The
  /// celebration lives in the app shell, so the model reports the occasion
  /// rather than staging it.
  @ObservationIgnored var onCompletion: ((CompletionEvent) -> Void)?
  /// The palette, search, the list finder and the rest of what used to be
  /// sheets. One at a time, by construction — see `WorkspaceOverlay.swift`.
  var activeOverlay: WorkspaceOverlay?
  /// Which window draws `activeOverlay`: the main window, or the focus panel
  /// when the panel had the keyboard as it was asked for. Decided at each
  /// presentation, so a quick edit asked for from the panel — and the
  /// repeat-clash question its save may raise — is drawn there rather than in
  /// a window behind it.
  var overlaySurface: WorkspaceOverlaySurface = .window
  /// Whether the focus panel has the keyboard. Set by the app shell.
  @ObservationIgnored var focusPanelHasKeyboard: (() -> Bool)?
  @ObservationIgnored var overlayKeyHandler: WorkspaceOverlayKeyHandler?
  /// Where the overlay's card is, in the window content's top-left space.
  /// The window's mouse monitor reads it to tell a click on the card from a
  /// click beside it; see `MainWindowController`.
  @ObservationIgnored var overlayPanelFrame: CGRect?
  /// The main window's title strip, in the same space, so a click in it
  /// while an overlay is up still reaches the mode strip or moves the window.
  @ObservationIgnored var titleStripFrame: CGRect?
  /// The active right-click context menu drawn in house style.
  var activeContextMenu: HouseContextMenuState?
  @ObservationIgnored var contextMenuFrame: CGRect?

  func presentContextMenu(at point: CGPoint, items: [HouseMenuItem]) {
    activeContextMenu = HouseContextMenuState(point: point, items: items)
  }

  func dismissContextMenu() {
    activeContextMenu = nil
    contextMenuFrame = nil
  }
  var searchQuery = "" { didSet { refreshSearchResults() } }
  var searchIncludesCompleted = false { didSet { refreshSearchResults() } }
  /// Written only by `refreshSearchResults()`; internal rather than
  /// `private(set)` so that method can live in `WorkspaceViewModel+Search.swift`.
  var searchResults: [TaskSearchResult] = []
  var selectedSearchResultID: String?
  var creationParentFolderID: String?
  var sidebarEditor: WorkspaceSidebarEditor?
  /// The list or folder currently showing a rename field, by its own id.
  /// Lists and folders both carry UUIDs, so one field serves both.
  var renamingSidebarItemID: String?
  /// A list or folder being named in its row before it exists — `%` and `d`
  /// in the sidebar, as Zed's project panel does a new file.
  var sidebarDraft: WorkspaceSidebarDraft?
  /// The sidebar row whose icon picker is open, by its outline id — a pinned
  /// nested list is drawn twice, and the picker opens on the copy you chose.
  var iconPickerRowID: String?
  /// Lists visited, most recent first, for ⌘P's ordering. Remembered across
  /// launches, since "the list I was in yesterday" is the common case.
  var recentListIDs: [String] = UserDefaults.standard.stringArray(forKey: "workspaceRecentListsV1") ?? []
  /// ⌃- and ⌃⇧-: the lists behind and ahead of this one in this session.
  @ObservationIgnored var listBackStack: [String] = []
  @ObservationIgnored var listForwardStack: [String] = []
  @ObservationIgnored var isWalkingListHistory = false
  /// The outline task whose title is being edited in its own row.
  var editingTaskTitleID: String?
  /// Where that edit puts the cursor: `ee` selects the title, `ea` and `ei`
  /// type after or before it.
  var editingTaskTitleCaret: WorkspaceTitleCaret = .selectAll
  var pendingSidebarDeletion: WorkspaceSidebarItem?
  /// A task a delete key has asked to remove, waiting for Return. Held by id
  /// so a task that disappears meanwhile simply has nothing left to confirm.
  var pendingTaskDeletionID: String?
  /// Which surface asked for it — the window or the panel. The other one
  /// taking a key lets it go, so Return there cannot confirm a delete
  /// prompted somewhere you are not looking.
  var pendingTaskDeletionSurface: WorkspaceOverlaySurface = .window
  var taskComposerFocusRequest = 0
  /// Whether a draft row is open in the pane, at the place the task it
  /// becomes will land (`taskInsertionReference` and its two flags say where).
  var isDraftingTask = false {
    didSet {
      // Closing the row, by any route, drops what was typed in it.
      guard !isDraftingTask, oldValue else { return }
      taskDraftText = ""
      taskDraftSelection = nil
      taskDraftColumnID = nil
      todayDraftAfterID = nil
    }
  }
  /// On Today, the planned task a new one is typed under and will be ranked
  /// straight after, so Return adds beside where you are rather than at the
  /// foot of the day. Nil there means the foot.
  var todayDraftAfterID: String?
  /// What is typed in the draft row. Held here rather than in the row, which
  /// is rebuilt whenever the task it is drawn beside changes — and dropped
  /// when that task leaves the pane — so a rebuilt row picks up where the
  /// last one was. See `WorkspaceViewModel+TaskDraft.swift`.
  var taskDraftText = ""
  /// Where the caret was in it, for a rebuilt row to put it back. Not
  /// observed: it moves on every key, and nothing draws it.
  @ObservationIgnored var taskDraftSelection: NSRange?
  /// The board column a draft at the foot of a column was opened in.
  var taskDraftColumnID: String?
  /// Quick capture's own field, in the title bar: the one add that is not
  /// into the list on screen, so it is not drawn in it.
  var quickCaptureFocusRequest = 0
  /// The normal composer is deliberately lightweight. These fields are only
  /// active for global capture, where the destination can be chosen
  /// without taking your hands off the keyboard.
  var isQuickCaptureActive = false
  var quickCaptureDestinationID: String?
  /// Not observed: it moves on every key, and nothing should redraw for that.
  /// What the status bar shows is `pendingKeyPrefix`, which moves only when
  /// the half-typed sequence does.
  @ObservationIgnored var desktopShortcutSequence = DesktopShortcutSequence() {
    didSet {
      let prefix = desktopShortcutSequence.prefix
      if pendingKeyPrefix != prefix { pendingKeyPrefix = prefix }
    }
  }
  /// The first key of a two-letter sequence, held while the second is awaited.
  private(set) var pendingKeyPrefix = ""
  /// On by default: a ticked-off task stays where it was for
  /// `completedLingerInterval`, then leaves the pane — the Done rail is where
  /// finished work is kept. `tc` turns it off and shows every completion.
  var hidesCompletedTasks = true
  /// Off by default: a task whose start is still to come is buried — drawn
  /// greyed, with its subtasks, below its blocked siblings until it starts
  /// and comes back to its place (the core's order). `tl` hides them.
  var hidesLaterTasks = false
  /// When the soonest lingering completion is due to go, and the reload that
  /// takes it away. Not observed: nothing draws them.
  @ObservationIgnored var nextCompletionExpiry: Date?
  @ObservationIgnored var completionExpiryTask: Task<Void, Never>?
  var taskInsertionReference: WorkspaceTask?
  var taskInsertionAbove = false
  var taskInsertionIsChild = false
  /// Written only by `WorkspaceViewModel+KeyboardFocus.swift`; internal rather
  /// than `private(set)` so those methods can live there.
  var keyboardFocusArea: WorkspaceFocusArea = .tasks
  var requestedFocusArea: WorkspaceFocusArea = .tasks
  /// Whether the inspector, given the keyboard, puts the caret in the task's
  /// title. `i` does — it means "edit this" — but `r` leaves the keyboard on
  /// the pane, or the `r` that should put the dock away would be typed.
  var inspectorFocusesTitle = true
  /// Set by `f` (true) or `F` (false) until the letter to find is typed.
  @ObservationIgnored var pendingFindForward: Bool?
  /// What `;` repeats.
  @ObservationIgnored var lastFind: (character: Character, forward: Bool)?
  /// The digits typed after `:`, drawn in the status bar; nil when `:` is not
  /// waiting for a number.
  var numberJump: String?
  /// Each row's number while `:` waits, for the rows to draw. Today's rows
  /// number themselves, so they need nothing from here.
  var numberJumpNumbers: [String: Int] = [:]
  /// The last key sent to the inspector's cursor, which the inspector owns
  /// (`InspectorCursor`); a new serial each time, so the same key twice is
  /// two requests.
  var inspectorKeyRequest = InspectorKeyRequest(action: .step(0), serial: 0)
  /// Plain navigation keys only belong to the focused list/board surface.
  /// Buttons, menus, and other controls keep their native keyboard behavior.
  var keyboardNavigationSurfaceActive = false
  /// Incremented for every request, including a request for the already active
  /// region. SwiftUI observes this to make the native control first responder.
  var focusRequest = 0
  /// The running block.
  ///
  /// This used to tell the app shell when a session ended, so the always-on-top
  /// strip could be taken down from outside any window. The tray is not taken
  /// down: finishing a block is the moment you want to see what is next, and
  /// the tray is the surface the scoring happened on.
  var activeFocusSession: FocusSession?
  private(set) var focusQueue: [FocusQueueTask] = []
  @ObservationIgnored var dailyTaskIDs: Set<String> = []
  /// The tasks whose daily is a habit — the ones ticked rather than closed.
  @ObservationIgnored var habitTaskIDs: Set<String> = []
  /// Each task's live periodic commitments — the subtasks that bring it into
  /// Dailies on their days — keyed by the task. Read with the dailies.
  var commitmentsByTask: [String: [WorkspaceCommitment]] = [:]
  /// The day the habits were last placed for, so the first poll of a new day
  /// moves them. See `checkForDayChange`.
  @ObservationIgnored var habitDayKey: String?
  /// What each waiting task waits on and when to chase it, for the chips on
  /// cards and rows. Reread with the task cache, when the file has changed.
  var waitingDetails: [String: TaskWaitingDetails] = [:]
  /// The open tasks still waiting on an open task, drawn greyed. Read with
  /// `waitingDetails`, at the same change stamp.
  var blockedTaskIDs: Set<String> = []
  /// The open tasks in the current scope whose start is still to come, with
  /// their starts, drawn greyed in the buried section. Taken from the
  /// scope's read at each reload, and checked again when the soonest start
  /// arrives.
  var buriedStarts: [String: Date] = [:]
  /// Each prioritised task's priority, 1–4 or -4 to -1, for the number on its row.
  /// Read with `waitingDetails`, under the same change stamp.
  var taskPriorities: [String: Int] = [:]
  /// The tasks with at least one link, for the link mark on their rows.
  /// Read with `waitingDetails`, under the same change stamp.
  var linkedTaskIDs: Set<String> = []
  /// The store and its change stamp `waitingDetails` was read at. While both
  /// match, nothing has been written, and the reread is skipped: switching
  /// lists rebuilds the task cache but cannot change who waits on what.
  @ObservationIgnored var waitingDetailsStamp: (store: ObjectIdentifier, stamp: WorkspaceChangeStamp)?
  /// When the poll next looks for a follow-up that has come due.
  @ObservationIgnored var nextFollowUpCheck: Date = .distantPast
  /// What finishing the current block did — a task closed, or a day's
  /// contribution logged. Held so the UI can say which, then cleared.
  var lastFocusOutcome: WorkspaceStore.FocusCompletionOutcome?
  /// A finished block waiting to be told how it went. Set the instant Done is
  /// pressed — the clock stops when the work stops, not when the judgement
  /// arrives — and cleared when the block is scored or the prompt is dropped.
  var pendingFocusCompletion: PendingFocusCompletion?
  /// Which surface asked the question, so the answer is offered where the
  /// block was finished. The summoned panel has to be able to close a block
  /// on its own — needing the window for it is exactly what stops the panel
  /// being usable as the only surface.
  var focusCompletionSurface: FocusCompletionSurface = .window
  /// The pending block, but only for the surface that asked.
  var windowFocusCompletion: PendingFocusCompletion? {
    focusCompletionSurface == .window ? pendingFocusCompletion : nil
  }
  var panelFocusCompletion: PendingFocusCompletion? {
    focusCompletionSurface == .panel ? pendingFocusCompletion : nil
  }
  /// What the last scored block earned, so the UI can show it and move on.
  var lastFocusAward: FocusAward?
  /// Written only by `reloadFocus()`; read by the panel and the prompt.
  var focusPoints: FocusPointsSummary = .zero
  var focusHistoryDate = Date.now
  /// Today's finished blocks, always today's. `focusHistory` follows the
  /// timeline's own day picker, so a surface that means "today" cannot read it
  /// — steering the timeline to last Tuesday would change what it said.
  private(set) var todayWorkBlocks: [FocusWorkBlock] = []
  private(set) var focusHistory: [FocusWorkBlock] = []
  /// What the blocks in `focusHistory` were scored, keyed by block id — an
  /// award carries the block's own id. Blocks finished without a judgement
  /// have no entry, which is the difference the timeline draws.
  private(set) var focusHistoryAwards: [String: FocusAward] = [:]
  /// Whether the timeline has the main pane.

  var errorMessage: String?

  /// Told after rows changed under the app — a sync pull or another
  /// process's write — beside the reload of the task tree. The theme library
  /// and the synced choice of theme listen here.
  @ObservationIgnored var onWorkspaceChangedElsewhere: (() -> Void)?

  init(legacyStore: LegacyOfflineTaskStore = LegacyOfflineTaskStore()) {
    self.legacyStore = legacyStore
    do {
      self.store = try WorkspaceStore()
      try self.store?.recoverInterruptedFocus()
      // And then settle it: a block paused on a day that is over is finished
      // here rather than restored as the running session. Without this every
      // quit left a paused block behind that nothing ever cleared.
      try self.store?.resolveStaleFocusSession()
      attachAgentInstructions()
      try load()
      // Ranked here rather than in the background: the first screen is the
      // day, and it should not paint empty and then fill.
      reloadNextUpNow()
      restoreFocusContext()
      restoreFoldedConditionSections()
      watchForExternalWrites()
      startSync()
      startFocusMonitor()
    } catch {
      self.store = nil
      self.errorMessage = error.localizedDescription
    }
  }

  deinit {
    externalWriteTimer?.invalidate()
    completionExpiryTask?.cancel()
    focusMonitorTask?.cancel()
    for observer in focusMonitorObservers {
      NSWorkspace.shared.notificationCenter.removeObserver(observer)
      NotificationCenter.default.removeObserver(observer)
    }
  }

  var selectedList: TaskList? { lists.first { $0.id == selectedListID } }
  /// Where quick capture lands. Found by role, so renaming it does not move it.
  var inboxList: TaskList? { lists.first { $0.systemRole == .inbox } }
  var selectedFolder: ListFolder? { folders.first { $0.id == selectedFolderID } }
  /// Reports a finished task to whatever is staging celebrations, counting the
  /// day's remaining work so the last one of the day can be marked as such.
  func celebrateCompletion(of task: WorkspaceTask) {
    guard let onCompletion else { return }
    let remaining = dayItems.filter { $0.task.id != task.id }.count + 1
    guard let event = completionEvent(for: task, remainingVisibleTaskCount: remaining) else { return }
    onCompletion(event)
  }

  func list(for task: WorkspaceTask) -> TaskList? { lists.first { $0.id == task.listId } }

  var scopeTask: WorkspaceTask? {
    guard let scopeTaskID else { return nil }
    return task(withID: scopeTaskID)
  }

  /// Resolves a local drag payload only through the workspace store. A drop
  /// target never trusts an arbitrary identifier from outside the workspace.
  func task(withID id: String) -> WorkspaceTask? {
    _ = taskContentRevision
    if let cached = taskCache[id] { return cached }
    guard !missingTaskIDs.contains(id), let store else { return nil }
    guard let task = try? store.task(id: id) else {
      missingTaskIDs.insert(id)
      return nil
    }
    taskCache[id] = task
    return task
  }

  var selectedTask: WorkspaceTask? {
    guard let selectedTaskID else { return nil }
    return task(withID: selectedTaskID)
  }

  var activeFocusTask: WorkspaceTask? {
    guard let taskID = activeFocusSession?.activeTaskId else { return nil }
    return task(withID: taskID)
  }

  func load() throws {
    guard let store else { return }
    let workspace = try store.bootstrapIfNeeded()
    self.workspace = workspace
    let importedList = try migrateLegacyTasksIfNeeded(into: workspace, store: store)
    folders = try store.folders(in: workspace.id)
    lists = try store.lists(in: workspace.id)
    archivedLists = try store.lists(in: workspace.id, includingArchived: true).filter(\.isArchived)
    let loadedFilterLists = try store.filterLists(in: workspace.id)
    if filterLists != loadedFilterLists { filterLists = loadedFilterLists }
    agentInstructions.reload()
    if let selectedFilterListID, !filterLists.contains(where: { $0.id == selectedFilterListID }) {
      // Deleted here, by undo or on another device: Everything is what it
      // was drawn over.
      self.selectedFilterListID = nil
    }
    if isEverythingSelected {
      selectedListID = nil
      scopeTaskID = nil
    }
    // A folder scope has no selected list on purpose, so it must not be read
    // as "nothing is selected" and repaired into one on the next reload.
    let hasFolderScope = selectedFolderID.map { id in folders.contains { $0.id == id } } ?? false
    if !isEverythingSelected && !hasFolderScope
      && (selectedListID == nil || !lists.contains(where: { $0.id == selectedListID })) {
      scopeTaskID = nil
      selectedTaskID = nil
      // On the first desktop launch, put a migrated user straight into their
      // existing work rather than an empty Inbox. This also covers people who
      // ran an earlier preview that completed the import before the desktop
      // window became the default launch surface.
      selectedListID = importedList?.id
        ?? lists.first(where: { $0.name.hasPrefix("Imported from old Priority") })?.id
        ?? lists.first?.id
      if selectedList?.systemRole == .inbox {
        viewMode = .outline
      }
    }
    if newTaskListID == nil || !lists.contains(where: { $0.id == newTaskListID }) {
      newTaskListID = selectedListID ?? lists.first?.id
    }
    if let selectedFolderID, !folders.contains(where: { $0.id == selectedFolderID }) {
      self.selectedFolderID = nil
    }
    try migrateLegacyDailiesIfNeeded(store: store)
    // Rows can have changed anywhere, including a board's column layout.
    boardColumnConfigurations = nil
    reloadFocus()
    refresh([.outline, .sidebar, .dailies, .nextUp])
    // Launch draws one scope; the ones a first switch goes to are read
    // behind it, Everything among them.
    prefetchAdjacentScopes()
  }

  /// Today's dailies, joined to their tasks and contributions. Reloaded rather
  /// than computed, because every row needs a database round trip and the list
  /// is read on every render of the dailies surface.
  var dailyItems: [DailyItem] = []
  /// Today's work and what put each task there: the Today column, plus
  /// anything due today, overdue, or starting today. Derived on every
  /// `reloadNextUp()` rather than written into the column, so clearing the
  /// column still clears the plan and tomorrow's day is not yesterday's.
  var todayPlan: [DayPlanEntry] = []
  /// Today, resolved to tasks. Stored rather than computed: it is read on
  /// every render of the day and of the menu bar, and resolving it used to
  /// cost a query per row. Rebuilt by `rebuildDayItems()`.
  var dayItems: [DayItem] = []
  /// The day as it is drawn: `dayItems` with the subtasks of every task you
  /// have unfolded beneath it. Rebuilt with `dayItems`; see `DayOutline`.
  var dayTree: [DayTreeItem] = []
  /// Today's tasks whose subtasks are showing. Folded is the default, so the
  /// day reads as a flat list until you open a task; kept across launches,
  /// as the outline keeps its folds.
  var todayUnfoldedTaskIDs = Set(
    UserDefaults.standard.stringArray(forKey: WorkspaceViewModel.todayUnfoldedTasksKey) ?? []) {
    didSet {
      guard todayUnfoldedTaskIDs != oldValue else { return }
      UserDefaults.standard.set(Array(todayUnfoldedTaskIDs).sorted(), forKey: Self.todayUnfoldedTasksKey)
      rebuildDayTree()
    }
  }
  /// Tasks finished and time logged today, against the same for the week so
  /// far. Refreshed alongside the day rather than on a ticker — it only moves
  /// when work is finished or logged, which is exactly when the day reloads.
  var workProgress: WorkProgress = .empty
  /// What the done rail shows: tasks closed within its window, newest first.
  /// Loaded only while the rail is open — see `reloadCompleted()`.
  var completedTasks: [WorkspaceTask] = [] {
    didSet {
      // Grouped and indexed here, once per load, rather than per render of
      // the rail and per row of it.
      doneGroups = CompletedWorkDigest.group(completedTasks, completedAt: { $0.completedAt ?? .distantPast })
      completedTaskIndex = Dictionary(
        completedTasks.enumerated().map { ($1.id, $0) }, uniquingKeysWith: { first, _ in first })
    }
  }
  /// The rail's rows, grouped by the day they were closed, newest first.
  private(set) var doneGroups: [CompletedWorkGroup<WorkspaceTask>] = []
  /// Each completed task's position in `completedTasks`, by id.
  @ObservationIgnored private(set) var completedTaskIndex: [String: Int] = [:]
  /// The row the rail's cursor is on, or nil for "the newest thing finished".
  var doneCursorID: String?
  /// The timeline breakdown row the keyboard is on, by task key, or nil for
  /// "the task with the most time".
  var timelineCursorID: String?
  /// The top of the ranking, and why. Nil when there is nothing to do —
  /// which is a real state worth rendering, not an error.
  var nextUp: ScoredNextUp?
  /// The head of what is worth doing, most important first, and every task in
  /// the day further down. Today's Focus button and the panel's fallback
  /// shortlist read it. Not the whole ladder: the core cuts it to
  /// `WorkspaceNextUpSnapshot.fallbackDayLength` plus the day, so a write does
  /// not carry thousands of ranked tasks across to the main thread.
  var focusLadder: [ScoredNextUp] = []
  var focusConditions: [TaskCondition] = []
  /// The conditions an open task requires: what "Available now" offers.
  var requiredConditionIDs: Set<String> = []
  /// How many open tasks require each condition, by id: the core's count, in
  /// which a completed or archived task no longer counts.
  var conditionRequirementCounts: [String: Int] = [:]
  var taskLoggedSeconds: [String: Int] = [:]
  var taskPlanningByID: [String: TaskPlanning] = [:]
  /// What is said to hold now — the place chosen, the boxes ticked — with
  /// the time and the goal. What *holds* adds what those provide, less
  /// `contextOffIDs`: `heldConditionIDs`, which the ranking reads.
  var focusContext = FocusContext()
  /// Provided conditions switched off for now, until whatever provides them
  /// is unset or replaced.
  var contextOffIDs: Set<String> = []
  /// The last answer of `conditionHoldings`, and what it was worked out from.
  @ObservationIgnored var conditionHoldingsCache: ConditionHoldingsCache?
  var suggestedContextIDs: Set<String> = []
  /// How each day starts — the hour it changes and the conditions it starts
  /// with — as the account keeps it, read again whenever the context is.
  var dayStart = DayStart()
  /// When the clock last asked whether the day had changed.
  @ObservationIgnored var lastDayStartCheck = Date.distantPast
  /// The Conditions tab's cursor; nil is its first row.
  var conditionsCursor: WorkspaceConditionsStop?
  /// The condition whose name is being edited in its row, if any.
  var renamingConditionID: String?
  /// Moves to put the caret in the Conditions tab's new-condition field.
  var conditionsAddFieldRequest = 0
  /// What is being typed on the Conditions tab's Free time row; nil while it is
  /// not being typed in.
  var conditionsTimeEntry: String?
  /// The category the Conditions tab's new-condition row files under; nil
  /// is none.
  var newConditionCategory: String?
  /// The Conditions tab's folded headings, by section id; kept per workspace.
  var foldedConditionSections: Set<String> = []
  /// Whether the tab shows the archived conditions under their heading.
  var showsArchivedConditions = false
  /// The condition whose open tasks are listed under its row (`o`), and them.
  var conditionTasksShownFor: String?
  var conditionTasks: [ConditionRequiringTask] = []
  var contextExpiresAt: Date?
  /// When the conditions on were said, as the workspace stamped it: what
  /// makes a place left on overnight read as stale.
  var contextConditionsSetAt: Date?
  var availableUntil: Date?
  var blockedFocusTasks: [BlockedFocusTask] = []
  var nextFocusEvaluationAt: Date?
  var focusStartOverride: FocusStartOverride?
  var allowsQueueResume = false
  var lastFocusCheckpointAt = Date.distantPast
  var focusExpiryPromptedBlockID: String?
  var breakFinishedNotifiedBlockID: String?
  var lastFocusClockAt = Date.now
  var lastFocusUptime = ProcessInfo.processInfo.systemUptime
  var lastFocusTimeZone = TimeZone.current.identifier
  /// Bumped whenever Today is asked for afresh, so the pane resets its search
  /// field the way a fresh summon resets the panel's.
  var dayPresentationCount = 0

  func reloadFocus() {
    guard let store else { return }
    do {
      activeFocusSession = try store.activeFocusSession()
      if let session = activeFocusSession {
        focusQueue = try store.focusQueue(for: session.id)
      } else {
        focusQueue = []
      }
      focusPoints = try store.focusPointsSummary()
      if let today = Calendar.current.dateInterval(of: .day, for: .now) {
        todayWorkBlocks = try store.focusWorkBlocks(in: today)
      }
      if let day = Calendar.current.dateInterval(of: .day, for: focusHistoryDate) {
        focusHistory = try store.focusWorkBlocks(in: day)
        focusHistoryAwards = Dictionary(
          try store.focusAwards(onDayOf: focusHistoryDate).map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
      }
    } catch {
      errorMessage = error.localizedDescription
    }
  }
}
