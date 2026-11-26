import Foundation
import TaktRustCore

/// One condition's part in what holds now: said to hold, provided by a
/// setting that is ("from Home"), or provided but switched off for now.
public struct FocusConditionHolding: Equatable, Sendable {
  public let conditionID: String
  /// Said to hold directly: a place chosen, a box ticked.
  public let isSet: Bool
  /// The conditions said to hold that provide it, in the tab's order.
  public let providedBy: [String]
  /// Provided, but switched off for now.
  public let isOff: Bool
  /// Whether it holds: what every requirement is checked against.
  public let holds: Bool
  /// Holds because it is the device condition of this device's kind, which
  /// nothing said can switch (`core/src/conditions/device.rs`).
  public let isDevice: Bool

  init(_ core: ConditionHolding) {
    conditionID = core.conditionId
    isSet = core.isSet
    providedBy = core.providedBy
    isOff = core.isOff
    holds = core.holds
    isDevice = core.isDevice
  }
}

/// What holds when some conditions are said to hold and some provided ones
/// are switched off: the said ones, plus everything they provide through any
/// number of links, less the overrides. The rules are the Rust core's
/// (`core/src/conditions/provides.rs`), which the phones and the CLI read
/// too, so every surface ranks and explains against the same set.
public enum FocusConditionHoldings {
  /// Every condition set, provided or switched off, by id.
  public static func holdings(
    _ conditions: [TaskCondition], set: Set<String>, off: Set<String>
  ) -> [String: FocusConditionHolding] {
    let rows = conditionHoldings(conditions: conditions.map(\.core), setIds: set.sorted(), offIds: off.sorted())
    return Dictionary(rows.map { ($0.conditionId, FocusConditionHolding($0)) }, uniquingKeysWith: { first, _ in first })
  }

  /// The ids of the conditions that hold.
  public static func held(_ conditions: [TaskCondition], set: Set<String>, off: Set<String>) -> Set<String> {
    Set(effectiveConditionIds(conditions: conditions.map(\.core), setIds: set.sorted(), offIds: off.sorted()))
  }

  /// One condition switched on, off, or (`on` nil) the other way. Off, one
  /// a setting still provides is switched off for now; on, an override goes
  /// and something nothing provides is said to hold; a place replaces the
  /// last one, and the overrides on what it provided go with it.
  public static func switching(
    _ conditionID: String, on: Bool? = nil, in conditions: [TaskCondition], set: Set<String>, off: Set<String>
  ) -> (set: Set<String>, off: Set<String>) {
    let said = focusContextSwitch(
      conditions: conditions.map(\.core), said: ConditionsSaid(conditionIds: set.sorted(), offIds: off.sorted()),
      conditionId: conditionID, on: on)
    return (Set(said.conditionIds), Set(said.offIds))
  }

  /// The overrides that survive the conditions said to hold changing from
  /// `previous` to `next`: those on something an unset or replaced setting
  /// provided go.
  public static func keptOverrides(
    _ conditions: [TaskCondition], previous: Set<String>, next: Set<String>, off: Set<String>
  ) -> Set<String> {
    Set(focusContextKeptOverrides(
      conditions: conditions.map(\.core), previousIds: previous.sorted(), nextIds: next.sorted(), offIds: off.sorted()))
  }
}

/// The kinds of device a device condition can mean, as the core stores them.
public enum ConditionDeviceKind: String, CaseIterable, Identifiable, Sendable {
  case laptop, phone, tablet

  public var id: String { rawValue }

  /// What a picker calls it.
  public var title: String { rawValue.capitalized }

  /// The kind this process said it runs on, if it has.
  public static var thisDevice: ConditionDeviceKind? {
    thisDeviceKind().flatMap(ConditionDeviceKind.init(rawValue:))
  }

  /// Says what kind of device this process runs on, so the device
  /// conditions of that kind hold here. Process-wide and not stored: what
  /// holds is synced, but the device in your hand is not, so every client
  /// says it once at launch, before anything asks what holds.
  public static func setThisDevice(_ kind: ConditionDeviceKind?) {
    setThisDeviceKind(kind: kind?.rawValue)
  }
}

extension TaskCondition {
  /// The kind of device it means, when it is a device condition of a kind
  /// this build knows.
  public var device: ConditionDeviceKind? {
    deviceKind.flatMap { ConditionDeviceKind(rawValue: $0.trimmingCharacters(in: .whitespaces).lowercased()) }
  }
}
