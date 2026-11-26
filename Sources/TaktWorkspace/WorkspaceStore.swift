import Foundation
import TaktRustCore
import TaktCore

/// The local source of truth for Priority's desktop workspace.
///
/// It intentionally has no knowledge of Checkvist or app UI. External services
/// translate their data into local records at the edge, so normal task editing
/// never needs network access.
public final class WorkspaceStore: @unchecked Sendable {
  /// The Rust core's handle on the workspace file (core/src/workspace.rs):
  /// every read and write goes through it, so the Mac, the iPhone, Android
  /// and the CLI share one implementation (docs/rust-core-migration.md).
  /// Internal rather than private so the sibling files — the same type, split
  /// only for size — can reach it.
  let core: CoreWorkspace
  /// The file, for the background handle below.
  private let databasePath: String
  /// A second handle on the same file, opened the first time it is asked
  /// for, for the reads a client makes off its main thread to keep what it
  /// shows resident (`scopeRead(_:hidingCompletedBefore:inBackground:)`).
  /// `core` serialises every call on one connection, so a 20 ms background
  /// read of Everything made there was 20 ms a keystroke's write could wait
  /// behind. On a handle of its own, it waits on nothing the main thread
  /// does; WAL lets it read while the main handle writes.
  private var backgroundCoreStorage: CoreWorkspace?
  private let backgroundCoreLock = NSLock()

  public convenience init() throws {
    try self.init(databaseURL: WorkspaceStore.defaultDatabaseURL())
  }

  public init(databaseURL: URL) throws {
    let directory = databaseURL.deletingLastPathComponent()
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    // The schema is the Rust core's (core/src/schema): it brings the file up
    // to date under GRDB's old `grdb_migrations` ledger, which every client
    // and older build still reads, before the handle opens it. The handle
    // sets a five-second busy timeout, as the CLI does, and WAL.
    _ = try migrateWorkspace(path: databaseURL.path)
    self.core = try CoreWorkspace.open(path: databaseURL.path)
    self.databasePath = databaseURL.path
  }

  /// See `backgroundCoreStorage`. Safe from any thread.
  func backgroundCore() throws -> CoreWorkspace {
    backgroundCoreLock.lock()
    defer { backgroundCoreLock.unlock() }
    if let handle = backgroundCoreStorage { return handle }
    let handle = try Self.mappingCoreErrors { try CoreWorkspace.open(path: databasePath) }
    backgroundCoreStorage = handle
    return handle
  }

  public static func defaultDatabaseURL() -> URL {
    // The file keeps the product's old name: it is the one thing every other
    // reader (the CLI, `scripts/dump_workspace_schema.sh`) finds by name, and
    // renaming it would buy nothing a user can see.
    AppIdentity.applicationSupportDirectory()
      .appending(path: "priority.sqlite", directoryHint: .notDirectory)
  }

  @discardableResult
  public func bootstrapIfNeeded(now: Date = .now) throws -> Workspace {
    // The Rust core's `setup::bootstrap`, outside the journal.
    let id = try coreWrite { try core.bootstrap(nowMs: now.coreMilliseconds) }
    guard let workspace = try workspaces().first(where: { $0.id == id }) else {
      throw WorkspaceStoreError.missingList
    }
    return workspace
  }

  /// The list quick capture lands in. Never archived, never deleted, and found
  /// by its role rather than by its name.
  public func inbox(in workspaceId: String) throws -> TaskList? {
    try Self.mappingCoreErrors { try core.inbox(workspaceId: workspaceId) }.map(TaskList.init)
  }

  @discardableResult

  public func workspaces() throws -> [Workspace] {
    try Self.mappingCoreErrors { try core.workspaces() }.map(Workspace.init)
  }

  public func folders(in workspaceId: String) throws -> [ListFolder] {
    try Self.mappingCoreErrors { try core.folders(workspaceId: workspaceId) }.map(ListFolder.init)
  }

  public func lists(in workspaceId: String, includingArchived: Bool = false) throws -> [TaskList] {
    try Self.mappingCoreErrors {
      try core.lists(workspaceId: workspaceId, includingArchived: includingArchived)
    }.map(TaskList.init)
  }

  /// Creates a folder after its siblings: the Rust core's `lists::create_folder`.
  public func createFolder(
    workspaceId: String,
    name: String,
    parentFolderId: String? = nil,
    now: Date = .now
  ) throws -> ListFolder {
    let created = try coreWrite {
      try core.createFolder(
        workspaceId: workspaceId, name: name, parentFolderId: parentFolderId, nowMs: now.coreMilliseconds)
    }
    return ListFolder(
      id: created.id, workspaceId: workspaceId, parentFolderId: parentFolderId,
      name: created.name, sortOrder: Int(created.sortOrder), createdAt: now, updatedAt: now)
  }

