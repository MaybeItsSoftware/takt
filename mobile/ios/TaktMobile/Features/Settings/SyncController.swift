import BackgroundTasks
import Foundation
import Observation
import OSLog
import TaktSync
import UIKit

/// The app's hold on sync: one `SyncSession` over the workspace store, kept
/// running while the app is in front, refreshed in the background, and
/// reported back into the workspace when a pull changes it.
///
/// All of the protocol lives in `TaktSync`; this is only the iOS rhythm
/// around it — scene phases, the background refresh task, the links from
/// Supabase's emails.
@MainActor
@Observable
final class SyncController {
  /// The one controller, once the workspace is open. Nil in UI tests, which
  /// run on a throwaway workspace that must never reach a server.
  private(set) static var shared: SyncController?

  static let refreshTaskID = "uk.co.maybeitssoftware.takt.sync"

  let session: SyncSession
  /// The last sign-in attempt's failure, for the settings screen.
  var signInError: String?
  var isSigningIn = false

  @ObservationIgnored private let logger = Logger(subsystem: "uk.co.maybeitssoftware.takt", category: "SyncController")

  init(model: WorkspaceModel) {
    session = SyncSession(store: model.store, deviceName: UIDevice.current.name, platform: "ios")
    session.onRemoteChanges = { [weak model] in model?.didChange() }
  }

  /// Installed from `featureInstallers`.
  static func install(on model: WorkspaceModel) {
    guard !AppGroup.isUITesting, shared == nil else { return }
    shared = SyncController(model: model)
  }

  // MARK: - Rhythm

  func sceneBecameActive() {
    session.activate()
  }

  func sceneLeftForeground() {
    session.deactivate()
    scheduleBackgroundRefresh()
  }

  /// Pull-to-refresh and "Sync now". Quietly nothing when unpaired.
  func syncNow() async {
    guard session.isSignedIn else { return }
    await session.syncNow()
  }

  /// Asks for a background refresh in a quarter of an hour or so. The
  /// system decides when; a paired device is all that asks.
  func scheduleBackgroundRefresh() {
    guard session.isSignedIn else { return }
    let request = BGAppRefreshTaskRequest(identifier: Self.refreshTaskID)
    request.earliestBeginDate = Date(timeIntervalSinceNow: 15 * 60)
    do {
      try BGTaskScheduler.shared.submit(request)
    } catch {
      logger.debug("Background refresh not scheduled: \(error.localizedDescription, privacy: .public)")
    }
  }

  /// The background refresh task's body: one cycle, then ask for the next.
  func backgroundRefresh() async {
    guard session.isSignedIn else { return }
    await session.syncNow()
    scheduleBackgroundRefresh()
  }

  // MARK: - Signing in

  /// Signs in to an existing account. True when signed in.
  @discardableResult
  func signIn(email: String, password: String, serverURL: URL) async -> Bool {
    await attempt { try await self.session.signIn(email: email, password: password, serverURL: serverURL) }
  }

  /// Makes an account. What to tell the person when it has to be confirmed
  /// by email first; nil when it signed straight in, or failed (with
  /// `signInError` set).
  func signUp(email: String, password: String, serverURL: URL) async -> String? {
    var outcome: SyncSession.SignUpOutcome?
    await attempt { outcome = try await self.session.signUp(email: email, password: password, serverURL: serverURL) }
    if case .confirmEmail(let address) = outcome { return SyncSession.confirmEmailMessage(for: address) }
    return nil
  }

  /// Google (or Apple) in Supabase's web flow. True when signed in.
  @discardableResult
  func signIn(with provider: SyncOAuthProvider, serverURL: URL) async -> Bool {
    await attempt { try await self.session.signIn(with: provider, serverURL: serverURL) }
  }

  /// Sign in with Apple from the native sheet. True when signed in.
  @discardableResult
  func signInWithApple(idToken: String, nonce: String, serverURL: URL) async -> Bool {
    await attempt { try await self.session.signInWithApple(idToken: idToken, nonce: nonce, serverURL: serverURL) }
  }

  /// `takt://auth-callback` from a Supabase email: confirming the
  /// address, or resetting the password.
  func openAuthLink(_ url: URL) async {
    isSigningIn = true
    signInError = nil
    defer { isSigningIn = false }
    await session.openAuthLink(url)
    signInError = session.linkProblem
  }

  /// Emails a link for choosing a new password. The confirmation to show when
  /// it was sent; nil, with `signInError` set, when it wasn't.
  func requestPasswordReset(email: String) async -> String? {
    var sent: String?
    await attempt { sent = try await self.session.requestPasswordReset(email: email) }
    return sent.map(SyncSession.passwordResetSentMessage(for:))
  }

  /// Sets the password after signing in from a reset email.
  @discardableResult
  func setNewPassword(_ password: String) async -> Bool {
    await attempt { try await self.session.setNewPassword(password) }
  }

  /// Signs this device out. Its tasks stay.
  func signOut() async {
    await session.signOut()
  }

  /// Deletes the account on the server. True when it is gone.
  @discardableResult
  func deleteAccount() async -> Bool {
    await attempt { try await self.session.deleteAccount() }
  }

  @discardableResult
  private func attempt(_ work: () async throws -> Void) async -> Bool {
    isSigningIn = true
    signInError = nil
    defer { isSigningIn = false }
    do {
      try await work()
      return true
    } catch SyncError.cancelled {
      // Closing the sign-in sheet is an answer, not a failure.
      return false
    } catch {
      signInError = error.localizedDescription
      return false
    }
  }
}

/// The phase in words, shared by the status line and the settings screen.
enum SyncPhaseText {
  static func describe(_ phase: SyncSession.Phase, now: Date = .now) -> String {
    switch phase {
    case .unpaired: return "Not set up"
    case .needsSignIn: return "Signed out — sign in again"
    case .syncing: return "Syncing…"
    case .failed(let message): return "Couldn't sync: \(message)"
    case .needsSubscription: return "Paused — needs Takt Sync"
    case .idle(let last):
      guard let last else { return "Signed in" }
      if now.timeIntervalSince(last) < 60 { return "Synced just now" }
      let relative = RelativeDateTimeFormatter()
      relative.unitsStyle = .short
      return "Synced \(relative.localizedString(for: last, relativeTo: now))"
    }
  }

  static func short(_ phase: SyncSession.Phase) -> String {
    switch phase {
    case .unpaired: "Off"
    case .needsSignIn: "Signed out"
    case .syncing: "Syncing"
    case .failed: "Error"
    case .needsSubscription: "Paused"
    case .idle: "On"
    }
  }

  static func symbol(_ phase: SyncSession.Phase) -> String {
    switch phase {
    case .unpaired, .needsSubscription: "icloud.slash"
    case .needsSignIn, .failed: "exclamationmark.icloud"
    case .syncing: "arrow.triangle.2.circlepath"
    case .idle: "checkmark.icloud"
    }
  }

  /// Whether the phase is something the user has to deal with.
  static func isProblem(_ phase: SyncSession.Phase) -> Bool {
    switch phase {
    case .needsSignIn, .failed: true
    default: false
    }
  }
}
