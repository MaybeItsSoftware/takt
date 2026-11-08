# Takt desktop workspace roadmap

> **Historical, 2026-11-08.** The Swift Mac app this describes (`Takt/`) has been
> retired; the Mac app is `desktop/` (Rust, GPUI). See `docs/rust-desktop-plan.md`.

## Purpose

Turn Takt from its original Checkvist-first menu-bar utility into a local-first macOS planning app: fast list outlines and focus flow in the spirit of Blitzit, with the planning, time-tracking, reporting, and project capabilities associated with Super Productivity.

The local SQLite workspace is the source of truth. Checkvist and other services are optional import/export/integration edges, never a requirement for opening, editing, or focusing work.

## Current baseline

Already implemented and committed:

- Main app opens as a normal desktop macOS app (`LSUIElement = NO`), with a simple menu-bar launcher.
- SQLite + GRDB workspace database at `~/Library/Application Support/Takt/priority.sqlite` (the file keeps its pre-rename name; a `Priority/` folder from an older install is copied across on first launch and left in place).
- Workspace, folders, lists, nested task outlines, notes, completion, basic inspector.
- Manual focus queue, persisted focus session, focus panel, floating timer.
- Desktop keyboard navigation: task selection/tree traversal, completion, focus, list switching, task/list/folder creation, shortcut reference.
- One-time migration of legacy offline tasks and the currently loaded legacy Checkvist list.

Relevant implementation locations:

| Area | Files |
| --- | --- |
| Local schema and persistence | `Sources/TaktWorkspace/WorkspaceModels.swift`, `Sources/TaktWorkspace/WorkspaceStore.swift` |
| Desktop state and migration | `Takt/WorkspaceViewModel.swift` |
| Desktop UI | `Takt/WorkspaceDesktopView.swift`, `Takt/MainWindowController.swift` |
| Focus floating timer | `Takt/FocusPanelController.swift` (the floating `NSPanel`), `Takt/FocusPanelView.swift` (its contents) |
| Startup and legacy import trigger | `Takt/AppDelegate.swift` |

## Non-negotiable product rules

1. Local first. All normal task operations work with no account or network connection.
2. No destructive migration. Import must copy source data, be idempotent, and report results.
3. The desktop workspace is the only task-editing surface. The menu bar is a
   status surface: it names what is next, drops the day down as a menu you can
   start work from, and outlives the window — `⌘Q` puts the windows away and
   leaves it running, and only its own Quit item ends the process. It still
   edits nothing.
4. Preserve keyboard parity. Every common task operation must be possible without a mouse and be discoverable in the shortcut reference.
5. Prefer small, reversible database migrations and test each migration with fixture databases.
6. Do not delete legacy source/data until its desktop replacement exists and migration/export is verified.

## Phase 1 — Finish the daily workspace

Goal: make Lists + Focus viable for daily use without relying on the old app.

### Lists and folders

- Treat active lists as sub-lists of a virtual Everything scope. Its views should aggregate tasks while preserving each task's list, project hierarchy, and an explicit destination for new work.
- Rename, archive, restore, delete, and reorder lists/folders. *Renaming is done
  in place — double-click, context menu, or ⌘R — rather than through the settings
  sheet. Reordering is a drag onto the gap between two rows, which is a separate
  target from the row itself so that dropping **on** a list can go on meaning
  "nest inside it". Folders drag too, and refuse their own descendants.*
- Support nested folders in both persistence and sidebar presentation.
- Add move-list-to-folder, move-folder, and keyboard commands.
- Add a list settings inspector: color, archive state, task count.

### Task editing and organization

- Add full task inspector fields: due date, estimate, tags, priority, recurrence, links/attachments as appropriate.
  *Recurrence now does something: `PeriodicSchedule` in `TaktCore` owns the
  vocabulary (`daily`, `weekdays`, `weekly`, `every N days/weeks`, `every
  <weekday>`) and completing a task that carries one closes that occurrence for
  good and writes the next as a new task, dated by stepping the cadence from the
  dates the finished one carried. It steps past a gap rather than restarting, so
  an ignored rhythm returns in the future rather than already overdue. Nothing
  pushes it into today — it carries a start date and the day claims whatever
  starts today. `RecurrenceRule` is now a shell over the same parser rather than
  a second copy of it.*
- Create/delete/rename/reorder/move/indent/outdent tasks.
- Support moving tasks between lists while preserving subtrees.
- Implement multi-selection and bulk complete/move/tag/delete.
- Add drag-and-drop, but keep an equivalent keyboard operation for every drag action.
- ~~Add undo/redo for all local mutations.~~ Done: every mutating store write runs through `journalledWrite`, which records row-level before/after images under one step per operation. ⌘Z / ⌘⇧Z.

### Fast capture and discovery

