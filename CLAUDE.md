# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Takt (formerly Priority) is a keyboard-first macOS desktop app (macOS 15.6+, Xcode 17+) with a menu bar surface beside it. The code says Takt too — the Xcode project, scheme and targets, the `Takt/` source folder, the Swift modules and the Rust crates — except where the old name is stored data: keychain service names, window autosave names, the iPhone and Android data folders inside their containers, the `X-Priority-Device` sync header, the `PRIORITY_*` runtime environment variables, and the built-in `priority` theme. Its own local workspace is the source of truth; Checkvist is an optional import/integration edge. `DESKTOP_WORKSPACE_ROADMAP.md` is authoritative for where that transition has got to. It is an Xcode app with a Swift Package layered on top — `Package.swift` exposes `TaktCore` (pure logic) and `TaktPlugins` (integration plugins) as SPM library targets that share source with the Xcode project, beside the workspace, sync and Rust-core targets.

`cli/` is a separate Rust crate producing `takt`. Bare `takt` opens a ratatui terminal UI whose tabs mirror the app's root views; `cli.rs`, `tui/` and `mcp.rs` are three front ends onto the single tool table in `tools.rs`, so none of them can implement behaviour the others lack. It is a command-line peer of the app that talks to the Checkvist API directly and reads the same local files. It also **writes the app's workspace database** (folders, lists, tasks: `cli/src/workspace_tasks.rs`) while the app is running. Each write calls the Rust core (`core/src`), the same function the apps call, inside an "MCP: …" undo step bracketed by the core's `journal::begin` and `journal::finish`. The app sees these writes because `WorkspaceViewModel+ExternalWrites.swift` polls `PRAGMA data_version`. A write changes in one place, the core, and every client picks it up. Migrations are no longer per client: they live in the Rust core (`core/src/schema`), and `scripts/dump_workspace_schema.sh` regenerates the tests' schema fixture from it. See `docs/mcp-server.md`. It shares no source with the Swift side, and the only thing in `Takt/` that may reference it is `MCPServerShim.swift`: the app bundles the CLI as a signed helper at `Contents/Helpers/takt` (see `scripts/bundle_cli.sh`) and `Takt --mcp-server` `execv`s it. That is the app's MCP server — there is no other. Consequently **an app build needs cargo**; set `TAKT_SKIP_CLI_BUNDLE=1` to skip it, at the cost of an app with no MCP server. Its credentials are deliberately its own (`~/.config/takt/config.json`, falling back to the old `~/.config/priority/config.json`; see `cli/src/config.rs`) rather than the app's keychain item, which is reachable only by something carrying the app's code signature. See `docs/cli.md`.

## Build, Run, Test

```bash
# Full app build (the canonical "does it compile" check)
xcodebuild -project 'Takt.xcodeproj' -scheme 'Takt' -configuration Debug -destination 'platform=macOS' build

# Run all SPM unit tests (one test target per library)
swift test

# Run a single test by filter (XCTest style)
swift test --filter TaktCoreTests.CommandEngineCommandParsingTests/testParseSimpleKeywordCommands

# Build + launch the Debug app (kills any running instance first)
./scripts/run.sh

# Produce a release DMG
./scripts/build_dmg.sh <version>

# The Rust CLI
cargo test --manifest-path cli/Cargo.toml
cargo clippy --manifest-path cli/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path cli/Cargo.toml --check
./scripts/install_cli.sh            # release build + a `takt` symlink onto PATH
```

`README.md` is authoritative for keybindings and command palette syntax — consult it when editing `core/src/keymap/catalogue.rs` (the command catalogue, which the Mac, the iPhone and Android all read; a new command is an entry there plus a `WorkspaceCommandID` case and its `run` arm), `Sources/TaktCore/WorkspaceKeyBindings.swift` or `CommandEngine.swift` so behaviour stays in sync.

## Architectural Layout (Two Build Systems, One Source Tree)

