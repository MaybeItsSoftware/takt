import AuthenticationServices
import TaktSync
import SwiftUI

/// Signing in to sync, and the state of sync once signed in.
///
/// Signed out: Sign in with Apple (the native sheet), Google (Supabase's web
/// flow), or an email and password to sign in or make an account. All three
/// are Supabase accounts. Signed in: the account, the status, Sync now, the
/// account's devices, signing out, and deleting the account — which the App
/// Store requires be possible in the app (5.1.1(v)). The server is
/// Takt's unless "Use a different server" says otherwise.
struct SyncSettingsView: View {
  @Environment(\.theme) private var theme
  var body: some View {
    Group {
      if let controller = SyncController.shared {
        SyncSettingsForm(controller: controller)
      } else {
        EmptyState(title: "Sync is off", message: "Sync isn't available in this build.", systemImage: "icloud.slash")
          .frame(maxWidth: .infinity, maxHeight: .infinity)
          .background(theme.paper)
      }
    }
    .navigationTitle("Sync")
    .navigationBarTitleDisplayMode(.inline)
  }
}

private struct SyncSettingsForm: View {
  @Environment(\.theme) private var theme
  @Environment(\.colorScheme) private var colorScheme
  @Bindable var controller: SyncController
  @State private var email = ""
  @State private var password = ""
  @State private var newPassword = ""
  @State private var serverURL = SyncServer.defaultURL.absoluteString
  @State private var usesOtherServer = false
  @State private var isConfirmingSignOut = false
  @State private var isConfirmingDelete = false
  /// The nonce whose hash went into the Apple request, kept for Supabase.
  @State private var appleNonce = ""
  /// What "Forgot password?" or "Create account" sent.
  @State private var notice: String?

  private var session: SyncSession { controller.session }

  var body: some View {
    List {
      // First, so a refused password says so without scrolling.
      if let error = controller.signInError {
        Section {
          Label(error, systemImage: "exclamationmark.triangle")
            .font(theme.type.caption)
            .foregroundStyle(theme.danger)
            .accessibilityIdentifier("sync.signInError")
        }
        .listRowBackground(theme.danger.opacity(0.08))
      }
      if session.isSignedIn {
        signedIn
      } else {
        signedOut
      }
    }
    .macSettingsList()
    .background(theme.paper)
    .font(theme.type.body)
    .disabled(controller.isSigningIn)
    .overlay {
      if controller.isSigningIn {
        ProgressView("Signing in…").padding(theme.space.lg)
          .background(theme.raised, in: RoundedRectangle(cornerRadius: theme.radius.panel))
          .overlay(RoundedRectangle(cornerRadius: theme.radius.panel).strokeBorder(theme.border, lineWidth: theme.stroke))
      }
    }
  }

  // MARK: - Signed in

