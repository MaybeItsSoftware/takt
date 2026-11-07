# Store listings

What to paste into App Store Connect and the Play Console. Field limits are in
brackets. Answers to the privacy forms follow from what the apps actually do;
if sync or analytics change, change these.

- **Privacy policy URL:** https://maybeitssoftware.github.io/takt/privacy/ — `site/privacy/`,
  served by GitHub Pages (`.github/workflows/pages.yml`), so the listings do not
  depend on the sync server. `sync-server/static/privacy.html` is the same text
  with its contact line filled in at serve time; change both together.
- **Bundle / package id:** `uk.co.maybeitssoftware.takt` on both stores
- **Category:** Productivity
- **Price:** free, no in-app purchases, no ads

## Shared text

**Name** [30]: `Takt: tasks and focus`

**Subtitle** (App Store) [30]: `Plan the day, then do it`

**Short description** (Play) [80]:
`Tasks, lists and a focus timer that pick what's next. Syncs with your Mac.`

**Promotional text** (App Store) [170]:
`Outline, board and matrix views of the same tasks, a day plan that knows how long you have, and a focus timer that keeps score.`

**Description** [4000]:

```
Takt is a task manager that helps you decide what to do next — and then
do it, at a steady beat.

PLAN
• Lists and folders, with tasks nested as deep as you like
• Three views of the same list: an outline that folds like Checkvist, a
  board, and an urgent/important matrix
• Today: the day in order, with a forecast of when you'll finish
• Due and start dates, repeats, estimates, tags, priorities and conditions
  ("at home", "low energy")
• Quick add that understands dates, tags and lists as you type

FOCUS
• A ranked list of what to work on now, with the reasons
• A running timer with a Live Activity and Dynamic Island on iPhone, and an
  ongoing notification on Android
• A score for each session and points over time

REVIEW
• A timeline of your day, a list of what got done, and progress charts
• Dailies: small habits ticked each day

EVERYWHERE
• Works fully offline; nothing leaves your phone unless you sign in
• Sign in with Apple, Google or email to sync with Takt on your other
  devices, including the Mac app
• Undo for everything, with a history of every step
• Themes: three built in, or make your own from three colours
• Widgets, and a quick-add shortcut

OPEN SOURCE AND FREE
• Open source under the GNU AGPL-3.0:
  github.com/MaybeItsSoftware/takt
• Sync between your devices is free, and you can self-host it for free

No ads, no tracking, no analytics.
```

**Keywords** (App Store) [100]:
`tasks,to-do,todo,planner,priority,focus,timer,pomodoro,lists,outline,kanban,habits,daily,gtd`

**Support URL:** https://github.com/MaybeItsSoftware/takt/issues — the
privacy page sends people to "the support link" on the store page.

**Uploading:** `scripts/upload_play_listing.sh` and
`scripts/upload_app_store_listing.sh` send this text and the framed images in
`design/store` (both take `--dry-run`). The App Store copy drops the Android
mention, which Apple rejects. The App Store key is open-parliament's, kept in
`~/.config/takt/asc.env`.

## App Store Connect

**App Privacy** ("Data used to track you": none). Collected, linked to the
user, not used for tracking, purpose **App Functionality** only:

| Data type | Why |
| --- | --- |
| Contact Info → Email Address | the sync account |
| User Content → Other User Content | the synced tasks, notes and lists |
| Identifiers → User ID | the account id that keys the synced rows |
| Identifiers → Device ID | each signed-in device's generated id and name, for the devices list |

Everything else: not collected. Collection only happens if the user signs in
to sync; that's still "collected" for Apple's purposes.

**Age rating:** every question "None"/"No" → 4+.

**Export compliance:** already answered in the build
(`ITSAppUsesNonExemptEncryption = NO`; it uses only HTTPS).

**Sign-in for review:** App Review needs a working account.
`scripts/review_account.sh` creates `appreview@maybeitssoftware.co.uk` with a
password (confirmed, so it never needs mail) and prints the password; put it
under App Review Information.

**Account deletion:** Settings → Sync → Delete account (guideline 5.1.1(v)).

