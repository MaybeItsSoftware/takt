# CLI Guide

`takt` is a Rust command-line tool for the same data the app works with:
your local workspace (folders, lists and tasks), your Checkvist lists, your
dailies, and your day log.

It is a peer of the app rather than a remote control for it. It talks to the
Checkvist API directly and reads and writes Takt's local files off disk, so
every command works whether or not the app is running. The writes take the same
locks the app does, so both can be open at once.

```bash
./scripts/install_cli.sh     # builds release and links `takt` onto your PATH
takt --help
```

Or without installing:

```bash
cargo build --release --manifest-path cli/Cargo.toml
cli/target/release/takt dailies
```

## The terminal UI

Run `takt` with no arguments and it opens a terminal UI with the same tabs
as the menu bar app, and the same keys to reach them.

```
 All q │ Due w │ Tags e │ Priority r │ Kanban t │ Matrix y │ Daily d
┌ All ───────────────────────────────────────────────────────────────┐
│▎▾ [ ] Ship v0.4                                                    │
│   ▸ [ ] Draft the release notes  #work                             │
│     [x] Tag the commit                                             │
│   ▸ [ ] Buy milk  #home                                            │
└────────────────────────────────────────────────────────────────────┘
 j/k move · l/h open-shut · ] [ zoom · space done · a add · ? help · esc quit
```

Tasks start shut. `l` opens one and its subtasks appear indented underneath;
`▸` marks a task with something behind it and `▾` one that's open. What's open
is remembered per list between runs, in `outline.json` beside the config file —
separately from the app, which keeps its own.

| Key | Action |
| --- | --- |
| `q` `w` `e` `r` `t` `y` `d` | Jump to All, Due, Tags, Priority, Kanban, Matrix, Daily |
| `tab` / `shift-tab` | Cycle tabs |
| `j` `k` or `↓` `↑` | Move |
| `l` `h` or `→` `←` | Open / shut a task's subtasks, then step in and back out |
| `]` `[` | Zoom the list into the selected task, and back out (All tab) |
| `space` | Complete a task, reopen a closed one, or tick a daily |
| `u` | Reopen |
| `x` | Mark "won't do" |
| `a` | Add a task — or a daily, on the Daily tab |
| `F5` / `ctrl-r` | Refresh from Checkvist |
| `?` | Help |
| `esc` | Quit |

The letters were the app's menu bar panel's, not a new set: `q` was its All view,
so it is the All tab here too. That means **`q` does not quit** — `esc` does.
The panel itself has been removed from the app; these letters outlived it.

Each tab shapes the same data differently:

- **Due** groups into overdue / today / later.
- **Tags** groups by tag, with the untagged kept together.
- **Priority** reads the ranks you set in the app with `1`–`9`, absolute queue
  first and then per-parent.
- **Kanban** uses the columns you configured in the app, evaluated in order — a
  task belongs to the first column it matches, and a catch-all column takes only
  what the others left. The due-date bucketing is a port of the app's
  `classifyDueBucket`, so the board agrees with the app's own
  bucketing rather than approximating it.
- **Matrix** splits the Eisenhower placements into DO / SCHEDULE / DELEGATE /
  ELIMINATE, at zero on each axis. A task sitting at the origin has not been
  judged, so it is left out rather than filed under "eliminate".
- **Daily** shows your dailies, the day's counts, and what you finished.

Adding a task while you are inside a subtask puts it there, as quick-add does in
the app. Kanban and Matrix are read-only views of state the app owns: you can
complete a task from them, but the column and quadrant are set in the app.

**Without Checkvist credentials the Daily tab still works in full** — it reads
local files — and the other six explain what is missing rather than failing.

Every key that changes something dispatches the same tool call an assistant
would make over MCP, so the terminal cannot do anything the other front ends
can't, or do it differently.

It needs an interactive terminal: `takt | cat`, or a cron job, gets a clear
error pointing at `--help` rather than a UI nobody can quit.

## Signing in

```bash
takt auth login
```

