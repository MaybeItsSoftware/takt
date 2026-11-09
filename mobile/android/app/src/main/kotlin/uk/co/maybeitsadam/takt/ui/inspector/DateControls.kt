package uk.co.maybeitsadam.takt.ui.inspector

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowLeft
import androidx.compose.material.icons.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import java.time.LocalDate
import java.time.LocalTime
import java.time.YearMonth
import java.time.format.DateTimeFormatter
import java.util.Locale
import uk.co.maybeitsadam.takt.ui.components.Format
import uk.co.maybeitsadam.takt.ui.components.Tag
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.PIcons

private val monthTitle = DateTimeFormatter.ofPattern("MMMM yyyy", Locale.UK)
private val spokenDay = DateTimeFormatter.ofPattern("EEEE d MMMM yyyy", Locale.UK)

/** `09:05`. */
fun hhmm(time: LocalTime): String = "%02d:%02d".format(time.hour, time.minute)

private fun YearMonth.index(): Int = year * 12 + monthValue - 1
private fun monthAt(index: Int): YearMonth = YearMonth.of(Math.floorDiv(index, 12), Math.floorMod(index, 12) + 1)

/**
 * The themed date (and optionally time) field: a field-shaped button showing
 * the value in the mono face, which opens an inline panel beneath it with a month grid,
 * an hour:minute stepper and Clear. Never the stock Material dialog.
 *
 * [time] non-null shows the time stepper; [onDate] with null clears.
 */
@Composable
fun ChalkDateField(
    label: String,
    date: LocalDate?,
    time: LocalTime?,
    today: LocalDate,
    onDate: (LocalDate?) -> Unit,
    onTime: (LocalTime) -> Unit,
    modifier: Modifier = Modifier,
    testTag: String? = null,
    placeholder: String = "No date",
    clearable: Boolean = true,
) {
    var expanded by rememberSaveable { mutableStateOf(false) }
    val colors = TaktTheme.colors
    val description = buildString {
        append(label).append(": ")
        append(date?.let { spokenDay.format(it) } ?: placeholder)
        if (date != null && time != null) append(" at ").append(hhmm(time))
    }
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(Metrics.sm)) {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = Metrics.touchTarget)
                .clip(Metrics.control)
                .background(colors.raised)
                .border(BorderStroke(Metrics.hairline, if (expanded) colors.primary else colors.inputBorder), Metrics.control)
                .clickable(role = Role.Button, onClickLabel = if (expanded) "Close calendar" else "Open calendar") { expanded = !expanded }
                .semantics { contentDescription = description }
                .then(if (testTag != null) Modifier.testTag(testTag) else Modifier)
                .padding(horizontal = Metrics.md),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
        ) {
            Icon(PIcons.Calendar, null, tint = if (date != null) colors.primary else colors.mutedText, modifier = Modifier.size(18.dp))
            Text(
                date?.let { Format.day(it, today) } ?: placeholder,
                style = TaktTheme.type.body,
                color = if (date != null) colors.ink else colors.dimText,
                modifier = Modifier.weight(1f),
                maxLines = 1,
            )
            if (date != null && time != null) {
                Text(hhmm(time), style = TaktTheme.type.mono.copy(fontSize = TaktTheme.type.body.fontSize), color = colors.ink)
            }
            Icon(
                if (expanded) Icons.Filled.KeyboardArrowUp else Icons.Filled.KeyboardArrowDown, null,
                tint = colors.mutedText, modifier = Modifier.size(18.dp),
            )
        }
        if (expanded) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .clip(Metrics.card)
                    .background(colors.raised)
                    .border(BorderStroke(Metrics.hairline, colors.border), Metrics.card)
                    .padding(Metrics.sm),
                verticalArrangement = Arrangement.spacedBy(Metrics.sm),
            ) {
                MonthGrid(selected = date, today = today, onPick = onDate)
                if (time != null && date != null) {
                    Box(Modifier.fillMaxWidth().heightIn(min = Metrics.hairline).background(colors.border))
                    TimeStepper(time = time, onChange = onTime, label = label)
                }
                if (clearable && date != null) {
                    Tag(
                        "Clear",
                        color = colors.danger,
                        onClick = { onDate(null) },
                        modifier = Modifier.semantics { contentDescription = "Clear ${label.lowercase()}" },
                    )
                }
            }
        }
    }
}