  /// Creates a list after its siblings: the Rust core's `lists::create_list`.
  public func createList(
    workspaceId: String,
    name: String,
    folderId: String? = nil,
    now: Date = .now
  ) throws -> TaskList {
    let created = try coreWrite {
      try core.createList(workspaceId: workspaceId, name: name, folderId: folderId, nowMs: now.coreMilliseconds)
    }
    return TaskList(
      id: created.id, workspaceId: workspaceId, folderId: folderId, name: created.name,
      colorHex: nil, sortOrder: Int(created.sortOrder), isArchived: false, createdAt: now, updatedAt: now)
  }

  public func task(id: String) throws -> WorkspaceTask? {
    try Self.mappingCoreErrors { try core.task(id: id) }.map(WorkspaceTask.init)
  }

  /// Renames a folder: the Rust core's `lists::rename_folder`.
  public func updateFolder(id: String, name: String, now: Date = .now) throws {
    try coreWrite { try core.renameFolder(id: id, name: name, nowMs: now.coreMilliseconds) }
  }

  /// Moves a folder into another or to the top: the Rust core's `lists::move_folder`.
  public func moveFolder(id: String, toParentFolderId parentFolderId: String?, now: Date = .now) throws {
    try coreWrite { try core.moveFolder(id: id, parentFolderId: parentFolderId, nowMs: now.coreMilliseconds) }
  }

  /// Deletes a folder as one undo step. Its lists move to the sidebar root
  /// (the schema's SET NULL) and folders inside it go with it. The write is
  /// the Rust core's (`lists::delete_folder`).
  public func deleteFolder(id: String) throws {
    try coreWrite { _ = try core.deleteFolder(id: id) }
  }

  /// Sets a list's name and colour: the Rust core's `lists::update_list`.
  public func updateList(id: String, name: String, colorHex: String?, now: Date = .now) throws {
    try coreWrite {
      try core.updateList(id: id, name: name, colourHex: colorHex, nowMs: now.coreMilliseconds)
    }
  }

  /// Renames a list and touches nothing else.
  ///
  /// Separate from `updateList` so that renaming from the sidebar cannot carry
  /// a stale colour along with it, and so undo offers "Rename List" rather
  /// than the settings sheet's broader "Edit List".
  /// Renames a list and nothing else: the Rust core's `lists::rename_list`.
  public func renameList(id: String, name: String, now: Date = .now) throws {
    try coreWrite { try core.renameList(id: id, name: name, nowMs: now.coreMilliseconds) }
  }

  /// Moves a list into a folder or to the top: the Rust core's `lists::move_list`.
  public func moveList(id: String, toFolderId folderId: String?, now: Date = .now) throws {
    try coreWrite { try core.moveList(id: id, folderId: folderId, nowMs: now.coreMilliseconds) }
  }

  /// Moves a list among its siblings: the Rust core's `lists::move_list_within_folder`.
  public func moveListWithinFolder(id: String, by offset: Int, now: Date = .now) throws {
    try coreWrite {
      try core.moveListWithinFolder(id: id, offset: Int32(clamping: offset), nowMs: now.coreMilliseconds)
    }
  }

  /// Puts `id` immediately before `targetID` among its siblings, moving it
  /// into `folderId` first when the drag crossed a folder boundary.
  ///
  /// `targetID` of nil means the end of that group, which is what a drop below
  /// the last row means. Reordering is absolute rather than a signed offset
  /// because a drag says where a thing landed, not how far it travelled.
  /// Drops a list before another in a folder: the Rust core's `lists::place_list`.
  public func placeList(
    id: String, before targetID: String?, inFolderId folderId: String?, now: Date = .now
  ) throws {
    try coreWrite {
      try core.placeList(id: id, beforeId: targetID, folderId: folderId, nowMs: now.coreMilliseconds)
    }
  }

  /// The same placement for folders, which nest, so the parent is checked for
  /// the cycle a folder dropped inside its own descendant would make.
  /// Drops a folder before another in a parent: the Rust core's `lists::place_folder`.
  public func placeFolder(
    id: String, before targetID: String?, inParentFolderId parentFolderId: String?, now: Date = .now
  ) throws {
    try coreWrite {
      try core.placeFolder(id: id, beforeId: targetID, parentFolderId: parentFolderId, nowMs: now.coreMilliseconds)
    }
  }

