package uk.co.maybeitsadam.takt.ui.review

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import uk.co.maybeitsadam.takt.core.FocusPoints
import uk.co.maybeitsadam.takt.core.FocusPointsSummary
import uk.co.maybeitsadam.takt.core.TaskProgressPeriod
import uk.co.maybeitsadam.takt.core.TaskStatus
import uk.co.maybeitsadam.takt.ui.commands.PaletteCommand
import uk.co.maybeitsadam.takt.ui.commands.RegisterCommands
import uk.co.maybeitsadam.takt.ui.components.EmptyState
import uk.co.maybeitsadam.takt.ui.components.Format
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.IconAction
import uk.co.maybeitsadam.takt.ui.components.MonoText
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.components.TaktTopBar
import uk.co.maybeitsadam.takt.ui.components.SectionHeader
import uk.co.maybeitsadam.takt.ui.components.TaskCheck
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics

/** Review: the day's focus on an hour ruler, the work that got done, and the trend. */
@Composable
fun ReviewScreen() {
    val shell = LocalShell.current
    val model = viewModel { ReviewViewModel(shell.container) }
    val section by model.section.collectAsStateWithLifecycle()

    RegisterCommands(
        remember(model) {
            ReviewSection.entries.map { s ->
                PaletteCommand("review.${s.name.lowercase()}", "Review: ${s.title}", "Review") { model.select(s) }
            }
        },
    )

    Column(Modifier.fillMaxSize().background(TaktTheme.colors.paper)) {
        TaktTopBar("Review", sidebar = true)
        Segmented(
            options = ReviewSection.entries,
            selected = section,
            label = { it.title },
            tag = { "review_section_${it.name.lowercase()}" },
            modifier = Modifier.padding(horizontal = Metrics.lg, vertical = Metrics.sm),
            onSelect = model::select,
        )
        Hairline()
        when (section) {
            ReviewSection.TIMELINE -> TimelinePane(model)
            ReviewSection.DONE -> DonePane(model)
            ReviewSection.PROGRESS -> ProgressPane(model)
        }
    }
}

/** The identity hues a task takes on the ruler and in the breakdown. Primary first; never danger. */
@Composable
private fun hue(index: Int): Color {
    val c = TaktTheme.colors
    val palette = listOf(c.primary, c.categoricalPurple, c.success, c.categoricalOrange, c.categoricalPink, c.warning)
    return palette[index % palette.size]
}

private val hourFormat = DateTimeFormatter.ofPattern("HH:mm", Locale.UK)
private val HOUR_HEIGHT: Dp = 56.dp

// region Timeline

@Composable
private fun TimelinePane(model: ReviewViewModel) {
    val day by model.day.collectAsStateWithLifecycle()
    val timeline by model.timeline.collectAsStateWithLifecycle()
    val today = LocalDate.now()
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(vertical = Metrics.md)
            .testTag("review_timeline"),
        verticalArrangement = Arrangement.spacedBy(Metrics.lg),
    ) {
        Row(Modifier.fillMaxWidth().padding(horizontal = Metrics.sm), verticalAlignment = Alignment.CenterVertically) {
            IconAction(Icons.AutoMirrored.Filled.KeyboardArrowLeft, "Previous day", tint = TaktTheme.colors.ink) { model.moveDay(-1) }
            Text(
                Format.day(day, today),
                style = TaktTheme.type.heading,
                color = TaktTheme.colors.ink,
                modifier = Modifier.testTag("review_day_title"),
            )
            IconAction(
                Icons.AutoMirrored.Filled.KeyboardArrowRight, "Next day",
                enabled = day < today, tint = TaktTheme.colors.ink,
            ) { model.moveDay(1) }
            Spacer(Modifier.weight(1f))
            if (day != today) PButton("Today", modifier = Modifier.padding(end = Metrics.sm)) { model.showDay(today) }
        }
        DayStrip(day, today, model::showDay)
        val current = timeline
        if (current == null || current.day != day) {
            Spacer(Modifier.height(HOUR_HEIGHT))
        } else {
            val scoring by model.scoresEachFocusBlock.collectAsStateWithLifecycle()
            StatRow(
                listOfNotNull(
                    "Focused" to if (current.totalSeconds > 0) Format.duration(current.totalSeconds) else "—",
                    "Blocks" to current.layout.placements.size.toString(),
                    if (scoring) "Points" to FocusPoints.formatted(current.points) else null,
                    "Finished" to current.completions.count { !it.cancelled }.toString(),
                ),
                Modifier.padding(horizontal = Metrics.lg),
            )
            Ruler(current, Modifier.padding(horizontal = Metrics.lg))
            if (current.summaries.isNotEmpty()) Breakdown(current)
            if (current.completions.isNotEmpty()) FinishedOnDay(current)
            if (current.summaries.isEmpty() && current.completions.isEmpty()) {
                Text(
                    "No focus logged on this day.",
                    style = TaktTheme.type.small,
                    color = TaktTheme.colors.mutedText,
                    modifier = Modifier.padding(horizontal = Metrics.lg),
                )
            }
        }
    }
}

