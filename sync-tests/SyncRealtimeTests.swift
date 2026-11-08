import Foundation
@testable import TaktSync
import TaktWorkspace
import XCTest

/// The Realtime channel's frames (`SyncRealtime`), and the scheduler giving
/// up its long-poll while the channel is joined, so an idle device holds no
/// request open on the sync server.
final class SyncRealtimeTests: XCTestCase {
  private let channel = "realtime:sync:6a1f"

  func testTheSocketAndChannelAreNamedAsSupabaseWants() throws {
    let url = SyncRealtime.socketURL(
      supabaseURL: try XCTUnwrap(URL(string: "https://abc.supabase.co")), key: "sb_publishable_x")
    XCTAssertEqual(url.absoluteString, "wss://abc.supabase.co/realtime/v1/websocket?apikey=sb_publishable_x&vsn=1.0.0")
    XCTAssertEqual(SyncRealtime.channel(accountId: "6a1f"), channel)
  }

  func testJoiningAsksForThePrivateChannelWithTheToken() throws {
    let text = SyncRealtime.join(channel: channel, token: "jwt", ref: "1")
    let frame = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
    let payload = try XCTUnwrap(frame["payload"] as? [String: Any])
    let config = try XCTUnwrap(payload["config"] as? [String: Any])
    XCTAssertEqual(frame["event"] as? String, "phx_join")
    XCTAssertEqual(frame["topic"] as? String, channel)
    XCTAssertEqual(config["private"] as? Bool, true)
    XCTAssertEqual(payload["access_token"] as? String, "jwt")
  }

  func testRepliesBroadcastsAndClosesAreToldApart() {
    func reply(_ status: String, ref: String) -> String {
      #"{"topic":"\#(channel)","event":"phx_reply","ref":"\#(ref)","payload":{"status":"\#(status)","response":{"reason":"Unauthorized"}}}"#
    }
    XCTAssertEqual(SyncRealtime.classify(reply("ok", ref: "1"), channel: channel, joinRef: "1"), .joined)
    XCTAssertEqual(
      SyncRealtime.classify(reply("error", ref: "1"), channel: channel, joinRef: "1"), .ended("Unauthorized"))
    XCTAssertEqual(SyncRealtime.classify(reply("ok", ref: "7"), channel: channel, joinRef: "1"), .other)
    let broadcast =
      #"{"topic":"\#(channel)","event":"broadcast","payload":{"type":"broadcast","event":"changed","payload":{"cursor":9}}}"#
    XCTAssertEqual(SyncRealtime.classify(broadcast, channel: channel, joinRef: "1"), .news)
    let elsewhere = #"{"topic":"realtime:sync:other","event":"broadcast","payload":{}}"#
    XCTAssertEqual(SyncRealtime.classify(elsewhere, channel: channel, joinRef: "1"), .other)
    let expired = #"{"topic":"\#(channel)","event":"system","payload":{"status":"error","message":"Token has expired"}}"#
    XCTAssertEqual(SyncRealtime.classify(expired, channel: channel, joinRef: "1"), .ended("Token has expired"))
    XCTAssertEqual(SyncRealtime.classify("not json", channel: channel, joinRef: "1"), .other)
  }

  /// Answers every pull with an empty page and records how long each one
  /// asked to wait.
  private final class RecordingTransport: SyncTransport, @unchecked Sendable {
    private let lock = NSLock()
    private var recorded: [Int] = []
    var waits: [Int] { lock.withLock { recorded } }

    func push(body: Data) async throws {}

    func changes(since cursor: Int64, limit: Int, wait: Int) async throws -> Data {
      lock.withLock { recorded.append(wait) }
      // Stands in for a long-poll that hears nothing.
      if wait > 0 { try await Task.sleep(for: .milliseconds(20)) }
      return Data(#"{"rows":[],"cursor":\#(cursor),"hasMore":false}"#.utf8)
    }
  }

  private func waitUntil(_ condition: () -> Bool) async throws {
    for _ in 0..<200 where !condition() { try await Task.sleep(for: .milliseconds(10)) }
    XCTAssertTrue(condition())
  }

  func testWhileTheChannelIsJoinedThereIsNoLongPollOnlyACycleOnNews() async throws {
    let url = FileManager.default.temporaryDirectory.appendingPathComponent("Realtime-\(UUID().uuidString).sqlite")
    defer { try? FileManager.default.removeItem(at: url) }
    let store = try WorkspaceStore(databaseURL: url)
    _ = try store.bootstrapIfNeeded()
    try store.beginSync(deviceId: "mac", serverURL: "memory://")
    // The first cycle snapshots and pulls; mark it done so every cycle is
    // one pull with the wait under test.
    let transport = RecordingTransport()
    let engine = SyncEngine(store: store, transport: transport, deviceId: "mac")
    _ = try await engine.sync()
    let scheduler = SyncScheduler(engine: engine) { _, _ in }

    await scheduler.start()
    try await waitUntil { transport.waits.contains(25) }

    await scheduler.realtimeJoined(true)
    try await waitUntil { transport.waits.last == 0 }
    try await Task.sleep(for: .milliseconds(100))
    let settled = transport.waits.count
    try await Task.sleep(for: .milliseconds(200))
    XCTAssertEqual(transport.waits.count, settled, "joined and idle: no more requests")

    await scheduler.newsArrived()
    try await waitUntil { transport.waits.count == settled + 1 }
    XCTAssertEqual(transport.waits.last, 0)

    await scheduler.realtimeJoined(false)
    try await waitUntil { transport.waits.last == 25 }
    await scheduler.stop()
  }
}
