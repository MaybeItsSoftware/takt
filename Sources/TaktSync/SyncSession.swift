import Foundation
import Observation
import TaktWorkspace

/// Sync as the apps see it: signed in or not, what it is doing, and the
/// handful of things a settings screen does. The Mac and iOS apps both hold
/// one, so signing in, the rhythm and the status read the same on each.
///
/// Accounts are Supabase's (`SyncAuthenticating`). Signing in happens there,
/// with an email and password, Google or Apple; then the device registers
/// itself with the sync server, and every request carries the Supabase access
/// token. See `docs/sync.md`.
@MainActor
@Observable
public final class SyncSession {
  public enum Phase: Equatable {
    /// Never signed in, or signed out on purpose.
    case unpaired
    /// Supabase refused to refresh the session (signed out elsewhere, the
    /// password changed, or the account deleted), or the device was signed
    /// in with the old server's tokens. Nothing syncs until it signs in again.
    case needsSignIn
    case idle(lastSyncedAt: Date?)
    case syncing
    case failed(String)
  }

  /// What making an account came to.
  public enum SignUpOutcome: Equatable, Sendable {
    case signedIn
    /// Supabase sent a confirmation email; the link in it signs in.
    case confirmEmail(String)
  }

  public private(set) var phase: Phase = .unpaired
  /// Set only while the device holds a session.
  public private(set) var credentials: SyncCredentials?
  /// The email and server last used here, to fill in the sign-in form.
  public private(set) var rememberedEmail: String?
  public private(set) var rememberedServerURL: URL?
  /// The sync server and Supabase project this device uses: Takt's own, or
  /// a self-hosted pair chosen with `use(_:)`. Kept through signing out.
  public private(set) var endpoints: SyncEndpoints
  /// The account and its devices, from `refreshAccount()`.
  public private(set) var account: SyncAccount?
  /// Signed in from a password-reset email: the settings screen asks for the
  /// new password.
  public private(set) var needsNewPassword = false
  /// Why the last link from a Supabase email didn't sign in. The settings
  /// screen shows it, and clears it.
  public var linkProblem: String?

  /// This device's id on the server, made once and kept.
  @ObservationIgnored public let deviceId: String
  @ObservationIgnored private let store: WorkspaceStore
  @ObservationIgnored private let credentialStore: any SyncCredentialStore
  @ObservationIgnored private var auth: any SyncAuthenticating
  /// Makes the Supabase client for a project, when `use(_:)` changes it.
  @ObservationIgnored private let makeAuth: (SyncEndpoints) -> any SyncAuthenticating
  @ObservationIgnored private let urlSession: URLSession
  @ObservationIgnored private let deviceName: String
  @ObservationIgnored private let platform: String
  @ObservationIgnored private var scheduler: SyncScheduler?
  @ObservationIgnored private var outboxWatch: Task<Void, Never>?
  /// From `setInFront(_:interval:)`, so a scheduler made later (after a
  /// sign-in) starts in the right rhythm.
  @ObservationIgnored private var isInFront = true
  @ObservationIgnored private var backgroundInterval: Duration = .seconds(120)
  /// A reset email was asked for here, so the link coming back is one.
  @ObservationIgnored private var isResettingPassword = false
  /// Called on the main actor after a pull has changed the workspace.
  @ObservationIgnored public var onRemoteChanges: (() -> Void)?

  /// `auth`, when given, is used whatever the endpoints (tests); otherwise
  /// `makeAuth` makes one for the endpoints' project, by default
  /// `SupabaseSyncAuth`.
  public init(
    store: WorkspaceStore, credentialStore: any SyncCredentialStore = KeychainSyncCredentialStore(),
    auth: (any SyncAuthenticating)? = nil, makeAuth: ((SyncEndpoints) -> any SyncAuthenticating)? = nil,
    deviceName: String, platform: String, urlSession: URLSession = .shared
  ) {
    self.store = store
    self.credentialStore = credentialStore
    let factory: (SyncEndpoints) -> any SyncAuthenticating
    if let auth {
      factory = { _ in auth }
    } else {
      factory = makeAuth ?? { SupabaseSyncAuth(endpoints: $0) }
    }
    self.makeAuth = factory
    // A device that chose a server of its own before a Supabase project
    // could be chosen too kept only the server, with Takt's accounts.
    var endpoints = SyncEndpoints.hosted
    if let saved = credentialStore.loadEndpoints() {
      endpoints = saved
    } else if let server = credentialStore.load()?.serverURL {
      endpoints.serverURL = server
    }
    self.endpoints = endpoints
    self.auth = factory(endpoints)
    self.urlSession = urlSession
    self.deviceName = deviceName
    self.platform = platform
    // Before anything rewrites the item: a device signed in by an older
    // build keeps the id it had.
    self.deviceId = credentialStore.deviceId()

    guard var saved = credentialStore.load() else { return }
    // The old server's token opens nothing now. Drop it, and keep the email
    // and server for signing in again.
    if saved.hasLegacyToken || (!saved.isSignedOut && self.auth.currentUser == nil) {
      saved.isSignedOut = true
      try? credentialStore.save(saved)
    }
    guard (try? store.syncState()) != nil else { return }
    rememberedEmail = saved.email
    rememberedServerURL = saved.serverURL
    if saved.isSignedOut {
      phase = .needsSignIn
    } else {
      credentials = saved
      phase = .idle(lastSyncedAt: try? store.syncState()?.lastSyncedAt)
    }
  }