  /// Moves a folder among its siblings: the Rust core's `lists::move_folder_within_siblings`.
  public func moveFolderWithinSiblings(id: String, by offset: Int, now: Date = .now) throws {
    try coreWrite {
      try core.moveFolderWithinSiblings(id: id, offset: Int32(clamping: offset), nowMs: now.coreMilliseconds)
    }
  }

  /// Archives or restores a list; the Inbox stays. The Rust core's
  /// `lists::set_list_archived`.
  public func setListArchived(_ archived: Bool, id: String, now: Date = .now) throws {
    try coreWrite {
      try core.setListArchived(id: id, archived: archived, nowMs: now.coreMilliseconds)
    }
  }

  /// Puts a list at `position` among the prioritised lists (1 the most
  /// important; past the end, last), or takes it out of them with nil, as
  /// one undo step. An archived or completed list cannot be prioritised.
  /// The write is the Rust core's (`lists::set_list_priority`).
  public func setListPriority(_ position: Int?, id: String, now: Date = .now) throws {
    try coreWrite {
      try core.setListPriority(id: id, position: position.map { Int64($0) }, nowMs: now.coreMilliseconds)
    }
  }

  /// Deletes a list and its tasks as one undo step; the Inbox is permanent.
  /// The write is the Rust core's (`lists::delete_list`).
  public func deleteList(id: String) throws {
    try coreWrite { _ = try core.deleteList(id: id) }
  }

  public func outline(in listId: String, parentTaskId: String? = nil) throws -> [TaskOutlineItem] {
    try listTree(in: listId).outline(under: parentTaskId)
  }

  /// The direct children used by a project's own Kanban board. Descendants are
  /// intentionally excluded: each project can be entered and planned as its
  /// own board without inheriting its parent's column.
  public func tasks(in listId: String, parentTaskId: String? = nil) throws -> [WorkspaceTask] {
    try Self.mappingCoreErrors {
      try core.childTasks(listId: listId, parentTaskId: parentTaskId)
    }.map(WorkspaceTask.init)
  }

  /// Returns the imported wrapper by its persisted identity. If it has been
  /// moved or promoted alongside other roots, show the actual hierarchy.
  public func visibleRootParentTaskID(for list: TaskList) throws -> String? {
    try Self.mappingCoreErrors { try core.visibleRootParent(listId: list.id) }
  }

  /// The virtual Everything scope aggregates active lists without changing
  /// any task's real list or parent.
  public func visibleRootTasks(in workspaceId: String) throws -> [WorkspaceTask] {
    try lists(in: workspaceId).flatMap { list in
      let parentID = try visibleRootParentTaskID(for: list)
      return try tasks(in: list.id, parentTaskId: parentID)
    }
  }

  /// Creates a task, with whatever the add field read off its title, as one
  /// undo step: the Rust core's `tasks::create_task`.
  public func createTask(
    listId: String,
    title: String,
    parentTaskId: String? = nil,
    kind: WorkspaceItemKind = .task,
    kanbanColumn: String? = nil,
    startAt: Date? = nil,
    atTop: Bool = false,
    adjacentTaskId: String? = nil,
    above: Bool = false,
    dueAt: Date? = nil,
    estimateSeconds: Int? = nil,
    tags: [String] = [],
    priority: Int? = nil,
    waitingOn: String? = nil,
    externalLinks: [String] = [],
    now: Date = .now
  ) throws -> WorkspaceTask {
    let new = NewTask(
      listId: listId, title: title, parentTaskId: parentTaskId, kind: kind.rawValue, notes: "",
      kanbanColumn: kanbanColumn, startAtMs: startAt?.coreMilliseconds, dueAtMs: dueAt?.coreMilliseconds,
      estimateSeconds: estimateSeconds.map(Int64.init), tags: tags, priority: priority.map(Int64.init),
      waitingOn: waitingOn, externalLinks: externalLinks, atTop: atTop, adjacentTaskId: adjacentTaskId, above: above)
    let id = try coreWrite { try core.createTask(task: new, nowMs: now.coreMilliseconds) }
    guard let task = try task(id: id) else {
      throw WorkspaceStoreError.missingTask
    }
    return task
  }

