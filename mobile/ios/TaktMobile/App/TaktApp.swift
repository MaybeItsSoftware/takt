import TaktSync
import TaktWorkspace
import SwiftUI

@main
struct TaktApp: App {
  @State private var boot: Boot
  @State private var themes: ThemeStore
  @Environment(\.scenePhase) private var scenePhase

  init() {
    let boot = Boot()
    let themes = ThemeStore()
    if let model = boot.model { themes.attach(to: model) }
    themes.install()
    _boot = State(initialValue: boot)
    _themes = State(initialValue: themes)
  }

  var body: some Scene {
    WindowGroup {
      Group {
        if let model = boot.model {
          RootView()
            .environment(model)
            .onOpenURL { url in DeepLink.handle(url, model: model) }
            .task { DeepLink.handleLaunchArguments(model: model) }
            .onChange(of: scenePhase) { _, phase in
              if phase == .active {
                model.startWatchingExternalWrites()
                // A start may have arrived while the app was away.
                model.refreshBlockedTaskIDs()
                Task { await model.checkExternalWrites() }
                SyncController.shared?.sceneBecameActive()
              } else {
                model.stopWatchingExternalWrites()
                if phase == .background { SyncController.shared?.sceneLeftForeground() }
              }
            }
        } else {
          LaunchFailureView(message: boot.failure ?? "")
        }
      }
      .environment(themes)
      .environment(\.theme, themes.theme)
      .preferredColorScheme(themes.colorScheme)
      .tint(themes.theme.primary)
      .font(themes.theme.type.body)
    }
    .backgroundTask(.appRefresh(SyncController.refreshTaskID)) {
      await SyncController.shared?.backgroundRefresh()
    }
  }
}

/// Opens the workspace once, for the life of the process.
@MainActor
@Observable
final class Boot {
  let model: WorkspaceModel?
  let failure: String?

  init() {
    do {
      let model = try WorkspaceModel.live()
      self.model = model
      self.failure = nil
      AppServices.install(on: model)
    } catch {
      self.model = nil
      self.failure = error.localizedDescription
    }
  }
}

/// Hooks for features that live outside the screens — the widget bridge, the
/// Live Activity — installed once the workspace is open. Each feature adds a
/// line here.
@MainActor
enum AppServices {
  static func install(on model: WorkspaceModel) {
    for installer in featureInstallers { installer(model) }
  }
}

struct LaunchFailureView: View {
  @Environment(\.theme) private var theme
  let message: String

  var body: some View {
    VStack(spacing: theme.space.md) {
      Image(systemName: "exclamationmark.triangle").font(theme.type.glyph(34)).foregroundStyle(theme.danger)
      Text("Takt couldn't open its workspace").font(theme.type.title)
      Text(message).font(theme.type.caption).foregroundStyle(theme.muted).multilineTextAlignment(.center)
    }
    .padding(theme.space.xl)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background(theme.paper)
  }
}

/// `takt://` links from the widgets, the Control Center control and the
/// Live Activity.
@MainActor
enum DeepLink {
  /// DEBUG: `-demo` seeds a sample workspace and `-open <url>` follows a
  /// link at launch, so screenshots can be scripted without tapping.
  static func handleLaunchArguments(model: WorkspaceModel) {
    #if DEBUG
    let arguments = ProcessInfo.processInfo.arguments
    if arguments.contains("-demo"), model.structure.lists.count <= 1 { model.seedDemo(); model.reloadStructureNow() }
    if arguments.contains("-seed5000"), !model.structure.lists.contains(where: { $0.name.hasPrefix("Seed") }) {
      model.seedTasks()
    }
    if let index = arguments.firstIndex(of: "-open"), arguments.indices.contains(index + 1),
      let url = URL(string: arguments[index + 1]) {
      handle(url, model: model)
    }
    // `-inspect <title>` selects a task, so the iPad's inspector has one;
    // after a beat, as switching root views clears the selection.
    if let title = UserDefaults.standard.string(forKey: "inspect"),
      let hit = try? model.store.searchTasks(in: model.workspace.id, matching: title).first(where: { $0.task.title == title }) {
      Task {
        try? await Task.sleep(for: .seconds(1))
        model.navigation.inspect(hit.task.id, isPad: UIDevice.current.userInterfaceIdiom == .pad)
      }
    }
    #endif
  }

  static func handle(_ url: URL, model: WorkspaceModel) {
    if SyncServer.isAuthCallback(url) {
      // Supabase's emails (confirming an address, resetting a password) come
      // back here. Settings shows the attempt and, if it fails, why.
      guard let sync = SyncController.shared else {
        model.errorMessage = "Sync isn't available in this build."
        return
      }
      model.navigation.isSettingsPresented = true
      model.navigation.isSyncSettingsPresented = true
      Task {
        await sync.openAuthLink(url)
        if sync.session.isSignedIn { model.showToast("Signed in to sync") }
      }
      return
    }
    guard url.scheme == "takt" else { return }
    let isPad = UIDevice.current.userInterfaceIdiom == .pad
    switch url.host() {
    case "add":
      model.navigation.quickAddListID = nil
      model.navigation.quickAddParentTaskID = nil
      model.navigation.isQuickAddPresented = true
    case "join":
      // An invite to a shared list, from the server's join page.
      model.navigation.joinInvite = JoinInvite(text: url.absoluteString)
    case "today": model.navigation.go(to: .today, isPad: isPad)
    case "focus":
      model.navigation.go(to: .focus, isPad: isPad)
      #if DEBUG
      if let title = query(url, "task") { startDemoBlock(title: title, model: model) }
      #endif
    case "review": model.navigation.go(to: .review, isPad: isPad)
    case "search": model.navigation.go(to: .search, isPad: isPad)
    case "lists":
      model.navigation.go(to: .lists, isPad: isPad)
      if let name = url.pathComponents.dropFirst().first?.removingPercentEncoding,
        let list = model.structure.lists.first(where: { $0.name == name }) {
        model.navigation.open(.list(list.id), isPad: isPad)
      }
    #if DEBUG
    case "demo": model.seedDemo()
    case "seed": model.seedTasks()
    #endif
    case "task":
      if let id = url.pathComponents.dropFirst().first {
        model.navigation.go(to: .today, isPad: isPad)
        model.navigation.inspect(id, isPad: isPad)
      }
    default: break
    }
  }

  #if DEBUG
  /// DEBUG: a query item off a `takt://` link, for the screenshot routes.
  static func query(_ url: URL, _ name: String) -> String? {
    URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?.first { $0.name == name }?.value
  }

  /// DEBUG: `takt://focus?task=<title>` starts a block on the task with that
  /// title, begun a minute and a half ago so the timer is visibly counting.
  static func startDemoBlock(title: String, model: WorkspaceModel) {
    let workspaceID = model.workspace.id
    model.perform { store in
      guard let hit = try store.searchTasks(in: workspaceID, matching: title).first(where: { $0.task.title == title })
      else { return }
      _ = try store.startFocusSession(
        taskId: hit.task.id, plannedSeconds: hit.task.estimateSeconds ?? 1_500, overrideAvailability: true,
        now: .now.addingTimeInterval(-95))
    }
  }
  #endif
}
