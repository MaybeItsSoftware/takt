import Foundation
import TestSQLite
import TaktCore
import TaktWorkspace
import XCTest

final class WorkspaceNestedListTests: XCTestCase {
  private var directory: URL!
  private var store: WorkspaceStore!
  private var workspaceID: String!
  private var list: TaskList!

  override func setUpWithError() throws {
    directory = FileManager.default.temporaryDirectory.appendingPathComponent("NestedLists-\(UUID().uuidString)")
    store = try WorkspaceStore(databaseURL: directory.appendingPathComponent("tasks.sqlite"))
    workspaceID = try store.bootstrapIfNeeded().id
    list = try store.createList(workspaceId: workspaceID, name: "Committees 2026–27")
  }

  override func tearDownWithError() throws {
    store = nil
    try? FileManager.default.removeItem(at: directory)
  }

  func testConversionPreservesIdentityHierarchyAndTaskDetailsAndIsUndoable() throws {
    let task = try store.createTask(listId: list.id, title: "Sailing")
    let child = try store.createTask(listId: list.id, title: "Book boats", parentTaskId: task.id)
    let due = Date(timeIntervalSince1970: 1_900_000_000)
    try store.updateTask(id: task.id, title: task.title, notes: "Notes", dueAt: due, estimateSeconds: 600)
    try store.setItemKind(.list, for: task.id)
    let converted = try XCTUnwrap(store.task(id: task.id))
    XCTAssertTrue(converted.isList)
    XCTAssertEqual(converted.notes, "Notes")
    XCTAssertEqual(converted.dueAt, due)
    XCTAssertEqual(converted.estimateSeconds, 600)
    XCTAssertEqual(try store.task(id: child.id)?.parentTaskId, task.id)
    XCTAssertEqual(try store.undo(), "Convert to List")
    XCTAssertFalse(try XCTUnwrap(store.task(id: task.id)).isList)
    try store.redo()
    XCTAssertTrue(try XCTUnwrap(store.task(id: task.id)).isList)
  }

  func testPromotionIsPersistedWithoutMovingTheNestedList() throws {
    let parent = try store.createTask(listId: list.id, title: "Societies", kind: .list)
    let nested = try store.createTask(listId: list.id, title: "Sailing", parentTaskId: parent.id, kind: .list)
    try store.setNestedListPromoted(true, id: nested.id)
    let reopened = try WorkspaceStore(databaseURL: directory.appendingPathComponent("tasks.sqlite"))
    let saved = try XCTUnwrap(reopened.task(id: nested.id))
    XCTAssertEqual(saved.isPromoted, true)
    XCTAssertEqual(saved.parentTaskId, parent.id)
    XCTAssertEqual(saved.listId, list.id)
    try reopened.undo()
    XCTAssertNotEqual(try reopened.task(id: nested.id)?.isPromoted, true)
  }

  func testEverythingAndFocusExcludeContainersButIncludeNestedTasks() throws {
    let parent = try store.createTask(listId: list.id, title: "Sailing", kind: .list)
    let empty = try store.createTask(listId: list.id, title: "Ideas", kind: .list)
    let child = try store.createTask(listId: list.id, title: "Book boats", parentTaskId: parent.id)
    XCTAssertEqual(try store.actionableTasks(in: workspaceID).map(\.id), [child.id])
    XCTAssertEqual(try store.nextUpCandidates().map(\.id), [child.id])
    XCTAssertThrowsError(try store.startFocusSession(taskId: empty.id))
  }

  func testCompletingAContainerSuppressesItsDescendantsWithoutCompletingThem() throws {
    let parent = try store.createTask(listId: list.id, title: "Sailing", kind: .list)
    let nested = try store.createTask(listId: list.id, title: "Trip", parentTaskId: parent.id, kind: .list)
    let child = try store.createTask(listId: list.id, title: "Book boats", parentTaskId: nested.id)
    try store.setStatus(.completed, for: parent.id)
    XCTAssertTrue(try store.actionableTasks(in: workspaceID).isEmpty)
    XCTAssertTrue(try store.nextUpCandidates().isEmpty)
    XCTAssertEqual(try store.task(id: child.id)?.status, .open)
    XCTAssertThrowsError(try store.startFocusSession(taskId: child.id))
    try store.undo()
    XCTAssertEqual(try store.actionableTasks(in: workspaceID).map(\.id), [child.id])
  }