  /// Opens, completes or cancels a task: the Rust core's `tasks::set_status`,
  /// which also writes a repeating task's next occurrence, stepped in the
  /// user's time zone, and ends the habits made from it.
  public func setStatus(_ status: TaskStatus, for taskId: String, now: Date = .now) throws {
    try coreWrite {
      try core.setStatus(
        taskId: taskId, status: status.rawValue, nowMs: now.coreMilliseconds,
        zone: TimeZone.current.identifier)
    }
  }

  /// Sets a task's title, notes, due time and estimate: the Rust core's
  /// `editor::update_task`. A due time replaces a due date in its planning.
  public func updateTask(
    id: String,
    title: String,
    notes: String,
    dueAt: Date?,
    estimateSeconds: Int?,
    now: Date = .now
  ) throws {
    try coreWrite {
      try core.updateTask(
        id: id, title: title, notes: notes, dueAtMs: dueAt?.coreMilliseconds,
        estimateSeconds: estimateSeconds.map { Int64($0) }, nowMs: now.coreMilliseconds, zone: TimeZone.current.identifier)
    }
  }

  public func taskEditorMetadata(for taskId: String) throws -> TaskEditorMetadata {
    try taskEditorSnapshot(for: taskId).metadata
  }

  /// Every task that has a priority, and what it is (1–4, or -4 to -1), so a row's
  /// number is a dictionary lookup rather than a read per row.
  public func taskPriorities() throws -> [String: Int] {
    var priorities: [String: Int] = [:]
    for row in try Self.mappingCoreErrors({ try core.allMetadata() }) {
      if let priority = row.priority.map(Int.init), TaskEditorValues.isPriority(priority) {
        priorities[row.taskId] = priority
      }
    }
    return priorities
  }

  /// Sets a task's priority, tags, links and repeat: the Rust core's
  /// `editor::update_editor_metadata`.
  public func updateTaskEditorMetadata(
    taskId: String, metadata: TaskEditorMetadata, now: Date = .now
  ) throws {
    try coreWrite {
      try core.updateEditorMetadata(taskId: taskId, metadata: metadata.core, nowMs: now.coreMilliseconds)
    }
  }

  public func kanbanColumn(for taskId: String) throws -> String? {
    try Self.mappingCoreErrors { try core.metadata(taskId: taskId) }?.kanbanColumn
  }

  /// Loads board placement in batches rather than opening a read per card.
  /// Missing metadata retains the same unplaced/default-column behaviour.
  public func boardMetadata(for taskIDs: [String]) throws -> (
    columns: [String: String], positions: [String: TaskMatrixPosition], starts: [String: Date]
  ) {
    try boardMetadata(for: taskIDs, using: core)
  }

  func boardMetadata(for taskIDs: [String], using handle: CoreWorkspace) throws -> (
    columns: [String: String], positions: [String: TaskMatrixPosition], starts: [String: Date]
  ) {
    guard !taskIDs.isEmpty else { return ([:], [:], [:]) }
    var columns: [String: String] = [:]
    var starts: [String: Date] = [:]
    var positions = Dictionary(uniqueKeysWithValues: Set(taskIDs).map {
      ($0, TaskMatrixPosition(urgency: nil, importance: nil))
    })
    for record in try Self.mappingCoreErrors({ try handle.metadataForTasks(taskIds: taskIDs) }) {
      columns[record.taskId] = record.kanbanColumn
      starts[record.taskId] = record.startAtMs.map { Date(coreMilliseconds: $0) }
      positions[record.taskId] = TaskMatrixPosition(
        urgency: record.matrixUrgency.map { Int($0) }, importance: record.matrixImportance.map { Int($0) },
        rank: record.matrixRank.map { Int($0) })
    }
    return (columns, positions, starts)
  }

  public func setKanbanColumn(_ column: String?, for taskId: String, now: Date = .now) throws {
    try setKanbanColumn(column, for: [taskId], now: now)
  }

  /// Moving a whole column is one transaction and one undo step.
  /// Puts tasks in a board column as one step: the Rust core's
  /// `tasks::set_kanban_column`.
  public func setKanbanColumn(_ column: String?, for taskIDs: [String], now: Date = .now) throws {
    guard !taskIDs.isEmpty else { return }
    try coreWrite {
      try core.setKanbanColumn(taskIds: taskIDs, column: column, nowMs: now.coreMilliseconds)
    }
  }

  public func matrixPosition(for taskId: String) throws -> TaskMatrixPosition {
    let metadata = try Self.mappingCoreErrors { try core.metadata(taskId: taskId) }
    return TaskMatrixPosition(
      urgency: metadata?.matrixUrgency.map { Int($0) }, importance: metadata?.matrixImportance.map { Int($0) },
      rank: metadata?.matrixRank.map { Int($0) })
  }

