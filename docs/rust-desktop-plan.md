# The desktop app in Rust

The Mac app is to be Rust from end to end: `desktop/` (`takt-desktop`), drawn
with GPUI, Zed's own UI framework, linking `takt-core` as a Rust crate. It
is built beside the Swift app under `Takt/` until it does everything that
app does, and then `install_local.sh` installs it instead. The Swift app is
not deleted: it stays in the tree and keeps building. The iPhone and Android apps stay native (GPUI has no mobile
target) and keep calling the core through UniFFI.

## Why

Measured on 2026-11-06 (release, the seeded 7,000-task workspace,
`TAKT_PERF=1 swift test -c release --filter WorkspacePerformanceBenchmarks`):

| | Swift app, main thread |
|---|---:|
| switching to a list already resident | 0.13 ms |
| the write itself (`updateTask(title)`) | 0.22 ms |
| the refresh after one edit, in a list | 24.9 ms |
| the refresh after one edit, in Everything | 60.9 ms |

Switching was already fast. What misses frames is what happens after a
write: the Swift view model reads the scope again, reshapes it and rebuilds
every index and the task cache, which is 100 to 300 times the write. SwiftUI's
own layout and diffing come on top and were never measured. Moving that
layer into Rust is the opportunity to stop rereading rather than to reread
faster.

## The rules the new app keeps

1. **A write patches what is held.** A write is one core call, then a change
   to the rows it touched. The scope is read again only when another process
   wrote (`data_version` moved), and then off the main thread, compared, and
   swapped in if it differs.
2. **Nothing reads the database while drawing.** A render reads the model in
   memory. Lists are `uniform_list`, so only the rows on screen are built.
3. **Policy lives in the core.** Anything a phone also needs (shaping, ranking,
   the keymap catalogue, the palette, the editor's drafts, themes) is in
   `core/`, called directly. `desktop/` holds what is the Mac's alone: views,
   focus, windows, the platform glue.
4. **It looks like Zed.** Zed's default themes (One Dark, One Light) as role
   tokens in `desktop/src/theme.rs`, its interface face (IBM Plex Sans) and
   monospace (Lilex), its title bar, docks, tab bar and status bar, flat
   rows whose selection is a fill with an accent border while the pane has
   the keyboard. Not the Swift app's look, and not a house style.
5. **Every key is a GPUI action bound from the catalogue** in
   `core/src/keymap/catalogue.rs`, so the README stays the reference.

## Stages

Each stage ships green on its own and adds to `takt-desktop`; the Swift app
keeps working throughout.

0. **Spike — done.** The sidebar's lists and a list's outline, `j`/`k`/`h`/`l`,
   `x` ticks a task off by patching one row. Against a copy of a real
   workspace (29 lists): a list switch 0.4 ms median, a tick 2.5–12 ms (all
   of it the core's `set_status` reaching the disk), first frame about 90 ms after launch.
   `TAKT_DESKTOP_SMOKE=1` checks this without anyone at the screen.
1. **Shell — mostly done.** The sidebar from `core::sidebar` (Today,
   Everything, the inbox, folders opening and closing, nested and pinned
   lists, filter lists), its cursor, the mode strip, the right dock, the
   status bar; where you were, the folds and the docks remembered in
   `Application Support/Takt/desktop.json` (the Swift app's preferences are
   left alone). Every key goes through the catalogue (`desktop/src/keys.rs`
   ports the Swift dispatch rules: which surfaces inherit which keys, the
   two-key sequences with their 1.2 s hold, chords that reach out of a text
   field). The command palette, the list navigator, search and the keyboard
   reference are overlays ranked by `core::palette`. Left: the theme
   switcher and the user keymap file.
2. **Editing the outline — mostly done.** A one-line text field
   (`desktop/src/text_field.rs`; no IME yet); new tasks below, above and as
   children with Tab and ⇧Tab in the draft; rename (select-all, insert,
   append); complete, invalidate, delete with Return to confirm; indent,
   outdent, move up and down, to the next or previous list; due today,
   tomorrow and cleared; undo and redo through the core's journal; folding;
   `f`/`F`/`;` find; other processes' writes picked up within a second by
   `data_version`. A title or status change patches the rows held; a move
   reads the one list again (about half a millisecond).
3. **The other views — mostly done.** The board (columns from
   `core::board_columns`, rule columns included, cards moved with ⌘←/⌘→),
   the matrix (⌥1–⌥4 to place), Everything and folders, filter lists, and
   Today (`core::next_up` ranked on a background thread). Left: dragging,
   column editing, and the board's cross-column subtasks.
4. **The inspector.** Read-only now: title, list, status, due, estimate,
   priority, tags, recurrence and notes. Left: editing it through `core::editor` drafts, conditions, dependencies, links,
   recurrence, reminders.
5. **Focus and the day.** Focus sessions and the focus panel, timeline, done
   rail, dailies, day view, habits, celebrations.
6. **The platform.** Menu bar status item and its menu, global hotkeys, the
   quick-capture panel, notifications for reminders, keychain, sync
   (`docs/sync.md`), Google sign-in and the calendar and tasks mirrors, the
   Checkvist import, the agent pane, settings, and `--mcp-server`, which
   `exec`s the bundled `takt` as now.
7. **Packaging.** An `.app` bundle with `Contents/Helpers/takt`, signing and
   notarisation as `docs/signing.md` describes, shaders precompiled (drop the
   `runtime_shaders` feature), the legacy-name migration, the DMG.
8. **Cutover.** `install_local.sh` installs the Rust app. The Swift app and
   its package targets stay in the tree and keep building; `TaktWorkspace`
   and the bindings are the iPhone's in any case.

## Risks

- **GPUI is pre-1.0** and its API moves between releases. It is pinned
  (`0.2.2`) and upgraded deliberately.
- **Accessibility.** GPUI exposes far less to VoiceOver than AppKit does.
- **No ready-made widgets.** Text input, menus, the status item and the
  global hotkeys are built here, the latter two on `objc2`.
- **Disk.** GPUI's build output sits beside `core/target`'s 6 GB.