  func testArchivingNestedListCanBeRestoredAndUndone() throws {
    let parent = try store.createTask(listId: list.id, title: "Sailing", kind: .list)
    let child = try store.createTask(listId: list.id, title: "Book boats", parentTaskId: parent.id)
    try store.setNestedListArchived(true, id: parent.id)
    XCTAssertTrue(try store.actionableTasks(in: workspaceID).isEmpty)
    XCTAssertTrue(try store.nextUpCandidates().isEmpty)
    try store.setNestedListArchived(false, id: parent.id)
    XCTAssertEqual(try store.actionableTasks(in: workspaceID).map(\.id), [child.id])
    try store.undo()
    XCTAssertNotNil(try store.task(id: parent.id)?.archivedAt)
  }

  func testAPrioritisedListLeadsItsColumnInNextUpAndCompletingItDropsTheRank() throws {
    let other = try store.createList(workspaceId: workspaceID, name: "Volunteering")
    let promo = try store.createTask(
      listId: other.id, title: "Promo", startAt: Date.now.addingTimeInterval(-2 * 86_400))
    let exam = try store.createTask(listId: list.id, title: "Revise")
    try store.setListPriority(1, id: list.id)
    XCTAssertEqual(try store.lists(in: workspaceID).first { $0.id == list.id }?.priorityRank, 1)
    XCTAssertEqual(try store.nextUpCandidates().first { $0.id == exam.id }?.listPriority, 1)
    let ranked = try store.nextUpSnapshot(workspaceId: workspaceID, context: FocusContext(), runningID: nil)
      .ranking.ranked.map(\.id).filter { [exam.id, promo.id].contains($0) }
    // The promo started two days ago, which used to put it first.
    XCTAssertEqual(ranked, [exam.id, promo.id])
    try store.setListCompleted(true, id: list.id)
    XCTAssertNil(try store.lists(in: workspaceID).first { $0.id == list.id }?.priorityRank)
  }

  func testTopLevelListCanEndAndReopenWithoutLosingTasks() throws {
    let task = try store.createTask(listId: list.id, title: "Annual handover")
    try store.setListCompleted(true, id: list.id)
    XCTAssertNotNil(try store.lists(in: workspaceID).first(where: { $0.id == list.id })?.completedAt)
    XCTAssertTrue(try store.actionableTasks(in: workspaceID).isEmpty)
    XCTAssertTrue(try store.nextUpCandidates().isEmpty)
    XCTAssertEqual(try store.task(id: task.id)?.status, .open)
    try store.setListCompleted(false, id: list.id)
    XCTAssertEqual(try store.actionableTasks(in: workspaceID).map(\.id), [task.id])
    let inbox = try XCTUnwrap(store.inbox(in: workspaceID))
    XCTAssertThrowsError(try store.setListCompleted(true, id: inbox.id))
  }

  func testConvertingBackToTaskClearsListOnlyFlagsAndRetainsChildren() throws {
    let parent = try store.createTask(listId: list.id, title: "Trip", kind: .list)
    let child = try store.createTask(listId: list.id, title: "Hotel", parentTaskId: parent.id)
    try store.setNestedListPromoted(true, id: parent.id)
    try store.setNestedListArchived(true, id: parent.id)
    try store.setItemKind(.task, for: parent.id)
    let saved = try XCTUnwrap(store.task(id: parent.id))
    XCTAssertFalse(saved.isList)
    XCTAssertNil(saved.isPromoted)
    XCTAssertNil(saved.archivedAt)
    XCTAssertEqual(try store.task(id: child.id)?.parentTaskId, parent.id)
  }

