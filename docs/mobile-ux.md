# The phones' UX: audit and target

The Mac is keyboard-first: one place says what's next (Today, ranked by the
core), chrome stays out of the way, every command is in the palette, capture
is one keystroke, and every edit can be undone. This page checks the iPhone
(`mobile/ios`) and Android (`mobile/android`) against that, flow by flow,
and sets the target both are built to. The two phones are meant to be the
same app in two toolkits: if one gains a gesture, the other gains the same
gesture.

Last checked against `main` on 17 October 2026.

## Audit (before)

Tap counts are from a cold start on Today, with the thumb.

| Flow | iPhone | Android | Mac |
| --- | --- | --- | --- |
| Capture for today | 1 (`+`, top right) + type, then the task goes to the Inbox and **doesn't appear on Today**. Plan for today is a switch, off by default | 1 (inline bar) + type, then the same: Inbox, not Today. There's no plan-for-today option | `⌘N` from Today plans it |
| Capture from Focus, Review or Search | 1 (`+`) | 2: there's no add button there, so first go to Today | 1 keystroke |
| Complete | 1 (tick) or swipe right | 1 (tick). Swipe right in outlines, but not on Today | `⌘↩` |
| Schedule (tomorrow) | Today: swipe left. Lists: long-press, then a menu item (2) | 2 everywhere: long-press, then a menu item | `⌘⇧T`, `b` |
| Move to a list | 3: long-press, Move…, then the list | 3: the same | `mm`, then type |
| Go to a list by name | Lists tab, scroll, tap (2+). Nothing finds a list by typing | the same | `⌘P`, then type |
| Run a command | No palette. Only `⌘` key commands with a hardware keyboard | A palette, but only with a hardware keyboard (`Ctrl+K`) | `⇧⌘P` |
| Undo | Top-right button or shake. The toast after an edit is text you can't tap | A snackbar with Undo after every edit | `⌘Z` |
| Start focus | 1 (▶ on a card) | 1 (▶ on a card) | `f` |
| Haptics | On completion | None | — |

What's wrong, beyond the counts:

- **Capture from Today is a dead end on both phones**: a task you add on
  Today goes somewhere else.
- **The palette is unreachable by touch** on both phones, so "everything
  by command" breaks as soon as there's no keyboard. Lists can only be
  reached by scrolling a tree.
- **The swipes don't match.** Swiping left on an iPhone outline row deletes
  or invalidates the task; on Android it cancels the task; on Android's
  Today it does nothing.
- **There are two ways to capture on Android Today** (an inline bar and a
  floating button), and the iPhone's add button is out of thumb reach in the
  top bar beside undo and redo.
- **The iPhone can't undo from where you are looking**: its toast can't be
  tapped, and most edits (complete, delete, due dates) give no feedback at
  all.
- **The top bars are crowded**: undo, redo and add on every iPhone root
  screen, and habit plus settings on Android Today.

## Target

1. **Find is the palette.** The Search tab becomes Find: one field that
   matches commands, lists and tasks together, so the Mac's `⇧⌘P`, `⌘P` and
   `⌘F` become one place you can reach from every screen. Its sections are:
   - Commands: the core's `phone_palette_commands()`, ranked by
     `palette_rank_commands` with recent ones lifted.
   - Lists: every list and filter list, ranked by `palette_rank`.
   - Tasks: full-text search, as before.

   With nothing typed, it shows your recent commands and your lists, so it
   doubles as a launcher. The matching is the Mac palette's own scoring,
   ported to the core (`core/src/palette.rs`), so a query lands on the same
   row on both phones. On the iPhone a hardware keyboard's palette and
   search keys open it; Android keeps its `Ctrl+K` palette as well, since
   that one also lists the selected row's commands.
2. **Capture is one tap, from the thumb, and lands where you are.** A square
   add button sits at the bottom right of Today and Lists on both phones.
   Opened from Today, quick add plans the task for today by default. The
   sheet reads the core's capture syntax (`30m #tag !1 @fri`) and shows it as
   chips. Android drops its second, inline capture bar on Today; the
   iPhone's top-bar `+` stays on iPad only.
3. **Swipes mean the same thing everywhere, on both phones.**
   - Swipe right completes, or logs a daily for today.
   - Swipe left schedules: Tomorrow on Today, Today on a list row.

   Anything rarer is in the long-press menu: move, invalidate, delete, start
   focus, habit.

   A tap opens the task's inspector, on every surface and both phones:
   Today, a list's outline, the board, the matrix, search and filter lists.
   Completing is the swipe (or the menu), never a tap, so looking at a task
   can't finish it. In search and filter lists, going to the task in its list
   is the long-press. On the iPad a tap selects, and the inspector column
   beside the list follows.
4. **Every edit can be undone from where you are.** After complete, delete,
   move, schedule and plan, a snackbar names what happened and offers Undo.
   An undo offers Redo. This is Android's existing behaviour, now on the
   iPhone too. Shake to undo stays on the iPhone.
5. **Haptics confirm a change of state**: completing, a swipe passing its
   threshold, undo. Android gains what the iPhone already does.
6. **Less chrome.** The iPhone's top bar keeps Undo, with History and Redo on
   long-press; add moves to the thumb. Android's Today bar keeps Settings
   only (New habit is in Find and the card menu). Settings is also a Find
   command on both phones.
7. **Tasks that start later follow the core.** The Mac is moving to showing
   them last among their siblings, greyed and with their start date, rather
   than hiding them. The order is the core's, so the phones pick it up when
   that lands without sorting anything themselves; drawing the start date on
   the row is the phones' only part, and is left for that change.

## Where it lives

- `core/src/palette.rs`: the phones' command list and the palette scoring
  (`phone_palette_commands`, `palette_rank_commands`, `palette_rank`).
  `Sources/TaktCore/PhonePalette.swift` wraps it for the iPhone.
- Find: Android `ui/search` (`FindModel`, `SearchScreen`) and
  `ui/commands/PhoneCommands`; iPhone `Features/Search` (`FindPalette`,
  `SearchScreen`). Each phone has a test that it runs every command the core
  lists. Android adds one of its own, New habit, until the iPhone has the
  habit form.
- Swipes: Android `ui/components/SwipeRow`; iPhone `.swipeActions` on the
  Today card and the outline row.
- Undo: Android `UndoCenter` (unchanged); iPhone `WorkspaceModel.announce`
  and `ToastView`.

## Out of scope here

Calendar on the phones, and the sync transport (both are being built
elsewhere). Restructuring Settings: it's long, but every page is reachable,
and Find now covers the common ones.