It prompts for your Checkvist email and your remote key (from
[checkvist.com/auth/profile](https://checkvist.com/auth/profile), read without
echo), checks them against the API, and only then writes them to
`~/.config/takt/config.json` with mode 0600 (through a 0600 temporary renamed
into place, so the file is never half-written). A mistyped key fails at this
point rather than as a puzzling 401 on some later command. Until that file
exists, the CLI reads the old `~/.config/priority/config.json` from before the
rename to Takt.

```
takt auth status        where the config lives and what is in effect
takt auth set-list ID   the default list, used when --list-id is omitted
takt auth logout        forget the key; --all deletes the file
takt auth path          just the path, for scripts
```

`auth status` never prints the remote key — only its length, which is enough to
spot a truncated paste.

### These credentials are the CLI's own

Signing in here does not sign you in to the Takt app. The invariant runs
one way: the CLI never reads the app's keychain. That is deliberate. The app
keeps its remote key in the login keychain, where it is reachable only by
something carrying the app's code signature; a CLI that depended on it would
work or not depending on how the app happened to be built and signed that day.
Its own file is predictable, portable, and works on a machine with no app
installed.

The other direction is allowed, and is how the app sets up an MCP client: it
writes its credentials *into* this file, then generates a client config that
carries no secret at all. Nothing about that makes the CLI depend on the app —
the file it reads is the same one `auth login` writes, whoever put it there. If
you signed in here as a different account, setting up an MCP client from the
app will overwrite `username` and `remote_key`; that only happens on an
explicit action in the app's settings. See `docs/mcp-server.md`. Both sides
read and write the file through the same code, `core/src/client_config.rs`:
one reading (a JSON object; blank values are absent; an empty file is an empty
config), one encoding, one private write.

The trade is that the key sits in a plain file rather than the keychain, as CLI
credential files conventionally do. It is created mode 0600 from the moment it
exists — not chmod-ed afterwards, since the gap between the two is a window in
which it is world-readable.

### Environment variables still win

- `CHECKVIST_USERNAME`, `CHECKVIST_REMOTE_KEY`, `CHECKVIST_LIST_ID`
- `CHECKVIST_BASE_URL` — defaults to `https://checkvist.com`
- `PRIORITY_MCP_STORE_DIR`, `PRIORITY_MCP_PREFS_PATH` — where the local
  files are read from
- `PRIORITY_CONFIG_PATH`, or `XDG_CONFIG_HOME` — where the config file itself
  lives

Any of these set in the environment beats the file, so an MCP client config that
passes credentials keeps working untouched, and `CHECKVIST_LIST_ID=999 takt
tasks` is a one-off override. `auth status` says which source each value came
from, and the auth commands warn you when a variable is shadowing what they just
wrote.

The dailies, day-log and metadata commands read local files and need no
credentials at all, so they work before you have signed in to anything.

### Editing the file by hand

```json
{
  "username": "you@example.com",
  "remote_key": "...",
  "list_id": "945183",
  "base_url": "https://checkvist.com",
  "store_directory": "~/Library/Application Support/Takt"
}
```

Every key is optional. `~` is expanded in the path keys. A missing or malformed
file is treated as an empty config rather than an error — `takt dailies`
has no business failing over a credential file it never consults.

`store_directory` defaults to the app's own location (`Takt/`, or the old
`Priority/` folder while `Takt/` does not exist yet), and that default is the
one place the CLI and the app are deliberately joined: reading the dailies and
day log the app writes is the whole reason those commands exist. Point it
elsewhere for a CLI-only setup.

## Commands

```
  lists       List your non-archived checklists
  new-list    Create a new checklist
  tasks       Show a list's tasks as a tree
  search      Search a list by content, tag and/or due date
  add         Add a task
  update      Change a task's content, due date and/or tags
  note        Append a note to a task
  move        Reorder a task among its siblings. Position is 1-based
  reparent    Move a task under a different parent, or to the list root
  move-project  Move a root task and its whole subtree to another list
  done        Complete a task
  reopen      Reopen a completed task
  invalidate  Mark a task "won't do"
  rm          Delete a task
  log         What actually happened on recent days
  dailies     Show your dailies with today's schedule and tick state
  daily       Create, change or tick a daily
  metadata    Takt's own state: ranks, recurrence, start dates, matrix, board
  ws          The app's local workspace: folders, lists and tasks (alias: workspace)
  focus       What the focus timer is doing
  focused     Focused time already recorded
  auth        Store, check or clear this CLI's Checkvist credentials
  mcp         Run as an MCP stdio server
  tools       List the tools this binary exposes
  call        Call a tool by name with raw JSON arguments
```