  @ViewBuilder
  private var signedIn: some View {
    Section {
      LabeledContent("Signed in as") {
        Text(session.email ?? "This account").foregroundStyle(theme.muted).textSelection(.enabled)
      }
      TimelineView(.periodic(from: .now, by: 30)) { context in
        LabeledContent("Status") {
          Label(SyncPhaseText.describe(session.phase, now: context.date), systemImage: SyncPhaseText.symbol(session.phase))
            .foregroundStyle(SyncPhaseText.isProblem(session.phase) ? theme.danger : theme.ink)
        }
      }
      if let server = session.credentials?.serverURL, server != SyncServer.defaultURL {
        LabeledContent("Server") {
          Text(server.host() ?? server.absoluteString).font(theme.type.numeral).foregroundStyle(theme.muted)
        }
      }
      Button {
        Task { await controller.syncNow() }
      } label: {
        Label("Sync now", systemImage: "arrow.triangle.2.circlepath")
      }
      .disabled(session.phase == .syncing)
      .accessibilityIdentifier("sync.now")
    } header: {
      header("Account")
    }
    .settingsRows()
    .task(id: session.credentials?.accountId) {
      try? await session.refreshAccount()
    }

    if session.account?.subscription?.required == true || session.phase == .needsSubscription {
      TaktSyncSection(theme: theme, controller: controller)
    }

    if session.needsNewPassword {
      Section {
        SecureField("New password", text: $newPassword)
          .textContentType(.newPassword)
          .accessibilityIdentifier("sync.newPassword")
        Button("Save password") {
          Task {
            if await controller.setNewPassword(newPassword) {
              newPassword = ""
              notice = "Password changed."
            }
          }
        }
        .disabled(newPassword.isEmpty)
        Button("Not now") { session.skipNewPassword() }
          .foregroundStyle(theme.muted)
      } header: {
        header("Choose a new password")
      } footer: {
        Text("You signed in from a password-reset email. Set the password to use from now on.")
          .font(theme.type.footnote).foregroundStyle(theme.muted)
      }
      .settingsRows()
    }

    Section {
      if let devices = session.account?.devices {
        ForEach(devices) { device in
          VStack(alignment: .leading, spacing: 2) {
            HStack {
              Text(device.displayName)
              Spacer()
              if device.current {
                Text("This device").font(theme.type.caption).foregroundStyle(theme.muted)
              }
            }
            Group {
              if let seen = device.lastSeenDate {
                Text("\(device.platformName) · seen \(seen, format: .relative(presentation: .named))")
              } else {
                Text(device.platformName)
              }
            }
            .font(theme.type.caption)
            .foregroundStyle(theme.muted)
          }
        }
      } else {
        Text("Loading…").foregroundStyle(theme.muted)
      }
    } header: {
      header("Devices")
    } footer: {
      Text("To add a device, sign in to the same account on it.")
        .font(theme.type.footnote).foregroundStyle(theme.muted)
    }
    .settingsRows()

    Section {
      Button("Sign out", role: .destructive) { isConfirmingSignOut = true }
        .accessibilityIdentifier("sync.signOut")
        .confirmationDialog("Sign out of sync?", isPresented: $isConfirmingSignOut, titleVisibility: .visible) {
          Button("Sign out", role: .destructive) { Task { await controller.signOut() } }
        } message: {
          Text("Your tasks stay on this device. They stop syncing with your other devices.")
        }
      Button("Delete account…", role: .destructive) { isConfirmingDelete = true }
        .accessibilityIdentifier("sync.deleteAccount")
        .confirmationDialog("Delete your sync account?", isPresented: $isConfirmingDelete, titleVisibility: .visible) {
          Button("Delete account", role: .destructive) { Task { await controller.deleteAccount() } }
            .accessibilityIdentifier("sync.confirmDelete")
        } message: {
          Text(
            "This deletes \(session.email ?? "your account") and everything synced to it, and signs out every device. "
              + "Each device keeps its own copy of the workspace, and this iPhone keeps its tasks. It can't be undone."
          )
        }
    } footer: {
      Text("Deleting the account removes everything synced to the server and signs out every device. Each device keeps its own copy.")
        .font(theme.type.footnote).foregroundStyle(theme.muted)
    }
    .settingsRows()
  }

  // MARK: - Signed out