  public var isSignedIn: Bool { credentials != nil }

  /// Who is signed in, as the provider describes them: the name and picture
  /// the account button shows. Read from the auth session, so it is there
  /// after a relaunch without asking the server.
  public var profile: SyncAuthUser? {
    guard credentials != nil else { return nil }
    return auth.currentUser
  }

  /// The email shown as "Signed in as …": the account's, once known.
  public var email: String? { account?.email ?? credentials?.email }

  // MARK: - Rhythm

  /// Starts the rhythm: a cycle now, a long-poll while the app is in front,
  /// and a cycle shortly after any local write. Safe to call repeatedly.
  public func activate() {
    guard let credentials, scheduler == nil else { return }
    let scheduler = SyncScheduler(engine: engine(for: credentials)) { [weak self] status, outcome in
      await self?.report(status, outcome)
    }
    self.scheduler = scheduler
    let background = isInFront ? nil : backgroundInterval
    Task {
      await scheduler.background(every: background)
      await scheduler.start()
    }
    watchOutbox()
  }

  /// For an app that keeps running out of sight (the Mac): with `false`, the
  /// long-poll gives way to a quick cycle every `interval`, while local
  /// writes still go up at once; `true` goes back to the long-poll. Unlike
  /// `deactivate()`, sync never stops. The phones, which iOS suspends, use
  /// `deactivate()` and a background refresh task instead.
  public func setInFront(_ inFront: Bool, interval: Duration = .seconds(120)) {
    isInFront = inFront
    backgroundInterval = interval
    guard let scheduler else { return }
    Task { await scheduler.background(every: inFront ? nil : interval) }
  }

  /// Stops the long-poll, for when the app goes to the background. Local
  /// writes still queue in the outbox and go with the next cycle.
  public func deactivate() {
    outboxWatch?.cancel()
    outboxWatch = nil
    if let scheduler { Task { await scheduler.stop() } }
    scheduler = nil
  }

  /// One cycle, for "Sync now" and for a background refresh. True when it
  /// succeeded.
  @discardableResult
  public func syncNow() async -> Bool {
    if let scheduler { return await scheduler.syncNow() }
    guard let credentials else { return false }
    do {
      let outcome = try await engine(for: credentials).sync()
      report(.idle(lastSyncedAt: Date()), outcome)
      return true
    } catch SyncError.unauthorized {
      sessionWasRefused()
      return false
    } catch {
      report(.failed(error.localizedDescription), nil)
      return false
    }
  }

  // MARK: - Signing in

  /// Signs this device in to an existing account.
  public func signIn(email: String, password: String, serverURL: URL? = nil) async throws {
    let email = try Self.checked(email: email, password: password)
    let user = try await auth.signIn(email: email, password: password)
    try await finishSignIn(user, serverURL: serverURL ?? endpoints.serverURL)
  }

  /// Makes an account and, unless Supabase wants the address confirmed
  /// first, signs this device in to it.
  public func signUp(
    email: String, password: String, serverURL: URL? = nil
  ) async throws -> SignUpOutcome {
    let email = try Self.checked(email: email, password: password)
    guard let user = try await auth.signUp(email: email, password: password) else {
      // The link in the email comes back to `completeSignIn(from:)`.
      rememberedEmail = email
      rememberedServerURL = serverURL ?? endpoints.serverURL
      return .confirmEmail(email)
    }
    try await finishSignIn(user, serverURL: serverURL ?? endpoints.serverURL)
    return .signedIn
  }

