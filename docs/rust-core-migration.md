# Rust core migration

Decided 2026-10-07: the workspace logic moves into one Rust crate,
`takt-core`, which every client calls. The interfaces stay native: SwiftUI on
the Mac and iPhone, Compose on Android, ratatui in the terminal. This file is
the plan; tick steps off here as they land.

## Why

The same store is written three times, and the copies drift.

| Copy | Where | Lines (2026-10-08) |
|---|---|---|
| Swift store and sync | `Sources/TaktWorkspace`, `Sources/TaktSync` | ~7,750 |
| Rust CLI writes | `cli/src/workspace*.rs` | ~2,860 |
| Kotlin store and sync | `mobile/android/data` | ~5,680 |

The 2026-10-07 audit found the drift this causes. The Mac had no SQLite busy
timeout while the CLI did. Sibling ordering lacked the CLI's `id` tiebreak. A
cross-list move journalled its rows differently on the server. Each fix had
to be made, and tested, up to three times. With one crate it is made once.

## Shape

```
core/                     takt-core: a library crate, no I/O beyond SQLite
  src/schema.rs           migrations, one ordered list (today: WorkspaceStore+Migrations.swift, WorkspaceSchema.kt)
  src/store/*.rs          write paths, one module per WorkspaceStore extension
  src/journal.rs          undo_control / change_log
  src/sync/*.rs           HLC, push/pull, merge (shared with sync-server)
  src/ffi.rs              UniFFI surface: records, errors, a Store object
cli/                      depends on core directly; workspace_tasks.rs shrinks to calls
sync-server/              depends on core's sync/merge types directly
Sources/TaktCoreFFI/      generated Swift bindings + the xcframework, linked by Mac and iPhone
mobile/android/core-ffi/  generated Kotlin bindings + the .so per ABI
```

A Cargo workspace at the root holds `core`, `cli` and `sync-server`, so all
three build against one lockfile.

**Database access.** The crate owns the connection, through `rusqlite` with
the bundled SQLite, so every platform runs the same SQLite version and the
same pragmas: WAL, a five-second busy timeout, foreign keys. Swift no longer
opens the file through GRDB at all.

**Change notification.** The apps poll `PRAGMA data_version` on the core's
own connection (`CoreWorkspace.dataVersion`). Every write of theirs goes
through that connection, which does not count its own commits, so the number
moves only for another process, with no "ignore our own writes" bookkeeping.

**Threading.** The `Store` object is `Send + Sync` behind a connection pool.
Swift calls it from a background executor and hops to the main actor with
the result, as `WorkspaceStore` callers already do.

## Order

Each step is one or more green commits. A step is done when the moved
behaviour has one implementation, and the Swift, Kotlin and cargo tests that
covered the old copies pass against it.

1. **Scaffold. Done 2026-10-08.** `core/` is the `takt-core` crate with
   UniFFI. `scripts/build_core_apple.sh` builds the xcframework and the Swift
   bindings (`Sources/TaktRustCore`). The data module's `buildRustCore`
   Gradle task runs `scripts/build_core_android.sh` for the `.so` files and
   the Kotlin bindings (`uniffi.takt_core`). One call, `coreVersion()`,
   crosses both bindings. It is shown in the Mac's Diagnostics and in
   Android's Settings, where a minified release on an emulator read it. It is
   tested by `TaktRustCoreTests` and `TaktCoreTest.kt`. CI builds the core
   before every Swift job and lints and tests it in a `core` job. There is
   no root Cargo workspace: `cli/target/release` is where the installed
   `takt` symlink points, and a workspace would move every crate's `target`
   to the root. Nor do the crates share one target directory: measured on
   2026-11-19, it saved 0.6 of the 6.8 GB the CLI, core and desktop gates
   leave behind, and those builds took half as long again rather than less.
2. **Schema and migrations. Done 2026-10-08.** `core/src/schema` holds the
   twenty migrations. Their SQL was captured from GRDB's migrator as it ran,
   so it was not retyped. The three steps that walk existing rows (v11, v12,
   v13) are ported by hand and tested. Progress stays in GRDB's
   `grdb_migrations` table under the same identifiers. The Mac and iPhone
   call `migrateWorkspace` before GRDB opens its pool. Android calls it
   before the androidx driver opens the file. The Swift migrator and
   Kotlin's migration steps are deleted. `scripts/dump_workspace_schema.sh`
   generates the fixture from the core, and a core test holds a new database
   to it byte for byte. On a backup of the real database the core applied
   nothing and changed nothing; on an Android emulator a release build
   opened an existing install through it and saved a task.
   `core/src/schema/triggers.rs` generates the journal and outbox triggers,
   matching all 69 in the fixture, for the next migration that needs them.
3. **Undo journal. Done 2026-10-08.** `core/src/journal.rs` holds undo,
   redo, the labels, the history and the history target, and `begin` and
   `finish`, which arm and settle a step inside a caller's transaction. The
   Mac, iPhone and Android call undo, redo and the reads through
   `CoreWorkspace`, the handle `core/src/workspace.rs` exports: one
   connection beside the client's own, on the same SQLite library. The CLI
   links the core and brackets its writes with `begin` and `finish`. The
   triggers moved in step two. What stays duplicated is `journalledWrite` in
   Swift and Kotlin: it has to run inside the client's own transaction, so it
   goes when the writes it wraps move in step four. Because the core writes
   on its own connection, the Mac takes in the new `data_version` after each
   `perform`, and Android's `WorkspaceDatabase.coreWrite` announces the
   changed tables itself.
4. **Task writes. Done 2026-10-08.** Every journalled write is in the core,
   and the Swift store, the Kotlin repository and the CLI call it: `tasks.rs`
   (create, delete, move, nudge, drop, indent, outdent, board column,
   matrix, status with the next occurrence), `lists.rs` (folders and lists,
   their settings sheets), `conversions.rs` (tasks to lists and back,
   nested-list flags, boards), `editor.rs` (the task editor and planning),
   `conditions.rs`, `dailies.rs`, `habits.rs` (with `HabitPolicy`),
   `today.rs` (focus order, planning for today), `waiting.rs` (waiting and
   follow-ups), `focus.rs` (sessions, with the candidate read and
   `TaskAvailabilityPolicy`) and `periodic.rs` (`PeriodicSchedule`). Each
   takes the caller's transaction, so the CLI journals it under "MCP: ";
   `CoreWorkspace` journals it for the apps. `journalledWrite` is gone from
   both apps. Their own core commits are kept out of their external-change
   tokens.
5. **The rest of the writes. Done 2026-10-08.** Bootstrap and the Inbox,
   themes and preferences (`setup.rs`), the Checkvist import, the
   plugin-era dailies and the board baseline (`imports.rs`) are in the core.
   No client opens a write transaction of its own any more, except
   Android's benchmark fixture. The CLI's workspace writes call the same
   functions, so it no longer copies Swift row for row; a real-data run
   against a copy of the live database (add, rename, link, waiting with a
   past follow-up, complete, task to list, delete) left it consistent.
6. **Reads. Done 2026-10-08.** Every read the Mac, iPhone and Android make
   is the core's. `records.rs` holds the task, list, folder and workspace
   rows, the outline and the visible-root reads; `search.rs` the FTS prefix
   search with titles weighted over notes; `rows.rs` dailies and what a day
   shows, the completion streak, conditions, metadata, focus sessions and
   queues, work blocks, awards and points, themes, preferences, boards and
   counts; `editor.rs` the editor snapshot. Each client keeps its record
   types and its small in-memory trees (`WorkspaceListTree`) over the core's
   rows, so screens did not change. Android's `Db` reaches the core, so the
   helpers its repository shares switched in place.
   **The next-up ranking (2026-10-08).** `core/src/ranking.rs` holds
   `NextUpSelector.evaluate` and `score`; `focus.rs` holds the candidate read
   and `TaskAvailabilityPolicy`. Swift's `TaktCore` and Kotlin's `:core`
   keep their types and wrap the core, so their callers did not change; the
   Kotlin bindings moved into `:core` for it. On a seeded 5,000-task
   workspace on the JVM, the whole next-up snapshot went from 37.7 ms to
   18.2 ms: the core's candidate read is 7.1 ms against Kotlin's 31.9 ms,
   while ranking 2,496 candidates costs 6.8 ms against 3.8 ms, most of it
   the crossing.
