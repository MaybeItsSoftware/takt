# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Takt (formerly Priority) is a keyboard-first macOS desktop app (macOS 15.6+) with a menu bar surface beside it, plus an iPhone app, an Android app and a `takt` CLI, all sharing one workspace through a sync server. The code says Takt too — the Swift modules, the Rust crates — except where the old name is stored data: keychain service names, window autosave names, the iPhone and Android data folders inside their containers, the `X-Priority-Device` sync header, the `PRIORITY_*` runtime environment variables, and the built-in `priority` theme. Its own local workspace is the source of truth; Checkvist is an optional import/integration edge.

**The Mac app is `desktop/`, in Rust on GPUI.** The Swift Mac app that used to live under `Takt/` (with `Takt.xcodeproj`, the `TaktPlugins` target and `plugin-tests/`) was retired on 2026-11-08; git history before that date holds it. Docs written before then (`docs/plugins.md`, `docs/state-ownership.md`, the plans and audits) describe it; read them as history, not as the current shape. The desktop app still *adopts* what the Swift app left on disk — its preferences domain and Application Support files — and that adoption code stays.

`cli/` is a separate Rust crate producing `takt`. Bare `takt` opens a ratatui terminal UI whose tabs mirror the app's root views; `cli.rs`, `tui/` and `mcp.rs` are three front ends onto the single tool table in `tools.rs`, so none of them can implement behaviour the others lack. It is a command-line peer of the app that talks to the Checkvist API directly and reads the same local files. It also **writes the app's workspace database** (folders, lists, tasks: `cli/src/workspace_tasks.rs`) while the app is running. Each write calls the Rust core (`core/src`), the same function the apps call, inside an "MCP: …" undo step bracketed by the core's `journal::begin` and `journal::finish`. A write changes in one place, the core, and every client picks it up. Migrations live in the Rust core (`core/src/schema`), and `scripts/dump_workspace_schema.sh` regenerates the tests' schema fixture from it. See `docs/mcp-server.md`. The app bundles the CLI as a signed helper at `Contents/Helpers/takt` (`scripts/install_desktop.sh` copies the release build in) and `Takt --mcp-server` `execv`s it (`exec_mcp_server` in `desktop/src/platform/mod.rs`). That is the app's MCP server — there is no other. Its credentials are deliberately its own (`~/.config/takt/config.json`, falling back to the old `~/.config/priority/config.json`; see `cli/src/config.rs`) rather than the app's keychain item. See `docs/cli.md`.

## Build, Run, Test

```bash
# The Mac app: release build, bundle, install as /Applications/Takt.app, relaunch
./scripts/install_local.sh

# The bundle alone, at build/desktop/Takt.app
TAKT_BUNDLE_ONLY=1 ./scripts/install_desktop.sh

# A signed, notarised DMG
./scripts/build_desktop_dmg.sh <version>

# The Swift package the iPhone app links (one test target per library)
swift test
swift test --filter TaktCoreTests.CommandEngineCommandParsingTests/testParseSimpleKeywordCommands

# The Rust CLI
cargo test --manifest-path cli/Cargo.toml
cargo clippy --manifest-path cli/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path cli/Cargo.toml --check
./scripts/install_cli.sh            # release build + a `takt` symlink onto PATH
```

`README.md` is authoritative for keybindings and command palette syntax — consult it when editing `core/src/keymap/catalogue.rs` (the command catalogue, which the Mac, the iPhone and Android all read) so behaviour stays in sync.

## The Swift Package

`Package.swift` now exists for the iPhone app (`mobile/ios`), which links its libraries rather than re-implementing them, and for `swift test`. Every target is a real directory under `Sources/`:

- `TaktCore` — pure, headless logic (command parser, recurrence, timer policies, theming, the visibility and shortcut engines). Tested by `corelogic-tests/`. Must stay free of UIKit/SwiftUI; a `public struct` needs an explicit `public init`.
- `TaktWorkspace` — `WorkspaceStore` and its `+*.swift` extensions and the models. Every read and write, the schema and its migrations are the Rust core's: `WorkspaceStore` holds one `CoreWorkspace` and turns its rows into the models (`WorkspaceRecords+Core.swift`, `WorkspaceRows+Core.swift`). `test-support/sqlite` (`TestSQLite`) gives tests a connection of their own. Tested by `workspace-tests/`.
- `TaktWorkspaceEditing` — the task editor drafts. Tested by `workspace-editing-tests/`.
- `TaktSync` — the client of `sync-server/`. Tested by `sync-tests/`.
- `TaktRustCore` — the core's generated UniFFI bindings (see *The Rust core*).

Fonts every client bundles live in `shared/fonts/` (the desktop `include_bytes!`es them, the iPhone and widget targets copy them as resources). The Mac icon's PNGs are `desktop/icon/`, written by `scripts/build_icons.sh`.

## Conventions and Tooling