  /// Google or Apple, in a browser sheet. Throws `SyncError.cancelled` when
  /// the sheet is closed.
  public func signIn(with provider: SyncOAuthProvider, serverURL: URL? = nil) async throws {
    try await finishSignIn(auth.signIn(with: provider), serverURL: serverURL ?? endpoints.serverURL)
  }

  /// Sign in with Apple, done natively by the app (`SyncAppleNonce`).
  public func signInWithApple(
    idToken: String, nonce: String, serverURL: URL? = nil
  ) async throws {
    try await finishSignIn(
      auth.signInWithApple(idToken: idToken, nonce: nonce), serverURL: serverURL ?? endpoints.serverURL)
  }

  /// A link from a Supabase email (confirming the address, or resetting the
  /// password) opened in the app. It only works on the device that asked for
  /// it, which holds the other half of the exchange.
  public func completeSignIn(from url: URL) async throws {
    let user: SyncAuthUser
    do {
      user = try await auth.signIn(fromCallback: url)
    } catch {
      throw SyncError.invalid(
        "That link didn't sign you in here. If it confirmed your email, sign in with your password.")
    }
    try await finishSignIn(user, serverURL: rememberedServerURL ?? endpoints.serverURL)
    if isResettingPassword {
      isResettingPassword = false
      needsNewPassword = true
    }
  }

  /// For the apps' URL handlers: `completeSignIn(from:)`, keeping a failure
  /// in `linkProblem` for the settings screen to show.
  public func openAuthLink(_ url: URL) async {
    linkProblem = nil
    do {
      try await completeSignIn(from: url)
    } catch {
      linkProblem = error.localizedDescription
    }
  }

  /// Emails a link for choosing a new password, and returns the email as
  /// sent, trimmed. Supabase answers alike whether or not the account
  /// exists; see `passwordResetSentMessage(for:)`.
  @discardableResult
  public func requestPasswordReset(email: String) async throws -> String {
    let email = email.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !email.isEmpty else { throw SyncError.invalid("Enter your email first.") }
    try await auth.resetPassword(email: email)
    isResettingPassword = true
    rememberedEmail = email
    return email
  }

  /// What to show once a reset was asked for. It can't say the account
  /// exists, because Supabase doesn't say.
  public static func passwordResetSentMessage(for email: String) -> String {
    "If there's an account for \(email), we've sent it a link to reset the password. Open it on this device."
  }

  /// What to show when a new account has to be confirmed first.
  public static func confirmEmailMessage(for email: String) -> String {
    "Check your email to confirm. We've sent a link to \(email); open it on this device to finish signing in."
  }

  /// Sets the password after signing in from a reset email.
  public func setNewPassword(_ password: String) async throws {
    guard !password.isEmpty else { throw SyncError.invalid("Enter a new password.") }
    try await auth.updatePassword(password)
    needsNewPassword = false
  }

  /// Leaves the password as it was.
  public func skipNewPassword() {
    needsNewPassword = false
  }

  /// Fetches the account's email and devices.
  public func refreshAccount() async throws {
    account = try await signedInCall { try await $0.account() }
  }

  /// Signs this device out: off the server's device list (if it can be
  /// reached; signing out offline still works here), and out of Supabase on
  /// this device only. The workspace stays as it is.
  public func signOut() async {
    if let credentials {
      _ = try? await transport(for: credentials).signOut()
    }
    await auth.signOut()
    try? forget()
  }

  /// Deletes the account: its rows on the server, its devices and the
  /// Supabase user. Each device keeps its own copy of the workspace. The
  /// apps ask the person to confirm first.
  public func deleteAccount() async throws {
    try await signedInCall { try await $0.deleteAccount() }
    await auth.signOut()
    try forget()
    rememberedEmail = nil
    rememberedServerURL = nil
  }

  // MARK: - Server

  /// Switches to `endpoints`: a self-hosted sync server and Supabase
  /// project, or `.hosted` to go back to Takt's. Nothing changes if they are
  /// the ones in use. Otherwise a self-hosted pair is checked first
  /// (`SyncEndpointCheck`) unless `check` is false, and then this device
  /// signs out cleanly (off the old server's device list, out of the old
  /// project, the outbox dropped) before the new ones are saved. The
  /// workspace stays as it is.
  public func use(_ endpoints: SyncEndpoints, check: Bool = true) async throws {
    guard endpoints != self.endpoints else { return }
    if check, !endpoints.isHosted {
      try await SyncEndpointCheck.check(endpoints, session: urlSession)
    }
    if isSignedIn {
      await signOut()
    } else {
      await auth.signOut()
      try? forget()
    }
    let projectChanged =
      endpoints.supabaseURL != self.endpoints.supabaseURL || endpoints.supabaseKey != self.endpoints.supabaseKey
    credentialStore.saveEndpoints(endpoints.isHosted ? nil : endpoints)
    self.endpoints = endpoints
    if projectChanged { auth = makeAuth(endpoints) }
    rememberedServerURL = endpoints.serverURL
    isResettingPassword = false
  }

