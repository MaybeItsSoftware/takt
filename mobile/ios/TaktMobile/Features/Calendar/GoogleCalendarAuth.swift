import AuthenticationServices
import CryptoKit
import Foundation
import Security
import UIKit

/// Google sign-in for Calendar on the iPhone: OAuth with PKCE through
/// `ASWebAuthenticationSession`, against the iOS OAuth client named by the
/// `GoogleIOSClientID` Info.plist key (docs/google-calendar.md).
///
/// Separate from sync's sign-in on purpose. Sync signs in to Supabase, which
/// never hands back a Google token that can reach the Calendar API; this
/// asks Google for `calendar.events` and `calendar.readonly` and keeps the
/// refresh token in the keychain, on this device only.
///
/// Drive linking signs in the same way with its own scope and keychain item
/// (`GoogleCalendarAuth.drive`), so connecting one never changes the other.
@MainActor
final class GoogleCalendarAuth {
  static let calendarScopes = [
    "https://www.googleapis.com/auth/calendar.events",
    "https://www.googleapis.com/auth/calendar.readonly",
  ]

  /// Only the Drive files picked in Google's Picker (docs/links.md).
  static let driveScopes = ["https://www.googleapis.com/auth/drive.file"]

  /// Drive linking's sign-in.
  static func drive() -> GoogleCalendarAuth {
    GoogleCalendarAuth(scopes: driveScopes, keychainService: "uk.co.maybeitssoftware.takt.google-drive")
  }

  let scopes: [String]
  private let keychainService: String

  private static let authorizeURL = URL(string: "https://accounts.google.com/o/oauth2/v2/auth")!
  private static let tokenURL = URL(string: "https://oauth2.googleapis.com/token")!
  private static let revokeURL = URL(string: "https://oauth2.googleapis.com/revoke")!
  private static let keychainAccount = "oauth-token"

  private struct Token: Codable {
    var accessToken: String
    var refreshToken: String
    var expiresAt: Date
  }

  private var token: Token?
  private var refreshing: Task<Token, Error>?

  init(
    scopes: [String] = GoogleCalendarAuth.calendarScopes,
    keychainService: String = "uk.co.maybeitssoftware.takt.google-calendar"
  ) {
    self.scopes = scopes
    self.keychainService = keychainService
    token = load()
  }

  /// The client id, when this build has one.
  static var clientID: String? {
    let value = (Bundle.main.object(forInfoDictionaryKey: "GoogleIOSClientID") as? String)?
      .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    return value.isEmpty || value.hasPrefix("$(") ? nil : value
  }

  static var isConfigured: Bool { clientID != nil }
  var isConnected: Bool { token != nil }

  /// Google's redirect for an iOS client: its id reversed, as a scheme the
  /// sign-in sheet catches itself.
  private static func redirectScheme(for clientID: String) -> String {
    "com.googleusercontent.apps." + clientID.replacingOccurrences(of: ".apps.googleusercontent.com", with: "")
  }

  func connect() async throws {
    _ = try await authorize()
  }

  /// Google's Picker for mobile apps (docs/links.md): a `drive.file`
  /// consent that shows the person their Drive to choose from
  /// (`trigger_onepick`) and comes back with the ids picked, as well as the
  /// token. Only for the Drive sign-in, since Google lets `drive.file` be
  /// asked for only alone there.
  func pickDriveFiles() async throws -> [String] {
    let items = try await authorize(extra: [
      .init(name: "trigger_onepick", value: "true"),
      .init(name: "allow_multiple", value: "true"),
      .init(name: "allow_folder_selection", value: "true"),
    ])
    return Self.pickedIDs(items)
  }

  /// The comma-separated `picked_file_ids` the Picker's redirect carries.
  nonisolated static func pickedIDs(_ items: [URLQueryItem]) -> [String] {
    (items.first { $0.name == "picked_file_ids" }?.value ?? "")
      .split(separator: ",")
      .map { $0.trimmingCharacters(in: .whitespaces) }
      .filter { !$0.isEmpty }
  }

