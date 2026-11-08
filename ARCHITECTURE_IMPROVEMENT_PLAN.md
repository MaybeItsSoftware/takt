# Architecture Improvement Plan

> **Historical, 2026-11-08.** The Swift Mac app this describes (`Takt/`) has been
> retired; the Mac app is `desktop/` (Rust, GPUI). See `docs/rust-desktop-plan.md`.

> **Superseded in part, 2026-10-13.** The Checkvist-era task stack this plan
> decomposes (`TaskRepository`, `SyncService`, `TaskMutationService`,
> `UndoService`, `NavigationState`, the offline and list-scoped stores, the
> service hosts and the `TaktAppLogic` target) has been deleted rather than
> finished: the workspace is the only task model, and Checkvist is an import
> source behind `CheckvistAccount` and the Rust core's `checkvist.rs`. The
> steps below that name those types are history. See
> `docs/state-ownership.md` and `docs/rust-core-migration.md`.


This document captures the structural problems in Priority as it stands today and the staged work needed to resolve them. It is referenced from `CLAUDE.md` and should be kept current — when a phase lands, mark it done; when scope changes, edit here rather than scattering decisions across PRs.

## Guiding Principles

- **Decompose along responsibilities, not file size.** Splitting one 800-LOC file into four 200-LOC files that all import each other is not progress.
- **Push state down, not coordination up.** `AppCoordinator` should orchestrate, not own; managers should own their state and expose intent-level APIs.
- **Plugin identity must not leak.** No `isUsingOfflineStore`-style booleans outside `Plugins/`. Switching is a plugin-layer concern.
- **One source of truth per fact.** No duplicated computed properties across `AppCoordinator` and `TaskRepository`.
- **Tests are part of the contract.** New seams introduced by this plan ship with tests; existing untested code is not refactored without a test first.

## Current State

Last audited 2026-08-18. Figures are from the working tree at that point, not
from when a phase landed — the previous version of this section still described
`AppCoordinator` as "2,205 LOC across 9 files" and cited line numbers in
`AppCoordinator+TaskScoping.swift` and `+StateAndLifecycle.swift`, files that no
longer exist. If you are reading a claim here that the code contradicts, the
code is right; fix this section.

| | Then (plan written) | Now |
|---|---|---|
| `AppCoordinator` | 2,205 LOC / 9 files | **713 LOC / 2 files** (`AppCoordinator.swift`, `+ServiceHosts.swift`; counted 2026-10-10) |
| `PopoverView.swift` | 2,068 LOC | 1,461 LOC |
| `SettingsView.swift` | 1,473 LOC | 526 LOC |
| `KanbanManager.swift` | 778 LOC | **deleted** 2026-10-10 with the board engines it alone read |
| `KeyboardShortcutRouter.swift` | 1,011 LOC | 923 LOC across 2 files; gates, sequences and guards in `TaktCore` |
| MCP implementations | 3 (Swift, Python, Rust) | **1** (the Rust CLI, bundled in the app) |
| How the app gets `TaktCore` | compiled its sources | **links the package product** |
| Test count | — | **1,441 SPM + 129 cargo** (last audited 2026-10-07) |
| SwiftLint in CI | never ran | runs (non-strict); the standing warning count lives in `TODO.md` |

Phases 0–5 are complete. What follows is the open list.

Each open finding below is either *partly addressed* (with what remains stated)
or *decided* (with the reason it has not been acted on). None is a bare "this is
bad" — if one reads that way it has gone stale, which is how this section got
into the state described above.

## Open Findings

### 1. `KeyboardShortcutRouter.handle` is a single ~950-line function

**Superseded.** The router went with the popover, and the desktop workspace
dispatches through `WorkspaceCommandCatalog` instead. The pieces below outlived
it unused and were deleted in phase 5 of `docs/zed-overhaul-plan.md`; only
`ShortcutKeyToken.nameByKeyCode` and `ShortcutReference.display(token:)`
remain, because the catalogue spells and renders keys with them. What follows
is the record of what they found.

**Mostly addressed.** Three pure pieces are out, in `TaktCore` and tested:

- `ShortcutKeyToken` — the token spelling (`"cmd+k"`, `"shift+enter"`), next to
  `ConfigurableShortcutAction.defaultBinding`, which has to agree with it.
  Writing its test found a dead binding: `rootFilter7` defaulted to `"comma"`,
  which no key press produced.
- `ShortcutGate` — the five modal gates (onboarding, the plugin-selection
  dialog, a running focus session, the focus prompt) *and their precedence*,
  which is the actual rule. 18 tests.
- `ShortcutSequenceBuffer` — the two-key state machine. One character of state
  that was read and written at nine scattered points; a buffer left standing
  swallows the next key press with no visible cause. 17 tests.
- `ShortcutResolver` — an ordered table of every binding and the conditions it
  is live under. The router no longer states its guards; it asks the table, so
  the two cannot drift.

**That table found a real bug.** `shadowedRules()` asks whether any rule is
covered everywhere by an earlier rule for the same action, and it was: the
`moveTaskUp` / `moveTaskDown` branches ran unguarded *above* the Daily view's
block, which binds the same gesture to reordering a daily. So in the Daily view
Cmd+↑/↓ silently reordered a task in the list underneath, and the Daily handler
was unreachable. Fixed by moving the general pair below the Daily block; the
test asserts no rule is shadowed.

A second audit, `deadBindings()`, looks for a binding that wins in no context at
all. There are none — and it corrected this document, which used to claim
`moveTaskUp`'s `cmd+k` alias was simply unreachable. It is not, quite:
`openCommandPalette` claims `cmd+k` first but only while nothing has keyboard
focus, so the alias survives in exactly the situation nobody would use it —
mid-typing, where it reorders the task behind the field. Both halves are
long-standing behaviour and changing either is a user-facing call, so
`testCmdKReachesTheCommandPaletteExceptWhileTyping` pins what is true instead.

**Still open:** the dispatch bodies. `handle` is under the file-length limit now
(the performing half moved to `KeyboardShortcutRouter+Support.swift`) but still
carries `// swiftlint:disable ... cyclomatic_complexity` — measured complexity
182 against a limit of 25. What remains is 60 branches each reaching a different
manager. Turning those into an `AppAction` enum is the next step, and it is now
a much safer one: the guards are already tested, so a mistake shows up as a
resolver test failure rather than a silently dead key.

### 2. Two shadow copies of the plugin model types

**Route 2 has started, and the premise behind this whole finding is gone.**

`TaktCore`'s sources moved to `Sources/TaktCore`, and the Xcode target
now *links* the package product instead of compiling the same files. That was
~46 files gaining an `import TaktCore` and ~300 declarations gaining
`public` — all compile-checked, and the target has no `@Observable` types, so
the silent "stops updating" failure mode did not apply.