- ~~Make the global quick-add hotkey create a local Inbox task and focus the desktop composer.~~ Done: the hotkey selects the Inbox, leaves the focus screen, and puts the caret in the composer.
- ~~Add local full-text search across task title and notes (SQLite FTS is preferred).~~ Done: external-content FTS5 over title and notes, kept level by triggers. ⌘F.
- ~~Reach focus without going to the app.~~ Done: a global hotkey (`⌃⌥⇧⌘F`) summons a
  floating focus panel over whatever you are in, with the caret already in its
  field. Empty it shows the day as a list of numbered cards — the Today column,
  each with its estimate and time-on-task, under a bar of planned against
  logged — with the running card grown to carry a live clock and a pause / skip
  / log / done strip. Typed it searches every task's title and notes and offers
  to add what was typed to today. Return starts, queues or creates, ⌘Return
  opens the task in the window, Escape hands the keyboard back to the app it
  interrupted. It carries none of the focus screen's ceremony — conditions, a
  time window, an estimate committed to up front — because that is how a day is
  decided, and this is the surface for working one.
  It is a window you leave up rather than an overlay that vanishes on the next
  click — movable, resizable, and remembered — because the thing most worth
  seeing while you work elsewhere is the clock that is running. It is also
  self-sufficient: starting, pausing, logging and *scoring* a block all happen
  in the panel, so none of it needs the main window and the app can stay out of
  the Dock for a whole session. Standard editing shortcuts are routed to its
  field by hand, since an accessory app has no Edit menu to carry them.
- Add filters for open/completed, due, tags, priority, and estimate.
- Add a command palette for navigation and creation; do not route it through legacy Checkvist state.

### Acceptance criteria

- A user can create a project outline, reorganize it, complete work, and find a task entirely offline.
- No common operation requires the legacy popover or a Checkvist credential.
- Keyboard reference documents every new action.
- Store tests cover tree moves, ordering, deletion/archival, undo boundaries, and FTS/filter behavior. *Filters are the part still missing; the rest is covered.*

## Phase 2 — Data safety and complete migration

Goal: make local data trustworthy before adding more views.

### Export, restore, and backups

- Add versioned JSON export containing workspace, folders, lists, tasks, focus data, and metadata.
- Add import/restore preview with validation and a clear merge/replace choice.
- Add automatic rotating backups in Application Support and a visible “last backup” status.
- Add database integrity check, migration error reporting, and recovery instructions.

### Legacy/Checkvist import

- Build an import assistant with source selection and a task/list count preview.
- Import all configured Checkvist lists, not only the currently loaded list.
- Preserve source-list names, hierarchy, notes, ordering, due dates, and completed/invalidated status where supplied by the source.
- ~~Track an immutable external source ID in local metadata so re-running import merges safely instead of duplicating tasks.~~ Done: `tasks.sourceSystem` + `tasks.sourceId` behind a partial unique index. A re-import updates content in place and leaves local placement alone.
- Provide an import result report: created, updated, skipped, orphaned-parent repairs, and errors. *`TaskImportOutcome` now carries created/updated; the rest is unreported.*
- Treat Checkvist sync as optional and explicitly separate “one-time import” from future two-way sync.

### Acceptance criteria

- Export then restore into an empty profile preserves the local workspace exactly.
- Re-running any import is idempotent.
- An import failure never leaves a partially committed source list.
- Automated tests cover old offline payloads, multiple Checkvist lists, completed tasks, malformed trees, duplicate IDs, and interrupted imports.

## Phase 3 — Planner views

Goal: give the local workspace the planning loop users expect from a desktop task app.

### Inbox, Today, Upcoming

- ~~Make Inbox a first-class system list.~~ Done: `task_lists.systemRole`. Found by role rather than by name, so renaming it is safe; archiving and deleting it are refused.
- Implement Today as a local planning view: manually include tasks, schedule tasks, reorder the day, and show overdue/upcoming work.
  *Partly done: `DayPlanSelector` derives the day from the running block, the
  hand-placed Today column, then overdue, due-today and starts-today work, each
  task claimed once by its strongest reason. The focus panel reads it. Manual
  reordering of the derived part, and the Upcoming view, are still missing.*
- Add Upcoming grouped by day/week and a clean unscheduled view.
- Ensure changing Today does not change a source list unless the user explicitly moves/schedules a task.

### Projects and visual modes

- Add project metadata and project overview/progress.
- Add local Kanban and Eisenhower views backed by workspace metadata, not old popover state.
- Add saved filters/views for contexts, tags, and projects.
- Add a calendar/time-block view once dates and estimates are stable.

### Acceptance criteria

- A user can capture into Inbox, plan Today, execute in Focus, and review/reshape a project without leaving the desktop app.
- Today and visual views are alternate projections of the same local tasks; no duplicate task stores.

## Phase 4 — Complete Focus and time tracking