7. **Sync engine. Done 2026-10-08.** `core/src/sync.rs` holds the pairing
   state, the snapshot, the outbox coalesced per row, acknowledging, and
   applying a pull (local edits win until pushed, unique-key rivals resolved
   the same way on every device, a second workspace folded in, orphans
   removed). The Mac, iPhone and Android call it. The hybrid logical clock
   and the server's merge are one crate, `takt-sync-rules`, in
   `sync-server/rules/`: the server merges with it, and the core depends on
   it and exports `hlc_tick` and `hlc_receive`, which Swift's and Kotlin's
   `HybridLogicalClock` wrap. It lives inside `sync-server/` because Railway
   builds the server from that directory alone; the server's `Cargo.toml`
   makes it a workspace member, so both share one lockfile. What stays in
   each client is the transport: HTTP, auth and the long poll, which are
   platform code by nature. Since step 9 the bodies are the core's too (see
   "The sync wire" below).
8. **Remove the copies.** Delete GRDB from the package, the Kotlin
   repository bodies and the CLI's store code. `TaktWorkspace` becomes a thin
   Swift wrapper over the generated bindings.
   **GRDB is gone (2026-10-08).** `WorkspaceStore` holds one `CoreWorkspace`
   and nothing else; the records lost their GRDB conformances; the
   external-change token is the core connection's `data_version`, on the Mac
   and on Android, so neither keeps count of its own commits any more. Tests
   that play another process open the file through `test-support/sqlite`.
