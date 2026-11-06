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
1. **Shell.** Window state persisted; left dock (sidebar with folders and
   nested lists from `core::sidebar`), right dock, status bar, mode strip;
   the theme switcher, with Zed's themes; the keymap from the catalogue, with two-key
   sequences; the command palette (`core::palette`) as the first overlay.
2. **Editing the outline.** A text input (GPUI ships none; build on its
   `input.rs` example, with IME), add, rename, indent, move, delete, undo and
   redo through the core's journal; folding; external writes by polling
   `data_version`; the write-returns-what-changed patch for every write kind.
3. **The other views.** Board and matrix, Everything and folders, Today and
   next up (`core::next_up`, re-ranked only for what a write touched), search,
   filter lists.
4. **The inspector.** `core::editor` drafts, conditions, dependencies, links,
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
