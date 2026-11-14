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

What Google checks: the app name on the consent screen matches the app, the
`client_id` in the sign-in address is this project's, and each scope asked
for is visibly used. About four minutes, unlisted on YouTube. Three clips —
the Mac, the iPhone, the Android phone — joined into one.

### Getting ready (once, about 15 minutes)

1. **A Google test account.** Not your own: everything in it will be on
   screen. In Google Calendar make a calendar named `Takt`
   (calendar.google.com → *Other calendars* → **+** → *Create new calendar*).
   In Google Drive upload any harmless file, e.g. `Demo notes.pdf`.
2. **Clear Takt's old grant.** Sign in to myaccount.google.com/permissions as
   that account; if Takt is listed, click it → *Delete all connections*.
3. **On the Mac, in Takt:** open Settings (`⌘,`) → **Google** in the sidebar.
   If it says *Disconnect Google Calendar, Tasks & Drive*, click it. Make sure
   the Google switch at the top and the **Google Calendar**, **Google Tasks**
   and **Google Drive** switches under *Services* are all on. Close Settings.
4. **A demo list.** `⌘⇧N`, name it `Demo`, `Return`. Add two tasks (`Return`
   on the list, type, `Return`): `Write project brief` and
   `Book dentist appointment`.
5. **Browser:** Chrome, signed in to the test account only, language
   English, bookmarks bar hidden. Open these tabs: the Takt homepage,
   Google Calendar, Google Tasks (tasks.google.com), myaccount.google.com/permissions.
6. **Quiet the Mac:** Focus → Do Not Disturb on; quit Slack, Mail, Messages.
7. **Phones:** Takt installed on both, signed out of Google Calendar
   (Settings → Calendar / Google Calendar → *Disconnect* if shown).

### Recording the Mac (clip 1)

Start: `⌘⇧5` → *Record Entire Screen* → *Options* → Microphone: your mic if
you will narrate, *None* if you will caption → *Record*. Stop: the ■ in the
menu bar. The file lands on the Desktop. Do a full practice run first, then
redo steps 2–3 of *Getting ready* before the real take.

Say each line in quotes out loud (or leave it out and I can caption it).

1. **Title (5 s).** Show a note or slide reading
   `Takt — Google OAuth verification demo · project takt-511416`.
   — "This is Takt, a task manager for Mac, iPhone and Android."
2. **Homepage (15 s).** Chrome, the Takt homepage tab. Scroll to the bottom,
   click *Privacy policy*, scroll it briefly to the Google section.
   — "This is the app's homepage and privacy policy."
3. **Start the sign-in (20 s).** Switch to Takt. `⌘,` → **Google** →
   *Connect Google Calendar, Tasks & Drive*. Chrome opens Google's sign-in.
4. **Show the client ID (10 s).** Click into Chrome's address bar, press
   `End`, then move the cursor back until `client_id=974752893800-…` is in
   view. Hold for three seconds.
   — "The client ID is this project's, 974752893800."
5. **Consent (25 s).** Click the test account. On the consent screen, hold
   long enough that *Takt* and every permission can be read. Tick each box,
   click *Continue*. The browser says it is done; switch back to Takt, which
   shows *Google connected.*
   — "Takt asks for Google Calendar and Google Tasks."
6. **`calendar.readonly` (20 s).** Still on the Google page, click
   *Calendar to sync with* and choose **Takt**.
   — "Takt reads the list of calendars only so you can pick one. That is all
   it uses this permission for."
7. **A task onto the calendar (45 s).** Close Settings, open the **Demo**
   list, select `Write project brief` and press `i` to open the inspector. Move to
   **Start** (`j`/`k`, or click it), `Return`, press `t` for today, `Tab`, type `15:00`,
   `Tab`, type `1h`, `Return`. Do the same on `Book dentist appointment`
   with `16:00`. Press `⌘⇧R` to sync. Switch to the Google Calendar tab and
   refresh: both appear today at 15:00 and 16:00 in the Takt calendar.
   — "A task with a start time becomes an event in the calendar you chose."
