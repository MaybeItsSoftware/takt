package uk.co.maybeitsadam.takt.reminders

import android.Manifest
import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import java.time.ZoneId
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.launch
import uniffi.takt_core.Reminder
import uniffi.takt_core.ReminderSettings
import uk.co.maybeitsadam.takt.MainActivity
import uk.co.maybeitsadam.takt.R
import uk.co.maybeitsadam.takt.app.AppContainer
import uk.co.maybeitsadam.takt.appContainer
import uk.co.maybeitsadam.takt.ui.ShellIntents

/**
 * Which reminders this phone sets, and when (docs/reminders.md): the Rust
 * core's `ReminderSettings`, kept in SharedPreferences under the same keys
 * the Mac and iPhone use. Per device, never synced — they are about which
 * device buzzes. Times are minutes after local midnight.
 */
class ReminderPreferences(context: Context) {
    private val prefs = context.getSharedPreferences("reminders", Context.MODE_PRIVATE)
    private val _settings = MutableStateFlow(read())
    val settings: StateFlow<ReminderSettings> = _settings.asStateFlow()

    fun update(change: (ReminderSettings) -> ReminderSettings) {
        val next = change(_settings.value)
        prefs.edit()
            .putBoolean(DUE_TIME, next.dueTime)
            .putBoolean(DUE_TODAY, next.dueToday)
            .putInt(DUE_TODAY_MINUTE, next.dueTodayMinute)
            .putBoolean(DAILIES_LEFT, next.dailiesLeft)
            .putInt(DAILIES_LEFT_MINUTE, next.dailiesLeftMinute)
            .apply()
        _settings.value = next
    }

    private fun read() = ReminderSettings(
        dueTime = prefs.getBoolean(DUE_TIME, false),
        dueToday = prefs.getBoolean(DUE_TODAY, false),
        dueTodayMinute = prefs.getInt(DUE_TODAY_MINUTE, 9 * 60),
        dailiesLeft = prefs.getBoolean(DAILIES_LEFT, false),
        dailiesLeftMinute = prefs.getInt(DAILIES_LEFT_MINUTE, 20 * 60),
    )

    /** The alarms set last time, so the next schedule can cancel the ones it no longer wants. */
    var scheduled: Set<String>
        get() = prefs.getStringSet(SCHEDULED, emptySet()).orEmpty()
        set(value) { prefs.edit().putStringSet(SCHEDULED, value).apply() }

    private companion object {
        const val DUE_TIME = "reminders.dueTime.enabled"
        const val DUE_TODAY = "reminders.dueToday.enabled"
        const val DUE_TODAY_MINUTE = "reminders.dueToday.minute"
        const val DAILIES_LEFT = "reminders.dailiesLeft.enabled"
        const val DAILIES_LEFT_MINUTE = "reminders.dailiesLeft.minute"
        const val SCHEDULED = "reminders.scheduled"
    }
}

/**
 * Sets an alarm for each reminder the core returns, and posts one when it
 * goes off.
 *
 * Each alarm has a ten-minute window (`AlarmManager.setWindow`), not an
 * exact time: exact alarms need a permission the user grants by hand, and
 * Play gives it outright only to alarm-clock apps. When one goes off, the
 * receiver asks the core again *then*, so a task finished since stays quiet
 * and the text is current, and every alarm is set again.
 */
class ReminderController(private val container: AppContainer) {
    private val context = container.context
    private val alarms = context.getSystemService(AlarmManager::class.java)
    val preferences = ReminderPreferences(context)

