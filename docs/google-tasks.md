# Google Tasks

Takt mirrors its lists into Google Tasks so the things you have to do are
on your phone. It is a mirror, not a sync: **Takt is the source of
authority**, and the rules below are what that phrase turns out to mean once
two people — you at a desk and you on a train — can both write.

## The shape

One Google Tasks list per Takt list, named the same. Lists are created on
the first pass, renamed when you rename them here, and deleted when you archive
or delete them here. Google Tasks lists that Takt did not create are never
read and never touched.

Within a list, every task is mirrored with its title, its notes and its due
day. Google Tasks nests exactly one level, so a subtask hangs under its parent
and anything deeper is mirrored flat rather than pushed into a shape Google
will refuse.

Google Tasks stores a due **day** and silently discards any time sent with it.
The day is taken in your calendar and sent as midnight UTC; reading one back
reverses that. Doing either half naively moves a late-evening task to the wrong
day, in opposite directions on either side of Greenwich.

A task typed in Google Tasks comes in with that day and no time. With
**Read times from task titles** on (off by default), a written time in its
title — `14:00`, `2pm`, `9.30am` — is its start on that day, and a range —
`14:00–15:00`, `2-3pm`, `9.30am to 11` — its start and length; such a task
is due by the end of its day rather than at noon, because a start after its
deadline is refused. Words such as "tonight" don't count, nor does a bare
`2-3`. The title itself is left as it is.

Turning Google Tasks off stops the mirror and leaves Google Tasks as it was.
**Remove Takt's lists from Google Tasks** (on the Google page whenever the
ledger holds any) turns it off, deletes every list in the ledger from Google,
with their tasks, and empties the ledger, so the next pass makes them afresh.
A list Takt adopted by name is in the ledger, and goes too; lists it never
mirrored don't. Nothing in Takt changes.

## Who wins

| What happened | What the mirror does |
| --- | --- |
| Anything changed in Takt | Pushed to Google Tasks |
| Task completed in Takt | Completed in Google Tasks |
| Task deleted in Takt | Deleted in Google Tasks |
| List archived in Takt | Its Google Tasks list is deleted |
| **Task ticked off in Google Tasks** | **Completed in Takt** |
| **Notes added in Google Tasks** | **Kept, and merged into the Takt task** |
| **Task typed into a mirrored Google list** | **Adopted into the matching Takt list** |
| Title, due date or notes *rewritten* in Google Tasks | The Takt version is written back over it, and the conflict is logged |
| Task deleted in Google Tasks | Recreated from Takt, and the conflict is logged |

The three rows in bold are the reason this is not simply a one-way push.
Completion is not a disagreement — it is the same answer arriving by another
route, so it is honoured rather than reverted. Notes added where Takt had
none, or appended to what Takt wrote, are additions rather than edits, and
additions are kept. A task typed straight into a mirrored list is information
too, so it is adopted rather than deleted: authority decides who wins an
argument, not who is allowed to write.

Rewriting is where authority bites. If the Google copy of a note no longer
contains what Takt wrote, something was thrown away, and Takt's copy
goes back over it. The same is true of a title or a due date, and of deleting a
task that Takt still has.

## The conflict log

Every time the authority rule discards something, it is written to
`google-tasks-conflicts.jsonl` in Takt's Application Support folder —
append-only, one JSON object per line, and tolerant of a torn tail, like
`daylog.jsonl`. The most recent entries are shown in *Settings → Google
Tasks*.

This is the point of the log: the rule means Takt sometimes overwrites
something a person deliberately typed on their phone, and a rule that does that
silently is indistinguishable from a bug.

## When it runs

- A few seconds after any local write, coalesced — typing a title is one push,
  not twelve.
- Every five minutes, to notice what was ticked off elsewhere. Google Tasks has
  no push channel to subscribe to.
- At launch, so anything done on a phone overnight has landed before you look.
- On demand, from *Sync now* on its Settings page.

A pass is never concurrent with itself: Google Tasks has no transactions, and
two passes racing would each read the other's half-finished work as remote
edits to argue with. A change arriving mid-pass queues one more pass rather
than being dropped.

## How it is put together

Three pieces, deliberately separate, because the interesting part is the
decision and the decision should be readable without a network:

| Piece | Where | What it is |
| --- | --- | --- |
| `GoogleTasksMirror` | `Sources/TaktCore/` | Pure. Given both sides and what was last pushed, returns the operations and the conflicts. All the rules above live here, and so do their tests. |
| `NativeGoogleTasksIntegrationPlugin` | `Takt/Plugins/Native/GoogleTasks/` | Transport. Lists, tasks, create, patch, delete, and paging. Knows nothing about when. |
| `GoogleTasksMirrorService` | `Takt/Plugins/Native/GoogleTasks/` | The driver. Snapshots the workspace, runs the planner, carries out the plan, writes the ledger and the log. |

### The ledger

`google-tasks-mirror.json`, beside the conflict log: Takt task id → the
Google task id and **what Takt last pushed for it**.

That last part is what makes the whole thing possible. Without it there is no
way to tell a remote edit from Takt's own echo coming back — both simply
look like "the two sides differ". Comparing each side against what was pushed
says which of them actually moved.

Losing the file is survivable. The next pass sees an empty ledger and adopts
whatever is already in the mirrored lists rather than duplicating it.

## Google Calendar

The same inbound rule reaches the other Google surface. When you create a
calendar event from a task, Takt remembers which event stands for which
task in `google-calendar-events.json`. Clearing that event off your calendar —
deleting or cancelling it — completes the task, on the same reasoning as a tick
in Google Tasks: it is what a person does to an event once it has happened.

Only events Takt created are watched, and only until they resolve.