8. **Completing it (20 s).** Back in Takt, select `Write project brief`,
   press `Space` to complete it, `⌘⇧R`. In Google Calendar, refresh: the
   event now reads `✓ Write project brief`.
   — "Completing the task updates its event."
9. **Calendar back to Takt (30 s).** In Google Calendar drag
   `Book dentist appointment` to 17:30, click it, rename it to
   `Dentist — Dr Lee`, save. Back in Takt, `⌘⇧R`: the task is renamed and its
   start is 17:30 in the inspector.
   — "Moving or renaming the event changes the task. That needs edit access
   to events, calendar.events."
10. **Events become tasks (25 s).** `⌘,` → **Google** → switch on *Sync
    events too, not just tasks*. In Google Calendar click tomorrow 10:00 in
    the Takt calendar, title `Team standup`, *Save*. In Takt `⌘⇧R`: `Team
    standup` arrives as a task.
11. **`tasks` (35 s).** Google Tasks tab, refresh: the **Demo** list is
    there with its tasks. Click *Add a task* in it, type `Buy printer ink`,
    `Return`. In Takt, `⌘⇧R`: `Buy printer ink` appears in Demo.
    — "Takt keeps each list in Google Tasks too, and brings back what you
    add there. It only changes the lists it made."
12. **`drive.file` (30 s).** Select any task in Demo, press `pl`, choose
    *Choose files from Google Drive…*. Being the first pick since
    disconnecting, Chrome shows Google's *Allow access* screen for "the
    specific Google Drive files you use with this app": hold on it for 3 s
    (this is the `drive.file` consent), *Allow access*, then click
    `Demo notes.pdf`, *Select*. Back in Takt the file shows as a chip in the
    links row just above Notes; hover it to show the preview.
    — "Takt sees only the files you pick here."
13. **Disconnect (15 s).** `⌘,` → **Google** → *Disconnect Google Calendar,
    Tasks & Drive*. In Chrome, the permissions tab, refresh: Takt is gone, or
    click it → *Delete all connections*.
    — "Disconnecting, or removing access here, stops it all."

Stop the recording.

### Recording the phones (clips 2 and 3, about 30 s each)

**iPhone.** Control Centre → *Screen Recording* (add it in Settings →
Control Centre if missing). In Takt: the lists screen → the gear (*Settings*)
→ **Google Calendar** → *Connect Google Calendar*. In the sign-in sheet, tap
the address bar so `client_id` shows, choose the test account, hold on the
consent screen, *Continue*, and back in Takt. Stop the recording.

**Android.** Swipe down twice → *Screen record* → *Start*. In Takt: open the
sidebar (☰) → **Settings** → **Calendar** → *Connect Google Calendar*.
Choose the test account, hold on Google's consent sheet with *Takt* and both
Calendar permissions, *Allow*, back in Takt showing it connected. Stop.

### Putting it together

AirDrop or copy the two phone clips to the Mac. To join and trim them:

```bash
# trim a clip: keep from 0:03 to 2:45
ffmpeg -i mac.mov -ss 00:00:03 -to 00:02:45 -c copy mac-trim.mov
# join: list the clips in order, then concatenate (re-encoding, since the
# phones record at other sizes)
ffmpeg -i mac-trim.mov -i iphone.mov -i android.mp4 -filter_complex \
  "[0:v]scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,setsar=1[a]; \
   [1:v]scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,setsar=1[b]; \
   [2:v]scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,setsar=1[c]; \
   [a][b][c]concat=n=3:v=1:a=0[v]" -map "[v]" takt-google-demo.mp4
```

(Without narration; add the audio streams if you narrated.) Upload to
YouTube → *Visibility: Unlisted*, title `Takt — Google OAuth verification`,
and paste the link into the verification form.
