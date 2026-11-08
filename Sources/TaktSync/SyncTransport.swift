import Foundation
import TaktRustCore
import TaktWorkspace

// The transport from `docs/sync.md`: HTTP, the token and the device header.
// The bodies themselves are the Rust core's (`core/src/sync/wire.rs`, with
// the server's own structs from `takt-sync-rules`): a push body arrives here
// made, and a pulled page leaves as the server sent it.

/// Where both apps sync unless told otherwise: the hosted server on Railway,
/// and the Supabase project whose accounts it trusts. A self-hosted server
/// goes in the field under "Use a different server"; it has to trust the same
/// Supabase project, since that is who the apps sign in with.
public enum SyncServer {
  public static let defaultURL = URL(string: "https://api.takt.maybeitssoftware.co.uk")!

  /// Where the default was before: the hosted server on Railway, and before that
  /// under Priority. A device signed in there is moved along to the new domain.
  public static let legacyDefaultURLs = [
    URL(string: "https://takt-sync.up.railway.app")!,
    URL(string: "https://priority-sync.up.railway.app")!,
  ]

  /// `url`, or the default if `url` is one of its old addresses.
  public static func current(_ url: URL) -> URL {
    legacyDefaultURLs.contains(url) ? defaultURL : url
  }

  /// The Supabase project accounts live in. The publishable key is meant to
  /// ship in apps: it names the project and grants nothing a signed-out
  /// visitor couldn't already do.
  public static let supabaseURL = URL(string: "https://rsckzmldfpfjdrvulwke.supabase.co")!
  public static let supabasePublishableKey = "sb_publishable_htC171zOquUGx7bBi8MrIQ_ZuIdqL5U"

  /// Where Supabase sends the browser back to after Google or Apple on the
  /// web, and where the links in its emails (confirming an address, resetting
  /// a password) open the app. It must be on the project's allowed redirect
  /// URLs.
  public static let authCallbackURL = URL(string: "takt://auth-callback")!

  /// Whether `url` is Supabase coming back to the app.
  public static func isAuthCallback(_ url: URL) -> Bool {
    url.scheme == authCallbackURL.scheme && url.host() == authCallbackURL.host()
  }

  /// A server address as typed: trimmed, and given `https://` when it has no
  /// scheme. Nil when it still isn't an http(s) URL with a host. The Rust
  /// core's `sync_http_url`, which Android reads addresses with too.
  public static func url(from typed: String) -> URL? {
    syncHttpUrl(typed: typed).flatMap(URL.init(string:))
  }
}

/// `GET /v1/account`: who this device is signed in as, and every device on
/// the account.
public struct SyncAccount: Equatable, Sendable {
  public var accountId: String
  public var email: String?
  public var devices: [SyncDevice]
  /// Takt Sync; nil from a server that predates it.
  public var subscription: SyncSubscription?
  public init(accountId: String, email: String?, devices: [SyncDevice], subscription: SyncSubscription? = nil) {
    self.accountId = accountId
    self.email = email
    self.devices = devices
    self.subscription = subscription
  }
}

/// Takt Sync, the hosted server's £1-a-year subscription, as `GET
/// /v1/account` reports it (docs/sync.md, "Subscription"). A self-hosted
/// server reports `required: false`.
public struct SyncSubscription: Equatable, Sendable, Decodable {
  /// Whether this server charges at all.
  public var required: Bool
  /// Whether this account may sync here now.
  public var active: Bool
  public var expiresAt: String?
  /// Granted rather than bought.
  public var comp: Bool

  public init(required: Bool, active: Bool, expiresAt: String?, comp: Bool) {
    self.required = required
    self.active = active
    self.expiresAt = expiresAt
    self.comp = comp
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    required = try container.decodeIfPresent(Bool.self, forKey: .required) ?? false
    active = try container.decodeIfPresent(Bool.self, forKey: .active) ?? true
    expiresAt = try container.decodeIfPresent(String.self, forKey: .expiresAt)
    comp = try container.decodeIfPresent(Bool.self, forKey: .comp) ?? false
  }

  private enum CodingKeys: String, CodingKey { case required, active, expiresAt, comp }

  public var expiresDate: Date? { expiresAt.flatMap(SyncDate.parse) }

  /// What a lapsed account is told, wherever sync stops for it.
  public static let neededMessage =
    "Hosted sync needs Takt Sync (£1 a year). Subscribe here, or self-host the server."
}

public struct SyncDevice: Equatable, Sendable, Identifiable {
  public var id: String
  public var name: String?
  public var platform: String?
  public var createdAt: String
  public var lastSeenAt: String?
  /// The device asking.
  public var current: Bool

  public init(
    id: String, name: String?, platform: String?, createdAt: String, lastSeenAt: String?, current: Bool
  ) {
    self.id = id
    self.name = name
    self.platform = platform
    self.createdAt = createdAt
    self.lastSeenAt = lastSeenAt
    self.current = current
  }

