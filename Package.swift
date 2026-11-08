// swift-tools-version: 6.0
import PackageDescription

let pluginTargetExcludes = [
  // Top-level non-source artefacts
  "ARCHITECTURE_IMPROVEMENT_PLAN.md",
  "Takt.xcodeproj",
  "CLAUDE.md",
  // The Rust CLI crate. No Swift sources, but `path: "."` would otherwise walk
  // all of `cli/target` on every build.
  "cli",
  // The Rust core; its Swift face is the TaktRustCore target.
  "core",
  // The Rust desktop app: no Swift, and its build tree holds GPUI's shader
  // sources, which would otherwise count as this target's resources.
  "desktop",
  "README.md",
  "build",
  "corelogic-tests",
  "docs",
  "plugin-tests",
  "workspace-tests",
  "workspace-editing-tests",
  "Takt/Editing",
  "Sources/TaktWorkspace",
  "Sources/TaktSync",
  "sync-tests",
  "test-support",
  // The sync server and the phone apps: other build systems, and their build
  // trees are large enough that walking them slows every build.
  "sync-server",
  "mobile",
  "scripts",

  // App resources and the core target's own source tree
  "Takt/Assets.xcassets",
  "Takt/Fonts",
  "Takt/Takt.entitlements",
  "Takt/Takt.release.entitlements",
  "Takt/Info.plist",
  "Takt/WorkspaceDesktopView.swift",
  "Takt/WorkspaceViewModel.swift",

  // App-level source folders not needed by the plugins library
  "Takt/Managers",
  "Takt/Models",

  // App-level source files at Takt/ root
  "Takt/AppCoordinator.swift",
  "Takt/AppDelegate.swift",
  "Takt/LifecycleController.swift",
  "Takt/MainApp.swift",
  "Takt/DiagnosticsLog.swift",
  "Takt/DiagnosticsView.swift",
  "Takt/MainWindowController.swift",
  "Takt/MainWindowTitleStrip.swift",
  "Takt/WindowContentSizing.swift",
  // The SwiftUI projection of a `TaktCore` theme. SwiftUI, so app-only.
  "Takt/Theme.swift",
  // App-only: reads UserDefaults and Application Support directly at startup.
  "Takt/LegacyNameMigration.swift",
  "Takt/LegacyOfflineTaskStore.swift",
  "Takt/CheckvistAccount.swift",
  "Takt/PreferencesStore.swift",
  "Takt/SettingsNavState.swift",
  "Takt/SettingsView.swift",
  "Takt/SettingsChrome.swift",
  "Takt/CelebrationPreview.swift",
  "Takt/TaskDeletionPrompt.swift",
  "Takt/WorkspaceTaskDraftRow.swift",
  "Takt/SettingsFontPicker.swift",
  "Takt/SettingsView+AdvancedPane.swift",
  "Takt/SettingsView+AppearancePane.swift",
  "Takt/SettingsView+GeneralPane.swift",
  "Takt/SettingsView+KeyboardPane.swift",

  // Plugin subtrees / files that are app-only or conflict with PluginModelStubs
  "Takt/Plugins/Registry",
  "Takt/Plugins/Native/Checkvist/CheckvistAPIClient.swift",
  "Takt/Plugins/Native/Checkvist/CheckvistConnectionState.swift",
  "Takt/Plugins/Native/Checkvist/CheckvistSession.swift",
  "Takt/Plugins/Native/Checkvist/NativeCheckvistSyncPlugin+Settings.swift",
  "Takt/Plugins/Native/Google/GoogleOAuthLoopbackReceiver.swift",
  "Takt/Plugins/Native/Google/GoogleAccountSettingsSection.swift",
  "Takt/Plugins/Native/GoogleCalendar/NativeGoogleCalendarIntegrationPlugin+Settings.swift",
  "Takt/Plugins/Native/GoogleCalendar/GoogleCalendarCompletionWatcher.swift",
  "Takt/Plugins/Native/GoogleCalendar/GoogleCalendarTwoWayService.swift",
  "Takt/Plugins/Native/GoogleTasks/NativeGoogleTasksIntegrationPlugin+Settings.swift",
  "Takt/Plugins/Native/Links/NativeLinksPlugin+Settings.swift",
  // App-only: the mirror drives the workspace store, which lives in a module
  // `TaktPlugins` does not depend on.
  "Takt/Plugins/Native/GoogleTasks/GoogleTasksMirrorService.swift",
  "Takt/Plugins/Native/GoogleTasks/GoogleTasksMirrorStores.swift",
  // App-only: drives NSOpenPanel (AppKit). It also depends on `TaktCore`'s
  // catalog, which this target *can* now import, so the exclusion is worth
  // revisiting once the NSOpenPanel dependency is hoisted out.
  "Takt/Plugins/Native/MCP/MCPClientInstaller.swift",
  "Takt/Plugins/Native/MCP/NativeMCPIntegrationPlugin+Settings.swift",
  "Takt/Plugins/Protocols/PluginSettingsPageProviding.swift",
  // App-only: the daily-log plugin traffics in `TaktCore` types (`DayLogEvent`,
  // `DayBoundary`, `DayLogAggregator`). This target can import `TaktCore`
  // now, so the exclusion is historical and worth revisiting; the logic worth
  // testing lives in `Sources/TaktCore/` and is covered by `corelogic-tests`.
  "Takt/Plugins/Native/DailyLog",
  "Takt/Plugins/Protocols/DailyLogPluginProtocol.swift",
  // App-only: completion celebrations are motion, motion is SwiftUI, and
  // SwiftUI can't be in this target. The decision logic they render lives in
  // `Sources/TaktCore/CompletionMilestonePolicy.swift` and is covered by
  // `corelogic-tests`.
  "Takt/Plugins/Native/Celebration",
  "Takt/Plugins/Protocols/CompletionCelebrationPluginProtocol.swift",
  // App-only for the DailyLog reason exactly: a theme traffics in `TaktCore`
  // types (`ThemePalette`, `ThemeStructure`); the import would resolve now,
  // so this too is worth revisiting. The palette arithmetic worth testing is
  // already in `Sources/TaktCore/Theming/` under `corelogic-tests`.
  "Takt/Plugins/Native/Theme",
  "Takt/Plugins/Protocols/ThemePluginProtocol.swift",
]

