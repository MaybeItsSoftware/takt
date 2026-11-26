# Desktop keyboard shortcuts

**The list of keys lives in the app, not here.** Press `Cmd+Shift+P` (or `Cmd+K`) for the command
palette, which shows every command the workspace has and the key that runs it,
or `Cmd+/` for the same information grouped as a reference sheet. Both are
rendered from `WorkspaceCommandCatalog` (`Sources/TaktCore/`), which reads
its entries from the Rust core's `core/src/keymap/catalogue.rs`: the single
place a workspace key is written down, which the iPhone's hardware keyboard
and Android's palette and chords read too.

This file used to carry a table of its own. It is gone deliberately. There were
four copies of these facts — this table, the README's, a seventy-row literal in
the help sheet, and the `switch` in the key router that actually decided —
and nothing kept them equal. The old settings pane had the same problem: it
told people `u` was undo for months after undo moved to `Cmd+Z`. A reference
that can be wrong is worse than no reference, because it is believed.

What remains here is the behaviour a table could not express, and the format
of the file you rebind keys in.

## Zed's keys

Where Zed has a key for something Takt does, Takt uses Zed's key, so
the two apps share muscle memory. A list is treated as Zed treats a file, and
a task as Zed treats a line:

| Zed | Key | In Takt |
| --- | --- | --- |
| Command palette | `Cmd+Shift+P` | The command palette (`Cmd+K` still works) |
| File finder | `Cmd+P` | Find or create a list |
| Find in project | `Cmd+Shift+F` | Search every task (`Cmd+F` too) |
| Toggle left dock | `Cmd+B` | Show or hide the sidebar |
| Focus project panel | `Cmd+Shift+E` | Focus the sidebar |
| Toggle agent panel | `Cmd+Shift+A` | The left dock's Agent tab |
| Toggle bottom dock | `Cmd+J` | Progress, Done and Timeline, on the tab it was on |
| Toggle right dock | `Cmd+Alt+B` | The right dock (`Cmd+Ctrl+I` still works) |
| Close all docks | `Cmd+Alt+Y` | Put the sidebar, right dock and bottom dock away |
| Move line up / down | `Alt+↑` / `Alt+↓` | Move the task, or the sidebar row; on Today, a planned card through the day |
| Document start / end | `Cmd+↑` / `Cmd+↓` | First / last task, or sidebar row |
| Delete line | `Cmd+Shift+K` | Delete the task |
| New file / new directory | `Cmd+N` / `Cmd+Alt+N` | New task / new folder |
| Rename | `F2` | Rename the task, or the sidebar row |
| Undo / redo | `Cmd+Z` / `Cmd+Shift+Z` | Undo / redo |
| Settings | `Cmd+,` | Settings |

The sidebar is Zed's project panel, with a list as a file and a folder as a
directory. Its keys act on the sidebar row, never on a task behind it:

| Zed project panel | Key | In Takt's sidebar |
| --- | --- | --- |
| New file | `Cmd+N`, `%` (vim) | New list, beside the row you are on, named in its row: `Return` makes it, `Esc` leaves nothing behind |
| New directory | `Cmd+Alt+N`, `d` (vim) | New folder, beside the row you are on, named in its row the same way |
| Rename | `F2`, `Shift+R` (vim) | Rename the list or folder |
| Trash / delete | `Delete`, `Cmd+Delete`, `Shift+D` (vim) | Delete the list, folder or nested list (it asks) |
| — | `Shift+I` | Choose the row's icon: a grid of symbols walked with the arrows (or `h` `j` `k` `l`) and chosen with `Return`, or an emoji for a list, nested list or folder |
| Collapse / expand all | `Cmd+←` / `Cmd+→` | Every folder |
| Collapse / expand | `←` `h` / `→` `l` | The folder; ← and `h` go up a level from a list |
| Select parent | `-` (vim) | Up a level |
| Open | `→`, `l` | Open the row — a folder expands (and, already open, enters), a list takes the keyboard. `Return` adds a task to the list you are on, here as everywhere |
| First / last | `gg` / `Shift+G` (vim) | The sidebar's ends |
| Previous / next directory | `{` / `}` (vim) | The previous / next folder |
| Command palette | `:` (vim) | The palette |

