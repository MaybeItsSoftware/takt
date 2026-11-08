import Foundation
import TaktRustCore

// Shared lists (docs/shared-lists-plan.md): what this device knows of the
// lists its account shares or was invited to, kept by the Rust core's
// `sync::spaces`. The requests are TaktSync's; this is the file's side.

extension WorkspaceStore {
  /// Every shared list this device knows, by name.
  public func sharedLists() -> [SharedList] {
    (try? Self.mappingCoreErrors { try core.sharedLists() }) ?? []
  }

  /// The shared list `listId` is, or nil when it isn't shared.
  public func sharedList(listId: String) -> SharedList? {
    (try? Self.mappingCoreErrors { try core.sharedList(listId: listId) }) ?? nil
  }

  /// The name to share `listId` under, or why it can't be shared: the
  /// Inbox, or a list someone else shared.
  public func shareableListName(listId: String) throws -> String {
    try Self.mappingCoreErrors { try core.shareableListName(listId: listId) }
  }

  /// Keeps the space the server answered sharing or joining with.
  @discardableResult
  public func recordSharedSpace(body: String) throws -> SharedList {
    try coreWrite { try core.recordSharedSpace(body: body) }
  }

  /// After leaving (its rows go) or stopping sharing (they stay, and go
  /// back to your own space on the next push).
  public func forgetSharedSpace(spaceId: String, stoppedSharing: Bool) throws {
    try coreWrite { try core.forgetSharedSpace(spaceId: spaceId, stoppedSharing: stoppedSharing) }
  }
}

extension SharedList {
  public var isOwner: Bool { role == "owner" }

  /// This account's id, as the list's members name it.
  public var yourAccountId: String? { members.first(where: \.you)?.accountId }

  /// "You (owner), sam@example.com".
  public var membersLine: String {
    let names = members.map { member -> String in
      let name = member.you ? "You" : (member.email ?? "Someone")
      return member.role == "owner" ? "\(name) (owner)" : name
    }
    return names.isEmpty ? "Just you so far" : names.joined(separator: ", ")
  }
}
