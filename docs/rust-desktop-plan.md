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
1. **Shell — done.** The sidebar from `core::sidebar` (Today,
   Everything, the inbox, folders, nested and pinned lists, filter lists),
   its cursor, the mode strip, the docks, the status bar; where you were,
   the folds and the docks remembered in
   `Application Support/Takt/desktop.json` (the Swift app's preferences are
   left alone). Every key goes through the catalogue (`desktop/src/keys.rs`),
   and `keymap.json` overrides it. The command palette, list navigator,
   search, keyboard reference, pickers, settings and diagnostics are
   overlays ranked by `core::palette`. Themes come from the core and the
   themes folder. Right-click menus on tasks, sidebar rows and panes are
   made of catalogue commands (`state/context_menu.rs`).
2. **Editing the outline — done.** A text field with IME
   (`desktop/src/text_field.rs`); the add row and quick capture read a typed
   title's trailing words through `core::capture` (`45m #tag @fri !1`);
   every edit command, undo and redo through the core's journal, folding,
   find, and other processes' writes picked up within a second. Tasks drag
   to reorder, into sidebar lists, board columns and matrix quadrants
   (`state/drag.rs`).
3. **The other views — done.** The board (rule columns, column editing,
   cards moved by key or by drag, each card drawing its subtree with a
   subtask filed in another column named by that column, and "↳ parent" on
   a card whose parent is elsewhere), the matrix, Everything, folders,
   filter lists, Today. The board's connecting *lines* are not drawn: each
   end names the other in words instead.
4. **The inspector — done.** Every field through `core::editor` drafts,
   conditions, dependencies, links, recurrence, commitments and habits as
   typed sheets.
5. **Focus and the day — done.** Sessions, the "how did it go?" prompt,
   breaks, the queue, the floating focus panel, the timeline, the done
   rail, progress, habits and commitments, celebrations.
6. **The platform — done.** The menu bar item and its menu, Carbon global
   hotkeys (recorded in Settings), the quick-capture window, notifications
   through `UNUserNotificationCenter` with reminders from `core::reminders`,
   open at login (`SMAppService`), the keychain, sync with Supabase sign-in
   (`desktop/src/sync/`), the Checkvist import, Google sign-in with the
   Calendar two-way pass and the Tasks mirror (`desktop/src/google/`, the
   Swift app's ledger file kept), the agent panel and quick ask (`aa`), the
   MCP client installer, and `--mcp-server`, which `exec`s the bundled
   `takt`. User plugins are not ported: the Swift app only installs their
   manifests and stores their settings, and runs nothing.
7. **Packaging — done but for shaders.** `scripts/install_desktop.sh`
   builds `/Applications/Takt Desktop.app` with `Contents/Helpers/takt`,
   the icon and the `takt://` scheme; `scripts/build_desktop_dmg.sh` makes
   a Developer ID DMG, notarised and stapled. The first launch makes the
   workspace and carries forward the old `Priority` and `Bar Tasker`
   folders. Left: `runtime_shaders` stays until the Metal toolchain is
   installed (`xcodebuild -downloadComponent MetalToolchain`).
8. **Cutover — waiting on a decision.** `install_local.sh` still installs
   the Swift app. Switching it means the Rust bundle installing as
   `Takt.app` in its place; whether it also takes the Swift bundle id
   (`uk.co.maybeitssoftware.takt`) decides whether macOS treats it as the
   same app for notifications and the login item. The Swift app and its package
   targets stay in the tree and keep building; `TaktWorkspace` and the
   bindings are the iPhone's in any case.

## Risks

- **GPUI is pre-1.0** and its API moves between releases. It is pinned
  (`0.2.2`) and upgraded deliberately.
- **Accessibility.** GPUI exposes far less to VoiceOver than AppKit does.
- **No ready-made widgets.** Text input, menus, the status item and the
  global hotkeys are built here, the latter two on `objc` and Carbon.
- **Two apps, one database.** Until the cutover, both apps can run at once
  against the same workspace. Writes are safe (the core serialises them and
  each sees the other's through `data_version`), but signed in to sync in
  both they would each push the one outbox; sign in in one of them.
- **Disk.** GPUI's build output sits beside `core/target`'s 6 GB.