/** A month of days, Monday first, with arrows to the neighbouring months. */
@Composable
fun MonthGrid(selected: LocalDate?, today: LocalDate, onPick: (LocalDate) -> Unit, modifier: Modifier = Modifier) {
    val colors = TaktTheme.colors
    var monthIndex by rememberSaveable { mutableIntStateOf(YearMonth.from(selected ?: today).index()) }
    val month = monthAt(monthIndex)
    val grid = remember(monthIndex) { DateFieldMath.monthGrid(month) }
    val headers = remember { DateFieldMath.weekdayHeaders() }
    Column(modifier.fillMaxWidth()) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            StepIcon(Icons.Filled.KeyboardArrowLeft, "Previous month") { monthIndex -= 1 }
            Text(
                monthTitle.format(month),
                style = TaktTheme.type.bodyStrong,
                color = colors.ink,
                textAlign = TextAlign.Center,
                modifier = Modifier.weight(1f),
            )
            StepIcon(Icons.Filled.KeyboardArrowRight, "Next month") { monthIndex += 1 }
        }
        Row(Modifier.fillMaxWidth()) {
            for (header in headers) {
                Text(
                    header, style = TaktTheme.type.small, color = colors.mutedText, textAlign = TextAlign.Center,
                    modifier = Modifier.weight(1f).padding(vertical = Metrics.xs),
                )
            }
        }
        for (week in grid) {
            Row(Modifier.fillMaxWidth()) {
                for (day in week) {
                    Box(Modifier.weight(1f).aspectRatio(1f).heightIn(min = 40.dp), contentAlignment = Alignment.Center) {
                        if (day != null) DayCell(day, isSelected = day == selected, isToday = day == today) { onPick(day) }
                    }
                }
            }
        }
    }
}

@Composable
private fun DayCell(day: LocalDate, isSelected: Boolean, isToday: Boolean, onClick: () -> Unit) {
    val colors = TaktTheme.colors
    Box(
        Modifier
            .padding(Metrics.xxs)
            .fillMaxWidth()
            .aspectRatio(1f)
            .clip(Metrics.control)
            .background(if (isSelected) colors.primary else colors.raised)
            .then(if (isToday && !isSelected) Modifier.border(BorderStroke(Metrics.hairline, colors.primary), Metrics.control) else Modifier)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics {
                contentDescription = spokenDay.format(day) + if (isToday) ", today" else ""
                selected = isSelected
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(
            day.dayOfMonth.toString(),
            style = TaktTheme.type.mono.copy(fontSize = TaktTheme.type.body.fontSize),
            color = when {
                isSelected -> colors.onAccent
                isToday -> colors.primary
                else -> colors.ink
            },
        )
    }
}

/** Hour and minute, each with its own minus and plus; minutes move in fives. */
@Composable
fun TimeStepper(time: LocalTime, onChange: (LocalTime) -> Unit, label: String, modifier: Modifier = Modifier) {
    val colors = TaktTheme.colors
    Row(
        modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Metrics.xs, Alignment.CenterHorizontally),
    ) {
        StepButton(InspectorIcons.Minus, "$label hour earlier") { onChange(DateFieldMath.stepHour(time, -1)) }
        ClockDigits("%02d".format(time.hour))
        StepButton(Icons.Filled.Add, "$label hour later") { onChange(DateFieldMath.stepHour(time, 1)) }
        Text(":", style = TaktTheme.type.mono.copy(fontSize = TaktTheme.type.title.fontSize), color = colors.mutedText)
        StepButton(InspectorIcons.Minus, "$label minutes earlier") { onChange(DateFieldMath.stepMinute(time, -1)) }
        ClockDigits("%02d".format(time.minute))
        StepButton(Icons.Filled.Add, "$label minutes later") { onChange(DateFieldMath.stepMinute(time, 1)) }
    }
}

@Composable
private fun ClockDigits(text: String) {
    Text(
        text,
        style = TaktTheme.type.mono.copy(fontSize = TaktTheme.type.title.fontSize),
        color = TaktTheme.colors.ink,
        textAlign = TextAlign.Center,
        modifier = Modifier.width(36.dp),
    )
}

@Composable
private fun StepIcon(icon: ImageVector, description: String, onClick: () -> Unit) {
    Box(
        Modifier
            .size(Metrics.touchTarget)
            .clip(Metrics.control)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, null, tint = TaktTheme.colors.mutedText, modifier = Modifier.size(20.dp))
    }
}
