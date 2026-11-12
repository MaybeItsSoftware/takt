import Foundation
import Observation
import TaktCore
import TaktWorkspace

/// One drawn outline row. A value, so the list diffs rows by content and a
/// row that did not change is not redrawn.
struct OutlineRow: Identifiable, Equatable, Sendable {
  let id: String
  let title: String
  let depth: Int
  let status: TaskStatus
  let isList: Bool
  let isPromoted: Bool
  let parentID: String?
  let listID: String
  let dueAt: Date?
  let estimateSeconds: Int?
  let hasNotes: Bool
  let hasChildren: Bool
  let isFolded: Bool
  let isPlanned: Bool
  /// The list's name, in a combined scope where rows come from many lists.
  let listName: String?
  /// What the Mac sets beside the title: tags, priority, waiting on, links.
  var chips: TaskChipMetadata?
  /// Children done and in all, for a parent: the Mac's `3/5`.
  var childrenDone = 0
  var childrenTotal = 0

  var menuContext: TaskMenuContext {
    TaskMenuContext(
      taskID: id, title: title, status: status, isList: isList, isPromoted: isPromoted,
      isPlanned: isPlanned)
  }
}

/// What a scope's outline reads from the store, before folding. Kept so a
/// fold or unfold can redraw without reading the database again.
struct OutlineBase: Sendable, Equatable {
  var items: [TaskOutlineItem] = []
  var planned: Set<String> = []
  var listNames: [String: String] = [:]
  var chips: [String: TaskChipMetadata] = [:]
  /// The task the scope's top-level rows hang from: a nested list's task, or
  /// an imported wrapper; nil for a list's own top level.
  var rootParentID: String?
  var isCombined = false

  /// Reads a scope. Pure over the store; safe off the main actor.
  static func load(
    store: WorkspaceStore, workspaceID: String, scope: ListScope, structure: WorkspaceStructure,
    hidesCompleted: Bool
  ) throws -> OutlineBase {
    var hidesCompleted = hidesCompleted
    var base = OutlineBase()
    switch scope {
    case .list(let listID):
      let tree = try store.listTree(in: listID)
      let parent = tree.visibleRootParentTaskID(registeredRootId: structure.list(listID)?.visibleRootTaskId)
      base.rootParentID = parent
      base.items = tree.visibleOutline(under: parent)
    case .nested(let listID, let taskID):
      let tree = try store.listTree(in: listID)
      base.rootParentID = taskID
      base.items = tree.visibleOutline(under: taskID)
    case .everything:
      base.isCombined = true
      base.items = try store.actionableTasks(in: workspaceID).map { TaskOutlineItem(task: $0, depth: 0) }
    case .folder(let folderID):
      base.isCombined = true
      base.items = try store.actionableTasks(in: workspaceID, limitedTo: structure.listIDs(inFolder: folderID))
        .map { TaskOutlineItem(task: $0, depth: 0) }
    case .filter(let filterID):
      base.isCombined = true
      guard let filter = try store.filterList(id: filterID) else { break }
      base.items = try store.filterListTasks(filter.definition, in: workspaceID)
        .map { TaskOutlineItem(task: $0, depth: 0) }
      // What the filter list asks for is what it shows: hiding finished work
      // would empty a list made to show it.
      hidesCompleted = filter.definition.hidesCompleted(whenToggled: hidesCompleted)
    }
    if hidesCompleted { base.items.removeAll { $0.task.status != .open } }
    let columns = try store.boardMetadata(for: base.items.map(\.id)).columns
    base.chips = (try? store.chipMetadata(for: base.items.map(\.id))) ?? [:]
    base.planned = Set(columns.compactMap { $0.value == NextUpSelector.todayColumnID ? $0.key : nil })
    if base.isCombined {
      base.listNames = Dictionary(structure.lists.map { ($0.id, $0.name) }, uniquingKeysWith: { first, _ in first })
    }
    return base
  }

  /// The rows as drawn: folded branches removed, each row told whether it
  /// has children and whether it is folded.
  func rows(folded: Set<String>) -> [OutlineRow] {
    let parents = TaskOutlineFolding.parentIDs(items)
    var counts: [String: (done: Int, total: Int)] = [:]
    for item in items {
      guard let parent = item.task.parentTaskId else { continue }
      var count = counts[parent] ?? (0, 0)
      count.total += 1
      if item.task.status == .completed { count.done += 1 }
      counts[parent] = count
    }
    return TaskOutlineFolding.visible(items, folded: folded).map { item in
      let task = item.task
      let count = counts[task.id] ?? (0, 0)
      return OutlineRow(
        id: task.id, title: task.title, depth: item.depth, status: task.status, isList: task.isList,
        isPromoted: task.isPromoted == true, parentID: task.parentTaskId, listID: task.listId,
        dueAt: task.dueAt, estimateSeconds: task.estimateSeconds, hasNotes: !task.notes.isEmpty,
        hasChildren: parents.contains(task.id), isFolded: folded.contains(task.id),
        isPlanned: planned.contains(task.id), listName: isCombined ? listNames[task.listId] : nil,
        chips: chips[task.id], childrenDone: count.done, childrenTotal: count.total)
    }
  }
}

