# Sync

Priority's workspace is a local SQLite database on every device. The Mac, the
iPhone and the Android phone each keep a full copy and work offline. Sync
copies row changes between them through one small server (`sync-server/`,
on Google Cloud Run, with its rows in the Supabase project's Postgres). The server stores rows and merges them; it knows nothing
about tasks.

The client half is the Rust core's (`core/src/sync.rs`): the outbox, the
cycle's push body and the pulled pages (`core/src/sync/wire.rs`, with the
server's own structs from `takt-sync-rules`), the clock, applying a pull, and
reading a typed server address (`core/src/sync/endpoints.rs`). The Mac and
iPhone (`Sources/TaktSync`) and Android (`mobile/android/data`) keep only the
transport: HTTP, the Supabase token and waiting for news (a Realtime channel,
or the long poll), moving bodies the core
made and handing back pages as the server sent them. The Rust CLI syncs
passively: it never talks to the server, but its writes go through the same
triggers, so they sync the next time the app does.

## What is synced

Every row of these tables, keyed by the column shown:

| Table | Key |
| --- | --- |
| `workspaces` | `id` |
| `list_folders` | `id` |
| `task_lists` | `id` |
| `tasks` | `id` |
| `task_metadata` | `taskId` |
| `task_conditions` | `id` |
| `task_dependencies` | `id` |
| `kanban_boards` | `id` |
| `filter_lists` | `id` |
| `dailies` | `id` |
| `daily_contributions` | `id` |
| `focus_sessions` | `id` |
| `focus_queue_items` | `id` |
| `focus_work_blocks` | `id` |
| `focus_awards` | `id` |
| `agent_instructions` | `scope` |
| `themes` | `id` |
| `preferences` | `key` |
| `focus_contexts` | `workspaceId` |

`themes` and `preferences` arrive in `v18_themes_and_preferences`:
`themes(id TEXT PRIMARY KEY, json TEXT NOT NULL, updatedAt DATETIME NOT NULL)`
and `preferences(key TEXT PRIMARY KEY, value TEXT, updatedAt DATETIME NOT NULL)`.
They hold the user's theme files and the cross-device choices described in
[themes](themes.md#the-chosen-theme-follows-you). Neither is journalled for undo.

`v19_habit_options` adds five columns to `dailies` — `sourceTaskId TEXT`,
`placementColumn TEXT`, `dropsAtDayEnd BOOLEAN NOT NULL DEFAULT 1`,
`expiryRule TEXT NOT NULL DEFAULT 'never'` and `expiresAt DATETIME` — and
reinstalls the `dailies` outbox and journal triggers, which name its columns.
A client that does not know them still syncs: the defaults describe a plain
daily. The Mac (`WorkspaceStore+Habits.swift`) and Android
(`WorkspaceRepositoryHabits.kt`) both move a habit's card between columns
and expire habits, from the same `HabitPolicy`. Each pass writes only the
habit's own `task_metadata` and `dailies` rows, so two devices converge on
the same values. The Habits list a new habit goes in has an id derived from
the workspace (`HabitPolicy.habitsListId`), so two devices that each make
it make one row. Other clients see the result as ordinary rows.

`v20_waiting_follow_ups` adds four columns to `task_metadata` — `waitingOn TEXT`
(who or what a task in the `waiting-on` column waits on), `waitingFollowUpAt
DATETIME` (when to chase it), `waitingFollowUpTaskId TEXT` (the follow-up already
made for it) and `followUpOfTaskId TEXT` (on a follow-up: the task it chases) —
and reinstalls the `task_metadata` outbox and journal triggers. Every client
that runs the follow-up engine (the Mac and Android; see `WaitingFollowUp` in
`Sources/TaktCore` and its Kotlin port) gives the follow-up task the same id,
derived from the source's id and the follow-up time:
`SHA-256("takt.follow-up:<sourceTaskId>:<epoch seconds>")`, its first 16 bytes
formatted as an uppercase UUID with the version-5 and RFC 4122 variant bits set.
Two devices that both make the same follow-up therefore write the same `tasks`
and `task_metadata` rows, which merge rather than duplicate.

`v21_task_dependencies` adds `task_dependencies(id TEXT PRIMARY KEY, taskId,
blockedByTaskId, createdAt)`: `taskId` waits on `blockedByTaskId` until that
task is completed or cancelled. Both ends reference `tasks(id) ON DELETE
CASCADE`, so deleting either task deletes the edge, and the tombstone syncs;
an edge that arrives before one of its tasks, or after it was deleted, is an
orphan the pull removes. It is the first migration written for the Rust core
rather than captured from GRDB, and it ends by reinstalling every journal and
outbox trigger (`schema::data::finish`). An edge's id is derived from its two
ends — `SHA-256("takt.dependency:<taskId>:<blockedByTaskId>")`, formatted like
a follow-up's id — so two devices that add the same dependency write one row.
The core refuses an edge that would close a cycle, but two devices offline can
each add half of one; nothing walks the graph beyond that check, so such a
cycle only leaves its tasks blocked (dimmed and sunk) until an edge is
removed. A client older than v21 skips the table's rows on pull, as it skips
any table it does not know. The rules the clients draw from it live in
`core/src/dependencies.rs`.

`v22_matrix_order` adds `matrixRank INTEGER` to `task_metadata`: a task's
place within its matrix quadrant, lowest first, and reinstalls the
`task_metadata` journal and outbox triggers. A quadrant reads ranked tasks
first, by rank, then the unranked in board order. Dropping a task at a point
in a quadrant (`tasks::place_in_matrix`) renumbers the quadrant's tasks from
0 in the order the client drew them, writing only the ranks that change;
moving a task to another quadrant without a point clears its rank, so it
lands after the ranked ones. A client older than v22 drops the column on
pull, as it drops any column it does not know.

`v23_filter_lists` adds `filter_lists(id TEXT PRIMARY KEY, workspaceId,
name, icon, colorHex, rulesJSON, sortOrder, createdAt, updatedAt)`: a saved,
named rule set evaluated over every task (`core/src/filter_lists.rs`), which
the sidebar draws under Today, Everything and the inbox. `rulesJSON` is
`{"match":"all"|"any","show":"open"|"done"|"all","rules":[{"field","op","value"}]}`;
a rule a client does not know is skipped when it evaluates rather than
refused, so an older device shows a broader list than a newer one wrote, never
an empty one. Ids are random; there is nothing to merge two devices' filter
lists on. The table is journalled (creating, editing and deleting one is an
undo step) and the migration ends by reinstalling every journal and outbox
trigger, like v21. When two devices' workspaces are merged into the canonical
one (`sync::adopt_workspaces`), filter lists move with the folders and lists.
Today and Everything are definitions shipped in the core, not rows.

`v24_condition_categories` adds `category TEXT` to `task_conditions`: the
heading a condition is filed under ("Location", "Equipment", "Energy"), or
NULL for none. It reinstalls the `task_conditions` journal and outbox
triggers, then files every existing place (`isLocation = 1`) under
"Location" with the journal and the outbox off, as a data step every device
runs for itself, so the backfill is not pushed. `isLocation` stays and stays
authoritative for exclusivity: the core's writes keep it equal to
`category = 'Location'` (filing a condition under Location makes it a place,
filing it anywhere else makes it not one, and making it a place through the
old `save_condition` files it under Location). A client older than v24 drops
the column on pull and still treats places as exclusive; a place it makes
arrives with no category and is shown under Location
(`conditions::effective_category`). Categories are free text, matched
case-insensitively, so "equipment" typed on one device joins "Equipment".

`v25_agent_instructions` adds `agent_instructions(scope TEXT PRIMARY KEY,
text, updatedAt)`: the agent panel's standing instructions
(`core/src/agent_instructions.rs`, `docs/agent-panel.md`), one row per scope.
The key *is* the scope — `account`, `folder:<id>` or `list:<id>` — rather
than a random id, so two devices that each write the account's instructions
write the same row and the merge keeps one of them instead of two rows that
both claim the account. It names no other row by foreign key: a row whose
folder or list is deleted is simply never read back. The table is journalled
(an edit, the agent's included, is an undo step) and the migration ends by
reinstalling every journal and outbox trigger. It syncs because the agent's
tools are the CLI's, which runs wherever the workspace is, and syncing it is
three entries in the table lists; a phone stores the rows and shows none of
them. The Mac's instructions lived in its preferences before v25, and the app
copies them in once, outside the journal (`import_agent_instructions`).

`v26_commitment_missed_stack` adds `stacksMissedFrom DATETIME` to `dailies`:
the start of the local day from which a habit's missed appearances stack
rather than vanish, NULL (every existing row) for vanishing. It reinstalls
every journal and outbox trigger. While it is set, the habit pass keeps the
card in its column while any scheduled day since then has no completed
contribution, and `dailies::log_contribution` records a completion against
the oldest such day rather than today. Only the commitment form sets it
(`commitments::save_commitment`). A client older than v26 drops the column on
pull and treats the habit as vanishing.

`v28_synced_focus_context` syncs `focus_contexts`, the table v27 added for
what holds now: which conditions are on (`conditionIdsJSON`), when they were
said (`conditionsSetAt`), the time available (`availableUntil`), the goal
(`mode`), when the answer lapses (`expiresAt`) and what an expired answer set
aside to offer back (`suggestedIdsJSON`) — one row per workspace, keyed by the
workspace's id, which every paired device shares once adoption has run
(`core/src/focus_context.rs`). The table does not change: the migration
reinstalls the outbox triggers, which now name it, and on a device that is
already paired queues the row it holds, so the next push sends it. It is not
journalled — saying where you are is not an undo step.

It is one answer, so it merges as one: the table is in
`triggers::WHOLE_ROW_TABLES`, and every change to it pushes every column
rather than the ones that moved. The server's per-column merge then gives
each column the same clock, and the last device to say where you are wins
the whole row; half of one device's answer under half of another's would be
neither. `conditionsSetAt` travels with the row, so a context said yesterday
on the phone counts as stale on the Mac (`focus_context_is_stale`). When
workspace adoption folds a device's own workspace into the canonical one,
its context comes along if the canonical workspace has none or it was saved
more recently, its conditions renamed to the canonical workspace's
conditions of the same name; otherwise it goes. The Mac, the iPhone and
Android read and write the row through the core, and each phone moved the
context its own settings held into it once, when the workspace had none.
The server needed nothing: it stores whatever tables it is sent. A client
older than v28 skips the table's rows on pull, as it skips any table it does
not know.

`v29_daily_contribution_skips` adds `skippedAt DATETIME` to
`daily_contributions`: when a day's appearance of a habit was set aside
without being done, NULL (every existing row) for not skipped. It reinstalls
every journal and outbox trigger. Cancelling a commitment's subtask writes it
(`commitments::skip_instead_of_cancelling`) on the day its showing appearance
belongs to. The habit pass reads a skipped day as settled, like a completed
one, so it is neither carried nor owed when missed appearances stack, but it
is not a completion: streaks, counts and `completedAt` are untouched. Two
devices' rows for the same day merge by keeping the earlier `skippedAt`, as
they keep the earlier `completedAt`. A client older than v29 drops the
column on pull and sees the day as not done.

`v31_link_details` adds `linkDetailsJSON TEXT` to `task_metadata`: a JSON
object keyed by link address of the name and file type the link picker knew
(`{"<url>": {"title": "Budget", "mimeType": "…"}}`), NULL for nothing known.
The addresses themselves stay in `externalLinksJSON`, so the two columns
merge independently; a name whose address is gone is ignored and pruned on
the next link write (`core/src/links.rs`). It reinstalls every journal and
outbox trigger. A client older than v31 drops the column on pull and draws
its links from the address alone. See `docs/links.md`.

`v32_follow_up_repeat` adds two columns to `task_metadata`:
`waitingFollowUpEveryDays INTEGER` (chase again every so many days, 1 to 365,
after a follow-up is completed; NULL is once) and `waitingFollowUpCount
INTEGER NOT NULL DEFAULT 0` (how many follow-ups this spell of waiting has
made, which numbers the next: "Follow up #2 with Sam: …"). Completing a
follow-up moves its source's `waitingFollowUpAt` on in the same step, so the
next follow-up's id — still SHA-256 of the source and the time — is the same
on every device that makes it. Each column merges on its own like the rest
of the row. It reinstalls every journal and outbox trigger. A client older
than v32 drops both on pull, makes unnumbered follow-ups, and never moves the
time on; a repeat set elsewhere still applies when a newer client completes
the follow-up. See `core/src/waiting.rs`.

`v33_recurrence_series` adds `task_metadata.seriesTaskId TEXT`: the id of
the first occurrence of the repeating task this one belongs to, NULL when a
task is its own series (every task written before it). The next turn and a
"keep both" copy are given the series of the task they come from, so a
priority set on one open occurrence can be written to every open occurrence
of the series in the same step — the newest one wins. It merges on its own
like the rest of the row and reinstalls every journal and outbox trigger. A
client older than v33 drops it on pull and sets a priority on the one
occurrence only. See `core/src/editor.rs`.

`v34_condition_provides` lets a condition provide others. It adds
`task_conditions.providesJSON TEXT`: a JSON array of the ids of the
conditions that hold whenever this one does (Home: Floor space, Desk,
Quiet), NULL for none. It is a column of the condition's row, so it is
journalled (setting it is an "Edit Condition" step) and merges like the
rest of the row, the whole list at once: two devices that each change what
Home provides keep the later list. The core refuses a condition providing
itself and a new link to an archived condition; a loop through several
(Home → Quiet → Home) is harmless, because what holds is worked out by
visiting each condition once (`core/src/conditions/provides.rs`). Merging
one condition into another rewrites every list naming it, once each, and
the merged one stops providing; an archived condition neither provides nor
is provided; an id whose condition was deleted elsewhere is ignored. When
adoption folds a device's own workspace into the canonical one, its lists
are renamed to the canonical conditions of the same name, as its context
is. It also adds `focus_contexts.offIdsJSON TEXT NOT NULL DEFAULT '[]'`:
the provided conditions said not to hold for now (housemates in: Quiet
off). It travels with the rest of the context, whole, and the core drops an
id from it whenever what provides it is turned off or replaced, or nothing
provides it any more (`focus_context::save`). It reinstalls every journal
and outbox trigger.

What holds — the conditions said, everything they provide, less what is
off for now — is the one set every client checks tasks against, negated
requirements included (`conditions::effective_ids`). A client older than
v34 drops both columns on pull and simply does not derive provided
conditions: on it, a task needing Floor space waits until Floor space
itself is turned on, as before. Whatever it saves keeps the server's
`providesJSON`, which it never pushes. Its whole-row push of the context
does not carry `offIdsJSON` either, so the server keeps the last override
a newer device sent: one whose provider the older device turned off is
dropped when a newer device next saves the context, but one whose provider
it turned off and on again survives until then.

`v35_series_title` adds `task_metadata.seriesTitle TEXT`: the title a
repeating task's next turn takes when this occurrence was renamed for
itself alone ("just today"), NULL otherwise. Closing the occurrence writes
the next turn with that title and NULL here, so the rename lasts one turn;
a rename for every turn, or back to that title, clears it. It merges like
the rest of the row and reinstalls every journal and outbox trigger. A
client older than v35 drops it on pull, so an occurrence it closes hands on
its own title, as before. See `core/src/tasks.rs` `keep_series_title`.

`v36_task_agent_context` adds `task_metadata.agentContext TEXT`: what the
agent should know about one task, kept apart from its notes, NULL when
nothing is written. The Mac's inspector edits it, the agent reads it beside
the task, and the `agent_instructions` tools reach it as the `task` scope.
It merges like the rest of the row and reinstalls every journal and outbox
trigger. A client older than v36 drops it on pull. See `core/src/agent_instructions.rs` `set_task_agent_context`.

Negative priorities (-1 to -4, behind every task with none) came with no
migration: `task_metadata.priority` was already an `INTEGER`, so a value
arrives on every client. A client built before them (core before
`c79a5461`) reads one as no priority, and its editor keeps only 1 to 4 of
what it writes back, so saving any of a task's tags, links, repeat or
priority there writes NULL over the -1 — even when the user only changed a
tag. The trigger pushes `priority` as changed, the server's per-column merge
takes it as the newest word, and every device then shows the task with no
priority, back among the rest instead of at the bottom. Nothing in a push
says which version of the core wrote it, so neither the server nor a newer
client can tell that NULL from a priority cleared on purpose, and the core
does not try to. Update every device before relying on a negative priority.

Negated requirements ("not Home") came with no migration either: a member
of a task's `requirementGroups` written `"!" + conditionId` is met while that
condition does *not* hold (`core/src/conditions/requirement.rs` is the one
place that reads one). The planning JSON was already free text, so it syncs
as it is. A client built before them reads `"!home"` as an id no condition
has, which no context holds: the task stays waiting there, and its
requirement shows as a missing condition, until that device is updated.
Nothing is lost — an older editor keeps the member when it saves the task
without touching its conditions — but removing the "missing" condition there
removes the requirement.

How each day starts came with no migration: four `preferences` rows,
which already sync per row (`core/src/day_start.rs`). `dayStart.resets`
("true" or "false"), `dayStart.hour` (0 to 23, the hour the day changes —
the Mac's daily log hour, `dailyLogRolloverHour`, is brought in once when
the account has none, and follows the account after that),
`dayStart.conditionIds` (a JSON array of condition ids) and
`dayStart.lastReset` (`yyyy-MM-dd`, the last day a device reset what holds).
Every client calls the core's `apply_day_start` when it evaluates — at
launch, on coming to the front, after a pull, and the Mac every half minute
— and the first call after the hour writes the starting set into
`focus_contexts` and stamps `dayStart.lastReset`, so the next device to look
sees the day already started and does nothing. A context saved since the day
began (`focus_contexts.updatedAt`) is never overwritten: something said on
another device at 06:00 and pulled before this one's first look stands. Two
devices that both reset before hearing from each other write the same set,
and the last-writer rule settles the identical rows. Day keys are read in
each device's own zone, and one a zone behind leaves a day it reads as still
to come. Changing the settings stamps today as reset, so a change never
resets there and then. A client built before it ignores the rows and never
resets; on it the context simply carries over, and a reset another device
made reaches it as an ordinary context change.

`v37_shared_lists` adds three tables of the device's own, none of them
synced or journalled: `sync_spaces` (the shared lists this account is in, as
the last pull listed them: space, list, role, members), `sync_row_spaces`
(which shared space each row was last sent to or received from) and
`sync_push_plan` (where a push in flight is sending its rows, recorded once
the server has them). See [Shared lists](#shared-lists).

`v38_task_authorship` adds `tasks.createdBy`, `tasks.completedBy` and
`task_metadata.assigneeAccountId`: account ids from a shared list's members,
NULL for nobody and for every task in a list of your own. All three are
shared columns, so they travel in the list's space. It reinstalls every
journal and outbox trigger. A client older than v38 drops them on pull: it
shows no authorship and offers a task assigned to someone else as anyone's.

`v40_list_priority` adds `task_lists.priorityRank`: a prioritised list's
place, 1 the most important, the prioritised lists of a workspace numbered
1..n without gaps; NULL, as every list starts, is not prioritised. Up next
ranks a task in a prioritised list ahead of others in the same board column.
Each edit renumbers the lists whose rank changed, so two devices that each
prioritised a list offline can merge to a tie or a gap; every reader orders
by rank, then sidebar order, and numbers the result 1..n, and the next edit
closes it up for good. Archiving, completing or deleting a list drops its
rank. In a shared list it is a personal column, so it goes to the
account's own space and other members never see it. It reinstalls every
journal and outbox trigger. A client older than v40 drops the column on
pull, as it drops any it does not have, shows no priorities and ranks Up
next as it always did; its own edits to a list leave the rank alone.

The following are not synced: the undo journal (`undo_control`, `change_log`), the FTS index
(rebuilt by its own triggers as synced rows land), `grdb_migrations`, and the
sync tables themselves. Day-log files stay per device. Theme *files* do
too, but their text travels as `themes` rows (the Mac mirrors its themes
folder into the table, see [themes](themes.md#the-chosen-theme-follows-you)).

## Local schema (`v17_sync`)

```sql
CREATE TABLE sync_control (id INTEGER PRIMARY KEY,
  recording INTEGER NOT NULL DEFAULT 0,   -- 1 once the device is signed in
  applying  INTEGER NOT NULL DEFAULT 0);  -- 1 while remote rows are written
INSERT INTO sync_control (id) VALUES (0);

CREATE TABLE sync_outbox (seq INTEGER PRIMARY KEY AUTOINCREMENT,
  tableName TEXT NOT NULL, rowId TEXT NOT NULL,
  operation TEXT NOT NULL,          -- 'insert' | 'update' | 'delete'
  changedJSON TEXT,                 -- update only: JSON array of column names (may hold nulls; ignore them)
  changedAtMs INTEGER NOT NULL);    -- wall clock when the trigger fired

CREATE TABLE sync_state (id INTEGER PRIMARY KEY,
  deviceId TEXT NOT NULL, cursor INTEGER NOT NULL DEFAULT 0,
  hlc TEXT, serverURL TEXT, canonicalWorkspaceId TEXT,
  needsSnapshot INTEGER NOT NULL DEFAULT 1, lastSyncedAt DATETIME);
```

Each synced table has `sync_outbox_<table>_{insert,update,delete}` AFTER
triggers, guarded by
`WHEN (SELECT recording FROM sync_control WHERE id = 0) = 1 AND (SELECT applying FROM sync_control WHERE id = 0) = 0`.
They use plain SQL only. They must never call an application-registered
function, because the CLI and `sqlite3` write to the same file without one. Like the
undo triggers, they name their columns, so **any later migration that adds or
removes a column on a synced table must reinstall them**. Migrations live in
the Rust core; `core/src/schema/triggers.rs` generates both kinds of trigger
from a table's current columns.

An undo is ordinary inserts and updates, so it syncs like any other edit.

## Hybrid logical clock

An HLC is the string `"<ms:013d>-<counter:04d>-<deviceId>"`. Because every part is
fixed width, plain string comparison orders HLCs correctly. A device keeps its
last HLC in `sync_state.hlc`. To stamp a change made at wall time `w`:
`ms = max(last.ms, w)`. The counter is `last.counter + 1` if `ms` equals `last.ms`,
and `0` otherwise. When it receives a row, the device advances its clock past
the row's HLC in the same way.

## Wire protocol (JSON over HTTPS)

Accounts are **Supabase Auth** users. The apps sign in with Supabase itself
(email and password, Google, or Apple), and Supabase handles confirming
emails and resetting passwords. Every request but `/health` then sends:

- `Authorization: Bearer <Supabase access token>`. The server checks its
  signature against the project's published keys and takes the account from
  its `sub`. An expired token gets `401`; the app refreshes it with Supabase
  and retries, and only a failed refresh means "signed out — sign in again".
- `X-Priority-Device: <uuid>`, the device's own id, made once and kept.
  Required on `/v1/push` (every write records its device); used elsewhere to
  mark the current device.

A value is a JSON `null`, number or string, exactly as SQLite stores it. Dates
are GRDB's `"yyyy-MM-dd HH:mm:ss.SSS"` text in UTC, and booleans are `0`/`1`.

### Devices and the account

- `POST /v1/devices` `{ "id", "name", "platform" }` records the device on the
  account. Sent after every sign-in; a device that was on another account
  moves to this one.
- `GET /v1/account` returns `{ "accountId", "email", "devices": [{ "id",
  "name", "platform", "createdAt", "lastSeenAt", "current" }] }`.
- `POST /v1/sign-out` takes the calling device off the list. The app signs out
  of Supabase itself and keeps its workspace.
- `POST /v1/account/delete` deletes the account's rows, its devices and the
  Supabase user. The app asks the user to confirm first. Every device keeps
  its local copy.

### Signing in on the Mac and iPhone

`Sources/TaktSync` holds both apps' half. `SyncServer` names the
Supabase project (its URL and publishable key) and the redirect,
`takt://auth-callback`, which has to be on the project's allowed
redirect URLs. `SyncEndpoints` is the server and project in use: those
defaults, or a self-hosted pair entered under "Use a different server"
(on the Mac and Android) and kept apart from the sign-in, through signing
out. Changing it signs the device out first. See
[self-hosting](self-hosting.md).

- **Email and password**: sign in, or create an account. When the project
  asks for confirmed emails, signing up says "check your email". "Forgot
  password?" calls Supabase's `recover`. The emailed link opens the app, which
  signs in and asks for a new password.
- **Google**: Supabase's web flow in `ASWebAuthenticationSession`, coming back
  on `takt://auth-callback`.
- **Apple**: on the iPhone, the native sheet. The ID token and a hashed nonce
  go to Supabase's `id_token` grant. The Mac is signed without a provisioning
  profile, so it can't hold the Sign in with Apple entitlement and uses the
  web flow, as for Google.

The Supabase session lives in the Keychain, through the SDK's own
`KeychainLocalStorage`, readable after first unlock so a background refresh
can sync. Before each request the transport asks the client for a token,
which refreshes it first if it has expired. A `401` refreshes once and
retries. A refresh Supabase refuses is "signed out". A refresh that fails
because the device is offline is only a failed cycle.

The device id is a UUID, made the first time and kept in the Keychain across
sign-outs. A device signed in under the old server-issued accounts keeps its
id, drops the old token, and asks to sign in again.

### `POST /v1/push`
Request:
```json
{ "changes": [
  { "table": "tasks", "id": "<uuid>", "op": "upsert", "hlc": "<hlc>",
    "values": { "title": "Buy milk", "updatedAt": "2026-10-02 09:00:00.000" } },
  { "table": "tasks", "id": "<uuid>", "op": "delete", "hlc": "<hlc>" } ] }
```
Response: `{ "accepted": <n>, "cursor": <server seq after applying> }`

For each change, the server resolves it against the stored row
`(data, colHlc, deleted, deletedHlc)`:
- **upsert:** for each column, the incoming value wins if its HLC is greater than
  `colHlc[col]`, or if the column has none. If the row is deleted and
  `hlc > deletedHlc`, the row comes back: `deleted = false`, and the incoming
  columns are applied over the old data. That is what undoing a delete looks
  like. If `hlc <= deletedHlc`, the change is ignored.
- **delete:** the row is deleted if `hlc` is greater than every `colHlc`. Then
  `deleted = true` and `deletedHlc = hlc`.
- If anything changed, the row gets a new `seq` from one global sequence.
  `lastDeviceId` records the pusher, for diagnostics only.

Pushing the same change twice changes nothing.

### `GET /v1/changes?since=<seq>&limit=<n>&wait=<seconds>`
Response:
```json
{ "rows": [ { "table": "tasks", "id": "<uuid>", "deleted": false,
              "values": { "...every column..." }, "hlc": "<max colHlc>" } ],
  "cursor": 1234, "hasMore": false }
```
The response holds the rows whose `seq > since`, in `seq` order, **including
rows the caller itself last wrote**. Skipping those looks like a free
saving, but it's wrong: a row the caller last wrote can still hold another
device's earlier edit to a different column that the caller has never seen.
Re-applying your own row is harmless. It also returns the cursor to resume
from. If there is nothing to send and `wait > 0` (at most 25), the request
waits for new rows, woken through Postgres `LISTEN/NOTIFY`.

The long poll is the fallback. Cloud Run bills every second a request is
open, so a device holding one all day cost money while nothing changed; where
the server says Realtime is on (below), devices wait there instead.

### `GET /health`
Returns `200 {"ok":true,"realtime":<bool>}`. Unauthenticated. `realtime`
says how a device should wait for other devices' changes.

### News on Supabase Realtime

When `realtime` is true, every push that writes at least one row also
broadcasts, inside its transaction, on the account's **private** Realtime
channel `sync:<account id>`:

```json
{ "event": "changed", "payload": { "cursor": 1234 } }
```

It means only "there is something to pull"; the rows still come from
`/v1/changes`. The pushing device hears its own broadcast too, which costs
one empty pull.

A device joins `realtime:sync:<account id>` on the app's Supabase project
(`wss://<ref>.supabase.co/realtime/v1/websocket?apikey=<publishable
key>&vsn=1.0.0`) with `config.private = true` and its Supabase access token.
The policy the server keeps on `realtime.messages`
(`sync-server/src/realtime.rs`) lets a signed-in user read only the channel
named after their own id. The server makes it at boot, and reports
`realtime: false` if it can't, or if the database has no `realtime` schema
(plain Postgres, self-hosting).

While joined, a device:
- pulls once on joining, for whatever was sent while it wasn't listening;
- pulls on every `changed`;
- hands the channel a fresh token when it refreshes its session (or rejoins);
- still runs a cycle every five minutes, in case a message was lost.

If it can't connect or join, it long-polls as before and retries the channel
with backoff. The websocket's heartbeats go to Supabase, not to the server,
so an idle device costs the server nothing.

## A client sync cycle

1. **First cycle only:** if `needsSnapshot = 1`:
   - Set `recording = 1` and add an `insert` outbox entry for every existing
     row in the synced tables, in parent-first table order.
   - Clear `needsSnapshot`.
   - Pull before pushing (step 3, then step 2).
2. **Push:**
   - Read the outbox in `seq` order and merge entries for the same row. The
     merged op is `delete` if the last entry is a delete; otherwise `upsert`,
     with every column if any entry was an insert, or else the union of the
     changed columns.
   - Read the values from the **live row** at push time.
   - Stamp each change with an HLC from its newest `changedAtMs`, in that
     order, starting from the stored clock; the clock is stored again when
     the batch is acknowledged.
   - Send them in batches of 100. After each accepted batch, delete the outbox
     entries up to the batch's highest `seq`.
3. **Pull:**
   - Fetch pages until `hasMore` is false.
   - Apply **all of them in one transaction** with `applying = 1` and
     `PRAGMA defer_foreign_keys = ON`:
     - Upsert: `UPDATE` if the row exists, else `INSERT`. Never use `INSERT OR REPLACE`, because a replace deletes, and a delete cascades.
     - Delete: `DELETE`.
     - Skip any row that still has an outbox entry, so a local edit made
       during the cycle is not overwritten. The next cycle resolves it.
     - If an insert hits a unique conflict on another key
       (`daily_contributions(dailyId, dayKey)`,
       `tasks(sourceSystem, sourceId)`, `task_lists(workspaceId, systemRole)`),
       the **smaller id wins on every device**, so two devices that made a row
       for the same key settle on the same one instead of each deleting its own.
       If the local row is smaller, keep it and queue a `delete` outbox entry for
       the incoming id. Otherwise delete the local row (recording on) and insert
       the incoming one. For `daily_contributions` the winner then takes
       `MAX(secondsLogged)` and the earlier non-null `completedAt` of the two,
       with recording on. Inbox rivals (`task_lists`) are the one exception:
       the incoming Inbox always wins and inherits the local Inbox's tasks.
   - Then set `applying = 0` and run **workspace adoption**.
   - Then delete orphans until `PRAGMA foreign_key_check` is clean. A task whose
     list was deleted on another device is removed, and its tombstone syncs.
   - Commit, then advance `cursor` and the HLC.
4. **Workspace adoption:**
   - `canonicalWorkspaceId` is the first workspace id a device ever receives
     from the server. If the server has none, it is the device's own workspace.
   - For any other local workspace:
     - Move the tasks in its Inbox into the canonical Inbox, then delete its Inbox.
     - Re-point its folders, lists and conditions to the canonical workspace.
     - Keep its `focus_contexts` row, under the canonical id, if the canonical
       workspace has none or this one was saved more recently; then drop it.
     - Delete it.
   - These writes run with recording on, so they sync.

When it runs: when the app comes to the foreground, 2 s after the last local write,
when the Realtime channel says there is news (or, without it, in a long-poll
loop while the app is in front), and in background refresh.

## Shared lists

A row belongs to a **space**: the account's own (the default, and every row
before this), or a shared list's. Design and reasoning are in
[shared-lists-plan.md](shared-lists-plan.md); this is the protocol.

**What goes where.** A shared list's space holds the list's `task_lists` row,
its `tasks`, their `task_metadata`, `task_dependencies` between two of its
tasks, and its `agent_instructions` (`list:<id>`). Nothing else. Their
*personal columns* — `task_lists`: `workspaceId`, `folderId`, `sortOrder`,
`visibleRootTaskId`, `priorityRank`; `task_metadata`: `matrixUrgency`, `matrixImportance`,
`matrixRank`, `kanbanColumn`, `focusRank`, `planningJSON`, `agentContext` —
never go there. They go to the pusher's own space as a **companion row**,
table `<table>#personal` with the same id, and a pull writes them over the
local row. A client that predates sharing skips the table, as it skips any
it doesn't know.

**Push.** A change names its space, `"space": "<uuid>"`, or leaves it out for
the account's own. The server writes a shared change only if the caller is a
member, refuses one for a table a shared space doesn't hold or for another
list's row (`400`), and **skips** (accepts without writing) one from a
non-member or a member's delete of the list itself, so a device just removed
doesn't retry its outbox for ever. Moving a task into a shared list sends it
there whole and deletes the copy in the account's own space; moving it out
deletes it from the shared space and sends it to the account's own whole. A
row whose space changes without an edit of its own (a task's metadata, when
the task moves; every row of a list just shared) is found by comparing where
it belongs with `sync_row_spaces`.

**One cursor.** Every push takes one global advisory lock rather than one per
account, so `seq` order is commit order across every space and a device's
single `since` covers its own space and every shared one. A join renumbers
the space's rows (`seq = nextval`), so they reach the new member past any
cursor, and every existing member re-applies them once, which is harmless.

**Pull.** Every row of a shared space carries `"space"`. Each page also
carries `"spaces"`: every shared list the account is in, with its list id,
the caller's role and the members (`accountId`, `email`, `role`, `you`). A
device brings `sync_spaces` to it on every pull: a new space whose list is
already here (the owner's other devices) has its rows marked as sent; a
space that has gone takes its rows from the device (a list you were in) or
leaves them as a private list (one you owned). A shared row is written
without its personal columns; a new shared list lands in the device's own
workspace, at the root of its sidebar. A delete from a shared space is
applied last, and only to a row that still belongs to that space, so a task
moved out on another device survives its old space's tombstone. A row from
the account's own space for a row that now lives in a shared list writes
only its personal columns, unless it is a task whose list says it moved out.

**News.** A push to a shared space nudges every member's own channel,
`sync:<account id>`, so a device keeps the one channel it already joins.

### Endpoints

| Request | What |
| --- | --- |
| `POST /v1/spaces` `{ "listId", "name" }` | Shares a list of the caller's own (idempotent). Answers the space. |
| `GET /v1/spaces` | `{ "spaces": [...] }`, as a pull lists them. |
| `POST /v1/spaces/<id>/invites` | Owner only. `{ "code", "spaceId", "expiresAt" }`: 128 random bits as 26 Crockford base32 characters, single use, seven days, at most 20 open per list (`429`). |
| `GET /v1/invites/<code>` | What the invite is for: `{ "spaceId", "name", "ownerEmail", "expiresAt", "joined" }`. `404` unknown, `410` used or expired. |
| `POST /v1/invites/<code>/accept` | Joins, renumbers the space's rows and nudges its members. Answers the space. |
| `DELETE /v1/spaces/<id>/members/<account>` | A member leaves (their own id), or the owner removes one. The owner can't leave (`403`). |
| `DELETE /v1/spaces/<id>` | Owner only: stops sharing. The space's rows go from the server; the owner's device that asked sends the list back to its own space. |
| `GET /join/<code>` | A page that opens `takt://join/<code>`, or shows the code to type. |

Deleting an account hands each list it owns to its longest-standing member,
or deletes the list's space when it has none, and leaves every list it was
in.

## Subscription

Takt's hosted server charges £1 a year for sync, as Takt Sync: an
auto-renewing yearly subscription (`takt_sync_yearly`) sold only in the
iPhone and Android apps, through the App Store and Google Play. RevenueCat
merges the two stores into one entitlement, `sync`. The apps log in to
RevenueCat with the Supabase account id, so a purchase made on one phone
covers every device on the account, the Mac included. A server someone
hosts themselves charges nothing.

- **Off by default.** Nothing is checked unless the server runs with
  `REQUIRE_SUBSCRIPTION=true`. `COMP_ACCOUNTS` lists accounts that sync free;
  an `entitlements` row with `source = 'comp'` does the same and no webhook
  overwrites it.
- **What is charged.** `/v1/push`, `/v1/changes` and every shared-list call
  answer `402 {"error": "…", "subscription": "required"}` to an account
  without an active, unexpired entitlement. `/health`, `/v1/devices`,
  `/v1/account`, `/v1/sign-out` and `/v1/account/delete` never do, so a
  lapsed device can say why, and its owner can leave or delete the account.
  Each member of a shared list needs their own subscription to sync it; the
  others' devices are unaffected by one member lapsing.
- **`GET /v1/account`** adds `"subscription": {"required", "active",
  "expiresAt", "comp"}`.
- **`POST /v1/webhooks/revenuecat`**, authorised by the header RevenueCat is
  configured to send (`REVENUECAT_WEBHOOK_SECRET`, bare or as a Bearer
  token; 404 without it). Each event is applied once, by its id
  (`entitlement_events`). A purchase, renewal, product change,
  uncancellation, non-renewing purchase, cancellation or billing issue marks
  the account active until the latest expiry it has heard of: a cancellation
  stops the renewal, not the year paid for, and a billing issue's grace is
  the expiry RevenueCat reports. An expiry ends it, unless a later renewal
  has already moved the expiry past it. A transfer (a restore signed in as
  another account) moves the entitlement. An event for another entitlement,
  or with no account id among its ids, is acknowledged and ignored.
- **The apps on a 402** stop syncing and say so — "Hosted sync needs Takt
  Sync (£1 a year). Subscribe in the iPhone or Android app, or self-host" —
  and try again only after the backoff, or at once after a purchase.
- **Setting it on the hosted server.** `scripts/configure_billing.sh` copies
  `REVENUECAT_WEBHOOK_SECRET`, `COMP_ACCOUNTS` and `REQUIRE_SUBSCRIPTION`
  from `~/.config/takt/revenuecat.env` onto `takt-sync`, leaving the rest of
  its environment as it is, and checks that the webhook answers. The phone
  builds read their RevenueCat keys from the same file.
- **RevenueCat's side.** Project `projc201565e`, with the apps
  `app3b3ef9c134` (App Store) and `appc8a22a6ddb` (Play), both
  `uk.co.maybeitssoftware.takt`. Their products are `takt_sync_yearly` and
  `takt_sync_yearly:yearly` (Play's `productId:basePlanId`; the base plan is
  `yearly`), both attached to the entitlement `sync`, which is the one the
  server counts. The webhook "Takt sync server" posts every event to
  `/v1/webhooks/revenuecat`; its Authorization header (`Bearer` and the
  secret) is set in the dashboard, since RevenueCat's API can't set it. The
  project's onboarding left a Test Store app and an unused entitlement,
  `uk_co_maybeitssoftware_takt_pro`.

## Server storage (Postgres)

In the `sync` schema of the Supabase project's database, which the Data API
doesn't expose; RLS is on with no policies besides. `account_id` is the
Supabase user's id.

```sql
CREATE TABLE devices (id UUID PRIMARY KEY, account_id UUID NOT NULL, name TEXT,
  platform TEXT, created_at TIMESTAMPTZ, last_seen_at TIMESTAMPTZ);
CREATE SEQUENCE row_seq;
CREATE TABLE rows (space_id UUID NOT NULL, account_id UUID NOT NULL,
  table_name TEXT, row_id TEXT,
  data JSONB NOT NULL DEFAULT '{}', col_hlc JSONB NOT NULL DEFAULT '{}',
  deleted BOOLEAN NOT NULL DEFAULT false, deleted_hlc TEXT, seq BIGINT NOT NULL,
  last_device_id UUID, PRIMARY KEY (space_id, table_name, row_id));
CREATE INDEX rows_space_seq ON rows(space_id, seq);
CREATE TABLE spaces (id UUID PRIMARY KEY, owner_account_id UUID NOT NULL,
  list_id TEXT NOT NULL, name TEXT, created_at TIMESTAMPTZ,
  UNIQUE (owner_account_id, list_id));
CREATE TABLE space_members (space_id UUID REFERENCES spaces ON DELETE CASCADE,
  account_id UUID, role TEXT, email TEXT, joined_at TIMESTAMPTZ,
  PRIMARY KEY (space_id, account_id));
CREATE TABLE entitlements (account_id UUID PRIMARY KEY, active BOOLEAN,
  expires_at TIMESTAMPTZ, source TEXT, product TEXT, updated_at TIMESTAMPTZ);
CREATE TABLE entitlement_events (id TEXT PRIMARY KEY, received_at TIMESTAMPTZ);
CREATE TABLE space_invites (code TEXT PRIMARY KEY, space_id UUID REFERENCES
  spaces ON DELETE CASCADE, created_by UUID, created_at TIMESTAMPTZ,
  expires_at TIMESTAMPTZ, used_by UUID, used_at TIMESTAMPTZ);
```

An account's own space has the account's id and no `spaces` row; on a
personal row `account_id` is the owner, on a shared one the last writer.
`seq` is one sequence across all spaces, so a cursor has gaps; it only has
to rise. Every push takes one advisory lock, which keeps `seq` order commit
order across spaces, so one cursor reads a device's own space and its shared
ones (see [Shared lists](#shared-lists)).
