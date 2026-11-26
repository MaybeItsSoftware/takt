import Foundation
import TaktCore
import TaktRustCore

extension WorkspaceStore {

  public func conditions(in workspaceId: String) throws -> [TaskCondition] {
    try Self.mappingCoreErrors { try core.conditions(workspaceId: workspaceId) }.map(TaskCondition.init)
  }

  @discardableResult
  /// Creates a condition: the Rust core's `conditions::create_condition`.
  public func createCondition(workspaceId: String, name: String, isLocation: Bool = false,
                              now: Date = .now) throws -> TaskCondition {
    let id = try coreWrite {
      try core.createCondition(
        workspaceId: workspaceId, name: name, isLocation: isLocation, nowMs: now.coreMilliseconds)
    }
    guard let record = try Self.mappingCoreErrors({ try core.condition(id: id) }).map(TaskCondition.init) else {
      throw TaskPlanningError.invalidCondition
    }
    return record
  }

  @discardableResult
  /// Creates a condition under a category (nil for none; "Location" makes it
  /// a place): the Rust core's `conditions::create_condition_in_category`.
  public func createCondition(workspaceId: String, name: String, category: String?,
                              now: Date = .now) throws -> TaskCondition {
    let id = try coreWrite {
      try core.createConditionInCategory(
        workspaceId: workspaceId, name: name, category: category, nowMs: now.coreMilliseconds)
    }
    guard let record = try Self.mappingCoreErrors({ try core.condition(id: id) }).map(TaskCondition.init) else {
      throw TaskPlanningError.invalidCondition
    }
    return record
  }

  /// Files a condition under a category, or none: the Rust core's
  /// `conditions::set_condition_category`. "Location" makes it a place and
  /// anything else makes it not one.
  public func setConditionCategory(id: String, category: String?, now: Date = .now) throws {
    try coreWrite {
      try core.setConditionCategory(id: id, category: category, nowMs: now.coreMilliseconds)
    }
  }

  /// The conditions an open task in the workspace requires: the Rust core's
  /// `conditions::required_condition_ids`.
  public func requiredConditionIDs(in workspaceId: String) throws -> Set<String> {
    Set(try Self.mappingCoreErrors { try core.requiredConditionIds(workspaceId: workspaceId) })
  }

  /// Saves a condition: the Rust core's `conditions::save_condition`.
  public func saveCondition(id: String, name: String, isLocation: Bool, isArchived: Bool,
                            now: Date = .now) throws {
    try coreWrite {
      try core.saveCondition(
        id: id, name: name, isLocation: isLocation, isArchived: isArchived, nowMs: now.coreMilliseconds)
    }
  }

  /// How many open tasks require each condition, by id: the Rust core's
  /// `conditions::condition_usage`. Completed and archived tasks, and those
  /// in archived lists, do not count.
  public func conditionUsage(in workspaceId: String) throws -> [String: Int] {
    let usage = try Self.mappingCoreErrors { try core.conditionUsage(workspaceId: workspaceId) }
    return Dictionary(usage.map { ($0.conditionId, Int($0.openTasks)) }, uniquingKeysWith: { _, last in last })
  }

  /// The open tasks that require a condition, list by list.
  public func tasksRequiring(condition id: String, in workspaceId: String) throws -> [ConditionRequiringTask] {
    try Self.mappingCoreErrors { try core.tasksRequiringCondition(workspaceId: workspaceId, conditionId: id) }
      .map { ConditionRequiringTask(id: $0.taskId, title: $0.title, listName: $0.listName) }
  }

  /// Moves a condition one place up (negative) or down under its heading.
  @discardableResult
  public func moveCondition(id: String, by offset: Int, now: Date = .now) throws -> Bool {
    try coreWrite { try core.moveCondition(id: id, offset: Int64(offset), nowMs: now.coreMilliseconds) }
  }

  /// Moves a whole heading (nil for "Other") one place up or down.
  @discardableResult
  public func moveConditionCategory(_ category: String?, in workspaceId: String, by offset: Int,
                                    now: Date = .now) throws -> Bool {
    try coreWrite {
      try core.moveConditionCategory(
        workspaceId: workspaceId, category: category, offset: Int64(offset), nowMs: now.coreMilliseconds)
    }
  }