  func testTasksWithChildrenAreNotAutomaticallyLists() throws {
    let parent = try store.createTask(listId: list.id, title: "Pack bag")
    _ = try store.createTask(listId: list.id, title: "Find passport", parentTaskId: parent.id)
    XCTAssertFalse(try XCTUnwrap(store.task(id: parent.id)).isList)
    XCTAssertThrowsError(try store.setNestedListPromoted(true, id: parent.id))
  }

  func testStandaloneListConvertsToAnInboxTaskAndUndoRestoresTheList() throws {
    let task = try store.createTask(listId: list.id, title: "Sailing", kind: .list)
    let child = try store.createTask(listId: list.id, title: "Boats", parentTaskId: task.id)
    try store.setNestedListPromoted(true, id: task.id)
    let root = try store.convertListToTask(id: list.id)
    let inbox = try XCTUnwrap(store.inbox(in: workspaceID))
    XCTAssertEqual(root.listId, inbox.id)
    XCTAssertFalse(root.isList)
    XCTAssertEqual(root.title, list.name)
    XCTAssertEqual(try store.task(id: task.id)?.parentTaskId, root.id)
    XCTAssertEqual(try store.task(id: child.id)?.parentTaskId, task.id)
    XCTAssertEqual(try store.task(id: child.id)?.listId, inbox.id)
    XCTAssertEqual(try store.task(id: task.id)?.isPromoted, true)
    XCTAssertFalse(try store.lists(in: workspaceID).contains(where: { $0.id == list.id }))
    try store.undo()
    XCTAssertTrue(try store.lists(in: workspaceID).contains(where: { $0.id == list.id }))
    XCTAssertEqual(try store.task(id: child.id)?.listId, list.id)
    XCTAssertNil(try store.task(id: root.id))
    try store.redo()
    XCTAssertEqual(try store.task(id: child.id)?.listId, inbox.id)
  }

  func testStandaloneConversionReusesAnImportedWrapperAndPreservesMetadata() throws {
    let imported = try XCTUnwrap(store.importTasks(workspaceId: workspaceID, listName: "Study", sourceSystem: "checkvist", seeds: [
      .init(sourceId: "root", parentSourceId: nil, title: "Study", status: .open, sortOrder: 0),
      .init(sourceId: "child", parentSourceId: "root", title: "Read", status: .open, sortOrder: 0)
    ]))
    let wrapperID = try XCTUnwrap(imported.list.visibleRootTaskId)
    try store.updateTask(id: wrapperID, title: "Study", notes: "Keep these notes", dueAt: nil, estimateSeconds: 600)
    try store.setKanbanColumn("today", for: wrapperID)
    let root = try store.convertListToTask(id: imported.list.id)
    XCTAssertEqual(root.id, wrapperID)
    XCTAssertEqual(root.notes, "Keep these notes")
    XCTAssertEqual(root.sourceId, "root")
    XCTAssertEqual(try store.kanbanColumn(for: wrapperID), "today")
    XCTAssertEqual(try store.tasks(in: root.listId, parentTaskId: root.id).count, 1)
    try store.undo()
    XCTAssertEqual(try store.visibleRootParentTaskID(for: imported.list), wrapperID)
  }

  func testArchivedContainersAndTheirDescendantsAreHiddenFromVisibleOutline() throws {
    let parent = try store.createTask(listId: list.id, title: "Old year", kind: .list)
    _ = try store.createTask(listId: list.id, title: "Archived work", parentTaskId: parent.id)
    let sibling = try store.createTask(listId: list.id, title: "New year", kind: .list)
    try store.setNestedListArchived(true, id: parent.id)
    XCTAssertEqual(try store.visibleOutline(in: list.id).map(\.id), [sibling.id])
    XCTAssertEqual(try store.outline(in: list.id).count, 3)
  }