Goal: match the low-friction focus flow of Blitzit and add Super Productivity-style time awareness.

### Focus sessions

- ~~Give a running session a surface the size of the work.~~ Done: the focus pane
  has two states rather than two surfaces — the ladder that picks a task, and
  then the block itself. It was a dialog-sized sheet over the board, which left
  the workspace visible round the edges and made returning to a session a
  different gesture from starting one. The window also opens on it, behind the
  `opensOnFocusScreen` preference.
- Add pause/resume, work/break phase transitions, skip, configurable durations, and session completion behavior.
- Support reordering/removing queue items while focused.
- Restore an active session correctly after relaunch, including timer elapsed time.
  *Each block is now timed from its own start rather than the session's, so a queued
  task is no longer credited the previous task's sitting.*
- ~~Add local notifications and optional sound; notifications must degrade safely when permission is denied.~~ Done:
  dispatched via UNUserNotificationCenter when a focus block reaches planned length or a break ends, with
  configurable notifications and sound preferences.

### Time tracking and review

- ~~Score a finished focus block by how well the work went.~~ Done: a block is worth
  the minutes it took, to one decimal place, multiplied by a quality multiplier the
  user gives when they press Done. Five presets from Scattered (×0.5) to Flow (×2.0),
  or any multiplier up to ×5. Scores are kept in `focus_awards` with the task's title
  copied in, so renaming or deleting the task later cannot rewrite what was earned.
- ~~Show the day's focused work back as a timeline.~~ Done: ⌘9 gives the timeline the
  main pane the way ⌘8 gives it to focus. Blocks are drawn against an hour ruler from
  when each one ran — a stored block is active seconds ending at the moment it was
  logged — with the running block growing live, a per-task breakdown beside it, and a
  day picker. It replaces the panel-sized history, which was too small to compare the
  shape of a day against the shape you meant it to have.
- Add manual start/stop tracking per task and project.
- Persist work logs independently from task completion.
- Add daily/weekly reports: focused time, estimates versus actuals, completed work, and project distribution.
  *Partly done: tasks now record `completedAt` in a column of their own — it was
  `updatedAt`, which moved whenever a finished task was edited — and
  `WorkProgressSummary` gives today's completions and logged time the week's
  equivalents as a denominator, along with the week's average over the days that
  have actually elapsed. The focus panel shows it under the day's bar. It is
  deliberately relative to the week rather than to a configured target, since a
  goal you must set up before the number means anything is a goal you will not
  set up. Estimates-versus-actuals and project distribution are still missing.*
- Allow CSV/JSON export of time logs.

### Acceptance criteria

- Timer state survives quit/relaunch without double-counting.
- Completing a focused task advances the queue predictably.
- Reports are calculated locally and have deterministic test fixtures.

## Phase 5 — Optional integrations

Goal: add value at the boundary without compromising local ownership.

- Checkvist: import first; only design two-way sync after conflict policy, source IDs, delete semantics, and offline queue behavior are specified and tested.
- Calendar: optional time-block export/import with an explicit mapping and no invisible task edits.
- MCP: retain as an optional plugin. The Obsidian task sync and AFFiNE plugins were deleted once nothing could reach them (2026-10-13); Obsidian daily notes stay with the Daily Log plugin.
- Add integration status, permissions, retry controls, and a clear “disconnect without deleting local data” flow.

## Phase 6 — Remove legacy surface

Only begin after Phases 1–3 have replacements and migration/export are proven.