let package = Package(
  name: "priority-core",
  // iOS is for the phone app in `mobile/ios`, which links the workspace
  // packages rather than re-implementing them. The plugin and app-logic targets
  // are only ever built for the Mac.
  platforms: [.macOS(.v15), .iOS(.v18)],
  products: [
    .library(name: "TaktCore", targets: ["TaktCore"]),
    .library(name: "TaktPlugins", targets: ["TaktPlugins"]),
    .library(name: "TaktWorkspace", targets: ["TaktWorkspace"]),
    .library(name: "TaktWorkspaceEditing", targets: ["TaktWorkspaceEditing"]),
    .library(name: "TaktSync", targets: ["TaktSync"]),
    .library(name: "TaktRustCore", targets: ["TaktRustCore"]),
  ],
  dependencies: [
    // Supabase Auth: accounts for sync (`docs/sync.md`). Only the `Auth`
    // product is linked, by `TaktSync`.
    .package(url: "https://github.com/supabase/supabase-swift.git", from: "2.55.0"),
  ],
  targets: [
    .target(
      name: "TaktCore",
      // The next-up ranking and the availability rules are the Rust core's.
      dependencies: ["TaktRustCore"],
      path: "Sources/TaktCore"
    ),
    // The Rust core (`core/`), compiled for every Apple slice by
    // `scripts/build_core_apple.sh`. The xcframework is a build product, not
    // committed, so the package resolves only once that script has run.
    // `docs/rust-core-migration.md` is the plan for what moves into it.
    .binaryTarget(
      name: "takt_coreFFI",
      path: "build/core/takt_coreFFI.xcframework"
    ),
    // The UniFFI bindings over it, regenerated by the same script and
    // committed so a diff shows when the boundary changed.
    .target(
      name: "TaktRustCore",
      dependencies: ["takt_coreFFI"],
      path: "Sources/TaktRustCore",
      // The core links the system SQLite on Apple platforms rather than
      // bringing its own, so nothing in the process has a second copy.
      linkerSettings: [.linkedLibrary("sqlite3")]
    ),
    .target(
      name: "TaktWorkspace",
      dependencies: [
        "TaktCore",
        // Every read and write, the schema and its migrations are the Rust
        // core's (docs/rust-core-migration.md).
        "TaktRustCore",
      ],
      path: "Sources/TaktWorkspace"
    ),
    // The client half of multi-device sync (`docs/sync.md`), shared by the Mac
    // and iOS apps. Android has its own in `mobile/android/data`.
    .target(
      name: "TaktSync",
      dependencies: [
        "TaktWorkspace",
        // The clock is the Rust core's, shared with the sync server.
        "TaktRustCore",
        .product(name: "Auth", package: "supabase-swift"),
      ],
      path: "Sources/TaktSync"
    ),
    .target(
      name: "TaktPlugins",
      // These sources compile into the Xcode app as well, and the app links
      // `TaktCore` rather than compiling it, so the module resolves on both
      // sides.
      // The Checkvist plugin calls the core (`core/src/checkvist.rs`) for
      // every request it makes and every answer it reads.
      dependencies: ["TaktCore", "TaktRustCore"],
      path: ".",
      exclude: pluginTargetExcludes,
      sources: [
        "Takt/Plugins/Protocols/PluginProtocols.swift",
        "Takt/Plugins/Native/Checkvist/NativeCheckvistSyncPlugin.swift",
        "Takt/Plugins/Native/Checkvist/CheckvistCredentialStore.swift",
        "Takt/Plugins/Native/Checkvist/CheckvistEndpoints.swift",
        "Takt/Plugins/Native/Checkvist/CheckvistModels.swift",
        "Takt/Plugins/Native/Checkvist/CheckvistModels+Core.swift",
        "Takt/Plugins/Native/Checkvist/CheckvistSessionError.swift",
        "Takt/Plugins/Native/Google/GoogleAccount.swift",
        "Takt/Plugins/Native/Google/GoogleOAuthTokenStore.swift",
        "Takt/Plugins/Native/GoogleCalendar/NativeGoogleCalendarIntegrationPlugin.swift",
        "Takt/Plugins/Native/GoogleTasks/NativeGoogleTasksIntegrationPlugin.swift",
        "Takt/Plugins/Native/Links/LinkCandidate.swift",
        "Takt/Plugins/Native/Links/ObsidianVaultIndex.swift",
        "Takt/Plugins/Native/Links/GoogleDriveSearch.swift",
        "Takt/Plugins/Native/Links/NativeLinksPlugin.swift",
        "Takt/Plugins/Native/MCP/NativeMCPIntegrationPlugin.swift",
        "Takt/Plugins/User/UserPluginManager.swift",
        "Takt/Plugins/User/UserPluginManifest.swift",
        "Takt/Plugins/Support/FormURLEncoding.swift",
        "plugin-tests-support/PluginModelStubs.swift",
      ]
    ),
    .target(
      name: "TaktWorkspaceEditing",
      dependencies: ["TaktWorkspace"],
      path: "Takt/Editing"
    ),
    // A second connection onto a workspace file, for tests that play another
    // process or look underneath the store, on the system SQLite.
    .target(
      name: "TestSQLite",
      path: "test-support/sqlite",
      linkerSettings: [.linkedLibrary("sqlite3")]
    ),
    .testTarget(
      name: "TaktCoreTests",
      dependencies: ["TaktCore"],
      path: "corelogic-tests"
    ),
    .testTarget(
      name: "TaktPluginTests",
      dependencies: ["TaktPlugins"],
      path: "plugin-tests"
    ),
    .testTarget(
      name: "TaktWorkspaceTests",
      // A connection of its own, so a migration test can write a fixture
      // database in the shape an older version of the app left behind.
      dependencies: ["TaktWorkspace", "TestSQLite"],
      path: "workspace-tests"
    ),
    .testTarget(
      name: "TaktRustCoreTests",
      dependencies: ["TaktRustCore"],
      path: "rustcore-tests"
    ),
    .testTarget(
      name: "TaktWorkspaceEditingTests",
      dependencies: ["TaktWorkspaceEditing", "TaktWorkspace"],
      path: "workspace-editing-tests"
    ),
    .testTarget(
      name: "TaktSyncTests",
      dependencies: [
        "TaktSync", "TaktWorkspace", "TestSQLite",
        .product(name: "Auth", package: "supabase-swift"),
      ],
      path: "sync-tests"
    ),
  ]
)