Two global options: `--list-id` and `--json`. `--json` prints the raw payload
instead of the rendering, which is the same JSON the MCP tool of that name
returns — useful for `jq`, and for checking what an assistant would have seen.

### Examples

```bash
takt lists
takt tasks                             # the default list, open tasks, as a tree
takt tasks --all                       # include closed and "won't do"
takt search -q report --due-before 2026-09-01
takt search -t work --limit 10

takt add Draft the release notes --due friday
takt add Check the numbers --parent 12345
takt note 12345 Waiting on the design review
takt update 12345 --tags work,urgent        # --tags "" removes every tag
takt move-project 12345 --to 678901         # the subtree comes too; --from defaults to --list-id
takt done 12345

takt log --days 7
takt dailies
takt daily add Read for twenty minutes --weekdays mon,wed,fri
takt daily add Water the plants --every-days 3
takt daily tick 5F385C47-E2A6-488E-B3E1-84B0511FFAD4
takt daily update <id> --every-days 4
takt daily update <id> --weekdays weekdays   # back off the cycle
takt daily update <id> --archive

takt --json dailies | jq '.dailies[] | select(.done | not)'
```

`--weekdays` takes what you would actually type: `mon,wed,fri`, `weekdays`,
`weekend`, `every day`, or `Calendar` numbers where 1 is Sunday. The numbers are
what everything downstream sees; the spellings exist only at this boundary.

`--every-days N` is the other kind of schedule: a cycle that rotates through the
week rather than sitting on fixed days, counted from the day you set it. The two
are alternatives — passing both is refused rather than silently resolved — and
`--weekdays` on an existing cycle ends it, restoring the days it had before.

### The local workspace

`takt ws` works on the tree the app shows, in
`~/Library/Application Support/Takt/priority.sqlite`, not on Checkvist. Its
ids are the workspace's UUIDs, and `ws tree` / `ws tasks` print them beside
every row. The global `--list-id` is a Checkvist id and is ignored here.

