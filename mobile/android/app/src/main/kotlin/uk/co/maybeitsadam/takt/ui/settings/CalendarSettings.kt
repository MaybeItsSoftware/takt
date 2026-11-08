package uk.co.maybeitsadam.takt.ui.settings

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.IntentSenderRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.toggleableState
import androidx.compose.ui.state.ToggleableState
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.settings.GoogleCalendarController
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.inspector.SwitchRow
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.parseHexColor

/**
 * Google Calendar (docs/google-calendar.md): connect this phone, choose the
 * calendar timed tasks are kept in step with, whether events made there become
 * tasks, and which calendars Today shows. The choices sync; the grant doesn't.
 */
@Composable
internal fun CalendarSection(calendar: GoogleCalendarController) {
    // The integrations under one heading, as the Mac's Settings → Plugins.
    // Google Calendar is the one this phone has.
    Column(verticalArrangement = Arrangement.spacedBy(Metrics.md)) {
        Text(
            "Plugins", style = TaktTheme.type.heading, color = TaktTheme.colors.ink,
            modifier = Modifier.testTag("settings_plugins"),
        )
        GoogleCalendarPlugin(calendar)
    }
}

@Composable
private fun GoogleCalendarPlugin(calendar: GoogleCalendarController) {
    val scope = rememberCoroutineScope()
    val connected by calendar.connected.collectAsStateWithLifecycle()
    val needsConsent by calendar.needsConsent.collectAsStateWithLifecycle()
    val error by calendar.error.collectAsStateWithLifecycle()
    val consent = rememberLauncherForActivityResult(ActivityResultContracts.StartIntentSenderForResult()) { result ->
        scope.launch { calendar.finishConsent(result.data) }
    }
    val connect = {
        scope.launch {
            calendar.connect()?.let { consent.launch(IntentSenderRequest.Builder(it).build()) }
        }
        Unit
    }

    if (!connected) {
        Section(
            "Google Calendar",
            footer = "Timed tasks go on a calendar you choose and stay in step with it, and Today shows the day's events.",
        ) {
            Row(Modifier.padding(Metrics.md)) {
                PButton("Connect Google Calendar", modifier = Modifier.testTag("calendar_connect"), onClick = connect)
            }
            error?.let { CalendarError(it) }
        }
        return
    }

    val settings by calendar.settings.collectAsStateWithLifecycle()
    val calendars by calendar.calendars.collectAsStateWithLifecycle()
    LaunchedEffect(Unit) { calendar.loadCalendars() }
    val current = settings
    val writable = calendars.filter { it.canWrite }

    CollapsibleSection(
        "Google Calendar",
        key = "calendar",
        summary = calendars.firstOrNull { it.id == current?.calendarId }?.summary ?: "Connected",
        footer = "Only one device writes to the calendar at a time. Make a calendar called Takt in Google Calendar to keep tasks apart.",
    ) {
        Text(
            "Keep timed tasks in", style = TaktTheme.type.label, color = TaktTheme.colors.mutedText,
            modifier = Modifier.padding(start = Metrics.md, end = Metrics.md, top = Metrics.md),
        )
        ChoiceRow("Off", selected = current?.calendarId == null, tag = "calendar_two_way_off") {
            scope.launch { calendar.update { it.copy(calendarId = null, importEvents = false) } }
        }
        writable.forEach { info ->
            Hairline(color = TaktTheme.colors.borderMuted)
            ChoiceRow(info.summary, selected = current?.calendarId == info.id) {
                scope.launch { calendar.update { it.copy(calendarId = info.id) } }
            }
        }
        Hairline()
        SwitchRow(
            "Turn new events into tasks",
            checked = current?.importEvents == true,
            enabled = current?.calendarId != null,
            onCheckedChange = { on -> scope.launch { calendar.update { it.copy(importEvents = on) } } },
            detail = "An event you make in that calendar becomes a task in the Inbox.",
            modifier = Modifier.padding(horizontal = Metrics.md).testTag("calendar_import"),
        )
        error?.let {
            Hairline()
            CalendarError(it)
        }
        Hairline()
        Row(Modifier.padding(Metrics.md), horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            if (needsConsent) PButton("Connect again", primary = true, onClick = connect)
            PButton("Disconnect", modifier = Modifier.testTag("calendar_disconnect")) { scope.launch { calendar.disconnect() } }
        }
    }

    if (calendars.isNotEmpty()) {
        val shown = calendars.count { current?.agendaCalendarIds.orEmpty().contains(it.id) }
        CollapsibleSection("Shown in Today", key = "calendar_agenda", summary = if (shown == 0) "None" else "$shown of ${calendars.size}") {
            calendars.forEachIndexed { index, info ->
                if (index > 0) Hairline(color = TaktTheme.colors.borderMuted)
                val shown = current?.agendaCalendarIds.orEmpty().contains(info.id)
                CalendarCheckRow(info.summary, parseHexColor(info.color) ?: TaktTheme.colors.primary, shown) {
                    scope.launch {
                        calendar.update {
                            it.copy(agendaCalendarIds = if (shown) it.agendaCalendarIds - info.id else it.agendaCalendarIds + info.id)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun CalendarError(message: String) {
    Text(
        message, style = TaktTheme.type.small, color = TaktTheme.colors.danger,
        modifier = Modifier.fillMaxWidth().padding(horizontal = Metrics.md, vertical = Metrics.sm).testTag("calendar_error"),
    )
}

/** A calendar's dot, its name, and a square check: ticked when Today shows it. */
@Composable
private fun CalendarCheckRow(title: String, color: androidx.compose.ui.graphics.Color, checked: Boolean, onToggle: () -> Unit) {
    val colors = TaktTheme.colors
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = Metrics.touchTarget)
            .clickable(role = Role.Checkbox, onClick = onToggle)
            .semantics { toggleableState = ToggleableState(checked) }
            .padding(horizontal = Metrics.md, vertical = Metrics.sm),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
    ) {
        Box(Modifier.size(DOT).clip(CircleShape).background(color))
        Text(title, style = TaktTheme.type.body, color = colors.ink, modifier = Modifier.weight(1f))
        Box(
            Modifier
                .size(BOX)
                .clip(Metrics.control)
                .background(if (checked) colors.primary else colors.raised)
                .border(BorderStroke(BOX_STROKE, if (checked) colors.primary else colors.inputBorder), Metrics.control),
            contentAlignment = Alignment.Center,
        ) {
            if (checked) Icon(Icons.Filled.Check, null, tint = colors.onAccent, modifier = Modifier.size(BOX * 0.75f))
        }
    }
}

private val DOT = 10.dp
private val BOX = 20.dp
private val BOX_STROKE = 1.5.dp