1. ~~Remove all calls and shortcuts that mutate legacy Checkvist/popover task state.~~ **Done for the calls.** The legacy command executor was the last thing that could reach them, and nothing had called it since the popover went: it is deleted, with the outline navigation it drove and the kanban column moves and board add the coordinator kept for it. The `kanban`, `matrix` and `tab` command families had already left the palette. What remains of the Checkvist write path is what sync and import use. `TaskMutationService`'s current-task mutations and `SyncService.moveTask` now have no caller either; their tests still pin them, so removing them is left to whoever next takes the services apart. **Finished 2026-10-13:** the whole Checkvist write path is gone — `TaskRepository`, `SyncService`, `TaskMutationService`, `UndoService`, `NavigationState`, the offline store and the list-scoped stores, the service hosts and the `TaktAppLogic` target. Checkvist is an import source: `CheckvistAccount` loads the lists through the Rust core's `checkvist.rs`, and the settings page chooses which of them the workspace copies.
2. ~~Replace remaining legacy settings with workspace/integration settings.~~ **Done for the settings window.** Settings is a sidebar window of General, Focus, Appearance, Keyboard, one page per integration, Sync and Advanced. Removed because nothing read them: the menu bar width, the font name, the legacy timer's position and mode, the named-time hours, the popover heights, and the accent preset / colour-token overrides / theme JSON. Replaced: the Quick Add parent task ID typed by hand is a workspace list chosen from a menu; the export of the Checkvist task cache is an export of the whole workspace as Markdown or JSON; merging Checkvist lists moved to the Checkvist page. The legacy command engine's `kanban`, `matrix` and `tab` families left the palette on 2026-09-26, and the executor that ran what was left went in step 4.
3. ~~Remove the old popover controller, popover views, popover layout, old task keyboard router, and their build references.~~ **Done.** The panel had already stopped being reachable — the status item shows a menu of today and the global hotkey toggles the window — so this removed a surface rather than changing behaviour. Gone with it: the kanban, matrix, daily-checklist, legacy focus-session and shortcut-reference views, the shared celebration row treatment, and `ShellMode`. `DiagnosticsView` was kept and mounted on the main window.
4. ~~Remove legacy-only managers after proving no plugin or CLI depends on them.~~ **Done.** Gone: `CommandExecutor`, `TaskNavigationService` and `TaskNavigationCoordinator`, `KanbanManager` and `KanbanTaskDataSourceAdapter`, `PopoverChromeManager` (its diagnostics flag now lives on `AppCoordinator`), and the `TaktCore` engines only they read — `KanbanColumn`, `KanbanSwimlanes`, `KanbanFilter`, `KanbanSelection`, `KanbanManualOrder`, `EisenhowerInheritance`, `MatrixSpread` and `MatrixClustering`. No plugin read them, and the CLI shares no Swift. Then the last three readers of the legacy task list were moved off it, and it went too: the Google Calendar settings page's "Create event from selected task" adds the task selected in the desktop window, through the inspector's own "Add to Google Calendar"; the daily log's plan is worked out from the Checkvist tasks by `DayLogPlan` (it stays Checkvist's, because the log keys its events by Checkvist id and only the Checkvist path records completions in it); and the Checkvist cursor `SyncService` and `TaskMutationService` still select into is answered by `AppCoordinator` from `NavigationState` and the repository's tasks. Gone with it: `TaskListViewModel`, `CacheState`, `CacheInvalidationBus` and every producer's `invalidate()`, `TaskOutlineBuilder`, `TaskVisibilityEngine`, `TaskScopeResolver`, the sort and tag halves of `TaskFilterEngine`, and `NavigationState.matrixSelectedTaskId`. No stored data was touched: the old preference keys and payloads stay where they are, and `taskMoveMode` still reads the persisted root view.
5. Rewrite README/onboarding/help around the desktop local-first workflow. *(README, `CLAUDE.md`, `docs/cli.md`, `docs/mcp-server.md` and `docs/state-ownership.md` have been brought in line with step 3. The onboarding dialog queue, which nothing ever displayed, is deleted; the in-app help is the command palette and keyboard reference, both drawn from the catalogue.)*
6. Run a clean install/migration/export/restore test before deleting compatibility code.

Step 3 left two Preferences panes configuring state that no longer had a
surface. Both are settled: the **Kanban columns** editor is gone, and
**Keybindings** holds only what is live — the three global hotkeys
`GlobalShortcutManager` registers, and a way into the window's own keymap
(`keymap.json`, laid over `WorkspaceCommandCatalog`; see
`docs/keyboard-shortcuts.md`). The per-action remapping stack it used to edit
(`ConfigurableShortcutAction`, `ShortcutResolver`, `ShortcutGate`,
`ShortcutSequenceBuffer`) was deleted with it. The legacy command engine's
`kanban`, `matrix` and `tab` families are gone from the palette, and the
executor that ran the rest of that engine was deleted in step 4.

Do not delete the legacy local payload automatically; keep it until one successful export/backup after migration, then offer the user an explicit cleanup action.

## Suggested delivery sequence

1. Phase 1 task/list CRUD, moving/reordering, local quick capture, search.
2. Phase 2 export/backup/import assistant and durable external IDs.
3. Phase 3 Inbox + Today + Upcoming.
4. Phase 4 focus pause/break/time logs/reports.
5. Phase 3 visual planner modes and calendar.
6. Phase 5 integrations.
7. Phase 6 legacy removal.

Each item should land as a coherent, tested commit. Do not combine schema changes, broad UI redesign, and legacy deletion in one change.

## Verification checklist for every milestone

- `swift test`
- `xcodebuild -project 'Takt.xcodeproj' -scheme 'Takt' -configuration Debug -destination 'platform=macOS' build`
- Manual keyboard-only smoke test for the changed workflow.
- Relaunch test to verify persistence.
- Offline test with network unavailable where relevant.
- Migration/export test whenever a schema or source mapping changes.

## Definition of done

Takt is done with this transition when a new user can create and organize local work, plan Today, focus, track time, export/restore safely, and optionally import/sync integrations — all from the desktop workspace — with no legacy menu-bar task interface or Checkvist account required.