    /** Whether notifications may be shown: asked for at runtime from Android 13. */
    val canNotify: Boolean
        get() = (
            Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
                ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) ==
                PackageManager.PERMISSION_GRANTED
            ) && NotificationManagerCompat.from(context).areNotificationsEnabled()

    /** Sets the alarms again after any write or pull, and whenever the settings change. */
    @OptIn(FlowPreview::class)
    fun attach() {
        container.scope.launch {
            val session = container.awaitSession()
            schedule()
            merge(
                session.repository.database.invalidations.map { },
                preferences.settings.drop(1).map { },
            ).debounce(1_000).collect { schedule() }
        }
    }

    /** Replaces every alarm with the ones the core wants now. */
    suspend fun schedule() {
        val wanted = read(System.currentTimeMillis(), DAYS, LIMIT)
        val ids = wanted.map { it.id }.toSet()
        for (stale in preferences.scheduled - ids) alarms.cancel(alarmIntent(stale, 0))
        for (reminder in wanted) {
            alarms.setWindow(AlarmManager.RTC_WAKEUP, reminder.fireAtMs, WINDOW_MS, alarmIntent(reminder.id, reminder.fireAtMs))
        }
        preferences.scheduled = ids
    }

    /** An alarm went off: post [id] as the core reads it now, if it still wants it. */
    suspend fun fire(id: String, fireAtMs: Long) {
        // Read from just before it was due, so the core still counts it as to come.
        val current = read(fireAtMs - 60_000, 1u, 200u).firstOrNull { it.id == id }
        if (current != null) post(current)
        schedule()
    }

    private suspend fun read(nowMs: Long, days: UInt, limit: UInt): List<Reminder> {
        val settings = preferences.settings.value
        if (!settings.dueTime && !settings.dueToday && !settings.dailiesLeft) return emptyList()
        val zone = ZoneId.systemDefault().id
        return runCatching {
            container.repository().database.coreRead { it.reminders(settings, nowMs, zone, days, limit) }
        }.getOrDefault(emptyList())
    }

    private fun post(reminder: Reminder) {
        if (!canNotify) return
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, "Reminders", NotificationManager.IMPORTANCE_DEFAULT).apply {
                description = "Due times, what is due today, and dailies left"
            },
        )
        val open = Intent(context, MainActivity::class.java)
            .setAction("$ACTION_OPEN/${reminder.id}")
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .apply { reminder.taskId?.let { putExtra(ShellIntents.EXTRA_TASK_ID, it) } }
        val notification = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(reminder.title)
            .setContentText(reminder.body)
            .setStyle(NotificationCompat.BigTextStyle().bigText(reminder.body))
            .setContentIntent(
                PendingIntent.getActivity(
                    context, reminder.id.hashCode(), open,
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                ),
            )
            .setAutoCancel(true)
            .build()
        manager.notify(reminder.id, NOTIFICATION_ID, notification)
    }

    /** Distinct per reminder by its action, which `Intent.filterEquals` compares. */
    private fun alarmIntent(id: String, fireAtMs: Long): PendingIntent = PendingIntent.getBroadcast(
        context,
        id.hashCode(),
        Intent(context, ReminderReceiver::class.java)
            .setAction("$ACTION_FIRE/$id")
            .putExtra(EXTRA_ID, id)
            .putExtra(EXTRA_AT, fireAtMs),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    companion object {
        private const val CHANNEL = "reminders"
        private const val ACTION_FIRE = "uk.co.maybeitsadam.takt.REMINDER"
        private const val ACTION_OPEN = "uk.co.maybeitsadam.takt.OPEN_REMINDER"
        const val EXTRA_ID = "reminder"
        const val EXTRA_AT = "fireAt"
        private const val NOTIFICATION_ID = 7_301
        private const val WINDOW_MS = 10 * 60_000L
        /** A week ahead, at most 48: well inside AlarmManager's per-app limit of 500. */
        private const val DAYS = 7u
        private const val LIMIT = 48u
    }
}

/** A reminder's alarm going off. */
class ReminderReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val id = intent.getStringExtra(ReminderController.EXTRA_ID) ?: return
        val at = intent.getLongExtra(ReminderController.EXTRA_AT, System.currentTimeMillis())
        val container = context.appContainer
        val pending = goAsync()
        container.scope.launch {
            try {
                container.reminders.fire(id, at)
            } finally {
                pending.finish()
            }
        }
    }
}

/** Alarms don't survive a restart, and a changed clock or zone moves them: set them again. */
class ReminderRescheduleReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action !in RESCHEDULE_ON) return
        val container = context.appContainer
        val pending = goAsync()
        container.scope.launch {
            try {
                container.reminders.schedule()
            } finally {
                pending.finish()
            }
        }
    }

    private companion object {
        val RESCHEDULE_ON = setOf(
            Intent.ACTION_BOOT_COMPLETED,
            Intent.ACTION_MY_PACKAGE_REPLACED,
            Intent.ACTION_TIME_CHANGED,
            Intent.ACTION_TIMEZONE_CHANGED,
        )
    }
}