  /// Places a task on the priority matrix: the Rust core's `tasks::set_matrix_position`.
  public func setMatrixPosition(
    _ position: TaskMatrixPosition,
    for taskId: String,
    now: Date = .now
  ) throws {
    try coreWrite {
      try core.setMatrixPosition(
        id: taskId, urgency: position.urgency.map(Int64.init), importance: position.importance.map(Int64.init),
        nowMs: now.coreMilliseconds)
    }
  }

  /// Places a task in a matrix quadrant at a point: `order` is the quadrant
  /// as it should now read, top to bottom, the task among it. The Rust
  /// core's `tasks::place_in_matrix`, one "Move Task" step.
  public func placeInMatrix(
    taskId: String, urgency: Int, importance: Int, order: [String], now: Date = .now
  ) throws {
    try coreWrite {
      try core.placeInMatrix(
        id: taskId, urgency: Int64(urgency), importance: Int64(importance), order: order,
        nowMs: now.coreMilliseconds)
    }
  }

  /// Moves a task and its complete subtree. A destination parent must belong to
  /// the destination list and may not be the task itself or one of its descendants.
  /// Moves a task and its subtree: the Rust core's `tasks::move_task`.
  public func moveTask(
    id: String, toListId listId: String, parentTaskId: String? = nil,
    toVisibleRoot: Bool = false, now: Date = .now
  ) throws {
    try coreWrite {
      try core.moveTask(
        id: id, listId: listId, parentTaskId: parentTaskId, toVisibleRoot: toVisibleRoot, nowMs: now.coreMilliseconds)
    }
  }

  /// Moves a task among its siblings: the Rust core's `tasks::move_task_within_siblings`.
  public func moveTaskWithinSiblings(id: String, by offset: Int, now: Date = .now) throws {
    try coreWrite {
      try core.moveTaskWithinSiblings(id: id, offset: Int32(clamping: offset), nowMs: now.coreMilliseconds)
    }
  }

  /// Places a newly captured task at the top of its project/list. The Today
  /// queue uses this same persistent order, so the first card is first live.
  /// Moves a task to the top of its siblings: the Rust core's `tasks::move_task_to_start`.
  public func moveTaskToStart(id: String, now: Date = .now) throws {
    try coreWrite { try core.moveTaskToStart(id: id, nowMs: now.coreMilliseconds) }
  }

  /// Reorders a card before another card without changing either task's real
  /// list or project parent. Cross-project drops remain a list-move operation.
  /// Drops a task before a sibling: the Rust core's `tasks::move_task_before`.
  public func moveTaskBefore(id: String, targetId: String, kanbanColumn: String? = nil, now: Date = .now) throws {
    try coreWrite {
      try core.moveTaskBefore(id: id, targetId: targetId, kanbanColumn: kanbanColumn, nowMs: now.coreMilliseconds)
    }
  }

  /// Makes the selected task a child of its immediately preceding sibling.
  /// Indents a task under the sibling above: the Rust core's `tasks::indent_task`.
  public func indentTask(id: String, now: Date = .now) throws {
    try coreWrite { try core.indentTask(id: id, nowMs: now.coreMilliseconds) }
  }

  /// Promotes a task one level, immediately after its former parent.
  /// Outdents a task to follow its parent: the Rust core's `tasks::outdent_task`.
  public func outdentTask(id: String, now: Date = .now) throws {
    try coreWrite { try core.outdentTask(id: id, nowMs: now.coreMilliseconds) }
  }

  /// Outdents a task without taking it out of the scope it is seen in. A
  /// task already at the top of the pane — its parent is the task entered
  /// (`scopeTaskID`, a nested list say) or the list's own wrapper root — has
  /// no level above it there: outdenting it carried it out of the scope, and
  /// the scope then showed it from outside. Returns whether the task moved.
  @discardableResult
  public func outdentTask(id: String, within scopeTaskID: String?, now: Date = .now) throws -> Bool {
    guard let task = try task(id: id), let parent = task.parentTaskId, parent != scopeTaskID else { return false }
    let listRoot = try Self.mappingCoreErrors { try core.visibleRootParent(listId: task.listId) }
    guard parent != listRoot else { return false }
    try outdentTask(id: id, now: now)
    return true
  }