  func testMovingAPromotedListKeepsThePromotionAndCannotCreateACycle() throws {
    let parent = try store.createTask(listId: list.id, title: "Societies", kind: .list)
    let nested = try store.createTask(listId: list.id, title: "Sailing", parentTaskId: parent.id, kind: .list)
    try store.setNestedListPromoted(true, id: nested.id)
    XCTAssertThrowsError(try store.moveTask(id: parent.id, toListId: list.id, parentTaskId: nested.id))
    let destination = try store.createList(workspaceId: workspaceID, name: "Next year")
    try store.moveTask(id: nested.id, toListId: destination.id)
    XCTAssertEqual(try store.task(id: nested.id)?.isPromoted, true)
    XCTAssertEqual(try store.task(id: nested.id)?.listId, destination.id)
    try store.undo()
    XCTAssertEqual(try store.task(id: nested.id)?.parentTaskId, parent.id)
  }

  func testConvertingTheCurrentFocusTaskIntoAContainerIsRejected() throws {
    let task = try store.createTask(listId: list.id, title: "Current work")
    _ = try store.startFocusSession(taskId: task.id)
    XCTAssertThrowsError(try store.setItemKind(.list, for: task.id))
    XCTAssertFalse(try XCTUnwrap(store.task(id: task.id)).isList)
  }

  func testStandaloneListCanBeDroppedIntoAnotherListAsOneUndoableSubtree() throws {
    let parent = try store.createList(workspaceId: workspaceID, name: "University")
    let task = try store.createTask(listId: list.id, title: "Sailing", kind: .list)
    let child = try store.createTask(listId: list.id, title: "Boats", parentTaskId: task.id)
    let nested = try store.nestList(id: list.id, inListId: parent.id)
    XCTAssertTrue(nested.isList)
    XCTAssertEqual(nested.title, list.name)
    XCTAssertNil(nested.parentTaskId)
    XCTAssertEqual(try store.task(id: task.id)?.parentTaskId, nested.id)
    XCTAssertEqual(try store.task(id: child.id)?.parentTaskId, task.id)
    XCTAssertEqual(try store.task(id: child.id)?.listId, parent.id)
    XCTAssertFalse(try store.lists(in: workspaceID).contains(where: { $0.id == list.id }))
    XCTAssertEqual(try store.undo(), "Move List into List")
    XCTAssertEqual(try store.task(id: child.id)?.listId, list.id)
    XCTAssertNil(try store.task(id: nested.id))
    try store.redo()
    XCTAssertEqual(try store.task(id: child.id)?.listId, parent.id)
  }

  func testStandaloneListCanBeDroppedIntoANestedListAndSelfDropsAreRejected() throws {
    let parent = try store.createList(workspaceId: workspaceID, name: "University")
    let nested = try store.createTask(listId: parent.id, title: "Societies", kind: .list)
    let root = try store.nestList(id: list.id, inListId: parent.id, parentTaskId: nested.id)
    XCTAssertEqual(root.parentTaskId, nested.id)
    XCTAssertThrowsError(try store.nestList(id: parent.id, inListId: parent.id, parentTaskId: root.id))
    let inbox = try XCTUnwrap(store.inbox(in: workspaceID))
    XCTAssertThrowsError(try store.nestList(id: inbox.id, inListId: parent.id))
  }

