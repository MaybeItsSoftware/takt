import Foundation
import TaktWorkspace

/// Lists and folders: creating, renaming, archiving, completing, deleting,
/// moving and reordering — the Mac sidebar's commands.
@MainActor
extension WorkspaceModel {
  @discardableResult
  func createList(named name: String, inFolder folderID: String? = nil) -> TaskList? {
    performReturning { try $0.createList(workspaceId: workspace.id, name: name, folderId: folderID) }
  }

  @discardableResult
  func createFolder(named name: String, inFolder parentID: String? = nil) -> ListFolder? {
    performReturning { try $0.createFolder(workspaceId: workspace.id, name: name, parentFolderId: parentID) }
  }

  /// A list nested inside another list, as a task-backed list.
  @discardableResult
  func createNestedList(named name: String, inList listID: String, parentTaskID: String?) -> WorkspaceTask? {
    performReturning { try $0.createTask(listId: listID, title: name, parentTaskId: parentTaskID, kind: .list) }
  }

  func renameList(_ listID: String, to name: String) {
    perform { try $0.renameList(id: listID, name: name) }
  }

  func renameFolder(_ folderID: String, to name: String) {
    perform { try $0.updateFolder(id: folderID, name: name) }
  }

  func saveList(_ listID: String, name: String, colorHex: String?) {
    perform { try $0.updateList(id: listID, name: name, colorHex: colorHex) }
  }

  func setArchived(_ archived: Bool, list listID: String) {
    if perform({ try $0.setListArchived(archived, id: listID) }) {
      announce(archived ? "Archived" : "Restored")
      if archived, navigation.currentScope?.listID == listID { leaveScope(listID: listID) }
    }
  }

  /// Puts a list among the prioritised lists at `position`, 1 the most
  /// important, or takes it out with nil. Up next leads each board column
  /// with them, in order.
  func setPriority(_ position: Int?, list listID: String) {
    if perform({ try $0.setListPriority(position, id: listID) }) {
      announce(position == nil ? "No longer prioritised" : "Priority \(position ?? 0)")
    }
  }

  /// Prioritises a list, last among the priorities.
  func prioritise(list listID: String) {
    setPriority(structure.prioritisedLists.count + 1, list: listID)
  }

  /// Moves a prioritised list `offset` places up (negative) or down them.
  func movePriority(of listID: String, by offset: Int) {
    guard let place = structure.priority(of: listID) else { return }
    let target = min(max(place + offset, 1), structure.prioritisedLists.count)
    if target != place { setPriority(target, list: listID) }
  }

  func toggleCompleted(list listID: String) {
    guard let list = structure.list(listID) else { return }
    perform { try $0.setListCompleted(list.completedAt == nil, id: listID) }
  }

  func deleteList(_ listID: String) {
    if perform({ try $0.deleteList(id: listID) }) { leaveScope(listID: listID) }
  }

  func deleteFolder(_ folderID: String) {
    perform { try $0.deleteFolder(id: folderID) }
    if navigation.currentScope == .folder(folderID) {
      navigation.listPath = []
      navigation.sidebarSelection = .scope(.everything)
    }
  }

  func moveList(_ listID: String, toFolder folderID: String?) {
    perform { try $0.moveList(id: listID, toFolderId: folderID) }
  }

  func moveFolder(_ folderID: String, toFolder parentID: String?) {
    perform { try $0.moveFolder(id: folderID, toParentFolderId: parentID) }
  }

  func moveList(_ listID: String, by offset: Int) {
    perform { try $0.moveListWithinFolder(id: listID, by: offset) }
  }

  func moveFolder(_ folderID: String, by offset: Int) {
    perform { try $0.moveFolderWithinSiblings(id: folderID, by: offset) }
  }

  /// Puts a list before another in a folder, for drag-to-reorder.
  func placeList(_ listID: String, before beforeID: String?, inFolder folderID: String?) {
    perform { try $0.placeList(id: listID, before: beforeID, inFolderId: folderID) }
  }

  func setNestedListArchived(_ archived: Bool, taskID: String) {
    perform { try $0.setNestedListArchived(archived, id: taskID) }
  }

  /// Turns a whole list back into a task in the Inbox.
  func convertListToTask(_ listID: String) {
    if let task = performReturning({ try $0.convertListToTask(id: listID) }) {
      navigation.listPath = [.list(task.listId)]
      if case .scope = navigation.sidebarSelection { navigation.sidebarSelection = .scope(.list(task.listId)) }
    }
  }

  private func leaveScope(listID: String) {
    navigation.listPath.removeAll { $0.listID == listID }
    if navigation.currentScope?.listID == listID || navigation.sidebarSelection == .scope(.list(listID)) {
      navigation.sidebarSelection = .scope(.everything)
    }
  }

  /// A scope's title, as a header shows it.
  func title(for scope: ListScope) -> String {
    switch scope {
    case .everything: return "Everything"
    case .filter(let id): return structure.filterList(id)?.name ?? "Filter"
    case .folder(let id): return structure.folder(id)?.name ?? "Folder"
    case .list(let id): return structure.list(id)?.name ?? "List"
    case .nested(_, let taskID):
      return structure.sidebar.nestedLists.first { $0.id == taskID }?.task.title ?? task(taskID)?.title ?? "List"
    }
  }
}