9. **Pure logic.** The engines still implemented twice, in Swift's
   `TaktCore` and Kotlin's `:core`, move into the core; each platform keeps
   its types and signatures and wraps the core's function, so callers did
   not change, and the old Swift and Kotlin suites still run as the oracle.

   **Recurrence, habits and dailies.** `core/src/recurrence.rs` exports the
   rules the writes already used: `PeriodicSchedule`'s parsing and next
   occurrence (`periodic.rs`), `HabitPolicy`'s scheduled day, expiry, carried
   appearance, reconciled column and Habits list id (`habits.rs`),
   `WaitingFollowUp`'s due follow-up, title, tag and id (`waiting.rs`), and
   whether a daily is due, both the workspace's (`WorkspaceDaily.isDue`) and
   the plugin era's (`Daily.isDue`, whose cycle runs backwards from its
   anchor too), in `dailies.rs`; the CLI's dailies file reader calls the
   latter as well, so that rule has one copy instead of three. The Swift and
   Kotlin suites are ported as Rust tests in `core/src/recurrence/tests.rs`.
   Two drifts were settled for Swift: the core clipped a waiting tag to 40
   Unicode scalars where both apps clip to 40 grapheme clusters (it now uses
   `unicode-segmentation`), and a cadence that cannot land (an overflowing
   interval) returns none instead of panicking. Left native on purpose:
   `DayBoundary`, which `DayLogAggregator` calls once per log event, where a
   crossing would cost several times the Calendar arithmetic it replaces;
   the display strings (`displayLabel`, `scheduleLabel`, `HabitFrequency`'s
   labels, and `WaitingFollowUp.label`, `dateTimeText` and `editableText`,
   which are locale formatting asked for by every waiting card); the
   capture-word parsing `HabitPolicy` borrows from `TaskCaptureToken`, which
   belongs with the capture syntax; and `DailyCollection`'s ordering and
   editing, which is plain list bookkeeping.

   **Focus and progress (2026-10-10).** `core/src/progress.rs` holds the
   focus timeline's layout and per-task summaries (`FocusDayTimeline`), today
   against the week (`WorkProgressSummary`), the progress graph's days
   (`TaskProgressSeries`), finished work by day (`CompletedWorkDigest`), the
   stale-session rule (`StaleFocusPolicy`), block scoring (`FocusPoints`
   minutes, clamp and score, which `focus::finish_block` now uses too, so the
   prompt's preview and the stored award are one sum) and the completion
   milestone's precedence (`CompletionMilestonePolicy.milestone`). Swift and
   Kotlin keep their types and signatures and wrap it; Kotlin's
   `DayPlanSelector` now calls `plan_day` as Swift's already did. Each call
   passes its inputs once and gets back numbers or indices into what it
   passed, so titles and items never cross back. Two read straight from the
   rows: `CoreWorkspace.work_progress` (the Mac's and Android's weekly totals,
   which used to fetch the week's blocks and completions across) and
   `resolve_stale_focus_session`, which reads the session, applies the rule
   and closes or discards it in the core. Time zone maths is chrono-tz with
   the caller's zone name, as in `periodic.rs`. One platform difference:
   Kotlin kept a paused session with no task until the day was over, Swift
   discarded it at once; Swift's rule won. What stays native: the number
   format (`FocusPoints.formatted`), the quality presets, the celebration
   durations and `CelebrationRowTreatment`, which are presentation, and
   `DayLogAggregator`, which works over the Mac's own JSON event log rather
   than the database, exists on one platform, and would cross every event of
   that log on every completion. The CLI's `focus_history` is a plain sum of
   rows and stayed as it was.

   **Boards, outlines and the matrix (2026-10-10).** The sidebar's order is
   `sidebar/outline.rs`: `WorkspaceSidebarOutline.rows` and `listIDs` in
   Swift and Kotlin wrap it, and each row crosses back as a kind and an
   index into what was passed, so no string is copied back. The Mac keeps
   its Today row and its wrapping cursor; the phone asks without Today and
   keeps its clamped cursor, the one place the two copies had disagreed.
   A 137-row sidebar costs the same as before, 0.4 ms a build in a debug
   test. Android's sidebar read every active list's tasks one list at a
   time and walked them in Kotlin for its nested lists and badges;
   `sidebar_index_with_open_counts` walks them in the core, so only the
   nested lists and the numbers cross: 26 ms to 11 ms on a 5,000-task list
   on the JVM. The Mac draws totals, not open counts, so its
   `sidebar_index` skips the status read that cost it a tenth (5.4 ms
   against 5.0 ms on the 7,000-task benchmark) and stays at 5.0 ms. The rest
   of the cluster stays native, measured on the same benchmark.
   `TaskOutlineFolding` (both platforms) folds an outline the screen holds
   against fold state that lives only in memory, 0.04 ms for a list's
   outline, against 6 to 10 µs a row to cross. `MatrixGeometry`,
   `MatrixQuadrant` and `RootDueBucket` are per-point maths and display
   names. `WorkspaceListTree` is the small tree over the core's rows step 6
   kept; its `WorkspaceSidebarIndex` and `WorkspaceBoardTrees` builders
   stay as the oracles the core's `sidebar.rs` and `board.rs` are tested
   against. Android's board still shapes the trees its outline read in the
   same scope, as the Mac's single-list board does, because reading them
   again in the core would cross the same rows twice. Kotlin's
   `KanbanColumn` had no caller outside its tests and is deleted. The Swift-only engines
   worked on the Checkvist-era tasks `TaskListViewModel` and `KanbanManager`
   kept in memory, which the core never holds, so none of them moved. Since
   the desktop roadmap's Phase 6 step 4 (2026-10-10) most are gone instead:
   `KanbanManager` drew nothing once the popover went, and `KanbanFilter`,
   `KanbanSelection`, `KanbanManualOrder`, `MatrixClustering` and
   `MatrixSpread` (with the Mac's own `KanbanColumn`, `KanbanSwimlanes` and
   `EisenhowerInheritance`) were deleted with it. The rest followed once the
   Google Calendar settings page acted on the workspace's selection and the
   daily log's plan stopped reading the view model's cache: `TaskListViewModel`,
   `TaskVisibilityEngine` and `TaskScopeResolver` are deleted too. What is left
   of `TaskFilterEngine` stays native for the same reason as before: it buckets
   the Checkvist tasks by due date for the daily log's plan (`DayLogPlan`) and
   answers the Checkvist cursor and subtree spans for the sync and mutation
   services, all over tasks the core does not hold.
   `MatrixNavigation` and `MatrixViewport` had no callers outside their
   tests and are deleted. `BoardLinks` says which rows and cards name each other's
   columns, and turns on no measured height. It overlaps `board.rs` only in which card a
   subtask hangs from, which the board read already carries. `BoardLinkRoute`
   routes the line drawn between a link's two ends and gives each a lane in
   its gutter; it is geometry over frames the view has just laid out, so it
   stays native too.

   **Themes.** `core/src/theme/` holds the whole theme format: a reader and
   writer for the JSON dialect Swift's `JSONDecoder` and `JSONEncoder`
   defined (a trailing comma is accepted, a key stated twice keeps its first
   value, a type error names its path in the same words, output is
   pretty-printed with sorted keys), the file and its unknown-key warnings,
   seeds, the merge and `extends` resolution, the audit, the four built-in
   themes, export, and the shared files. Its tests rebuild every file in
   `shared/themes/` byte for byte from their inputs, and regenerating them
   from Swift leaves them unchanged. `TaktCore` and Android's `:core` keep
   their own theme types and call the core once per folder, file or theme,
   converting in `Theme+Core.swift` and `ThemeCore.kt`; Android no longer
   copies the built-in files onto its classpath, because the core hands them
   over. What stays native is what a view reads per render: a palette's role
   lookup, and the hex parse list colours go through in row views, which a
   call across the boundary would cost more than. `ThemeTypographyOverride`
   (the reader's fonts, applied through the core's merge) and the Mac's
   `ThemeFolderMirror` stay native too. Where the two ports had disagreed,
   Swift's behaviour is what the core does: fullwidth hex digits read as 0
   rather than failing, a non-integral number in a message reads as Swift
   writes it (`1e-05`, not `1.0E-5`), and the built-in scrim is exactly 0.7
   alpha on Android as on the Mac, where it was 179/255 read from a file.

   **Export.** `core/src/export.rs` writes the whole workspace, every list
   (archived too) in the sidebar's order with its task tree depth first, as
   Markdown or JSON: `CoreWorkspace.export_workspace(workspace_id, format,
   exported_at_ms)` reads and writes it in one call on the reader
   connection, so no row crosses and the clients get one string. The bytes
   are the ones the Mac wrote with Foundation's `JSONEncoder`
   (`.prettyPrinted, .sortedKeys, .iso8601`): the theme writer in
   `theme/json.rs` gained a `Json::Integer` and a slash-escaping entry point
   for it, since the export writes `/` as `\/` where themes do not. The
   Mac's builder is kept as a test oracle in
   `workspace-tests/WorkspaceExportTests.swift`, which seeds a store with
   every field set and unset, slashes, quotes, controls, U+2028, emoji, CRLF
   notes, archived and completed lists and nested lists, and holds the
   core's JSON and Markdown to `JSONEncoder` and the old loop byte for byte;
   the fixtures Android's port was pinned to moved to `core/src/export/`.
   `WorkspaceViewModel+Export.swift` (94 lines to 24) and Android's
   `WorkspaceExport.kt` (187 to 19) are now a call and the formats' names,
   extensions and MIME types (`WorkspaceStore+Export.swift`,
   `WorkspaceRepositoryExport.kt`). One difference between the ports was found and Swift's
   behaviour kept: Swift splits notes into `>` lines by `Character`, and
   `\r\n` is one character that is not `\n`, so a Windows line ending never
   split a note on the Mac while Kotlin split it and left a stray `\r`.
   There is no `takt export`: every CLI command is a call into the shared
   tool table, so a command there would have been an MCP tool too, and the
   iPhone has no export.

   **The day log.** `daylog.jsonl` was read twice, by Swift's `DayLogFileStore`
   and by the CLI's `local.rs` for `daily_log_fetch` and the dailies tools.
   Both readings, the appends, `DayBoundary`'s logical days and weeks,
   `DayLogAggregator`'s netting, buckets, summary and streak, and the text of
   `DailyNoteMarkdown.section` are now `core/src/day_log.rs`. Whole files cross,
   not events: `CoreDayLog` reads the file itself, holds the parsed log in Rust
   and answers each projection in one call, and recording an event is one call
   that appends under the same `flock` on `<file>.lock`. The Mac's Daily plugin
   holds one through `DayLogHistory`, so it no longer keeps an event array in
   Swift at all; the plugin contract lost `events` and gained
   `priorCompletionStreak(now:)`, which the streak milestone used to compute
   from that array. `DayBoundary`, `DayLogAggregator`, `DayLogFileStore`,
   `DayLogFormatting.focusDuration` and `DailyNoteMarkdown.section` keep their
   public signatures and wrap the core, and their Swift tests run unchanged as
   the oracle; the array-taking aggregator functions pass the whole array per
   call, which only the tests and one-off renders do. `DayBoundary` moved too:
   its zone and first weekday come from its `Calendar` (the core counts in the
   Gregorian calendar), and nothing calls it per event any more. The CLI calls
   the core as a Rust crate, and `daily_log_fetch` and `dailies_list` print the
   same bytes as before on a mixed fixture. Where the two readings disagreed,
   the core follows Swift: a line with an unknown kind, a missing or mistyped
   field, or a timestamp `JSONDecoder`'s `.iso8601` refuses (fractional seconds
   included) is dropped whole, where the CLI kept it with nulls; and a
   completion's `at` is echoed in the file's own `Z` form rather than as
   whatever text the line held, which differs only for a hand-written offset.
   What stays native is `ManagedMarkdownBlock`'s splice, shared with the AFFiNE
   export, and Android's own `DayBoundary`, which serves only its stale-focus
   policy and never reads the log.

   **Plugin converters.** The Mac's integration plugins kept two pieces of
   pure logic in `TaktCore`, and both are the core's now. `core/src/google_tasks.rs`
   is `GoogleTasksMirror.plan`: given the local lists and tasks, Google's
   lists and tasks and the ledger of what was last pushed, it returns the
   operations and the conflicts in one call per pass (lists adopted by exact
   title before they are created, completion honoured from either side,
   notes added in Google merged back, everything else Takt's, grandchildren
   flattened to Google's one level). `core/src/affine.rs` is the AFFiNE
   documents: a task's document and title, splicing a section under its
   heading, and the checklist rendered, read back (items and the lines Takt
   did not write, one pass) and rewritten, or not when nothing changed;
   `AFFiNEExportService` now makes one call to read a checklist and one to
   rewrite it, where it made five. Swift keeps the public types, the
   `Codable` ledger file and every signature, so no caller changed and
   `GoogleTasksMirrorTests` and both AFFiNE suites run unchanged as the
   oracle; they are ported to `google_tasks/tests.rs` and `affine/tests.rs`.
   Both engines compared and split text the way Swift's `String` does,
   which the new `swift_text.rs` reproduces: canonical equivalence for
   equality and `hasPrefix` (so a note Google hands back precomposed is not
   a conflict every pass), grapheme counts, a `\r\n` that does not end a
   line, and Foundation's whitespace sets. One order changed: the ledger is
   a dictionary, so Swift planned deletions of vanished lists and tasks in
   hash order; it now crosses sorted by local id, and they come in that
   order. HTTP and OAuth stay in Swift, as transport. Also native on
   purpose: `formatDueDate` and `parseDueDate`, which reduce a date to
   Google's day string in the user's `Calendar` before it crosses; the sync
   stamp and `dayDocumentTitle`, which format in that calendar with a
   user-chosen `DateFormatter` pattern; and `daySection`, which strips
   `DailyNoteMarkdown`'s markers and moves with that file. `GoogleTasks` and
   `AFFiNE` exist only on the Mac, so there is no Kotlin port to retire.
   The per-task document (`title(forTaskContent:)`, `taskDocument`) had no
   caller outside its tests, so it was deleted rather than ported.

   **Capture syntax and commands (2026-10-10).** `core/src/capture.rs`
   reads the add field's trailing tokens (`45m #work @fri !1 wait:Sam`),
   their preview labels, the follow-up field's day and time words, and
   Checkvist's free-text due dates; `core/src/command.rs` reads the Mac
   palette's commands and the date words its `due` and `start` take
   (`tomorrow 9am`, `4pm fri`, `in 90m`, `2026-4-2 8:05pm`). Swift's
   `TaskCapture`, `TaskCaptureToken`, `DueDateParsing` and `CommandEngine`,
   and Kotlin's `TaskCapture`, `TaskCaptureToken` and `DueDateParsing`, are
   wrappers now; the Swift tests are Rust tests too, with edges measured
   against Foundation before the Swift went. Where the platforms disagreed,
   the Mac's answer won: Android gained `^fri` and `wait:Sam` (its quick add
   now files and chips who a task waits on), and a Checkvist date with a time
   that is not an internet date-time is that day's midnight in UTC
   everywhere, which is what Foundation's lenient ISO reading answered,
   where Android had read the time and zone. Each parse is one crossing per
   keystroke, not per word. What stays native: the palette's suggestion rows
   and `WorkspaceCommandQuery`, which rank the Mac's own catalogue on each
   keystroke and fuzzy-match every list in the go-to overlay, so moving them
   would cross once per row; and the CLI's due bucket in the terminal UI,
   which reads Checkvist's leading `yyyy-MM-dd` as a local day and belongs
   with the board's buckets.

   **The sync wire (2026-10-10).** The bodies a cycle sends and receives
   are made and read in the core, with the server's own structs:
   `takt-sync-rules` gained `wire.rs` (`PushRequest`, `WireChange`,
   `ChangesResponse`, `ChangedRow`, `ErrorBody`), which `sync-server` now
   decodes a push and encodes a page with, and `core/src/sync/wire.rs`
   encodes a push and decodes a page with, so the two ends cannot drift.
   `prepare_sync_push` reads the outbox, stamps each row from the stored
   clock in edit order and returns one JSON string; `finish_sync_push`
   acknowledges it and stores the clock in one transaction. A pull gathers
   its pages in a `SyncPull` object, one string a page, and
   `apply_sync_pull` receives every stamp into the stored clock and applies
   them in one transaction. So rows no longer cross UniFFI as records in
   either direction, and neither platform keeps a clock between the steps:
   Swift's and Kotlin's `SyncEngine`s shrank to a loop that moves bytes, and
   `SyncTransport` on both carries bodies (`push(body:)`,
   `changes(...) -> Data`/`String`). The account body, the device
   registration body, the server's error text and its sentence, server
   timestamps, the `/health` and Supabase settings checks' JSON and the
   failure backoff are core functions too, and `core/src/sync/endpoints.rs`
   reads typed addresses for `SyncEndpoints.resolve` and
   `SyncServer.url(from:)` on the Mac and iPhone and `SyncEndpoints.resolve`
   and `httpURL` on Android. The Swift and Kotlin engine, transport and
   endpoint suites still run, their in-memory servers reading the core's
   bodies as JSON; `core/src/sync/wire_tests.rs` ports the cycle against the
   server's real merge, and the endpoint cases are Rust tests in
   `endpoints.rs`. Where the platforms disagreed, the Mac's answer won: a
   whole number in a page (`1.0`, `1e3`) is an integer, as Foundation read
   it, where Kotlin kept a real; a real with no fraction is pushed as `1`, as
   Foundation wrote it; and a typed address keeps its path's case and the
   Mac's trimming of pasted suffixes. Two Mac answers lost, because nothing
   could open the address they kept: a scheme that only starts with `http`
   (`httpx://`) and a user and password in the address are refused, as
   Android refused them; scheme and host are lowercased everywhere. The
   protocol did not change; the server's push and pull bytes are what they
   were. What stays native is the transport (`URLSession`, OkHttp, the
   token refresh, the long poll's timeout and Android's 25-second clamp on
   `wait`), the scheduler's rhythm, the sign-in flows, the credential
   stores, the legacy-address move, and the presentation: device names,
   status text and each platform's error wording.

   **Client configuration (2026-10-10).** `core/src/client_config.rs` is
   the format of the two kinds of config file Takt writes for other
   programs. The CLI's credential store, `~/.config/takt/config.json`, was
   read and saved by `cli/src/config.rs` and seeded and cleared by the Mac
   (`TaktCLIConfigWriter`, plus about a hundred lines of file handling in
   `IntegrationCoordinator`); now both go through the core: one reading (a
   JSON object, blank values absent, an empty file an empty config), one
   encoding (pretty, sorted keys, unescaped slashes, a trailing newline),
   the seeding merge (the app's username and remote key win, `list_id` only
   fills a gap, every other key survives, the legacy
   `~/.config/priority/config.json` read but never written), the clearing,
   and one private write, through a sibling temporary created 0600 and
   renamed over the target, with any folder it makes at 0700.
   `seed_cli_config` and `clear_cli_config_credentials` do the whole job
   given a home directory. An MCP client's own config has one format too:
   the entry's shape, the merge that keeps every other key and server, the
   rule that replaces an entry the app wrote as Priority (recognised by its
   command), the `claude mcp add-json` line, the Zed snippet, the pasteable
   whole config, and the catalogue of clients with their paths and
   detection markers. `TaktCLIConfigWriter`, `MCPClientConfigWriter` and
   `MCPClientCatalog` keep their public signatures and wrap it, and
   `NativeMCPIntegrationPlugin` builds its copied config through it, where
   it had its own `Encodable` copy of the entry. `TaktCLIConfigTests` and
   `MCPClientConfigTests` run unchanged as the oracle and are ported to
   `client_config/tests.rs`, with the file handling's modes, the legacy
   fallback and a symlinked config added. Where the two had differed: an
   empty file is an empty config to the CLI too, where it refused to save
   over one; the CLI's save is atomic now, where it truncated in place; a
   symlinked config is written through to its target on both sides, where
   the Mac's rename replaced the link; and the files are encoded by
   `serde_json` rather than `JSONSerialization`, so a key is followed by
   `": "` rather than `" : "`, empty containers are `[]` and `{}`, keys sort
   by code point rather than Foundation's case-insensitive order, a real is
   written shortest (`0.1`, not `0.10000000000000001`), and an integer past
   64 bits loses precision. The merge compares entries by value, so none of
   that makes a repeat setup report a change. What stays native: reading
   and writing another client's file, which needs the Mac's sandbox
   bookmark and leaves an existing file's mode alone; detecting a client,
   which is the Mac's `fileExists` over paths the core lists; the real home
   directory (`getpwuid`); the pasteboard; and the CLI's environment-first
   precedence, `$PRIORITY_CONFIG_PATH` and `$XDG_CONFIG_HOME`, which are
   the CLI's alone. There is no Kotlin caller, and the CLI did not gain an
   MCP-install command.

   **The phones' screens.** The iPhone's feature models and Android's
   view models were audited pair by pair for shaping both did over the
   core's rows; what moved is what let fewer rows cross or retired a second
   copy. Review's timeline (`core/src/review.rs`, `review_timeline`) keeps,
   lays out, groups and scores the day's blocks and hands back indices, and
   its progress charts are one read, `CoreWorkspace.review_progress`, which
   reads the period's completions, creations and blocks and buckets them by
   local day, so only the 7, 30 or 90 days cross. `TimelineDay` and
   `ProgressSummary` keep their types on both phones. Android's next-up
   snapshot read the candidates out, then sent them back in twice (to plan
   the day and to rank); it now makes the one `next_up` call the Mac and
   iPhone make (`NextUpSelector.read`), 13.9 ms to 10.5 ms for 2,496
   candidates on a 5,000-task list on the JVM, and Today, the widget and the
   focus service ask for the ladder's head only, as the iPhone's do. The
   inspector's subtask and logged-work counts are `task_work_facts`
   (`core/src/task_work.rs`): the iPhone read the task's whole list to walk
   its branch and Android ran SQL of its own for the blocks; four numbers
   cross now. The iPhone's board reads its scope through `scopeRead` and
   shapes it with `WorkspaceScopeShaping.board`, the Mac's path, so
   Everything's and a folder's board are walked in the core (`board.rs`)
   instead of reading every list's tree twice: 168 ms to 79 ms on the
   7,000-task benchmark. A combined scope's outline draws only the cards, so
   `actionable_tasks_packed` returns those alone, picked by the same
   selection `combined_board` uses: `WorkspaceStore.actionableTasks` went
   from 55 ms to 23 ms there. The iPhone's Today takes its logged time from
   the week's totals it already reads rather than reading the day's blocks.
   Where the copies disagreed, the iPhone's answer won: the best day of a
   tie is the first (Kotlin's `TaskProgressSeries.bestDay` took the last,
   under a comment that said Swift did), block seconds are summed as stored
   where Android clamped a negative to zero (charts and inspector alike), and
   a cancelled subtask counts as done. What stays native, and why. The done
   rail groups through `CompletedWorkDigest`, already the core's, over tasks
   it draws one by one, so they cross anyway; Android's timeline also marks
   the day's finished tasks, which the iPhone does not draw. Android's list
   screen shows Everything as every list's outline under a header, so its
   outline needs every tree and its board shapes the same trees, as above.
   The iPhone's Lists tree is SwiftUI composed over the structure already in
   memory, crossing nothing (Android's already uses the core's sidebar
   outline; its `openCounts` stays as the oracle for hand-built trees); the
   two still differ in that the iPhone puts sub-folders before a folder's
   lists and shows totals, Android lists first and open counts. The focus
   screen's start objections and wording, the requirement editing in the
   inspector, a stage's seed length and the conditions' visibility are per
   tap or per keystroke and cross no rows; they differ too (Android clamps a
   start to the planned length and turns the store's refusal into an
   override, dedupes requirement groups, and seeds a stage from a blocked
   candidate where the iPhone uses the task's estimate), which is product
   behaviour to settle, not shaping. Android's hide-completed outline drops a
   closed task's whole branch where the Mac and iPhone keep its open
   subtasks; it is still Android's own shaping.

   **Planning.** The `planningJSON` codec, its normalisation and the
   refusals' wording are the core's alone (`core/src/planning.rs`, read by
   the editor and the focus ladder too); Swift's and Kotlin's `TaskPlanning`
   wrap it, and `taskPlanningValues` is one crossing that returns only the
   planned tasks already decoded, where Android used to cross every metadata
   row and parse each in Kotlin.

   What stays in Swift and Kotlin after step 9 is presentation (views,
   locale formatting, colour conversion), platform transport (HTTP, auth,
   OAuth, the long poll, moving bodies the core makes and reads), per-point geometry and fold state that lives only
   in UI memory, per-tap logic that reads no rows, and the Mac's
   Checkvist-era engines, which work on tasks the core never holds. Each
   paragraph above says why for its own cluster.

10. **Everything else (audit 2026-10-11).** Decided 2026-10-11: every piece
    of logic that is not view code, OS integration or transport moves, so
    the apps are only views; the Checkvist client and import move too. The
    audit below walked every Swift file in `Sources/`, `Takt/` and
    `mobile/ios`, and every Kotlin file in `mobile/android`'s `core`, `data`
    and `app` modules, for logic the core does not yet own. Board columns,
    kanban and filter lists are left out on purpose: they are being reworked
    in parallel and get their own pass afterwards. "Both" means the same
    rule is written on more than one platform. Each row's plan is either
    *move* (and the commit that moved it fills in *done*) or *native*, with
    the reason.

    **Duplicated across platforms**

    | Item | Current home(s) | Both? | Plan |
    |---|---|---|---|
    | Checkvist requests and answers (login, lists, tasks, create, update, move, reparent, notes, actions, delete; token, lossy task and note reading, depth-first order) | Mac `NativeCheckvistSyncPlugin`, `CheckvistSession`, `CheckvistTaskRepository`, `CheckvistModels`; CLI `checkvist.rs` | Mac + CLI | move: `core/src/checkvist.rs`, sans-IO like the sync wire — **done 2026-10-12** ("The Checkvist client" below) |
    | Checkvist import seeds (dedupe, order, `checkvist:<list>:` ids, status, notes) | Mac `WorkspaceViewModel+Import` (three copies) | Mac ×3 | move: `checkvist_import_seeds` — **done 2026-10-12** ("The Checkvist client") |
    | `DayBoundary` | Kotlin `DayBoundary.kt` (hand port); Swift already wraps the core | Kotlin + core | move: Kotlin wraps the core's `day_boundary_*` — **done 2026-10-12** ("The screens' small rules") |
    | `BlockedSection` labels | `TaktCore/BlockedSection`, iOS `BlockedSections`, Kotlin `BlockedSection.kt` | all three | move — **done 2026-10-12** ("The screens' small rules") |
    | `DayArrangement` (reordering planned day rows) | `TaktCore/DayArrangement`, Kotlin `DayModel.kt` | Swift + Kotlin | move — **done 2026-10-12** ("The screens' small rules") |
    | `DayForecast` (remaining estimate, finish time) | `TaktCore/DayForecast`, Kotlin `DayModel.kt` | Swift + Kotlin | move — **done 2026-10-12** ("The screens' small rules") |
    | `FocusDeferral` (in an hour, this afternoon, tomorrow, next week) | iOS `FocusModel`, Kotlin `FocusModel.kt` | iOS + Kotlin | move — **done 2026-10-12** ("The screens' small rules") |
    | `FocusContextSelection` (condition toggle, restore) | `TaktCore`, iOS `FocusModel`, Kotlin `FocusViewModel` | all three | move — **done 2026-10-12** ("The screens' small rules") |
    | Task editor: estimate parsing, three-way reconcile, save validation | `TaktWorkspace/TaskEditorDraft`, Kotlin `TaskEditorDraft.kt`, `InspectorLogic.InspectorValidation` | Swift + Kotlin | move — **done 2026-10-12** ("The task editor's draft") |
    | Outline drop planning (sibling-only reorder, no-op detection) | iOS `OutlineDropPlanner`, Kotlin `OutlineShaping.planDrop` | iOS + Kotlin | **done 2026-10-13**: decided 2026-10-12 for the iPhone's rule; `outline_drop_plan` in `shaping.rs`, see "The outline drop" below |
    | Kotlin outline walk and sidebar index fallbacks (`WorkspaceListTree.outline`, `WorkspaceSidebarIndex.build`, `ListsTreeShaping.openCounts`) | Kotlin, beside the core's `outline`, `sidebar_index_with_open_counts` | Kotlin + core | native as test oracles, as `WorkspaceListTree` is on the Mac (step 9) |
    | `TaskOutlineFolding`, `MatrixGeometry`, `RootDueBucket` | Swift, Kotlin | both | native: per-point maths and fold state held only in UI memory (step 9) |
    | Date and duration text (`Format`, `WaitingFollowUp.label`, `FocusPoints.formatted`, `SyncStatusText`) | iOS, Kotlin, Mac | all three | native: locale presentation |
    | Focus wording and start objections (`FocusText`, `+Conditions`) | iOS, Kotlin, Mac | all three | native: per-tap wording over reasons the core already returns; the platforms differ on purpose (step 9, "The phones' screens") |
    | Sync session, token refresh, scheduler rhythm, credential stores | `TaktSync`, Kotlin `sync/`, `settings/` | both | native: transport and auth (step 7) |
    | Habit form, recurrence presets, requirement groups, blocker picker | Kotlin `HabitFormLogic`, `InspectorLogic`; Mac overlays | Mac + Kotlin | native: per-tap form state over values the core validates on save |
    | Undo history travel (`undoCount/redoCount`) | iOS `UndoHistory`, Kotlin `UndoCenter` | both | native: counts over the core's history rows, one subtraction |
    | Widget precedence (`NextUpSummary`, `WidgetSnapshotBuilder`) | iOS, Kotlin | both | native: picks the head of the core's next-up answer |
    | Keyboard chord parsing | iOS `KeyboardCommands.parse`, Kotlin `KeyCatalog.Chord` | both | **done 2026-10-13**: `keyboard_chord` in `keymap.rs`, read by the iPhone through `WorkspaceKeyChord` and by Android's `KeyCatalog`; see "The keymap and the catalogue" below |

    **Mac only**

    | Item | Current home | Plan |
    |---|---|---|
    | `WorkspaceKeymap` parse and resolve, `WorkspaceKeyBindings` | `TaktCore` | **done 2026-10-13** (decided 2026-10-12, with the catalogue): `keymap_parse` and `keymap_resolve` in `keymap.rs`; `WorkspaceKeyBindings`, the lookup table the router reads per key press, stays Swift and is built from the core's answer once per load |
    | `VimFind`, `CursorStepping` | `TaktCore` | move: pure list maths, one call per key press — **done 2026-10-12** ("The Mac's cursor rules") |
    | `DesktopShortcutSequence`, `DoubleTapModifier` | `TaktCore` | native: timers over `NSEvent` timing |
    | `WorkspaceCommandQuery` (palette ranking) | `TaktCore` | native: ranks the Mac's own catalogue every keystroke (step 9) |
    | `TaskDraftAnchor`, `DailyTitleEdit` | `TaktCore` | native: per-refresh UI state over rows on screen |
    | `IntegrationLinkStore`, `DailyNotePath`, `ManagedMarkdownBlock`, Obsidian note body, Google Calendar event payload, `AgentStreamProtocol`, `AgentToolSummary`, `JSONValue` (was `MCPWire`) | `TaktCore`, plugins | native: integration edges that exist only on the Mac and talk to its other programs; no second copy to retire |
    | `TaskRepository`, `SyncService`, `TaskMutationService`, `OfflineTaskSyncPlugin`, `LocalTaskStore`, `ReorderQueue`, `UndoService`, the list-scoped priority stores, `OfflineReplayPolicy`, `AuthRetryPolicy`, `RemoteKeyBootstrapPolicy`, `CompletionRetention`, `TimerElapsedReassignmentPolicy`, `TaskFilterEngine`, `DayLogPlan`, `TaskTreeFormatter` | `Takt/`, `TaktCore` | **deleted 2026-10-13** (decided 2026-10-12), with the `TaktAppLogic` target and its tests, `NavigationState`, the service hosts and the managers only it fed. The import and the settings page run on `CheckvistAccount` over the core's `checkvist.rs`; see "The Checkvist stack, deleted" below |
    | `DiagnosticsReport`, `SyncStatusSummary`, celebrations, `FocusTimerDisplay`, `RelativeDateTextCache`, `WindowFrameClamp`, `FileLock`, helper locators, `AppTerminationPolicy`, `OnboardingResetPolicy`, `AutoRefreshThrottlePolicy` | `TaktCore` | native: presentation and OS |
    | Theme palette lookup, hex parse, typography override, folder mirror | `TaktCore/Theming`, Kotlin `theme/` | native (step 9, "Themes") |

    **Android only**

    | Item | Current home | Plan |
    |---|---|---|
    | Dead raw-SQL helpers (`moveSubtree`, `upsertKanbanColumn`, `normalizedVisibleRootName`, `taskDescendantIDs`, `persistTaskOrder`, `encodeStringArray`) | `data/workspace/StoreSupport.kt`, `Queries.kt` | delete: no callers; the core does each — **done** (2026-10-12, and `upsertKanbanColumn` 2026-10-13) |
    | `SqlDates`, change tracking triggers, observers | `data/db` | native: the androidx driver's own observation |
    | `ChartScale`, `DateFieldMath` | `app/ui/review`, `app/ui/inspector` | native: axis and calendar-grid geometry drawn per frame |
    | `SearchMatching` (highlight ranges) | `app/ui/search` | native: per-row presentation of the core's search answer |
    | View-model orchestration (`ListViewModel`, `TodayViewModel`, `FocusViewModel`, `FocusActions`, `TaskInspectorViewModel`, `UndoCenter`, `ForegroundEngines`, `SyncController`, `TaskCommands`) | `app/` | native: lifecycle and coroutines over repository calls that are already the core's |
    | Board and matrix shaping, filter lists | `BoardShaping.kt`, `FilterList*` | **partly done 2026-10-13**, see "The board pass" below |

    **The Checkvist client (done 2026-10-12).** `core/src/checkvist.rs` is
    both halves of the conversation, without the network, as the sync wire
    is: `checkvist_request` turns a `CheckvistCall` into method, address,
    headers and a JSON body, and `checkvist_login_token`, `checkvist_lists`,
    `checkvist_tasks` (parents first, siblings by position, orphans' subtrees
    after, by missing parent), `checkvist_created_task` and
    `checkvist_created_list` read the answers into rows with the Mac's lossy
    rules (a number where text was expected, `text` for `content`, any key
    naming a due date or deadline). The CLI sends them through `ureq` and the
    Mac through `URLSession`; the token, the one retry after a 401 and the
    credentials stay with each, as transport. `checkvist_import_seeds` is the
    import's seeds, so the three copies in `WorkspaceViewModel+Import` are one
    call each. The CLI's project move keeps its request sequence and borrows
    the core's `project_tasks`. Where the two clients differed: every body is
    JSON (the Mac sent forms); a due date is `due_date`, the documented field,
    where the CLI sent `due`; the Mac now reads a note's `comment`, which is
    where Checkvist puts its text, so imported tasks carry their notes where
    they used to arrive empty; and the CLI keeps an orphan's subtree, where it
    dropped it. Checkvist's own `parse` stays the CLI's choice for its create
    and off everywhere else.

    **The screens' small rules (done 2026-10-12).** `core/src/shaping.rs`
    holds the rules each platform had written for itself: where the Blocked
    label goes (`blocked_section_labels`, groups crossing as numbers and the
    answer as row indices), moving a planned task up or down the day and
    showing the move at once (`day_arrangement_moving`, `_applying`), the
    day's forecast (`day_forecast`), turning a focus condition on or off with
    one place at a time and restoring a saved context
    (`focus_context_toggle`, `_restore`), and when a deferred task comes back
    (`focus_deferral_date`, wall-clock times in the caller's zone, a skipped
    hour resolved as `periodic.rs` does). Swift's `BlockedSection`,
    `DayArrangement`, `DayForecast` and `FocusContextSelection` keep their
    signatures and wrap it, and `FocusDeferralTime` joins them for the
    iPhone, whose focus model now calls both instead of its own copies;
    Android's `BlockedSection`, `DayArrangement`, `DayForecast.of`,
    `FocusDeferral.date` and `toggleCondition` call it directly, and
    `DayBoundary.kt` wraps the core's `day_boundary_*` as the Mac's does. The
    Swift, iPhone and Kotlin suites run unchanged as the oracle. No answer
    changed; the one difference the ports had is that Kotlin's day
    rearrangement swapped in the planned card for an id and Swift the bare
    id, which both still do over the core's ids.

    **The task editor's draft (done 2026-10-12).** `core/src/editor_draft.rs`
    holds the rules both drafts had written out: an estimate or minimum
    block typed in minutes read to whole seconds (`editor_parse_minutes`,
    the same pattern, rounding and range refusal), seconds shown back as
    minutes the way Swift prints a `Double` (`editor_minutes_text`), and
    the three-way reconcile a save underneath an open draft goes through
    (`editor_reconcile`): each field crosses as four booleans the platform
    worked out with its own equality, plus the estimate's texts and
    seconds for the rule that a dirty estimate whose text matches but whose
    seconds moved still conflicts, and each field comes back as follow the
    save or not and set, clear or keep its conflict. Swift's and Kotlin's
    `TaskEditorDraft` keep their types and fields and call it; Kotlin's
    `swiftDouble` is gone. The save's own shaping (trimming the title,
    normalising tags and links, the planning record) stays in the drafts,
    because it ends in the core's `save_editor`, which normalises the same
    values again; Android's `InspectorValidation` stays native as per-field
    wording under a text field. The dead Kotlin store helpers left from
    before step 4 (`moveSubtree`, `normalizedVisibleRootName`,
    `persistTaskOrder`, `taskDescendantIDs`, `encodeStringArray`) are
    deleted; `upsertKanbanColumn` waits for the board pass.

    **The Mac's cursor rules (done 2026-10-12).** Arrow-key stepping, which
    wraps on one step and stops at the ends on a page (`cursor_step`), and
    Vim's `f`/`F` over row titles (`vim_find`, case and accents folded by
    canonical decomposition, where Foundation folded them) are in
    `shaping.rs`; `CursorStepping` and `VimFind` wrap them, one call per
    key press, and their suites run unchanged.

    **The board pass (done 2026-10-13, in part).** The board, matrix and
    filter-list logic step 10 left out was audited across the Mac, the
    iPhone and Android. Moved, in `core/src/board_rules.rs`:
    `board_cross_column`, the rule that a subtask nobody filed follows its
    parent's column and one filed elsewhere is a card of its own there
    (Swift's `WorkspaceScopeShaping.board`, which the Mac and the iPhone
    share, and Android's `BoardShaping.shape` call it; rows cross as parent
    indices and column codes, and the call is skipped when no subtask is
    filed at all); `matrix_arrangement` and `matrix_quadrant`, the matrix's
    sort into quadrants with ranked cards first (`MatrixQuadrantIndex` wraps
    it for the Mac and now the iPhone, which sorted per render and now sorts
    once a load; Android's `BoardShaping.matrix` calls it); and
    `board_column_id`, a new column's id from its title
    (`WorkspaceKanbanColumn.newID` on the Mac and iPhone,
    `BoardShaping.uniqueColumnId` on Android). The filter-list rule that a
    list asking for finished work shows it whatever the toggle says is one
    Swift place, `WorkspaceFilterDefinition.hidesCompleted(whenToggled:)`, for
    the Mac's pane and the iPhone's board and outline (Android has no such
    toggle). `upsertKanbanColumn` is deleted. One answer changed: a matrix
    axis counts when it is above zero, where the Mac and the iPhone matched
    exactly 0 or 1. The phones' inspector pickers write -5 and 5, so a task
    placed there sat in no quadrant on the Mac (not even unplaced) and was
    unplaced on the iPhone, while Android, which already read signs, put it
    in its quadrant; now all three do. Measured on the 7,000-task benchmark
    (`WorkspaceSwitchingBenchmarks`, release): shaping Everything's board
    10.7 ms to 11.1 ms, the matrix index 2.9 ms to 3.3 ms; both run once a
    refresh, off the per-key path. Left as they are, with the reason. Android
    still walks Everything's trees in Kotlin (`WorkspaceListTree.actionableTasks`
    and `WorkspaceBoardTrees`) rather than calling `combined_board`: its
    outline needs every tree anyway (step 9, "The phones' screens"), so
    moving it is a rework of Android's board read, not a move. The single
    list's tree walk (`WorkspaceBoardTrees`) stays in Swift and Kotlin for the
    same reason the Mac's does. Three behaviours differ by platform and are
    product decisions rather than duplicates: a moved card carries its
    same-column subtasks on the Mac only; deleting a column that is drawn
    only because cards are filed in it refiles them to a different fallback
    on each platform; and the phones' inspectors read different column sets
    (Android the layout, the iPhone the drawn columns). The board keys and
    the stock columns' fallback id are spelled on each platform; the core
    exports `board_stock_columns` but no key builder.

    **The outline drop (done 2026-10-13).** Decided 2026-10-12: the iPhone's
    rule is the rule for both phones. `outline_drop_plan` in `shaping.rs`
    takes each drawn row's depth, parent and list, and the drag's source and
    destination, and answers with row indices: before a sibling, to the end of
    its siblings, or into the children of the parent of an anchor row, before
    that row or at the end. It reads the drop from the rows directly above and
    below where it lands; a drop inside the carried subtree changes nothing.
    Siblings share a parent and a list, which the iPhone never needed (its
    drags are within one tree) and which keeps Android's Everything from
    putting a task before another list's root as though it were a sibling.
    `OutlineDropRule` in `TaktCore` wraps it, and the iPhone's
    `OutlineDropPlanner` maps the indices back to ids; Android's
    `OutlineShaping.planDrop` calls it directly and its `ListViewModel` gained
    the reparent (a move to the anchor's list and parent, then a place before
    the anchor). One call per drop. The iPhone's `OutlineDropPlannerTests` run
    unchanged against it and are ported to `shaping/tests.rs`. What changed is
    Android's: a drop among another parent's children (`b` in front of `a2` in
    `ListsShapingTest`) now joins that parent, before `a2`, where it was
    refused with "Drag among its siblings"; more generally Android no longer
    looks past deeper rows to the nearest sibling, so a drop with deeper rows
    on both sides joins their parent, where it was refused or kept at its own
    level.

    **The keymap and the catalogue (done 2026-10-13).** Decided 2026-10-12:
    the command catalogue moves into the core and Android reads it.
    `core/src/keymap/catalogue.rs` holds every entry the Mac's
    `WorkspaceCommandCatalog+Entries.swift` held (id, title, group, keys,
    per-surface keys, surface, motion, note), converted mechanically with
    their comments and checked field for field against a dump of the Swift
    catalogue before it went; `keymap.rs` holds the keymap file's reading
    (`keymap_parse`), laying it over the catalogue (`keymap_resolve`), a key's
    spelling (`keymap_normalized_key`), the commands that read their own key,
    and taking a key apart for a toolkit (`keyboard_chord`). The Mac reads the
    catalogue once at launch into `WorkspaceCommandCatalog.defaults` and
    resolves a keymap once per load of `keymap.json`; the router still reads
    `WorkspaceKeyBindings`, a Swift dictionary built from that answer, so no
    key press crosses into the core. `WorkspaceKeymap` and the catalogue keep
    their signatures, and `WorkspaceKeymapTests` and
    `WorkspaceCommandCatalogTests` run unchanged as the oracle; a new test
    holds every core entry to a `WorkspaceCommandID` case. The iPhone's
    `KeyboardCommands.parse` takes keys apart through `WorkspaceKeyChord`.
    Android's hand-copied `KeyCatalog.kt` is now ids only: titles, groups and
    keys come from the core, and its key handling (the shell, a list screen,
    a selected row, the matrix) matches the catalogue's chords, built once,
    instead of chords written out by hand. Surfaces map as screens: the shell
    takes `anywhere` commands with a modifier, a list screen the view
    switches and list commands, a selected row the task commands, which may
    be bare keys. Translation: ⌘ is Ctrl (or Meta), ⌥ is Alt; a Mac key held
    on Control has no translation (Ctrl is already ⌘) and neither has a
    sequence. Where that leaves a command with no key, or Android has a
    convention of its own, `KeyCatalog.androidKeys` adds one (`z` fold, `f`
    focus, Alt+T plan for today, Enter open, Ctrl+R rename list, Ctrl+Y
    redo), and four commands the catalogue has no entry for (select next and
    previous, the undo history, settings) keep an Android title. What changed
    on Android, because the Mac's keys are now its keys: Today is Ctrl+T (was
    Ctrl+1), Board, Outline and Matrix are Ctrl+1, 2, 3 (were Ctrl+2, 3, 4),
    due today is Ctrl+Alt+T (was Ctrl+T), Everything also answers Ctrl+E,
    search Ctrl+Shift+F, delete Ctrl+Delete, indent and outdent also
    Alt+Right and Alt+Left,
    and a palette row's titles are the Mac's ("Show as a board"). The core's
    keymap reading differs from Foundation's in one way a person could see:
    the sentence for a file that is not JSON quotes `serde_json`'s reason.
    Measured: reading the catalogue is one call of 161 entries at launch;
    nothing per key press changed.

    **The Checkvist stack, deleted (done 2026-10-13).** Decided 2026-10-12:
    the Checkvist-era task stack is deleted rather than ported, because
    porting it would have put a retired feature into the shared core. Gone:
    `TaskRepository`, `SyncService`, `TaskMutationService` (and `+Board`),
    `UndoService`, `ReorderQueue`, `OptimisticTaskID`, `OfflineTaskSyncPlugin`,
    the three list-scoped stores, `TaskTreeFormatter`, `NavigationState`, the
    service hosts (`TaskServiceHosts.swift`, `AppCoordinator+ServiceHosts.swift`),
    the timer, quick entry, focus session, start date and recurrence managers
    that only fed them, the onboarding service, the reachability monitor, the
    integration and daily log data-source adapters, and from `TaktCore`
    `OfflineReplayPolicy`, `AuthRetryPolicy`, `RemoteKeyBootstrapPolicy`,
    `CompletionRetention`, `TimerElapsedReassignmentPolicy`, `TaskFilterEngine`,
    `DayLogPlan`, `VisibilityTask`, `TimerStore`, `RootTaskView`,
    `OnboardingResetPolicy`, `SyncStatusSummary` and `IntegrationLinkStore`,
    with their tests; the `TaktAppLogic` target and `applogic-tests` went with
    them (Package.swift, CLAUDE.md, `docs/state-ownership.md`). What had to keep
    working is the import and the Checkvist settings page. `CheckvistAccount`
    holds the account, its lists and which of them the workspace copies (all by
    default, so the activation-time import works as before; the page can now
    exclude a list and import at once); `NativeCheckvistSyncPlugin` is login,
    lists and a list's open tasks, each a core request and a core reading sent
    through `URLSession`, with the one retry after a 401 inlined where
    `AuthRetryPolicy` was. The old offline store is read once more, for its
    migration, by `LegacyOfflineTaskStore`. What the stack touched elsewhere:
    the menu bar already read the workspace; the Google Calendar page already
    acted on the workspace's selection; the MCP setup now takes the account and
    the default list from `CheckvistAccount`; Obsidian's per-task sync queue,
    task links and the AFFiNE checklist and day export had no caller left in
    the coordinator and were removed there, and then (2026-10-13) the
    Obsidian and AFFiNE plugins themselves, whose every remaining member
    served only those paths — with `core/src/affine.rs`, the AFFiNE
    markdown and helper locator in `TaktCore`, and their palette commands.
    Obsidian daily notes are the Daily Log plugin's and stay; and the day log no longer records task events, because its
    events are keyed by Checkvist id and only the Checkvist mutation path and
    timer wrote them. The settings page's list switcher, offline upload,
    overwrite and merge-lists tools went too: each wrote to Checkvist or to the
    offline store.