The consequence is larger than the move. **`TaktAppLogic` and
`TaktPlugins` can now `import TaktCore`.** That import used to break the
app build, because the same sources are compiled straight into the app where no
such module existed — the constraint this document called "the one-file-one-target
rule" and treated as fixed. It is not fixed; it only held while the app compiled
the sources. Three things followed immediately:

- `OfflineReplayPolicy.swift` moved into `Sources/TaktCore`, ending the
  exile this document used as its example of the rule.
- The due-date parsing moved out of `CheckvistTask` into
  `TaktCore.DueDateParsing`, so the real model and the shadow now share it
  rather than the shadow re-deriving forty lines of date formats.
- `TaskListViewModel` became reachable — see finding 3.

**Still open:** `applogic-support/AppLogicSharedTypes.swift` and
`plugin-tests-support/PluginModelStubs.swift` still exist. Deleting them needs
`TaktPlugins` to be linked by the app as well, and that is blocked on three
plugin files that name app-only services (`CheckvistSession`,
`CheckvistTaskRepository`, `ObsidianSyncService`, `GoogleOAuthLoopbackReceiver`
— which is what `PluginModelStubs` fakes). Only `ObsidianSyncService` imports
AppKit; the others are Foundation-only, so most of that is movable. Until then
`SharedTypeDriftTests` keeps the two declarations honest, and the exclude lists
are shorter but not gone.

### 3. Managers with no tests

The pattern is now established five times: take the pure decision out, make it
generic or parameterised so it needs no app types, put it in `TaktCore`, and
leave the manager gathering inputs and applying results.

**Done since the audit:**

- `KanbanFilter` — membership and ordering. 23 tests. *(Deleted 2026-10-10 with
  `KanbanManager`; see the entry at the end of the history.)*
- `KanbanSelection` — the other half of `KanbanManager`: which card is selected
  and which column has focus. Works on a grid of task ids per column, so it
  knows nothing about `CheckvistTask` or how the board filtered itself. 26
  tests. It also fixed a real inefficiency — `nextKanbanTask` used to re-filter
  every column to resolve focus and then again to read the one it settled on.
- `PendingSyncQueue` and `IntegrationLinkStore` — the Obsidian sync queue and
  the calendar link map out of `IntegrationCoordinator`. 22 tests. (The MCP half
  of that coordinator turned out to be already factored into
  `CoreLogic/MCPClientConfig.swift` with tests, so it needed nothing.)
- **`TaskListViewModel`** — the one this document called the highest-value and
  hardest target. A `TaskListViewModelHost` seam in the shape of
  `TaskMutationHost`/`SyncHost` replaced the five app-only managers it named
  concretely; every member of that protocol is a read-only scalar, which is why
  it was worth doing at all. 12 tests, including the two that matter:
  `cacheVersion` is bumped on invalidation, and — the subtle one — reading a
  *clean* cache still registers an observation dependency. That second test was
  verified to fail when `_ = cacheVersion` is removed, which is the whole reason
  the line exists and is invisible by eye.

**Writing those tests found a crash.** `TimerStore.rolledUpElapsedByTaskId`
recursed down the child tree with no cycle guard, and memoised only *after*
returning — so a cycle in the parent chain never terminated. Not a wrong number:
a stack overflow, `SIGSEGV`, from data that arrives over the network. Four
ancestor walks had already been guarded during the audit; this one was missed
because it walks *children* rather than parents. Fixed, with tests for the
two-node cycle, a longer one, and a self-parented task.

**Still zero coverage:** `IntegrationCoordinator`'s AppKit-bound half (the
Obsidian and Google Calendar flows), the router's dispatch bodies, and every
view.

### 4. SwiftLint's standing warnings

