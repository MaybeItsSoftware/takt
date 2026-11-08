# MCP Server Guide

Takt ships an MCP stdio server so an AI assistant can work directly with your Checkvist data.

- Server command: `takt mcp` — the CLI, which the app ships at
  `Contents/Helpers/takt`. `Takt --mcp-server` also works and hands over
  to it; see [One server, two ways to name it](#one-server-two-ways-to-name-it)
- Transport: stdio, newline-delimited JSON — one JSON-RPC object per line, as the
  MCP stdio transport specifies. LSP-style `Content-Length` framing is also
  accepted, and replies mirror whichever framing the client used. A
  `Content-Length` over 64 MiB, or one that isn't a number, gets a `-32700`
  parse error rather than a body that size.
- Dependencies: none beyond Takt itself — the server ships inside the app

## What It Can Do

The server exposes 51 MCP tools, in three groups.

**Checkvist tools** — these reach the Checkvist API directly, so they work
whether or not the app is running:

| Tool | What it does |
|---|---|
| `task_lists` | List non-archived checklists |
| `list_create` | Create a checklist |
| `task_fetch` | Fetch a list's tasks (open only by default) |
| `task_search` | Filter by content substring, tag, and/or due date |
| `task_add` | Quick-add at the root or under a specific parent |
| `task_update` | Change content and/or due |
| `task_note_add` | Append a note (a Checkvist comment) |
| `task_move` | Reorder among siblings (1-based position) |
| `task_reparent` | Move under a different parent, or to the root |
| `project_move` | Move a root task and its full subtree to another list, with an old-to-new ID map |
| `task_complete` | Close a task |
| `task_reopen` | Reopen a task |
| `task_invalidate` | Mark "won't do" |
| `task_delete` | Delete a task |

**Local tools** — these reach the state Takt keeps on this machine, which
Checkvist has no representation for:

| Tool | What it does | |
|---|---|---|
| `daily_log_fetch` | What happened on recent days: completions, focus time, unfinished/deferred tasks, daily ticks | read |
| `dailies_list` | Configured dailies with today's schedule and tick state | read |
| `task_metadata` | Priority ranks (scoped and absolute), recurrence rules, start dates, Eisenhower urgency/importance, kanban columns | read |
| `task_matrix_set` | Eisenhower urgency/importance, in batches | write, **only while the app is closed** |
| `daily_add` | Create a daily | write |
| `daily_update` | Rename, reschedule, archive/unarchive a daily | write |
| `daily_tick` | Tick or un-tick a daily for today | write |

**Workspace tools** — these reach the app's own database,
`~/Library/Application Support/Takt/priority.sqlite` (the old `Priority/` one
while `Takt/` does not exist yet), which is the app's
source of truth. It is the tree the app actually shows, not the Checkvist edge of
it, and the only part of the server that touches it. Ids here are the
workspace's uppercase UUIDs, not Checkvist's integers:

| Tool | What it does | |
|---|---|---|
| `focus_status` | What the focus timer is doing: the task, paused or running, elapsed and planned seconds, and the queue behind it | read |
| `focus_history` | Focused time already recorded over the last N logical days, newest first | read |
| `workspace_tree` | Folders, lists (with `folder_id`, open-task counts, visible root) and nested lists | read |
| `workspace_tasks` | One list's task tree: ids, titles, notes, status, kind, kanban column, external links, `requires` (its conditions by name), `blocked: true` on a task waiting on an open task | read |
| `workspace_task_dependencies` | What a task waits on (`blocked_by`) and what waits on it (`blocking`), each with id, title, status and list, and whether it is blocked now | read |
| `workspace_task_add` | Create a task or nested list, at a list's top level or under a parent, with notes, links and a column | write |
| `workspace_task_update` | Title, notes, links, status, column, kind (task ⇄ nested list), sidebar pin, `waiting_on`, `follow_up_at` (both file the task in Waiting on) and `follow_up_every_days` (chase again that many days after each follow-up is done), `due`, `start`, `priority`, `estimate_minutes` and `assignee` (a shared list's member by email, account id or `me`); null clears any of them | write |
| `workspace_task_move` | Reparent (subtree follows, across lists too) and/or reorder by 1-based position | write |
| `workspace_task_to_list` | Promote a task to a standalone list, optionally in a folder, keeping its subtasks | write |
| `workspace_task_delete` | Delete a task and its subtree | write |
| `workspace_task_blocker_add` | Make `task_id` wait on `blocked_by_task_id` (any list) until it is done; a cycle is refused | write |
| `workspace_task_blocker_remove` | Stop `task_id` waiting on `blocked_by_task_id` | write |
| `workspace_task_commit` | Commit to `task_id` on a `rule` (`daily` by default): a subtask carrying a habit, in Today on its days until the task is done; `title` defaults to "Work on …", `stacks_missed` keeps a missed day owed | write |
| `workspace_conditions` | Conditions under their categories in the app's order (settings first), each with its open-task count and what it provides; the context (what was said, everything that `holds` with where it comes from, what is `off_for_now`, when it was said and whether that was an earlier day, the time available, goal, expiry, last context); with `condition`, the open tasks needing it | read |
| `workspace_task_conditions_set` | What a task requires: each item a condition or an array of alternatives, by name or id, and `not Home` or `!Home` for one that must not hold; `create_missing` makes new ones | write |
| `workspace_condition_provides_set` | What a condition provides (`condition`, `provides`, by name or id; `create_missing` makes new ones): while it holds, they hold too. Not itself; one undo step | write |
| `workspace_day_start_set` | How each day starts: `resets`, `hour` (0–23, when the day changes — the daily log's too), `conditions` (by name or id, one place at most). At that hour what holds goes back to them, once a day on whichever device looks first; fields left out keep what they were, none only reports. Not an undo step | write |
| `workspace_context_set` | What holds now: `set`, `clear`, `on`, `off` (a place replaces the last place; `off` on a provided condition switches it off until what provides it changes), `mode`, `available_minutes`, `expires_in_minutes`. Not an undo step; the app picks it up at once, and sync carries it to the other devices | write |
| `workspace_filter_lists` | Filter lists (saved rule sets over every task): built-in Today and Everything first, then the user's, each with id, name, icon, a one-line summary, `match`, `show` and `rules` | read |
| `workspace_filter_list_tasks` | The tasks a filter list (by id, `builtin:today` included, or name) shows now, evaluated by the Rust core in this machine's time zone, list by list | read |
| `workspace_filter_list_create` | Create a filter list from a name, optional icon, `match` (all/any), `show` (open/done/all) and `rules` (`{field, op, value}`; the description lists the vocabulary) | write |
| `workspace_filter_list_update` | Change a filter list's name, icon, match, show or rules (replaced whole); the built-ins are refused | write |
| `workspace_filter_list_delete` | Delete a filter list; no task changes | write |
| `workspace_folder_create` | Create a folder, optionally inside another | write |
| `workspace_list_create` | Create a list, optionally in a folder | write |
| `workspace_list_move` | Move a list into a folder, or to the top level | write |
| `workspace_list_delete` | Delete a list and its tasks (not the Inbox) | write |
| `workspace_board_columns` | A board's columns (`board`: a list id, `everything` or `default`), with `source` (own, default or builtin) and each rule column's rules | read |
| `workspace_board_column_set` | Add a column (`title`, optional `position`), or rename one or set its `rules` (`column_id`); an empty `rules` makes it ordinary | write |
| `workspace_board_column_delete` | Delete a column; its filed tasks move to `move_to` (default the first ordinary column) in the same step | write |
| `agent_instructions_get` | The agent panel's standing instructions: every scope that has any, or one (`scope`: `account`, `folder` or `list`, with the folder's or list's `id`), each with its `scope`, `id`, `name`, `text` and `updated_at`. `scope: task` with a task's `id` is that task's agent context, kept apart from its notes | read |
| `agent_instructions_set` | Replace a scope's instructions (or, with `scope: task`, a task's agent context), or add a paragraph with `mode: append`; blank text clears it. Returns `previous`, the new `text` and whether it `changed`; an unchanged save writes nothing | write |

Notes:

- The Checkvist tools talk directly to the Checkvist API.
- A workspace write that changed anything carries `undo_step` in its result:
  the journal group of its "MCP: …" step. The Mac's agent panel hands it to
  the core (`CoreWorkspace::undo_step`) to take back exactly that change; any
  other client can ignore it.
- It does not automate the local macOS app UI.
- `task_add` supports both root insertion and specific parent insertion.
- The local tools need no IPC. They read the app's preferences plist by bundle
  id and the day-log files at the same Application Support path, under the same
  `flock` protocol the app uses.
- The focus tools open the database with `SQLITE_OPEN_READ_ONLY`, so they are
  read-only by the connection rather than by convention, and they stay that
  way: finishing a block has day-log and scoring side effects that live in
  `WorkspaceStore`, and a second writer would skip them. None of the workspace
  tools recompute the app's policy. Which task is next, whether one is
  available in the current context and how a day is scored all live in
  `TaktCore`, and a second implementation of them here is the thing this
  server stopped having. `database_path` in the CLI's config, or
  `$PRIORITY_MCP_DB_PATH`, points them elsewhere.
- The task-tree writes work **while the app is running**. See
  [How the workspace writes stay safe](#how-the-workspace-writes-stay-safe).

### How the workspace writes stay safe

The `workspace_*` write tools make the CLI a second writer to a database that
the apps own through the Rust core. That is safe because of four things
(`cli/src/workspace_tasks.rs`):

1. **Each write is the Rust core's.** `cli/src/workspace_tasks.rs` calls
   the same function in `core/src` that the Mac, iPhone and Android apps
   call (`tasks::create_task`, `tasks::set_status`, `tasks::move_task`,
   `conversions::move_task_to_folder`, `lists::create_list` and the rest),
   so there is one implementation of the sort-order conventions, the
   uppercase UUIDs, the UTC `YYYY-MM-DD HH:MM:SS.SSS` timestamps, the
   lazily created `task_metadata` rows and the refusals. The search index is
   kept level by the schema's own FTS triggers.
2. **Every write is one undo step, labelled `MCP: …`.** Each runs inside the
   same protocol as `journalledWrite`: arm `undo_control` with a fresh group
   and label, let the database's `change_log` triggers record the rows, disarm,
   clear redo if anything changed, and trim the journal to 100 steps, all in
   one transaction. The app's Undo menu then offers, say, "Undo MCP: New Task",
   and undoing it replays those rows exactly like one of its own. A refused
   write rolls back, journal entries included.
3. **SQLite does the locking.** `BEGIN IMMEDIATE` takes the write lock up
   front, with a 5-second busy timeout on both sides, so each writer waits out
   the other rather than failing. Foreign keys are on, as in the app, so a
   delete cascades to subtasks. A database older than the schema these writes
   target (`v16_task_completion_time`) is refused rather than written.
4. **The app notices.** Nothing in the app's process hears about another
   process's commit, so `WorkspaceViewModel+ExternalWrites.swift` polls
   `PRAGMA data_version` once a second on the Rust core's connection, which
   every app write goes through (`CoreWorkspace.dataVersion`). That number
   moves only when *another* connection commits. When it moves, the app flushes any draft being typed,
   reloads the way it does after undo, and reconciles open editors against the
   new rows, so a field changed on both sides shows as a conflict instead of
   being lost.

Promotion follows the app's model. `workspace_task_to_list` is the app's
"drop on a folder / the top level" (`moveTaskToFolder`). It creates a list
named after the task, with the source list's colour, and keeps the task itself
as the new list's `visibleRootTaskId`, turned into `itemKind = 'list'`, so its
subtasks become the list's contents with ids and hierarchy intact. The lighter
"nested list pinned to the sidebar" (`isPromoted`) is
`workspace_task_update` with `kind: "list", pinned: true`, and the task stays
where it is.

Dates on a workspace task follow the inspector. `due` is a day
(`2026-10-17`, the task's planning `dueDate`, due by the end of it) or a time
(`2026-10-17 17:00` local, or RFC 3339, the task's `dueAt`); either replaces
the other, as the inspector's Time toggle does. `start` is a day (its local
midnight) or a time, and must come before the deadline. `priority` is 1
(Urgent) to 4 (Low), or -4 to -1, which rank behind every task with none
(-1 the very back) and move the task to the bottom of its siblings where a
positive one moves it to the top; 0 or null clears it. On a repeating task
the priority is the series': setting it on one open occurrence sets it on
every open one, and the next turn inherits it. `estimate_minutes` is whole minutes.
Null clears each of them. The tool takes only ISO dates: "friday" is the
agent's to resolve, which is why the app's system prompt states today's date.
A single change is named as the app names it ("Set Priority", "Schedule
Task", "Edit Task" for the due date and estimate), and a call that changes
nothing leaves no undo step. `workspace_tasks` shows `due_date` or `due_at`,
`start_at`, `priority` and `estimate_seconds` on the rows that have them.

Deliberately not here: completing a **repeating** task. Its next occurrence is
scheduled from a `PeriodicSchedule` that only `TaktCore` can parse, so the
tool refuses rather than ending the series. Complete those in the app.
- `task_metadata` is read-only, and stays that way. Priorities, recurrence and
  start dates live in `UserDefaults`, which the running app holds in memory and
  rewrites on its own schedule — there is no equivalent of the file lock below
  that would let an external write survive. Setting those has to go through the
  app.
- `task_matrix_set` is the one narrow exception, and it does not disprove the
  rule above so much as work around it. It refuses outright while Takt is
  running (`pgrep -x Takt`, or `Priority` for an older build), and writes through
  `defaults write` rather than the plist file, so `cfprefsd` stays the single
  owner of the store. Both halves
  are load-bearing: a direct file write is invisible to `cfprefsd` and gets
  overwritten by its cached copy, and a write of any kind made while the app is
  running is discarded the moment the user places one task by hand.

  It exists because the alternative was placing two hundred tasks by hand. A
  bulk first pass from an assistant, corrected afterwards in the app, is a
  different job from "set this one task's urgency" — which still goes through
  the app, as above.

### How the daily writes stay safe

Two processes edit `dailies.json` and `daylog.jsonl`: the app and this server.
Three things make that safe, and all three are load-bearing:

1. **A file lock.** `FileLock` (`Sources/TaktCore/FileLock.swift`) takes `flock(2)` on a
   sibling `.lock` file — a sibling, because saves are atomic (temp + rename) and
   replace the data file's inode, so a lock held on it guards nothing. The CLI
   takes the same lock on the same path, so the two genuinely exclude each
   other.
2. **Read/modify/write, never save-a-snapshot.** `DailyDefinitionsStore.mutate`
   re-reads inside the lock and applies the change to what is on disk *now*. The
   app used to write its launch-time copy back wholesale, which silently erased
   anything added since. A mutation must be expressed as *what changed*.
3. **A directory watcher.** `DailyLogService` watches the store directory (not
   the files — the rename would orphan a file watch) and reloads, so a daily
   added here shows up in the app without a relaunch.

The serialised format is therefore a cross-process interface. Two constraints
are pinned by `DailyDefinitionsStoreFormatTests`:

- Dates are `yyyy-MM-ddTHH:mm:ssZ` with **no fractional seconds**. The Swift
  decoder is `.iso8601`, which rejects them, and `load()` turns a decode failure
  into an *empty* collection — so one bad timestamp makes every daily vanish and
  the next save persists that emptiness.
- `activeWeekdays` is written sorted, so saves don't churn the file.

A daily is scheduled *either* on weekdays *or* on a cycle. `intervalDays` (with
`intervalAnchor`, a day the cycle lands on) is present only for the second kind
and takes over from `activeWeekdays` entirely — the weekday set stays on the
record so that ending the cycle restores it. Both fields are absent on every
daily written before cycles existed, which decodes as the weekday case. In the
tools this is `interval_days`: passing it alongside `active_weekdays` is refused
rather than resolved in favour of whichever the implementation checks first, and
passing `active_weekdays` to `daily_update` clears an existing cycle.

### One server, two ways to name it

There is one implementation: the `takt` CLI (`cli/src/mcp.rs`). The app
**ships** it, at `Contents/Helpers/takt`, copied in by
`scripts/install_desktop.sh` and signed with the app.

| Command | What happens |
|---|---|
| `takt mcp` | The server, directly. What newly written configurations use. |
| `Takt --mcp-server` | The app `execv`s the bundled helper (`exec_mcp_server`, `desktop/src/platform/mod.rs`). What configurations written before this change say (as `Priority --mcp-server`, from before the rename). |

Because the app bundles the CLI, `Takt --mcp-server` works on a machine
where the CLI was never installed separately — which is what made retiring the
old server safe.

There used to be a second implementation: 1,760 lines of Swift in
`Takt/Plugins/MCP/MCPServer.swift`, running in-process. A third, a bundled
`python3` fallback script, went earlier. Both existed for the same reason — a
client might be pointed at any of them — and both cost the same thing: every
tool change was a two- or three-way edit, and every divergence a two- or
three-way diff. They were held equal from the outside by
`scripts/mcp_parity_check.py`, which drove each over stdio and compared tool
lists, answers, files written, and HTTP requests, because neither could import
the other.

What kept the Swift one alive was never a capability the CLI lacked; the parity
check proved that every tool the two shared agreed. It was that MCP client
configurations already written to users' disks name
`/Applications/Priority.app/Contents/MacOS/Priority --mcp-server`. Bundling the
CLI and turning that path into a shim removed the reason, so:

- ~1,760 lines of Swift are gone, as is ~610 lines of parity harness and a CI
  job;
- there is one implementation to be correct rather than two to keep equal;
- and existing configurations keep working untouched, because the CLI already
  accepted the bare `--mcp-server` flag (`cli/src/main.rs`) and already reads
  credentials from the environment ahead of its own config file
  (`cli/src/config.rs`) — which is exactly where a client configuration written
  back then put them.

`cargo test` covers the server. `scripts/mcp_smoke_check.py` covers the seam:

```bash
python3 scripts/mcp_smoke_check.py   # needs a Debug app build
```

It drives both spellings above and checks they answer `initialize` and expose
the same 54 tools — in particular that an old-style invocation, with
credentials in `env`, still reaches a working server. It reads no real data and
needs no Checkvist credentials.

## Setup (the short version)

**Settings → MCP.** Enable the toggle and the page
walks three steps:

1. **Checkvist connected** — the server signs in with your Checkvist credentials,
   so setup is blocked until they exist: the client buttons are disabled without
   them, and the coordinator refuses as well rather than trusting a UI guard.
   There would be nothing to hand the server, and every tool call would fail
   inside your AI client, a long way from the app. Once they exist, setting up a
   client copies that login down into the CLI's own store — see
   [Configuration](#configuration).
2. **Server command found** — the path to the executable an MCP client launches.
   Press Refresh after moving the app.
3. **Add to an AI client** — one button per client detected on this machine.

Takt detects Claude Code, Claude Desktop, Cursor, Windsurf, VS Code, and
Zed. What the button does depends on the client, because the wrong route is worse
than no route:

| Client | Action | Why |
|---|---|---|
| Claude Desktop, Cursor, Windsurf, VS Code | Writes the config file | Plain JSON files that hold nothing but MCP config |
| Claude Code | Copies a `claude mcp add-json` command to run | Claude Code rewrites `~/.claude.json` constantly; editing it underneath would race |
| Zed | Copies a snippet to paste | `settings.json` carries comments that a JSON rewrite would delete |

Direct writes **merge**: your other MCP servers and every unrelated key survive.
If the existing file isn't valid JSON, Takt refuses rather than replacing
it. Keys come back sorted, so expect the file to be reformatted once. The
format — the entry's shape, the merge, the rule that replaces an entry the app
wrote as Priority, the `claude mcp add-json` command, the Zed snippet and the
catalogue of clients — is `core/src/client_config.rs`; the app only reads and
writes the client's file.

Config writes go straight to the client's file — release builds are not
sandboxed (see `Priority.release.entitlements` for why), so no folder-access
prompt is involved. A client config Takt creates from scratch is tightened
to mode 0600; one that already existed keeps the mode its owner chose.

Before it writes or copies anything, setup seeds the CLI's credential store: it
merges your username and remote key into `~/.config/takt/config.json`,
creating `~/.config/takt` at mode 0700 and the file at 0600 (starting from the
old `~/.config/priority/config.json` if only that exists). It merges
rather than replaces, so a `base_url` you set by hand for a self-hosted
Checkvist survives, and it only fills `list_id` when that key is absent — the
generated MCP entry already names the default list per client, so the CLI's own
default for terminal use is left alone. Nothing happens to the file when it
already says this. The file is written whole: a temporary created at 0600 is
renamed over it, so neither the CLI nor a client ever reads a truncated copy.
That read, merge and write are `core/src/client_config.rs`, the same code
`takt auth login` saves the file through, so the app and the CLI cannot
disagree about the file's format.

If nothing is detected, use **Copy Client Config** and paste it in by hand — the
rest of this guide covers that.

## Configuration

The server signs in to Checkvist itself, so it needs a username and a remote
key. They can come from a file or from the environment, and the order is the
conventional one: **the environment beats the file, and a variable set in the
environment means the file is not consulted for that value at all**
(`cli/src/config.rs`).

### Credentials in the CLI's own store (recommended)

Keep them in `~/.config/takt/config.json` and leave the client config
credential-free. Two ways to put them there, and they write the same file:

- **From Takt.** Setting up a client seeds the file first and then hands the
  client an entry with no secret in it. (**Copy Client Config** only copies;
  it does not touch the file.)
- **From the terminal.** `takt auth login` prompts for both, checks them
  against the API before writing, and creates the file at mode 0600. See
  `docs/cli.md` for that command and its `auth status` / `auth set-list`
  siblings.

Either way there is one copy of the key on this machine, and every client that
launches the server reads it — which is the point.

### Environment variables (the override)

Set on the MCP process, usually through an `env` block in a client config:

- `CHECKVIST_USERNAME`
- `CHECKVIST_REMOTE_KEY`
- `CHECKVIST_LIST_ID` (default list)
- `CHECKVIST_BASE_URL` (defaults to `https://checkvist.com`)

Setting one of these means the config file is **not read for that value** — not
merged with, not fallen back to. That is right for a one-off override, for CI,
and for `scripts/mcp_smoke_check.py`, which drives the server with credentials
in the environment precisely so it cannot pick up whatever the developer
happens to have configured. `PRIORITY_CONFIG_PATH` (or `XDG_CONFIG_HOME`) moves
the file itself, for the same reason.

It is the wrong tool for a client config you intend to keep, because that pins
the key: see below.

### Rotating your remote key

A remote key baked into a client's `env` block is a second copy of it, and the
client goes on presenting the old one until someone edits that file by hand —
as a 401 from inside the AI client, with nothing in Takt saying why.

With credentials in the CLI's store there is one copy, so rotation is: change
the key in Takt and set the client up once more (which re-seeds the file),
or run `takt auth login` again. Every configured client follows, because
they all read the one file. If any client config still carries the key in
`env`, that entry keeps using the pinned value until you replace it — setting
that client up again from Takt rewrites the entry into the credential-free
form.

### Choosing a list

If `CHECKVIST_LIST_ID` is not set — by the client entry Takt generates, by
your own `env` block, or by `list_id` in the config file — pass `list_id` in
tool calls that need a list.

### Finding the server command

Command resolution priority, used both by the settings pane when it generates a
config and by `MCPServerShim` when `--mcp-server` looks for something to run:

1. `PRIORITY_MCP_EXECUTABLE_PATH` (explicit override — point a development build
   at a freshly built CLI without reinstalling the app)
2. The bundled helper: `/Applications/Takt.app/Contents/Helpers/takt`,
   then the same path relative to the running bundle
3. A separately installed CLI: `~/.local/bin`, `~/bin`, `/usr/local/bin`,
   `/opt/homebrew/bin`

If none resolves, a generated config points at
`/Applications/Takt.app/Contents/Helpers/takt` so it is obvious what to
fix, and `--mcp-server` exits with the list of paths it tried on stderr, where
the client will log it.

Extra control env vars:

- `PRIORITY_MCP_GUIDE_PATH` to override guide detection

## Run Manually

Once credentials are in place — `takt auth login`, or any client set up from
Takt's settings:

```bash
'/Applications/Takt.app/Contents/Helpers/takt' mcp
```

To override them for one run, without touching the stored ones:

```bash
CHECKVIST_USERNAME="you@example.com" \
CHECKVIST_REMOTE_KEY="your-remote-key" \
CHECKVIST_LIST_ID="123456" \
'/Applications/Takt.app/Contents/Helpers/takt' mcp
```

It will wait for an MCP client to connect over stdio.

## Client Config Example

Most MCP clients accept a JSON config similar to this — which is what Takt
generates now, carrying a default list and no credentials:

```json
{
  "mcpServers": {
    "takt": {
      "command": "/Applications/Takt.app/Contents/Helpers/takt",
      "args": ["mcp"],
      "env": {
        "CHECKVIST_LIST_ID": "123456"
      }
    }
  }
}
```

With no default list set, the `env` block is omitted entirely rather than
written empty:

```json
{
  "mcpServers": {
    "takt": {
      "command": "/Applications/Takt.app/Contents/Helpers/takt",
      "args": ["mcp"]
    }
  }
}
```

Use your own app path. If you are writing this by hand rather than letting
Takt write it, run `takt auth login` first — there is nothing in the
config that would sign the server in.

A configuration written before the CLI was bundled names the app binary
instead (under its pre-Takt name), and one written before credentials moved
out of `env` carries them inline:

```json
{
  "mcpServers": {
    "priority": {
      "command": "/Applications/Priority.app/Contents/MacOS/Priority",
      "args": ["--mcp-server"],
      "env": {
        "CHECKVIST_USERNAME": "you@example.com",
        "CHECKVIST_REMOTE_KEY": "your-remote-key",
        "CHECKVIST_LIST_ID": "123456"
      }
    }
  }
}
```

That still works in both respects for as long as the binary it names is on
disk: the app hands the process to the bundled helper, and the environment
still beats the file, so those inline credentials are the ones the server uses.
After the rename, though, that binary is the old `Priority.app`, and
`scripts/install_local.sh` removes it — so set
the client up again from Takt. That regenerates the whole entry in the
credential-free form above under the name `takt`, and removes the old
`priority` entry, which the installer recognises as its own by its command
(ending in `/Contents/Helpers/priority`, `/Contents/MacOS/Priority` or
`/bin/priority`), so a client never ends up with both. The inline credentials
are also pinned: rotating your remote key means editing this file too, or
setting the client up again.

A separately installed CLI works too, if you have run `scripts/install_cli.sh`
(it links `~/.local/bin/takt`):

```json
{ "mcpServers": { "takt": { "command": "/Users/you/.local/bin/takt", "args": ["mcp"] } } }
```

## Suggested First Calls

1. `task_lists`
2. `task_fetch` (omit `list_id` if default is set)
3. `task_add` with `location: "default"` and sample content
4. `task_add` with `location: "specific"` and `parent_task_id`