  public var createdDate: Date? { SyncDate.parse(createdAt) }
  public var lastSeenDate: Date? { lastSeenAt.flatMap(SyncDate.parse) }

  /// The platform in words, for a device list.
  public var platformName: String {
    switch platform {
    case "macos": "Mac"
    case "ios": "iPhone"
    case "android": "Android"
    case let other?: other
    case nil: "Device"
    }
  }

  /// The name, or the platform when the device never gave one.
  public var displayName: String {
    guard let name, !name.isEmpty else { return platformName }
    return name
  }
}

/// The server's timestamps are RFC 3339 with up to nine fractional digits
/// (chrono's default), read by the Rust core to the millisecond.
public enum SyncDate {
  public static func parse(_ string: String) -> Date? {
    syncTimestampMs(text: string).map { Date(timeIntervalSince1970: Double($0) / 1000) }
  }
}

extension SyncAccount {
  /// `GET /v1/account`'s body, read by the core.
  init(body: Data) throws {
    let account = try syncDecodeAccount(body: String(decoding: body, as: UTF8.self))
    struct Wrapper: Decodable { var subscription: SyncSubscription? }
    let subscription = (try? JSONDecoder().decode(Wrapper.self, from: body))?.subscription
    self.init(
      accountId: account.accountId, email: account.email,
      devices: account.devices.map {
        SyncDevice(
          id: $0.id, name: $0.name, platform: $0.platform, createdAt: $0.createdAt, lastSeenAt: $0.lastSeenAt,
          current: $0.current)
      },
      subscription: subscription)
  }
}

/// What a device remembers about its sync sign-in, kept outside the database:
/// in the Keychain on Apple platforms. The secret itself, the Supabase
/// session, is kept by the Supabase client in an item of its own.
public struct SyncCredentials: Codable, Equatable, Sendable {
  public var serverURL: URL
  public var deviceId: String
  public var accountId: String?
  public var email: String?
  /// Supabase refused to refresh the session: signed out from elsewhere, the
  /// password changed, or the account deleted. Kept rather than cleared, so
  /// the sign-in form can offer the same email and server again.
  public var isSignedOut: Bool
  /// Saved before accounts moved to Supabase, with a token from the old
  /// server that no server accepts now. Read, never written.
  public private(set) var hasLegacyToken = false

  public init(
    serverURL: URL, deviceId: String, accountId: String? = nil, email: String? = nil, isSignedOut: Bool = false
  ) {
    self.serverURL = serverURL
    self.deviceId = deviceId
    self.accountId = accountId
    self.email = email
    self.isSignedOut = isSignedOut
  }

  private enum CodingKeys: String, CodingKey {
    case serverURL, deviceId, accountId, email, isSignedOut, token
  }

  // An item saved by an older build may hold a token, and may lack the
  // account, the email and the signed-out flag.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    serverURL = SyncServer.current(try container.decode(URL.self, forKey: .serverURL))
    deviceId = try container.decode(String.self, forKey: .deviceId)
    accountId = try container.decodeIfPresent(String.self, forKey: .accountId)
    email = try container.decodeIfPresent(String.self, forKey: .email)
    isSignedOut = try container.decodeIfPresent(Bool.self, forKey: .isSignedOut) ?? false
    hasLegacyToken = try container.decodeIfPresent(String.self, forKey: .token) != nil
  }

  public func encode(to encoder: any Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(serverURL, forKey: .serverURL)
    try container.encode(deviceId, forKey: .deviceId)
    try container.encodeIfPresent(accountId, forKey: .accountId)
    try container.encodeIfPresent(email, forKey: .email)
    try container.encode(isSignedOut, forKey: .isSignedOut)
  }
}

/// Moves a sync cycle's bodies, which the core makes and reads: a push body
/// out, and each page of the feed back as the server sent it.
public protocol SyncTransport: Sendable {
  /// Sends a `POST /v1/push` body.
  func push(body: Data) async throws
  /// One `GET /v1/changes` page's body.
  func changes(since cursor: Int64, limit: Int, wait: Int) async throws -> Data
}

/// Where the transport gets the access token every request carries: the
/// Supabase client in the apps (`SupabaseSyncAuth`), a fake in tests.
public protocol SyncAccessTokenProvider: Sendable {
  /// A token good for now: the stored one, or a refreshed one if it has
  /// expired. Throws `SyncError.unauthorized` when there is no session left.
  func accessToken() async throws -> String
  /// A new token, whether or not the old one looked expired, because the
  /// server refused it. Throws `SyncError.unauthorized` when Supabase refuses
  /// the refresh, and anything else (offline, say) as it is.
  func refreshedAccessToken() async throws -> String
}

