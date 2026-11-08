# State Ownership Map

> Historical: this describes the Swift Mac app under `Takt/`, retired on
> 2026-11-08. The Mac app is now `desktop/` (Rust, GPUI).

> Navigation reference for "where does this piece of state actually live, who derives
> from it, and what happens when it changes." Written to cut through the
> `AppCoordinator` forwarding layer — many properties you reach through
> `AppCoordinator` are getters/setters into one of the objects below.
>
> Keep this current when state moves between owners.

## The data-flow spine (read this first)

There is one task model: the desktop workspace, `WorkspaceViewModel` over
`WorkspaceStore` and the Rust core. It refreshes itself; see the last section.

The Checkvist-era task stack (`TaskRepository`, `NavigationState`,
`SyncService`, `TaskMutationService`, `UndoService`, the offline store, the
list-scoped priority and Eisenhower stores, the timer, quick entry, focus,
start date and recurrence managers that fed it, and the `TaktAppLogic` target
that held them) was deleted on 2026-10-13. Nothing on screen had shown its
tasks since the popover went; what it still did was load Checkvist's lists for
the import and serve the Checkvist settings page, which `CheckvistAccount`
does now.

## Owners

### `CheckvistAccount` — Checkvist as an import source
`Takt/CheckvistAccount.swift`, held by `AppCoordinator` as `checkvist`.

| State | Notes |
|---|---|
| `isEnabled` | prefs key `checkvistIntegrationEnabled` |
| `username` / `remoteKey` | the username persists to prefs; the key to the keychain through the plugin. A new name, or a cleared key, clears the `takt` command's seeded copy (`onCredentialsChanged`) |
| `defaultListId` | prefs key `checkvistListId`: the list the `takt` command's Checkvist tools default to, seeded into its config by the MCP setup |
| `availableLists`, `isLoading`, `errorMessage`, `lastSuccessfulLoadAt` | in memory; every failure is recorded in `DiagnosticsLog` through `onErrorMessageSet` |
| `excludedImportListIDs` | lists the user has asked not to copy, stored as exclusions so a new Checkvist list is copied by default |

The requests and the reading of the answers are the Rust core's
(`core/src/checkvist.rs`); `NativeCheckvistSyncPlugin` sends them through
`URLSession` and keeps the token and the one retry after a 401. `AppDelegate`
runs the import on activation (throttled) and from the settings page:
`CheckvistAccount.importSnapshots` fetches each list the workspace lacks, and
`WorkspaceViewModel.importCheckvistLists` writes them through the core's
`checkvist_import_seeds` and `import_tasks`. Which lists are already copied is
the workspace's mapping (`WorkspaceViewModel.checkvistWorkspaceListIDsKey`).

### Feature managers (each owns its slice + persists to `PreferencesStore`)
| Manager | Owns | Notes |
|---|---|---|
| `PreferencesManager` | prefs-backed settings: appearance mode, hotkeys, the Quick Add list, focus behaviour, `confirmBeforeDelete`, `launchAtLogin`, etc. The theme pick and font choices are `ThemeManager`'s | |
| `IntegrationCoordinator` | `googleCalendar/googleTasks/mcpIntegrationEnabled`, `mcpServerCommandPath`, the detected MCP clients | reads the Checkvist account and default list through `dataSource` for the MCP setup; the Google Calendar page's "Create event from selected task" goes to the workspace's selection through `addSelectedWorkspaceTaskToGoogleCalendar` |
| `DailyLogManager` | the daily log plugin's settings, its dailies and its notes | nothing records task events into its log any more: they were keyed by Checkvist id and written only by the deleted Checkvist mutation path |

### `AppCoordinator` — genuinely owns
`statusMessage` (auto-clears after 3s), `showsDiagnostics` (the main window's
diagnostics sheet), `isApplyingLaunchAtLoginChange`, `importCheckvistNow`
(set by `AppDelegate`), and `lifecycle`.

### Failure state

`AppCoordinator.statusMessage` erases itself on a timer and
`CheckvistAccount.errorMessage` is overwritten by the next failure.
`DiagnosticsLog` is the bounded in-memory record that closes that gap. It is
fed from `LifecycleController` and `AppCoordinator`, chained onto the
`onError` / `onStatus` / `onErrorMessageSet` handlers — **those are single
closure slots, not multicast**, so anything else that wants to observe them
has to chain too, not replace.

## The desktop workspace: one refresh per action

The desktop window reads `WorkspaceViewModel`, which refreshes through
`WorkspaceViewModel+Refresh.swift`.

- A reload called inside `perform { }` (a write) or `batchingRefreshes { }`
  (navigation) only marks a `WorkspaceRefresh` flag — `.outline`, `.board`,
  `.sidebar`, `.dailies`, `.nextUp`. When the outermost block ends the marks are
  served once, in that order, and `perform`'s side effects (draft refresh,
  `onLocalWrite`) run once. A reload called outside any block runs at once.
- Reading `visibleNavigationTasks` mid-block serves the marks first, so code
  that changes scope and then asks what is on screen sees the new scope. Any
  other read of derived state after a reload in the same block must call
  `flushPendingRefresh()` itself.
- The main pane's scope — one list, or Everything or a folder together — is
  read once (`WorkspaceScopeRead`) and kept resident between switches in
  `WorkspaceScopeCache` (`WorkspaceViewModel+ScopeCache.swift`) with the
  `changeStamp()` it was read at; the outline, board and matrix are shaped
  from it once and kept with it. A switch to a scope whose stamp still holds
  reads nothing. See `docs/performance.md` for what is resident, when it is
  dropped and what runs off the main thread. Pass `refreshSidebar: false`
  when a change cannot touch a list or a count — a plain task's status or
  title.
- `.nextUp` is ranked off the main thread (`WorkspaceNextUpSnapshot`) and
  applied when it lands, unless something was written meanwhile
  (`writeEpoch`), in which case it is read again. `reloadNextUpNow()` is the
  synchronous form for the few callers that read the ladder on the next line.
- After a refresh `taskCache` holds every task a surface names — board, outline,
  nested lists, done rail, the day, the selection, the running block — so
  `task(withID:)` in a view body is a lookup, not a query. `dayItems`, the
  undo/redo labels and the outline's grouping are stored, not computed.
- Views take selection as values (`isSelected`, `hasKeyboard`,
  `selectedCardID`) from a parent that reads `selectedTaskID` once, so an arrow
  key re-renders the two rows it moved between.
