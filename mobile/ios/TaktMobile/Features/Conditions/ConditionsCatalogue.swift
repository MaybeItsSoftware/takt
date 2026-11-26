import Foundation
import TaktWorkspace

/// The condition catalogue as the manager draws it: every condition, active
/// first, with how many tasks require each — read in one go off the main
/// actor.
struct ConditionsCatalogue: Sendable, Equatable {
  var conditions: [TaskCondition] = []
  /// How many open tasks require each condition, by ID: the core's count,
  /// in which a completed or archived task no longer counts.
  var usage: [String: Int] = [:]

  static let empty = ConditionsCatalogue()

  static func load(store: WorkspaceStore, workspaceID: String) throws -> ConditionsCatalogue {
    let conditions = try store.conditions(in: workspaceID)
    return ConditionsCatalogue(conditions: conditions, usage: try store.conditionUsage(in: workspaceID))
  }

  var active: [TaskCondition] { conditions.filter { !$0.isArchived } }
  /// The active conditions under their category headings.
  var activeSections: [TaskConditionSection] { TaskConditionSection.sections(active) }
  /// The categories a condition can be filed under: Location, then every other one in use.
  var categories: [String] { TaskConditionSection.categories(conditions) }
  var archived: [TaskCondition] { conditions.filter(\.isArchived) }
}

/// Writes to the catalogue. Tasks refer to a condition by its ID, so renaming
/// one keeps every requirement naming it, and archiving hides it from the
/// pickers without breaking the tasks that still need it.
@MainActor
extension WorkspaceModel {
  @discardableResult
  func createCondition(named name: String, isLocation: Bool) -> TaskCondition? {
    let workspaceID = workspace.id
    return performReturning { try $0.createCondition(workspaceId: workspaceID, name: name, isLocation: isLocation) }
  }

  /// Makes a condition under a category (nil for none; "Location" makes it a place).
  @discardableResult
  func createCondition(named name: String, category: String?) -> TaskCondition? {
    let workspaceID = workspace.id
    return performReturning { try $0.createCondition(workspaceId: workspaceID, name: name, category: category) }
  }

  /// Files a condition under a category, or none. Location makes it a place;
  /// anything else makes it not one.
  func setConditionCategory(_ condition: TaskCondition, category: String?) {
    perform { store in try store.setConditionCategory(id: condition.id, category: category) }
  }

  func saveCondition(_ condition: TaskCondition, name: String? = nil, isLocation: Bool? = nil, isArchived: Bool? = nil) {
    perform { store in
      try store.saveCondition(
        id: condition.id, name: name ?? condition.name, isLocation: isLocation ?? condition.isLocation,
        isArchived: isArchived ?? condition.isArchived)
    }
  }
}

extension ConditionDeviceKind {
  /// The SF Symbol a device condition is drawn with.
  var symbol: String {
    switch self {
    case .laptop: "laptopcomputer"
    case .phone: "iphone"
    case .tablet: "ipad"
    }
  }
}

@MainActor
extension WorkspaceModel {
  /// Marks a condition as meaning a kind of device, or (nil) an ordinary
  /// condition again. Marking it takes it out of what is said to hold, since
  /// from then on each device decides that by what it is.
  func setConditionDevice(_ condition: TaskCondition, kind: ConditionDeviceKind?) {
    perform { store in try store.setConditionDeviceKind(id: condition.id, kind: kind?.rawValue) }
  }

  /// What a list requires — a standalone list, or a nested list's task id —
  /// as requirement groups.
  func listRequirementGroups(_ listID: String) -> [[String]] {
    (try? store.listRequirementGroups(listId: listID)) ?? []
  }

  /// Sets what a list requires; every task in it, and in nested lists under
  /// it, inherits it.
  func setListRequirements(_ groups: [[String]], list listID: String) {
    perform { store in try store.setListRequirements(listId: listID, groups: groups) }
  }
}