  /// The sign-in sheet's round trip with `extra` parameters, the code
  /// exchanged and the token kept. Answers the redirect's parameters.
  private func authorize(extra: [URLQueryItem] = []) async throws -> [URLQueryItem] {
    guard let clientID = Self.clientID else { throw GoogleCalendarAuthError.notConfigured }
    let scheme = Self.redirectScheme(for: clientID)
    let redirectURI = "\(scheme):/oauth2redirect"
    let verifier = Self.randomURLSafe(bytes: 32)
    let challenge = Self.base64URL(Data(SHA256.hash(data: Data(verifier.utf8))))
    let state = Self.randomURLSafe(bytes: 16)
    var components = URLComponents(url: Self.authorizeURL, resolvingAgainstBaseURL: false)!
    components.queryItems = [
      .init(name: "client_id", value: clientID),
      .init(name: "redirect_uri", value: redirectURI),
      .init(name: "response_type", value: "code"),
      .init(name: "scope", value: scopes.joined(separator: " ")),
      .init(name: "code_challenge", value: challenge),
      .init(name: "code_challenge_method", value: "S256"),
      .init(name: "state", value: state),
      .init(name: "access_type", value: "offline"),
      .init(name: "prompt", value: "consent"),
    ] + extra
    let callback = try await GoogleWebSignIn.run(components.url!, callbackScheme: scheme)
    let items = URLComponents(url: callback, resolvingAgainstBaseURL: false)?.queryItems ?? []
    guard items.first(where: { $0.name == "state" })?.value == state else {
      throw GoogleCalendarAuthError.failed("Google's answer didn't match the request.")
    }
    if let error = items.first(where: { $0.name == "error" })?.value {
      throw error == "access_denied" ? GoogleCalendarAuthError.cancelled : GoogleCalendarAuthError.failed(error)
    }
    guard let code = items.first(where: { $0.name == "code" })?.value else {
      throw GoogleCalendarAuthError.failed("Google sent no authorization code.")
    }
    let answer = try await Self.post([
      "code": code, "client_id": clientID, "redirect_uri": redirectURI,
      "grant_type": "authorization_code", "code_verifier": verifier,
    ])
    guard let refresh = answer.refreshToken ?? token?.refreshToken else {
      throw GoogleCalendarAuthError.failed("Google sent no refresh token.")
    }
    store(Token(accessToken: answer.accessToken, refreshToken: refresh, expiresAt: answer.expiry))
    return items
  }

  /// A token good for at least a minute, refreshed when it is not.
  func accessToken() async throws -> String {
    guard let current = token else { throw GoogleCalendarAuthError.notConnected }
    if current.expiresAt > Date().addingTimeInterval(60) { return current.accessToken }
    if let refreshing { return try await refreshing.value.accessToken }
    let task = Task { () throws -> Token in
      guard let clientID = Self.clientID else { throw GoogleCalendarAuthError.notConfigured }
      let answer = try await Self.post([
        "client_id": clientID, "refresh_token": current.refreshToken, "grant_type": "refresh_token",
      ])
      return Token(
        accessToken: answer.accessToken, refreshToken: answer.refreshToken ?? current.refreshToken,
        expiresAt: answer.expiry)
    }
    refreshing = task
    defer { refreshing = nil }
    do {
      let fresh = try await task.value
      store(fresh)
      return fresh.accessToken
    } catch GoogleCalendarAuthError.revoked {
      forget()
      throw GoogleCalendarAuthError.notConnected
    }
  }

  /// Signs out here and tells Google to drop the grant.
  func disconnect() {
    if let refresh = token?.refreshToken {
      var request = URLRequest(url: Self.revokeURL)
      request.httpMethod = "POST"
      request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
      request.httpBody = Data("token=\(refresh)".utf8)
      Task.detached { _ = try? await URLSession.shared.data(for: request) }
    }
    forget()
  }

  /// Google refused the token at use: the grant is gone.
  func forget() {
    token = nil
    SecItemDelete(query() as CFDictionary)
  }

  // MARK: - Token endpoint

  private struct Answer {
    let accessToken: String
    let refreshToken: String?
    let expiry: Date
  }