@Composable
private fun DayStrip(selected: LocalDate, today: LocalDate, onSelect: (LocalDate) -> Unit) {
    val days = remember(today) { dayStrip(today).toImmutableList() }
    val state = rememberLazyListState(initialFirstVisibleItemIndex = maxOf(0, days.size - 1))
    LazyRow(
        state = state,
        contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = Metrics.lg),
        horizontalArrangement = Arrangement.spacedBy(Metrics.xs),
        modifier = Modifier.testTag("review_day_strip"),
    ) {
        items(days, key = { it.toEpochDay() }, contentType = { "day" }) { date ->
            val isSelected = date == selected
            val accent = TaktTheme.colors.primary
            Column(
                Modifier
                    .width(44.dp)
                    .height(56.dp)
                    .clip(Metrics.control)
                    .background(if (isSelected) accent.copy(alpha = 0.10f) else TaktTheme.colors.raised)
                    .border(
                        BorderStroke(Metrics.hairline, if (isSelected) accent.copy(alpha = 0.6f) else TaktTheme.colors.border),
                        Metrics.control,
                    )
                    .clickable(role = Role.Button, onClickLabel = "Show this day") { onSelect(date) }
                    .semantics { contentDescription = Format.day(date, today) },
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                Text(shortWeekday(date.dayOfWeek), style = TaktTheme.type.small, color = if (isSelected) TaktTheme.colors.ink else TaktTheme.colors.mutedText)
                Text(date.dayOfMonth.toString(), style = TaktTheme.type.mono, color = if (isSelected) accent else TaktTheme.colors.ink)
            }
        }
    }
}

@Composable
private fun Ruler(day: TimelineDay, modifier: Modifier = Modifier) {
    val layout = day.layout
    val zone = remember { ZoneId.systemDefault() }
    val height = HOUR_HEIGHT * layout.hourCount
    val gridColor = TaktTheme.colors.borderMuted
    Row(
        modifier
            .fillMaxWidth()
            .height(height + 8.dp)
            .testTag("review_ruler"),
    ) {
        Box(Modifier.width(44.dp).height(height + 8.dp)) {
            layout.hours.forEachIndexed { index, hour ->
                Text(
                    hourFormat.format(hour.atZone(zone)),
                    style = TaktTheme.type.monoSmall,
                    color = TaktTheme.colors.mutedText,
                    modifier = Modifier.offset(y = HOUR_HEIGHT * index - 2.dp),
                )
            }
        }
        BoxWithConstraints(Modifier.weight(1f).height(height + 8.dp).padding(top = Metrics.xs)) {
            val laneWidth = maxWidth / maxOf(1, layout.laneCount)
            for (index in 0..layout.hourCount) {
                Box(
                    Modifier
                        .offset(y = HOUR_HEIGHT * index)
                        .fillMaxWidth()
                        .height(Metrics.hairline)
                        .background(gridColor),
                )
            }
            layout.placements.forEach { placement ->
                val color = hue(day.hue(placement.id))
                val blockHeight = maxOf(6.dp, HOUR_HEIGHT * (placement.minutes / 60).toFloat())
                val description = "${placement.block.title}, ${Format.duration(placement.block.seconds)}" +
                    if (placement.block.isLive) ", running" else ""
                Row(
                    Modifier
                        .offset(x = laneWidth * placement.lane, y = HOUR_HEIGHT * (placement.offsetMinutes / 60).toFloat())
                        .width(laneWidth - 4.dp)
                        .height(blockHeight)
                        .clip(Metrics.control)
                        .background(color.copy(alpha = 0.14f))
                        .border(BorderStroke(Metrics.hairline, color.copy(alpha = 0.4f)), Metrics.control)
                        .clearAndSetSemantics { contentDescription = description },
                ) {
                    Box(Modifier.width(3.dp).height(blockHeight).background(color))
                    Column(Modifier.padding(horizontal = Metrics.xs + Metrics.xxs, vertical = Metrics.xxs)) {
                        if (blockHeight > 20.dp) {
                            Text(
                                placement.block.title, style = TaktTheme.type.small, color = TaktTheme.colors.ink,
                                maxLines = 1, overflow = TextOverflow.Ellipsis,
                            )
                        }
                        if (blockHeight > 38.dp) {
                            MonoText(
                                Format.duration(placement.block.seconds) + if (placement.block.isLive) " · live" else "",
                                style = TaktTheme.type.monoSmall,
                            )
                        }
                    }
                }
            }
            // Completions: an emerald dot at the right edge, at the minute the task was closed.
            day.completions.forEach { completion ->
                val minutes = day.offsetMinutes(completion.at) ?: return@forEach
                val color = if (completion.cancelled) TaktTheme.colors.danger else TaktTheme.colors.success
                val verb = if (completion.cancelled) "Cancelled" else "Finished"
                Box(
                    Modifier
                        .align(Alignment.TopEnd)
                        .offset(y = HOUR_HEIGHT * (minutes / 60).toFloat() - 5.dp)
                        .size(10.dp)
                        .clip(CircleShape)
                        .background(color)
                        .border(BorderStroke(Metrics.hairline, TaktTheme.colors.paper), CircleShape)
                        .clearAndSetSemantics {
                            contentDescription = "$verb ${completion.title} at ${Format.time(completion.at, zone)}"
                        },
                )
            }
        }
    }
}