  // MARK: - Internals

  private func transport(for credentials: SyncCredentials) -> HTTPSyncTransport {
    HTTPSyncTransport(serverURL: credentials.serverURL, deviceId: deviceId, tokens: auth, session: urlSession)
  }

  private func engine(for credentials: SyncCredentials) -> SyncEngine {
    SyncEngine(store: store, transport: transport(for: credentials), deviceId: deviceId)
  }

  /// Runs an account call with this device's session, noticing a refusal.
  @discardableResult
  private func signedInCall<T>(_ call: (HTTPSyncTransport) async throws -> T) async throws -> T {
    guard let credentials else { throw SyncError.notPaired }
    do {
      return try await call(transport(for: credentials))
    } catch SyncError.unauthorized {
      sessionWasRefused()
      throw SyncError.unauthorized
    }
  }

  private static func checked(email: String, password: String) throws -> String {
    let email = email.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !email.isEmpty else { throw SyncError.invalid("Enter your email address.") }
    guard !password.isEmpty else { throw SyncError.invalid("Enter your password.") }
    return email
  }

  /// Signed in with Supabase: the device introduces itself to the sync
  /// server, and only then counts as signed in. If the server can't be
  /// reached the Supabase session is dropped again, so the two never
  /// disagree about whether this device is signed in.
  private func finishSignIn(_ user: SyncAuthUser, serverURL: URL) async throws {
    let credentials = SyncCredentials(serverURL: serverURL, deviceId: deviceId, accountId: user.id, email: user.email)
    do {
      try await transport(for: credentials).registerDevice(name: deviceName, platform: platform)
    } catch {
      await auth.signOut()
      throw error
    }
    try adopt(credentials)
  }

  private func adopt(_ credentials: SyncCredentials) throws {
    deactivate()
    try store.beginSync(deviceId: credentials.deviceId, serverURL: credentials.serverURL.absoluteString)
    try credentialStore.save(credentials)
    self.credentials = credentials
    rememberedEmail = credentials.email
    rememberedServerURL = credentials.serverURL
    account = nil
    phase = .idle(lastSyncedAt: nil)
    activate()
  }

  /// Leaves sync on this device: no session, no outbox, the triggers off.
  private func forget() throws {
    deactivate()
    credentialStore.clear()
    credentials = nil
    account = nil
    needsNewPassword = false
    phase = .unpaired
    try store.endSync()
  }

  /// Supabase refused to refresh the session. Syncing stops instead of
  /// retrying a request that can only fail, and the keychain item is kept,
  /// marked, so the email and server survive a relaunch for the sign-in
  /// form. The outbox keeps recording, so edits made meanwhile (deletes
  /// included) still go up after signing back in.
  private func sessionWasRefused() {
    guard var refused = credentials else { return }
    deactivate()
    refused.isSignedOut = true
    try? credentialStore.save(refused)
    credentials = nil
    account = nil
    phase = .needsSignIn
  }

  private func report(_ status: SyncEngine.Status, _ outcome: SyncEngine.Outcome?) {
    guard credentials != nil else { return }
    switch status {
    case .idle(let last): phase = .idle(lastSyncedAt: last)
    case .syncing: phase = .syncing
    case .failed(let message): phase = .failed(message)
    case .signedOut: sessionWasRefused()
    }
    if outcome?.changedWorkspace == true { onRemoteChanges?() }
  }

  /// Notices local writes, whoever made them — this app, or the CLI writing
  /// the same file — by watching the outbox rather than asking every mutation
  /// to report itself. One cheap EXISTS query a second.
  private func watchOutbox() {
    outboxWatch?.cancel()
    outboxWatch = Task { [weak self, store] in
      var lastSeq: Int64?
      while !Task.isCancelled {
        try? await Task.sleep(for: .seconds(1))
        let seq = try? store.latestSyncOutboxSeq()
        if let seq, seq != lastSeq {
          lastSeq = seq
          await self?.scheduler?.noteLocalChange()
        }
      }
    }
  }
}
