# Shared lists

A plan, written 2026-11-08, and built the same day: steps 1 to 6 below.
[sync](sync.md#shared-lists) is now the description of what is; this file
records why, and where the build differs from the plan (next section).

## What was built, and where it differs

- **One cursor, one lock.** The plan gave a device one cursor per space.
  Instead every push takes a single global advisory lock, so `seq` order is
  commit order across all spaces and the integer `since` every client
  already sends still covers them all. No client's pull loop changed. A join
  renumbers the space's rows so they reach the new member past any cursor.
  The cost is that pushes no longer run side by side across accounts. At
  free-tier scale a push holds the lock for milliseconds; revisit it
  (per-space cursors) if pushes ever queue.
- **News on the account's own channel.** A push to a shared space nudges
  `sync:<account id>` for each member, rather than a `space:<id>` channel.
  The same number of messages, and no client's Realtime code changed.
- **Companion rows, not new tables.** Instead of `task_personal` and
  `list_placements`, a shared row's personal columns travel in the account's
  own space as `<table>#personal` with the same id. No synced table changed
  shape, so no client older than sharing breaks on a column it doesn't know.
  `v37_shared_lists` adds only the device's own bookkeeping
  (`sync_spaces`, `sync_row_spaces`, `sync_push_plan`).
- **A removed member's changes are skipped, not refused.** A device just
  taken out of a list can't know until its next pull; refusing its batch
  would leave its outbox stuck for ever.
- **Who did what, and assignment** came a step later, as v38
  (`v38_task_authorship`): `tasks.createdBy`, `tasks.completedBy` and
  `task_metadata.assigneeAccountId`, all shared. The core stamps the first
  two where it creates and closes tasks in a shared list, from the account
  id the list's members name `you`; reopening clears `completedBy`. A task
  assigned to someone else is left out of Today, next up and the focus
  ladder (`core/src/assignment.rs`); unassigned, it is anyone's.
- **Emails, not display names.** Members are shown by the email they sign in
  with.

Today Takt has one user per workspace. Every row the server holds is keyed
by `(account_id, table_name, row_id)`, a device reads only its own account's
feed, and the Realtime channel is `sync:<account id>`. Nothing can be shown
to a second person.

The goal is for a list to be **shared**: its owner invites other Takt
accounts to it, and each of them sees the list, its tasks and its subtasks
in their own sidebar. They can edit them on any of their devices, offline
included, and the edits merge as edits do today. Everything else stays
private to each account: other lists, conditions, habits, focus sessions,
the agent's instructions, themes and preferences.

Out of scope for the first version: sharing a folder, sharing a single
task, public read-only links, comments, and per-person permissions finer
than "owner" and "member". Assigning a task to a member is its own step,
below.

## The idea: spaces

A **space** is a set of rows that share one feed and one set of readers.
Every account has one personal space, which is what syncs today. Sharing a
list creates a new space holding that list's rows, and its members read that
space as well as their own.

The server still knows nothing about tasks. It learns only one new thing:
each row belongs to a space, and an account may read and write a space only
if it is a member. Merging is unchanged: per column, by HLC.

### Why not copy shared rows into each member's account

The other way to do this is fan-out on write: a push to a shared list writes
a copy into every member's account feed. It keeps the client unchanged, but
it stores each shared row once per member and has to merge every copy, and a
member removed from a list leaves copies behind that have to be found and
tombstoned. It also gets in the way of end-to-end encryption (below), where
a list's rows must be sealed under one key that every member holds. A space
stores each row once and gives each list one obvious key.

## Server

### Schema

```sql
CREATE TABLE spaces (
  id UUID PRIMARY KEY,
  kind TEXT NOT NULL CHECK (kind IN ('personal', 'shared')),
  owner_account_id UUID NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE space_members (
  space_id UUID NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
  account_id UUID NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('owner', 'member')),
  display_name TEXT,            -- what the other members see
  joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (space_id, account_id)
);
CREATE INDEX space_members_account ON space_members(account_id);

CREATE TABLE space_invites (
  code TEXT PRIMARY KEY,        -- 128 random bits, base32
  space_id UUID NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
  created_by UUID NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  used_by UUID,
  used_at TIMESTAMPTZ
);
```

`rows` gains `space_id UUID NOT NULL`, and its key becomes `(space_id,
table_name, row_id)`. A migration creates a personal space for every
account, with the same id as the account, and sets `space_id = account_id`
on every existing row. That keeps the change easy to check and means an old
client's account feed *is* its personal space's feed. `account_id` stays on
`rows` as the last writer's account, for diagnostics, as `last_device_id`
does now.

The advisory push lock moves from per account to per space:
`'push:' || space_id`. Seq order only needs to be commit order within one
feed.

### Wire protocol

- **Push.** Each change carries `"space": "<uuid>"`. If it is missing, the
  change goes to the caller's personal space, which is what every client
  sends today. The server refuses the whole batch (`403`) if the caller is
  not a member of a named space, or if the change is for a table a shared
  space does not take (below). One push may touch several spaces. It takes
  their locks in id order, so two pushes cannot deadlock.
- **Pull.** `GET /v1/changes` takes one cursor per space,
  `?since=<personal seq>&spaces=<id>:<seq>,<id>:<seq>`, and returns rows
  tagged with their space, along with a `cursors` map. **One request covers
  every space the device follows**, so the number of pulls does not grow
  with the number of shared lists. That is the cost property the whole
  design depends on (see *Hosting costs*). A space the caller has left
  comes back as `"removed": ["<id>"]`, which tells the device to drop that
  space's rows locally.
- **Spaces.** `POST /v1/spaces` `{ "listId" }` makes a shared space for a
  list the caller owns. `GET /v1/spaces` lists the caller's spaces and their
  members. `POST /v1/spaces/<id>/invites` returns a code and a link,
  `takt://join/<code>`, with an https fallback. `POST /v1/join/<code>` joins.
  `DELETE /v1/spaces/<id>/members/<account>` removes a member; a member may
  remove themselves (leave), and the owner may remove anyone.
- **Realtime.** A push to a shared space broadcasts on `space:<id>`, and a
  device joins one channel per space over the one websocket it already
  holds. The policy on `realtime.messages` allows a channel `space:<id>`
  only to members of that space. A push to a personal space still
  broadcasts on `sync:<account id>`.

### What a shared space accepts

| Table | In a shared space | Why |
| --- | --- | --- |
| `task_lists` | the one shared list | |
| `tasks` | yes | |
| `task_metadata` | yes, minus per-person columns | see below |
| `task_dependencies` | only if both ends are in the space | an edge into a private task would reveal it |
| `agent_instructions` | the `list:<id>` scope only | |
| everything else | no | conditions, habits, focus, themes and preferences stay personal |

The server only checks table names. The client is the one that knows which
columns are personal.

## Client and core

### Which rows go where

A local row of a synced table is in a shared space if it is the shared list,
or a task (or its metadata, or a dependency) whose `listId` is that list.
The core works this out when it builds a push
(`core/src/sync/wire.rs`). Nothing in a trigger changes: the outbox still
records what changed, and the push decides where each change goes.

**Moving a task into or out of a shared list** moves it between spaces. The
push sends a delete to the old space and an insert with every column to the
new one, for the task and each of its descendants, which move with it
because their `listId` follows the parent's. Moving a task with forty
subtasks into a shared list is therefore eighty-odd changes in one push,
which still fits in one batch.

### Per-person state on a shared task

`task_metadata` mixes facts about the task, which everyone should see, with
one person's way of working on it, which nobody else should overwrite.

| Shared | Personal |
| --- | --- |
| `priority`, `startAt`, `tagsJSON`, `recurrenceRule`, `externalLinksJSON`, `linkDetailsJSON`, `waitingOn`, `waitingFollowUp*`, `followUpOfTaskId`, `seriesTaskId`, `seriesTitle` | `matrixUrgency`, `matrixImportance`, `matrixRank`, `kanbanColumn`, `focusRank`, `planningJSON` (conditions are personal ids), `agentContext` |

A new local table, `task_personal(taskId PRIMARY KEY, accountId, …the
personal columns)`, synced in the **personal** space, holds the right-hand
column for tasks in shared lists. The core's reads join it over
`task_metadata` for a task in a shared list, and its writes send those
columns there instead. For a private task nothing changes, so neither the
private rows nor every existing reader has to move. The push leaves the
personal columns out of a shared `task_metadata` row.

`planningJSON` is the hard one. A requirement names a condition, and
conditions belong to one account. Keeping requirements personal means each
member decides for themselves when a shared task is doable, which is the
right answer for the first version.

Recurring tasks work unchanged: the next turn is a new row in the same list,
so it lands in the same space. Follow-ups made from a shared task go into
the same list (they do today), so they are shared too. Two members' devices
derive the same follow-up id, so the follow-up merges into one row rather
than two.

### The shared list on a member's device

- **Workspace adoption must leave it alone.** Today adoption takes any
  workspace that isn't the canonical one and folds it in (`sync.md`, step
  4). A shared list arrives with its owner's `workspaceId`, so without a
  change a member's device would adopt it, re-point the list to the
  member's own workspace and push that back, moving the owner's list into
  the member's workspace. Shared lists get their own local workspace row per
  space, marked `kind = 'shared'`, which adoption skips and which is never
  pushed.