  func testImportedStandaloneListReusesWrapperAndDropsBesideDestinationVisibleRoots() throws {
    let source = try XCTUnwrap(store.importTasks(workspaceId: workspaceID, listName: "Study", sourceSystem: "checkvist", seeds: [
      .init(sourceId: "sourceRoot", parentSourceId: nil, title: "Study", status: .open, sortOrder: 0),
      .init(sourceId: "sourceChild", parentSourceId: "sourceRoot", title: "Read", status: .open, sortOrder: 0)
    ])).list
    let destination = try XCTUnwrap(store.importTasks(workspaceId: workspaceID, listName: "University", sourceSystem: "checkvist", seeds: [
      .init(sourceId: "destRoot", parentSourceId: nil, title: "University", status: .open, sortOrder: 0),
      .init(sourceId: "destChild", parentSourceId: "destRoot", title: "Admin", status: .open, sortOrder: 0)
    ])).list
    let nested = try store.nestList(id: source.id, inListId: destination.id)
    XCTAssertEqual(nested.id, source.visibleRootTaskId)
    XCTAssertEqual(nested.parentTaskId, destination.visibleRootTaskId)
    XCTAssertTrue(nested.isList)
    XCTAssertEqual(try store.tasks(in: destination.id, parentTaskId: destination.visibleRootTaskId).count, 2)
    try store.undo()
    XCTAssertEqual(try store.visibleRootParentTaskID(for: source), source.visibleRootTaskId)
  }

  func testTaskCanMoveBetweenNestedListsWithinTheSameStandaloneList() throws {
    let source = try store.createTask(listId: list.id, title: "Sailing", kind: .list)
    let destination = try store.createTask(listId: list.id, title: "Hiking", kind: .list)
    let task = try store.createTask(listId: list.id, title: "Book transport", parentTaskId: source.id)
    let child = try store.createTask(listId: list.id, title: "Get quote", parentTaskId: task.id)
    try store.moveTask(id: task.id, toListId: list.id, parentTaskId: destination.id)
    XCTAssertEqual(try store.task(id: task.id)?.parentTaskId, destination.id)
    XCTAssertEqual(try store.task(id: child.id)?.parentTaskId, task.id)
    try store.undo()
    XCTAssertEqual(try store.task(id: task.id)?.parentTaskId, source.id)
  }

  func testNestedListCanBecomeATopLevelListAndUndoRestoresItsParent() throws {
    let parent = try store.createTask(listId: list.id, title: "Societies", kind: .list)
    let nested = try store.createTask(listId: list.id, title: "Sailing", parentTaskId: parent.id, kind: .list)
    let child = try store.createTask(listId: list.id, title: "Boats", parentTaskId: nested.id)
    let standalone = try store.moveTaskToFolder(id: nested.id, folderId: nil)
    XCTAssertNil(standalone.folderId)
    XCTAssertEqual(standalone.visibleRootTaskId, nested.id)
    XCTAssertEqual(standalone.name, nested.title)
    XCTAssertNil(try store.task(id: nested.id)?.parentTaskId)
    XCTAssertEqual(try store.task(id: child.id)?.listId, standalone.id)
    XCTAssertEqual(try store.task(id: child.id)?.parentTaskId, nested.id)
    XCTAssertEqual(try store.visibleRootTasks(in: workspaceID).filter { $0.listId == standalone.id }.map(\.id), [child.id])
    XCTAssertEqual(try store.undo(), "Move Item to Top Level")
    XCTAssertEqual(try store.task(id: nested.id)?.parentTaskId, parent.id)
    XCTAssertEqual(try store.task(id: child.id)?.listId, list.id)
    try store.redo()
    XCTAssertEqual(try store.task(id: child.id)?.listId, standalone.id)
  }

  func testNestedListCanBeMovedIntoAFolderWithoutLosingItsContents() throws {
    let folder = try store.createFolder(workspaceId: workspaceID, name: "University")
    let parent = try store.createTask(listId: list.id, title: "Societies", kind: .list)
    let nested = try store.createTask(listId: list.id, title: "Sailing", parentTaskId: parent.id, kind: .list)
    let child = try store.createTask(listId: list.id, title: "Boats", parentTaskId: nested.id)
    try store.setNestedListPromoted(true, id: nested.id)
    let standalone = try store.moveTaskToFolder(id: nested.id, folderId: folder.id)
    XCTAssertEqual(standalone.folderId, folder.id)
    XCTAssertEqual(standalone.name, nested.title)
    XCTAssertEqual(standalone.visibleRootTaskId, nested.id)
    XCTAssertNil(try store.task(id: nested.id)?.parentTaskId)
    XCTAssertEqual(try store.task(id: child.id)?.parentTaskId, nested.id)
    XCTAssertEqual(try store.task(id: child.id)?.listId, standalone.id)
    XCTAssertEqual(try store.visibleRootTasks(in: workspaceID).filter { $0.listId == standalone.id }.map(\.id), [child.id])
    try store.undo()
    XCTAssertEqual(try store.task(id: nested.id)?.parentTaskId, parent.id)
    XCTAssertEqual(try store.task(id: nested.id)?.isPromoted, true)
    XCTAssertEqual(try store.task(id: child.id)?.listId, list.id)
    try store.redo()
    XCTAssertEqual(try store.task(id: child.id)?.listId, standalone.id)
  }