public enum SyncError: LocalizedError, Equatable {
  /// The server said no; `message` is its `{"error": "..."}` text.
  case server(status: Int, message: String)
  /// The session is gone: signed out elsewhere, or the account deleted.
  case unauthorized
  case notPaired
  /// Something the app caught before asking anyone.
  case invalid(String)
  /// The person closed the sign-in window. Not worth a message.
  case cancelled
  /// `402`: the hosted server wants Takt Sync, and this account hasn't it.
  case subscriptionRequired

  public var errorDescription: String? {
    switch self {
    case .server(let status, let message):
      return syncRefusalText(status: Int32(clamping: status), message: message)
    case .unauthorized: return "Signed out. Sign in again to keep syncing."
    case .notPaired: return "This device isn't signed in to sync."
    case .invalid(let message): return message
    case .cancelled: return "Sign-in was cancelled."
    case .subscriptionRequired: return SyncSubscription.neededMessage
    }
  }
}

/// The transport over HTTPS, and the account routes beside it.
///
/// Every request carries `Authorization: Bearer <Supabase access token>` and
/// `X-Priority-Device: <device id>`. The token is asked for before each
/// request, so an expired one is refreshed first. A `401` all the same means
/// the server disagrees, so the token is refreshed once and the request sent
/// again. Only a refused refresh, or a second `401`, is
/// `SyncError.unauthorized`.
public struct HTTPSyncTransport: SyncTransport {
  public static let deviceHeader = "X-Priority-Device"

  public let serverURL: URL
  public let deviceId: String
  let tokens: any SyncAccessTokenProvider
  let session: URLSession

  public init(
    serverURL: URL, deviceId: String, tokens: any SyncAccessTokenProvider, session: URLSession = .shared
  ) {
    self.serverURL = serverURL
    self.deviceId = deviceId
    self.tokens = tokens
    self.session = session
  }

  public func push(body: Data) async throws {
    _ = try await send(post("v1/push", body: body))
  }

  public func changes(since cursor: Int64, limit: Int, wait: Int) async throws -> Data {
    var components = URLComponents(url: serverURL.appending(path: "v1/changes"), resolvingAgainstBaseURL: false)!
    components.queryItems = [
      URLQueryItem(name: "since", value: String(cursor)),
      URLQueryItem(name: "limit", value: String(limit)),
      URLQueryItem(name: "wait", value: String(wait)),
    ]
    var request = URLRequest(url: components.url!)
    // A long-poll holds the request open on purpose.
    request.timeoutInterval = TimeInterval(wait + 30)
    return try await send(request)
  }

  /// Records this device on the account. Sent after every sign-in.
  public func registerDevice(name: String, platform: String) async throws {
    let body = syncRegisterDeviceBody(id: deviceId, name: name, platform: platform)
    _ = try await send(post("v1/devices", body: Data(body.utf8)))
  }

  /// Takes this device off the account's list.
  public func signOut() async throws {
    _ = try await send(post("v1/sign-out", body: Self.emptyObject))
  }

  /// Who this device is signed in as, and the account's devices.
  public func account() async throws -> SyncAccount {
    try SyncAccount(body: await send(URLRequest(url: serverURL.appending(path: "v1/account"))))
  }

  /// Deletes the account: its rows, its devices and the Supabase user. The
  /// apps ask the person to confirm first.
  public func deleteAccount() async throws {
    _ = try await send(post("v1/account/delete", body: Self.emptyObject))
  }

  /// Any other request to the server: the shared-list routes, whose paths
  /// and bodies the core makes.
  public func request(_ method: String, _ path: String, body: Data?) async throws -> Data {
    var request = URLRequest(url: serverURL.appending(path: path))
    request.httpMethod = method
    if let body {
      request.setValue("application/json", forHTTPHeaderField: "Content-Type")
      request.httpBody = body
    }
    return try await send(request)
  }

  private static let emptyObject = Data("{}".utf8)

  private func post(_ path: String, body: Data) -> URLRequest {
    var request = URLRequest(url: serverURL.appending(path: path))
    request.httpMethod = "POST"
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.httpBody = body
    return request
  }

  /// Sends `request` with a token, and once more with a refreshed one if the
  /// server refuses the first, and returns the answer's body. A refusal
  /// carries the server's own `{"error": "..."}` message.
  private func send(_ request: URLRequest) async throws -> Data {
    var (data, status) = try await send(request, token: tokens.accessToken())
    if status == 401 {
      (data, status) = try await send(request, token: tokens.refreshedAccessToken())
      if status == 401 { throw SyncError.unauthorized }
    }
    if status == 402 { throw SyncError.subscriptionRequired }
    guard (200..<300).contains(status) else {
      throw SyncError.server(status: status, message: syncServerMessage(body: String(decoding: data, as: UTF8.self)))
    }
    return data
  }

  private func send(_ request: URLRequest, token: String) async throws -> (Data, Int) {
    var request = request
    request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    request.setValue(deviceId, forHTTPHeaderField: Self.deviceHeader)
    let (data, response) = try await session.data(for: request)
    return (data, (response as? HTTPURLResponse)?.statusCode ?? 0)
  }
}