@Composable
private fun Breakdown(day: TimelineDay) {
    Column {
        SectionHeader("By task")
        day.summaries.forEach { summary ->
            Row(
                Modifier.fillMaxWidth().heightIn(min = 40.dp).padding(horizontal = Metrics.lg),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
            ) {
                Box(Modifier.size(10.dp).clip(RoundedCornerShape(2.dp)).background(hue(summary.hue)))
                Text(
                    summary.title, style = TaktTheme.type.body, color = TaktTheme.colors.ink,
                    maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
                )
                MonoText("${summary.blocks}×", color = TaktTheme.colors.dimText)
                MonoText(Format.duration(summary.seconds), modifier = Modifier.width(56.dp))
            }
            Hairline(Modifier.padding(start = Metrics.lg), color = TaktTheme.colors.borderMuted)
        }
    }
}

@Composable
private fun FinishedOnDay(day: TimelineDay) {
    val zone = remember { ZoneId.systemDefault() }
    val navigator = LocalShell.current.navigator
    Column {
        SectionHeader("Finished", detail = day.completions.size.toString())
        day.completions.forEach { completion ->
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = Metrics.touchTarget)
                    .clickable(onClickLabel = "Reveal in list") { navigator.openList(completion.listId, completion.taskId) }
                    .padding(horizontal = Metrics.lg),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
            ) {
                Box(
                    Modifier.size(8.dp).clip(CircleShape)
                        .background(if (completion.cancelled) TaktTheme.colors.danger else TaktTheme.colors.success),
                )
                Text(
                    completion.title, style = TaktTheme.type.body, color = TaktTheme.colors.mutedText,
                    textDecoration = if (completion.cancelled) TextDecoration.LineThrough else null,
                    maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
                )
                MonoText(Format.time(completion.at, zone), color = TaktTheme.colors.dimText)
            }
            Hairline(Modifier.padding(start = Metrics.lg), color = TaktTheme.colors.borderMuted)
        }
    }
}

// endregion

// region Done

@Composable
private fun DonePane(model: ReviewViewModel) {
    val groups by model.done.collectAsStateWithLifecycle()
    val navigator = LocalShell.current.navigator
    val zone = remember { ZoneId.systemDefault() }
    val current = groups ?: return
    if (current.isEmpty()) {
        EmptyState(
            "Nothing finished yet",
            detail = "Completed tasks from the last five weeks show here.",
            modifier = Modifier.testTag("review_done"),
        )
        return
    }
    LazyColumn(Modifier.fillMaxSize().testTag("review_done")) {
        current.forEach { group ->
            item(key = "day-${group.day.epochSecond}", contentType = "header") {
                SectionHeader(group.title(zone), detail = group.items.size.toString(), modifier = Modifier.background(TaktTheme.colors.paper))
            }
            items(group.items, key = { it.task.id }, contentType = { "done" }) { item ->
                val task = item.task
                Column {
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .heightIn(min = 56.dp)
                            .combinedClickable(
                                onClickLabel = "Reveal in list",
                                onLongClickLabel = "Open details",
                                onLongClick = { model.inspect(task.id) },
                            ) { navigator.openList(task.listId, revealTaskId = task.id) }
                            .padding(end = Metrics.lg)
                            .testTag("review_done_row"),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        TaskCheck(task.status, label = task.title, onToggle = { model.reopen(task.id) })
                        Column(Modifier.weight(1f)) {
                            Text(
                                task.title,
                                style = TaktTheme.type.body,
                                color = TaktTheme.colors.mutedText,
                                textDecoration = if (task.status == TaskStatus.CANCELLED) TextDecoration.LineThrough else null,
                                maxLines = 2,
                                overflow = TextOverflow.Ellipsis,
                            )
                            if (item.listName.isNotEmpty()) {
                                Text(item.listName, style = TaktTheme.type.small, color = TaktTheme.colors.dimText, maxLines = 1)
                            }
                        }
                        task.completedAt?.let { MonoText(Format.time(it, zone), color = TaktTheme.colors.dimText) }
                    }
                    Hairline(Modifier.padding(start = Metrics.touchTarget), color = TaktTheme.colors.borderMuted)
                }
            }
        }
    }
}

