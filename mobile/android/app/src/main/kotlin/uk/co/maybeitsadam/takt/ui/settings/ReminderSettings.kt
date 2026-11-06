package uk.co.maybeitsadam.takt.ui.settings

import android.Manifest
import android.app.TimePickerDialog
import android.content.Intent
import android.os.Build
import android.provider.Settings
import android.text.format.DateFormat
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import java.time.LocalTime
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import uniffi.takt_core.ReminderSettings
import uk.co.maybeitsadam.takt.reminders.ReminderController
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.inspector.SwitchRow
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme

/**
 * Settings' Reminders section (docs/reminders.md): three switches, each with
 * its time. Switching the first one on asks for notification permission on
 * Android 13 and later.
 */
@Composable
internal fun ReminderSection(reminders: ReminderController) {
    val context = LocalContext.current
    val settings by reminders.preferences.settings.collectAsStateWithLifecycle()
    var canNotify by remember { mutableStateOf(reminders.canNotify) }
    var waiting by remember { mutableStateOf<((ReminderSettings) -> ReminderSettings)?>(null) }

    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        canNotify = reminders.canNotify
        waiting?.let { if (granted) reminders.preferences.update(it) }
        waiting = null
    }

    fun turnOn(change: (ReminderSettings) -> ReminderSettings) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU && !reminders.canNotify) {
            waiting = change
            ask.launch(Manifest.permission.POST_NOTIFICATIONS)
        } else {
            reminders.preferences.update(change)
        }
    }

    fun pick(minute: Int, set: (Int) -> Unit) {
        TimePickerDialog(
            context,
            { _, hour, min -> set(hour * 60 + min) },
            minute / 60, minute % 60, DateFormat.is24HourFormat(context),
        ).show()
    }

    Section("Reminders", footer = "Set on this phone only. A due day without a time is in the morning summary.") {
        val anyOn = settings.dueTime || settings.dueToday || settings.dailiesLeft
        if (anyOn && !canNotify) {
            Text(
                "Notifications are off for Takt. Tap to turn them on in Settings.",
                style = TaktTheme.type.small,
                color = TaktTheme.colors.danger,
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = Metrics.touchTarget)
                    .clickable {
                        context.startActivity(
                            Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                                .putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName),
                        )
                    }
                    .padding(Metrics.md)
                    .testTag("settings_reminders_denied"),
            )
            Hairline()
        }
        SwitchRow(
            "At a task's due time",
            checked = settings.dueTime,
            onCheckedChange = { on -> if (on) turnOn { it.copy(dueTime = true) } else reminders.preferences.update { it.copy(dueTime = false) } },
            detail = "A task due at a time goes off then.",
            modifier = Modifier.padding(horizontal = Metrics.md).testTag("settings_reminders_due_time"),
        )
        Hairline(color = TaktTheme.colors.borderMuted)
        SwitchRow(
            "Due today",
            checked = settings.dueToday,
            onCheckedChange = { on -> if (on) turnOn { it.copy(dueToday = true) } else reminders.preferences.update { it.copy(dueToday = false) } },
            detail = "Each morning, what is due that day. None on a day with nothing due.",
            modifier = Modifier.padding(horizontal = Metrics.md).testTag("settings_reminders_due_today"),
        )
        if (settings.dueToday) {
            TimeRow("Each morning at", settings.dueTodayMinute, "settings_reminders_due_today_time") {
                pick(settings.dueTodayMinute) { m -> reminders.preferences.update { it.copy(dueTodayMinute = m) } }
            }
        }
        Hairline(color = TaktTheme.colors.borderMuted)
        SwitchRow(
            "Dailies left",
            checked = settings.dailiesLeft,
            onCheckedChange = { on -> if (on) turnOn { it.copy(dailiesLeft = true) } else reminders.preferences.update { it.copy(dailiesLeft = false) } },
            detail = "Each evening, the dailies not yet done. None once they all are.",
            modifier = Modifier.padding(horizontal = Metrics.md).testTag("settings_reminders_dailies_left"),
        )
        if (settings.dailiesLeft) {
            TimeRow("Each evening at", settings.dailiesLeftMinute, "settings_reminders_dailies_left_time") {
                pick(settings.dailiesLeftMinute) { m -> reminders.preferences.update { it.copy(dailiesLeftMinute = m) } }
            }
        }
    }
}

@Composable
private fun TimeRow(title: String, minute: Int, tag: String, onClick: () -> Unit) {
    val time = LocalTime.of(minute / 60, minute % 60).format(DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT))
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = Metrics.touchTarget)
            .clickable(onClick = onClick)
            .padding(horizontal = Metrics.md)
            .testTag(tag),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(title, style = TaktTheme.type.body, color = TaktTheme.colors.ink, modifier = Modifier.weight(1f))
        Text(time, style = TaktTheme.type.small, color = TaktTheme.colors.primary)
    }
}
