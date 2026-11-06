# Reminders

Local notifications for what is due. Each device keeps its own settings (Settings → Reminders on the Mac, iPhone and
Android), off until switched on:

| Reminder | Default | Goes off |
|---|---|---|
| At a task's due time | — | when an open task with a due *time* is due |
| Due today | 09:00 | each day something is due, naming it (timed or a due day) |
| Dailies left | 20:00 | each day with dailies due and not yet ticked, naming them |

Nothing is sent from a server: the sync server never reads what a task says (`docs/e2e-sync-plan.md`), so reminders
are scheduled on the device. The settings don't sync, since they're about which device buzzes. Every platform uses
the same keys: `reminders.dueTime.enabled`, `reminders.dueToday.enabled`/`.minute`,
`reminders.dailiesLeft.enabled`/`.minute` (minutes after local midnight).

## What goes off: the core

`core/src/reminders.rs`, `CoreWorkspace.reminders(settings, now, zone, days, limit)`, decides every reminder for the
next `days` local days, soonest first. A client never works it out itself; it hands the list to the platform. Ids are
stable (`due.<task>`, `today.<day>`, `dailies.<day>`), so setting the list again replaces in place. Open tasks only:
not done, not archived, not lists, not inside a closed or archived list item. A daily counts as done for the day when
its contribution for that local calendar day has `completedAt`.

## Mac and iPhone

`Sources/TaktWorkspace/WorkspaceReminders.swift`: `ReminderPreferences` (UserDefaults), and `ReminderScheduler`,
which replaces every pending `takt.reminder.*` request with calendar-triggered ones. iOS keeps at most 64 pending
notifications per app, so both ask for a week, at most 48.

- **Mac**: `Takt/Managers/ReminderCoordinator.swift` sets them again after every write or pull
  (`WorkspaceViewModel+Refresh`), on activation and when the settings change. A tap on a due-time reminder reveals the
  task (`FocusNotificationManager` is the notification centre's delegate).
- **iPhone**: `mobile/ios/TaktMobile/Features/Reminders/ReminderService.swift`, on every `WorkspaceModel` change. A
  tap opens the task's inspector.

Text is fixed when set, so a reminder written on another device after the app last ran isn't set until the app next
opens or syncs.

## Android

`mobile/android/app/.../reminders/Reminders.kt`. Each reminder is an alarm with a ten-minute window
(`AlarmManager.setWindow`) rather than an exact alarm, which needs a permission Play only gives alarm-clock apps. When
one goes off, `ReminderReceiver` asks the core again *then*: a task finished since stays quiet, the text is current,
and the alarms are set again. So Android keeps reminding without the app being opened. Alarms are set again after a
restart, an app update, or a clock or time-zone change (`ReminderRescheduleReceiver`). Android 13 and later ask for
notification permission when the first reminder is switched on.