  func testTasksDroppedOnAFolderBecomeSeparateTopLevelLists() throws {
    let folder = try store.createFolder(workspaceId: workspaceID, name: "University")
    let task = try store.createTask(listId: list.id, title: "Book boats")
    let child = try store.createTask(listId: list.id, title: "Get a quote", parentTaskId: task.id)
    let other = try store.createTask(listId: list.id, title: "Book room")
    let first = try store.moveTaskToFolder(id: task.id, folderId: folder.id)
    let second = try store.moveTaskToFolder(id: other.id, folderId: folder.id)
    XCTAssertNotEqual(first.id, second.id)
    XCTAssertEqual(first.name, "Book boats")
    XCTAssertEqual(second.name, "Book room")
    XCTAssertEqual(first.visibleRootTaskId, task.id)
    XCTAssertEqual(second.visibleRootTaskId, other.id)
    XCTAssertEqual(first.folderId, folder.id)
    XCTAssertTrue(try XCTUnwrap(store.task(id: task.id)).isList)
    XCTAssertNil(try store.task(id: task.id)?.parentTaskId)
    XCTAssertEqual(try store.visibleOutline(in: first.id, parentTaskId: first.visibleRootTaskId).map(\.task.id), [child.id])
    XCTAssertEqual(try store.task(id: child.id)?.parentTaskId, task.id)
    XCTAssertEqual(try store.task(id: child.id)?.listId, first.id)
    try store.undo()
    XCTAssertEqual(try store.task(id: other.id)?.listId, list.id)
    XCTAssertFalse(try XCTUnwrap(store.task(id: other.id)).isList)
    try store.undo()
    XCTAssertEqual(try store.task(id: child.id)?.listId, list.id)
    XCTAssertFalse(try XCTUnwrap(store.task(id: task.id)).isList)
    XCTAssertFalse(try store.lists(in: workspaceID).contains(where: { $0.id == first.id }))
  }

  func testStandaloneListCanMoveToAFolderWithoutChangingIdentity() throws {
    let folder = try store.createFolder(workspaceId: workspaceID, name: "University")
    let task = try store.createTask(listId: list.id, title: "Handover")
    try store.moveList(id: list.id, toFolderId: folder.id)
    XCTAssertEqual(try store.lists(in: workspaceID).first(where: { $0.id == list.id })?.folderId, folder.id)
    XCTAssertEqual(try store.task(id: task.id)?.listId, list.id)
    try store.undo()
    XCTAssertNil(try store.lists(in: workspaceID).first(where: { $0.id == list.id })?.folderId)
  }

