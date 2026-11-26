import Foundation
import TaktCore
import TaktWorkspace
import XCTest

/// The in-memory shapes the desktop now builds from one read of a list have
/// to be exactly what the store's per-shape queries answered, or the refresh
/// that stopped asking the store four times would be drawing a different tree.
final class WorkspaceListTreeTests: XCTestCase {
  private var directory: URL!
  private var store: WorkspaceStore!
  private var workspaceID: String!
  private var list: TaskList!
  private var other: TaskList!

  override func setUpWithError() throws {
    directory = FileManager.default.temporaryDirectory.appendingPathComponent("ListTree-\(UUID().uuidString)")
    store = try WorkspaceStore(databaseURL: directory.appendingPathComponent("tasks.sqlite"))
    workspaceID = try store.bootstrapIfNeeded().id
    list = try store.createList(workspaceId: workspaceID, name: "Work")
    other = try store.createList(workspaceId: workspaceID, name: "Home")
  }

  override func tearDownWithError() throws {
    store = nil
    try? FileManager.default.removeItem(at: directory)
  }

  /// A tree with every awkward case in it: nesting, a nested list, an
  /// archived nested list with children, a finished task and a closed list.
  private func seed() throws -> (project: WorkspaceTask, nested: WorkspaceTask, archived: WorkspaceTask) {
    let project = try store.createTask(listId: list.id, title: "Project")
    _ = try store.createTask(listId: list.id, title: "Step one", parentTaskId: project.id)
    let done = try store.createTask(listId: list.id, title: "Step two", parentTaskId: project.id)
    try store.setStatus(.completed, for: done.id)
    let nested = try store.createTask(listId: list.id, title: "Reading", kind: .list)
    let inner = try store.createTask(listId: list.id, title: "Papers", parentTaskId: nested.id, kind: .list)
    _ = try store.createTask(listId: list.id, title: "Paper A", parentTaskId: inner.id)
    let archived = try store.createTask(listId: list.id, title: "Old", kind: .list)
    _ = try store.createTask(listId: list.id, title: "Hidden", parentTaskId: archived.id)
    try store.setNestedListArchived(true, id: archived.id)
    let closed = try store.createTask(listId: list.id, title: "Closed", kind: .list)
    _ = try store.createTask(listId: list.id, title: "Inside closed", parentTaskId: closed.id)
    try store.setStatus(.completed, for: closed.id)
    _ = try store.createTask(listId: other.id, title: "Groceries")
    return (project, nested, archived)
  }

  func testTreeShapesMatchTheStoresOwnQueries() throws {
    let seeded = try seed()
    let tree = try store.listTree(in: list.id)
    for parent in [nil, seeded.project.id, seeded.nested.id, seeded.archived.id] {
      XCTAssertEqual(tree.outline(under: parent), try store.outline(in: list.id, parentTaskId: parent))
      XCTAssertEqual(tree.visibleOutline(under: parent), try store.visibleOutline(in: list.id, parentTaskId: parent))
      XCTAssertEqual(tree.children(of: parent), try store.tasks(in: list.id, parentTaskId: parent))
    }
    XCTAssertEqual(
      tree.visibleRootParentTaskID(registeredRootId: list.visibleRootTaskId),
      try store.visibleRootParentTaskID(for: list))
  }

  func testActionableTasksMatchAcrossLists() throws {
    _ = try seed()
    let lists = try store.lists(in: workspaceID)
    let trees = try store.listTrees(in: lists.map(\.id))
    let shaped = lists.flatMap { trees[$0.id]?.actionableTasks(visibleRootTaskId: $0.visibleRootTaskId) ?? [] }
    XCTAssertEqual(shaped, try store.actionableTasks(in: workspaceID))
    XCTAssertFalse(shaped.contains { $0.title == "Hidden" || $0.title == "Inside closed" || $0.isList })
  }