The same files are compiled by two different systems, which is the most important thing to know before editing:

1. **Xcode project** (`Takt.xcodeproj`) — builds the actual macOS app from everything under `Takt/`.
2. **Swift Package** (`Package.swift`) — builds its libraries from real directories, except `TaktPlugins` and `TaktWorkspaceEditing`, which are subsets of `Takt/`:
   - `TaktCore` — sources rooted at `Sources/TaktCore/`. Pure, headless logic only (command parser, recurrence, timer policies, the visibility and shortcut engines). This is what `corelogic-tests/` exercises. **The app links this one** rather than compiling its sources, so everything it uses across that boundary is `public`.
   - `TaktPlugins` — explicit `sources:` list of plugin files plus `plugin-tests-support/PluginModelStubs.swift` (which provides minimal stub models so plugin code compiles without the app shell). Tested by `plugin-tests/`.
   - `TaktWorkspace` — sources rooted at `Sources/TaktWorkspace/`: `WorkspaceStore` and its `+*.swift` extensions and the models. Every read and write, the schema and its migrations are the Rust core's: `WorkspaceStore` holds one `CoreWorkspace` and turns its rows into the models (`WorkspaceRecords+Core.swift`, `WorkspaceRows+Core.swift`). There is no GRDB any more; `test-support/sqlite` (`TestSQLite`) gives tests a connection of their own. **The app links this one too.** Tested by `workspace-tests/`.
   - `TaktWorkspaceEditing` — `Takt/Editing/`, the task editor drafts. Tested by `workspace-editing-tests/`.
   - `TaktSync` — `Sources/TaktSync/`, the client of `sync-server/`. Linked by the app. Tested by `sync-tests/`.

Consequences when editing:

- `Package.swift` has a large `pluginTargetExcludes` list and an explicit `sources:` list for the `path: "."` target. Adding a new plugin file requires updating them, or `swift test` starts failing even though Xcode still builds. `Sources/TaktCore/` needs no such bookkeeping — it is a real single-target directory, so a new file there is picked up by both builds.
- `TaktCore` must stay free of AppKit/SwiftUI/UI dependencies — it is consumed by the test target without the app. New declarations there need `public` to be visible to the app, and a `public struct` needs an explicit `public init` (the synthesised memberwise one is internal).
- `TaktPlugins` deliberately excludes each plugin's `+Settings.swift` extension and any service that pulls in app types (e.g. `GoogleTasksMirrorService.swift`, `MCPClientInstaller.swift`). Keep cross-plugin / app-only types out of the curated `sources:` list.
- **A file can only belong to one SPM target.** That still holds for `TaktPlugins`, whose sources are compiled by both build systems. It *can* `import TaktCore`, because the app links that module rather than compiling its sources, so the import resolves on both sides.

## App Composition

