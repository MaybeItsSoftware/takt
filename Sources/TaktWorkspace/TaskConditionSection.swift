import Foundation
import TaktRustCore

/// Conditions under one heading: a category, or "Other" for the
/// uncategorised. The headings, their order and what "Available now" offers
/// are the Rust core's (`core/src/conditions.rs`), so the Mac and the phones
/// group alike.
public struct TaskConditionSection: Identifiable, Equatable, Sendable {
  /// The category, or nil for the uncategorised.
  public let category: String?
  /// What the heading says.
  public let title: String
  /// Whether saying one holds replaces the rest: the Location heading.
  public let isExclusive: Bool
  /// Whether it is a setting — something you say about your situation, which
  /// switches on what tasks need — rather than properties: Location, and any
  /// heading a condition under which provides something. Settings come first.
  public let isSetting: Bool
  /// Whether it is the Device heading, first of all: conditions that hold on
  /// a device of their kind by themselves, so they are shown as holding here
  /// or not rather than offered as something to switch.
  public let isDevice: Bool
  public let conditions: [TaskCondition]

  public var id: String { category.map { "category.\($0)" } ?? "uncategorised" }

  /// The category places are filed under.
  public static let locationCategory = "Location"

  init(_ section: ConditionSection) {
    category = section.category
    title = section.title
    isExclusive = section.isExclusive
    isSetting = section.isSetting
    isDevice = section.isDevice
    conditions = section.conditions.map(TaskCondition.init)
  }

  /// Every condition under its heading: the settings first (Location, then
  /// any heading whose conditions provide something), then the rest, each in
  /// the order the workspace keeps.
  public static func sections(_ conditions: [TaskCondition]) -> [TaskConditionSection] {
    conditionSections(conditions: conditions.map(\.core)).map(TaskConditionSection.init)
  }

  /// What "Available now" offers: only the conditions an open task requires
  /// and the settings that provide one, and any that is on or switched off
  /// for now, under their headings. Empty when nothing needs one.
  public static func available(
    _ conditions: [TaskCondition], required: Set<String>, active: Set<String>
  ) -> [TaskConditionSection] {
    availableConditionSections(
      conditions: conditions.map(\.core), requiredIds: Array(required), activeIds: Array(active)
    ).map(TaskConditionSection.init)
  }

  /// The categories a condition can be filed under: Location, then every
  /// other one in use, alphabetically.
  public static func categories(_ conditions: [TaskCondition]) -> [String] {
    conditionCategories(conditions: conditions.map(\.core))
  }

  /// A typed category made canonical: trimmed, nil when empty, "location"
  /// as "Location".
  public static func normalizedCategory(_ category: String?) -> String? {
    conditionCategoryNormalized(category: category)
  }
}

extension TaskCondition {
  /// The heading it is shown under: its category, Location for a place
  /// filed under none (as an older client leaves one), or "Other".
  public var categoryTitle: String {
    conditionSectionTitle(condition: core)
  }
}
