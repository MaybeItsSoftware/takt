import Foundation
import TaktRustCore

/// What a task row says beside its title, as the Mac's `detail_chips` reads
/// it from a task's metadata: who it waits on, its tags, whether it repeats or
/// links out, and its priority.
public struct TaskChipMetadata: Sendable, Equatable {
  public var waitingOn: String?
  public var tags: [String]
  public var isRecurring: Bool
  public var isLinked: Bool
  public var priority: Int?

  public init(
    waitingOn: String? = nil, tags: [String] = [], isRecurring: Bool = false, isLinked: Bool = false,
    priority: Int? = nil
  ) {
    self.waitingOn = waitingOn
    self.tags = tags
    self.isRecurring = isRecurring
    self.isLinked = isLinked
    self.priority = priority
  }

  /// Nothing to draw.
  public var isEmpty: Bool { waitingOn == nil && tags.isEmpty && !isRecurring && !isLinked && priority == nil }
}

extension WorkspaceStore {
  /// The chips for many tasks in one read. A task with nothing to say is
  /// left out.
  public func chipMetadata(for taskIDs: [String]) throws -> [String: TaskChipMetadata] {
    guard !taskIDs.isEmpty else { return [:] }
    var chips: [String: TaskChipMetadata] = [:]
    for record in try Self.mappingCoreErrors({ try core.metadataForTasks(taskIds: taskIDs) }) {
      let entry = TaskChipMetadata(
        waitingOn: record.waitingOn.flatMap { $0.isEmpty ? nil : $0 },
        tags: Self.strings(record.tagsJson), isRecurring: record.recurrenceRule.map { !$0.isEmpty } ?? false,
        isLinked: !Self.strings(record.externalLinksJson).isEmpty, priority: record.priority.map { Int($0) })
      if !entry.isEmpty { chips[record.taskId] = entry }
    }
    return chips
  }

  private static func strings(_ json: String) -> [String] {
    guard let data = json.data(using: .utf8) else { return [] }
    return ((try? JSONDecoder().decode([String].self, from: data)) ?? []).filter { !$0.isEmpty }
  }
}
