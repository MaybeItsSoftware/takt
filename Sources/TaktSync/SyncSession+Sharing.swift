import Foundation
import TaktRustCore
import TaktWorkspace

// Shared lists from the phone (docs/shared-lists-plan.md): sharing,
// inviting, joining, leaving, stopping sharing and removing someone. Each is
// one or two requests; the rows travel in the next ordinary sync, which each
// starts once it is done.

/// An invite made for one person.
public struct SharedListInvite: Equatable, Sendable {
  public let listName: String
  /// The link to send: the server's page for the code, which opens the app.
  public let link: URL
  /// The code in groups of four, for reading out or typing.
  public let code: String
}

extension SyncSession {
  /// Shares `listId` if it isn't yet, then makes an invite. Run again for
  /// each person: an invite works once, for seven days.
  public func shareList(listId: String) async throws -> SharedListInvite {
    let name = try store.shareableListName(listId: listId)
    let shared: SharedList
    if let known = store.sharedList(listId: listId) {
      shared = known
    } else {
      let body = syncShareListBody(listId: listId, name: name)
      let answer = try await signedInCall {
        try await $0.request("POST", syncSpacesPath(), body: Data(body.utf8))
      }
      shared = try store.recordSharedSpace(body: String(decoding: answer, as: UTF8.self))
    }
    let answer = try await signedInCall {
      try await $0.request("POST", syncSpaceInvitesPath(spaceId: shared.spaceId), body: Data("{}".utf8))
    }
    let invite = try syncDecodeInvite(body: String(decoding: answer, as: UTF8.self))
    let server = credentials?.serverURL.absoluteString ?? endpoints.serverURL.absoluteString
    guard let link = URL(string: syncInviteLink(serverUrl: server, code: invite.code)) else {
      throw SyncError.invalid("The invite link couldn't be made.")
    }
    Task { _ = await syncNow() }
    return SharedListInvite(listName: name, link: link, code: syncInviteCodeGrouped(code: invite.code))
  }

  /// What an invite is for, before joining it. Nil when `text` holds no code.
  public func previewInvite(_ text: String) async throws -> SyncInvitePreview? {
    guard let code = syncInviteCode(text: text) else { return nil }
    let answer = try await signedInCall { try await $0.request("GET", syncInvitePath(code: code), body: nil) }
    return try syncDecodeInvitePreview(body: String(decoding: answer, as: UTF8.self))
  }

  /// Joins the list an invite link or code names.
  @discardableResult
  public func joinList(_ text: String) async throws -> SharedList {
    guard let code = syncInviteCode(text: text) else {
      throw SyncError.invalid("That isn't an invite link or code.")
    }
    let answer = try await signedInCall {
      try await $0.request("POST", syncAcceptInvitePath(code: code), body: Data("{}".utf8))
    }
    let shared = try store.recordSharedSpace(body: String(decoding: answer, as: UTF8.self))
    _ = await syncNow()
    return shared
  }

  /// Leaves a list someone else shared: it goes from this account's devices.
  public func leaveList(listId: String) async throws {
    guard let shared = store.sharedList(listId: listId) else { return }
    guard !shared.isOwner else { throw SyncError.invalid("It's your list: stop sharing it instead.") }
    guard let you = shared.yourAccountId else {
      throw SyncError.invalid("Sync hasn't said who you are in this list yet.")
    }
    try await signedInCall {
      try await $0.request("DELETE", syncSpaceMemberPath(spaceId: shared.spaceId, accountId: you), body: nil)
    }
    try store.forgetSharedSpace(spaceId: shared.spaceId, stoppedSharing: false)
    onRemoteChanges?()
  }

  /// Stops sharing a list you own: it stays yours.
  public func stopSharing(listId: String) async throws {
    guard let shared = store.sharedList(listId: listId) else { return }
    guard shared.isOwner else {
      throw SyncError.invalid("Only its owner can stop sharing it. You can leave it.")
    }
    try await signedInCall { try await $0.request("DELETE", syncSpacePath(spaceId: shared.spaceId), body: nil) }
    try store.forgetSharedSpace(spaceId: shared.spaceId, stoppedSharing: true)
    _ = await syncNow()
  }

  /// Takes someone out of a list you own.
  public func removeMember(listId: String, accountId: String) async throws {
    guard let shared = store.sharedList(listId: listId) else { return }
    try await signedInCall {
      try await $0.request("DELETE", syncSpaceMemberPath(spaceId: shared.spaceId, accountId: accountId), body: nil)
    }
    _ = await syncNow()
  }
}