**Screenshots:** 6.9" iPhone (1320×2868) and 13" iPad (2064×2752) — the
build runs on iPad (`TARGETED_DEVICE_FAMILY` 1,2), so both are required. Three to five: Today, the outline, Focus with the timer, the
board, a theme.

## Play Console

**Data safety:**

- Collects data: yes. Shares data: no.
- Encrypted in transit: yes.
- Users can request deletion: yes, in the app (Settings → Sync → Delete
  account), and the same steps are on the privacy page.

| Data type | Collected | Optional? | Purpose |
| --- | --- | --- | --- |
| Personal info → Email address | yes | optional (only to sync) | Account management, App functionality |
| Personal info → User IDs | yes | optional | Account management |
| App activity → Other user-generated content | yes | optional | App functionality |

**Foreground service** (Policy → App content → Foreground service
permissions): type *special use*, as declared in the manifest — "Shows the running focus timer as an ongoing notification with
pause and done, so the timer keeps time while the app is in the background.
Starts only when the user starts a focus session."

**Content rating:** questionnaire, category Utility/Productivity, all
answers "No" → Everyone.

**Target audience:** 13 and over (it isn't designed for children).

**Ads:** no.

**App access:** "All or some functionality is restricted" → give the same
review account as Apple's, noting that the app works without signing in.

**Graphics:** icon 512×512 (from `mobile/android/app/src/main/res`), feature
graphic 1024×500, and at least two phone screenshots.

**Release:** internal testing first, with `build/play/takt-<v>-<n>.aab`
from `scripts/build_play_bundle.sh`. Opt in to Play App Signing on the first
upload; then copy Play's app-signing SHA-1 into its own Google OAuth Android
client, or Google sign-in fails for store installs. The full set of clients,
the Supabase provider settings and where the build reads the Web client id
from are in [Google sign-in on Android](android-google-sign-in.md).

## Releasing Android

Both paths build with `scripts/build_play_bundle.sh` and upload with the
fastlane lane `internal` in `mobile/android/fastlane/Fastfile`, which puts the
bundle on the **internal** testing track as a completed release and leaves the
listing, images and screenshots alone. versionCode is the commit count on
`HEAD`; bump `versionName` in `mobile/android/app/build.gradle.kts` (or pass
it) for each release. Play only accepts API uploads once the app exists and a
first bundle has been uploaded by hand in the Console.

Credentials are a Google Play service account key (Play Console → Users and
permissions → invite the service account with release rights for Takt).

**Locally**, with `mobile/android/keystore.properties` in place and Ruby with
bundler installed:

```bash
./scripts/build_play_bundle.sh 0.3.0
PLAY_JSON_KEY_FILE=~/path/to/play-service-account.json \
  ./scripts/upload_play_internal.sh build/play/takt-0.3.0-<versionCode>.aab
```

`PLAY_SERVICE_ACCOUNT_JSON` (the key's contents) works in place of
`PLAY_JSON_KEY_FILE`. The script runs `bundle install` in `mobile/android`
the first time and refuses to start without one of the two.

**In CI**, run *Android internal testing*
(`.github/workflows/android-internal.yml`) from the Actions tab, optionally
with a versionName; or `gh workflow run android-internal.yml -f version_name=0.3.0`.
It rebuilds the upload keystore from secrets, runs the same build script on
Ubuntu, keeps the `.aab` as a run artifact and uploads it.

The repository secrets it reads (names only; `gh secret set NAME` prompts for
the value, or pipe it in):

```bash
base64 -i mobile/android/<upload-keystore>.jks | gh secret set ANDROID_KEYSTORE_BASE64
gh secret set KEYSTORE_PASSWORD
gh secret set KEY_ALIAS
gh secret set KEY_PASSWORD
gh secret set PLAY_STORE_SERVICE_ACCOUNT_JSON < ~/path/to/play-service-account.json
gh secret set TAKT_GOOGLE_WEB_CLIENT_ID   # optional; without it the Google button is hidden
```

The four keystore values are the ones in `mobile/android/keystore.properties`.