The live count, and what each warning is, is kept in **`TODO.md`** ("Known-good
backlog"), the single place the number lives; the figures that used to sit here
(thirteen, then nine, then the workspace files' arrival) are now history and
the per-file list went stale every time one was fixed. What remains after the
2026-10-07 cleanup is length and complexity on the known large files only —
none of them a new kind, none suppressed.

CI runs `swiftlint lint` without `--strict`, so these are advisory and errors
block. Don't add to the list; don't suppress it either — every warning
introduced while doing the work above was fixed rather than accepted.

### 5. Day-log rotation

The read amplification is fixed — `DailyLogService.reloadFromDisk()` now stats
both files before decoding anything. What remains is that `daylog.jsonl` still
grows without bound and is held entirely in memory. That is a deliberate
trade (every projection needs the full history, since a reopen can cancel a
completion from any earlier day), and a few thousand events a year is nothing.
Revisit if someone's log reaches a size where the launch-time `loadAll` is
noticeable.

### 6. The in-process MCP server — done

**Closed.** `Takt/Plugins/MCP/MCPServer.swift` (1,760 LOC) is deleted, along
with `scripts/mcp_parity_check.py` (614 LOC), the `mcp-parity` CI job, and
`CoreLogic/MCPMessageFraming.swift` (161 LOC plus its tests), which existed only
to frame that server's stdio.

The one thing keeping it alive was never a capability the CLI lacked — the
parity check proved all nineteen tools agreed on answers, files written and HTTP
requests made. It was that MCP client configurations already on users' disks name
`/Applications/Priority.app/Contents/MacOS/Priority --mcp-server`.

So the app ships the CLI instead. `scripts/bundle_cli.sh`, from an Xcode build
phase, cargo-builds it and installs it at `Contents/Helpers/takt`, signed
with the app. `TaktEntryPoint.main()` checks for `--mcp-server` before
`MainApp.main()` and hands the process to `MCPServerShim.run()`, which `execv`s
the helper — so a process that only speaks JSON-RPC on stdio never initialises
AppKit, and there is no supervision to get wrong.

Existing configurations keep working untouched, and largely because the CLI was
already built for it: `cli/src/main.rs` accepts the bare `--mcp-server` flag,
and `cli/src/config.rs` puts the environment above its own config file — which
is exactly where a client configuration puts credentials.

Two costs, both deliberate:

- **Building the app now needs cargo.** `TAKT_SKIP_CLI_BUNDLE=1` opts out
  and produces an app with no MCP server; `scripts/build_dmg.sh` refuses to
  package a bundle missing the helper. `ENABLE_USER_SCRIPT_SANDBOXING` had to go
  to `NO`, because cargo writes outside any declarable output.
- **`CLAUDE.md`'s rule changed.** "Nothing in `Takt/` may reference it"
  became "…except the `--mcp-server` shim, which execs the bundled helper". The
  app *runs* the CLI; it still shares no source with it.

What replaced the parity harness is `scripts/mcp_smoke_check.py`, which is a
different question: `cargo test` covers the server, and the smoke check covers
the seam — that both spellings reach it and expose the same nineteen tools, and
specifically that an old-style invocation with credentials in `env` still gets a
working server. Plus `MCPHelperLocator` in `TaktCore`, 12 tests, for the
search order and its diagnostic.

## History: the phased plan

Phases 0–4 are complete; Phase 5 is partly done and partly deferred on purpose.
Kept in full because the *reasoning* behind each move is the useful part — why a
file sits where it does, and which constraint put it there. The open work is in
"Open Findings" above, not here.

Order mattered: each phase removed blockers for the next.

### Phase 0 — Safety net (prerequisite)

Before refactoring `AppCoordinator` we need a regression harness for the behaviour we're about to move.

- [x] Add integration-style tests for `TaskRepository`: load, mutate, reorder, switch list, switch online/offline. Use the existing `OfflineTaskSyncPlugin` plus a fake `CheckvistSyncPlugin` to drive both branches.
- [x] Add tests for reordering paths (`ReorderQueueTests`). Undo paths gained coverage in step 3.2 once `UndoService` was extracted behind the `UndoActionPerforming` protocol — see `applogic-tests/UndoServiceTests.swift`. `taskAction` and the rest of the coordinator-level mutation orchestration gained direct coverage in step 3.8, once `TaskMutationService` and `SyncService` moved into `TaktAppLogic` behind the host protocols — see `applogic-tests/TaskMutationServiceTests.swift` and `applogic-tests/SyncServiceTests.swift`.
- [x] Add a test for cache invalidation: mutate `tasks`, `availableLists`, `priorityTaskIdsByParentId`, assert `TaskListViewModel.cache` rebuilds (this will reveal the missing `availableLists` `didSet`).

### Phase 1 — De-duplicate plugin-switch state

Concrete, low-risk; removes the most-cited leak.

- [x] Move `isUsingOfflineStore`, `activeCredentials`, `activeSyncPlugin` to a single owner. Renamed to `canSyncRemotely` on `TaskRepository` (inverted boolean so the offline plugin no longer leaks into the public name); the duplicates in `AppCoordinator.swift` (`activeCredentials`) and `AppCoordinator+TaskScoping.swift` (`isUsingOfflineStore`) now forward to the repository.
- [x] Replace `if isUsingOfflineStore` callsites with intent-revealing methods on the repository — `canSyncRemotely`. `AppCoordinator+TaskSync.swift` and `SettingsView.swift` were updated. (Note: `hasCredentials` / `hasListSelection` / `canAttemptLogin` are still computed-property duplicates between `AppCoordinator+TaskScoping.swift` and `TaskRepository.swift`; left as-is because the plan didn't name them and Phase 3 will eat them when AppCoordinator decomposes.)
- [x] Remove the `manager.isUsingOfflineStore` reach from `NativeCheckvistSyncPlugin+Settings.swift:273`. Replaced with a direct `manager.listId.isEmpty` check, since the workspace caption only cares whether the user has picked a list.

### Phase 2 — Unify cache invalidation

Goal: one mechanism, no missed invalidations.

- [x] Replaced `onCacheRelevantChange` callbacks with a single `CacheInvalidationBus` (`Takt/CacheInvalidationBus.swift`). Producers (`TaskRepository`, `NavigationState`, `KanbanManager`, `QuickEntryManager`, `TimerManager`, `StartDateManager`, `FocusSessionManager`) take the bus at init and call `bus.invalidate()` from their `didSet`s. The lone subscriber today (`AppCoordinator`) registers once in `setupChildCallbacks` and routes into `TaskListViewModel.invalidateCaches()`. The bus's `init` is `nonisolated` so `@MainActor` managers can keep a `CacheInvalidationBus()` default value on the parameter.
- [x] Audited every `var` on `TaskRepository` and `AppCoordinator`. Added `didSet` for `availableLists` and `isNetworkReachable` (both fire the bus). `checkvistIntegrationEnabled` also fires the bus from `didSet`, so `setupChildCallbacks` no longer reaches into `invalidateCaches()` from its callback. Vars that don't drive task-visibility caches (`isLoading`, `errorMessage`, `lastUndo`, the auth-credential vars whose downstream effect already routes through `tasks`/priority-queue reload) are intentionally exempt and remain unhooked. The previously-XCTExpectFailure'd cases in `TaskRepositoryCacheInvalidationTests` now pass without the wrapper.
- [x] Removed the per-manager `onCacheRelevantChange` properties and the AppCoordinator-side `currentParentId` setter no longer calls `invalidateCaches()` (NavigationState fires the bus from its own `didSet`).

### Phase 3 — Decompose AppCoordinator

Phase 0 must be complete; without tests this is too dangerous.

Target shape:

- `AppCoordinator` shrinks to lifecycle wiring + composition (≤200 LOC).
- Behaviour moves to dedicated services consumed by views directly via `@Environment` / `@Observable`:
  - `TaskMutationService` ← `+TaskMutations`, `+QuickAdd`, `+Undo` (one undo stack, owned here). **Done (steps 3.2 + 3.4):**
    - **Undo half (step 3.2):** split out into `Takt/UndoService.swift` (owns `lastAction` and the rewind switch, replacing the misplaced `TaskRepository.lastUndo` slot). Depends on the new `UndoActionPerforming` protocol rather than `AppCoordinator` directly, which let it move into `TaktAppLogic` — `applogic-tests/UndoServiceTests.swift` covers record/clear and the rewind dispatch for every `UndoableAction` case (10 new tests). `+Undo.swift` is deleted.
    - **Mutation half (step 3.4):** new `Takt/TaskMutationService.swift` owns mark-done / reopen / invalidate / `taskAction`, `updateTask`, `addTask`, `addTaskAsChild`, `deleteTask`, `createNextOccurrence`, and the QuickAdd flow (`beginQuickAddEntry`, `setQuickAddSpecificLocationToCurrentTask`, `submitQuickAddTask`). `+TaskMutations.swift` and `+QuickAdd.swift` are forwarding shims; the recurrence convenience accessors (`recurrenceRule(for:)`, `setRecurrenceRule`, `clearRecurrenceRule`) stay in `+TaskMutations.swift` since they're already one-liners over `recurrence`.
    - **Promoted to AppLogic (step 3.8).** Both `TaskMutationService` and `SyncService` now depend on `TaskMutationHost` / `SyncHost` (`Takt/TaskServiceHosts.swift`) instead of holding a `weak var coordinator: AppCoordinator?`. The UI-bound behaviour they used to inline — the `NSHapticFeedbackManager` + `withAnimation` completion sequence, the kanban column maths, the recurrence rule store, the `TimerElapsedReassignmentPolicy` remap — is expressed as behaviour the host performs, so it lives in `AppCoordinator+ServiceHosts.swift` (the only app-only half of the split) while the services compile into `TaktAppLogic`. `applogic-tests/TaskMutationServiceTests.swift` (20 tests) and `applogic-tests/SyncServiceTests.swift` (14 tests) drive them against `StubTaskServiceHost`, covering `taskAction` rollback, the optimistic add/delete paths, offline queueing, the recurrence hand-off, and offline replay. Writing them surfaced a real defect: `TaskRepository.init`'s `pendingOfflineWorkStore` default ignored the injected `defaults`, so the offline queue always went to `UserDefaults.standard`.
      - `OfflineReplayPolicy.swift` lives at `Sources/TaktCore/OfflineReplayPolicy.swift`. It spent a while at the app root because `TaktAppLogic` could not `import TaktCore` then (its sources were also compiled straight into the Xcode app, where `TaktCore` wasn't a module); once the app started *linking* the package product instead, the import resolved on both sides and the file moved back into `TaktCore`, where `corelogic-tests` covers it.
    - AppCoordinator still exposes the original method names as forwarding shims so existing keybindings, `CommandExecutor`, and view call sites keep working — those move in the forwarding cull.
  - `TaskNavigationService` ← `+Navigation`, `+TaskScoping`. **Partly done (step 3.3):** new `Takt/TaskNavigationService.swift` owns the navigation actions (next/prev, enter/exit, navigate-to, clamp) and the four root-task view-switch operations (`setRootTaskView`, `cycleRootTaskView`, `cycleRootScopeFilter`, `selectRootScopeFilter`), wrapping the pure-logic `TaskNavigationCoordinator` struct. `+Navigation.swift` is now a forwarding shim; the four moved methods in `+TaskScoping.swift` are forwarders too. `TaskRepository.navigationCoordinator` is gone — the service holds its own logic instance. **Not yet moved out of `+TaskScoping.swift`:** the connection-state derivations (`hasCredentials`, `canAttemptLogin`, `checkvistConnectionState`, `canSyncRemotely`), priority-on-current-task mutations (`setPriorityForCurrentTask` etc.), plugin/MCP view-helpers, and the cache/badge accessors. Those belong to later steps (`SyncService`, `TaskMutationService`) or to view-side cleanup.
  - `SyncService` ← `+TaskSync`, `+ReorderingAndTiming` reorder-flush logic. **Done (step 3.5):** new `Takt/SyncService.swift` owns the network-facing surface — login, `fetchTopTask`, list management (`fetchLists`, `loadCheckvistLists`, `switchCheckvistList`, `createCheckvistListAndSwitch`, `mergeOpenTasksBetweenLists`, `selectList`, `uploadOfflineTasksToCheckvist`), the offline-mutation flush, and the reorder/move surface (`moveTask` + per-view strategies, the reorder queue lifecycle, `indentTask`, `unindentTask`). `+TaskSync.swift` is now a 45-line forwarding shim. `+ReorderingAndTiming.swift` is a mixed file: forwarders for the reorder/indent surface, plus the helpers that *didn't* move because other services still call them through the coordinator (`subtreeBlockRange`, the timer/cache roll-up accessors, `executeCommandInput`, the date-resolver helpers). Renaming or splitting that residual file belongs to the post-cull cleanup.
  - `LifecycleController` ← `+StateAndLifecycle` setup/teardown. **Done (step 3.1):** new `Takt/LifecycleController.swift` owns the cache-bus subscription, repository/manager-callback wiring, and the network-monitor lifecycle. AppCoordinator constructs it in `init` and calls `lifecycle.start()`; `reachabilityMonitor` stays on AppCoordinator so the nonisolated `deinit` can stop it without an actor hop. The four other concerns previously crammed into `+StateAndLifecycle.swift` (priority-queue forwarding, loading helpers, keychain bootstrap, onboarding dialogs) remain in that extension for now — they belong to later services in this phase.
- ~~Forwarding properties on `AppCoordinator` are deleted; views read from the relevant service directly.~~ **Method forwarders culled (step 3.6); property cull complete (step 3.7).** Final batch landed: `listId`, `errorMessage`, `username`, `remoteKey`, `availableLists`, `isLoading`, `taskEisenhowerLevels`, `activeCredentials`, and the unused `usernameLower` — all `TaskRepository`-owned, all deleted. Views go through `@Environment(TaskRepository.self) var repository`; non-view callers (`CommandExecutor`, `KeyboardShortcutRouter`, `MenuBarController`, `LifecycleController`, the `AppCoordinator+*` extensions) read via `manager.repository.X` / `coordinator.repository.X` / `repository.X`. `SyncService` and `TaskMutationService` were also narrowed in the same pass — they now take a strong `TaskRepository` field in their initializer (matching `TaskNavigationService`'s shape) instead of routing every forwarder hop through the weak coordinator reference. Two SwiftUI bindings in `NativeCheckvistSyncPlugin+Settings.swift` (the username and remote-key fields) needed explicit `Binding(get:set:)` wrappers because `repository` is a `let` on `AppCoordinator` — the keypath-derived `$manager.repository.username` projection isn't writable.

### Phase 3 outcome (step 3.7 — property forwarder cull, started)

The incremental migration path is now proven end-to-end:
- **Three `NavigationState`-owned forwarders deleted: `rootScopeFocusLevel`, `currentSiblingIndex`, `currentParentId`.** `NavigationState` is injected into the popover environment (`MenuBarController`); every view that reads them (`PopoverView` + its `+TaskRow`/`+QuickEntryBar` extensions, `KanbanBoardView`'s `KanbanColumnView`/`KanbanTaskCard`, `EisenhowerMatrixView`) now takes `@Environment(NavigationState.self)` and reads directly. Non-view readers (`CommandExecutor`, `KeyboardShortcutRouter`, `MenuBarController`) go through `manager.navigationState`; services (`SyncService`, `TaskMutationService`) through `coordinator.navigationState`; the `AppCoordinator+*` extensions through their own `navigationState`. One subtlety to keep in mind for the rest of the cull: a few non-view sites masquerade as views — e.g. `PopoverLayout.preferredHeight(for manager:)` is a static helper that only has the `manager` parameter, so it must use `manager.navigationState`, not a bare `navigationState`.
- **Both data-source protocols decoupled from `AppCoordinator` (step 3.7a — the unblocker).** `AppCoordinator` no longer conforms to `KanbanTaskDataSource` *or* `IntegrationDataSource`; dedicated adapters (`KanbanTaskDataSourceAdapter`, `IntegrationDataSourceAdapter`, both held strongly by `AppCoordinator` because the `dataSource` slots are `weak`) implement the protocols against `repository`/`navigationState`/`taskListViewModel` directly. `KanbanManager`/`IntegrationCoordinator` bodies are unchanged. This is what let `currentSiblingIndex`/`currentParentId`/`rootTaskView`/`hideFuture`/`cache`/`tasks` (all protocol members across the two protocols) be deleted. Note: `IntegrationDataSourceAdapter` still holds a `weak` `AppCoordinator` for `currentTask` alone — that property is genuinely coordinator-derived (dispatches on `rootTaskView` + kanban selection), and is the thing to move down onto TLVM/a navigation service next; doing so also unblocks deleting the `listId`/`activeCredentials` forwarders cleanly.
- **View-shaping persistence moved into `TaskListViewModel` (step 3.7b — the gate-clearer).** `rootTaskView`/`selectedRootDueBucketRawValue`/`selectedRootTag` now persist from TLVM's own `didSet`s (TLVM took a `PreferencesStore` dep and loads their initial values in `init`, where `didSet` doesn't fire).
- **`TaskListViewModel`-owned forwarders deleted (step 3.7c).** With persistence moved and the kanban adapter in place, `rootTaskView` / `selectedRootDueBucketRawValue` / `selectedRootTag` are gone from `AppCoordinator`; the `selectedRootDueBucket` typed wrapper moved onto TLVM too. `TaskListViewModel` is injected into the popover env alongside `NavigationState`; `PopoverView` and `KanbanTaskCard` read via `@Environment(TaskListViewModel.self)`. The `AppCoordinator+TaskScoping` filtering helpers (`taskMatchesActiveRootScope`, `shouldShowDueSectionHeaders`, `rootScopeShowsFilterControls`, `remainderSectionHeader`, `currentTask`) now read these off `taskListViewModel` directly — note this is the same logic TLVM already implements internally, so consolidating those duplicated helpers onto TLVM is a natural follow-up.
- **Cache-derived view helpers consolidated onto `TaskListViewModel` (step 3.7d).** The badge / section-header / tag / priority-rank helpers that previously lived as duplicates on `AppCoordinator+TaskScoping` moved to TLVM, where the `cache` they read already lives: `priorityRank` / `absolutePriorityRank` / `priorityPath` / `priorityBadgeLabel` / `eisenhowerBadgeLabel` (+ `formatEisenhowerCoordinate`), `remainderStartIndex`, `rootDueSectionHeader` / `remainderSectionHeader` / `rootDueSectionCount` (each self-contained via private `isRootLevel` / `shouldShowRootScopeSection` / `shouldShowDueSectionHeaders` on TLVM), `rootLevelTagNames`, and `isDescendant`. The dead-duplicate privates on the coordinator (`taskMatchesActiveRootScope`, `hasAnyTag`, `hasTag`, `rootDueBucket` — TLVM already implemented its own) were deleted. Views read via `@Environment(TaskListViewModel.self)` (`FocusSessionOverlay` gained the env declaration — it renders under the popover environment, so TLVM is injected); non-view callers go through `manager.taskListViewModel.X` / `coordinator.taskListViewModel.X` (the router, `PopoverLayout.preferredHeight`, `TaskNavigationService`, `TaskMutationService`, `subtreeBlockRange`).
- **`TaskNavigationService` narrowed (step 3.3 follow-up).** Now constructed with concrete `repository` + `navigationState` dependencies instead of reaching every forwarder through `coordinator`; the pure-forwarder hops read the concrete deps and two methods dropped their coordinator reference entirely. This is the template for narrowing `SyncService`/`TaskMutationService` next.
- **Optimistic-mutation failure handling consolidated.** `TaskMutationService.resolveMutationFailure(whenOffline:whenOnline:)` now owns the single offline-vs-online reachability branch that was copy-pasted across six mutation paths (`taskAction`, `updateTask`, `addTask`, `addTaskAsChild`, `submitQuickAddTask`, `deleteTask`). The `applyOptimisticMoveAndSync` / `addTaskInKanbanColumn` paths on `AppCoordinator` itself still hand-roll the dance (and `applyOptimisticMoveAndSync` writes `pendingTaskMutations` directly rather than through `enqueuePendingMutation`, so it skips the disk write-through — latent inconsistency worth fixing when those move into the service).

See `docs/state-ownership.md` for the current owner-of-record map used to navigate this.
- ~~Remove the `AppCoordinator+*.swift` extension pattern entirely~~ — **Partially done.** Five extension files are gone (`+Navigation`, `+TaskSync`, `+TaskMutations`, `+QuickAdd`, `+Undo`). Three remain, but each has been further trimmed in the step 3.7 cleanup pass (76 / 157 / 303 LOC):
  - `+StateAndLifecycle` — the trivial repository forwarders (`savePriorityQueue`, `removeTasksFromPriorityQueue`, `reconcilePriorityQueueWithOpenTasks`, `beginLoading`, `endLoading`, `withLoadingState`, `setAuthenticationRequiredErrorIfNeeded`, `runBooleanMutation`) were deleted; the services (`SyncService`, `TaskMutationService`) call `repository.X` directly. What's left is keychain bootstrap, onboarding-dialog management, and `reconcilePendingObsidianSyncQueueWithOpenTasks` (a genuine cross-manager helper).
  - `+TaskScoping` — the priority-on-current-task mutations (`setPriorityForCurrentTask`, `setAbsolutePriorityForCurrentTask`, `sendCurrentTaskToPriorityBack`, `clearPriorityForCurrentTask`, `clearAbsolutePriorityForCurrentTask`) moved into `TaskMutationService`; callers go through `manager.taskMutationService.X`. The `invalidateCaches` / `ensureVisibleTasksCacheValid` forwarders were deleted (callers go through `taskListViewModel.X`). The cache-derived view helpers were consolidated onto `TaskListViewModel` in step 3.7d (see below). What's left is connection-state derivations, view-derived properties (`currentTask`, `visibleTasks`, `breadcrumbs`, `currentTaskChildren`, `currentLevelTasks`), the root-scope section gates that still feed `currentTask`/the router (`isRootLevel`, `shouldShowRootScopeSection`, `rootScopeShowsFilterControls`, `isSearchFilterActive`), and `shouldShowBreadcrumbPath` (kept on the coordinator because it reads `preferences.showTaskBreadcrumbContext`, a `PreferencesManager` dep TLVM doesn't hold).
  - `+ReorderingAndTiming` — `subtreeBlockRange`, command/date helpers, and the timer/cache roll-up accessors. The roll-up accessors (`totalElapsed`, `childCountByTaskId`, `rolledUpElapsedByTaskId`) deliberately touch `timer.timerByTaskId` to re-subscribe SwiftUI on tick boundaries; that's a deliberate observation hook, not a forwarder.

### Phase 3 outcome (step 3.6 — method forwarder cull)

After Layers 1–4 of the cull every method forwarder pointing at a new service is gone:
- `manager.lastUndo` / `undoLastAction()` → `manager.undoService.lastAction` / `.undo()`
- `manager.nextTask()` / `previousTask()` / `enterChildren()` / `exitToParent()` / `navigateTo(task:)` / `clampSelectionToVisibleRange()` → `manager.taskNavigationService.X` (with `navigateTo(task:)` → `navigate(to:)`)
- `manager.setRootTaskView(_:)` / `cycleRootTaskView(direction:)` / `cycleRootScopeFilter(direction:)` / `selectRootScopeFilter(at:)` → `manager.taskNavigationService.X`
- `manager.markCurrentTaskDone()` / `reopenCurrentTask()` / `invalidateCurrentTask()` / `taskAction(_:endpoint:isUndo:)` / `updateTask(...)` / `addTask(...)` / `addTaskAsChild(...)` / `deleteTask(_:isUndo:)` / `createNextOccurrence(for:)` / `beginQuickAddEntry(...)` / `setQuickAddSpecificLocationToCurrentTask()` / `submitQuickAddTask(...)` → `manager.taskMutationService.X`
- `manager.login()` / `fetchTopTask()` / `fetchLists()` / `loadCheckvistLists(...)` / `switchCheckvistList(to:)` / `createCheckvistListAndSwitch(name:)` / `mergeOpenTasksBetweenLists(...)` / `selectList(_:)` / `uploadOfflineTasksToCheckvist(...)` / `flushPendingTaskMutations()` → `manager.syncService.X`
- `manager.moveTask(_:direction:)` / `movePriorityTask(_:direction:)` / `indentTask(_:)` / `unindentTask(_:)` → `manager.syncService.X` (the kanban `moveTask(id:toColumn:)` overload stays on AppCoordinator — it's a real method, not a forwarder)

Wiring side-effect: `UndoActionPerforming` conformance moved from `AppCoordinator` to `TaskMutationService` (which actually owns those methods now). `AppCoordinator.init` constructs `taskMutationService` first, then `UndoService(performer: self.taskMutationService)`.

Internal callers in services also retargeted: `TaskMutationService` calls `coordinator.taskNavigationService.clampSelectionToVisibleRange()` and `coordinator.syncService.fetchTopTask()` directly; `SyncService` calls `coordinator.taskMutationService.updateTask(...)`; `LifecycleController` calls `coordinator.syncService.flushPendingTaskMutations()`; `AppDelegate` calls `checkvistManager.syncService.fetchTopTask()` and `checkvistManager.taskMutationService.beginQuickAddEntry()`.

### Phase 4 — Split oversized views

- [x] `PopoverView.swift` (2,068 LOC) → **done.** Split into the main shell (1,273 LOC) plus two extension files: `PopoverView+TaskRow.swift` (451 LOC — `taskRow`, `dueSectionHeader`, and every badge: timer/priority/matrix/start/recurrence/due/metadata-token, plus `formatTaskContent` and the inline-tag formatter) and `PopoverView+QuickEntryBar.swift` (359 LOC — `quickEntryBar`, the icon/placeholder/font/sequence-hint helpers, and the submit/escape/tab/empty-list-composer actions). `MarqueeTextLine` and several helpers (`themeColor`, `isAddMode`, `activePromptText`, `activePromptTextBinding`, `clearPrompt`, `shouldShowEmptyListComposer`) dropped `private` so the extensions can see them. `breadcrumbPath(for:includeCurrentParent:)` stays in `PopoverView.swift` because both extensions need it. The full plan-vision split (`PopoverHeader`, dedicated `TaskListPane`, separate overlay file) wasn't necessary to clear the lint error limit — those further splits are available if `PopoverView.swift` grows again.
- [x] `SettingsView.swift` (1,473 LOC) → **done.** Main file is now 517 LOC; the four panes live as extensions in their own files: `SettingsView+DebugPane.swift` (19 LOC), `SettingsView+ThemePane.swift` (189 LOC), `SettingsView+KeybindingsPane.swift` (457 LOC), `SettingsView+PreferencesPane.swift` (316 LOC). The pane bodies stay as `var <pane>Pane: some View` on `SettingsView` so `selectedPaneContent` dispatches normally — `@State` properties have to live on the original struct, so they were upgraded from `private` to internal. The plugins pane stays in the main file (it already enumerates plugins generically via `pluginCards` / `userPluginCards`).
- [x] **Plugin status-label convention fix** (called out in `docs/plugins.md`): `SettingsView.pluginStatusLabel(for:)` had a switch on plugin identifier (`"native.checkvist.sync"` → `checkvistManager.checkvistIntegrationEnabled`, etc.). Replaced with a new `sidebarStatusLabel(manager:)` requirement on `PluginSettingsPageProviding` (default = `"Built-in plugin"`); each of the four native plugins overrides it in its `+Settings.swift` to report Enabled/Disabled against its own toggle state. `SettingsView` now calls `page.plugin.sidebarStatusLabel(manager: checkvistManager)` with no plugin-aware branching.
- [x] `KanbanManager.swift` (was 794 LOC) → separate column-state ownership from filter/sort logic. **Done in the 2026-08-18 audit**, exactly as predicted here: `subtreeTasks`, `columnForTask`, `taskMatchesCondition` and `sortedForKanban` moved into a `KanbanFilter` namespace in `TaktCore` that takes its inputs as parameters instead of reaching through `dataSource`, generic over `VisibilityTask` so it needs no app types. `KanbanColumn.swift` moved into `CoreLogic/` alongside it. `KanbanManager` is 702 LOC and now owns board *state*; the rules are covered by 23 tests. The prompt was testability rather than lint pressure — the note below was right that the file length was never the point.

### Phase 5 — MCP isolation (optional, lower priority)

- [x] **Done, differently.** The plan here was to extract `MCPServer` into a separate executable target and have the app invoke it. What happened instead is that the server was *deleted*: the Rust CLI already implemented every tool identically, so the app bundles that binary at `Contents/Helpers/takt` and `--mcp-server` `execv`s it. The concern recorded below — that extraction would either strand installed client configurations or add a child-process indirection for little benefit — was the right concern and is what the shim answers: configurations keep working, and there is no supervision because the process is replaced rather than spawned. See finding 6.

- [x] **Done (step 5.2):** Promoted the canonical Checkvist data types (`CheckvistNote`, `CheckvistTask`, `CheckvistList`, plus the freshly-extracted `CheckvistTaskCachePayload`, `CheckvistSessionError`, `ObsidianOpenMode`) into `TaktPlugins` sources by un-excluding `CheckvistModels.swift` and splitting `CheckvistTaskCachePayload` / `CheckvistSessionError` / `ObsidianOpenMode` into focused files under their plugin folders. Removed the six duplicate type definitions from `plugin-tests-support/PluginModelStubs.swift`; what remains there is the four genuine app-service fakes (`ObsidianSyncService`, `CheckvistSession`, `CheckvistTaskRepository`, `GoogleOAuthLoopbackReceiver`) the plugin code reaches into directly — the file header now describes that accurately.

  Also marked `CheckvistModels.swift`'s two static date-formatter arrays `nonisolated(unsafe)` (they were fine in the Xcode build but tripped SPM's strict concurrency check once the file landed in the plugin target).

  **Not done (separate concern):**
  - `applogic-support/AppLogicSharedTypes.swift` still re-declares the Checkvist types for `TaktAppLogic`. Sharing them via `import TaktPlugins` would require turning every imported type (`Plugin`, `CheckvistSyncPlugin`, `CheckvistTask` and friends) `public` — that meaningfully broadens the plugin library's API surface and is left as follow-on work.
  - The four service fakes in `PluginModelStubs.swift` still exist because the plugin code references the concrete service types (`ObsidianSyncService`, `CheckvistSession`, etc.) directly. Eliminating those requires introducing protocol seams for those services first — a structural refactor outside Phase 5.2's documented scope.

### Post-Phase-4 audit (2026-08-18)

An external audit of the whole tree. What it changed:

- **Two data-loss bugs in the offline queue.** `applyOptimisticMoveAndSync` on
  `AppCoordinator` wrote `repository.pendingTaskMutations` directly, bypassing
  the write-through enqueue, so an offline task-move edit never reached disk.
  And `SyncService.flushPendingTaskMutations` called `clearPendingOfflineWork()`
  — which wipes the persisted payload — *before* issuing its first request, so
  quitting mid-flush discarded every item that had not yet failed. The replay
  now removes each item only once the server confirms it, re-files work behind a
  succeeded create under the real id, and refuses to run twice concurrently.
- **The derived cache is now observable.** `TaskListViewModel.cacheStorage` is
  `@ObservationIgnored`, so a view reading the cache while it happened to be
  clean registered no SwiftUI dependency and never updated again — and whether
  that happened depended on whether a non-view reader (`KanbanManager`) had
  cleared the dirty flag first. A hand-maintained list of `_ = repository.x`
  touches in `visibleTasks` covered one of the fifteen accessors and had already
  fallen behind its inputs (the priority queues drive row ordering and were
  missing). Replaced by an observable `cacheVersion` read inside
  `ensureVisibleTasksCacheValid()`, before its early return.
- **The visibility layer moved into `TaktCore`.** `TaskVisibilityEngine` and
  `TaskFilterEngine` are pure `import Foundation` and decide what the user sees,
  but belonged to no SPM target and so had no tests. They are now generic over a
  new `VisibilityTask` protocol — stating the six properties the algorithms
  actually read — with `CheckvistTask` conforming in a one-line app-side
  extension. 36 new tests. Writing them found an unguarded ancestor walk that
  spins forever on a parent cycle; four such walks were found and fixed
  (`TaskFilterEngine.isDescendant`, `TaskVisibilityEngine`'s priority fold,
  `TaskListViewModel.breadcrumbs` and `computePriorityPaths`,
  `PopoverView.breadcrumbPath`).
- **The kanban mutations left `AppCoordinator`.** `applyOptimisticUpdate` and
  `addRootTask` now live on `TaskMutationService` (in
  `TaskMutationService+Board.swift`) and go through `resolveMutationFailure`
  like every other mutation. This is why the bugs above existed: they were the
  last two paths hand-rolling the dance. The add path had *no* offline branch at
  all — a card added while disconnected was discarded. `AppCoordinator` keeps
  only the adapter that turns a `KanbanColumn` into content/due.
- **The Python MCP server is gone.** `scripts/priority_mcp_server.py` (1,897
  LOC) existed as a fallback for when the app binary could not be resolved,
  which the Rust CLI already covers. It made every tool change a three-way edit.
  `scripts/mcp_parity_check.py` then drove the two remaining implementations
  and *failed* on a missing build of either, rather than skipping — with only
  two left, a skip would have left it comparing Swift to itself. It went when
  the Swift server did: with one implementation there is nothing to hold
  equal, and `scripts/mcp_smoke_check.py` checks the handover seam instead.
- **SwiftLint runs in CI.** It never had, so the file-length *error* limit this
  document calls "the constraint" was never enforced. Non-strict, so errors
  block and the 13 standing warnings don't. See Open Finding 4.
- **The keybinding token format is tested.** `ShortcutKeyToken` moved into
  `TaktCore` next to `ConfigurableShortcutAction`, whose defaults have to
  agree with it. That found `rootFilter7`'s dead `"comma"` binding. See Open
  Finding 1.
- **The shadow types are pinned.** `applogic-tests/SharedTypeDriftTests.swift`
  compares the two declarations of `CheckvistTask` / `CheckvistList` /
  `CheckvistNote` / `CheckvistTaskAction` directly. It immediately found that
  `CheckvistNote` had already drifted — the shadow was missing `created_at` and
  `updated_at`. See Open Finding 2.
- **Four unbounded ancestor walks.** A parent cycle — impossible in server data,
  reachable from a corrupt cache — would spin forever on the main actor during a
  cache rebuild, freezing the app rather than showing wrong rows. All four now
  carry a visited set.
- **Day-log read amplification.** `DailyLogService.reloadFromDisk()` decoded the
  whole append-only log and compared the entire array on every directory event,
  including this process's own writes — so the cost of recording an event grew
  with the length of the user's history. Two `stat` calls short-circuit it now.
- **`KanbanFilter` extracted.** The board's membership and ordering rules came
  out of `KanbanManager` into `TaktCore`, closing the Phase 4 item that had
  been deferred for want of lint pressure. 23 tests on card placement and the
  five-way sort tie-break, which had none. See Open Finding 3.
- **Finding 6 re-examined rather than re-deferred.** The reason given for
  keeping the in-process MCP server — that only it can reach the app's
  `UserDefaults` — is not true of the Rust CLI, which reads the same plist by
  bundle id. The only remaining reason is installed client configs. See Open
  Finding 6 for the migration that would follow from deciding that.
- **`hasPendingOfflineWork` can drive UI.** It was computed entirely from
  `@ObservationIgnored` storage, so it could never have. A revision counter
  bumped by the two write-through points closes that without making the hot
  queues observable.
- Smaller: `AppCoordinator.statusMessage`'s auto-clear is generation-guarded so
  overlapping messages stop cutting each other short; `OptimisticTaskID` reads
  its cursor from the store it was passed rather than always
  `UserDefaults.standard`.

### Findings sweep (2026-08-18, second pass)

Closing the audit's open list. In order:

- **`KanbanSelection`** — the second half of `KanbanManager` into `TaktCore`,
  working on a grid of task ids so it needs no app types. 26 tests. Also removed
  a per-column re-filter that ran on every arrow key.
- **`PendingSyncQueue` / `IntegrationLinkStore`** — the pure rules out of
  `IntegrationCoordinator`. 22 tests. The MCP half needed nothing; it was
  already factored.
- **`ShortcutGate`, `ShortcutSequenceBuffer`, `ShortcutResolver`** — the
  router's modal gates, its two-key state machine, and an ordered table of every
  binding's availability that the router now consults instead of restating.
  **Found a live bug**: the Daily view's Cmd+↑/↓ reorder was unreachable behind
  an unguarded general branch. Also corrected this document's claim about the
  `cmd+k` alias, which is reachable — just only while typing.
- **The in-process MCP server deleted** (finding 6), and the app now ships the
  Rust CLI as a signed helper. ~2,500 lines of Swift and Python gone, plus a CI
  job.
- **`Sources/TaktCore`, linked rather than compiled** (finding 2). The move
  itself was mechanical; what it changed is that `TaktAppLogic` and
  `TaktPlugins` can now import `TaktCore`, which this document had
  recorded as impossible. `OfflineReplayPolicy` came home and the due-date
  parsing is shared rather than duplicated into the shadow type.
- **`TaskListViewModel` reachable at last** (finding 3) via a
  `TaskListViewModelHost` seam. 12 tests, including one that fails if the
  `_ = cacheVersion` line is removed — the observation fix that is invisible by
  eye.
- **A crash found by those tests.** `TimerStore.rolledUpElapsedByTaskId`
  recursed into a parent-chain cycle forever: `SIGSEGV`, from data that arrives
  over the network. The audit had guarded four ancestor walks; this one walks
  *children*, so it was missed.

Tests 455 → **570**. SwiftLint warnings 13 → **9**, none suppressed.

### Since then, 2026-09-16

The desktop workspace grew past those numbers and this document did not
follow. Recorded here rather than silently left stale:

- Tests 570 → **983**. The new ones are `workspace-tests/`: the store, the
  Inbox's system role, FTS search, the undo journal, and the migration ladder.
- SwiftLint warnings 9 → **19**, none suppressed and none of them new kinds.
  All ten are `file_length` / `type_body_length` on the workspace files, which
  arrived with it: `WorkspaceDesktopView.swift` (1,396 counted lines),
  `WorkspaceViewModel.swift` (1,011), `WorkspaceStore.swift`, and one
  `redundant_discardable_let` in `WorkspaceDailiesView.swift`. The split-by-
  extension pattern the rest of the app uses (`+Sidebar`, `+Focus`, `+Search`,
  `+History`) is the obvious remedy and has not been applied to the two views.
- `WorkspaceStore` is now split as `+Dailies`, `+Import`, `+Search`, `+Undo`.
- 2026-09-28: `WorkspaceViewModel.swift` is split the same way (`+Import`,
  `+Loading`, `+Selection`, `+KeyboardFocus`, `+ListManagement`,
  `+TaskMutations`), leaving ~690 lines of stored state, init and `load()`.
  Its `file_length` and `type_body_length` warnings are gone: 24 → **22**.
- 2026-10-07: a lint pass cleared everything that was not a tracked length or
  complexity warning. The counts above are history from here on; the live
  number is in `TODO.md` and nowhere else.
- 2026-10-10, roadmap Phase 6 step 4: the legacy-only managers went.
  `CommandExecutor` (no caller since the popover), `TaskNavigationService` and
  `TaskNavigationCoordinator` (reached only from it), `KanbanManager` and
  `KanbanTaskDataSourceAdapter`, `PopoverChromeManager` (its diagnostics flag
  moved onto `AppCoordinator`), and the `TaktCore` engines only they read:
  `KanbanColumn`, `KanbanSwimlanes`, `KanbanFilter`, `KanbanSelection`,
  `KanbanManualOrder`, `EisenhowerInheritance`, `MatrixSpread`,
  `MatrixClustering`. `TaskListViewModel` was trimmed to what the services and
  integrations read. Still here, because something reachable reads them:
  `TaskListViewModel`, `CacheState`, `CacheInvalidationBus`,
  `TaskVisibilityEngine` and `TaskFilterEngine` (the Checkvist cursor that
  `SyncService` and `TaskMutationService` keep, the Google Calendar settings
  page's "Create event from selected task", the daily log's plan), and the
  services' current-task mutations, whose only caller was the executor but
  which their tests still pin.
- 2026-10-10, later: the legacy task list view model went too, once its last
  readers were moved. The Google Calendar settings page's "Create event from
  selected task" adds the desktop window's selected task through
  `WorkspaceViewModel.addSelectedTaskToGoogleCalendar` (the dates an event
  takes are `TaskEditorSnapshot.calendarEventTiming`, tested); the daily
  log's plan is `TaktCore.DayLogPlan` over the Checkvist tasks, tested with a
  fixed clock; and the Checkvist cursor is answered in
  `AppCoordinator+ServiceHosts.swift` from `NavigationState` and
  `repository.tasks` (`TaskFilterEngine.cursorLevel` / `cursorTask` /
  `subtreeBlockRange`). Deleted: `TaskListViewModel` and its host protocol,
  `CacheState`, `CacheInvalidationBus` with every producer's `invalidate()`,
  `TaskOutlineBuilder`, `TaskVisibilityEngine`, `TaskScopeResolver`, the sort
  and tag halves of `TaskFilterEngine`, `NavigationState.matrixSelectedTaskId`
  and `QuickEntryManager.isSearchFilterActive`, with their tests. Finding 3's
  observation fix (`cacheVersion`) went with the cache it protected. The
  `TaskVisibilityEngine` complexity warning went with it; the live count is in
  `TODO.md`. The services' current-task mutations and `SyncService.moveTask`
  are still there, still uncalled, still pinned by their tests.

## Out of Scope

- Replacing `@Observable` / SwiftUI patterns. The concurrency model (`@MainActor` everywhere, async/await for I/O) is sound; don't churn it.
- `FocusCore/` package — sibling, unrelated, leave alone.
- SwiftLint config relaxation. The 1,500-LOC error limit is the *constraint*, not the problem; raising it would mask Phase 4.

## How to use this document

- When opening a PR that lands part of a phase, check the box and link the PR.
- When a finding turns out to be wrong or stale, edit the Findings section — don't leave drift between this doc and the code.
- New architectural problems get appended to Findings with a file:line citation; only promote to a phase once the fix is scoped.