- SwiftLint config (`.swiftlint.yml`) is intentionally permissive: many style-only rules disabled, `file_length` warning at 800 / error at 1500, `function_body_length` warning at 150, `cyclomatic_complexity` warning at 25. Don't gratuitously split files just to satisfy stricter defaults. CI runs `swiftlint lint` (not `--strict`), so **warnings are advisory and errors block**. Run it as `swiftlint lint` from the root; the config excludes `.claude` and the iOS build folder, which otherwise crash SourceKit.
- Swift logging uses `os.Logger` with subsystem `uk.co.maybeitssoftware.takt`; reuse this subsystem with a category that matches the type. `AppIdentity.bundleIdentifier` in `Sources/TaktCore/AppIdentity.swift` holds it. Keychain service names deliberately keep the old `uk.co.maybeitsadam.priority` prefix: they are storage keys, not identity.

## Verifying Changes

After changing the Swift package (`Sources/`, its tests):

```bash
swift test
swiftlint lint          # `brew install swiftlint`; errors block, warnings don't
```

and the iPhone gates below, since it is the package's consumer.

After changing anything under `cli/`, also run:

```bash
cargo test --manifest-path cli/Cargo.toml
cargo clippy --manifest-path cli/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path cli/Cargo.toml --check
```

After changing the MCP server (`cli/src/`) or the handover (`exec_mcp_server` in `desktop/src/platform/mod.rs`, the helper step in `scripts/install_desktop.sh`), also run:

```bash
python3 scripts/mcp_smoke_check.py
```

`cargo test` covers the server; the smoke check covers the seam, and specifically that a client configuration written before the CLI became the server — naming the app's executable with `--mcp-server` and credentials in `env` — still reaches a working server. It uses `build/desktop/Takt.app` (left by `install_local.sh` or `TAKT_BUNDLE_ONLY=1 scripts/install_desktop.sh`), else `/Applications/Takt.app`; reads no real data and needs no credentials.

### The Rust desktop app

`desktop/` (`takt-desktop`) is the Mac app, on GPUI; it is what
`install_local.sh` installs as `Takt.app` (bundle id
`uk.co.maybeitssoftware.takt`, executable `Contents/MacOS/Takt`).
`docs/rust-desktop-plan.md` is the plan and records its stages. It links
`takt-core` as a Rust crate, and it is styled after Zed (One Dark / One
Light, IBM Plex Sans, Zed's chrome), not after a house style. A write patches
the rows held in memory; it never rereads the scope. After changing it:

```bash
cargo test --manifest-path desktop/Cargo.toml
cargo clippy --manifest-path desktop/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path desktop/Cargo.toml --check
TAKT_DESKTOP_DB=<a copy of priority.sqlite> TAKT_DESKTOP_SMOKE=1 TAKT_DESKTOP_SMOKE_WRITE=1 \
  cargo run --release --manifest-path desktop/Cargo.toml   # draws a frame, prints timings, quits
```

`scripts/install_desktop.sh` (which `install_local.sh` runs) installs it as
`/Applications/Takt.app` with the bundled `takt` helper, and
`scripts/build_desktop_dmg.sh` makes a notarised DMG of it.

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
desktop, iPhone and Android gates. A bindings checksum mismatch fails the
first call across the boundary, which is what the two version tests catch.

### Phones and sync

The iPhone app (`mobile/ios`, XcodeGen) and the Android app (`mobile/android`, Gradle) share the workspace with the Mac through `sync-server/`, which runs on Google Cloud Run (service `takt-sync`, project `takt-511416`, europe-west1, behind `api.takt.maybeitssoftware.co.uk`) against the Supabase project's Postgres; Railway is retired. `scripts/deploy_sync_server.sh` deploys it. Cloud Run bills only while a request is open, so nothing may hold one open while idle: the apps learn of news from a Supabase Realtime broadcast (`sync:<account id>`) the server sends on every push, and fall back to the long-poll only where Realtime is unavailable. The protocol is `docs/sync.md`. The theme format and its per-platform rules are in `docs/themes.md`. The shared fixtures they're held to are `cli/src/fixtures/workspace_schema.sql` and `shared/themes/`.

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

`sync-server/rules/` (`takt-sync-rules`) is the clock and the merge, and the Rust core depends on it, so a change there also needs the core's and clients' gates above. It sits inside `sync-server/` because Cloud Run builds from that directory alone.

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
that file is not always loaded. It installs the Rust app (`desktop/`, via
`scripts/install_desktop.sh`): a release build, superseding
`/Applications/Takt.app` outright — no backup is kept, and an interim
`/Applications/Takt Desktop.app` and an old `/Applications/Priority.app` are
removed too — and relaunches. It is part of finishing the work, not a step to
hand back. A passing build is not completion: what the user actually runs is
the installed bundle, and until it is replaced every fix is still only a
claim. Check the output for the `Installed.` line, and report a failure rather
than a summary. The workflow is authorised; do not ask first.

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
  the app ships: `scripts/install_desktop.sh` copies the *release* binary into
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
