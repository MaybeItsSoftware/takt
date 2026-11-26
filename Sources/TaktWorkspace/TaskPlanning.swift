import Foundation
import TaktCore
import TaktRustCore

public struct TaskCondition: Codable, Identifiable, Equatable, Sendable {
  public let id: String
  public let workspaceId: String
  public var name: String
  public var isLocation: Bool
  public var isArchived: Bool
  public let createdAt: Date
  public var updatedAt: Date
  /// The category as stored ("Location", "Equipment"), or nil. Read the
  /// heading it is shown under from `TaskConditionSection`, which also files
  /// an older client's place, which has none, under Location.
  public var category: String?
  /// Where it sits in the Conditions tab, workspace-wide; nil for one nobody
  /// has placed. The order itself is the core's (`condition_sections`).
  public var sortOrder: Int?
  /// The conditions it switches on while it holds (Home: Floor space, Quiet),
  /// by id. What that makes hold is the core's (`condition_holdings`).
  public var provides: [String] = []
  /// "laptop", "phone" or "tablet" for a device condition, nil for any
  /// other. A device condition holds on a device of its kind by itself and
  /// is never said to hold or switched off (`core/src/conditions/device.rs`).
  public var deviceKind: String?

  /// Whether it means the device in your hand rather than something you say.
  public var isDevice: Bool { !(deviceKind ?? "").trimmingCharacters(in: .whitespaces).isEmpty }
}

/// Stored alongside metadata so complete planning edits share one undo step.
/// References are stable catalogue IDs; all groups match, any member may match.
/// The stored JSON, its normalisation and the refusals' wording are the Rust
/// core's (`core/src/planning.rs`). `Codable` stays for the editor drafts file.
public struct TaskPlanning: Codable, Equatable, Sendable {
  public var startAt: Date?
  public var dueDate: String?
  public var requirementGroups: [[String]]?
  public var minimumBlockSeconds: Int?
  public var requiresSingleSitting: Bool?

  public init(startAt: Date? = nil, dueDate: String? = nil, requirementGroups: [[String]]? = nil,
              minimumBlockSeconds: Int? = nil, requiresSingleSitting: Bool? = nil) {
    self.startAt = startAt; self.dueDate = dueDate; self.requirementGroups = requirementGroups
    self.minimumBlockSeconds = minimumBlockSeconds; self.requiresSingleSitting = requiresSingleSitting
  }

  /// Empty groups and a false single-sitting flag collapse to absent; all-absent is nil.
  /// The start is kept as given: the core only drops fields, and its milliseconds would round it.
  public var normalized: TaskPlanning? {
    taskPlanningNormalized(planning: core).map { var result = TaskPlanning($0); result.startAt = startAt; return result }
  }
}

public struct FocusWorkBlock: Codable, Identifiable, Sendable, Equatable {
  public let id: String
  public let sessionId: String?
  public let taskId: String?
  public let taskTitle: String
  public let seconds: Int
  public let recordedAt: Date
  public var originalTaskId: String?
}

public enum TaskPlanningError: LocalizedError {
  case invalidCondition, invalidSchedule, invalidMinimum, estimateRequired, invalidDate, unavailable
  public var errorDescription: String? {
    let core: PlanningError = switch self {
    case .invalidCondition: .invalidCondition
    case .invalidSchedule: .invalidSchedule
    case .invalidMinimum: .invalidMinimum
    case .estimateRequired: .estimateRequired
    case .invalidDate: .invalidDate
    case .unavailable: .unavailable
    }
    return taskPlanningErrorMessage(error: core)
  }
}