  /// Files every condition under one heading under another, keeping their
  /// places; onto a category in use, the two merge.
  @discardableResult
  public func renameConditionCategory(_ from: String?, to: String?, in workspaceId: String,
                                      now: Date = .now) throws -> Int {
    Int(try coreWrite {
      try core.renameConditionCategory(workspaceId: workspaceId, from: from, to: to, nowMs: now.coreMilliseconds)
    })
  }

  /// Sets what a condition provides, replacing what it did: the Rust core's
  /// `conditions::set_condition_provides`, one "Edit Condition" step. False
  /// when it already provided exactly that.
  @discardableResult
  public func setConditionProvides(id: String, provides: [String], now: Date = .now) throws -> Bool {
    try coreWrite {
      try core.setConditionProvides(id: id, providedIds: provides, nowMs: now.coreMilliseconds)
    }
  }

  /// Marks a condition as the device condition of `kind` ("laptop", "phone"
  /// or "tablet"), or with nil makes it an ordinary one again: the Rust
  /// core's `conditions::device::set_condition_device`, one "Edit Condition"
  /// step. Marking it takes it out of what is said to hold and of what
  /// others provide, and stops it being a place. False when it already was.
  @discardableResult
  public func setConditionDeviceKind(id: String, kind: String?, now: Date = .now) throws -> Bool {
    try coreWrite {
      try core.setConditionDeviceKind(id: id, kind: kind, nowMs: now.coreMilliseconds)
    }
  }

  /// What a list requires — a standalone list, or a nested list's task id —
  /// as requirement groups; empty when nothing.
  public func listRequirementGroups(listId: String) throws -> [[String]] {
    try Self.mappingCoreErrors { try core.listRequirementGroups(listId: listId) }
  }

  /// Sets what a list requires, replacing what it did; every task in it, and
  /// in the nested lists under it, inherits it. Empty requires nothing. The
  /// Rust core's `conditions::inherited::set_list_requirements`, one "Edit
  /// Conditions" step. False when it already required exactly that.
  @discardableResult
  public func setListRequirements(listId: String, groups: [[String]], now: Date = .now) throws -> Bool {
    try coreWrite {
      try core.setListRequirements(listId: listId, groups: groups, nowMs: now.coreMilliseconds)
    }
  }

  /// The lists a task inherits requirements from, outermost first: its list,
  /// then each nested list above it that requires anything.
  public func inheritedRequirements(taskId: String) throws -> [TaskInheritedRequirement] {
    try Self.mappingCoreErrors { try core.inheritedRequirements(taskId: taskId) }.map(TaskInheritedRequirement.init)
  }

  /// Merges a condition into another: its tasks require the other, and it
  /// is archived. Returns how many tasks changed.
  @discardableResult
  public func mergeConditions(_ from: String, into: String, now: Date = .now) throws -> Int {
    Int(try coreWrite { try core.mergeConditions(from: from, into: into, nowMs: now.coreMilliseconds) })
  }

  /// What holds now, as last saved, or nil if it never was.
  public func focusContext(in workspaceId: String) throws -> SavedFocusContext? {
    try Self.mappingCoreErrors { try core.focusContext(workspaceId: workspaceId) }.map(SavedFocusContext.init)
  }

  /// Saves what holds now. Not an undo step. The core stamps when the
  /// conditions on were said, whatever `conditionsSetAt` carries.
  @discardableResult
  public func saveFocusContext(_ context: SavedFocusContext, in workspaceId: String, now: Date = .now) throws -> Bool {
    try Self.mappingCoreErrors {
      try core.saveFocusContext(workspaceId: workspaceId, context: context.core, nowMs: now.coreMilliseconds)
    }
  }

  /// Copies a task's requirements, start and block rules onto its subtasks,
  /// each keeping its own due date: the Rust core's
  /// `editor::apply_planning_to_descendants`.
  public func applyPlanningToDescendants(of taskId: String) throws {
    try coreWrite {
      try core.applyPlanningToDescendants(taskId: taskId, nowMs: Date.now.coreMilliseconds, zone: TimeZone.current.identifier)
    }
  }

}