/// How a drag in the outline lands in the tree.
///
/// A drop is read from its neighbours: dropped in front of a sibling, it goes
/// before that sibling; dropped after the last of its siblings, it goes to
/// the end; dropped among another parent's children, it joins that parent.
enum OutlineDrop: Equatable {
  case before(String)
  case toEnd
  case reparent(parentID: String?, before: String?)
}

enum OutlineDropPlanner {
  /// `destination` is SwiftUI's `onMove` offset: the index in `rows`, before
  /// the move, that the row is inserted in front of. The rule is the core's,
  /// through `OutlineDropRule`.
  static func plan(rows: [OutlineRow], source: Int, destination: Int) -> OutlineDrop? {
    let plan = OutlineDropRule.plan(
      rows: rows.map { OutlineDropRule.Row(depth: $0.depth, parentID: $0.parentID, listID: $0.listID) },
      source: source, destination: destination)
    switch plan {
    case nil: return nil
    case .before(let target): return .before(rows[target].id)
    case .toEnd: return .toEnd
    case .reparent(let anchor, let before):
      return .reparent(parentID: rows[anchor].parentID, before: before ? rows[anchor].id : nil)
    }
  }
}

/// Where the inline composer is open: what the new task's parent will be,
/// and the row it is drawn after.
struct OutlineComposer: Equatable {
  var parentID: String?
  /// The sibling the task is created next to; nil appends to the parent.
  var adjacentID: String?
  var above = false
  var depth: Int
  /// The row the field is drawn after; nil draws it first.
  var anchorRowID: String?
  var text = ""
}

/// One list scope's outline: reading it, folding it, and the inline
/// composer and rename state.
@MainActor
@Observable
final class OutlineModel {
  let scope: ListScope
  private(set) var rows: [OutlineRow] = []
  private(set) var isLoaded = false
  var composer: OutlineComposer?
  var editingTaskID: String?
  var editingText = ""

  var folded: Set<String> {
    didSet {
      guard folded != oldValue else { return }
      UserDefaults.standard.set(Array(folded).sorted(), forKey: foldKey)
      reflow()
    }
  }

  var hidesCompleted: Bool {
    didSet {
      guard hidesCompleted != oldValue else { return }
      UserDefaults.standard.set(hidesCompleted, forKey: "outlineHidesCompleted")
      needsReload = true
    }
  }

  /// Moves when the base read must be repeated though nothing was written —
  /// hiding completed tasks changes what is read.
  private(set) var reloadToken = 0
  @ObservationIgnored private(set) var base = OutlineBase()
  @ObservationIgnored private var generation = 0
  @ObservationIgnored private var needsReload = false {
    didSet { if needsReload { needsReload = false; reloadToken &+= 1 } }
  }

  private var foldKey: String { "outlineFolds.\(scope.storageKey)" }

  init(scope: ListScope) {
    self.scope = scope
    folded = Set(UserDefaults.standard.stringArray(forKey: "outlineFolds.\(scope.storageKey)") ?? [])
    hidesCompleted = UserDefaults.standard.bool(forKey: "outlineHidesCompleted")
  }

  /// Reads the scope and draws it, both off the main actor.
  func load(_ model: WorkspaceModel) async {
    generation &+= 1
    let mine = generation
    let store = model.store
    let workspaceID = model.workspace.id
    let structure = model.structure
    let scope = scope
    let hides = hidesCompleted
    let folded = folded
    let result = await Task.detached(priority: .userInitiated) { () -> (OutlineBase, [OutlineRow])? in
      guard let base = try? OutlineBase.load(
        store: store, workspaceID: workspaceID, scope: scope, structure: structure, hidesCompleted: hides)
      else { return nil }
      return (base, base.rows(folded: folded))
    }.value
    guard mine == generation, let (base, rows) = result else { return }
    self.base = base
    if self.rows != rows { self.rows = rows }
    isLoaded = true
  }

  /// Redraws after a fold changes, from the rows already read.
  private func reflow() {
    generation &+= 1
    let mine = generation
    let base = base
    let folded = folded
    Task { @MainActor [weak self] in
      let rows = await Task.detached(priority: .userInitiated) { base.rows(folded: folded) }.value
      guard let self, mine == self.generation else { return }
      if self.rows != rows { self.rows = rows }
    }
  }