Right-clicking a row opens Zed's project panel menu — New List, New Folder;
Rename, Icon…, the row's settings, Move Up / Down; what only that kind of row
has, Archive, Delete; Expand All / Collapse All — with each item's sidebar key
on its right. The keys are read from the keymap in force, so a rebound key is
the one shown; a two-key sequence such as `oo` cannot be a menu's key, so its
chord is shown instead.

Because the sidebar has its own `d`, `h` and `l`, it does not hold them for
the task sequence that begins with one (`dd`): a letter a surface
binds on its own runs at once there. And Shift on a letter is a key of its
own — `Shift+D` is not `d` — though a `Shift` letter nothing binds still runs
the plain letter, so `Shift+X` remains the way to skip a sequence's wait.

Taking those keys moved what Takt had on them: archiving a list is
`Cmd+Alt+A` (was `Cmd+Shift+A`), promoting a nested list `Cmd+Alt+P` (was
`Cmd+Shift+P`), renaming a task by chord `Cmd+Ctrl+E` (was `Cmd+Shift+E`), and
moving a task or sidebar row `Alt+↑`/`Alt+↓` (was `Cmd+↑`/`Cmd+↓`).

Zed's two-chord sequences, such as `Cmd+K Cmd+S` for the keymap, are not
supported: `Cmd+K` is the palette here. **Open the keymap file** is in the
palette instead. Zed keys with nothing to act on in a task app — go to line,
multi-cursor, toggle comment, the terminal — are left unbound.

## Your own keys: `keymap.json`

The window's keys can be rebound in
`~/Library/Application Support/Takt/keymap.json`. **Open the keymap
file** in the command palette creates it (as `[]`, an empty keymap) and opens
it in your editor. It is re-read when you save it and whenever Takt comes
to the front; **Reload the keymap** forces it. The palette, the reference, the
tooltips and the menu bar all show the keys in force, so a rebound key's key
cap moves with it.

The shape is Zed's: an array of blocks, each with an optional `context` and a
`bindings` object from key to command id. `null` unbinds.

```json
[
  { "bindings": { "cmd+shift+k": "taskComplete", "gg": null } },
  { "context": "board", "bindings": { "x": "taskDelete", "delete": null } }
]
```

- **Command ids** are the cases of `WorkspaceCommandID`
  (`Sources/TaktCore/WorkspaceCommandID.swift`) — `taskComplete`,
  `goBoard`, `listRename` and so on.
- **Keys** are written the way the catalogue writes them: modifiers `cmd`,
  `ctrl`, `option`, `shift` joined to the key with `+` (Zed's `-` works too,
  and so do `command`, `alt`, `return`, `esc` and `backspace`). Named keys are
  `enter`, `escape`, `tab`, `space`, `delete`, `up`/`down`/`left`/`right`,
  `home`, `end`, `pageup`, `pagedown`, `f2` and `comma`. Two plain letters,
  such as `gt`, make a two-letter sequence. Shift on a lone character is
  already in the character: write `?`, not `shift+/`. A letter is the
  exception — `shift+d` is a key of its own, as in Zed.
- **Without a `context`** a binding applies wherever the command does. The key
  is added to the command's own keys — the defaults stay — and taken from any
  other command that had it on the same surface. A `null` there removes the
  key from every command.
- **With a `context`** it applies on that surface only: `today`, `board`,
  `outline`, `matrix`, `sidebar`, `inspector`, `timeline`, `done` or
  `conditions`. A `null`
  there stops the key meaning anything on that surface unless the same file
  gives it a new job there. A command that belongs to one surface — the
  timeline's, the done rail's, the Conditions tab's — can only be bound in its own context. The
  retired focus pane's contexts (`focus`, `focusRunning`) and command ids are
  reported and skipped rather than failing the file.