  /// Deletes a task and its subtree as one undo step. The write is the Rust
  /// core's (`tasks::delete_task`), shared with Android and the CLI.
  public func deleteTask(id: String) throws {
    try coreWrite { _ = try core.deleteTask(id: id) }
  }

  /// Runs a call into the Rust core, turning the failures callers react to
  /// into the store's own errors, so the screens that match on
  /// `WorkspaceStoreError` see the same cases whichever side made the write.
  static func mappingCoreErrors<T>(_ call: () throws -> T) throws -> T {
    do {
      return try call()
    } catch {
      switch error.coreFailure {
      case .missingTask: throw WorkspaceStoreError.missingTask
      case .missingDaily: throw WorkspaceStoreError.missingDaily
      case .missingList: throw WorkspaceStoreError.missingList
      case .missingFolder: throw WorkspaceStoreError.missingFolder
      case .systemListIsPermanent: throw WorkspaceStoreError.systemListIsPermanent
      case .emptyName: throw WorkspaceStoreError.emptyName
      case .invalidFolderMove: throw WorkspaceStoreError.invalidFolderMove
      case .invalidCondition: throw TaskPlanningError.invalidCondition
      case .invalidSchedule: throw TaskPlanningError.invalidSchedule
      case .invalidMinimum: throw TaskPlanningError.invalidMinimum
      case .estimateRequired: throw TaskPlanningError.estimateRequired
      case .invalidDate: throw TaskPlanningError.invalidDate
      case .editorConflict: throw TaskEditorError.conflictingChanges
      case .invalidVisibleRoot: throw TaskEditorError.invalidVisibleRoot
      case .noActiveFocusTask: throw WorkspaceStoreError.noActiveFocusTask
      case .duplicateSourceId: throw WorkspaceStoreError.duplicateLegacySourceID
      case .unavailable: throw TaskPlanningError.unavailable
      case .invalidTaskMove: throw WorkspaceStoreError.invalidTaskMove
      case .dependencyCycle: throw WorkspaceStoreError.dependencyCycle
      case .missingFilterList: throw WorkspaceStoreError.missingFilterList
      case .invalidFilter(let detail): throw WorkspaceStoreError.invalidFilter(detail)
      case .invalidRecurrence(let rule): throw WorkspaceStoreError.invalidRecurrence(rule)
      case .columnRefused(let detail): throw WorkspaceStoreError.columnRefused(detail)
      case .noJournal, .other, nil: throw error
      }
    }
  }
}

public enum WorkspaceStoreError: LocalizedError, Equatable {
  case emptyName
  case duplicateLegacySourceID
  case missingTask
  case missingDaily
  case noActiveFocusTask
  case missingList
  case missingFolder
  case invalidTaskMove
  case dependencyCycle
  case missingFilterList
  case invalidFilter(String)
  case invalidRecurrence(String)
  case columnRefused(String)
  case invalidFolderMove
  case systemListIsPermanent

  public var errorDescription: String? {
    switch self {
    case .emptyName: return "A workspace item needs a name."
    case .duplicateLegacySourceID: return "The legacy task store contains duplicate task IDs."
    case .missingTask: return "That task is no longer available."
    case .missingDaily: return "That daily is no longer available."
    case .noActiveFocusTask: return "This focus session has no active task."
    case .missingList: return "That list is no longer available."
    case .missingFolder: return "That folder is no longer available."
    case .invalidTaskMove: return "A task cannot be moved into itself or one of its subtasks."
    case .dependencyCycle: return "A task cannot wait on itself, or on a task that already waits on it."
    case .missingFilterList: return "That filter list is no longer available."
    case .invalidFilter(let detail): return detail
    case .columnRefused(let detail): return detail
    case .invalidRecurrence(let rule):
      return "“\(rule)” is not a schedule. Try daily, weekdays, every friday, every 2 weeks or mon, wed, fri."
    case .invalidFolderMove: return "A folder cannot be moved into itself or one of its subfolders."
    case .systemListIsPermanent: return "The Inbox cannot be archived or deleted. You can rename it instead."
    }
  }
}

private extension String {
  var nilIfEmpty: String? { isEmpty ? nil : self }
}

extension Date {
  /// This moment as the Rust core takes it: whole milliseconds since 1970,
  /// rounded to the nearest. A date GRDB read back from `...20.123` can be a
  /// hair under it, and truncating would make it `...20.122`, so a value
  /// read, sent to the core and compared there would no longer match.
  var coreMilliseconds: Int64 { Int64((timeIntervalSince1970 * 1000).rounded()) }
}