- `MainApp.swift` is a near-empty `@main` that installs `AppDelegate` via `NSApplicationDelegateAdaptor`. The activation policy is **not** fixed: `applicationDidFinishLaunching` sets `.regular` because the desktop window is the launch surface, and `applyActivationPolicy(hasOrdinaryWindow:)` drops back to `.accessory` once the last ordinary window closes, so a menu-bar-only session keeps no Dock icon. `INFOPLIST_KEY_LSUIElement` is `NO` accordingly. Anything that assumes "menu bar only" — including where the app icon is visible — is reading a policy the app left behind.
- `AppDelegate` is the composition root: it owns the singleton `AppCoordinator` (constructed with `PluginRegistry.nativeFirst()`), the `WorkspaceViewModel`, the `MenuBarController` (the status item and its menu — the popover it used to host is gone), and the `GlobalShortcutManager` (Carbon hotkeys for show-window, focus-panel and quick-add). It also runs the Checkvist import: on activation (throttled) and from the settings page's Import button it loads the account's lists and hands the ones the workspace lacks to `WorkspaceViewModel.importCheckvistLists`.
- **MCP launch mode**: `TaktEntryPoint.main()` in `MainApp.swift` checks for `--mcp-server` and hands the process to `MCPServerShim.run()`, which `execv`s the bundled CLI. This runs *before* `MainApp.main()`, so a process that only speaks JSON-RPC on stdio never initialises AppKit. Preserve that ordering when refactoring startup. See `docs/mcp-server.md`. Straight after that check, and before anything reads preferences or Application Support, `LegacyNameMigration.runIfNeeded()` (`Takt/LegacyNameMigration.swift`) copies — never moves — whatever the old `Application Support/Priority` (and `Bar Tasker`) folder holds that `Takt/` lacks, and the old preferences domain's keys; the MCP server skips it because the CLI falls back to the old locations itself.
- `AppCoordinator` is the composition object the workspace leans on for preferences, integrations, themes, celebrations, the daily log's notes, diagnostics and Checkvist. The Checkvist-era task stack (`TaskRepository`, `SyncService`, `TaskMutationService`, `UndoService`, `NavigationState`, the offline store and the list-scoped stores, with the `TaktAppLogic` target that held them) was deleted on 2026-10-13: the workspace is the only task model. What is left of Checkvist is `CheckvistAccount` (`Takt/CheckvistAccount.swift`): the account, its lists, and which of them the workspace copies. Its requests and answers are the Rust core's (`core/src/checkvist.rs`), sent by `NativeCheckvistSyncPlugin` through `URLSession`; the key stays in the keychain. The desktop window reads `WorkspaceViewModel`, which refreshes itself. See `docs/state-ownership.md`.

## Plugin Architecture

All external integrations (the Checkvist import, Google Calendar, Google Tasks, MCP, the daily log and its Obsidian daily notes) are plugins behind protocols in `Takt/Plugins/Protocols/PluginProtocols.swift`. Native implementations live one folder per plugin under `Takt/Plugins/Native/<Name>/`, registered through `PluginRegistry` (`PluginRegistry.nativeFirst()` is the production factory).

Conventions enforced by `docs/plugins.md`:

- One folder per plugin; do **not** put plugin-specific services or models at the app root.
- Plugin settings UI lives in a plugin-local `<PluginName>+Settings.swift` extension that conforms to `PluginSettingsPageProviding`. `SettingsView` enumerates active plugins generically — never add `switch`/`if`-by-plugin logic there.
- New plugin files that the SPM `TaktPlugins` target needs must be added to the explicit `sources:` list in `Package.swift`; UI/`+Settings.swift` files stay app-only and should be left out (or excluded).

`NativeDailyLogPlugin` is the one deliberate exception to "contracts live in `Protocols/PluginProtocols.swift`, implementations compile into `TaktPlugins`". Its contract sits in its own file (`Protocols/DailyLogPluginProtocol.swift`) and the whole `Native/DailyLog/` folder is excluded from the `TaktPlugins` target, because it depends on `TaktCore` types (`DayLogEvent`, `DayBoundary`, `DayLogAggregator`) — the same one-file-one-target constraint that keeps `MCPClientInstaller.swift` app-only (it depends on `TaktCore` types too, which `TaktPlugins` can now import — so this exclusion is worth revisiting). Its testable logic lives in `Sources/TaktCore/` instead. Nothing records task events into its log any more: its events are keyed by Checkvist task id, and only the deleted Checkvist mutation path wrote them; the plugin still writes the closed days' notes and keeps the plugin-era dailies. See `docs/plugins.md`.

## Conventions and Tooling