- Within a block the `null`s apply first, so unbinding a key and binding it
  again can be written in either order. Blocks apply top to bottom.

Mistakes are reported, never fatal. An unknown command id, a key no key press
can produce, an unknown context, or one key bound twice on the same surface is
a line in **Diagnostics → Recent problems** and a message on the window; the
rest of the file still applies, and a file that is not JSON at all leaves the
defaults in force. The few commands that act on *which* key ran them — the
priority digits, the direction keys, `⌥1`–`⌥4` on the matrix — cannot be
bound to another key, though their keys can be unbound.

The three global hotkeys (show the window, the focus panel, quick add) work
outside the window and are set in **Settings → Keyboard** instead.

## Two-letter sequences

The desktop uses [Checkvist's shortcuts](https://checkvist.com/help) for
matching actions: type two letters in succession while navigating tasks or the
sidebar. Text fields retain their normal macOS editing shortcuts, so a sequence
never fires while you are typing into one.

Which sequences exist is read from the catalogue per surface — there is no
second list. A letter is held only where some sequence on the current surface
begins with it, and a held letter is never lost: if the next key does not
complete a sequence, the held letter runs its own binding first and the new
key is then handled as usual (it may start a sequence of its own); if no key
follows within 1.2 seconds, the held letter runs then; holding Shift skips
the wait. The same goes for a minus and a digit: `-2` to `-4` set the
priorities behind the back, so `-` on its own waits a moment before sending a
task to the back. No other task key is held now: `x` begins no sequence
(extracting a branch is `ex`, not Checkvist's `xx`), so it completes at once.
A hold is dropped without running when focus
moves or text editing begins, because the key no longer means what it did.

The chords that are how you leave a text field — the view and region keys,
the creation chords, the palette, search, `Cmd+/`, and the agent panel's toggle for the agent
panel — work from inside one; the set is `reachableFromTextField` in the
catalogue. `Cmd+Z` deliberately is not — inside a field it belongs to the text
you are typing, not to the workspace behind it.

The agent panel counts as a field as a whole while it holds the keyboard, its
approval cards included: Return on a focused card approves that card, and must
not fall through to open the task selected behind it. See
[`agent-panel.md`](agent-panel.md).

## Surfaces own the keyboard

The timeline is a tab in the bottom dock (`Cmd+9`), and while it has the
keyboard it owns it outright. A key the screen does not answer to does not fall
through to the task surface behind it — `Delete`, a digit or `i` there used to
act on a task nobody could see. The only workspace keys that stay live are the
window's own (`reachableFromFullPaneScreens` in the catalogue): the view keys,
which leave the screen for the place you asked for, the palette, the
reference, search, undo and redo, the sidebar, agent, inspector and
done-rail toggles, and the region keys (`Ctrl+Tab`, `Ctrl+1`–`Ctrl+3`), since
the timeline sits beside the work rather than over it. Chords the catalogue does not know, such as `Cmd+Q` and `Cmd+W`, still
reach the app. The palette knows this: it labels each command with the surface
it belongs to and sorts the ones that apply where you are to the top.

The sidebar, the done rail, the Conditions tab and the inspector hold a cursor
or controls of their own, so the bare task keys — `Space`, `x`, `Delete`, the
digits, `Tab` — do not reach through them to the selected task. They keep the
chords, `/`, `?`, `i` and `Esc`, and all but the inspector keep the two-letter
sequences. The inspector's own keys take some of those over: it is a vim
buffer of rows, where `j` / `k`, `gg` / `Shift+G` move the cursor, `Return`,
`i` or `e` edit the row, `x` or `Delete` clear it and `h` or `←` leave; a field
is insert mode, left with `Esc` (and `Return` in a one-line field). See the
README.

The Conditions tab's cursor walks both its halves — what is available now, and
the conditions themselves. `Space` turns the condition on or off, `Return` or
`F2` renames it in its row (on the new-condition row, puts the caret in its
field, where `Return` makes it), `a` or `Cmd+N` goes to that row, `t` files
the condition under a category (Location makes it a place), `p` ticks what it
provides, `e` archives or restores it, `c` clears what is
available and `Shift+U` uses the last context again. In a name field `Return`
saves and `Esc` gives up, both handing the keyboard back to the cursor.

The right dock's two tabs — the inspector and the conditions — and the bottom
dock's done rail and timeline are changed with `Cmd+Shift+{` / `Cmd+Shift+}`,
Zed's previous / next tab, and with `[` / `]` where no field would take them;
the keyboard comes along to the new tab, within the same dock. The bottom
dock's Progress tab has nothing to step through, so it is reached by clicking
its tab and is not a stop. On a task pane the same chords change list. `Esc` on
any of them hands the keyboard back to the work and leaves the dock open — in
the inspector, from the row cursor; `Esc` in one of its fields only ends the
edit — and `r` puts
the right dock away, `Cmd+J` the bottom one. `Ctrl+Tab` walks the sidebar, the
tasks, the bottom dock and the right dock, left to right.

## Vim's find

`f` then a letter moves to the next task whose title starts with that letter
(case, accents and leading punctuation ignored), `Shift+F` then a letter to
the previous one, and `;` repeats the last find. It wraps round the ends. On
the board it walks the column you are in; in the sidebar it finds a list or
folder, which makes `f` and a letter the quickest way to a list you can see.
`f` owns the next key outright, so a letter typed after it never runs a
command of its own; `Esc` lets it go.

With it come vim's `gg` / `Shift+G` for the first and last task, `Ctrl+D` /
`Ctrl+U` to move eight down or up, and `gx` to open the task's first link.
Starting a focus block, which `f` used to be, is `gf`.

## Return and Shift-Return

They mean the same thing on every task surface:

- **Return always adds a task**, as in Checkvist — below the selected task
  (on the board, below the card a selected subtask is drawn on), at the foot
  of the day on Today — and so does
  `Cmd+N`. `Shift+Return` and `Option+Return` add one above the selection,
  `Option+Shift+Return` a subtask. Return is never "open": with nothing
  selected it still adds, so it is never dead.
- **Space and `x` tick the task off** (or reopen it); `Shift+Space` cancels it
  instead — it stopped mattering, rather than got done. On Today, starting a
  card — a task on the day *is* a block to run — is `gf`, and `gf` again on
  the running card finishes it. From anywhere else, a field included,
  `Cmd+Shift+Return` finishes the running block and `Cmd+Shift+.` stops it.
- **`→` and `]` open the task** — its subtasks as a board of their own
  on the board and the matrix; in the outline `→` folds and unfolds instead
  (below). `←` and `[` leave again.

The outline used to add a task on Return and a subtask on Shift-Return, while
Today started and ticked. One key meaning two things depending on the pane is
exactly what a keyboard-first app cannot afford, and Today's meaning is the one
a Blitzit-style day is built on.

The board keeps Left/Right for column navigation, including empty columns.
Up/Down walk a column card by card. `l` steps into the card you are on: the
cursor goes onto its first subtask row (a folded card unfolds first), Up/Down
then walk the rows the card shows, and everything that acts on a task —
Space, `ee`, the inspector, `za`, `,`/`.` — acts on the row you are on. `h`
or Esc steps back out onto the card. A subtask filed in another column is
skipped inside the card; it is reached as a card in its own column.
Left/Right from a subtask leave from the column its card is in. On an entered card's board the columns left
of the card's own are hidden, and neither the arrows nor `Cmd+←`/`Cmd+→`
reach them.

On the board, Return on a card adds a card below it, and on a subtask row
adds a sibling subtask below that row on the same card (Shift+Return above).
A subtask filed in another column than its parent's is a card of its own
there, and Return on that card adds a plain card to the column, at the foot
of its plain cards, rather than another subtask of the parent.
`ee`/F2 edits a card's or subtask row's title in place, as in the outline.
`Alt+Up`/`Alt+Down` move a card past the card above or below in its column,
and a subtask row past the row above or below on its card.

Board columns are edited from the header's right-click menu, which shows
each item's key: `Cmd+Shift+C` adds a column and `Cmd+Ctrl+C` deletes the
one you are in, asking where its tasks go. Rename, move left/right, add
left/right, edit rules, make these the default columns and reset to the
default are keyless palette commands acting on the column you are in.

## Folding subtasks where they are

Opening a task with Return makes its subtasks the whole pane, which is the
wrong move when you only want to look inside it. So, as in Checkvist, a task's
subtasks fold and unfold where they stand:

- **In the outline, `→` and `←` fold.** `→` on a folded task shows its
  subtasks, and on an open one steps onto the first of them. `←` on an open
  task folds it, and on anything else steps up to the task it hangs from; at
  the top level it leaves, as `←` does everywhere else. On a task with no
  subtasks `→` goes to the inspector instead; `l` and `]` still open it, and
  Return adds a task below it.
- **`Cmd+←` / `Cmd+→` fold and unfold the whole outline**, as they do every
  folder in the sidebar. Folding leaves the selection on the top-level task
  it was inside.
- **`za` (vim's fold toggle) folds the task you are on** in the outline, the
  board card, or the subtask row on a card — where `←`/`→` belong to the
  columns. Every row with subtasks carries a chevron that does the same.
- **`,` folds and `.` unfolds**, on the board as in the outline: `,` on a
  card hides its subtask rows, leaving "▸ N subtasks" on the card, and on a
  subtask row with none of its own folds the task it sits under, the cursor
  going with it; `.` shows them again. On a task or card already open,
  `.` opens the next layer of subtasks under it, a layer a press, and `,`
  shuts the deepest open layer first, so the task itself folds last.

There is one set of folds, shared by the outline and the board, and it
survives a relaunch. Going to a task from search or the done rail unfolds the
branches it is in.
Escape cancels task entry and returns to navigation.

## Planning the day

`Ctrl+T` puts the selected task on today — the Today board column — or takes
it off, from any task surface, and the status bar says which it did. It is not
`td` / `Cmd+Alt+T`, which set the due date to today (`Cmd+T` itself goes to
Today): a date is a fact about the task, a plan is a choice about the day, and
Today treats them differently.

On Today, `Alt+↑` / `Alt+↓` stop meaning "move among siblings", because the
day's cards come from every list and are siblings of nothing. They move the
card through the planned part of the day instead, and the whole planned order
is written at once (one undo step, "Reorder Today"), so it holds when some
other task's urgency changes. Only planned cards move. An overdue, due-today
or starting-today card is in the day because of its date and stays where the
date puts it; pressing `Alt+↓` on one leaves it and tells you to plan it first
with `Ctrl+T`, after which it is yours to arrange. The running block heads the
day whatever it is, and does not move either.

## Typing a task

Whatever adds a task reads the end of what you typed: `30m` or `1h30m` sets
the estimate, `@fri`, `@tomorrow`, `@3d` or `@2026-10-02` the due day, `#work`
a tag and `!1`–`!4` the priority (1 the most urgent). So `Write notes 45m #work @fri !1` is one
Return, not a Return and four edits. Only trailing words count, and the first
one from the end that is not a token stops the reading, so `Read 30 pages`
keeps its `30`; the first word is always the title's. The title bar's field
and the focus panel's "Add … to today" row show what they found as chips
before you press Return. The full table is in the README, under
[Typing a task](../README.md#typing-a-task).

## Undo

Undo is available in Edit and in the sidebar's history menu, which names the action being undone/redone. History persists across launches and retains the latest 100 complete actions. Creating a board card, assigning its column and placing it at the top undo together; card placement and column removal also undo as complete actions. Undo/redo reveals restored work and clears selections/scopes that no longer exist. Saved edits are undoable; unsaved inspector drafts remain drafts. Completing a focus task can reopen with undo while its elapsed time and points remain recorded. Timer records stay outside edit history.