## Risks

- **App size and build time.** An app build already needs cargo for the
  bundled CLI, so the toolchain is not new. The core adds an xcframework
  with arm64 and x86_64 slices for macOS and for the iOS simulator and
  device.
- **iOS widgets.** The widget extension does not open the store today; it
  imports only SwiftUI, WidgetKit, AppIntents and ActivityKit. Keep it that
  way, so the extension never has to link the core.
- **Android ABIs.** Build arm64-v8a and x86_64; the user's phone is arm64.
- **FFI cost on hot reads. Measured 2026-10-09.** One call per refresh was
  not enough: each row crossing UniFFI costs about 8 to 10 µs, which is more
  than the work done on it. Moving logic into the core pays only when fewer
  rows cross. On a seeded 7,000-task workspace
  (`workspace-tests/WorkspacePerformanceBenchmarks.swift`,
  `core/examples/perf_reads.rs`):
  - The sidebar (`sidebar.rs`) walks every list in the core and returns only
    the nested lists and one count per list. It used to fetch every task. The
    sidebar's read went from 93 ms to 5 ms, and the main-thread refresh after
    one edit from 81 ms to 8 ms.
  - Next up (`next_up.rs`) reads the candidates, plans the day
    (`DayPlanSelector` now wraps `next_up::plan`) and ranks them in one
    call. The Mac and the iPhone's Today and widget keep only the first
    eight ranked tasks and the day's own tasks. The snapshot went from
    69 ms to 14 ms; without a limit, as the iPhone's focus screen asks, it
    is 34 ms.
  - Its reads use a second, `query_only` connection on the same file, so a
    main-thread read no longer waits behind it. Measured while a snapshot
    runs, the worst such read went from 4.6 ms to 1.8 ms. Writes,
    `data_version` and the journal stay on the first connection, so the
    external-change token still ignores the app's own commits.
  - The waiting chips and the planning values read only the metadata rows
    that carry them (`waiting_metadata`, `planning_metadata`): 5 ms each
    down to 0.03 ms. The Mac rereads the chips only when
    `WorkspaceStore.changeStamp()` has moved, so switching lists skips the
    reread.
  - A combined scope's board (`board.rs`), Everything's or a folder's,
    selects the actionable cards, walks every card's tree, names every
    parent and carries each drawn row's column and matrix place in one
    call. Finished rows the pane would hide outright stay in the core, and
    the tree indexes cross as packed `u32` bytes rather than one value at a
    time. The board refresh after one edit went from 92 ms to 68 ms. What
    was left was the cards themselves: the board draws all 6,594 of them,
    and as records they cost about 6 µs each to cross, some 42 ms of the
    60 ms read. `tasks_in_lists` reads its columns by position now, which
    took its own Rust time from 20 ms to 10 ms. A single list's board still
    shapes the tree the outline has already read that refresh, since
    reading it again in the core would cross the same rows twice.
  - Those 6 µs were UniFFI's Swift side, not the core: it reads a record
    field by field out of `Data`, a bounds-checked copy per integer and a
    fresh `[UInt8]` per string, and a `TaskRow` has seventeen fields, nine
    of them strings or optional strings. So the big row reads cross as one
    buffer (`packed_rows.rs`: a presence bitmask, the integers, then
    length-prefixed UTF-8), which `PackedTaskRows` in Swift decodes in one
    pass over raw memory, sharing one string for a run of rows from the
    same list. The board's `rows` cross that way, and the Mac's `listTrees`
    reads `tasks_in_lists_packed`; the record `tasks_in_lists` stays for
    anything else. The views see the same `WorkspaceTask`s; a test holds
    the packed decoding to the record path. Measured 2026-10-09: the
    combined board's read went from 62 ms to 21 ms (the crossing alone
    from 56 ms to 16 ms), the board refresh after one edit from 77 ms to
    36 ms, every list's trees from 58 ms to 17 ms, one list's from 1.4 ms
    to 0.4 ms, and the single-list refresh after one edit from 63 ms to
    21 ms. Decoding 7,000 rows in Swift takes 2 ms; what remains is the
    core's own reading and walking.
- **Two SQLite libraries in one process. Resolved 2026-10-08.** Two
  copies of SQLite sharing a file can each release the other's POSIX locks
  and corrupt it, so the core never brings a second one into an app. On
  Apple it links the system SQLite, which GRDB uses. On Android it links
  androidx's bundled SQLite: `libsqliteJni.so` exports the whole C API, and
  `scripts/build_core_android.sh` links the core against it, so the core's
  `.so` needs `libsqliteJni.so` and carries no SQLite of its own. The core
  can therefore open the file while the app holds it. Keep the core's
  androidx version and the driver's in step: both come from
  `libs.versions.toml`.
- **Captured triggers are text.** A captured migration's triggers name the
  columns its table had then. Replaying it on a database where a later
  migration already ran would install stale triggers. That cannot happen in
  order, which is the only way migrations apply, but tests that rewind one
  migration must rewind the later ones too.
- **Two writers.** The CLI and each app write the same file through their
  own core connections. The five-second busy timeout on every connection is
  what keeps that safe; do not remove it.