  func testVisibleRootIsOnlyTheWrapperWhileItIsTheOnlyRoot() throws {
    let wrapper = try store.createTask(listId: list.id, title: "Wrapper")
    let child = try store.createTask(listId: list.id, title: "Child", parentTaskId: wrapper.id)
    let tree = try store.listTree(in: list.id)
    XCTAssertEqual(tree.visibleRootParentTaskID(registeredRootId: wrapper.id), wrapper.id)
    XCTAssertNil(tree.visibleRootParentTaskID(registeredRootId: child.id))
    XCTAssertNil(tree.visibleRootParentTaskID(registeredRootId: nil))
    _ = try store.createTask(listId: list.id, title: "Second root")
    XCTAssertNil(try store.listTree(in: list.id).visibleRootParentTaskID(registeredRootId: wrapper.id))
  }

  func testSidebarIndexCountsEveryTaskAndNestsListsByListDepth() throws {
    let seeded = try seed()
    let lists = try store.lists(in: workspaceID)
    let index = WorkspaceSidebarIndex(lists: lists, trees: try store.listTrees(in: lists.map(\.id)))

    XCTAssertEqual(index.taskCounts[list.id], try store.outline(in: list.id).count)
    XCTAssertEqual(index.taskCounts[other.id], 1)
    // Reading and its nested Papers; Closed is a list too, just finished.
    XCTAssertEqual(index.nestedLists.map(\.task.title), ["Reading", "Papers", "Closed"])
    XCTAssertEqual(index.nestedLists.map(\.depth), [0, 1, 0])
    XCTAssertEqual(index.archivedNestedLists.map(\.id), [seeded.archived.id])
  }

  /// The core walks the sidebar itself so that only its answer crosses; it
  /// has to be the answer the trees give.
  func testCoreSidebarIndexMatchesTheTrees() throws {
    _ = try seed()
    let wrapper = try store.createTask(listId: other.id, title: "Wrapper", kind: .list)
    _ = try store.createTask(listId: other.id, title: "Inner", parentTaskId: wrapper.id, kind: .list)
    let lists = try store.lists(in: workspaceID)
    let fromTrees = WorkspaceSidebarIndex(lists: lists, trees: try store.listTrees(in: lists.map(\.id)))
    XCTAssertEqual(try store.sidebarIndex(lists: lists), fromTrees)
    XCTAssertEqual(try store.sidebarIndex(lists: []), WorkspaceSidebarIndex(nestedLists: [], archivedNestedLists: [], taskCounts: [:]))
  }

  /// A card's tree runs to every level, depths counted from the card, and a
  /// task inside it has a tree of its own — the case of a subtask surfaced as
  /// a card in another column, or a card on a board scoped into a parent.
  func testBoardTreesReachEveryLevelBeneathEveryCardAndInsideIt() throws {
    let project = try store.createTask(listId: list.id, title: "Project")
    let step = try store.createTask(listId: list.id, title: "Step", parentTaskId: project.id)
    let part = try store.createTask(listId: list.id, title: "Part", parentTaskId: step.id)
    let detail = try store.createTask(listId: list.id, title: "Detail", parentTaskId: part.id)
    try store.setStatus(.completed, for: detail.id)
    let sibling = try store.createTask(listId: list.id, title: "Sibling", parentTaskId: project.id)
    let archived = try store.createTask(listId: list.id, title: "Old", parentTaskId: project.id, kind: .list)
    _ = try store.createTask(listId: list.id, title: "Hidden", parentTaskId: archived.id)
    try store.setNestedListArchived(true, id: archived.id)
    let elsewhere = try store.createTask(listId: list.id, title: "Elsewhere")
    let tree = try store.listTree(in: list.id)

    let board = WorkspaceBoardTrees(cardIDs: [project.id], trees: [tree])
    let beneath = try XCTUnwrap(board.descendants[project.id])
    XCTAssertEqual(beneath.map(\.task.title), ["Step", "Part", "Detail", "Sibling"])
    XCTAssertEqual(beneath.map(\.depth), [0, 1, 2, 0])
    // Finished work stays in the tree, drawn ticked rather than dropped.
    XCTAssertEqual(beneath[2].task.status, .completed)
    XCTAssertEqual(board.descendants[step.id]?.map(\.task.title), ["Part", "Detail"])
    XCTAssertEqual(board.descendants[step.id]?.map(\.depth), [0, 1])
    XCTAssertEqual(board.descendants[detail.id], [])
    XCTAssertEqual(board.descendants[sibling.id], [])
    XCTAssertNil(board.descendants[elsewhere.id])
    XCTAssertEqual(board.parents[detail.id]?.id, part.id)
    XCTAssertNil(board.parents[project.id])

    // A board scoped into Project has Step as a card: its tree is complete,
    // and nothing above it is keyed.
    let scoped = WorkspaceBoardTrees(cardIDs: [step.id, sibling.id], trees: [tree])
    XCTAssertEqual(scoped.descendants[step.id]?.map(\.depth), [0, 1])
    XCTAssertNil(scoped.descendants[project.id])
    XCTAssertEqual(Set(scoped.descendants.keys), [step.id, part.id, detail.id, sibling.id])
  }