- **Where it sits in the sidebar** is personal. `folderId` and `sortOrder`
  move to a personal `list_placements(listId, folderId, sortOrder)` row for
  a shared list, as the per-person task columns do. Archiving a shared list
  archives it for everyone. Hiding it is leaving it.
- **Undo** works as it does now: an undo is ordinary writes, and they merge.
  An undo step can undo a member's change only if it was pulled into this
  device's journal window, which it never is: the journal records local
  writes only. So undo reverts your own edits and never anyone else's.
- **Leaving or being removed** deletes the space's rows locally with
  recording off, so the delete is not pushed to the space. A private task
  that depended on a shared one loses the edge, as an orphan does today.
- **The CLI** writes through the core, so its writes to a shared list land
  in the right space with no change of its own. `lists_list` and its
  friends gain a `shared` field and a `members` tool.

### Who did what

`tasks` gains `createdBy` and `completedBy` (account ids). The members'
names come from `GET /v1/spaces`. That's enough for "completed by Sam" in
the inspector. A full activity feed is not part of the first version.

### Assignment (a later step)

`task_metadata.assigneeAccountId`, shared. Today and focus would then show
a shared task only to its assignee, or to everyone while it has none. It
needs nothing from the server, so it can wait until people ask for it.

## End-to-end encryption