```bash
takt ws tree                                   # folders, lists, nested lists
takt ws tasks <LIST_ID>                        # a list's open tasks as a tree
takt ws tasks <LIST_ID> --all --parent <TASK_ID>

takt ws add Read the paper --list <LIST_ID> \
  --link 'obsidian://open?vault=Studies&file=Paper' -c this-week
takt ws add Section 3 --parent <TASK_ID> -n "Start with the proofs"
takt ws update <TASK_ID> --title "Read the paper twice" --link https://example.com
takt ws update <TASK_ID> --no-links
takt ws update <TASK_ID> --kind list --pin      # nested list, pinned to the sidebar
takt ws update <TASK_ID> -w Sam --follow-up "2026-10-08 14:00"   # to Waiting on; "" clears
takt ws update <TASK_ID> --every 3              # chase again 3 days after each follow-up is done; 0 is once
takt ws update <TASK_ID> --due 2026-10-17 -p 1  # due by the end of the day, Urgent
takt ws update <TASK_ID> --due "2026-10-17 17:00" --start 2026-10-15 -e 90
takt ws update <TASK_ID> --due "" -p 0 -e 0     # clear the due date, priority and estimate
takt ws done <TASK_ID>
takt ws move <TASK_ID> --parent <TASK_ID>       # the subtree comes too
takt ws move <TASK_ID> --list <LIST_ID> --position 1
takt ws to-list <TASK_ID> --folder <FOLDER_ID>  # promote to its own list
takt ws block <TASK_ID> <BLOCKER_ID>          # TASK waits on BLOCKER until it is done
takt ws unblock <TASK_ID> <BLOCKER_ID>
takt ws commit <TASK_ID> [-t TITLE] [-r daily|weekdays|"mon, wed"] [--stacks]   # a subtask in Today on its days
takt ws deps <TASK_ID>                        # what it waits on, what waits on it
takt ws conditions [NAME]                     # conditions, open-task counts, what holds now; NAME's tasks
takt ws require <TASK_ID> 'Home|Campus' Mat   # needs Home or Campus, and Mat; none clears
takt ws require <TASK_ID> 'not Home'          # only away from Home ("!Home" too)
takt ws here Campus --for 60                  # at Campus (the last place goes off), an hour free
takt ws here --clear                          # anywhere
takt ws provides Home 'Floor space' Quiet     # while Home holds, so do they; none clears
takt ws here --off Quiet                      # a provided one off until Home goes
takt ws day-start Home --hour 4               # each day starts at 04:00 with Home on
takt ws day-start --nothing                   # each day starts with nothing on
takt ws day-start --off                       # what holds carries over; bare day-start shows it
takt ws new-folder Computer Science
takt ws new-list Revision --folder <FOLDER_ID>
takt ws move-list <LIST_ID> --folder <FOLDER_ID>
takt ws rm <TASK_ID>
takt ws rm-list <LIST_ID>                     # the list and its tasks

takt ws filters                               # Today, Everything and your filter lists
takt ws filter Today                          # what one shows now, by id or name
takt ws filter-new Due work -r tag:has:work -r due:within:7
takt ws filter-new Stuck -m any -r blocked:is -r waiting:is
takt ws filter-new Done this week -s done --rules '[{"field":"due","op":"within","value":"7"}]'
takt ws filter-edit <FILTER_ID> --name "Due soon" -r due:within:3   # rules replace the set
takt ws filter-edit <FILTER_ID> --no-rules --icon ""                # every open task, no icon
takt ws filter-rm <FILTER_ID>

takt ws columns <LIST_ID>                     # a list's board columns; or everything, default
takt ws column-set <LIST_ID> -t Review --position 2              # a new column
takt ws column-set <LIST_ID> -c urgent -t Urgent -r tag:has:urgent   # a rule column
takt ws column-set default -t Someday         # the columns a board without its own uses
takt ws column-rm <LIST_ID> in-progress --to this-week   # its tasks move in the same step

takt ws instructions                          # the agent's standing instructions, every scope
takt ws instructions --list <LIST_ID>         # one scope: --account, --folder ID or --list ID
takt ws instructions-set --account Write task titles as verbs.
takt ws instructions-set --folder <FOLDER_ID> --append Modules go by their code.
takt ws instructions-set --list <LIST_ID>     # no text: clears the list's
```

These write while the app is open. Each command is one step in the app's Undo
menu, labelled "MCP: …", and the app picks the change up within about a
second. `docs/mcp-server.md` explains how that is made safe. Completing a
repeating task writes its next occurrence, stepped in this machine's time
zone, through the same Rust core code the app uses (`core/src/tasks.rs`).
A blocked task — open, and waiting on a task that is still open — is marked
`(blocked)` in `ws tasks` and listed after its unblocked siblings, as the app
draws it; a dependency that would make a cycle is refused.

A filter list is a saved question asked of every task rather than a place
tasks live. Its rules are `FIELD:OP` or `FIELD:OP:VALUE` (everything after the
second colon is the value), all of which must hold, or any one with
`-m any`; `-s` picks open, done or all tasks. The rules are evaluated by the
Rust core (`core/src/filter_lists.rs`), the same code the apps use, in this
machine's time zone. The fields and their operators are listed in the
`workspace_filter_list_create` line of `takt tools`. Today and Everything are
built in and cannot be edited.

The agent instructions are what the Mac's agent panel sends ahead of a
message: the account's, then each enclosing folder's, then the list's
(`docs/agent-panel.md`). They live in the workspace database
(`agent_instructions`, written by `core/src/agent_instructions.rs`), so the
app's settings fields, these commands and the agent's own
`agent_instructions_set` tool all edit the same rows, and they sync. Without
`--append` the text replaces what the scope holds.

### The escape hatch

Every command is one of the MCP tools under a friendlier name. If you want the
tool directly:

```bash
takt tools
takt call daily_add '{"title": "Stretch", "active_weekdays": [2,4,6]}'
takt call task_search '{"query": "invoice", "include_closed": true}'
```

