# Google Calendar

Takt puts timed tasks on a Google calendar you choose and keeps the two in
step: move or rename the event in Google and the task moves or renames; edit,
complete or delete the task and the event follows. On the phones, Today also
shows the day's events from the calendars you pick, interleaved with timed
tasks.

The deciding is the Rust core's (`core/src/google_calendar.rs`), so the Mac,
the iPhone and Android behave the same. Each platform keeps only what the
Checkvist integration keeps (`core/src/checkvist.rs`): OAuth and sending
the requests the core builds.

## Which tasks become events

An **open** task becomes an event in the two-way calendar when it has a time:

| Task has | Event |
| --- | --- |
| a start (`planning.start_at_ms`) | starts at the start, lasts the estimate (30 min without one) |
| no start, a due **time** (`dueAt` with no `planning.due_date`) | ends at the due time, starts the estimate earlier |
| a due **date** only, or nothing | no event |

Archived tasks, tasks in archived lists and list-like items never become
events. Nothing older than a day is created, so switching it on does not
fill the calendar with history; tasks already linked are kept up to date
whatever their age.

Afterwards:

- **Completing** the task keeps the event (it happened) and prefixes its
  title with `✓ `. Reopening removes the prefix.
- **Cancelling** it, deleting it, archiving it or clearing its time deletes
  the event.
- **Deleting the event in Google** detaches the task: it is left exactly as
  it is, and no event is recreated until the task's time changes. Deleting a
  calendar slot is ambiguous — done, dropped or just unscheduled — so the
  answer that destroys nothing wins.

## Mapping

| Event | Task |
| --- | --- |
| `summary` (less a leading `✓ `) | title |
| `start` / `end` (`dateTime`) | start + estimate, or due − estimate … due, per the table above |
| `extendedProperties.private.taktTaskId` | id |
| `id` | `calendar_event_links.eventId` |

The description, location, attendees and colour are Google's and never
written after creation, so a note added to the event in Google survives. A
remote edit that turns the event all-day is ignored for timing (its title
still counts) and the next local change puts the time back.

## Identity, and why two devices are safe

An event Takt creates has a **derived id**: `takt` + the first 16 bytes of
`SHA-256("takt.calendar:<taskId>")` as lowercase hex, which is valid
base32hex as Google requires. Every device computes the same id for the same
task, so two devices that both create a task's event make one event: the
second insert gets `409`, and the core's fallback request patches the event
in place (also restoring one deleted earlier, which Google keeps as
`cancelled`). The task id is also written to
`extendedProperties.private.taktTaskId`, so an event is recognised even when
the link row has not reached this device yet.

On top of that, **only one device pushes at a time**. The lease is the synced
preference `googleCalendar.lease` (`{"device": <sync device id>, "until": ms}`,
one hour, renewed when half has gone). A device whose pass finds another's
unexpired lease reads nothing and writes nothing. If two offline devices both
take it, the derived ids keep the calendar free of duplicates and the next
passes converge through the conflict rule below. Importing events as tasks
is the one thing the derived ids cannot dedupe (the task id is random), which
is why it is opt-in and lease-only.

## Conflict rule

Per field group — the title, and the time (start and end together) — against
what was last agreed (`calendar_event_links.pushedJSON`):

- changed on one side only → that side wins;
- changed on both, to different values → **last writer wins**: the task's
  last write (`max(tasks.updatedAt, task_metadata.updatedAt)`) against the
  event's `updated`; a tie goes to Takt.

This is the workspace sync's rule (per-column LWW) at the granularity
Google gives us. Completion only travels from Takt to Google. Whatever the
merge settles is written to whichever side lacks it and becomes the new
agreed state. A remote change the task cannot take (a start past its due
date) is refused by the editor's own checks, and Takt's value is pushed back.

## Events made in Google