- SwiftLint config (`.swiftlint.yml`) is intentionally permissive: many style-only rules disabled, `file_length` warning at 800 / error at 1500, `function_body_length` warning at 150, `cyclomatic_complexity` warning at 25. Don't gratuitously split files just to satisfy stricter defaults. CI runs `swiftlint lint` (not `--strict`), so **warnings are advisory and errors block**; the standing backlog is a handful of length and complexity warnings on the large files, counted in `TODO.md` rather than suppressed. Don't add to it. Run it as `swiftlint lint` from the root; it is a `--quiet` run of `Takt` and `Sources` that matters, and the config excludes `.claude` and the iOS build folder, which otherwise crash SourceKit.
- Logging uses `os.Logger` with subsystem `uk.co.maybeitssoftware.takt`; reuse this subsystem with a category that matches the type. `AppIdentity.bundleIdentifier` in `Sources/TaktCore/AppIdentity.swift` holds it, alongside `AppIdentity.applicationSupportDirectory()`, which every Application Support path should go through rather than spelling out `Takt/`. Keychain service names deliberately keep the old `uk.co.maybeitsadam.priority` prefix: they are storage keys, not identity.

## Verifying Changes

After any plugin or core-logic change, run both:

```bash
xcodebuild -project 'Takt.xcodeproj' -scheme 'Takt' -configuration Debug -destination 'platform=macOS' build
swift test
swiftlint lint          # `brew install swiftlint`; errors block, warnings don't
```

Xcode catches app-only breakage; `swift test` catches breakage in `TaktCore`/`TaktPlugins`/the workspace targets (including `Package.swift` source-list drift).

After changing anything under `cli/`, also run:

```bash
cargo test --manifest-path cli/Cargo.toml
cargo clippy --manifest-path cli/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path cli/Cargo.toml --check
```

After changing the MCP server (`cli/src/`) or the handover (`Takt/MCPServerShim.swift`, `scripts/bundle_cli.sh`), also run:

```bash
python3 scripts/mcp_smoke_check.py
```

There used to be two implementations of the same MCP server — one in Swift inside the app, one in the CLI — held equal from the outside by `scripts/mcp_parity_check.py`, because neither could import the other. The Swift one is gone: the app bundles the CLI and `--mcp-server` execs it, so there is one implementation to be right instead of two to keep equal. `cargo test` covers the server; the smoke check covers the seam, and specifically that a client configuration written before that change — naming the app's executable with `--mcp-server` and credentials in `env` — still reaches a working server. It checks `Takt --mcp-server`; a configuration naming the pre-rename `Priority.app` works only while that bundle exists, and the app's MCP setup replaces such an entry with a `takt` one. Needs a Debug app build; reads no real data and needs no credentials.

### The Rust desktop app

