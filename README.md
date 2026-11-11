<img src="Takt/Assets.xcassets/AppIcon.appiconset/ios-1024.png" alt="" width="72" align="left" />

# Takt

**Plan the day, then do it.** Takt is the gold standard of easy-to-use open source software: a task manager for Mac, iPhone, iPad, Android, Wear OS and the terminal.
A ranked Today list, an outline, a board, focus timers, dailies, and a command line that reaches the same data.

<br clear="left" />

---

| | |
| --- | --- |
| **Source** | [github.com/MaybeItsSoftware/takt](https://github.com/MaybeItsSoftware/takt) — open source under the [GNU AGPL-3.0](LICENSE) |
| **Open source** | Every app, the CLI and the sync server, under the [GNU AGPL-3.0](LICENSE) — no ads, no tracking. The apps are free and work offline with no account |
| **Sync** | [Host it yourself](docs/self-hosting.md) with the server in this repo, free and with every feature, or use Takt's hosted server for £1 a year (Takt Sync, bought in the iPhone or Android app; one purchase covers every device) |
| **Platforms** | macOS 15.6+, iPhone and iPad, Android and Wear OS, and a terminal CLI |
| **For AI agents** | [`llms.txt`](site/llms.txt) and an [MCP server](docs/mcp-server.md) over the whole workspace |
| **Website** | [maybeitssoftware.github.io/takt](https://maybeitssoftware.github.io/takt/) |

On the Mac, Takt opens as a desktop window, keeps a menu bar surface for quick capture, and is built to be driven without the mouse. Its own local workspace owns your tasks; Checkvist is an optional import. Takt adds the things a plain list has no representation for — priority ranking, start dates, recurrence, focus sessions, daily habits, and a record of what actually happened each day.

It works offline, with no account. It works from the terminal. And it exposes the whole surface to an AI assistant over MCP.

## Contents

- [Install](#install) · [First run](#first-run)
- [Keyboard flow](#keyboard-flow) · [Focus](#focus) · [Command palette](#command-palette)
- [Views](#views) · [Waiting on](#waiting-on) · [Blocked by](#blocked-by) · [Shared lists](#shared-lists) · [Quick ask](#quick-ask) · [The menu bar](#the-menu-bar) · [The window](#the-window) · [Themes](#themes) · [Diagnostics](#diagnostics)
- [Daily log](#daily-log) · [Obsidian daily notes](#obsidian-daily-notes) · [AFFiNE](#affine)
- [Command line](#command-line) · [MCP server](#mcp-server) · [Plugins](#plugins)
- [Build from source](#build-from-source) · [Where your data lives](#where-your-data-lives)

## Install

1. Download the latest `.dmg` from [Releases](https://github.com/MaybeItsSoftware/takt/releases).
2. Drag `Takt.app` into `Applications`.
3. Right-click it once and choose **Open**.

The build is signed with a development certificate rather than a Developer ID, so Gatekeeper will ask the first time. If it refuses outright:

```bash
xattr -cr /Applications/Takt.app
```

Or build it yourself — see [Build from source](#build-from-source).

## First run

Open Settings with `Cmd+,`. It is one window with a sidebar of pages — type
to filter them, walk them with `Tab`, or jump with a key, shown beside each
entry: `⌘1`–`⌘7` the categories, `⌥⌘1`–`⌥⌘6` the plugins' pages, and
`⇧⌥⌘1`–`⇧⌥⌘6` a plugin's switch, on or off. `↑`/`↓` move between
settings, and `Return` or `Space` flips a switch or opens a choice (`Space`
only while nothing is typed in the filter; after that it is a space in it):

| Page | What |
| --- | --- |
| General | Launch at login, confirming deletes, and reminders |
| Focus | Where a started block runs, and whether each block is scored |
| Appearance | The theme gallery, light/dark/system, the interface, heading and numeral fonts, the text size, and the completion celebration (its switch, its style and its sound) |
| Keyboard | The two global hotkeys (show the window, and the focus panel `⌃⌥⇧⌘F`, which also adds tasks — see *The focus panel*), the list Quick Add captures into; the shortcut rows show only while global hotkeys are on |
| AI | **AI features** (on by default; off hides the agent panel, quick ask and MCP, and sends nothing to an AI), and MCP: adding Takt's server to the assistants on this Mac |
| Plugins | A heading in the sidebar with a page under it for each built-in plugin — Google Calendar, Google Tasks, Google Drive, Checkvist, Obsidian, the Markdown editor — each with its switch beside its name. The Google plugins fold under a **Google** entry (its chevron folds them away) with a switch of its own: off, every Google plugin stops, and each keeps its own switch for when Google is on again. Its page holds that switch and the Google sign-in (**Connect Google Calendar, Tasks & Drive**, which is not your Takt account); each plugin's own switch is what turns its mirroring or linking on, with nothing further to switch inside it. Google signs in through Takt's own client; **Use your own Google client** (off by default) asks for a client ID from a Cloud project of yours and only then shows its ID and secret rows, and turning it off forgets them and signs Google out. A plugin's page holds its switch and, while it is on, its settings. Off, a plugin's settings, link-picker rows and background work go |
| Sync | The account that keeps the Mac and phones on one workspace, drawn as an account screen: choose **Takt Sync** or **Your own server** ([self-hosting](docs/self-hosting.md); it asks for the address), then sign in with Apple, Google or email, or create an account. Signed in, the card shows who you are, when it last synced, your devices and Takt Sync, with Sync now, the password, Sign out and Delete account. Changing server signs you out |
| Advanced | Export the workspace as Markdown or JSON, diagnostics, the app's data folder, `keymap.json` (open, reload), and the themes folder (open, reload, export the theme as JSON) |

None of it is required: the workspace is local-first and works with every row
left alone. Checkvist, Google Calendar and the Obsidian daily notes (Daily Log)
are set up under **Plugins**, when you want them.

## Keyboard flow

The window is keyboard-first: every mode, every surface and every task action
has a key. The mode strip names the modes, and the View, Task and Workspace
menus list what you can do, with its chord beside each item that has one. A
menu can only show a chord, so an item whose only key is a two-letter
sequence (`ww`, `cc`, `bu`, `ex`…) is listed without its key; the table below
and [`docs/keyboard-shortcuts.md`](docs/keyboard-shortcuts.md) have every one.
So none of it is reachable only by something you already have to know.

| Key | Action |
| --- | --- |
| `Cmd+1` / `Cmd+2` / `Cmd+3` | Show the list as a board / outline / matrix |
| `Cmd+T` | Today |
| `Cmd+8` / `Cmd+9` | The focus panel / the timeline, a tab in the bottom dock |
| `Cmd+0` / `Cmd+E` / `gh` | Everything, across all active lists |
| `Cmd+I` | The Inbox |
| `Ctrl+1` / `Ctrl+2` / `Ctrl+3` | Focus the sidebar, the task surface, the inspector |
| `Cmd+B` | The left dock (Zed's), on its tab — the sidebar or the agent — with the keyboard in it; up already, it takes the keyboard; from inside it, it goes away. Its tabs' own keys (`Cmd+Shift+A` for the agent) go to one tab. Both tabs are the same width |
| `j` / `k` / `↑` / `↓` | Move the selection |
| `Cmd+↑` / `Cmd+↓` | First / last task |
| `Alt+↑` / `Alt+↓` | Move the task up / down (Zed's move line) — on Today, earlier or later in the day |
| `h` `j` `k` `l` / arrows, `v`, `Tab` | On the matrix, move to the nearest task that way; `v` is move mode, where they nudge the selected task across its axes (`Shift` for a fine nudge); `Tab` / `Shift+Tab` step between tasks — see [The matrix](#the-matrix) |
| `Alt+←` / `Alt+→` | Outdent / indent the task — out of, or under, the task above |
| `Cmd+←` / `Cmd+→`, or `Shift+Alt+←` / `Shift+Alt+→` | Move the card, or the subtask row you are on, a board column left / right — a subtask moved past its parent's column becomes a card of its own there (in the outline `Cmd+←` / `Cmd+→` still fold and unfold everything) |
| `Cmd+Shift+←` / `Cmd+Shift+→` | Move the board column you are in left / right, taking its cards with it |
| `go` / `Cmd+Shift+O` | Go to the task's list: leave Today, a filter list, Everything or a folder for the list the task belongs to, with it selected. In the focus panel it opens the window there; on the phones it is **Go to list** in the task's menu |
| `Cmd+Alt+↑` / `Cmd+Alt+↓`, or `Cmd+Shift+{` / `Cmd+Shift+}` | The list above / below in the sidebar — Zed's previous / next tab (in the sidebar itself `Cmd+Alt+↑` / `↓` still move a folder; `Cmd+Alt+←` / `→` outdent and indent the task, as `Alt+←` / `→` do) |
| `Ctrl+-` / `Ctrl+Shift+-` | Back / forward through the lists you have been in — Zed's go back / go forward |
| `Shift+Alt+↑` / `Shift+Alt+↓` | Move the task to the list above / below in the sidebar (`mm` picks any list; type, and below the lists it offers every open task that matches, from any list, to file the task under as its last subtask — one undo step) |
| `Ctrl+T` | Plan the task for today, or take it off |
| `gb` / `Cmd+Shift+H` | Make a habit from the task, or edit the habit it is — see [Habits](#habits) |
| `ww` | Waiting on: who or what, and when to follow up — moves the task to Waiting on; see [Waiting on](#waiting-on) |
| `bb` / `Cmd+Shift+B` | Blocked by: find a task this one waits on, or type a new one and `Cmd+Return` to make it just above; see [Blocked by](#blocked-by) |
| `cc` | Conditions: tick what the task needs (Home, Floor space…), or type a new one — `Space` or `Return` ticks, `Esc` closes; with no task selected, the Conditions tab. See [Conditions](#conditions) |
| `ex` | Extract the branch as a list: the task and everything under it leave for a list of their own in the sidebar |
| `aa` | Ask the agent: a one-line popup over the task — "due friday", "split it into steps" — that does it there, each change a card with Undo and the keys that would have done the same; see [Quick ask](#quick-ask) |
| `Delete` / `Cmd+Delete` / `Cmd+Shift+K` | Delete the task — it asks, and `Return` confirms |
| `Return` | **Always** add a task, as in Checkvist — below the selection — on the board, below the card a selected subtask is drawn on — and at the foot of the day on Today. On a subtask's own card in another column than its parent's, the new task is a plain card of that column, at the foot of its plain cards, rather than another subtask of the parent. While you type it in a list, `Tab` makes it a subtask of the task above and `Shift+Tab` brings it back out a level, keeping what you have typed |
| `Space` / `x` | **Always** complete the task (or reopen it) — unless you are typing |
| `Shift+Space` | Cancel the task, or reinstate it — it stopped mattering, rather than got done |
| `1`–`4` / `0` | Set the task's priority at once (1 is the most urgent), as `!1`–`!4` does when you type a task, or clear it — on the outline, the board, the matrix and Today. Setting or changing a priority moves the task to the top of its list (row `:1`); clearing one leaves it where it is. The number shows in a small box beside the title; one undo step each |
| `-` / `-1`–`-4` | Send the task to the back: priority −1, as `!-1` writes it — behind every task with no priority, and to the bottom of its list. `-2` to `-4` set the others behind the back, each just ahead of the one after, as `!-2`–`!-4` do. A lone `-` waits a moment in case a digit follows |
| `tc` | Hide or show completed tasks — a completed task stays put for three seconds and then leaves the list; `tc` shows them all again |
| `bu` | Bury the task: it starts tomorrow, so until then it sinks, greyed and subtasks and all, to the **Buried** section below its blocked siblings in the outline, on the board and in the matrix — and Today picks it up the morning it arrives. `start <value>` buries it until any other time; `clear start` digs it up |
| `tl` | Hide or show buried tasks — anything whose start is still to come. Shown by default; `tl` hides them, with their subtasks. A filter list with a start-date rule always shows what it matches, and jumping to a buried task from search shows them again |
| `gf` | Start a focus timer on the task: it counts up, with no set length and no break. With a block running, queue the task behind it |
| `gp` | Start a pomodoro on the task: a block of the suggested length (25 minutes unless the task's estimate says otherwise), with a break offered at its end. With a block running, queue the task behind it. Mac only |
| `Cmd+Shift+Return` | Finish the running focus block — completes its task and asks how it went, as Done does. Works from anywhere in the window, inside a field too |
| `Cmd+Shift+.` | Stop the running focus block — the time so far is logged unscored, the task stays open and the queue is dropped. Works from anywhere, the focus panel included |
| `Cmd+Alt+8` / `ga` | Set what's available now — the right dock's Conditions tab, with the keyboard in it: the conditions (Home, Campus, Floor space…) that hold here, which decide what the day offers. Again from inside it puts the dock away. See [Conditions](#conditions) |
| `:` then a number, `Return` | Vim's `:12`: go to that row. Today's rows show their numbers; elsewhere they show while you type. Past the last row is the last row |
| `f` / `Shift+F` then a letter, `;` | Vim's find: the next / previous task (in the sidebar, list) whose title starts with that letter; `;` repeats it |
| `gg` / `Shift+G`, `Ctrl+D` / `Ctrl+U` | First / last task, eight down / up (vim) |
| `gx` / `Cmd+O` | Open the task's first link (vim's open-the-URL-under-the-cursor) |
| `nm` / `Cmd+Ctrl+M` | Open the notes in the **Markdown editor** (the Markdown editor plugin): the notes the size of the window, the text on the left and the Markdown drawn on the right (`Cmd+E` cycles split / text / preview). On a task linked to an Obsidian note, `Cmd+Shift+O` chooses whether the notes are *additional* to the note (the default: Takt's own notes, the note drawn read-only beside them, `Ctrl+Tab`) or *the same as* it (the editor edits the `.md` file in the vault). `Cmd+S` saves, `Esc` or `Cmd+Return` saves and returns, and `Cmd+Ctrl+M` from inside the inspector's notes field carries what you typed in. A change made on disk while you type is never written over unasked. See [the Markdown editor](docs/markdown-editor.md) |
| `pl` / `Cmd+Ctrl+L` | Link…: find a note or folder in your Obsidian vaults or a Drive file linked before, choose "Choose files from Google Drive…" to pick files in Google's own picker in the browser (only the files you pick are shared with Takt), or paste any address — `↑` `↓` choose, `Return` links, `Tab` narrows to Obsidian or Drive. The picker is as tall as its rows, and a line under it says what you can type and which sources are switched off. Rows with links show a small link mark; see [Links](docs/links.md) |
| `]` / `[` | Open the task's subtasks / leave them |
| `→` / `←` | In the outline, show a task's subtasks and step into them / hide them and step out — without opening the task. Elsewhere, open and leave it. At the top level of a list, `←` goes back to the list in the sidebar — opening the sidebar if it was closed, and closing it again once the keyboard leaves it |
| `za` / `Cmd+←` / `Cmd+→` | Fold or unfold the task you are on (outline or board) / fold / unfold the whole outline |
| `.` / `,` | Grow the task you are on to show its subtasks / shrink it (on a task with none, folds the one it sits under) — in the outline and on a board card or subtask row |
| `l` / `h` | On the board, step into the card's subtask rows / back out onto the card (`Esc` too). Inside, `↑`/`↓` walk the rows and Space, `ee`, the inspector and folding act on the row you are on |
| `Cmd+Shift+C` / `Cmd+Ctrl+C` | On the board, add a column / delete the one you are in (it asks where its tasks go) |
| `Alt+Up` / `Alt+Down` | Move the task up or down past its neighbour: on the board, the card above or below in its column, or the row above or below on its card |
| `Cmd+N` / `Cmd+Shift+N` / `Cmd+Alt+N` | New task / list / folder |
| `F2` / `Shift+R` | In the sidebar, rename the list or folder you are on |
| `Shift+Return` / `Alt+Shift+Return` | New task above the selection (Checkvist's) / new subtask |
| `Cmd+F` / `Cmd+Shift+F` | Search |
| `Cmd+P` / `gl` | Go to a list — opened empty, it lists the ones you were in most recently first. Every list and nested list, matched by the letters you type (`wsr` finds "Write the spring report"), with its folder beside it. Clicking the list's name at the top of the pane opens it too |
| `Cmd+Ctrl+I` / `Cmd+Ctrl+D` | The right dock on its Inspector tab / the bottom dock on its Done tab, or put it away |
| `Cmd+Shift+A` | The left dock's Agent tab — your Claude Code or Codex, each change it makes shown with an Undo |
| `Cmd+J` | The bottom dock — Progress, Done and Timeline — as `Cmd+B` does the left: on the tab it was on, with the keyboard in it on Done or Timeline; up already, it takes the keyboard; from inside it (or on Progress, which never takes it), it goes away (Zed's bottom dock) |
| `Cmd+R` or `Cmd+Alt+B` / `Cmd+Alt+Y` | The right dock — Inspector and Conditions — as `Cmd+B` does the left, or close all docks (Zed's); `Cmd+R` works from inside a field |
| `Cmd+Shift+R` | Sync now: Takt Sync, and Google when it is connected and on, straight away rather than on their next turn; works from inside a field |
| `Cmd+Shift+{` / `Cmd+Shift+}`, or `[` / `]` | In a dock, its previous / next tab — Inspector and Conditions on the right, Done and Timeline below — with the keyboard brought along (Zed's previous / next tab; on a task pane the same chords change list). `Ctrl+Tab` leaves it for the next region |
| `oo` / `Cmd+Alt+I` | List settings |
| `Cmd+Z` / `Cmd+Shift+Z`, or `Ctrl+Z` / `Ctrl+Shift+Z`, or `uu` | Undo / redo |
| `Cmd+Shift+P` / `Cmd+K` | The command palette — everything the workspace can do, and its key |
| `Cmd+/` | The same commands, grouped as a reference |
| `Esc` | Cancel what you are typing, or leave the surface you are in |

In the sidebar the keys are Zed's project panel's, lists standing in for files
and folders for directories: `Cmd+N` new list and `Cmd+Alt+N` new folder beside
the row you are on, named in place in a field drawn where it will appear (`Return` makes it, `Esc` leaves nothing behind), `F2` rename, `Delete` delete (it asks), `Cmd+←` / `Cmd+→`
collapse / expand every folder, `→` (or `l`) opens the row. On a folder that is already open, `→` **enters** it: its lists' tasks together on one board or outline, with the keyboard on them, so you can work through the folder without opening each list. On a list, `→` goes into its tasks the same way. `Return` is not "open" anywhere, the sidebar included — it adds a task to the list you are on, as it does everywhere else. Its vim panel's netrw
keys work too: `d` new folder, `%` new list, `Shift+D` delete, `Shift+I` icon (a symbol or an emoji), `Shift+R`
rename, `h` / `l` / `-` collapse, expand and go up, `gg` / `Shift+G` the ends,
`{` / `}` the previous / next folder and `:` the palette. Right-clicking a row
opens the same panel's menu, each item with its sidebar key beside it.

The **inspector** is modal, as vim is. With the keyboard in it, `j` / `k` (or
`↓` / `↑`) move a row cursor down every row, in sections set apart by a
gap (no headings) by what you do with a task, and within When, Filed and Needs each row is its label beside its
value, so Start sits plainly above Estimate, Due and Repeat:
the title; working on it (Start focus, which says how far it has got — time
logged against the estimate and subtasks done — and Log time worked, where on
the Mac a minute selector floats under the row — `↑` / `↓` step five
minutes, `Shift+↑` / `Shift+↓` fifteen, `←` / `→` walk the presets (15, 25,
30, 45, 60 and 90 minutes, each one click too), digits type the minutes,
`Return` logs it and `Esc` leaves — to record work done without the
timer and without finishing the task, which counts toward a daily due today
as a timed block would; then daily progress and the commitment); when
(Start, the slot it is scheduled into and hidden until; Estimate, how long it takes; how it may be split once it
has one, Due, Repeat); the notes; where it is filed (List, Priority, Tags); what it
waits on (Conditions, Blocked by, Waiting on); and the actions —
and `gg` / `Shift+G` (or `Home` / `End`) go to the first and last. Landing on
a text row only highlights it, so the notes no longer keep the keyboard.
`Return`, `i` or `e` edits the row: it puts the caret in a field, presses a
button, flips a switch, opens a menu, the date picker, the repeat selector or,
on Conditions, the `cc` finder. On the Mac the date picker under Start
and Due floats over the rows below it, and is a month of days with the time typed beside it, not picked: the
arrows (or `hjkl`) move a day or a week, `[` / `]` a month, `t` goes to today,
and `Tab` moves to the time field, where `9`, `9:30`, `14:00` and `2pm` all
read (a digit typed on the days starts it, `↑` / `↓` nudge it by 15 minutes).
Under Scheduled — the slot set aside for doing the task, which is what Google
Calendar shows as its event — the time also takes a range, `9-10:30`, and a
second `Tab` reaches **For**, its length (`45`, `45m`, `1h 30m`), which is the
task's estimate. `Shift+Tab` goes back, `Return` sets it, `⌫` on the days
clears it and `Esc` leaves it as it was. A Due with no time is due that day,
all day; a Scheduled with no time starts at 9:00. On the Mac the repeat selector reads "Every
‹3› ‹days›": `↑` / `↓` change the part in focus, `←` / `→` move between the
count and the unit (day, week, weekday, or a named day), digits type the
count, `d` and `w` pick days and weeks, `Return` sets it, `⌫` stops it
repeating and `Esc` leaves it; it shows the next four times it would come
round. `x` or `Delete` clears it — empties a field, takes a date or a
blocker away, sets the priority back to none. `←` / `→` step a menu such as
Priority, `h` (and `←` elsewhere) hands the keyboard back to the tasks, and so
does `Esc`. In a one-line field `Return` keeps what you typed and `Esc` puts
it back, and either returns you to the row; `↑` / `↓` keep it and move on. In
the notes and the links the arrows and `Return` are the text's: `Esc` or
`Cmd+Return` keeps the text and comes out; with the Markdown editor plugin on,
the maximise button on the notes' row (or `nm`, or `Cmd+Ctrl+M` from inside them) opens them
full size, and says whether they are the task's linked Obsidian note.
`Tab` / `Shift+Tab` keep the field
and go on to the next / previous row, editing it if it is a field. The mouse
works as before. A task's links sit beside its name, each as its kind's icon
(a click opens it), followed by a link button with its key, `pl`: it opens the
finder, where you type to search Obsidian and Drive or paste an address, and
where a link already there can be opened or removed. The task's own keys work from the inspector too — the ones
shown beside its rows (`nn` the notes, `tt` the tags, `pl` a link, `⌥T` the
estimate, `Space` to complete) and every `Cmd` chord — except where the
inspector has the key itself (`i`, `e`, `x`, `j`, `k`, `h`, `gg`); on a list or
folder they wait, so they never reach a task you can't see.

On the Mac the inspector also takes a **list or a folder**. While the
keyboard is in the sidebar on one, the inspector shows it rather than the
selected task, and `Ctrl+3` (or `Ctrl+Tab`) takes the keyboard into it there;
it shows one too when no task is selected. A list's rows are its name, what it
is (how many tasks, whether it is completed, archived or shared), its
**Folder** (`←` / `→` step through the folders, `x` puts it at the top
level), its icon, the agent's context for it, sharing, completing or
reopening it, archiving or restoring it, and **More settings**, the `oo`
sheet with its root task and colour. A folder's are its name, what it holds,
the folder it sits **Inside** and its icon. `Esc` or `h` hands the keyboard
back to the sidebar.

The palette, search, the list finder, the move picker, quick edits (`t`, `dd`,
`ee`…), the reference and the new list / folder / column prompts all open in
one **overlay** at the top of the window rather than as sheets. One is up at a
time and asking for another replaces it (`Cmd+K` from search opens the
palette); `Esc` or a click outside closes it, and `Return` confirms — in a
notes edit too, where `Shift+Return` is the new line.

The arrows read by modifier. Bare, they move the cursor, and past the last row
it wraps to the first (and back from the first to the last); with `Cmd`, they
move it to the ends. In the sidebar, `→` on a folder that is already open
enters its tasks (`Return` adds a task there, as everywhere). `Alt` moves the *task* within its list — up and down among its
siblings, out and in a level. `Shift+Alt` carries it somewhere else — another
board column, another list. `Ctrl` is left to macOS, which gives it and the
arrows to Spaces and Mission Control.

The table above is the short version. The complete one is in the app: `Cmd+K`,
or a **double-tap of Shift**, opens the palette, and `Cmd+/` — or the keyboard icon in
the status bar — shows the same commands as a grouped reference. Both are rendered from a single catalogue, the Rust core's
`core/src/keymap/catalogue.rs` read through `WorkspaceCommandCatalog`, which is also what the
key router dispatches through — so a key, a palette row and a reference row
cannot disagree about what happens. [`docs/keyboard-shortcuts.md`](docs/keyboard-shortcuts.md)
covers the behaviour a table cannot: sequence timing, which keys survive a text
field, which surfaces take the keyboard outright, and how to rebind any of it
in `~/Library/Application Support/Takt/keymap.json` (**Open the keymap
file** in the palette).

### Typing a task

A new task can say more about itself than its title. End it with any of these
and they are set as it is filed, rather than by opening the task again straight
afterwards:

| Ending | Sets |
| --- | --- |
| `30m`, `1h`, `1.5h`, `1h30m` | The estimate (up to a day) |
| `@today`, `@tomorrow`, `@fri`, `@3d`, `@2w`, `@2026-10-02` | The day it is due — a weekday means the next one, today included |
| `^mon`, `^tomorrow`, `^2026-10-05` — any of the same days | The day it starts: a task in Today or This week moves to the one that day belongs to, as a start set in the editor moves it. With a due day too, the start must come first — `^mon @fri` the same week is fine, `^mon @fri` when Monday comes after Friday is shown in red and not filed until one changes. (Checkvist reads `^` as due; Takt does not.) |
| `#work` | A tag — it has to start with a letter, so `#12` stays in the title |
| `!1`–`!4` | The priority — 1 is the most urgent, 4 the least. A task added with one goes to the top of its list |
| `!-1`–`!-4` | A priority behind every task that has none — `!-1` the very back, `!-2` just before it. A task added with one goes to the bottom of its list |
| `wait:Sam` | Waiting on Sam — filed in the Waiting on column straight away |
| `https://…`, `obsidian://…` | A link — a Drive file or folder, a Doc or Sheet, an Obsidian note, a page. An address on its own names the task for what it points at. See [Links](docs/links.md) |

`Write the release notes 45m #work @fri !1` files a task called "Write the
release notes", estimated at 45 minutes, tagged `work`, due on Friday, at
priority 1 — the most urgent. `write essay ^mon @fri` starts on Monday and is
due on Friday; the chips beside the field say *starts Mon 5 Oct* and *due Fri
9 Oct*.

In the add field and the quick-capture panel, `⌘⌫` deletes the word before the
caret, as `⌥⌫` does, rather than the whole line.

The same endings work when you **edit a title in place**. In the outline,
`F2`, `ee` or a click on the task already selected turns its title into a field
in the row, as in Checkvist; `ea` starts typing at the end of the title and `ei`
at the start, as vim's `a` and `i` do, and `r` or `Shift+R` clears it so what you type replaces it, as vim's `R` does in Zed: type ` @fri !2` after the title and Return, and the
title stays as it was while the due day and priority change (` ^mon` moves its start). Tags are added to
the ones the task has. Escape puts the title back; clicking away saves.

In the row that opens for a new task (`Return`, `Shift+Return`), `Tab` makes it a
subtask of the task above and `Shift+Tab` takes it back out, before you type or
after. `↑` and `↓` close the row, dropping what was typed as `Escape` does, and
carry on to the task above or below where it sat. `Return` files the task and
closes the row, leaving you in normal mode with the new task selected; press
`Return` again for the next one. While the row is open the inspector shows the
task being typed, not the one you started from.

**Quick Add's slots.** To the right of Quick Add's field sit three slots: the
due day, the conditions the task needs, and its time to complete. `→` at the
end of the text (or `Tab`) moves into the date slot, `→` again to conditions,
then time; `←` (or `Shift+Tab`) goes back, and from the date slot back into
the text with the caret at its end. A click puts the keys on a slot too.
`Return` in any slot files the task and `Esc` drops it, as in the text.

| Slot | Keys |
|---|---|
| Date | `↑` a day on (the first press is today), `Shift+↑` a week on, `↓` the calendar — arrows move a day or a week, `Shift+↑` / `Shift+↓` a month, `Return` picks, `Esc` back to the slot — and `Delete` no date |
| Conditions | `↓` or `Space` opens the conditions finder `cc` uses, ticking the task not yet made; `Esc` back to the slot. `Delete` drops the last one |
| Time | `↑` / `↓` step through 5m, 10m, 15m, 30m, 45m, 1h, 1.5h, 2h, 3h, 4h, 6h, 8h; digits type minutes; `Delete` clears it |

What a slot holds wins over the same thing typed on the end of the title
(`@fri`, `30m`), and the task, its due day, estimate and conditions are one
undo step. The row a new task is typed in inside the window has no slots: its
`↑` and `↓` already leave the row, and it is drawn as the row or card it will
become, which a board column has no room to widen.

Only the **trailing** words are read, and the first word from the end that is
not one of these stops the reading — so a title is never rewritten in the
middle: `Read 30 pages` and `Buy 2m of cable` keep every word. The first word is
always the title's, and a second estimate, day or priority stops the reading
too, leaving the earlier one in the title where you can see it. As you type,
what will be set shows as chips beside the field — in the row a new task is
typed in, in Quick Add's window and in the focus panel's "Add … to today" row —
so nothing is a surprise after Return.

### Coming from Checkvist

The desktop workspace supports Checkvist-style two-letter commands, including
`uu`, `ee`, `dd`, `nn`, `tt`, `mm` and `ww`, plus native undo/redo in the
Edit menu. Board arrows navigate every column, including empty ones,
`Up`/`Down` step through the cards, and `l` steps into the card you are on so
they walk its subtask rows instead (`h` or `Esc` steps back out); `Space`
completes the row you are on and `Return` adds a task, as Checkvist's do. The sidebar outlines the list you are navigating.

A task you tick off stays where it was for three seconds — long enough to see
it go and to take it back — and then leaves the list; the Done rail keeps it.
`tc` shows every completed task again, and toggles back.

A task whose start date is still to come is **buried**: it stays in the
outline, on the board and in the matrix, greyed and with the day it starts on
its row ("starts Mon"), but it sinks below its siblings — below the blocked
ones too — under a quiet **Buried** label, with everything beneath it. So a
parent's children read open work, then **Blocked**, then **Buried**, each in
its own order; a task that is both blocked and buried is buried. The moment it
starts it comes back to its place by itself. `bu` buries the selected task
until tomorrow; `tl` hides buried tasks altogether, and toggles back. `⌥↑`/`⌥↓`
step a buried task past its buried neighbours, as they step a blocked one past
blocked ones. Today and the focus suggestions already left future starts alone
and still do.

Subtasks fold where they stand, as they do in Checkvist, so you can look
inside a task without opening it: in the outline `→` unfolds a task and steps
into its subtasks and `←` folds it and steps back out, `za` folds the task you
are on in the outline or on a board card, and every row with subtasks has a
chevron. Folds are shared by the outline and the board and kept across
launches — see [`docs/keyboard-shortcuts.md`](docs/keyboard-shortcuts.md#folding-subtasks-where-they-are).

Checkvist's `ll`, `hc`, `hh` and `xx` are `gl`, `tc`, `gb` and `ex` here. A letter
that begins a sequence and also means something on its own would wait up to 1.2
seconds for a second letter, then do its own job; any other key ends the wait at
once, and holding Shift skips it. Of the task keys only `-` does that now, in
case a digit follows to make `-2` to `-4`: `x` completes at once, since
extracting a branch moved from `xx` to `ex`. A letter that begins no sequence
where you are never waits.

The one-letter root tabs that used to collide with those sequences are gone
with the menu bar panel they belonged to, so a sequence no longer has to
compete with a tab for its starter letter.

## Today

The window opens on **Today**: the day as a numbered list of cards, the same one
the hotkey summons over other apps. One component, two mounts — a day that read
differently depending on where you opened it would be two days.

Each card carries what the task is, what it should cost, and what it has cost so
far. `gf` (or `Return` in the panel) on a card starts a timer on it, `gp` (or `p`) a pomodoro; the card you are on grows a live clock and a
strip of pause, skip, log and done, so a whole block runs without the list ever
going away.

The day also says when it ends: the header reads like `1h 20m of 3h · 1h 40m
left · done by 17:35` — what the estimated tasks still owe, worked straight
through from now, counting the block that is running. Tasks with no estimate
are left out of it (the tooltip says how many), and in the window clicking a
card's estimate, or its "No estimate", opens the same estimate editor as
`Option+T`.

The two mounts differ in their chrome only. In the window the pane is its header
— the scope, and time logged against the day's estimates — and the list, walked
with the ordinary selection keys like any other pane. The day's other numbers
are in the status bar (click them for the timeline), a task is added from the
title bar's field — which, while Today is up, puts it on today — and a search is
`Cmd+F`. The summoned panel has no title bar or status bar to lean on, so it
keeps its own: a bar of estimated against logged under the same finish time, a line setting today against
the week, a list of the day's logged blocks, and a field that searches every
task's title and notes and offers to add what you typed to today.

A card with open subtasks has a chevron beside its number, and opens where it
stands, as a branch does in Checkvist's outline: `→` (or `l`) shows its open
subtasks indented beneath it, hung from the same guides and `└` as the outline's,
and a second press steps into them; `←` (or `h`) folds them away again, or from
a subtask steps back up to its task. The arrow keys walk the subtasks like any
other row, and everything a card answers to — `Space`, `ee`, the inspector,
`gf` — works on them too. `Return` on a subtask adds the next one under the
same task, typed in place. The subtasks are the task's open ones in its list's
order, whether or not they are on today themselves; one that is on today is
drawn once, under its task, while that task is open. Which cards are open is
remembered between launches.

Anything already done is ticked off from here rather than somewhere else: each
card's number becomes a tick when the pointer is over it, and `Space`
does the same to the card you are on. Finishing plays whichever celebration is
configured, wherever it was finished from.

A task that owes the day a **daily contribution** is badged where it is read
rather than gathered into a screen of its own — a daily is a requirement placed
on an ordinary task, not a kind of item. Clicking the badge records today's
contribution without starting a block; the task itself stays open until it is
genuinely finished, and ticking off such a task records the contribution rather
than closing it.

The window also carries a strip naming which of the four modes is up, then
Focus, which raises the focus panel, and Timeline, which opens in the bottom
dock under them — so none of it is reachable only by a key you have to already know.

| Key | Action |
| --- | --- |
| `Cmd+T` | Today |
| `Cmd+2` | Board |
| `Cmd+3` | Outline |
| `Cmd+4` | Matrix |
| `Cmd+8` | Focus — raises the floating focus panel |
| `Cmd+9` | The day's timeline |
| `Cmd+0` | Everything, across all active lists |
| `Ctrl+1` / `Ctrl+2` / `Ctrl+3` | Focus the sidebar, the task surface, the inspector |
| `Cmd+B` | The left dock (Zed's), on its tab — the sidebar or the agent — with the keyboard in it; up already, it takes the keyboard; from inside it, it goes away. Its tabs' own keys (`Cmd+Shift+A` for the agent) go to one tab. Both tabs are the same width |
| `Up` / `Down` (or `j` / `k`) | Move between cards, and through the subtasks of an open one |
| `Right` / `l` | Open a card to show its subtasks, then step into them — on one with none, go to the inspector |
| `Left` / `h` | Fold an open card, or step from a subtask up to its card — at the top of the day, back to the list in the sidebar |
| `gf` / `gp` (window), `Return` / `p` (panel) | Start a timer / a pomodoro on the card you are on, or queue it behind the one running |
| `Space` / `x` | Tick the card off without running a block — on the running card, finish it |
| `Return` | Add a task — on a subtask, the next subtask under the same card (in the panel, from what you typed) |
| `Ctrl+T` | Plan the card for today, or take it off — an overdue or dated card becomes one you placed |
| `Alt+↑` / `Alt+↓` | Arrange the day: move a planned card earlier or later, whichever lists they came from |
| `Cmd+Return` | Open the card in the main window (the panel) |

The digit row used to move the caret between the window's three panes. Changing
what you are looking at is the bigger thing, so it took the row and pane focus
moved to `Ctrl`.

## Focus

Focus is the floating panel described [below](#the-focus-panel): the day to
pick from while nothing runs, and the queue once something does. There is no
focus pane in the main window. `Cmd+8`, the title bar's **Focus** button and
Today's **Focus** button all raise the panel, and `⌃⌥⇧⌘F` summons it from any
app.

A block is one of two kinds. **A timer** (`gf`, the inspector's *Start focus*,
`Return` in the panel) counts up for as long as you work, with no length and
no break; you end it with Finish or Stop. **A pomodoro** (`gp`, `p` in the
panel) runs for the suggested length, 25 minutes unless the task's estimate
says otherwise, and when it runs out asks how it went and offers a 5-minute
break. Either one puts up [the focus pill](#the-focus-pill).

The quality question is what turns minutes into points, so it is asked by
default — but **Ask how each focus block went** in Settings turns it off, and
blocks then close at ×1 without stopping.

**Leaving a block** never means finding it first. `Cmd+Shift+Return` finishes
it (Done: the task closes and the question is asked) and `Cmd+Shift+.` stops it
(the session ends, the time so far is logged unscored and the task stays open),
from anywhere in the window — inside a text field too — and from the focus
panel. Both are in the **Task** menu, greyed while nothing runs, and in the
command palette only while a block is running. A **Stop** button sits wherever
a running block is drawn: in the status bar, on the panel's strip, on Today's
running card and in the inspector of the task being worked on, and the menu
bar item's menu has **Stop Focusing**. `Esc` still only hides the panel.

The floating clock is a **companion, not a remote control that needs its
station**: it names the task, draws the block's progress as a hairline, and
carries pause, log and done, so closing the main window while a block is running
leaves the clock up rather than taking it away. It appears on its own whenever
the last window closes on a running block.

`Cmd+9` opens the day's **timeline** as a tab in the bottom dock, beside the
progress graph and the done rail: the hours run left to right across the dock
and every block is a bar along them, the running one growing live, with blocks
too close to share a row stacked into rows of their own so no two titles
overlap. The day's figures (time focused, blocks, the longest, points) and the
breakdown by task sit in a column on the right, and the day and its controls on
the dock's tab bar. A long day scrolls sideways rather than squeezing; drag the
dock taller for more rows. `←` / `→` (or `h` / `l`) step a day, `t` comes back
to today, `↑` / `↓` (or `j` / `k`) walk the breakdown and `o` opens the task
under the cursor in its own list. `Esc` hands the keyboard back to the work and
leaves the timeline open; `Cmd+9` or `Cmd+J` puts it away.

### Conditions

A **condition** is a place or circumstance a task needs before it is worth
starting — *Home*, *Campus*, *Floor space*. Each is filed under a
**category** — *Location*, *Equipment*, *Energy*, whatever you type — or
none, which shows under *Other*. **Location** is the one category with a rule:
you are in one place at a time, so its conditions are exclusive. A task that requires one stays out
of the day and the panel until you say that condition holds; a task with no
conditions is available whenever. Its row and card show what it needs as
tags, one per requirement (*Home*, *Campus or Library*, *not Home*): in the success
colour while it holds, in the warning colour while it is what keeps the task
back.

**Requiring one** is `cc` on the task (or **Conditions** in the palette and
the Task menu). A finder opens with every condition as a row with a box,
under its category's heading (once you type, each row names its category instead):
type to narrow, `↑`/`↓` to choose, and `Space` or `Return` ticks the row or
unticks it — saved at once, one undo step each. `!` ticks the row as **not**
holding instead (*not Home*: the task waits while you are at Home), turns a
condition already ticked into its "not", and unticks a "not". Type a name nothing is called
yet and the last row is **New condition “…”**: `Return` makes it and requires
it together (two undo steps, the requirement first, since the condition
outlives the task); it is made with no category. It stays open for the next one, and `Esc` closes it. A
second **Location** joins the first as either-or (*Home or Campus*) rather than
asking for both, since you can only be in one. `Space` ticks once you have
moved with the arrows, or with nothing typed; otherwise it is a space in the
name. `Cmd+Return` goes to the Conditions tab.

**Saying where you are** is `gw` (or **Say where you are…** in the palette),
from anywhere: a finder of the places — the ones tasks wait on first, each
with how many are waiting — and *Anywhere*. Type a few letters and `Return`:
the place replaces the last one, and *Anywhere* turns every place off and
keeps the rest on.

Saying what holds now, and managing the conditions themselves, live in the right dock's **Conditions** tab.
`Cmd+Alt+8` (or `ga`, or **Set what's available now…** in the palette) opens
it with the keyboard in it; so does clicking the status bar's indicator, which
shows the active conditions (or *Anywhere*); in the focus panel, `Cmd+Alt+8`
hands over to the window's tab. When a condition is holding tasks back, Today (and the panel) ends with
one line saying how many and on what — *4 waiting on Campus 3 · Library 1*;
clicking it opens the tab.

What holds now is kept in the workspace, not the app's preferences, so the
`takt` CLI and the agent read it and set it too (`takt ws here`,
`workspace_context_set`), and the app picks their change up at once. It
remembers when its conditions were said: one said on an earlier day is
**stale** — a place left on overnight is probably not where you are — and
the status bar's indicator and the tab show it in the warning colour, with
**Clear** (`c`) beside it.

- **Available now**: the settings first — **Where you are**, **Free time**,
  then any category whose conditions provide others, such as a *Tools on
  hand* you make (Laptop, Phone, Headphones) — then the plain properties
  under their category headings, a box each, and **only those an open task
  needs** (or that provide one it needs): a condition nothing open requires
  changes nothing, so it is not asked about, and a category with none of
  those is left out. One that is on always stays, so it can be turned off.
  Each one off says how many tasks are **waiting** on it. With nothing
  needing a condition it says *No open tasks need a condition*. **Where you
  are** is one row: `←`/`→` step through the places, from *Anywhere*, since
  you are in one place at a time. The other conditions combine: `Space` (or
  a click) turns one on or off, and `→`/`←` turn it on and off too. `c`
  clears them all. On a row with nothing to change, `←` goes back to the
  tasks. **Free time** is how long you have: `←`/`→` give or take
  a quarter of an hour, and typing a number anywhere in the tab sets it —
  `45`, `1h30`, `17:00`, `5pm`, then `Return` (an empty field is no limit);
  a task whose estimate does not fit waits. With a time set, **Only what I
  can finish** offers only the tasks whose remaining work fits in it.
  **What a condition provides holds with it**: say you are at Home, which
  provides Floor space, Desk and Quiet, and those show as on, each noting
  *from Home*, so a task needing Floor space is offered at Home or the Gym
  without ever turning Floor space on. Turning a provided one off (the
  housemates are in: Quiet off) is an override, shown *off for now*, and it
  lasts until what provides it is turned off or replaced — changing where
  you are starts the new place with everything it provides. A task that
  needs a condition *not* to hold is checked against the same set.
  **Forget this** (`←`/`→`: never, in one, two or four hours, at midnight)
  is when where you are and the time you have stop counting. What you set is kept
  across relaunches and follows you across your devices: say you are at
  Campus on the phone and the Mac knows, and the last device to say where
  you are wins. A context said on an earlier day is flagged as possibly
  stale on every device. A context that expires is offered back with **Use
  again** (`Shift+U`).
- **Start each day**: **Afresh** (`Space`, or `←`/`→`) says that when the day
  changes, what holds goes back to what each day starts with — you start at
  Home every day — with nothing switched off for now and no free time set;
  off, what holds carries over. **Day changes at** (`←`/`→`, 04:00 unless you
  move it) is the hour, the same one the daily log counts days from, so moving
  it here moves it there (and the daily log's **Day starts at** moves it
  here); the account keeps it, so every device changes day at the same hour.
  **Start with** (`Return` or `p`) opens the conditions picker on the set each
  day starts with: tick Home, a laptop; one place at most. It happens once a
  day, on whichever device looks first after the hour — the Mac checks every
  half minute and when it comes to the front, the phones when they open or come
  back, the CLI and the agent when they read or set what holds — and a device
  that was closed for days resets once when it opens. Something you said since
  the day changed is kept, on any device. Changing these never resets there
  and then; the first is at the next change of day. `takt ws day-start` and
  `workspace_day_start_set` read and set them.
- **Conditions**: a new-condition row (`a` or `Cmd+N` puts the caret in it,
  `Return` makes the condition, filed under the category in the menu beside
  it — from a condition's row, that condition's category; `t` on the row
  picks another; a name a live condition already has is that condition), then every condition under its category heading with how
  many open tasks need it — completed and archived tasks do not count. `Return` or `F2` renames one in its row (tasks
  keep their requirements), `t` files it under a category — a finder of the
  categories in use, Location first, then *No category*; type a name nothing
  uses for **New category “…”** — and `e` archives or restores it. Filing one
  under Location makes it a place; filing it elsewhere makes it not one. `p`
  sets what it **provides**: the conditions picker opens on it, and each
  tick is a condition that holds whenever this one does (one undo step
  each; a name nothing has yet makes a new one, ticked). Links chain — Home
  provides Desk, Desk provides Typing — and a loop does no harm. Its row
  says what it provides. **Manage conditions…** in the palette opens the tab
  on the new-condition row.
- **Organising them**: `Alt+↑`/`Alt+↓` move a condition within its category
  and `Alt+Shift+↑`/`Alt+Shift+↓` move its whole category, as they move tasks;
  the order is kept in the workspace, so the phones and the finders follow it.
  `Shift+T` renames the category under the cursor, every condition in it
  with it — a name already in use merges the two; Location keeps its name. `m` merges
  the condition into another, chosen in a finder: every task that needed it
  needs the other instead, what holds now follows, and it is archived (one
  undo step). `o` lists the open tasks that need it under its row (click one
  to go to it; `o` again hides them). `z` folds or unfolds its category, and
  `Shift+Z` every category; folds are remembered per workspace. Archived
  conditions are apart, under a folded **Archived** heading at the foot: `z`
  or `Return` on it shows them.
- `↑`/`↓` (or `j`/`k`) walk every half with one cursor; `Esc` or `←` hands
  the keyboard back to the work and leaves the dock open, `Cmd+R` puts it away.
- **In the inspector**: the Conditions row's **Add condition** does
  what `cc` does with the mouse, its conditions under their category
  headings — **New condition…** at the foot of its menu makes one (with a
  category menu beside its name) and requires it — and **Or…** widens a
  requirement to either of two.

On the phones, the Focus screen's **Where you are** follows the same rule: only
the conditions an open task needs (and any that is on), under their category
headings in the order the Mac keeps, and **New condition** asks for a category (one in use, or typed). The
iPhone's Conditions sheet groups by category too, and a condition's long-press
menu files it under one (**Category**, with **New category…**); its counts
are open tasks only, as on the Mac. What a setting provides shows as on there
too, *from Home*, and turning it off is the same override; what a condition
provides is set on the Mac (or with `takt ws provides`). The phones keep what holds now on the
phone itself, and reordering, merging and renaming a category are the Mac's
for now; what they do there reaches the phones through sync.

### The focus panel

`⌃⌥⇧⌘F` summons the focus panel over whatever app you are in, and pressing
it again puts the panel away; `Esc` does too. It appears and goes at once,
with no fade. It is the one hotkey for starting, finding and adding tasks:
Quick Add has no hotkey of its own any more. The status item's menu, `Cmd+8`
and the app menu's Quick Add open it too. Rebind or disable the hotkey in
Settings → Keyboard.

The panel is the focus pill grown larger: it opens over the pill's place
(the top centre of the screen until you drag the pill elsewhere), the pill
hides while it is up, and putting the panel away shrinks it back to the
pill.

At the top are the running task and its clock. It opens on its rows: the
day to start from, or while a block runs the queue behind it. Every action
is a letter (see below); `/` or `n` start typing.

**Typing** lists the day until you type. Type and it finds every open task and list that matches, with a
last row, **Add "…"**, that files what you typed as a new task (`45m`,
`#tag`, `@fri` and `!1` read as in Quick Add, and their chips show on the
row):

| Key | Action |
| --- | --- |
| `↑` / `↓` (or `Ctrl+N` / `Ctrl+P`) | Move through the rows |
| `Return` | On a task, start a timer on it. On a list, narrow to that list's tasks, and Add then files there. On Add, file the task: it heads the day, highlighted, and `Return` again starts it (opened from the app menu's Quick Add, the panel goes instead) |
| `Cmd+Return` | On Add, file the task and put the panel away |
| `Option+Return` | Start a pomodoro on the highlighted task |
| `Cmd+O` | Open the highlighted task, or list, in the main window |
| `Tab` | Choose the list Add files into: a picker of every list, with its folder, that you type to filter; `↑` / `↓` (or `Tab`) move, `Return` chooses and `Esc` goes back to your typing. Clicking **Adds to …** under the field opens it too. It starts at Quick Add's list, and narrowing to a list chooses that list |
| `Esc` | Clear what you typed, then leave a narrowed list, then back to the rows |

**On the rows, every action is a letter**, and a footer of key hints spells
them out:

| Key | Action |
| --- | --- |
| `j` / `k`, `↓` / `↑` | Move through the queue |
| `Return` / `p` | Start a timer / a pomodoro on the highlighted task, or while a block runs queue it as one |
| `o` | Open the highlighted task (or with none, the running one) in the main window, which takes the keyboard |
| `Space` | Pause or resume the running block (on a break, back to work) |
| `d` | Finish the block, completing its task, and ask how it went |
| `s` | Stop: the time so far is logged and the task stays open |
| `/` or `n` | Type, to find or add a task |
| `Esc` | Hide the panel: back to the pill while a block runs |

While the block's "how did it go?" question is up, the keys answer it instead.

### The focus pill

Starting any block, a timer or a pomodoro, puts up **the focus pill**: a small
strip at the top centre of the main display with the task's title and its
clock, counting up for a timer and down for a pomodoro or a break. It floats
over every app without taking the keyboard from the one you are in. Drag it
anywhere; click it for the focus panel. It goes when the session ends. Turn it
off with **Show the focus pill while a block runs** in Settings → Focus.

## Command palette

`Cmd+K`, or a **double-tap of Shift**, as in Checkvist. A modifier on its own
produces no key-down event, so `⇧⇧` is recognised as a gesture rather than
bound as a shortcut; Shift held for a capital letter counts as a chord rather
than a tap, so typing never trips it. It opens from inside a text field too,
since leaving the field to do something else is most of what it is for.

Type to narrow. A prefix of the name beats a match in the middle of it, the
initials work (`atk` finds *Add a task*), and so does typing the key itself —
`cmd+shift+d` finds the command that key runs, which is the answer to "what
was that shortcut again" as often as the name is. Commands for the surface you
are on sort first; the rest stay listed rather than disappearing, because the
palette is also how you find out that a surface you are not on has the thing
you want.

Every row carries its key, and rows that have no key say so plainly rather
than looking unbound. Rows for the arrows, `J`/`K`, `Home`/`End` and the
priority digits are listed too, dimmed and not selectable: they move a cursor,
and the palette has already taken the keyboard away from wherever that cursor
was, so running one from a list would do nothing you could see. They are shown
because "what can I press here" is a fair question about them.

### Typed commands

Separately from the palette above, `CommandEngine` parses a text command
language inherited from the Checkvist-era menu bar panel. **It has no
surface**: the executor that ran these commands had no caller once the panel
went, and was deleted. The families below parse and are tested but cannot be
reached from the app. They are documented because the parser is still there and
still correct, not because you can type them today. Most accept several spellings: `unrepeat`, `no
repeat`, `remove repeat` and `clear repeat` all do the same thing.

| Family | Commands |
| --- | --- |
| **Status** | `done`, `undone`, `invalidate`, `delete`, `undo` |
| **Due** | `due <value>`, `clear due` |
| **Start date** | `start <value>`, `clear start` (`edit start` is unrelated: it opens the title for editing with the cursor at its start) |
| **Repeat** | `repeat <rule>`, `repeat daily`, `repeat every <n> <unit>`, `clear repeat` |
| **Tags** | `tag <name>`, `untag <name>` |
| **Priority** | `priority <1-9>`, `priority back`, `clear priority` |
| **Outline** | `expand`, `collapse`, `expand all`, `collapse all`, `enter children`, `exit parent`, `move up`, `move down`, `add sibling`, `add child`, `open link`, `edit` |
| **View** | `list <name>`, `toggle children` / `toggle subtree`, `toggle context`, `toggle hide future`, `command palette` |
| **Timer** | `toggle timer`, `pause timer` |
| **Calendar** | `sync google calendar`, `open google calendar` |
| **MCP** | `mcp guide`, `mcp config`, `copy mcp config`, `refresh mcp path` |
| **App** | `preferences`, `search`, `quick add`, `refresh lists`, `upload offline tasks`, `window`, `diagnostics` |

Due values understand natural language and times: `due today 14:30`, `due tomorrow 9am`, `due next week`, `due 4pm fri`, `due next monday morning`. The time words `morning`, `noon`, `afternoon`, `evening`, `midnight`, `eod` and `cob` all resolve to configurable named times.

There is no typed-command family for the matrix or the board: the parser never
grew one, and the keys (`Alt+1`–`Alt+4`, `Cmd+←` / `Cmd+→`) are the only way to
place a task on the matrix or move a card. The **View** and **Timer** families
act on the Checkvist-side state the removed menu bar panel rendered. What they
change no longer has a surface either, so they are doubly stranded — a workspace
that has not finished migrating, reachable by nothing.

## Views

A list is drawn one of three ways — board, outline or matrix — and the mode
strip in the title bar names the one you are in, in ink on a quiet fill, the
others muted. Today is a list of its own in the sidebar, drawn as the day. The
timeline is a tab in the bottom dock.

| Mode | Key | What it shows |
| --- | --- | --- |
| **Today** | `Cmd+T` | The day as a numbered list of cards — see [Today](#today) |
| **Board** | `Cmd+1` | Columns of cards, dragged between and within |
| **Outline** | `Cmd+2` | The list as a tree, indented, with each branch folding in place |
| **Matrix** | `Cmd+3` | The Eisenhower matrix as a plane with two axes — urgent to not urgent across, important to not important down — each task a dot at its own point, with the tasks not yet placed in a tray beneath. See [The matrix](#the-matrix) |
| **Focus** | `Cmd+8` | Raises the floating focus panel — the day, or the block that is running |
| **Timeline** | `Cmd+9` | The day as elapsed time rather than as a list |

`Cmd+0` shows **Everything** — every active list at once, rather than the one
the sidebar has selected.

### The matrix

The matrix is one plane, not four boxes. Its axes cross in the middle:
**urgency** runs across, most urgent at the left edge, and **importance** runs
down, most important at the top, so the four quadrants are where they always
were — Do now top left, Schedule top right, Delegate bottom left, Eliminate
bottom right — and their names sit faintly in the corners. Each placed task is
a small chip: a dot at its exact point and its title beside it (to the left of
the dot on the right-hand side of the plane). Two chips that would cover each
other are drawn a little apart; where they are stored does not change.

| Key | Action |
| --- | --- |
| Drag a chip, or a card from the tray | Drop it anywhere on the plane: it sits exactly there |
| Drag a chip onto the tray | Take it off the plane |
| `h` `j` `k` `l` / arrows | The nearest task that way (nearest along it first, then off to the side); down from the lowest goes into the tray, and up from the tray comes back |
| `v` | Move mode for the selected chip, which is tinted while it lasts; `v`, `Return` or `Esc` ends it |
| `h` / `l`, `←` / `→` in move mode | Nudge it more urgent / less urgent, a tenth of the way to the edge |
| `k` / `j`, `↑` / `↓` in move mode | Nudge it more important / less important, the same |
| `Shift` with those, in move mode | A fine nudge, a fifth as far |
| `Tab` / `Shift+Tab` | The next / previous task: quadrant by quadrant, each from the top, then the tray, wrapping round |
| `Option+1`–`Option+4` | Place the selected task at the centre of a quadrant (1 do now, 2 schedule, 3 delegate, 4 eliminate), beside it when another task is there |
| `Return` | A new task, at the centre of the selected task's quadrant — or in the tray when the selection is there |
| Click / double click | Select / rename |

On a task in the tray the arrows and `j`/`k` move along the tray as they do
everywhere else. `←` and `→` with nothing further that way do what they do
anywhere else.

A point runs from −100 to 100 on each axis, and may sit on an axis: on the
line counts as not urgent, or not important. It is stored ten further from
zero (0 as −10, 1 as 11), so it is never mistaken for a placement made by
quadrant. The phones, the CLI and the MCP
server read only which side of each axis a task is on, so they still see its
quadrant. A task placed by quadrant before — by an older Mac app, the phones,
or `takt matrix` with values from −9 to 9 — is drawn inside its quadrant, the
old 1 and 0 at its centre, several on one spot spread on a small grid by the
order they had; nudging or dragging it gives it a point of its own.

### Board columns

Each list has its own columns. A list that has never had its columns edited
uses the **default columns** — Backlog, In progress, This week, Waiting on and
Today until you set your own — and its first edit gives it columns of its own.
Everything's board and a filter list's work the same way.

Right-click a column's header to rename it, move it left or right, add a
column beside it, edit its rules, delete it, make this board's columns the
default, or reset this board to the default. The palette reaches each of them
too, acting on the column you are in:

| Command | Key |
| --- | --- |
| Add a board column | `Cmd+Shift+C` |
| Delete this board column… | `Cmd+Ctrl+C` |
| Rename / move left / move right / add left / add right / edit rules | palette |
| Make these the default board columns / reset this board to the default | palette |

Deleting a column asks where its tasks go — the remaining columns, the first
already chosen — and the move and the deletion are one undo step. **Today**
and **Waiting on** can be deleted from a board, but their tasks stay planned
or waiting and show in the first column there.

A column can carry **rules**, the same rules a filter list has, edited in the
same sheet. Tasks its rules match are drawn in it whichever column they are
filed in — the first rule column from the left wins — and a task matching
none stays in the column it is filed in. Dropping a task into a rule column
edits the task to match where one edit does (a tag is added, a priority or
due date set, the task planned for today); where the rules do not say what to
change, or the task would stay in an earlier rule column, the drop is refused
and the status bar says why.

Entering a card (`→` or `]`) shows its subtasks on a board of their own, with
the columns to the left of the card's own hidden: a step is not at an earlier
stage than the task it belongs to. Subtasks filed in a hidden column are drawn
in the card's column, without being refiled, so leaving changes nothing.
Waiting on and rule columns are not stages and stay.

Right-clicking a pane's empty space offers a new task, a new column on the
board, the other views and showing or hiding completed tasks; a task's own
menu shows each action's key on the right, as bound.

### Filter lists

Today, Everything and the Inbox are the stock lists at the top of the sidebar.
Under them sit your **filter lists**: saved, named rules asked of every task
in every active list — "tagged work and due within 7 days", "priority 2 or more
urgent and not blocked", "waiting on Sam". Selecting one shows Everything narrowed
to its matches, on the board, the outline or the matrix. Its rules can test a
tag, the due or start date, priority, the list or folder, whether it is
planned for today, blocked or waiting, its estimate, its board column, or
text in the title or notes, and combine by **every rule** or **any rule**;
**Show** picks open, finished or all tasks. Today and Everything are the
same kind of question, built in.

| Key | Action |
| --- | --- |
| `Cmd+Option+Shift+N` | Create a filter list |
| `Cmd+Option+Shift+E` | Edit the filter list in scope (or create one) |
| `Cmd+Option+I` / `oo` on its row | Its sheet: name, icon and rules |
| `Delete` on its row | Delete it (one undo step; no task is touched) |
| `Cmd+Up` / `Cmd+Down` on its row | Move it up or down |

The rules are evaluated in the Rust core, so the phones and `takt` show the
same tasks for the same filter list.

There used to be seven views instead, keyed `q` through `u`, inside a menu bar
panel: All, Due, Tags, Priority, Kanban, Matrix and Daily. Due, Tags and
Priority were filters over one list wearing the costume of separate places, and
Daily was a dashboard for a thing that is now a badge on a task. What is left
is the four that ask genuinely different questions.
## Waiting on

The board's **Waiting on** column holds what is out of your hands. `ww` on a
task opens a small form with three fields, and saving moves the task into that
column if it is not there already:

| Field | What it takes |
| --- | --- |
| Waiting on | Who or what — `Sam`, `Legal`, `invoice`. Shown as a small tag on the card, the outline row and in the inspector |
| Follow up | When to chase it: the add field's date words (`@fri`, `tomorrow`, `3d`, `2026-10-08`) with or without a time (`9am`, `2:30pm`, `14:00`, `noon`), in either order — `tomorrow 9am`, `fri at 2pm`, `2026-10-08 14:00`. A day on its own is 9:00; a time on its own is the next one to come. Shown as `↻ Thu 14:00` |
| Repeat | Empty or `once` for one follow-up, or every so many days, 1 to 365: `3`, `3d`, `every 3 days`, `every day`, `weekly`. Needs a follow-up time |

`Tab` and the arrows move between the fields, the line under Follow up says
what it read, `Return` saves, and emptying a field clears it.

At the follow-up time, if the task is **still open and still in Waiting on**,
a task called "Follow up with Sam: Contract signed" (or "Follow up: Contract
signed" with no tag) lands in **Today**, due at that time, beside the task it
chases; its inspector links back. It is made once — deleting it does not bring
it back, and setting a new time makes a new one. A task that leaves Waiting on
before its time gets no follow-up; one that leaves after it leaves the
follow-up where it is, for you to finish. A time already past makes it at
once. The app checks every quarter-minute while it runs, and on the first poll
of a new day; Android checks too, and both give the follow-up the same id, so
a phone and a Mac that both notice it make one task between them.

**Chasing again.** With a repeat, completing the follow-up while the task is
still open and still in Waiting on sets its next follow-up time: that many
days after the later of when you completed it and the time it was for, at the
same time of day. Done on Saturday evening, a Thursday 14:00 follow-up that
repeats every 3 days comes back on Tuesday at 14:00. That one is made like
the first, and numbered: "Follow up #2 with Sam: Contract signed", then #3.
A follow-up you never complete makes no more, so they do not pile up;
cancelling or deleting one stops the chain there, and so do completing the
task itself, taking it out of Waiting on and clearing it. Completing the
follow-up and moving the time on are one step, so one undo takes back both.
The card shows which follow-up is next and how often:
`↻ Tue 14:00 · #2 · every 3d`. The count belongs to one spell of waiting:
naming someone else, or clearing both who and when, starts again from the
first. The add field's `waiting on Sam` stays as it was; the repeat is set in
the form, the inspector (a **Repeat** menu on the Mac, Once / Every few days
on Android, and on the iPhone, with a − / + stepper that moves a day at a
time up to a week, then a week at a time) or from `takt`. The iPhone's
inspector section takes who, a follow-up time (off, a quick Tomorrow 9:00, In
3 days or Next week, or a date and time picker) and the repeat, and saves them
with its own **Save** — **Move to Waiting on** when the task is elsewhere —
or empties them with **Clear**; its outline rows, Today cards, board cards,
search results and inspector header show the tag and the `↻` time.

The inspector edits all three in place under **Waiting on**. From `takt`,
`takt ws update <id> --waiting-on Sam --follow-up "2026-10-08 14:00" --every 3`
does the same (the MCP tool takes `waiting_on`, `follow_up_at` and
`follow_up_every_days`; `--every 0` or null is once); the app makes the
follow-up when it next looks.

## Blocked by

A task can wait on other tasks, in any list. `bb` (or `Cmd+Shift+B`, or
**Blocked by** in the palette) on a task opens a finder over every open task;
`Return` makes the one you choose a blocker. When the task it waits on does
not exist yet, the last row is **New task** with what you typed — `Return` on
it, or `Cmd+Return` from anywhere in the finder, writes it beside the task
being blocked (same list, same parent, just above it, since it has to be done
first; on the board, in the same column) and makes it the blocker, in one undo
step. The title is read the way the add field reads it, so `!1`, `@fri` and
`45m` work there too. Until every task it waits on is
done or cancelled, the task is **blocked**: its title is greyed in the outline,
on the board, in the matrix and on Today, it sinks below its unblocked
siblings (above any buried ones), and it is never offered as next up.
Finishing the last blocker puts it back where it was.

The inspector's **Blocked by** row lists what the task waits on — open or
done, with the list when it is another — each with a button to stop waiting on
it, and under **Blocks** the tasks waiting on this one. Adding and removing are
each one undo step. A task cannot wait on itself, or on a task that already
waits on it, however far round; the app says so rather than adding it.

## Shared lists

A list can be shared with other Takt accounts. Everyone in it sees its tasks and subtasks on every device they sync, and can add, edit, complete and delete them; edits merge the way edits between your own devices do. What stays each person's own: where the list sits in their sidebar, its tasks' board columns and matrix placement, their conditions for a task and its agent context. Nothing else in a workspace is ever shared.

| Where | How |
| --- | --- |
| Mac | **Share the list** in the command palette (or the list's row in Settings → Shared lists) copies an invite link; each run makes a new one. **Join a shared list** takes a link or code, and is filled in from the clipboard. **Show who the list is shared with**, **Leave the shared list** and **Stop sharing the list** do what they say. A shared list has the two-people icon in the sidebar. |
| iPhone, Android | Long-press the list → **Share…** for the people in it, an invite to send, and leaving or stopping sharing. **Join a shared list** is in the Lists toolbar. |
| Terminal | `takt ws shared` lists the shared lists and who is in them; `takt ws tree` marks them; `takt ws update <id> --assign sam@example.com` (or `me`, or `""` for nobody) assigns a task. Sharing itself happens in the apps. |

A task in a shared list says who added it and who finished it, and can be **assigned** to someone in the list: **Assign the task to someone in the list** on the Mac (or the inspector's *Assigned to* row, ← and → to step through the people), the inspector's **Shared** section on the iPhone and Android. A task assigned to someone else stays out of your Today, next up and focus; one assigned to nobody is anyone's.

An invite works once, for seven days, and goes as a link (`…/join/<code>`, which opens Takt) or the code typed in. Only the list's owner invites. A member can leave, and can't delete the list; the owner stops sharing instead, which keeps the list for them and takes it off everyone else's devices. If the owner deletes their account, the longest-standing member becomes the owner. The Inbox can't be shared, and sync has to be on: shared lists travel through it. Every device in a shared list should be on a version from November 2026 or later — an older one shows the list but its edits stay on its own account. How it works is in [docs/shared-lists-plan.md](docs/shared-lists-plan.md).

## Repeating work

Give a task a **period** — `daily`, `weekdays`, `weekly`, `every 3 days`,
`every 2 weeks`, `every monday` — and finishing it schedules the next one.

The occurrence you did stays done. It keeps the day it was finished on and
counts towards the week like any other completed task, and a *new* task is
written for the next time round, dated by stepping the cadence from the dates
this one carried. A single row flipped back to open could only ever record the
last time you did it, which is a week of work that reads as a week of nothing.

Missing a few is not a reset: the cadence steps until it lands in the future,
so `every 3 days` ignored for a fortnight comes back on its own grid rather
than arriving already overdue. Nothing pushes the repeat into today — it
carries a start date, and **the day already claims whatever starts today**.
That pair is the whole of the autoscheduling; there is no third mechanism and
nothing to turn on.

The repeat arrives without a column or a ladder position, because a plan for a
day nobody has made yet is not a plan.

## The menu bar

The status item is a **standing reminder**, not a launcher. It names the next
thing on today — or, while a block is running, that task and its clock — and
clicking it drops the day down as a menu you can start any task from without
the app ever coming to the front. Overdue work is coloured; the tick marks
whatever is running.

It also **outlives the window**. Closing the windows (`⌘W`) leaves the status
item behind, and the app with it; `⌘Q`, or **Quit Takt** in either menu, quits.

The Mac's own menu bar along the top of the screen has **Takt** (About,
Settings… `⌘,`, Hide `⌘H`, Quit `⌘Q`), **File** (Quick Add…) and **Window**
(Main Window, Focus Panel), so everything the keys reach first also has
somewhere to click.

## The window

The window is the app. It opens on Today, carries the mode strip and the add
field in its title bar, and everything you can do you can do here. It is laid
out the way an editor is — Zed's layout, specifically: flat surfaces split by
hairlines, every column topped by a header band of one height so a single rule
runs straight across the window under all three, and exactly one strip along
the bottom.

- **Title bar** — the window's title after the traffic lights, the mode strip
  (words only), and **Add a task** on the right (`a` or `Cmd+N` to reach it).
  It names where the task will land: the list on screen (inside the task you
  have opened, if any), the folder's chosen list (`Cmd+Alt+[` / `]` to change
  it), or the **inbox** when no one list is on screen — Everything included.
  On Today it also puts the task on today, and says so (`Today · Inbox`).
  Pressed on a task, `a` / `Alt+Return` / `Alt+Shift+Return` place it below,
  above or inside that task instead.
  Endings like `30m`, `@fri`, `#work` and `!1` set the task's estimate, due
  day, tags and priority, and show as chips before you press Return — see
  [Typing a task](#typing-a-task).

- **Left dock** — two tabs, **Lists** and **Agent**, in a tab bar like the
  right dock's; its tab, visibility and width are remembered.
  - **Lists** is the sidebar of lists and folders. Lists only: Focus and the
    timeline are in the mode strip, not repeated as sidebar rows. The tab bar
    carries its actions as small glyphs — new list (`Cmd+Shift+N`), new folder
    (`Cmd+Alt+N`), archived lists to restore (only while there are any), and
    undo/redo — rather than a bar along its foot; hover one for its name and
    key.
  - **Agent** (`Cmd+Shift+A`) is an assistant panel in the manner of Zed's: your
    own **Claude Code** or **Codex**, run headless, with Takt's MCP server as
    its only tools — no shell, no files, no web, and no API key (it uses your
    own login). Choose which with **Agent** in Settings ▸ AI (which marks any
    that isn't installed), or with **Use agent: Claude Code** / **Use agent:
    Codex** in the palette; the choice is kept, and the quick ask uses it too.
    The dropdown also lists each of your **Claude Code profiles**: every
    `~/.claude-<name>` folder and every `claude-<name>` command in
    `~/.local/bin`, Homebrew or `/usr/local/bin`. A profile with its own
    `claude-<name>` command runs that (so a wrapper script works, since apps
    get no login shell and so none of your aliases); one without runs `claude`
    with `CLAUDE_CONFIG_DIR` set to its folder. A thread keeps the agent it
    started with — switching mid-thread says so in the transcript, and the
    next thread (New thread, or after Stop) uses the new one. It **reads freely** — the lists, tasks, dailies, the day log
    and the focus timer — and each read shows as one muted line in the
    transcript. Its **changes apply straight away** — adding, editing,
    completing, moving or deleting anything — and each shows as a card saying
    what it changed (the tool, the title, the list by name, and what a field
    held struck through above what it holds now) with an **Undo** that takes
    back exactly that change, leaving later ones alone; when a later change
    built on it, the button says it will take those back too ("Undo with 1
    later change"). Once undone the card says so, with **Redo**. Tab from the
    field reaches the newest such card, Return presses its button and Esc
    returns to the field; the assistant is told what you undid. To approve
    each change before it runs instead, turn on **Ask before changing tasks**
    in Settings ▸ Agent (off by default; from the next thread): then every
    change stops as a card with **Approve** and **Deny**, Return approves with
    the card holding the keyboard and Esc denies, and stopping the thread
    withdraws an unanswered card. Codex cannot stop to ask, so with that
    setting on a Codex thread is given only the reads and says what it would
    change instead. Each message also tells it the tasks on
    screen with their ids (the first 30), so "these" and "the top three"
    work. Return sends, Shift+Return is a new line, Esc hands the keyboard
    back to the tasks; the glyphs on the tab bar start a new thread and stop
    the current one. Every change is also an ordinary "MCP: …" step in the
    Undo menu. If the chosen agent is not installed where Takt
    looks (for `claude`: `~/.local/bin`, `~/.claude/local`, Homebrew,
    `/usr/local/bin`; for `codex`: Homebrew, `/usr/local/bin`, `~/.local/bin`,
    `~/.npm-global/bin`), the panel says so, takes a path, and offers the
    other agent if that one is installed; if it is not signed in, too old or
    out of usage, the error says what to run. **Standing instructions** can be written
    for the whole account (Settings ▸ Agent), for a folder and for a list (in
    its settings, `oo`); a message carries the account's, then each enclosing
    folder's, then the list's, and only when they changed since the last
    message in the thread. The line above the field names the ones in effect
    where you are, and clicking one opens it to edit. The agent keeps them
    too: tell it something lasting ("in this list, titles start with a
    verb") and it saves it to the narrowest scope that fits, as a change like
    any other, showing the instructions before and after. They
    live in the workspace, so they sync, and `takt ws instructions` reads them
    from a terminal. Every change card also says **which keys would have made
    the same change** — "Keys `D D` then type “friday”", "Keys `M M` then
    choose the list", "Keys `Space`" — worked out by the app from the call and
    your own keymap, so a rebound key shows as rebound; a change no key makes
    (a daily, a filter list's rules removed) shows none. The last
    [quick ask](#quick-ask) sits at the top of the panel, its cards undoable
    there too. See [`docs/agent-panel.md`](docs/agent-panel.md).
- **Main pane** — a header band with where you are on the left (the scope, and
  the way back out of an opened task) and a count or a few glyph buttons on the
  right, then the mode's content. Panes carry no footers of their own: their
  key hints are in the reference (`Cmd+/`) and the palette (`Cmd+K`).
The docks split by job: the left one is for getting about, the right one
helps you finish the task in front of you, and the bottom one keeps track of
what has been done.

- **Right dock** — the inspector and the conditions, as tabs of one resizable
  column, in a tab bar: the showing tab in ink with a rule under it and a close
  glyph at its right. Its visibility, width and tab are remembered, and it does
  not close itself when you navigate; with nothing selected the inspector says
  so.
- **Bottom dock** (`Cmd+J`) — under the main pane, Progress, Done and Timeline
  as tabs, in the right dock's tab bar with the showing tab's own figures and
  controls at its far end. Progress is a bar per day for the tasks closed and a
  line for the tasks added, over 7, 30 or 90 days, with the totals and the net
  on the tab bar (hover a day for its own figures). Done is the rail of what
  you finished, by day, with today's tally on the tab bar. Timeline is the
  day's focus blocks across the hours. Its visibility, height, tab and period
  are remembered; Done and Timeline take the keyboard and are stops for
  `Ctrl+Tab`, the graph has nothing to step through and is not.
- **Status bar** — a thin strip along the foot, glyphs at its ends like an
  editor's. On the left, the left dock's Lists and Agent toggles and the
  bottom dock's Progress, Done and Timeline toggles (the Lists
  tooltip says which region has the keyboard); in the middle, a half-typed key sequence (`d…`), an error,
  or a passing message; on the right, the running block and its clock (click to
  return to it), today's time logged, points and tasks done (the week's in the
  tooltip; click for the timeline), what is available now (click for the
  Conditions tab), Google Tasks sync, the history — what `Cmd+Z` or
  `Cmd+Shift+Z` just undid or redid ("Undid Edit Task"), else the last thing
  done; click it for the list of steps, and choose one to go back to it (or,
  for a step undone, forward to it) — and at the right edge
  the right dock's Inspector toggle — each toggle on the side its
  pane opens on.

A selected row is a quiet fill; the row the keyboard is on adds a one-point
line in the focus colour, so which pane will answer the arrow keys is always
visible without a thick ring. Empty panes say so in one line of muted text in
the middle.

Settings is `Cmd+,` and the app menu; there is no gear in the window.

It used to be the second-class half of a pair. The first-class half was a
400pt menu bar panel that dismissed on any outside click — right for a glance,
wrong for half an hour of reorganising — and the window existed so the same
views had somewhere to live that survived being clicked away. That panel is
gone; what is left of the menu bar is a standing reminder and a way in.

Open it from **the status item's menu → Open Main Window**, from **Window →
Takt** in the menu bar, or with the global hotkey. It is resizable, remembers its
frame, and `Esc` leaves it alone.

**A Dock icon appears while the window is open** and goes again when you close
it — unless **Show in the menu bar** (Settings ▸ General, on by default) is
off: then there is no menu bar item, and the Dock icon stays while the window
is closed, so it is still the way back. That is what buys you `Cmd-Tab`, `Cmd-W`, `Cmd-Q` and — the one you notice
if it is missing — an Edit menu, without which copy and paste do nothing in
the quick-add field.

The **global focus panel** is the one surface that is not the window: it is
the same day list, summoned over whatever app you are in, and it keeps working
with every window closed.

### Quick ask

`aa` on a task (or **Ask the agent to do something to the task** in the
palette and the Task menu) opens a one-line popup over it, the way `bb` and
`cc` do: "Tell the agent…", with the task named on the right. Return sends it
to the agent with that task as the subject — its title, id and list — so "make
it due friday", "split this into steps" or "move it to Errands" needs no
looking up. It runs as a one-shot thread of its own, so the panel's
conversation is left alone. The popup shows the work as it happens, then the
short reply and a card for each change, with its Undo and the keys that would
have done the same. Tab (or ↓) selects a card and Return presses its button —
Undo, Redo, or Approve with **Ask before changing tasks** on, when ⌫ denies.
Esc closes the popup at any point without stopping anything; `aa` again
brings back an ask still under way, and the last one stays at the top of the
agent panel.

## Themes

Pick a look in **Settings → Appearance**, from a gallery that draws each theme
in its own colours and faces: **Takt**, the default — set as the Mac app is, in IBM Plex
Sans with square panels and hairlines, warm paper in the light and a deep slate in the dark — which follows
your Light/Dark setting; **Zed** and **Zed Dark**; and **Grape**, the house design
language — chalk paper, grape ink and the Arvo slab serif. Light, dark or
system is chosen beside it.

**Fonts are yours, over any theme.** Appearance sets the interface, heading and
numeral/code faces and a text size (85–130%, scaling every size the theme sets
in proportion). Each picker lists the theme's own face, the families Takt
ships — Inter, Geist, IBM Plex Sans, Arvo, Lilex, JetBrains Mono, Geist Mono —
and every family installed on the Mac, searchable and each set in itself.
Switching theme keeps your choices; **Reset to theme** hands them back.

Your own themes are JSON files in
`~/Library/Application Support/Takt/themes/`, and they appear in the same
gallery. A file can extend a built-in and change only a few colours or sizes,
and the app reloads it as you save, so the easiest start is **Duplicate current
theme** on the Appearance page, or **Export the current theme as JSON** in the
command palette — it writes a complete copy, opens it and switches to it.
**Open the themes folder** and **Reload** are under Advanced. A broken file is
reported on the status line and in Diagnostics, never fatal.

**Zed themes work as they are.** Drop a Zed theme file (the kind Zed keeps in
`~/.config/zed/themes/`, or exports from a theme extension) into the same
folder, and each theme in it — Zed files are families, usually a dark and a
light — appears in the gallery under its own name, fixed to its appearance, in
Zed's type and square panels. On the Mac, the themes already in Zed's own
`~/.config/zed/themes/` are listed too, read-only and on that Mac only; copy a
file into Takt's folder (or import it on a phone) to sync it.

**Format and a full example: [docs/themes.md](docs/themes.md)**

## Diagnostics

**Show diagnostics** in the command palette, or **Workspace → Diagnostics**. It opens as
a sheet on the main window and answers "why does this look wrong?":

- **Status** — connection, current list, network, sync age, open task count, and whether anything is queued offline.
- **Health** — a green tick or an orange triangle per integration, with the detail underneath. The MCP row shows the resolved command; the Google Calendar and Google Tasks rows show whether the shared Google sign-in covers them.
- **Recent problems** — every failure this session, timestamped. Worth having because nothing else keeps one: the error line is overwritten by the next thing that fails, the status message erases itself after three seconds, and integration errors were not retained at all. Not written to disk — it covers this run of the app, not last fortnight's.
- **Data** — where everything lives, with a Reveal button each, including `~/.config/takt/config.json`, which belongs to the CLI rather than the app and is a recurring source of confusion.

**Copy Report** and **Export Report…** produce plain text you can paste into an
issue. Both run it through a redactor first: anything labelled like a
credential, and any bare token-shaped run, comes out as `<redacted>`. Check it
before you post it anyway.

## Daily log

What the app writes down about a day, and what reads it back.

### Dailies
A daily is not a kind of item and not a view. It is a requirement placed on
an ordinary task: contribute to this every day. The task keeps its place in
its list and gains a badge on its card in Today that you tick to record the
day's contribution, without starting a block.
Recurring things you intend to do — habits, not tasks — sitting at the top of the view as a checklist.

- **They reset at every rollover and never go overdue.** Miss one and it's a gap in the history: nothing to clear, nothing to reschedule. That's the whole reason they aren't Checkvist tasks with a `repeat daily` rule — a recurring *task* goes overdue and starts competing with real deadlines.
- **They're local.** Stored in `~/Library/Application Support/Takt/dailies.json`, so "brush teeth" never clutters your project lists or syncs to other Checkvist clients. Ticking one is instant and works offline.
- **Ticks land in the same log as task completions**, so they appear in the Obsidian note alongside them. The chart plots the ticks alone: a day's task count is whatever happened to be on the list, and summing the two in let it swamp the routine the chart sits under.
- **Two kinds of schedule.** Fixed weekdays (`Mon Wed Fri`, weekdays, weekends, every day) or a rotating cycle — every other day, every three days — counted from the day you set it. A cycle walks through the week, so it's the one for "water the plants", not "standup".
- **Set the schedule as you type** from the menu in the add field, or edit any daily in full — day-by-day toggles, cycle length — in `Settings → Daily Log`.
- **Rename in place with `a`, delete with `Delete`**, without leaving the checklist. Deleting *archives*: the row goes from today's list and from the editor, but every past day that ticked it still renders with its title rather than a raw id, and it can be restored from the Deleted list in `Settings → Daily Log`. That is why deleting needs no confirmation — nothing has been lost.

### Habits

`gb` (or `Cmd+Shift+H`) opens the habit form. On a task it makes a habit *from*
that task — select "Learn drums" and save "Practise drums" — which goes into the
Habits list with a daily attached, so everything above about dailies holds for
it. On a task that already has a daily it edits that daily, and with nothing
selected it makes a habit of its own.

| Row | What it sets |
| --- | --- |
| Habit | The name, prefilled with the task's title |
| How often | Every day, on chosen days (`1`–`7` toggle Mon–Sun), every N days (type N), or weekly — counted from the day you make it |
| Appears in | The board column each appearance lands in: Today, This week or Waiting (the `today`, `this-week` and `waiting-on` columns) |
| End of day | **Disappears**: a missed day is a gap. **Stays until done**: it is carried, in its column, until you tick it |
| Estimate | `30m`, `1h`, `1h30` or minutes — each appearance's target, and the task's estimate |
| Ends | **When the task is done** (the default for a habit made from a task), **on a date** (`2026-12-31`, `3w`, `friday`; it last appears the day before), or **never** |

`Tab`, `Shift+Tab`, `↑` and `↓` move between rows, `←` and `→` change a choice,
`Space` flips the end-of-day toggle, `Return` saves from any row and `Escape`
cancels.

Each day the app puts a due habit into its column and takes it out once it is
ticked, dropped at the end of its day, or ended. `Space` on a habit ticks it
for the day rather than completing it. A card you move to another column by
hand is left where you put it. Completing the task a habit came from ends the
habit straight away — undo brings it back — and the same is true when the
task is completed from `takt` or an assistant.

### Committing to a task periodically

`Cmd+Shift+D`, or **Commit periodically…** under the inspector's Repeat
row, commits you to working on the selected task on a rhythm. It opens a
small form of controls rather than flipping a switch; its header names the
task the commitment's subtask goes under and whose completion ends it.

| Row | What it sets |
| --- | --- |
| Title | What turns up in Today and Dailies, prefilled as "Work on *task*"; a title is required |
| Repeat | **Daily**, **Weekdays**, **On days**, **Every *n* days** or **Every *n* weeks**. `←` `→` choose, digits type the count (on a choice without one they mean days), `+` `-` step it, `d` and `w` pick days and weeks |
| On | With **On days**: the day chips, Mon to Sun. `←` `→` move, `Space` (or `1`–`7`) ticks a day; the last one ticked stays |
| Until | **No end** (it runs until the task is done) or the last day it appears: `←` `→` step a day, `Shift+←` `Shift+→` a week, never before today; `x` goes back to no end |
| Missed | The switch "A missed day stays owed until done" (**Stacks**), off by default (**Vanishes**); `Space` flips it |

`Tab` / `Shift+Tab` or `↓` / `↑` move between rows (on the Title row the
letters are typed, and `↑` `↓` still move), `Return` saves, `Esc` cancels, and
every chip, row and button can be clicked. Under the rows, **Next** shows the
next four days it would appear (an every-*n*-days rhythm already saved counts
from the day it was made, so it shows the rule instead).

Saving adds a subtask under the task, carrying a habit whose source is the
task: it lands in Today on its days and appears in Dailies, where ticking it
records the day's contribution and leaves both tasks open. What happens to a
missed appearance is the **Missed** row's:

- **Vanishes** — as commitments always behaved: a daily rhythm drops a missed
  day at its end, and a weekly one, or one day a week, stays until the next one
  replaces it.
- **Stacks** — each missed appearance stays owed. The subtask stays in Today
  while any scheduled day since you chose it is undone, and each tick pays off
  the oldest, so three missed days take three ticks. Switching it on does not
  reach back before the day you switched; switching it off forgives what is
  owed. The inspector shows how many are missed.

The subtask is only there when there is something to do. Completing it —
`Space`, its checkbox, ticking it in Today or Dailies, or an agent's
`task_complete` — ticks today rather than finishing it, and once nothing is
due today it *rests*: the outline, the boards (Today's column included), the
filter lists and next-up leave it out until its next scheduled day, when it
comes back on its own. Today not being one of its days rests it too (a
weekdays commitment is gone on Saturday). With **Stacks** it stays while
anything is still owed. Undoing the tick brings it back straight away. A
resting commitment is still listed in its parent's Repeat row, marked
**✓ today** once today's is ticked, and in Dailies on its days, ticked.

Cancelling the subtask — `Shift+Space`, or an agent setting it `cancelled` —
skips today instead. The day is set aside without a tick, the subtask stays
open and the commitment live, and it rests until its next scheduled day as a
ticked one does; its parent's Repeat row says **skipped today**. A skipped day
is not a missed one, so with **Stacks** it is not owed (with a missed day
still owed, a skip sets aside the oldest, as a tick pays it off). Undo brings
it back straight away. Only **Stop** and completing the parent end a
commitment.

The task's inspector lists its commitments with **Edit** and **Stop**; stopping
keeps what was logged and closes the subtask. Completing the task ends every
commitment on it, whatever its end date, and closes their subtasks — undo
brings them back. On a task that has a commitment, `Cmd+Shift+D` opens it for
editing (`Cmd+Delete` in the form, off the Title row, stops it); on a task that is still a daily
itself, from before commitments, it stops that daily.

### What the day records

- **Recording is always on and always local.** Completions, reopens, invalidations, finished focus sessions and the day's plan are appended to `~/Library/Application Support/Takt/daylog.jsonl` — one JSON object per line, so it stays readable with `tail`, and a torn write costs one event rather than the file.
- **Checkvist owns current state, the log owns history, Obsidian owns the archive.** Nothing syncs backwards, so there is no conflict resolution anywhere in this.
- **The day's plan is derived, not authored.** The first time you open the app after your rollover hour, whatever is due, overdue or starting that day is snapshotted. That's what the day's note measures its "N of M planned left" against — you never plan a day by hand.
- **Deferring is not slipping.** Pushing a due date forward is recorded distinctly from letting a task rot, so the view doesn't nag about a decision you made deliberately.
- **The day starts at your rollover hour, not midnight** (default 04:00), so a session finishing at 01:30 counts towards the day it belonged to.
- **No backfill.** History starts the day you first run this build. The chart is drawn from day one regardless — a flat run of days is a true statement about a history that has just started — with a "collecting since" line underneath until the window fills.

## Obsidian daily notes

In `Settings → Daily Log`, point Takt at your dailies folder and set the note naming to match your vault (`yyyy-MM-dd` by default; a subfolder pattern like `yyyy/MM` nests them). The preview line shows exactly which note today's block would land in. Then switch on "Write days into Obsidian daily notes", which stays disabled until a folder is chosen.

Once a day closes, its block is spliced into that day's note:

```markdown
<!-- priority:begin -->
## Log

**5 done** · **2/3 dailies** · **1h 40m focused** · 2 of 7 planned left

_Dailies:_
- [x] Read
- [x] Walk
- [ ] Stretch

- [x] Ship the DMG
- [x] Review the sync PR

_Unfinished:_
- [ ] Write release notes
<!-- priority:end -->
```

Only the text between the markers is ever touched, and rewriting a day replaces its own block rather than stacking a second one. **Creating missing notes is off by default**, so the plugin can't beat a Templater or Daily Notes template to the file — turn it on only if nothing else builds your dailies.

## Command line

`takt` is a Rust CLI covering the same ground: your lists, your dailies, your day log. It talks to the Checkvist API directly and reads Takt's local files off disk, so it works whether or not the app is running — and its writes take the same `flock(2)` the app does, so both can be open at once.

Run it with no arguments and it opens a **terminal UI with the same tabs as the app**, and the same keys to reach them:

```
 All q │ Due w │ Tags e │ Priority r │ Kanban t │ Matrix y │ Daily d
┌ All ───────────────────────────────────────────────────────────────┐
│▎[ ] Ship v0.4                                                      │
│   [ ] Draft the release notes  #work                               │
│   [x] Tag the commit                                               │
│ [ ] Buy milk  #home                                                │
└────────────────────────────────────────────────────────────────────┘
 j/k move · l/h in-out · space done · a add · ? help · esc quit
```

Or drive it by subcommand:

```bash
./scripts/install_cli.sh     # release build + a `takt` symlink onto your PATH
takt auth login

takt                         # the terminal UI
takt tasks
takt add Draft the release notes --due friday
takt search -q report --due-before 2026-09-01
takt daily add Read for twenty minutes --weekdays mon,wed,fri
takt daily add Water the plants --every-days 3
takt log --days 7
takt --json dailies | jq '.dailies[] | select(.done | not)'
```

Its credentials are its own, in `~/.config/takt/config.json` at mode 0600 — separate from the app's login-keychain item, so neither depends on how the other was built or signed. The dailies, log and metadata commands need no credentials at all.

Every command is one of the MCP tools under a friendlier name, and the same binary serves them over MCP with `takt mcp`. It is also the app's MCP server: Takt.app ships this binary at `Contents/Helpers/takt`.

**Full guide: [docs/cli.md](docs/cli.md)**

## MCP server

Takt exposes **45 MCP tools** so an AI assistant can work with your lists directly: the workspace's folders, lists, tasks, blockers, filter lists and board columns, the local state Checkvist has no representation for (day log, dailies, priority ranks, recurrence, the matrix), and the Checkvist API. [`docs/mcp-server.md`](docs/mcp-server.md) lists them.

All but one are reads or Checkvist writes. `task_matrix_set` places tasks on the Eisenhower matrix in bulk (`takt matrix <id>:<urgency>:<importance> …` from a terminal), and refuses while Takt is running — the app holds those coordinates in memory and would overwrite them. Quit Takt, let the assistant do a first pass over the whole list, then reopen and correct it by dragging.

Set it up from `Settings → AI → MCP`. It detects Claude Code, Claude Desktop, Cursor, Windsurf, VS Code and Zed, and adds Takt to the one you pick in a single click, preserving any servers already in that client's config.

There is one implementation — the Rust CLI — and the app ships it at `Contents/Helpers/takt`, so it works whether or not you installed the CLI separately. `Takt --mcp-server` hands the process over to it, which keeps configurations written for older versions working; one written before the rename names `Priority.app`, and setting that client up again replaces its `priority` entry with a `takt` one. There were three implementations once; `scripts/mcp_smoke_check.py` is what remains of holding them together, and it now only checks that handover.

**Full guide: [docs/mcp-server.md](docs/mcp-server.md)**

## Plugins

Every external integration is a plugin behind a protocol.

Built in: `NativeCheckvistSyncPlugin`, `NativeGoogleCalendarIntegrationPlugin`, `NativeMCPIntegrationPlugin`, `NativeDailyLogPlugin`.

To install your own, open `Settings → Installed plugins` and click **Install plugin…** (folder, `.zip`, or `.priority-plugin`), or drop a plugin folder into `~/Library/Application Support/Takt/Plugins` and hit **Reload**.

Built-in plugins are fully functional; user-installed plugins are manifest-driven (settings, metadata, lifecycle) and prepared for runtime capability wiring.

**Authoring guide: [docs/plugins.md](docs/plugins.md)**

## Build from source

Requirements: macOS 15.6+ and [Rust](https://rustup.rs). Xcode 17+ for the
iPhone app and the Swift package tests.

```bash
git clone https://github.com/MaybeItsSoftware/takt.git
cd takt

# The Mac app (Rust, on GPUI): build, bundle, install as /Applications/Takt.app
./scripts/install_local.sh

# Its tests, and the shared core's
cargo test --manifest-path desktop/Cargo.toml
cargo test --manifest-path core/Cargo.toml

# The CLI — also the app's MCP server, bundled into Takt.app
cargo test --manifest-path cli/Cargo.toml

# `Takt --mcp-server` still reaches it (after install_local.sh, or
# TAKT_BUNDLE_ONLY=1 scripts/install_desktop.sh)
python3 scripts/mcp_smoke_check.py

# The Swift package the iPhone app links
scripts/build_core_apple.sh && swift test

# A signed, notarised DMG
./scripts/build_desktop_dmg.sh <version>
```

### Layout

| Path | What |
| --- | --- |
| `desktop/` | The Mac app, in Rust on GPUI ([plan](docs/rust-desktop-plan.md)) |
| `core/` | `takt-core`, the shared Rust core every client calls |
| `cli/` | The `takt` CLI, terminal UI and MCP server |
| `mobile/ios`, `mobile/android` | The iPhone and Android apps |
| `Sources/` | The Swift package the iPhone app links: `TaktCore`, `TaktWorkspace`, `TaktWorkspaceEditing`, `TaktSync`, and `TaktRustCore` (the core's generated bindings) |
| `sync-server/` | The sync server ([protocol](docs/sync.md)) |
| `shared/` | Fixtures and assets every client shares: themes, fonts |
| `scripts/` | Build, install, release, and the MCP smoke check |
| `docs/` | [CLI](docs/cli.md) · [MCP](docs/mcp-server.md) · [sync](docs/sync.md) · [self-hosting sync](docs/self-hosting.md) |

The Swift Mac app that used to live under `Takt/` was retired on 2026-11-08;
git history holds it.

## Where your data lives

| Path | What |
| --- | --- |
| `~/Library/Application Support/Takt/` | Dailies, day log, task cache, installed plugins |
| `~/Library/Preferences/uk.co.maybeitssoftware.takt.plist` | Settings, priority ranks, recurrence rules, start dates |
| Login keychain, service `uk.co.maybeitsadam.priority` | The app's Checkvist remote key (the service keeps its pre-Takt name) |
| `~/.config/takt/config.json` | The CLI's own credentials, mode 0600 |

Nothing is sent anywhere except Checkvist, and Google Calendar, Google Tasks or Google Drive if you enable them (Obsidian daily notes are written to a folder on this Mac).

Google Drive linking reaches only the files you pick. "Pick from Google Drive" (in the Mac's link picker, `pl`, and under the links in the iPhone and Android inspectors) opens Google's own file picker; Takt is granted the files you choose there and nothing else (the `drive.file` scope), and links each one by its Drive address, name and type. Pasting a Drive address needs no Google access at all. See [Links](docs/links.md).

Google Tasks, when enabled, is a **mirror**: each list becomes a Google Tasks list of the same name, and Takt is the source of authority. Ticking a task off on your phone completes it here, notes you add there are kept, and a task you type there is adopted — but an edit that overwrites what Takt holds is replaced and written to a conflict log. Google Tasks keeps a day but no time, so with **Read times from task titles** on (off by default) a task typed there with a time or range in its title ("Call Sam 14:00–15:00", "Gym 2-3pm") comes in scheduled for then, the range as its length. Turning Google Tasks off leaves its lists in Google as they were; **Remove Takt's lists from Google Tasks** on the Google page (shown while any are there) deletes them, after you type "remove", and turns Google Tasks off, changing nothing in Takt, so that switching it on again mirrors afresh. `docs/google-tasks.md` has the full table.

> Upgrading from **Priority** (or **Bar Tasker** before it)? Everything is carried across automatically on first launch — preferences, dailies, the day log, the workspace database and your keychain item. The old locations are copied rather than moved, so they stay on disk until you delete them.

## License

MIT