`docs/e2e-sync-plan.md` seals every value under one **account key** that
only the account's devices hold. A shared space can't use it: its members
have different account keys. The fit is:

- Each space has a random 256-bit **space key**. A personal space's key is
  the account key, which changes nothing for a private workspace.
- Each account has a long-lived X25519 **identity key pair**. The public
  half goes on the server, and the private half is sealed under the account
  key, so every one of the account's devices has it once paired.
- `space_members` gains `sealed_key`: the space key sealed to that member's
  public key. The owner seals it when an invite is accepted. That is a
  second step, the owner's device being online after the join, unless the
  invite link carries the key in its fragment (`takt://join/<code>#<key>`),
  which the server never sees. **Recommended:** the fragment, so joining
  needs only the one device.
- Removing a member **rotates** the space key: the owner's device re-seals
  the space's rows under a new key and seals that key to the members who
  remain. Until then the removed member can still read rows they already
  held, which no scheme can prevent.

If shared lists ship before E2E, none of this is needed yet, but the
`spaces` schema should already carry the nullable `sealed_key` column, so
that E2E adds no migration to it. Whichever ships second has to handle the
other, and it costs less if the space model goes in first: E2E then
handles one key per space from its first day, rather than retrofitting
per-space keys onto per-account ones.

## Hosting costs

