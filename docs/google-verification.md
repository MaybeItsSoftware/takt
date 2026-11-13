# Google OAuth verification

Takt's Google sign-ins go through Cloud project `takt-511416` (number
`974752893800`). Two of its scopes are **sensitive**, so past 100 users,
or to drop the "unverified app" screen, Google has to verify the app. Until
then only the test users listed on the consent screen can connect.

## What is asked for, and by which client

| Scope | Sensitivity | Mac | iPhone | Android |
| --- | --- | --- | --- | --- |
| `calendar.events` | sensitive | yes | yes | yes |
| `calendar.readonly` | sensitive | yes | yes | yes |
| `tasks` | sensitive | yes | — | — |
| `drive.file` | non-sensitive | yes | yes | yes |

The Mac asks for Calendar and Tasks in one browser sign-in (`google-connect`
in `desktop/src/state/google.rs`), through Takt's own Desktop client, which
`scripts/install_desktop.sh` compiles in from
`~/.config/takt/google-desktop.env`. Drive is a sign-in of its own in
Google's Picker on every platform (docs/links.md). The old
`drive.metadata.readonly` is restricted and never asked for; it must not be
listed on the consent screen.

## Branding

- **Homepage:** https://takt.maybeitssoftware.co.uk/ (`site/`, GitHub Pages)
- **Privacy policy:** https://takt.maybeitssoftware.co.uk/privacy/ — it has
  the Google section and the Limited Use statement Google looks for.
- **Authorized domain:** `maybeitssoftware.co.uk`, verified in Google Search
  Console by the account that owns the Cloud project.

## Scope justifications

For the console's *Data access* page, one per scope.

**`calendar.readonly`** — Takt lists the user's calendars so they can choose
which one Takt keeps their timed tasks in. It reads nothing else with this
scope; it cannot create calendars.

**`calendar.events`** — Takt is a task manager that puts each task with a
start or due time on the calendar the user chose, and keeps the two in step:
moving or renaming the event in Google Calendar moves or renames the task,
and completing, rescheduling or deleting the task updates the event. If the
user turns it on, events added to that calendar come back into Takt as
tasks. Takt only touches events in the one calendar the user picked.

**`tasks`** (Mac) — Takt keeps a copy of the user's lists in Google Tasks
and brings changes made there back, so tasks entered on a phone in Google
Tasks reach Takt. It creates and edits only the task lists it made, and the
user can remove them all from Takt's settings.

**`drive.file`** — the user picks files in Google's Picker to link to a task;
Takt sees only those files.

All Google data is fetched and sent from the user's device. The sign-in stays
in the device's keychain; nothing from Google reaches Takt's server except as
ordinary tasks the user syncs.

## The demo video

Unlisted on YouTube, in English, about four minutes. Recorded on the Mac,
whose sign-in runs in a real browser so the `client_id` shows, with a short
phone segment. Narrate or caption each step with *why* the scope is needed.

**Before recording:** use a Google test account with a calendar called
"Takt" and nothing private in it; disconnect Google in Takt (Settings →
Google) and revoke Takt at myaccount.google.com/permissions so the full
consent screen appears; set the browser to English; close notifications.

1. **Title card (5 s)** — "Takt — Google API scope demonstration", the
   project ID `takt-511416`.
2. **The app (15 s)** — open https://takt.maybeitssoftware.co.uk/ in the
   browser, scroll to the privacy link, then open Takt on the Mac.
3. **Consent (40 s)** — Settings → Google → *Connect Google Calendar, Tasks
   & Drive*. In the browser, click into the address bar and pause on
   `client_id=974752893800-…` long enough to read. Choose the account, show
   the consent screen with the app name and every scope listed, tick them,
   continue, back in Takt: "Google connected."
4. **`calendar.readonly` (20 s)** — *Calendar to sync with* lists the
   account's calendars; choose "Takt". Say: this is the only thing the scope
   is used for.
5. **`calendar.events`, Takt → Google (45 s)** — add a task and give it a
   start time (today, 15:00) and an estimate. Click *Sync Google now*. In
   Google Calendar the event appears at 15:00. Complete the task in Takt,
   sync, and the event's title gains `✓ `.
6. **`calendar.events`, Google → Takt (40 s)** — in Google Calendar drag a
   second task's event to 16:30 and rename it; sync; the task in Takt moves
   and renames. Turn on *Sync events too, not just tasks*, add an event in
   Google, sync, and it arrives in Takt as a task.
7. **`tasks` (40 s)** — after a sync, open Google Tasks (tasks.google.com or
   the Calendar side panel): Takt's lists are there. Add a task to one in
   Google Tasks, sync, and it appears in Takt.
8. **`drive.file` (30 s)** — on a task, link a file with *Pick from Google
   Drive…*: Google's Picker opens, pick one file, and it is linked to the
   task. Say: Takt sees only files picked here.
9. **Phones (45 s)** — on the iPhone, Settings → Google Calendar → *Connect
   Google Calendar*: show the sign-in sheet with its URL bar and the consent
   screen. On Android, Settings → Google Calendar → *Connect Google
   Calendar*: show Google's consent sheet with Takt's name and the two
   Calendar scopes.
10. **Disconnect (15 s)** — Settings → Google → *Disconnect Google Calendar,
   Tasks & Drive* on the Mac, and Takt gone from
   myaccount.google.com/permissions.

Upload as **Unlisted**, and paste the link into the verification form.