extension WorkspaceStore {
  public func taskPlanningValues() throws -> [String: TaskPlanning] {
    // Decoded and normalised in the core: one crossing, only the planned tasks.
    let entries = try Self.mappingCoreErrors { try core.taskPlanningValues() }
    return Dictionary(entries.map { ($0.taskId, TaskPlanning($0.planning)) }, uniquingKeysWith: { _, last in last })
  }
}

/// What holds now — the conditions on, the time available, the goal, when
/// the answer lapses and what was set aside when it last did — as the
/// workspace keeps it (`focus_contexts`), where the CLI and the agent can
/// read and set it too.
public struct SavedFocusContext: Equatable, Sendable {
  public var conditionIDs: Set<String>
  /// When the conditions on were said; nil with none on.
  public var conditionsSetAt: Date?
  public var availableUntil: Date?
  public var expiresAt: Date?
  public var mode: FocusTimeMode
  public var suggestedIDs: Set<String>
  /// Provided conditions switched off for now, until whatever provides them
  /// is unset or replaced; the core drops the rest when it saves.
  public var offIDs: Set<String>

  public init(
    conditionIDs: Set<String> = [], conditionsSetAt: Date? = nil, availableUntil: Date? = nil,
    expiresAt: Date? = nil, mode: FocusTimeMode = .progress, suggestedIDs: Set<String> = [],
    offIDs: Set<String> = []
  ) {
    self.conditionIDs = conditionIDs; self.conditionsSetAt = conditionsSetAt
    self.availableUntil = availableUntil; self.expiresAt = expiresAt
    self.mode = mode; self.suggestedIDs = suggestedIDs; self.offIDs = offIDs
  }

  init(_ core: StoredFocusContext) {
    self.init(
      conditionIDs: Set(core.conditionIds), conditionsSetAt: core.conditionsSetAtMs.map(Date.init(coreMilliseconds:)),
      availableUntil: core.availableUntilMs.map(Date.init(coreMilliseconds:)), expiresAt: core.expiresAtMs.map(Date.init(coreMilliseconds:)),
      mode: FocusTimeMode(rawValue: core.mode) ?? .progress, suggestedIDs: Set(core.suggestedIds),
      offIDs: Set(core.offIds))
  }

  var core: StoredFocusContext {
    StoredFocusContext(
      conditionIds: conditionIDs.sorted(), conditionsSetAtMs: conditionsSetAt?.coreMilliseconds,
      availableUntilMs: availableUntil?.coreMilliseconds, expiresAtMs: expiresAt?.coreMilliseconds,
      mode: mode.rawValue, suggestedIds: suggestedIDs.sorted(), offIds: offIDs.sorted())
  }

  /// Whether it had lapsed by `now`.
  public func hasExpired(at now: Date) -> Bool { expiresAt.map { $0 <= now } ?? false }

  /// As it stands at `now`: once expired, its conditions are set aside to be
  /// offered back. The core's `focus_context_in_force`.
  public func inForce(at now: Date) -> SavedFocusContext {
    SavedFocusContext(focusContextInForce(context: core, nowMs: now.coreMilliseconds))
  }
}

/// One list a task inherits requirements from: read only on the task, set
/// on the list.
public struct TaskInheritedRequirement: Identifiable, Equatable, Sendable {
  /// The standalone list's id, or the nested list's task id.
  public let listID: String
  public let listName: String
  /// Whether it is a nested list (a task record) rather than a standalone one.
  public let isNested: Bool
  public let groups: [[String]]

  public var id: String { listID }

  public init(listID: String, listName: String, isNested: Bool, groups: [[String]]) {
    self.listID = listID; self.listName = listName; self.isNested = isNested; self.groups = groups
  }

  init(_ core: InheritedRequirement) {
    self.init(listID: core.listId, listName: core.listName, isNested: core.isNested, groups: core.groups)
  }
}

/// An open task that requires a condition, with the list it is in.
public struct ConditionRequiringTask: Identifiable, Equatable, Sendable {
  public let id: String
  public let title: String
  public let listName: String

  public init(id: String, title: String, listName: String) {
    self.id = id; self.title = title; self.listName = listName
  }
}