### What runs today

- **Cloud Run** `takt-sync`, europe-west1: 1 vCPU, 512 MiB, at most one
  instance (`maxScale: 1`), concurrency 80, scaling to zero. It bills only
  while a request is open.
- **Supabase**: Postgres (the `sync` schema), Auth and Realtime. As far as
  the repo records it (`self-hosting.md`), on the free plan.
- **Cloud Build** on each deploy.

For one person's devices, all of that is within the free tiers.

### What sharing adds, by resource

The numbers below take one household as the unit: **one shared list, three
members, three devices each, two of them in the foreground at a time**,
with about 100 edits a day to the shared list. Edits that land within 2 s of
each other go in one push, so call it 60 pushes a day. Prices are list
prices as of mid-2026. Check them against the Cloud Run and Supabase pricing
pages before relying on them.

| Resource | What sharing changes | Per household per month | Free allowance | Past it |
| --- | --- | --- | --- | --- |
| Cloud Run requests | Each push wakes every foreground device of every member, so one push leads to about six pulls instead of one | 60 × 7 × 30 ≈ **12,600** requests | 2 million | $0.40 per million |
| Cloud Run CPU | Same requests, ~0.1 vCPU-s each (billing rounds up to 100 ms) | ≈ **1,300 vCPU-s**, ≈ 650 GiB-s | 180,000 vCPU-s, 360,000 GiB-s | ≈ $0.000024 per vCPU-s, $0.0000025 per GiB-s |
| Postgres storage | **Nothing.** Shared rows are stored once, in the space. Membership and invites are bytes | ≈ 0 | 500 MB (free plan) | Pro: 8 GB included, then ≈ $0.125 per GB |
| Each new collaborator's own account | Their personal workspace, which they'd have anyway if they used Takt | ~1.5 MB plaintext, 5–6 MB under E2E | (same 500 MB) | |
| Supabase egress (database to Cloud Run, Realtime) | Pulls return whole rows to more devices | ~12,600 pulls × a few KB ≈ **50–100 MB** | 5 GB (free), 250 GB (Pro) | ≈ $0.09 per GB |
| Cloud Run egress to devices | Same bytes onward | ≈ 50–100 MB | | ≈ $0.12 per GB, under a cent here |
| Realtime messages | One broadcast per push on `space:<id>`, delivered to each joined device | 60 × 7 × 30 ≈ **12,600** | 2 million (free), 5 million (Pro) | $2.50 per million |
| Realtime connections | **Nothing.** A device joins its extra channels over the socket it already holds | +0 sockets | 200 peak (free), 500 (Pro) | |
| Auth | Each collaborator is a monthly active user | +2 MAU | 50,000 (free), 100,000 (Pro) | ≈ $0.00325 per MAU |
| Invite email | **Nothing**, if invites are links shared from the share sheet (recommended) | 0 | | |
| …or, if invites are emailed | Supabase's built-in mail only reaches the project's own team and is limited to a few an hour, so it needs custom SMTP (the server's environment already carries a Resend key) | a handful | Resend: 3,000/month, 100/day | Resend Pro ≈ $20/month |

**The design decision that matters most** is that one pull covers every
space. If each shared list were pulled separately, or long-polled
separately, requests and the open time Cloud Run bills would grow with the
number of lists × devices, and the safety cycle every five minutes would
multiply too. Bundled, a device makes as many requests as it does today,
plus the pulls other members' edits wake it for.

### What it adds up to

- **A household (the case above): no extra cost.** Roughly 1% of Cloud
  Run's free CPU and requests, under 1% of Realtime messages, and about 2%
  of free egress. Even a hundred such households stay inside Cloud Run's
  free tier on CPU (≈ 130,000 of 180,000 vCPU-s).
- **The step change is not load, it's dependency.** Sharing is the point at
  which someone other than you relies on the server. The Supabase free plan
  pauses a project after seven days with no requests, and keeps no backups.
  Pausing gets less likely the more people use it, and every member's
  devices hold full copies of what they share. Even so, if you open
  sharing to people outside your household, **Supabase Pro, $25 a month**
  (with $10 of compute credit, which covers the smallest instance), is the
  real line item. It also lifts the concurrent-connection ceiling from 200
  to 500, which is the free-plan limit you'd hit first: one socket per
  foreground device, on any account, shared lists or not.
- **Cloud Run's `maxScale: 1`** at concurrency 80 handles hundreds of
  devices, because pulls are short now that waiting happens on Realtime.
  Raising it costs nothing while idle. **Keep `minScale` at 0.** An
  always-on instance is roughly $10–15 a month in europe-west1, and nothing
  in this design needs one.
- **Past household scale**, cost per active account rises by about the
  number of people it shares with, on reads only: Cloud Run pulls, egress
  and Realtime deliveries. Storage and writes do not multiply.

### What would make it expensive

Spelled out so nothing slips in later:

- Per-space long-polls, or per-space pulls on a timer (see above).
- Fan-out on write (copying rows into each member's account), which would
  multiply storage and push time by the number of members.
- Pushing presence ("Sam is viewing this list") through the server. If it's
  ever wanted, use Realtime Presence directly from the clients. It costs
  messages, not Cloud Run time.
- Emailing a notification per change. Mobile push through APNs and FCM is
  free. Email per change is not, and it isn't wanted anyway.

## Steps

Each one ships on its own and leaves sync working for clients that haven't
updated.

1. **Server: spaces under the hood.** Migration (personal spaces equal to
   account ids, `rows.space_id`, the lock per space), and the push and pull
   accepting `space` and `spaces` while defaulting to the personal space.
   No client changes. Every existing device keeps working, which the
   server's integration tests show by running the current clients' wire
   fixtures.
2. **Core: the local model.** `task_personal`, `list_placements`, shared
   workspace rows that adoption skips, and the push assigning changes to
   spaces. The migration reinstalls the triggers, and the fixture is
   regenerated. Tested with two simulated accounts on one server.
3. **Server: membership.** `spaces`, `space_members`, `space_invites`, the
   endpoints, membership checks on push and pull, and the Realtime policy
   for `space:<id>`.
4. **Mac: share, join, leave.** "Share list…" in the list's menu and in the
   palette, the invite link, a shared marker and the member list in the
   sidebar and inspector, and "completed by".
5. **iPhone and Android**: the same, plus opening `takt://join/…`.
6. **CLI**: `shared` on lists and a `members` tool. That's the tool count in
   `scripts/mcp_smoke_check.py` and `docs/mcp-server.md` and a read
   classification in `core/src/agent_tools.rs`.
7. **(Later) assignment.**

E2E's steps then add `sealed_key` handling and rotation on removal. If E2E
lands first, steps 1 and 3 add them instead.

## Open questions

- **Can a member share the list onward, or only the owner?** Recommended:
  only the owner invites. Members can leave.
- **What happens when the owner deletes their account?** Recommended: the
  longest-standing member becomes owner. With no members left, the space is
  deleted.
- **Should a member be able to delete the list?** Recommended: no. Deleting
  is the owner's. For a member, "delete" in the list menu becomes "leave".
- **Free-plan limits as an abuse surface.** Anyone can already make an
  account. Sharing adds invites, which should expire after seven days and be
  limited to twenty outstanding per space.
