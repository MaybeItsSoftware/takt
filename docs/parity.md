# Mac and Android parity

This table tracks what the Mac app does and where the Android app stands on
each feature. Add a row when the Mac gains a feature, and update the row when
Android catches up or the decision changes.

- **Done**: Android does it, on the same rules and the same rows, so the two
  agree after sync.
- **Not on a phone**: the feature is tied to a desktop: the keyboard, the menu
  bar, a floating window, local files, a process the phone doesn't run. Where
  Android has its own equivalent, the row names it.
- **Missing**: it would make sense on a phone and isn't built yet. The note
  says why or what is in the way.

Last checked against `main` on 6 October 2026.

## Planning

| Mac feature | Android | Notes |
| --- | --- | --- |
| Today: the day as numbered cards, estimate vs logged, "done by" finish time | Done | `ui/today` |
| Plan for today / take off, arrange the day (`Ctrl+T`, `Alt+↑/↓`) | Done | Card menu, and the same chords with a hardware keyboard |
| Board, Outline, Matrix | Done | `ui/lists` (`BoardPane`, `OutlinePane`, `MatrixPane`) |
| Everything (all active lists) | Done | The Lists tab's Everything entry |
| Nested lists, folders, list settings, archive | Done | |
| Capture syntax (`30m #tag !1 @fri`) | Done | `TaskCaptureSyntax` in `:core` |
| Repeating work (a period schedules the next occurrence) | Done | `scheduleNextOccurrence` |
| Waiting on: tag, follow-up time, follow-up task in Today | Done | Same deterministic follow-up id as the Mac (5f7d70f) |
| Task inspector (dates, estimate, priority, tags, planning, conditions, notes) | Done | `ui/inspector` |
| Delete asks first, and undo works either way | Done | Confirm dialog in lists and the inspector, plus an Undo snackbar. The Mac's "Confirm before deleting" switch is not ported: on a phone it always asks |
| Undo, redo and history | Done | Same journal and the same labels |
| Search | Done | In the Find tab, under commands and lists |
| Command palette | Done | The Find tab, by touch: commands, lists and tasks in one field, ranked by the core (`core/src/palette.rs`). `Ctrl+K` with a hardware keyboard. See [the phones' UX](mobile-ux.md) |
| Typed command language (`CommandEngine`) | Not on a phone | It has no surface on the Mac either |

## Dailies and habits

| Mac feature | Android | Notes |
| --- | --- | --- |
| Dailies: a contribution a task owes each day | Done | Ticking logs today's contribution and leaves the task open |
| Habit form (`gb` / `Cmd+Shift+H`): frequency incl. every N days, column, drop or carry, estimate, end | Done | A bottom sheet. Open it from a task's menu (create or edit), from Today's toolbar or "New habit" in the palette (standalone), or with `Ctrl+Shift+H` |
| Habit engine: place each appearance in its column, drop or carry a missed day, expire with the source task or on a date | Done | `HabitPolicy` in `:core`, with the Mac's test cases. Runs when the app comes forward, every 30 s while it is in front (which covers a new day), and after undo and redo |
| Closing a task ends the habits made from it | Done | In every close path, inside the same write, so undo brings them back |
| One Habits list on two devices | Done | Its id is derived from the workspace on both platforms (`HabitPolicy.habitsListId`) |

## Focus

| Mac feature | Android | Notes |
| --- | --- | --- |
| Focus ladder, conditions, available time, queue | Done | Focus tab |
| Stale and interrupted blocks | Done | |
| Score a block: ×1.0, nudged a tenth at a time, running total | Done | Minus and plus buttons in place of the arrow keys |
| `\` logs a block at ×0 | Done | "Doesn't count (×0)" button |
| "Focus points" setting (`scoresEachFocusBlock`): off asks nothing and hides all points | Done | Settings → Focus. Like the Mac's, it is kept per device |
| Timeline of the day's blocks, progress review | Done | Review tab |
| Floating focus panel, menu-bar clock, "run blocks in" preference | Not on a phone | Android has the ongoing notification with pause, log and done (`FocusService`), and a home-screen Next up widget |
| Global hotkeys (show window, focus panel, quick add) | Not on a phone | Android has the quick-add tile, launcher shortcuts and the widget |

## Look and feel

| Mac feature | Android | Notes |
| --- | --- | --- |
| Default theme "Takt" with Inter and Geist Mono | Done | The fonts are bundled in `res/font` and survive release shrinking. `BundledFontsTest` pins the mapping |
| Built-in themes, theme files, import, follow the device's choice | Done | Shared conformance fixtures in `shared/themes` |
| Theme gallery with miniatures | Done, as a list | Themes are listed by name, without miniatures |
| Font controls: interface, heading and numeral families, text size (cc8a82d, 8108dae) | Done | Settings → Type. `ThemeTypographyOverride` in `:core`, merged over the theme as on the Mac and kept per device. The faces are the seven bundled families; a phone cannot offer fonts installed on the system. The size scales the theme's own sizes, on top of the system font scale |
| Completion celebrations (Strike, Spark, Fold, None) | Done | |
| Celebration preview in Settings | Done | Plays on a sample row when chosen, or with Preview |

## Sync and accounts

| Mac feature | Android | Notes |
| --- | --- | --- |
| Sync through the sync server | Done | |
| Email and password, Apple | Done | |
| Google sign-in | Done, needs setup | Hidden until the build has the Web client id. See [Google sign-in on Android](android-google-sign-in.md) |
| Self-hosted server and Supabase project | Done | Settings → Sync → "Use a different server" |
| Devices list, delete account | Done | |

## Integrations and tools

| Mac feature | Android | Notes |
| --- | --- | --- |
| Checkvist import and sync | Missing | It works on the Mac, and what it imports reaches the phone through sync. A phone client would need its own Checkvist credentials |
| Google Calendar, Google Tasks | Missing | Same as Checkvist: the integration runs on the Mac |
| Obsidian daily notes | Not on a phone | Writes into a local vault folder |
| Markdown editor plugin (notes full size; a linked Obsidian note as the notes) | Missing | The notes are edited in the inspector as before. "Same as the Obsidian file" needs the vault on disk, so it stays a Mac choice; a Markdown preview of the notes would not. See `docs/markdown-editor.md` |
| Daily log files | Missing | Per-device files on the Mac. Nothing records them on Android |
| Workspace export (Markdown, JSON) | Done | Settings → Your data, through the system save dialog. `WorkspaceExport` in `:core` writes the same bytes as the Mac (fixtures from Foundation's `JSONEncoder`). Neither app imports it |
| MCP server and `takt` CLI | Not on a phone | Desktop processes |
| Diagnostics window | Not on a phone | |

## Keyboard-only

These are not on a phone. A hardware keyboard on Android gets the Mac
catalogue's chords for the commands it runs (the core's
`core/src/keymap/catalogue.rs`, read by `KeyCatalog`, ⌘ as Ctrl and ⌥ as
Alt) and the palette.

| Mac feature | Android |
| --- | --- |
| Shortcut remapping | Not on a phone |
| `?` keyboard reference palette | Not on a phone |
| `Cmd+Backspace` deletes, Return confirms | Not on a phone. Delete is a menu item with a confirm dialog |
| Double-Shift for the palette, `Cmd+1`–`Cmd+0` view keys | Not on a phone |

## Rows two devices can both make

When two devices could each make the same row before they sync, the row must
merge into one, not arrive twice. These cases are covered:

| Row | How |
| --- | --- |
| A waiting task's follow-up | The id is derived from the source task and the follow-up time (`WaitingFollowUp.followUpTaskId`) |
| The Habits list | The id is derived from the workspace (`HabitPolicy.habitsListId`) |
| A habit's column and expiry | The engine writes only the habit's own `task_metadata` and `dailies` rows, so passes on both devices converge on the same values |
| A day's contribution to a daily | Sync folds a rival with the same `(dailyId, dayKey)` into one (`resolveUniqueRival`, as in `WorkspaceStore+Sync.swift`) |

A new engine that creates rows on its own needs a row here, and the
deterministic id to back it.