  @ViewBuilder
  private var signedOut: some View {
    if session.phase == .needsSignIn {
      Section {
        Label("Signed out. Sign in again to keep syncing.", systemImage: "exclamationmark.icloud")
          .foregroundStyle(theme.danger)
          .accessibilityIdentifier("sync.needsSignIn")
      }
      .settingsRows()
    }

    Section {
      SignInWithAppleButton(.signIn) { request in
        appleNonce = SyncAppleNonce.make()
        request.requestedScopes = [.email]
        request.nonce = SyncAppleNonce.sha256(appleNonce)
      } onCompletion: { result in
        signInWithApple(result)
      }
      .signInWithAppleButtonStyle(colorScheme == .dark ? .white : .black)
      .frame(height: 44)
      .clipShape(RoundedRectangle(cornerRadius: theme.radius.control))
      .accessibilityIdentifier("sync.apple")
      // Google's own button (`SyncProviderButton`), in the theme's
      // appearance and at its control radius, beside Apple's native one.
      SyncProviderButton(.google, isDark: colorScheme == .dark, cornerRadius: theme.radius.control) {
        guard let server = chosenServer() else { return }
        Task { await controller.signIn(with: .google, serverURL: server) }
      }
      .accessibilityIdentifier("sync.google")
    } header: {
      header("Sign in")
    } footer: {
      Text("Keep this iPhone, your Mac and your other devices on one workspace. Every device keeps a full copy and works offline.")
        .font(theme.type.footnote).foregroundStyle(theme.muted)
    }
    .settingsRows()

    Section {
      // Apple and Google above are the usual way in; the email form is six
      // rows, so it waits behind one until it is wanted.
      SettingsDisclosure("Email and password", key: "sync.email") {
        TextField("Email", text: $email)
          .textContentType(.username)
          .keyboardType(.emailAddress)
          .textInputAutocapitalization(.never)
          .autocorrectionDisabled()
          .accessibilityIdentifier("sync.email")
        SecureField("Password", text: $password)
          .textContentType(.password)
          .submitLabel(.go)
          .onSubmit { signIn() }
          .accessibilityIdentifier("sync.password")
        Button("Sign in") { signIn() }
          .disabled(!canSubmit)
          .accessibilityIdentifier("sync.signIn")
        Button("Create account") {
          guard let server = chosenServer() else { return }
          notice = nil
          Task {
            notice = await controller.signUp(email: email, password: password, serverURL: server)
            if controller.signInError == nil { password = "" }
          }
        }
        .disabled(!canSubmit)
        .accessibilityIdentifier("sync.signUp")
        Button("Forgot password?") { requestPasswordReset() }
          .font(theme.type.callout)
          .foregroundStyle(theme.muted)
          .accessibilityIdentifier("sync.forgotPassword")
        if let notice {
          Text(notice)
            .font(theme.type.caption)
            .foregroundStyle(theme.muted)
            .accessibilityIdentifier("sync.notice")
        }
      }
    } header: {
      header("Or with email")
    } footer: {
      Text("A new password needs 8 characters.")
        .font(theme.type.footnote).foregroundStyle(theme.muted)
    }
    .settingsRows()

    Section {
      DisclosureGroup("Use a different server", isExpanded: $usesOtherServer) {
        TextField(SyncServer.defaultURL.absoluteString, text: $serverURL)
          .textInputAutocapitalization(.never)
          .autocorrectionDisabled()
          .keyboardType(.URL)
          .font(theme.type.callout)
          .accessibilityIdentifier("sync.server")
      }
    } footer: {
      Text("For a server you run yourself. Leave it as it is to use Takt's.")
        .font(theme.type.footnote).foregroundStyle(theme.muted)
    }
    .settingsRows()
    .onAppear(perform: prefill)
  }

  private var canSubmit: Bool {
    !email.trimmingCharacters(in: .whitespaces).isEmpty && !password.isEmpty
  }

  private func signIn() {
    guard canSubmit, let server = chosenServer() else { return }
    Task {
      if await controller.signIn(email: email, password: password, serverURL: server) { password = "" }
    }
  }

  /// The native sheet answered: hand Apple's identity token to Supabase.
  private func signInWithApple(_ result: Result<ASAuthorization, any Error>) {
    switch result {
    case .success(let authorization):
      guard let credential = authorization.credential as? ASAuthorizationAppleIDCredential,
        let data = credential.identityToken, let idToken = String(data: data, encoding: .utf8)
      else {
        controller.signInError = "Apple didn't send a sign-in token. Try again."
        return
      }
      guard let server = chosenServer() else { return }
      let nonce = appleNonce
      Task { await controller.signInWithApple(idToken: idToken, nonce: nonce, serverURL: server) }
    case .failure(let error):
      // Closing the sheet is an answer, not a failure.
      if (error as? ASAuthorizationError)?.code == .canceled { return }
      controller.signInError = error.localizedDescription
    }
  }

  /// Emails a reset link for the typed address.
  private func requestPasswordReset() {
    notice = nil
    Task { notice = await controller.requestPasswordReset(email: email) }
  }

