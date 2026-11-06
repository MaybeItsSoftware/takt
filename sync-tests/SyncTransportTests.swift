import Auth
import Foundation
@testable import TaktSync
import TaktWorkspace
import XCTest

/// The account routes from `docs/sync.md` ("Wire protocol", "Devices and the
/// account") as the transport sends them, and what the session does with the
/// answers: the Supabase token on every request, one refresh-and-retry on a
/// 401, and a refused refresh stopping sync rather than retrying. Supabase
/// itself is a fake (`FakeAuth`), so nothing reaches the network.
final class SyncTransportTests: XCTestCase {
  private let server = URL(string: "https://sync.example.com")!
  private let deviceID = "5b0c2f0e-8a0e-4c1e-9d5e-0a1b2c3d4e5f"
  private var urlSession: URLSession!
  private var directory: URL!

  override func setUpWithError() throws {
    StubURLProtocol.reset()
    let configuration = URLSessionConfiguration.ephemeral
    configuration.protocolClasses = [StubURLProtocol.self]
    urlSession = URLSession(configuration: configuration)
    directory = FileManager.default.temporaryDirectory.appendingPathComponent("SyncTransport-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
  }

  override func tearDownWithError() throws {
    StubURLProtocol.reset()
    try? FileManager.default.removeItem(at: directory)
  }

  private func credentials() -> SyncCredentials {
    SyncCredentials(serverURL: server, deviceId: deviceID, accountId: "acc-1", email: "me@example.com")
  }

  private func transport(_ auth: FakeAuth) -> HTTPSyncTransport {
    HTTPSyncTransport(serverURL: server, deviceId: deviceID, tokens: auth, session: urlSession)
  }

  // MARK: - Transport

  private static let emptyPush = Data(#"{"changes":[]}"#.utf8)

  func testEveryRequestCarriesTheAccessTokenAndTheDevice() async throws {
    let auth = FakeAuth(token: "jwt-1")
    _ = try await transport(auth).push(body: Self.emptyPush)
    _ = try await transport(auth).changes(since: 0, limit: 10, wait: 0)
    try await transport(auth).registerDevice(name: "Mac", platform: "macos")

    XCTAssertEqual(StubURLProtocol.requests.map(\.path), ["/v1/push", "/v1/changes", "/v1/devices"])
    for request in StubURLProtocol.requests {
      XCTAssertEqual(request.authorization, "Bearer jwt-1")
      XCTAssertEqual(request.device, deviceID)
    }
    XCTAssertEqual(
      StubURLProtocol.requests.last?.json, ["id": deviceID, "name": "Mac", "platform": "macos"])
    XCTAssertEqual(auth.refreshes, 0)
  }

  func testA401IsRefreshedOnceAndRetried() async throws {
    StubURLProtocol.respondOnce(to: "/v1/push", status: 401, body: #"{"error":"unauthorized"}"#)
    let auth = FakeAuth(token: "stale", refreshed: "fresh")

    try await transport(auth).push(body: Self.emptyPush)
    XCTAssertEqual(auth.refreshes, 1)
    XCTAssertEqual(StubURLProtocol.requests.map(\.authorization), ["Bearer stale", "Bearer fresh"])
    XCTAssertEqual(StubURLProtocol.requests.map(\.device), [deviceID, deviceID])
  }

  func testA401AfterTheRefreshIsSignedOut() async throws {
    StubURLProtocol.respond(to: "/v1/push", status: 401, body: #"{"error":"unauthorized"}"#)
    let auth = FakeAuth(token: "stale", refreshed: "fresh")
    do {
      _ = try await transport(auth).push(body: Self.emptyPush)
      XCTFail("pushed with a token the server refuses")
    } catch {
      XCTAssertEqual(error as? SyncError, .unauthorized)
    }
    XCTAssertEqual(auth.refreshes, 1)
    XCTAssertEqual(StubURLProtocol.requests.count, 2, "one retry, not a loop")
  }

  func testARefusedRefreshIsSignedOutWithoutARetry() async throws {
    StubURLProtocol.respond(to: "/v1/push", status: 401, body: #"{"error":"unauthorized"}"#)
    let auth = FakeAuth(token: "stale", refreshFails: SyncError.unauthorized)
    do {
      _ = try await transport(auth).push(body: Self.emptyPush)
      XCTFail("pushed after the refresh was refused")
    } catch {
      XCTAssertEqual(error as? SyncError, .unauthorized)
    }
    XCTAssertEqual(StubURLProtocol.requests.count, 1)
  }

  func testRefusalsCarryTheServersOwnMessage() async throws {
    StubURLProtocol.respond(
      to: "/v1/account/delete", status: 503, body: #"{"error":"deleting accounts isn't set up on this server"}"#)
    do {
      try await transport(FakeAuth()).deleteAccount()
      XCTFail("a 503 deleted the account")
    } catch {
      XCTAssertEqual(error.localizedDescription, "Deleting accounts isn't set up on this server.")
    }
    StubURLProtocol.respond(to: "/v1/account", status: 502, body: "<html>bad gateway</html>")
    do {
      _ = try await transport(FakeAuth()).account()
      XCTFail("a 502 read the account")
    } catch {
      XCTAssertEqual(error.localizedDescription, "The sync server answered 502.")
    }
  }

  func testAccountListsDevicesWithTheServersTimestamps() async throws {
    StubURLProtocol.respond(
      to: "/v1/account", status: 200,
      body: """
        {"accountId":"acc-1","email":"me@example.com","devices":[
          {"id":"dev-1","name":"Mac","platform":"macos","createdAt":"2026-10-03T09:00:00.123456789Z",
           "lastSeenAt":"2026-10-03T10:00:00Z","current":true},
          {"id":"dev-2","name":null,"platform":"android","createdAt":"2026-10-03T09:30:00.5Z",
           "lastSeenAt":null,"current":false}]}
        """)
    let account = try await transport(FakeAuth()).account()

    XCTAssertEqual(StubURLProtocol.requests.first?.method, "GET")
    XCTAssertEqual(account.email, "me@example.com")
    XCTAssertEqual(account.devices.map(\.current), [true, false])
    XCTAssertEqual(account.devices[1].displayName, "Android")
    let created = try XCTUnwrap(account.devices[0].createdDate)
    XCTAssertEqual(created.timeIntervalSince1970, 1_791_018_000.123, accuracy: 0.001)
    XCTAssertNotNil(account.devices[0].lastSeenDate)
    XCTAssertNotNil(account.devices[1].createdDate)
    XCTAssertNil(account.devices[1].lastSeenDate)
  }

  func testDeletingAndSigningOutPostNothingButTheToken() async throws {
    let transport = transport(FakeAuth())
    try await transport.deleteAccount()
    try await transport.signOut()
    XCTAssertEqual(StubURLProtocol.requests.map(\.path), ["/v1/account/delete", "/v1/sign-out"])
    XCTAssertEqual(StubURLProtocol.requests.map(\.method), ["POST", "POST"])
    XCTAssertEqual(StubURLProtocol.requests.first?.json, [:])
  }

  func testSupabaseRefusingARefreshIsSignedOutButBeingOfflineIsNot() {
    XCTAssertEqual(SupabaseSyncAuth.refreshError(AuthError.sessionMissing) as? SyncError, .unauthorized)
    let refused = HTTPURLResponse(url: server, statusCode: 400, httpVersion: nil, headerFields: nil)!
    let invalid = AuthError.api(
      message: "Invalid Refresh Token", errorCode: .refreshTokenNotFound, underlyingData: Data(),
      underlyingResponse: refused)
    XCTAssertEqual(SupabaseSyncAuth.refreshError(invalid) as? SyncError, .unauthorized)
    let offline = URLError(.notConnectedToInternet)
    XCTAssertEqual(SupabaseSyncAuth.refreshError(offline) as? URLError, offline)
  }

  func testCredentialsFromTheOldServerLoseTheirToken() throws {
    let old = #"{"serverURL":"https://sync.example.com","deviceId":"dev-1","token":"tok-1","email":"me@example.com"}"#
    let decoded = try JSONDecoder().decode(SyncCredentials.self, from: Data(old.utf8))
    XCTAssertTrue(decoded.hasLegacyToken)
    XCTAssertEqual(decoded.email, "me@example.com")
    XCTAssertFalse(decoded.isSignedOut)
    let saved = try XCTUnwrap(String(bytes: try JSONEncoder().encode(decoded), encoding: .utf8))
    XCTAssertFalse(saved.contains("tok-1"))
  }

  func testCredentialsSavedAgainstTheOldDefaultMoveToTheNewOne() throws {
    let saved = #"{"serverURL":"https://priority-sync.up.railway.app","deviceId":"dev-1"}"#
    let decoded = try JSONDecoder().decode(SyncCredentials.self, from: Data(saved.utf8))
    XCTAssertEqual(decoded.serverURL, SyncServer.defaultURL)
    // A server of the user's own stays theirs.
    let own = #"{"serverURL":"https://sync.example.com","deviceId":"dev-1"}"#
    let kept = try JSONDecoder().decode(SyncCredentials.self, from: Data(own.utf8))
    XCTAssertEqual(kept.serverURL.absoluteString, "https://sync.example.com")
  }

  func testATypedServerAddressGetsAScheme() {
    XCTAssertEqual(SyncServer.url(from: " sync.example.com "), URL(string: "https://sync.example.com"))
    XCTAssertEqual(SyncServer.url(from: "http://localhost:8080"), URL(string: "http://localhost:8080"))
    XCTAssertNil(SyncServer.url(from: ""))
    XCTAssertNil(SyncServer.url(from: "ftp://example.com"))
    XCTAssertEqual(SyncServer.defaultURL.absoluteString, "https://api.takt.maybeitssoftware.co.uk")
    XCTAssertEqual(SyncServer.current(URL(string: "https://takt-sync.up.railway.app")!), SyncServer.defaultURL)
    XCTAssertEqual(SyncServer.current(URL(string: "https://priority-sync.up.railway.app")!), SyncServer.defaultURL)
    XCTAssertTrue(SyncServer.isAuthCallback(URL(string: "takt://auth-callback?code=abc")!))
    XCTAssertFalse(SyncServer.isAuthCallback(URL(string: "takt://today")!))
    // The scheme from before the rename is no longer the app's.
    XCTAssertFalse(SyncServer.isAuthCallback(URL(string: "priority://auth-callback?code=abc")!))
  }

  func testTheAppleNonceIsHashedAsSupabaseChecksIt() {
    XCTAssertEqual(
      SyncAppleNonce.sha256("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    XCTAssertEqual(SyncAppleNonce.make().count, 32)
    XCTAssertNotEqual(SyncAppleNonce.make(), SyncAppleNonce.make())
  }

  // MARK: - Session

  private func store() throws -> WorkspaceStore {
    let store = try WorkspaceStore(databaseURL: directory.appendingPathComponent("\(UUID().uuidString).sqlite"))
    _ = try store.bootstrapIfNeeded()
    return store
  }

  @MainActor
  private func session(
    _ store: WorkspaceStore, _ keychain: InMemorySyncCredentialStore, _ auth: FakeAuth
  ) -> SyncSession {
    SyncSession(
      store: store, credentialStore: keychain, auth: auth, deviceName: "Mac", platform: "macos",
      urlSession: urlSession)
  }

  @MainActor
  func testSigningInRegistersTheDeviceAndSigningOutForgetsIt() async throws {
    let store = try store()
    let keychain = InMemorySyncCredentialStore(deviceId: deviceID)
    let auth = FakeAuth(user: SyncAuthUser(id: "acc-1", email: "me@example.com"))
    let session = session(store, keychain, auth)

    try await session.signIn(email: "  me@example.com ", password: "correct horse", serverURL: server)
    session.deactivate()
    XCTAssertEqual(auth.signIns, ["me@example.com"])
    XCTAssertTrue(session.isSignedIn)
    XCTAssertEqual(session.email, "me@example.com")
    XCTAssertEqual(keychain.load(), credentials())
    XCTAssertEqual(try store.syncState()?.deviceId, deviceID)
    let register = try XCTUnwrap(StubURLProtocol.requests.first { $0.path == "/v1/devices" })
    XCTAssertEqual(register.json, ["id": deviceID, "name": "Mac", "platform": "macos"])

    await session.signOut()
    XCTAssertEqual(session.phase, .unpaired)
    XCTAssertNil(keychain.load())
    XCTAssertNil(try store.syncState())
    XCTAssertEqual(auth.signOuts, 1)
    XCTAssertNotNil(StubURLProtocol.requests.last { $0.path == "/v1/sign-out" })
    // The device keeps its id for next time.
    XCTAssertEqual(keychain.loadDeviceId(), deviceID)
  }

  @MainActor
  func testAServerThatCantRegisterTheDeviceLeavesItSignedOut() async throws {
    StubURLProtocol.respond(to: "/v1/devices", status: 500, body: #"{"error":"internal error"}"#)
    let keychain = InMemorySyncCredentialStore(deviceId: deviceID)
    let auth = FakeAuth(user: SyncAuthUser(id: "acc-1", email: "me@example.com"))
    let session = session(try store(), keychain, auth)
    do {
      try await session.signIn(email: "me@example.com", password: "pw", serverURL: server)
      XCTFail("signed in without the server")
    } catch {
      XCTAssertEqual(error.localizedDescription, "Internal error.")
    }
    XCTAssertFalse(session.isSignedIn)
    XCTAssertEqual(auth.signOuts, 1, "the Supabase session doesn't outlive the failure")
    XCTAssertNil(keychain.load())
  }

  @MainActor
  func testASignUpWaitingOnConfirmationSaysSoAndRegistersNothing() async throws {
    let auth = FakeAuth(user: nil)
    let session = session(try store(), InMemorySyncCredentialStore(), auth)
    let outcome = try await session.signUp(email: " new@example.com", password: "correct horse", serverURL: server)
    XCTAssertEqual(outcome, .confirmEmail("new@example.com"))
    XCTAssertFalse(session.isSignedIn)
    XCTAssertEqual(session.rememberedEmail, "new@example.com")
    XCTAssertTrue(StubURLProtocol.requests.isEmpty)
  }

  @MainActor
  func testEmptyFieldsAreCaughtBeforeSupabase() async throws {
    let auth = FakeAuth()
    let session = session(try store(), InMemorySyncCredentialStore(), auth)
    do {
      _ = try await session.signUp(email: " ", password: "x", serverURL: server)
      XCTFail("signed up without an email")
    } catch {
      XCTAssertEqual(error.localizedDescription, "Enter your email address.")
    }
    do {
      try await session.requestPasswordReset(email: "  ")
      XCTFail("asked for a reset without an email")
    } catch {
      XCTAssertEqual(error.localizedDescription, "Enter your email first.")
    }
    XCTAssertTrue(auth.signIns.isEmpty)
    XCTAssertTrue(auth.resets.isEmpty)
  }

  @MainActor
  func testAResetLinkSignsInAndAsksForTheNewPassword() async throws {
    let auth = FakeAuth(user: SyncAuthUser(id: "acc-1", email: "me@example.com"))
    let session = session(try store(), InMemorySyncCredentialStore(deviceId: deviceID), auth)

    let sent = try await session.requestPasswordReset(email: " me@example.com ")
    XCTAssertEqual(auth.resets, ["me@example.com"])
    XCTAssertEqual(
      SyncSession.passwordResetSentMessage(for: sent),
      "If there's an account for me@example.com, we've sent it a link to reset the password. Open it on this device.")

    try await session.completeSignIn(from: URL(string: "takt://auth-callback?code=abc")!)
    session.deactivate()
    XCTAssertTrue(session.isSignedIn)
    XCTAssertTrue(session.needsNewPassword)
    try await session.setNewPassword("new password")
    XCTAssertEqual(auth.passwords, ["new password"])
    XCTAssertFalse(session.needsNewPassword)
  }

  @MainActor
  func testARefusedRefreshStopsSyncAndAsksForSignInAgain() async throws {
    StubURLProtocol.respond(to: "/v1/push", status: 401, body: #"{"error":"unauthorized"}"#)
    StubURLProtocol.respond(to: "/v1/changes", status: 401, body: #"{"error":"unauthorized"}"#)
    let store = try store()
    try store.beginSync(deviceId: deviceID, serverURL: server.absoluteString)
    let keychain = InMemorySyncCredentialStore(credentials(), deviceId: deviceID)
    let auth = FakeAuth(refreshFails: SyncError.unauthorized)
    let session = session(store, keychain, auth)
    XCTAssertTrue(session.isSignedIn)

    let synced = await session.syncNow()
    XCTAssertFalse(synced)
    XCTAssertEqual(session.phase, .needsSignIn)
    XCTAssertFalse(session.isSignedIn)
    XCTAssertEqual(session.rememberedEmail, "me@example.com")
    XCTAssertEqual(session.rememberedServerURL, server)
    XCTAssertEqual(keychain.load()?.isSignedOut, true)
    // The outbox keeps recording, so what changes meanwhile goes up later.
    XCTAssertNotNil(try store.syncState())

    // No further attempts: there is no session to try.
    let attempts = StubURLProtocol.requests.count
    session.activate()
    let retried = await session.syncNow()
    XCTAssertFalse(retried)
    XCTAssertEqual(StubURLProtocol.requests.count, attempts)

    // And it is still signed out after a relaunch.
    let relaunched = self.session(store, keychain, FakeAuth())
    XCTAssertEqual(relaunched.phase, .needsSignIn)
    XCTAssertEqual(relaunched.rememberedEmail, "me@example.com")
  }

  @MainActor
  func testARefreshThatFailsOfflineIsNotASignOut() async throws {
    StubURLProtocol.respond(to: "/v1/account", status: 401, body: #"{"error":"unauthorized"}"#)
    let store = try store()
    try store.beginSync(deviceId: deviceID, serverURL: server.absoluteString)
    let auth = FakeAuth(refreshFails: URLError(.notConnectedToInternet))
    let session = session(store, InMemorySyncCredentialStore(credentials(), deviceId: deviceID), auth)
    do {
      try await session.refreshAccount()
      XCTFail("read the account without a token")
    } catch {
      XCTAssertEqual((error as? URLError)?.code, .notConnectedToInternet)
    }
    XCTAssertTrue(session.isSignedIn)
  }

  @MainActor
  func testAnAccountCallThatFindsTheSessionGoneSignsOutToo() async throws {
    StubURLProtocol.respond(to: "/v1/account", status: 401, body: #"{"error":"unauthorized"}"#)
    let store = try store()
    try store.beginSync(deviceId: deviceID, serverURL: server.absoluteString)
    let auth = FakeAuth(refreshFails: SyncError.unauthorized)
    let session = session(store, InMemorySyncCredentialStore(credentials(), deviceId: deviceID), auth)
    do {
      try await session.refreshAccount()
      XCTFail("read the account with a dead session")
    } catch {
      XCTAssertEqual(error as? SyncError, .unauthorized)
    }
    XCTAssertEqual(session.phase, .needsSignIn)
  }

  @MainActor
  func testDeletingTheAccountKeepsTheWorkspaceAndLeavesSync() async throws {
    let store = try store()
    let workspaces = try store.workspaces()
    try store.beginSync(deviceId: deviceID, serverURL: server.absoluteString)
    let keychain = InMemorySyncCredentialStore(credentials(), deviceId: deviceID)
    let auth = FakeAuth()
    let session = session(store, keychain, auth)

    try await session.deleteAccount()
    XCTAssertEqual(StubURLProtocol.requests.map(\.path), ["/v1/account/delete"])
    XCTAssertEqual(session.phase, .unpaired)
    XCTAssertEqual(auth.signOuts, 1)
    XCTAssertNil(keychain.load())
    XCTAssertNil(try store.syncState())
    XCTAssertEqual(try store.workspaces(), workspaces)
  }

  @MainActor
  func testADeviceSignedInToTheOldServerAsksToSignInAgainAndKeepsItsId() throws {
    let store = try store()
    try store.beginSync(deviceId: deviceID, serverURL: server.absoluteString)
    let old = #"{"serverURL":"https://sync.example.com","deviceId":"\#(deviceID)","token":"tok-1","email":"me@example.com"}"#
    let keychain = InMemorySyncCredentialStore(try JSONDecoder().decode(SyncCredentials.self, from: Data(old.utf8)))

    let session = session(store, keychain, FakeAuth())
    XCTAssertEqual(session.phase, .needsSignIn)
    XCTAssertEqual(session.rememberedEmail, "me@example.com")
    XCTAssertEqual(session.deviceId, deviceID)
    XCTAssertEqual(keychain.loadDeviceId(), deviceID)
    XCTAssertEqual(keychain.load()?.hasLegacyToken, false, "the token is gone from the keychain")
    XCTAssertEqual(keychain.load()?.isSignedOut, true)
  }

  @MainActor
  func testANewDeviceMakesAnIdOnceAndKeepsIt() throws {
    let keychain = InMemorySyncCredentialStore()
    let first = session(try store(), keychain, FakeAuth()).deviceId
    let second = session(try store(), keychain, FakeAuth()).deviceId
    XCTAssertNotNil(UUID(uuidString: first))
    XCTAssertEqual(first, second)
  }

  func testTheSchedulerStopsAfterARefusedToken() async throws {
    let transport = RefusingTransport()
    let store = try store()
    try store.beginSync(deviceId: "dev-1", serverURL: "memory://")
    let engine = SyncEngine(store: store, transport: transport, deviceId: "dev-1")
    let statuses = StatusLog()
    let scheduler = SyncScheduler(engine: engine) { status, _ in await statuses.append(status) }

    let first = await scheduler.syncNow()
    let second = await scheduler.syncNow()
    await scheduler.start()
    XCTAssertFalse(first)
    XCTAssertFalse(second)
    XCTAssertEqual(transport.calls, 1)
    let isSignedOut = await scheduler.isSignedOut
    XCTAssertTrue(isSignedOut)
    let last = await statuses.last
    XCTAssertEqual(last, .signedOut)
    let engineStatus = await engine.status
    XCTAssertEqual(engineStatus, .signedOut)
  }
}

extension SyncTransportTests {
  func testTheSchedulerHoldsNoLongPollInTheBackground() async throws {
    let transport = RecordingTransport()
    let store = try store()
    try store.beginSync(deviceId: "dev-1", serverURL: "memory://")
    let engine = SyncEngine(store: store, transport: transport, deviceId: "dev-1")
    let scheduler = SyncScheduler(engine: engine) { _, _ in }

    await scheduler.background(every: .seconds(60))
    await scheduler.start()
    try await Task.sleep(for: .milliseconds(300))
    XCTAssertEqual(transport.waits, [0], "one quick cycle, then nothing until the interval")

    await scheduler.background(every: nil)
    try await Task.sleep(for: .milliseconds(300))
    XCTAssertEqual(transport.waits, [0, 25], "back in front: a sync, held open as a long-poll")
    await scheduler.stop()
  }
}

/// A quiet server that records each pull's `wait`, and holds a long-poll
/// open until it is cancelled.
private final class RecordingTransport: SyncTransport, @unchecked Sendable {
  private let lock = NSLock()
  private var log: [Int] = []
  var waits: [Int] { lock.withLock { log } }

  func push(body: Data) async throws {}

  func changes(since cursor: Int64, limit: Int, wait: Int) async throws -> Data {
    lock.withLock { log.append(wait) }
    if wait > 0 { try await Task.sleep(for: .seconds(wait)) }
    return Data(#"{"rows":[],"cursor":0,"hasMore":false}"#.utf8)
  }
}

/// Supabase, as far as the session needs it, without the network.
final class FakeAuth: SyncAuthenticating, @unchecked Sendable {
  private let lock = NSLock()
  private let token: String
  private let refreshed: String
  private let refreshFails: (any Error)?
  private var user: SyncAuthUser?
  private var refreshCount = 0
  private var signInLog: [String] = []
  private var resetLog: [String] = []
  private var passwordLog: [String] = []
  private var signOutCount = 0

  init(
    token: String = "jwt", refreshed: String = "jwt-refreshed", refreshFails: (any Error)? = nil,
    user: SyncAuthUser? = SyncAuthUser(id: "acc-1", email: "me@example.com")
  ) {
    self.token = token
    self.refreshed = refreshed
    self.refreshFails = refreshFails
    self.user = user
  }

  var refreshes: Int { lock.withLock { refreshCount } }
  var signIns: [String] { lock.withLock { signInLog } }
  var resets: [String] { lock.withLock { resetLog } }
  var passwords: [String] { lock.withLock { passwordLog } }
  var signOuts: Int { lock.withLock { signOutCount } }

  var currentUser: SyncAuthUser? { lock.withLock { user } }

  func accessToken() async throws -> String { token }

  func refreshedAccessToken() async throws -> String {
    lock.withLock { refreshCount += 1 }
    if let refreshFails { throw refreshFails }
    return refreshed
  }

  func signIn(email: String, password: String) async throws -> SyncAuthUser {
    try lock.withLock {
      signInLog.append(email)
      guard let user else { throw SyncError.invalid("Invalid login credentials") }
      return user
    }
  }

  func signUp(email: String, password: String) async throws -> SyncAuthUser? {
    lock.withLock {
      signInLog.append(email)
      return user
    }
  }

  func resetPassword(email: String) async throws { lock.withLock { resetLog.append(email) } }

  func signIn(with provider: SyncOAuthProvider) async throws -> SyncAuthUser {
    guard let user = currentUser else { throw SyncError.cancelled }
    return user
  }

  func signInWithApple(idToken: String, nonce: String) async throws -> SyncAuthUser {
    try await signIn(with: .apple)
  }

  func signIn(fromCallback url: URL) async throws -> SyncAuthUser {
    try await signIn(with: .google)
  }

  func updatePassword(_ password: String) async throws { lock.withLock { passwordLog.append(password) } }

  func signOut() async { lock.withLock { signOutCount += 1 } }
}

private actor StatusLog {
  var all: [SyncEngine.Status] = []
  var last: SyncEngine.Status? { all.last }
  func append(_ status: SyncEngine.Status) { all.append(status) }
}

/// A server that has forgotten the device.
private final class RefusingTransport: SyncTransport, @unchecked Sendable {
  private let lock = NSLock()
  private var count = 0
  var calls: Int { lock.withLock { count } }

  func push(body: Data) async throws {
    lock.withLock { count += 1 }
    throw SyncError.unauthorized
  }

  func changes(since cursor: Int64, limit: Int, wait: Int) async throws -> Data {
    lock.withLock { count += 1 }
    throw SyncError.unauthorized
  }
}

/// Answers requests from a table of canned responses keyed by path, and
/// records what was asked. A one-off answer goes first; paths without an
/// answer get an empty sync page or `{"ok":true}`, so a session that starts
/// its rhythm in a test has something to talk to.
final class StubURLProtocol: URLProtocol, @unchecked Sendable {
  struct Recorded {
    var method: String
    var host: String?
    var path: String
    var authorization: String?
    var device: String?
    var body: Data?
    var json: [String: String]? {
      body.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: String] }
    }
  }

  private static let lock = NSLock()
  nonisolated(unsafe) private static var responses: [String: (Int, String)] = [:]
  nonisolated(unsafe) private static var onceResponses: [String: [(Int, String)]] = [:]
  nonisolated(unsafe) private static var recorded: [Recorded] = []

  static var requests: [Recorded] { lock.withLock { recorded } }

  static func respond(to path: String, status: Int, body: String) {
    lock.withLock { responses[path] = (status, body) }
  }

  static func respondOnce(to path: String, status: Int, body: String) {
    lock.withLock { onceResponses[path, default: []].append((status, body)) }
  }

  static func reset() {
    lock.withLock {
      responses = [:]
      onceResponses = [:]
      recorded = []
    }
  }

  override static func canInit(with request: URLRequest) -> Bool { true }
  override static func canonicalRequest(for request: URLRequest) -> URLRequest { request }

  override func startLoading() {
    let url = request.url!
    let path = url.path()
    let body = request.httpBody ?? request.httpBodyStream.map(Self.read)
    let canned = Self.lock.withLock { () -> (Int, String)? in
      Self.recorded.append(
        Recorded(
          method: request.httpMethod ?? "GET", host: url.host(), path: path,
          authorization: request.value(forHTTPHeaderField: "Authorization"),
          device: request.value(forHTTPHeaderField: "X-Priority-Device"), body: body))
      if var once = Self.onceResponses[path], !once.isEmpty {
        let first = once.removeFirst()
        Self.onceResponses[path] = once
        return first
      }
      return Self.responses[path]
    }
    let (status, text) = canned ?? Self.fallback(for: path)
    let response = HTTPURLResponse(url: url, statusCode: status, httpVersion: "HTTP/1.1", headerFields: nil)!
    // A long-poll answered at once would spin the session's loop; hold it.
    let delay: TimeInterval = path == "/v1/changes" && url.query()?.contains("wait=25") == true ? 0.5 : 0
    DispatchQueue.global().asyncAfter(deadline: .now() + delay) { [client] in
      client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
      client?.urlProtocol(self, didLoad: Data(text.utf8))
      client?.urlProtocolDidFinishLoading(self)
    }
  }

  override func stopLoading() {}

  private static func fallback(for path: String) -> (Int, String) {
    switch path {
    case "/v1/push": (200, #"{"accepted":0,"cursor":0}"#)
    case "/v1/changes": (200, #"{"rows":[],"cursor":0,"hasMore":false}"#)
    case "/v1/devices", "/v1/sign-out", "/v1/account/delete": (200, #"{"ok":true}"#)
    default: (404, #"{"error":"not found"}"#)
    }
  }

  private static func read(_ stream: InputStream) -> Data {
    stream.open()
    defer { stream.close() }
    var data = Data()
    var buffer = [UInt8](repeating: 0, count: 4096)
    while stream.hasBytesAvailable {
      let count = stream.read(&buffer, maxLength: buffer.count)
      guard count > 0 else { break }
      data.append(buffer, count: count)
    }
    return data
  }
}