`desktop/` (`takt-desktop`) is the Mac app being rebuilt in Rust on GPUI,
beside the Swift one, which stays. `docs/rust-desktop-plan.md` is the plan and
says which stage it is on. It links `takt-core` as a Rust crate, and it is
styled after Zed (One Dark / One Light, IBM Plex Sans, Zed's chrome), not
after the Swift app or a house style. A write patches the rows held in
memory; it never rereads the scope. After changing it:

```bash
cargo test --manifest-path desktop/Cargo.toml
cargo clippy --manifest-path desktop/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path desktop/Cargo.toml --check
TAKT_DESKTOP_DB=<a copy of priority.sqlite> TAKT_DESKTOP_SMOKE=1 TAKT_DESKTOP_SMOKE_WRITE=1 \
  cargo run --release --manifest-path desktop/Cargo.toml   # draws a frame, prints timings, quits
```

Never drive it with simulated keystrokes (`osascript … keystroke`): they go to
whatever is frontmost, which is usually the user's terminal.

### The Rust core

`core/` is `takt-core`, the shared Rust crate the workspace logic is moving
into (`docs/rust-core-migration.md` is the plan and records which step it is
on). Every client calls it through UniFFI: the Swift package's
`TaktRustCore` target wraps `build/core/takt_coreFFI.xcframework`, and the
Android data module loads `libtakt_core.so` through JNA. Neither library is
committed, so **a fresh checkout cannot resolve the Swift package until
`scripts/build_core_apple.sh` has run**. Every build script and CI job calls
it first (`TAKT_CORE_IF_STALE=1` skips it when `core/` is unchanged). Gradle
runs `scripts/build_core_android.sh` through `buildRustCore` before
`preBuild`. The generated bindings are committed: `Sources/TaktRustCore/` and
`mobile/android/core/src/main/java/uniffi/` (in `:core`, so its ranking
rules can call the core; `:data` ships the native libraries and JNA). Never
edit them by hand.

After changing anything under `core/`:

```bash
cargo test --manifest-path core/Cargo.toml
cargo clippy --manifest-path core/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path core/Cargo.toml --check
scripts/build_core_apple.sh          # rebuilds the xcframework and Swift bindings
swift test --filter TaktRustCoreTests
(cd mobile/android && ./gradlew :data:testDebugUnitTest)   # rebuilds the .so and Kotlin bindings
```

Then commit the regenerated bindings alongside the Rust change, and run the
Mac, iPhone and Android gates below. A bindings checksum mismatch fails the
first call across the boundary, which is what the two version tests catch.

### Phones and sync

The iPhone app (`mobile/ios`, XcodeGen) and the Android app (`mobile/android`, Gradle) share the workspace with the Mac through `sync-server/` (Railway). The protocol is `docs/sync.md`. The theme format and its per-platform rules are in `docs/themes.md`. The shared fixtures they're held to are `cli/src/fixtures/workspace_schema.sql` and `shared/themes/`.

- **Schema changes** are one change, in the Rust core, and every client picks it up: the Mac and iPhone through `WorkspaceStore.init`, Android through `WorkspaceDatabase.open`. Add the migration at the end of `MIGRATIONS` in `core/src/schema/mod.rs` with its SQL file beside it. If it adds or removes a column on a journalled or synced table, call `triggers::reinstall` at the end of it. Then regenerate the fixture (`scripts/dump_workspace_schema.sh`) and commit it; the core's, the CLI's and Android's tests all check against it. Identifiers are permanent: a database records which ones it ran.
- **Theme format changes** go into the Rust core, `core/src/theme/`; `Sources/TaktCore/Theming` and Android's `core/theme` only wrap it. Then run the core's gates, rerun `TAKT_REGENERATE_THEMES=1 swift test --filter ThemeConformance` and commit the regenerated `shared/themes`. Android must still pass every conformance case.

After changing anything under `mobile/ios`:

```bash
(cd mobile/ios && xcodegen generate)
xcodebuild -project mobile/ios/TaktMobile.xcodeproj -scheme TaktMobile -destination 'generic/platform=iOS Simulator' build-for-testing
./scripts/install_ios.sh            # Release build onto the booted simulator, or DEVICE_ID=<udid>
```

After changing anything under `mobile/android` (Gradle builds share `/tmp/priority-gradle.lock`):

```bash
(cd mobile/android && ./gradlew :core:test :data:testDebugUnitTest :app:testDebugUnitTest :wearcontract:test :wear:testDebugUnitTest lint :app:assembleDebug :wear:assembleDebug :app:assembleRelease :wear:assembleRelease)
./scripts/install_android.sh        # onto the connected device or emulator
./scripts/build_play_bundle.sh      # signed .aab for the Play Console, into build/play/
```

After changing `sync-server/`:

```bash
cargo test --manifest-path sync-server/Cargo.toml --workspace   # integration tests need a local Postgres; see sync-server/README.md
cargo clippy --manifest-path sync-server/Cargo.toml --workspace --all-targets -- -D warnings
cargo fmt --manifest-path sync-server/Cargo.toml --all --check
```

`sync-server/rules/` (`takt-sync-rules`) is the clock and the merge, and the Rust core depends on it, so a change there also needs the core's and clients' gates above. It sits inside `sync-server/` because Railway builds from that directory alone.

## Working Loop

These happen on every piece of work here without being asked for.

**Commit as you go, straight onto `main`.** Not one commit at the end — one per
coherent change, each of them green. This is a deliberate override of the
global "commit only when asked; branch first on the default branch" rule:
`main` is the working branch in this repo. Match the existing subject style —
conventional prefix, lowercase, a sentence saying what changed, no trailing
period; `git log` is the reference. A change whose gates don't pass isn't a
commit, so run *Verifying Changes* above first, plus the Rust and MCP gates
when they apply.

Where the work splits into several commits, order them so each one's tree
builds on its own — a history you can't bisect is a history you can only read.
`git worktree add` on a candidate commit checks that without disturbing the
working tree.

**Reinstall the app, every time, yourself.**

```bash
./scripts/install_local.sh
```

This is already the standing instruction in `AGENTS.md`, repeated here because
that file is not always loaded. It builds Release, supersedes
`/Applications/Takt.app` outright — no backup is kept, and an old
`/Applications/Priority.app` is removed too — and relaunches. It is part of
finishing the work, not a step to hand back. A passing `xcodebuild` is not
completion: what the user actually runs is the installed bundle, and until it
is replaced every fix is still only a claim. Check the output for
`** BUILD SUCCEEDED **` and for the `Installed.` line, and report a failure
rather than a summary. The workflow is authorised; do not ask first.

**Ship phone changes to Play internal testing, yourself.** After anything
under `mobile/android` (or `core/`, which the phone links), once the Android
gates pass and the change is committed:

```bash
./scripts/build_play_bundle.sh
PLAY_JSON_KEY_FILE=~/.config/takt/play-service-account.json \
  ./scripts/upload_play_internal.sh build/play/takt-<version>-<code>.aab
```

The user asked for this standing on 2026-10-18 ("for newest builds upload to
play internal testing"): the internal track, not USB, is how the newest build
reaches their phone, so it is part of finishing the work and needs no asking.
fastlane's log ends on an update banner, so grep it for errors rather than
reading the tail. "Version code N has already been used" means the code
repeated — rerun with `TAKT_VERSION_CODE` set higher. If a phone is on adb,
`./scripts/install_android.sh` as well. When there is a Wear OS bundle
(`scripts/build_wear_bundle.sh`), it uploads the same way, with its own
version code.

**Parallel agents share one tree here.** The disk has room for roughly one
extra checkout's build output, not three (`core/target` alone is ~6 GB), so
agents work in the main tree on disjoint files rather than in worktrees.
Each commits only its own paths (`git commit -- <paths>`, never `git add -A`;
where a file holds another agent's uncommitted hunks, stage your diff with
`git apply --cached`). Only the agent changing `core/` regenerates bindings.

**Reinstall after anything under `cli/`.**

```bash
./scripts/install_cli.sh
```

The installed command is a *symlink* at `~/.local/bin/takt` pointing into
`cli/target/release/`, so what actually matters is that a **release** build is
current — the symlink then updates for free. Two consequences:

- `cargo build` (debug) refreshes neither the installed command nor the helper
  the app ships: `scripts/bundle_cli.sh` copies the *release* binary into
  `Contents/Helpers/takt`. A debug-only build leaves both stale while every
  test still passes, which is exactly how a stale MCP server goes unnoticed.
- `rm -rf cli/target` breaks the installed command rather than leaving an old
  copy behind.

A new entry in `cli/src/tools.rs` needs a front end on **each** side that can
express it — a `Command` case and `resolve` arm in `cli.rs` as well as the
declaration in `mcp.rs` — because the point of the shared tool table is that a
terminal can reach anything an assistant can. It also needs
`EXPECTED_TOOL_COUNT` in `scripts/mcp_smoke_check.py` and the count in
`docs/mcp-server.md` updating; the smoke check fails on the number, which is
the intended reminder. And it needs classifying as a read or a write in
`core/src/agent_tools.rs`, which the agent panel uses to decide what to
pre-allow; a CLI test fails until it is.