  /// The hosted server, unless the disclosure names another.
  private func chosenServer() -> URL? {
    guard let url = SyncServer.url(from: serverURL) else {
      controller.signInError = "That server address isn't a web address."
      return nil
    }
    return url
  }

  /// After a refused session, the same email and server again.
  private func prefill() {
    if email.isEmpty, let remembered = session.rememberedEmail { email = remembered }
    if let server = session.rememberedServerURL, server != SyncServer.defaultURL {
      serverURL = server.absoluteString
      usesOtherServer = true
    }
  }

  private func header(_ text: String) -> some View {
    SettingsGroupLabel(text)
  }
}

/// Takt Sync on Takt's hosted server: whether this account has it, and the
/// £1-a-year purchase. A self-hosted server never asks, and never shows it.
private struct TaktSyncSection: View {
  let theme: Theme
  let controller: SyncController
  @State private var purchase = TaktSyncPurchase.shared

  /// Apple's standard licence agreement, which is Takt's terms of use.
  private static let termsURL = URL(string: "https://www.apple.com/legal/internet-services/itunes/dev/stdeula/")!
  private static let privacyURL = URL(string: "https://takt.maybeitssoftware.co.uk/privacy/")!

  private var session: SyncSession { controller.session }
  private var subscription: SyncSubscription? { session.account?.subscription }
  private var isActive: Bool { subscription?.active == true || purchase.isActive }

  var body: some View {
    Section {
      LabeledContent("Takt Sync") {
        Text(statusText).foregroundStyle(isActive ? theme.ink : theme.muted)
      }
      if !isActive {
        if purchase.isAvailable {
          Button {
            Task { await purchase.subscribe(then: refreshed) }
          } label: {
            Label("Subscribe — \(purchase.product?.localizedPriceString ?? "£1") a year", systemImage: "icloud")
          }
          .disabled(purchase.isWorking || purchase.product == nil)
          .accessibilityIdentifier("sync.subscribe")
        } else {
          Text("Purchases aren't available in this build.").foregroundStyle(theme.muted)
        }
      }
      if purchase.isAvailable {
        Button("Restore purchases") { Task { await purchase.restore(then: refreshed) } }
          .disabled(purchase.isWorking)
      }
      if let problem = purchase.problem {
        Text(problem).font(theme.type.footnote).foregroundStyle(theme.danger)
      }
    } header: {
      Text("Takt Sync").font(theme.type.caption).foregroundStyle(theme.muted).textCase(nil)
    } footer: {
      VStack(alignment: .leading, spacing: 6) {
        Text(
          "Syncing through Takt's server costs £1 a year, and one purchase covers every device on this account. "
            + "The server is open source: host it yourself and sync is free, with every feature."
        )
        // App Review guideline 3.1.2: the renewal terms and both links, beside the purchase.
        Text(
          "Takt Sync renews automatically each year unless you cancel at least 24 hours before it ends. "
            + "Cancel any time in Settings → your name → Subscriptions."
        )
        HStack(spacing: 12) {
          Link("Terms of Use", destination: Self.termsURL)
          Link("Privacy Policy", destination: Self.privacyURL)
        }
        .foregroundStyle(theme.ink)
        .underline()
      }
      .font(theme.type.footnote).foregroundStyle(theme.muted)
    }
    .settingsRows()
    .task(id: session.credentials?.accountId) {
      await purchase.prepare(account: session.credentials?.accountId)
    }
  }

  private var statusText: String {
    if subscription?.comp == true { return "Included" }
    guard isActive else { return "Not subscribed" }
    if let until = subscription?.expiresDate {
      return "Until \(until.formatted(date: .abbreviated, time: .omitted))"
    }
    return "Active"
  }

  /// After a purchase: the server hears from RevenueCat within seconds, so
  /// ask it again and sync.
  private func refreshed() async {
    try? await Task.sleep(for: .seconds(2))
    try? await session.refreshAccount()
    await controller.syncNow()
  }
}