This is not a fallback for missing features — it is the same dispatch table the
subcommands go through. It exists so that a tool argument the CLI has no flag
for is still reachable, and so `takt call` can be used to reproduce exactly
what an assistant did.

`takt tools --json` prints the definitions as the MCP server's `tools/list`
returns them, input schemas included. The agent panel reads it to give
Antigravity the parameters it cannot otherwise see (`docs/agent-panel.md`).

## As an MCP server

This binary *is* Takt's MCP server. There is no other — the app ships a copy
at `Takt.app/Contents/Helpers/takt` and `Takt --mcp-server` hands
the process over to it (`Takt/MCPServerShim.swift`).

```bash
takt mcp
takt --mcp-server     # accepted too, so a config written for the app works unchanged
```

That second spelling is load-bearing rather than a convenience: MCP client
configurations written before the app bundled this binary say
`/Applications/Priority.app/Contents/MacOS/Priority --mcp-server` (the app's
name before Takt), and they keep working, for as long as that binary is there,
because both the flag and the environment-first credential rule
below were already here.

If you have run `auth login`, no `env` block is needed at all:

```json
{
  "mcpServers": {
    "takt": {
      "command": "/Users/you/.local/bin/takt",
      "args": ["mcp"]
    }
  }
}
```

Passing credentials in `env` still works and still takes precedence, which is
what makes an existing configuration keep behaving identically — the config file
can never override what a client already supplies.

See `docs/mcp-server.md` for the tool list and the history of how three
implementations became one.

## Editing dailies while the app is open

Both processes edit `dailies.json` and `daylog.jsonl`, and both go through the
same `flock(2)` on the same sibling `.lock` file, so a write from here cannot
interleave with one from the app. The CLI also re-reads the file inside the lock
rather than saving a snapshot it loaded earlier, which is what stops a
concurrent write being silently erased.

The app watches its store directory, so a daily added here appears in the app
without a relaunch.

What is *not* writable from here is `metadata` — priority ranks, recurrence
rules and start dates. Those live in `UserDefaults`, which the running app holds
in memory and rewrites on its own schedule, so there is no file lock that would
let an external write survive. Setting them has to go through the app.

## Layout

```
cli/
  Cargo.toml
  src/
    main.rs       entry point; --mcp-server is intercepted before argument parsing
    cli.rs        subcommands, the auth commands, and the renderings
    tools.rs      the tools, implemented once
    checkvist.rs  the API client
    config.rs     ~/.config/takt/config.json (its format and write are the core's
                  client_config.rs), and the environment-first rule
    local.rs      dailies, day log, and preferences, off disk
    lock.rs       flock(2), on the same lock files the app takes
    workspace.rs  the app's database, read-only: the focus timer and its history
    workspace_tasks.rs  the app's database, read and written: folders, lists, tasks
    mcp.rs        the JSON-RPC stdio server and the tool schemas
    tests.rs      unit tests
    workspace_tests.rs  the workspace writes, against scratch copies of the real schema
    fixtures/workspace_schema.sql  that schema, from scripts/dump_workspace_schema.sh
```

`cli.rs` and `mcp.rs` are both front ends onto `tools.rs`. Neither implements a
tool, so the CLI and the MCP server cannot drift apart — a command that cannot
be expressed as a tool call does not belong in `cli.rs`.

The crate is deliberately outside both `Package.swift` and the Xcode project: it
shares no source with them, and nothing in `Takt/` should ever `import` it.
The app *runs* it — `scripts/bundle_cli.sh`, from an Xcode build phase, cargo-
builds it and installs it into the bundle — but that is a process boundary, not
a source dependency. One consequence worth knowing: **building the app needs
cargo.** `TAKT_SKIP_CLI_BUNDLE=1` opts out, and produces an app with no MCP
server.

## Developing

```bash
cargo test --manifest-path cli/Cargo.toml
cargo clippy --manifest-path cli/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path cli/Cargo.toml --check
python3 scripts/mcp_smoke_check.py       # needs a Debug app build
```

`cargo test` is what covers the server's behaviour, now that there is only one
implementation of it. The smoke check covers the seam instead: that
`Takt --mcp-server` still reaches this binary, and that a configuration
written before the migration still gets a working server. CI runs all four.