  func testTasksByIDReadsOnlyWhatExists() throws {
    let task = try store.createTask(listId: list.id, title: "One")
    let found = try store.tasks(ids: [task.id, "missing", task.id])
    XCTAssertEqual(found.keys.sorted(), [task.id])
    XCTAssertTrue(try store.tasks(ids: []).isEmpty)
  }

  func testNextUpSnapshotAgreesWithTheIndividualReads() throws {
    let task = try store.createTask(listId: list.id, title: "Write report")
    _ = try store.createTask(listId: other.id, title: "Groceries")
    let now = Date.now
    let snapshot = try store.nextUpSnapshot(
      workspaceId: workspaceID, context: FocusContext(), runningID: nil, now: now)
    let candidates = try store.nextUpCandidates(now: now)
    let ranking = NextUpSelector.evaluate(candidates, now: now, context: FocusContext())

    XCTAssertEqual(snapshot.ranking.ranked, ranking.ranked)
    XCTAssertEqual(snapshot.todayPlan, DayPlanSelector.plan(candidates: candidates, now: now))
    XCTAssertEqual(snapshot.loggedSeconds, try store.loggedWorkTotals())
    XCTAssertEqual(snapshot.planning, try store.taskPlanningValues())
    XCTAssertEqual(snapshot.conditions, try store.conditions(in: workspaceID))
    XCTAssertEqual(snapshot.workProgress, try store.workProgress(now: now))
    // The day falls back to the ranking, so its rows travel with it.
    XCTAssertEqual(snapshot.dayTasks[task.id], try store.task(id: task.id))
  }

  /// A limited ladder is the full one's head, plus the day's tasks wherever
  /// they rank, in ladder order; the day and the blocked list are whole.
  func testALimitedLadderKeepsItsHeadAndTheDay() throws {
    // Five tasks due tomorrow with more work than there is time for, which
    // ranks them ahead of Today without putting them in the day; the sixth
    // is in Today, so it ranks last.
    let now = Date.now
    var created: [WorkspaceTask] = []
    for index in 0..<5 {
      created.append(try store.createTask(
        listId: list.id, title: "Task \(index)", dueAt: now.addingTimeInterval(86_400), estimateSeconds: 3 * 86_400))
    }
    created.append(try store.createTask(
      listId: list.id, title: "Planned", kanbanColumn: NextUpSelector.todayColumnID))
    let full = try store.nextUpSnapshot(workspaceId: workspaceID, context: FocusContext(), runningID: nil, now: now)
    let short = try store.nextUpSnapshot(
      workspaceId: workspaceID, context: FocusContext(), runningID: nil, now: now, ladderLimit: 2)

    let inDay = Set(full.todayPlan.map(\.id))
    XCTAssertEqual(
      short.ranking.ranked.map(\.id),
      full.ranking.ranked.enumerated().filter { $0.offset < 2 || inDay.contains($0.element.id) }.map(\.element.id))
    XCTAssertEqual(full.ranking.ranked.last?.id, created[5].id)
    XCTAssertEqual(short.ranking.ranked.count, 3)
    XCTAssertEqual(short.ranking.ranked.last?.id, created[5].id)
    XCTAssertEqual(short.todayPlan, full.todayPlan)
    XCTAssertEqual(short.ranking.blocked, full.ranking.blocked)
    XCTAssertEqual(short.ranking.nextEvaluationAt, full.ranking.nextEvaluationAt)
  }
}