  private static func post(_ form: [String: String]) async throws -> Answer {
    var request = URLRequest(url: tokenURL)
    request.httpMethod = "POST"
    request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
    var allowed = CharacterSet.alphanumerics
    allowed.insert(charactersIn: "-._~")
    request.httpBody = Data(
      form.map { key, value in
        "\(key)=\(value.addingPercentEncoding(withAllowedCharacters: allowed) ?? value)"
      }
      .joined(separator: "&").utf8)
    let (data, response) = try await URLSession.shared.data(for: request)
    let status = (response as? HTTPURLResponse)?.statusCode ?? 0
    let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
    guard (200..<300).contains(status), let access = json["access_token"] as? String else {
      if json["error"] as? String == "invalid_grant" { throw GoogleCalendarAuthError.revoked }
      let detail = json["error_description"] as? String ?? json["error"] as? String
      throw GoogleCalendarAuthError.failed(detail ?? "Google answered \(status).")
    }
    let lifetime = (json["expires_in"] as? Double) ?? 3600
    return Answer(
      accessToken: access, refreshToken: json["refresh_token"] as? String,
      expiry: Date().addingTimeInterval(lifetime))
  }

  // MARK: - Keychain

  private func query() -> [String: Any] {
    [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: keychainService,
      kSecAttrAccount as String: keychainAccount,
    ]
  }

  private func store(_ token: Token) {
    self.token = token
    guard let data = try? JSONEncoder().encode(token) else { return }
    SecItemDelete(query() as CFDictionary)
    var item = query()
    item[kSecValueData as String] = data
    item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    SecItemAdd(item as CFDictionary, nil)
  }

  private func load() -> Token? {
    var item = query()
    item[kSecReturnData as String] = true
    item[kSecMatchLimit as String] = kSecMatchLimitOne
    var result: CFTypeRef?
    guard SecItemCopyMatching(item as CFDictionary, &result) == errSecSuccess, let data = result as? Data
    else { return nil }
    return try? JSONDecoder().decode(Token.self, from: data)
  }

  // MARK: - PKCE

  private static func randomURLSafe(bytes count: Int) -> String {
    var bytes = [UInt8](repeating: 0, count: count)
    _ = SecRandomCopyBytes(kSecRandomDefault, count, &bytes)
    return base64URL(Data(bytes))
  }

  private static func base64URL(_ data: Data) -> String {
    data.base64EncodedString()
      .replacingOccurrences(of: "+", with: "-")
      .replacingOccurrences(of: "/", with: "_")
      .replacingOccurrences(of: "=", with: "")
  }
}

enum GoogleCalendarAuthError: LocalizedError, Equatable {
  case notConfigured
  case notConnected
  case cancelled
  case revoked
  case failed(String)

  var errorDescription: String? {
    switch self {
    case .notConfigured: "This build has no Google OAuth client for Calendar."
    case .notConnected: "Connect Google Calendar in Settings."
    case .cancelled: "Sign-in was cancelled."
    case .revoked: "Google withdrew access. Connect Google Calendar again."
    case .failed(let detail): detail
    }
  }
}

/// The sign-in sheet, as sync's `WebSignIn` runs it.
@MainActor
private final class GoogleWebSignIn: NSObject, ASWebAuthenticationPresentationContextProviding {
  private static var current: ASWebAuthenticationSession?
  private static let anchor = GoogleWebSignIn()

  static func run(_ url: URL, callbackScheme: String) async throws -> URL {
    defer { current = nil }
    return try await withCheckedThrowingContinuation { continuation in
      // `@Sendable`, so nonisolated: AuthenticationServices answers from its
      // XPC queue, and a main-actor closure traps the moment it is called
      // there (the crash fixed in Sources/TaktSync/SyncAuth.swift).
      let session = ASWebAuthenticationSession(url: url, callbackURLScheme: callbackScheme) { @Sendable url, error in
        if let url {
          continuation.resume(returning: url)
        } else if let error = error as? ASWebAuthenticationSessionError, error.code == .canceledLogin {
          continuation.resume(throwing: GoogleCalendarAuthError.cancelled)
        } else {
          continuation.resume(throwing: error ?? GoogleCalendarAuthError.cancelled)
        }
      }
      session.presentationContextProvider = anchor
      current = session
      if !session.start() {
        current = nil
        continuation.resume(throwing: GoogleCalendarAuthError.failed("Couldn't open the sign-in window."))
      }
    }
  }

  nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
    MainActor.assumeIsolated {
      let windows = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.flatMap(\.windows)
      return windows.first(where: \.isKeyWindow) ?? windows.first ?? ASPresentationAnchor()
    }
  }
}