  // MARK: - Folding

  func toggleFold(_ id: String) {
    if folded.contains(id) { folded.remove(id) } else { folded.insert(id) }
  }

  func setAllFolded(_ fold: Bool) {
    folded = fold ? TaskOutlineFolding.parentIDs(base.items) : []
  }

  /// A reveal from elsewhere: completed tasks shown and the named ancestors
  /// unfolded, written through to the defaults as any fold is.
  func apply(_ request: RevealRequest) {
    hidesCompleted = false
    if !folded.isDisjoint(with: request.ancestorIDs) { folded.subtract(request.ancestorIDs) }
  }

  /// Opens every folded ancestor of a row so it can be seen.
  func reveal(_ id: String) {
    var ancestors: Set<String> = []
    var current = TaskOutlineFolding.parentID(of: id, in: base.items)
    while let parent = current {
      ancestors.insert(parent)
      current = TaskOutlineFolding.parentID(of: parent, in: base.items)
    }
    if !folded.isDisjoint(with: ancestors) { folded.subtract(ancestors) }
  }

  // MARK: - Composer

  /// Opens the composer for a sibling below `row` — or at the end of the
  /// scope when there is no row.
  func composeSibling(after row: OutlineRow?) {
    guard let row else {
      composer = OutlineComposer(parentID: base.rootParentID, depth: 0, anchorRowID: rows.last?.id)
      return
    }
    composer = OutlineComposer(
      parentID: row.parentID, adjacentID: row.id, depth: row.depth, anchorRowID: lastVisibleRow(inBranchOf: row.id))
  }

  func composeAbove(_ row: OutlineRow) {
    let index = rows.firstIndex { $0.id == row.id } ?? 0
    composer = OutlineComposer(
      parentID: row.parentID, adjacentID: row.id, above: true, depth: row.depth,
      anchorRowID: index > 0 ? rows[index - 1].id : nil)
  }

  /// Opens the composer as the last child of `row`, unfolding it first.
  func composeChild(of row: OutlineRow) {
    if folded.contains(row.id) { folded.remove(row.id) }
    let lastChild = TaskOutlineFolding.descendantIDs(of: row.id, in: base.items).last
    composer = OutlineComposer(
      parentID: row.id, adjacentID: nil, depth: row.depth + 1, anchorRowID: lastChild ?? row.id)
  }

  /// The last row drawn beneath `id`, or `id` itself.
  private func lastVisibleRow(inBranchOf id: String) -> String {
    guard let index = rows.firstIndex(where: { $0.id == id }) else { return id }
    var end = index
    while end + 1 < rows.count, rows[end + 1].depth > rows[index].depth { end += 1 }
    return rows[end].id
  }

  /// Files what the composer holds and keeps it open beneath the new task,
  /// Checkvist-style, so a run of tasks can be typed one after another.
  @discardableResult
  func commitComposer(_ model: WorkspaceModel) -> WorkspaceTask? {
    guard var composer, let listID = listID(for: composer) else { self.composer = nil; return nil }
    let text = composer.text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !text.isEmpty else { self.composer = nil; return nil }
    let created = model.createTask(
      text, listID: listID, parentTaskID: composer.parentID, adjacentTo: composer.adjacentID, above: composer.above)
    if let created {
      composer.adjacentID = created.id
      composer.above = false
      composer.anchorRowID = created.id
      composer.text = ""
      self.composer = composer
    }
    return created
  }

  private func listID(for composer: OutlineComposer) -> String? {
    if let listID = scope.listID { return listID }
    if let adjacent = composer.adjacentID, let row = rows.first(where: { $0.id == adjacent }) { return row.listID }
    return nil
  }

  // MARK: - Renaming

  func beginRenaming(_ row: OutlineRow) {
    editingText = row.title
    editingTaskID = row.id
  }

  func commitRename(_ model: WorkspaceModel) {
    guard let id = editingTaskID else { return }
    model.rename(id, to: editingText)
    editingTaskID = nil
  }

  // MARK: - Moving

  /// Applies a drag. One or two store calls, each its own undo step.
  func move(from source: IndexSet, to destination: Int, model: WorkspaceModel) {
    guard scope.isSingleTree, let index = source.first,
      let drop = OutlineDropPlanner.plan(rows: rows, source: index, destination: destination),
      let listID = scope.listID
    else { return }
    let id = rows[index].id
    switch drop {
    case .before(let target):
      model.place(id, before: target)
    case .toEnd:
      model.move(id, by: Int.max / 2)
    case .reparent(let parentID, let before):
      let parent = parentID ?? base.rootParentID
      model.perform { store in
        try store.moveTask(id: id, toListId: listID, parentTaskId: parent)
      }
      if let before { model.place(id, before: before) }
    }
  }
}
