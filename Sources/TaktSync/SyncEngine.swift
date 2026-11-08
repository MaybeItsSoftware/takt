import Foundation
import TaktWorkspace

/// Runs the client half of the sync cycle in `docs/sync.md` against one store.
///
/// An actor so a cycle never overlaps another: a write that lands mid-cycle
/// stays in the outbox and goes out with the next one, and a second request for
/// a cycle while one runs is folded into a single follow-up.
public actor SyncEngine {
  public enum Status: Equatable, Sendable {
    case idle(lastSyncedAt: Date?)
    case syncing
    case failed(String)
    /// The server refused the token. Retrying cannot help; signing in can.
    case signedOut
    /// The hosted server wants Takt Sync. Not an error: sync waits.
    case needsSubscription
  }

  public struct Outcome: Equatable, Sendable {
    public var pushed: Int
    public var pulled: Int
    /// Whether the pull changed the workspace, so the UI knows to reload.
    public var changedWorkspace: Bool
  }

  let store: WorkspaceStore
  let transport: any SyncTransport
  /// This device. The clock the core stamps with is the stored one, made on
  /// the device id the store was paired with, which is this one.
  let deviceId: String
  let wallClock: @Sendable () -> Date
  public private(set) var status: Status = .idle(lastSyncedAt: nil)
  // An actor runs other calls while one awaits the network, so two cycles
  // could interleave and a slower pull land its older rows and cursor after a
  // faster one. Cycles queue here instead.
  private var running = false
  private var waiters: [CheckedContinuation<Void, Never>] = []

  /// Small enough that one push finishes inside the request timeout even
  /// when the server's database is far away: the server merges a push row by
  /// row, a couple of round trips to Postgres each, and a batch it cannot
  /// finish in time is rolled back whole and sent again for ever. Each
  /// accepted batch empties its part of the outbox, so a first sync of a big
  /// workspace makes progress batch by batch instead.
  static let pushBatch = 100
  static let pullPage = 1000

  public init(
    store: WorkspaceStore, transport: any SyncTransport, deviceId: String,
    wallClock: @escaping @Sendable () -> Date = { Date() }
  ) {
    self.store = store
    self.transport = transport
    self.deviceId = deviceId
    self.wallClock = wallClock
  }

  /// One push-then-pull cycle. The first cycle after pairing snapshots the
  /// workspace and pulls first, so a device joining an account adopts it.
  @discardableResult
  public func sync(wait: Int = 0) async throws -> Outcome {
    while running { await withCheckedContinuation { waiters.append($0) } }
    running = true
    defer {
      running = false
      if !waiters.isEmpty { waiters.removeFirst().resume() }
    }
    guard let state = try store.syncState() else { throw SyncError.notPaired }
    status = .syncing
    do {
      var outcome = Outcome(pushed: 0, pulled: 0, changedWorkspace: false)
      if state.needsSnapshot {
        try store.enqueueSyncSnapshot(now: wallClock())
        try await pull(from: state.cursor, wait: 0, outcome: &outcome)
        try await push(outcome: &outcome)
      } else {
        try await push(outcome: &outcome)
        let cursor = try store.syncState()?.cursor ?? state.cursor
        try await pull(from: cursor, wait: wait, outcome: &outcome)
      }
      status = .idle(lastSyncedAt: wallClock())
      return outcome
    } catch {
      switch error as? SyncError {
      case .unauthorized: status = .signedOut
      case .subscriptionRequired: status = .needsSubscription
      default: status = .failed(error.localizedDescription)
      }
      throw error
    }
  }

  /// Sends the outbox in batches. The core makes each body from the outbox,
  /// stamped from the stored clock in the order the edits were made, and
  /// keeps the clock when the batch is acknowledged.
  private func push(outcome: inout Outcome) async throws {
    while let batch = try store.prepareSyncPush(limit: Self.pushBatch, now: wallClock()) {
      try await transport.push(body: batch.body)
      try store.finishSyncPush(batch, now: wallClock())
      outcome.pushed += batch.count
    }
  }

  /// Gathers every page of the feed in the core, then applies them in one
  /// transaction: a task can arrive a page before its list, and only the end
  /// of the whole pull is a consistent state.
  private func pull(from cursor: Int64, wait: Int, outcome: inout Outcome) async throws {
    let pages = SyncPullPages()
    var next = cursor
    var firstPage = true
    while true {
      let body = try await transport.changes(since: next, limit: Self.pullPage, wait: firstPage ? wait : 0)
      firstPage = false
      let page: (cursor: Int64, hasMore: Bool)
      do {
        page = try pages.add(body)
      } catch {
        throw SyncError.invalid("The sync server's answer couldn't be read.")
      }
      next = page.cursor
      if !page.hasMore { break }
    }
    let applied = try store.applySyncPull(pages, now: wallClock())
    outcome.pulled += applied.pulled
    outcome.changedWorkspace = outcome.changedWorkspace || applied.changed
  }

  static func ms(_ date: Date) -> Int64 { Int64(date.timeIntervalSince1970 * 1000) }
}