// endregion

// region Progress

@Composable
private fun ProgressPane(model: ReviewViewModel) {
    val period by model.period.collectAsStateWithLifecycle()
    val progress by model.progress.collectAsStateWithLifecycle()
    val points by model.points.collectAsStateWithLifecycle()
    val success = TaktTheme.colors.success
    val primary = TaktTheme.colors.primary
    val days = remember(progress) { progress.days.map { it.day }.toImmutableList() }
    val bestFormat = remember { DateTimeFormatter.ofPattern("EEEE d MMMM", Locale.ENGLISH) }
    val zone = remember { ZoneId.systemDefault() }
    val span = "the last ${period.days} days"

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(Metrics.lg)
            .testTag("review_progress"),
        verticalArrangement = Arrangement.spacedBy(Metrics.lg),
    ) {
        Segmented(
            options = TaskProgressPeriod.entries,
            selected = period,
            label = { "${it.days} days" },
            tag = { "review_period_${it.days}" },
            onSelect = model::selectPeriod,
        )
        StatRow(
            listOf(
                "Done" to progress.totalCompleted.toString(),
                "Added" to progress.totalAdded.toString(),
                "Net" to signed(progress.net),
                "Focus" to Format.duration(progress.focusMinutes * 60),
            ),
        )
        ChartCard(
            title = "Finished per day",
            description = "Tasks finished per day over $span: ${progress.totalCompleted} in total" +
                (progress.bestDay?.let { ", most on ${bestFormat.format(it.day.atZone(zone))} with ${it.completed}" } ?: "") + ".",
            days = days,
            series = persistentListOf(ChartSeries("Finished", progress.days.map { it.completed.toDouble() }.toImmutableList(), success)),
            kind = ChartKind.BARS,
            testTag = "chart_finished",
        )
        ChartCard(
            title = "Minutes focused",
            description = "Minutes focused per day over $span: ${Format.duration(progress.focusMinutes * 60)} in total.",
            days = days,
            series = persistentListOf(ChartSeries("Minutes", progress.days.map { it.focusMinutes.toDouble() }.toImmutableList(), primary)),
            kind = ChartKind.BARS,
            testTag = "chart_focus",
        )
        ChartCard(
            title = "Added and finished",
            description = "Running totals over $span: ${progress.totalAdded} added, ${progress.totalCompleted} finished, " +
                "net ${signed(progress.net)}.",
            days = days,
            series = persistentListOf(
                ChartSeries("Added", progress.days.map { it.cumulativeAdded.toDouble() }.toImmutableList(), primary),
                ChartSeries("Finished", progress.days.map { it.cumulativeCompleted.toDouble() }.toImmutableList(), success),
            ),
            kind = ChartKind.LINES,
            testTag = "chart_net",
        )
        progress.bestDay?.let { best ->
            Text(
                "Best day: ${bestFormat.format(best.day.atZone(zone))}, ${best.completed} finished",
                style = TaktTheme.type.small,
                color = TaktTheme.colors.mutedText,
            )
        }
        val scoring by model.scoresEachFocusBlock.collectAsStateWithLifecycle()
        if (scoring) PointsSummary(points)
    }
}

@Composable
private fun PointsSummary(points: FocusPointsSummary) {
    Column(verticalArrangement = Arrangement.spacedBy(Metrics.sm)) {
        Text("Focus points", style = TaktTheme.type.label, color = TaktTheme.colors.mutedText)
        StatRow(
            listOf(
                "Today" to FocusPoints.formatted(points.today),
                "Last 7 days" to FocusPoints.formatted(points.last7Days),
                "All time" to FocusPoints.formatted(points.allTime),
                "Blocks today" to points.blocksToday.toString(),
            ),
            Modifier.testTag("review_points"),
        )
    }
}

private fun signed(value: Int): String = if (value > 0) "+$value" else value.toString()

// endregion