  func testFolderMoveRejectsAnotherWorkspaceAndKeepsClosedContainerState() throws {
    let elsewhereID = UUID().uuidString
    try DatabaseQueue(path: directory.appendingPathComponent("tasks.sqlite").path).write { db in
      try db.execute(sql: "INSERT INTO workspaces (id, name, createdAt, updatedAt) VALUES (?, ?, ?, ?)",
        arguments: [elsewhereID, "Elsewhere", Date.now, Date.now])
    }
    let otherFolder = try store.createFolder(workspaceId: elsewhereID, name: "Other")
    let nested = try store.createTask(listId: list.id, title: "Last year", kind: .list)
    let child = try store.createTask(listId: list.id, title: "Handover", parentTaskId: nested.id)
    XCTAssertThrowsError(try store.moveTaskToFolder(id: nested.id, folderId: otherFolder.id))
    XCTAssertEqual(try store.task(id: child.id)?.listId, list.id)
    let folder = try store.createFolder(workspaceId: workspaceID, name: "University")
    try store.setStatus(.completed, for: nested.id)
    let standalone = try store.moveTaskToFolder(id: nested.id, folderId: folder.id)
    XCTAssertNotNil(standalone.completedAt)
    XCTAssertFalse(try store.actionableTasks(in: workspaceID).contains(where: { $0.id == child.id }))
    XCTAssertEqual(try store.task(id: child.id)?.status, .open)
  }

  func testEmptyNestedListMovedToFolderHasAValidVisibleRootAndEditableSettings() throws {
    let folder = try store.createFolder(workspaceId: workspaceID, name: "University")
    let nested = try store.createTask(listId: list.id, title: "Sailing", kind: .list)
    let standalone = try store.moveTaskToFolder(id: nested.id, folderId: folder.id)
    XCTAssertEqual(try store.visibleRootCandidates(in: standalone.id).map(\.id), [nested.id])
    try store.saveListSettings(id: standalone.id, name: "Sailing", colorHex: nil, folderId: folder.id,
      isArchived: false, visibleRootTaskId: nil)
    try store.saveListSettings(id: standalone.id, name: "Sailing plans", colorHex: "#abcdef", folderId: folder.id,
      isArchived: false, visibleRootTaskId: nested.id)
    XCTAssertEqual(try store.visibleRootParentTaskID(for: standalone), nested.id)
    XCTAssertTrue(try store.tasks(in: standalone.id, parentTaskId: nested.id).isEmpty)
  }

  /// ⌥← on a card at the top of a nested list's board moved it up a level,
  /// out of the list, and the board then showed the list itself as a card.
  func testOutdentStopsAtTheScopeItIsSeenIn() throws {
    let nested = try store.createTask(listId: list.id, title: "Shakespeare", kind: .list)
    let card = try store.createTask(listId: list.id, title: "Reimburse for posters", parentTaskId: nested.id)
    let subtask = try store.createTask(listId: list.id, title: "Reimburse for chocolate", parentTaskId: card.id)

    XCTAssertFalse(try store.outdentTask(id: card.id, within: nested.id))
    XCTAssertEqual(try store.task(id: card.id)?.parentTaskId, nested.id)

    // Below the top it still comes out a level, into the scope.
    XCTAssertTrue(try store.outdentTask(id: subtask.id, within: nested.id))
    XCTAssertEqual(try store.task(id: subtask.id)?.parentTaskId, nested.id)

    // Seen from the list itself, the nested list's card has a level above it.
    XCTAssertTrue(try store.outdentTask(id: card.id, within: nil))
    XCTAssertNil(try store.task(id: card.id)?.parentTaskId)
    XCTAssertFalse(try store.outdentTask(id: card.id, within: nil))
  }

  /// An imported list's wrapper root bounds its top level as an entered
  /// task does.
  func testOutdentStopsAtTheListsWrapperRoot() throws {
    let imported = try XCTUnwrap(store.importTasks(workspaceId: workspaceID, listName: "Study", sourceSystem: "checkvist", seeds: [
      .init(sourceId: "root", parentSourceId: nil, title: "Study", status: .open, sortOrder: 0),
      .init(sourceId: "child", parentSourceId: "root", title: "Read", status: .open, sortOrder: 0)
    ]))
    let wrapperID = try XCTUnwrap(imported.list.visibleRootTaskId)
    let child = try XCTUnwrap(store.tasks(in: imported.list.id, parentTaskId: wrapperID).first)
    XCTAssertFalse(try store.outdentTask(id: child.id, within: nil))
    XCTAssertEqual(try store.task(id: child.id)?.parentTaskId, wrapperID)
  }
}
