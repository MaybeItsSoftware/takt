import Foundation
import TaktRustCore

/// Waits for other devices' news on the account's Supabase Realtime channel
/// (`docs/sync.md`, "News on Supabase Realtime"), so the scheduler need not
/// hold a long-poll open on the sync server, which Cloud Run bills for every
/// second it is open. The server broadcasts `changed` on `sync:<account>`
/// after every push that writes; this only passes that on, and the cycle
/// pulls as it always did.
///
/// Realtime speaks the Phoenix channel protocol: JSON frames of `{topic,
/// event, payload, ref, join_ref}`, a `phx_join` answered by a `phx_reply`,
/// and a heartbeat on the `phoenix` topic or the socket is dropped. The Mac's
/// Rust app has the same client in `desktop/src/sync/realtime.rs`.
public final class SyncRealtime: Sendable {
  /// Joined (true) or not (false), and news, in the order they happen.
  public enum Signal: Sendable, Equatable {
    case joined(Bool)
    case news
  }

  let serverURL: URL
  let socketURL: URL
  let channel: String
  let tokens: any SyncAccessTokenProvider
  let session: URLSession

  public init(
    serverURL: URL,
    endpoints: SyncEndpoints,
    accountId: String,
    tokens: any SyncAccessTokenProvider,
    session: URLSession = .shared
  ) {
    self.serverURL = serverURL
    socketURL = Self.socketURL(supabaseURL: endpoints.supabaseURL, key: endpoints.supabaseKey)
    channel = Self.channel(accountId: accountId)
    self.tokens = tokens
    self.session = session
  }

  static func socketURL(supabaseURL: URL, key: String) -> URL {
    var components = URLComponents(
      url: supabaseURL.appending(path: "realtime/v1/websocket"), resolvingAgainstBaseURL: false)!
    components.scheme = components.scheme == "http" ? "ws" : "wss"
    components.queryItems = [
      URLQueryItem(name: "apikey", value: key), URLQueryItem(name: "vsn", value: "1.0.0"),
    ]
    return components.url!
  }

  static func channel(accountId: String) -> String { "realtime:sync:\(accountId)" }

  /// Listens until the task is cancelled, telling `signal` what happens.
  /// While the server doesn't broadcast, or the channel can't be joined, it
  /// says `.joined(false)` and tries again later; the scheduler long-polls
  /// meanwhile. Returns when the session is refused for good.
  public func run(_ signal: @escaping @Sendable (Signal) async -> Void) async {
    var failures: UInt32 = 0
    while !Task.isCancelled {
      guard await serverBroadcasts() else {
        try? await Task.sleep(for: .seconds(600))
        continue
      }
      let ended = await listen(signal)
      await signal(.joined(false))
      switch ended {
      case .cancelled, .unauthorized: return
      case .failed(let joined):
        failures = joined ? 1 : failures + 1
        let wait = max(2, Int(syncBackoffSeconds(failures: failures)))
        try? await Task.sleep(for: .seconds(wait))
      }
    }
  }

  private func serverBroadcasts() async -> Bool {
    guard let (data, response) = try? await session.data(from: serverURL.appending(path: "health")),
      (response as? HTTPURLResponse)?.statusCode == 200,
      let health = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    else { return false }
    return health["realtime"] as? Bool == true
  }

  enum Ended: Equatable {
    case cancelled
    case unauthorized
    case failed(joined: Bool)
  }

  /// One connection: join, then read until it breaks.
  private func listen(_ signal: @escaping @Sendable (Signal) async -> Void) async -> Ended {
    let socket = session.webSocketTask(with: socketURL)
    socket.resume()
    defer { socket.cancel(with: .goingAway, reason: nil) }

    let joinRef = "1"
    let sentToken: String
    do {
      sentToken = try await tokens.accessToken()
      try await socket.send(.string(Self.join(channel: channel, token: sentToken, ref: joinRef)))
    } catch SyncError.unauthorized {
      return .unauthorized
    } catch {
      return Task.isCancelled ? .cancelled : .failed(joined: false)
    }

    // Heartbeats, and the token handed on whenever it is refreshed, beside
    // the reads. Ends with the connection.
    let channel = channel
    let tokens = tokens
    let keepAlive = Task { [sentToken] in
      var sentToken = sentToken
      var reference = 1
      while !Task.isCancelled {
        try? await Task.sleep(for: .seconds(25))
        reference += 1
        try? await socket.send(.string(Self.heartbeat(ref: String(reference))))
        if let current = try? await tokens.accessToken(), current != sentToken {
          sentToken = current
          reference += 1
          try? await socket.send(
            .string(Self.accessToken(channel: channel, token: current, ref: String(reference), joinRef: joinRef)))
        }
      }
    }
    defer { keepAlive.cancel() }

    var joined = false
    while true {
      let message: URLSessionWebSocketTask.Message
      do {
        message = try await socket.receive()
      } catch {
        return Task.isCancelled ? .cancelled : .failed(joined: joined)
      }
      guard case .string(let text) = message else { continue }
      switch Self.classify(text, channel: channel, joinRef: joinRef) {
      case .joined:
        joined = true
        await signal(.joined(true))
      case .news:
        await signal(.news)
      case .ended:
        return .failed(joined: joined)
      case .other:
        break
      }
    }
  }

  // MARK: - Frames

  enum Incoming: Equatable {
    case joined
    case ended(String)
    case news
    case other
  }

  static func join(channel: String, token: String, ref: String) -> String {
    frame([
      "topic": channel,
      "event": "phx_join",
      "payload": [
        "config": [
          "broadcast": ["self": false, "ack": false],
          "presence": ["key": ""],
          "postgres_changes": [Any](),
          "private": true,
        ] as [String: Any],
        "access_token": token,
      ] as [String: Any],
      "ref": ref,
      "join_ref": ref,
    ])
  }

  static func heartbeat(ref: String) -> String {
    frame(["topic": "phoenix", "event": "heartbeat", "payload": [String: Any](), "ref": ref])
  }

  static func accessToken(channel: String, token: String, ref: String, joinRef: String) -> String {
    frame([
      "topic": channel, "event": "access_token", "payload": ["access_token": token],
      "ref": ref, "join_ref": joinRef,
    ])
  }

  static func classify(_ text: String, channel: String, joinRef: String) -> Incoming {
    guard let data = text.data(using: .utf8),
      let frame = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      frame["topic"] as? String == channel
    else { return .other }
    let payload = frame["payload"] as? [String: Any] ?? [:]
    let reason =
      (payload["response"] as? [String: Any])?["reason"] as? String
      ?? payload["message"] as? String ?? "refused"
    switch frame["event"] as? String {
    case "phx_reply" where frame["ref"] as? String == joinRef:
      return payload["status"] as? String == "ok" ? .joined : .ended(reason)
    case "broadcast":
      return .news
    case "phx_close":
      return .ended("the channel closed")
    case "phx_error":
      return .ended("the channel failed")
    case "system" where payload["status"] as? String == "error":
      return .ended(reason)
    default:
      return .other
    }
  }

  private static func frame(_ object: [String: Any]) -> String {
    let data = (try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])) ?? Data()
    return String(decoding: data, as: UTF8.self)
  }
}
