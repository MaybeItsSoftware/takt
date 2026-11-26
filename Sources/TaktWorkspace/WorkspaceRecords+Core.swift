import Foundation
import TaktRustCore

// The workspace's rows as the Rust core reads them (core/src/records.rs),
// turned into the records the screens use. Every read that hands back whole
// rows goes through these, so a column added to a table is added once.

extension Date {
  /// A moment the core handed back as milliseconds since 1970.
  init(coreMilliseconds ms: Int64) {
    self.init(timeIntervalSince1970: Double(ms) / 1000)
  }
}

extension WorkspaceTask {
  init(_ row: TaskRow) {
    self.init(
      id: row.id, listId: row.listId, parentTaskId: row.parentTaskId, title: row.title, notes: row.notes,
      status: TaskStatus(rawValue: row.status) ?? .open, sortOrder: Int(row.sortOrder),
      dueAt: row.dueAtMs.map(Date.init(coreMilliseconds:)), estimateSeconds: row.estimateSeconds.map { Int($0) },
      sourceSystem: row.sourceSystem, sourceId: row.sourceId,
      itemKind: row.itemKind.flatMap(WorkspaceItemKind.init(rawValue:)), isPromoted: row.isPromoted,
      archivedAt: row.archivedAtMs.map(Date.init(coreMilliseconds:)),
      completedAt: row.completedAtMs.map(Date.init(coreMilliseconds:)),
      createdAt: Date(coreMilliseconds: row.createdAtMs), updatedAt: Date(coreMilliseconds: row.updatedAtMs))
  }
}

extension TaskList {
  init(_ row: ListRow) {
    self.init(
      id: row.id, workspaceId: row.workspaceId, folderId: row.folderId, name: row.name, colorHex: row.colorHex,
      sortOrder: Int(row.sortOrder), isArchived: row.isArchived,
      systemRole: row.systemRole.flatMap(TaskListRole.init(rawValue:)), visibleRootTaskId: row.visibleRootTaskId,
      completedAt: row.completedAtMs.map(Date.init(coreMilliseconds:)),
      createdAt: Date(coreMilliseconds: row.createdAtMs), updatedAt: Date(coreMilliseconds: row.updatedAtMs),
      priorityRank: row.priorityRank.map { Int($0) },
      requirementGroups: row.requirementGroups.isEmpty ? nil : row.requirementGroups)
  }
}

extension ListFolder {
  init(_ row: FolderRow) {
    self.init(
      id: row.id, workspaceId: row.workspaceId, parentFolderId: row.parentFolderId, name: row.name,
      sortOrder: Int(row.sortOrder), createdAt: Date(coreMilliseconds: row.createdAtMs),
      updatedAt: Date(coreMilliseconds: row.updatedAtMs))
  }
}

extension Workspace {
  init(_ row: WorkspaceRow) {
    self.init(
      id: row.id, name: row.name, createdAt: Date(coreMilliseconds: row.createdAtMs),
      updatedAt: Date(coreMilliseconds: row.updatedAtMs))
  }
}

extension TaskOutlineItem {
  init(_ item: OutlineItem) {
    self.init(task: WorkspaceTask(item.task), depth: Int(item.depth))
  }
}