Ignored by default: an event without Takt's private property and without a
link is not a task, so by default only tasks sync. With **Sync events too, not just tasks** on (the synced preference
`googleCalendar.importEvents`), a timed, non-cancelled event in the two-way
calendar that ends no more than a day ago becomes a task in the Inbox — the
title, a start at the event's start, an estimate of its length — and the
event is tagged with the new task's id.

## Incremental sync

The lease holder lists `events?syncToken=…&showDeleted=true` page by page,
then hands every changed event to `CoreWorkspace::google_calendar_reconcile`,
which applies the local side (one `Google Calendar` undo step) and returns
the pushes to send. Each push's answer goes back through
`google_calendar_record_push`, which writes the link. The new
`nextSyncToken` is stored last, so a pass that dies half-way is repeated.

- **No token** (first pass, a new lease holder, a new calendar): a full list.
  A linked event missing from a full list counts as deleted.
- **`410 Gone`** on the list: the token has expired; the client clears it and
  starts again with a full list.
- An event not in an incremental page is assumed unchanged.

Sync tokens are per device and per calendar, in the local
`calendar_sync_tokens` table; they are never synced.

## Storage (`v30_google_calendar`)

- `calendar_event_links(taskId PK, calendarId, eventId, state, pushedJSON,
  remoteUpdatedAt, createdAt, updatedAt)` — **synced**, not journalled. No
  foreign key to `tasks`: a deleted task's link is how its event gets
  deleted. `state` is `linked` or `detached`.
- `calendar_sync_tokens(calendarId PK, syncToken, updatedAt)` — local.
- Synced preferences: `googleCalendar.calendarId` (the two-way calendar;
  none means two-way is off), `googleCalendar.importEvents`,
  `googleCalendar.agendaCalendars` (a JSON array of ids shown in Today) and
  `googleCalendar.lease`.

The Google sign-in itself is per device and never synced.

## OAuth

Scopes: `https://www.googleapis.com/auth/calendar.events` (read and write
events) and `https://www.googleapis.com/auth/calendar.readonly` (list the
calendars). Neither can create a calendar: make one called "Takt" in Google
Calendar and pick it, or pick any calendar you own.

- **Mac**: the existing `GoogleAccount` (loopback + PKCE, `docs/plugins.md`),
  which now asks for both scopes. An existing sign-in predates them and
  says to sign in again.
- **iPhone**: `ASWebAuthenticationSession` + PKCE against an **iOS** OAuth
  client, redirecting to its reversed client id. The completion closure is
  `@Sendable` (the `SyncAuth.swift` crash). The client id is the
  `GOOGLE_IOS_CLIENT_ID` build setting; without it the Calendar row is
  hidden. Tokens live in the keychain.
- **Android**: `Identity.getAuthorizationClient(…).authorize(…)` for the two
  scopes, which grants against the package's existing Android OAuth clients
  (per package and SHA-1) and returns a short-lived access token silently
  once consented. No refresh token is kept.

Google Cloud project `takt-511416`. Both scopes are **sensitive**: the OAuth
consent screen must list them, and past 100 users the app needs Google's
verification (a privacy policy, a demo video of the scope in use). Until
then only test users can grant them.

Drive linking signs in separately, with `drive.file` alone, through Google's
Picker (docs/links.md "Google Drive"); it never joins this grant's scopes.

## Platforms

| | Two-way sync | Today agenda | Settings |
| --- | --- | --- | --- |
| Mac | `GoogleCalendarTwoWayService`: after a local change, every 5 min and at launch | — (the desktop has its own day) | the Google Calendar plugin page |
| iPhone | `GoogleCalendarController`: on foreground and every 15 min while open | `CalendarAgendaSection` | Settings → Calendar |
| Android | `GoogleCalendarController`: on foreground and every 5 min while open | `CalendarAgenda` | Settings → Calendar |

The Apple transport (`GoogleCalendarClient`, in `TaktWorkspace`) is shared by
the Mac and the iPhone; Android's is `GoogleCalendarRepository` in `:data`.
Both only send what the core built and hand back what Google answered.
