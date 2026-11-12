package uk.co.maybeitsadam.takt.ui.today

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import java.time.LocalDate
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.core.DayPlanReason
import uk.co.maybeitsadam.takt.core.FocusSession
import uk.co.maybeitsadam.takt.ui.commands.KeyCommand
import uk.co.maybeitsadam.takt.ui.commands.RegisterCommands
import uk.co.maybeitsadam.takt.ui.conditions.NowConditionsBar
import uk.co.maybeitsadam.takt.ui.components.EmptyState
import uk.co.maybeitsadam.takt.ui.components.Format
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.Disclosure
import uk.co.maybeitsadam.takt.ui.components.RowPlace
import uk.co.maybeitsadam.takt.ui.components.rowFill
import uk.co.maybeitsadam.takt.ui.components.rowFrame
import uk.co.maybeitsadam.takt.ui.components.sectionHeaderFrame
import uk.co.maybeitsadam.takt.ui.components.IconAction
import uk.co.maybeitsadam.takt.ui.components.MonoText
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.components.TaktTopBar
import uk.co.maybeitsadam.takt.ui.components.SectionHeader
import uk.co.maybeitsadam.takt.ui.components.SwipeAction
import uk.co.maybeitsadam.takt.ui.components.SwipeRow
import uk.co.maybeitsadam.takt.ui.focus.BlockIconButton
import uk.co.maybeitsadam.takt.ui.focus.ClockReading
import uk.co.maybeitsadam.takt.ui.focus.QualityPrompt
import uk.co.maybeitsadam.takt.ui.focus.isTicking
import uk.co.maybeitsadam.takt.ui.focus.rememberNotificationGate
import uk.co.maybeitsadam.takt.ui.focus.rememberTicker
import uk.co.maybeitsadam.takt.ui.lists.ConfirmDialog
import uk.co.maybeitsadam.takt.ui.lists.MoveToListSheet
import uk.co.maybeitsadam.takt.ui.lists.WaitingMarkers
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.navigation.Tab
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.PIcons
import uk.co.maybeitsadam.takt.ui.theme.parseHexColor

/**
 * The day: the running block first, then what was planned (in hand order,
 * draggable), overdue, due today and starting today; with nothing planned,
 * the top of the ranking. A forecast says what the day still owes and when
 * it would be done. The Mac's `DayView`, for a phone.
 */
@Composable
fun TodayScreen() {
    val shell = LocalShell.current
    val container = shell.container
    val vm = viewModel { TodayViewModel(container) }
    val day by vm.state.collectAsStateWithLifecycle()
    val pending by vm.pendingCompletion.collectAsStateWithLifecycle()
    val gate = rememberNotificationGate()
    val listState = rememberLazyListState()
    val drag = rememberPlannedDrag(listState)
    val scope = rememberCoroutineScope()
    // Rows just finished, held where they stood and crossed out for a moment
    // before they go, so a swipe reads as done rather than as vanished.
    val leavingCards = remember { mutableStateMapOf<String, Leaving<DayCard>>() }
    val leavingDailies = remember { mutableStateMapOf<String, Leaving<DayDaily>>() }
    // The card whose "Move to list…" or "Delete…" is asking.
    var moving by remember { mutableStateOf<DayCard?>(null) }
    var deleting by remember { mutableStateOf<DayCard?>(null) }
    // A row on its way out has been finished already: a second tap or swipe
    // on it would log it again.
    fun finish(card: DayCard, kind: DaySectionKind, index: Int) {
        if (card.id in leavingCards) return
        if (!card.isRunning && !card.isDailyDoneToday) {
            leavingCards[card.id] = Leaving(card, kind, index)
            scope.launch { delay(LEAVE_MILLIS); leavingCards.remove(card.id) }
        }
        vm.tickOff(card)
    }
    fun finish(daily: DayDaily, index: Int) {
        if (daily.id in leavingDailies) return
        if (!daily.isDone) {
            leavingDailies[daily.id] = Leaving(daily, null, index)
            scope.launch { delay(LEAVE_MILLIS); leavingDailies.remove(daily.id) }
        }
        vm.toggleDaily(daily)
    }

    RegisterCommands(
        listOf(
            KeyCommand.GO_FOCUS.palette { shell.navigator.openTab(Tab.FOCUS) },
        ),
    )

    Column(Modifier.fillMaxSize().background(TaktTheme.colors.paper)) {
        TaktTopBar("Today", subtitle = longDay(LocalDate.now()), sidebar = true)
        // A leaving card the day now shows in another section stays there:
        // put back in its old one too, it would be the same key twice.
        val onTheDay = day.cards.mapTo(HashSet()) { it.id }
        val sections = DaySectionKind.entries.mapNotNull { kind ->
            val cards = day.sections.firstOrNull { it.kind == kind }?.cards.orEmpty()
            withLeaving(cards, leavingCards.values.filter { it.kind == kind }, DayCard::id, onTheDay)
                .takeIf { it.isNotEmpty() }?.let { kind to it }
        }
        val dailies = withLeaving(day.dailies, leavingDailies.values, DayDaily::id)
        val plannedIds = day.plannedIds
        LazyColumn(
            state = listState,
            modifier = Modifier.fillMaxSize().testTag("today_list"),
            contentPadding = PaddingValues(bottom = 96.dp),
        ) {
            item(key = "forecast", contentType = "forecast") { DayHeader(day) }
            item(key = "conditions", contentType = "conditions") { NowConditionsBar() }
            item(key = "calendar", contentType = "calendar") { CalendarAgenda(container.calendar, onOpenTask = container.inspector::open) }
            for ((kind, sectionCards) in sections) {
                item(key = "section-${kind.name}", contentType = "section") {
                    SectionHeader(kind.title, Modifier.sectionHeaderFrame(), detail = sectionCards.count { it.id !in leavingCards }.toString())
                }
                val cards = if (kind == DaySectionKind.PLANNED && drag.order != null) {
                    val byId = sectionCards.associateBy { it.id }
                    drag.order.orEmpty().mapNotNull(byId::get)
                } else {
                    sectionCards
                }
                items(cards, key = { it.id }, contentType = { if (it.isRunning) "running" else "card" }) { card ->
                    val index = sectionCards.indexOf(card)
                    val isDragging = drag.draggingId == card.id
                    val itemModifier = if (isDragging) {
                        Modifier.zIndex(1f).graphicsLayer { translationY = drag.offset }
                    } else {
                        Modifier.animateItem()
                    }.rowFrame(RowPlace.of(cards.indexOf(card), cards.size), selected = card.isRunning)
                    if (card.isRunning && day.session != null) {
                        RunningDayCard(
                            card = card,
                            session = day.session!!,
                            onPause = vm::togglePause,
                            onLog = { vm.requestCompletion(completeTask = false) },
                            onDone = { vm.requestCompletion(completeTask = true) },
                            onStop = vm::endSession,
                            onOpen = { vm.inspect(card) },
                            modifier = itemModifier,
                        )
                    } else {
                        val isLeaving = card.id in leavingCards
                        SwipeRow(
                            start = if (isLeaving) null else SwipeAction(
                                if (card.dailyId != null) "Log today" else "Done",
                                Icons.Filled.Check,
                                TaktTheme.colors.success,
                            ) { finish(card, kind, index) },
                            end = if (isLeaving) null else SwipeAction("Tomorrow", PIcons.Calendar, TaktTheme.colors.warning) { vm.deferToTomorrow(card) },
                            modifier = itemModifier,
                        ) {
                            DayCardRow(
                                card = card,
                                isDragging = isDragging,
                                isLeaving = isLeaving,
                                canStart = true,
                                onTick = { finish(card, kind, index) },
                                onStart = { gate { vm.start(card) } },
                                menu = { dismiss ->
                                    CardMenu(
                                        card, vm, dismiss,
                                        onComplete = { finish(card, kind, index) },
                                        onStart = { gate { vm.start(card) } },
                                        onHabit = { container.habits.open(card.id) },
                                        onMove = { moving = card },
                                        onDelete = { deleting = card },
                                        onGoToList = { listId -> shell.navigator.openList(listId, card.id) },
                                    )
                                },
                                handle = if (card.isPlanned && plannedIds.size > 1) {
                                    Modifier.plannedDragHandle(drag, card.id, plannedIds, vm::arrange)
                                } else {
                                    null
                                },
                            )
                        }
                    }
                }
            }
            if (dailies.isNotEmpty()) {
                item(key = "section-dailies", contentType = "section") {
                    SectionHeader("Dailies", Modifier.sectionHeaderFrame(), detail = dailies.count { it.id !in leavingDailies }.toString())
                }
                items(dailies, key = { "daily-${it.id}" }, contentType = { "daily" }) { daily ->
                    val index = dailies.indexOf(daily)
                    val isLeaving = daily.id in leavingDailies
                    SwipeRow(
                        start = if (isLeaving) null else SwipeAction("Log today", Icons.Filled.Check, TaktTheme.colors.success) { finish(daily, index) },
                        end = null,
                        modifier = Modifier.animateItem().rowFrame(RowPlace.of(index, dailies.size)),
                    ) {
                        DailyRow(
                            daily,
                            isLeaving = isLeaving,
                            onToggle = { finish(daily, index) },
                            onOpen = { container.inspector.open(daily.taskId) },
                        )
                    }
                }
            }
            if (day.isLoaded && sections.isEmpty() && dailies.isEmpty()) {
                item(key = "empty", contentType = "empty") {
                    EmptyState(
                        "Nothing planned for today",
                        detail = "Add one with +, or swipe a task left to put it off.",
                    )
                }
            }
        }
    }

    pending?.let { completion ->
        QualityPrompt(completion, onScore = vm::confirmCompletion, onCancel = vm::cancelCompletion)
    }
    moving?.let { card ->
        val lists by vm.lists.collectAsStateWithLifecycle()
        MoveToListSheet(
            card.title, card.listId, lists, onDismiss = { moving = null },
            onMove = { vm.moveToList(card, it) },
            onMoveToNew = { vm.moveToNewList(card, it) },
        )
    }
    deleting?.let { card ->
        ConfirmDialog(
            "Delete ${card.title}?", "This deletes the task and everything beneath it. You can undo it.",
            onDismiss = { deleting = null },
        ) { vm.delete(card) }
    }
}

private fun longDay(date: LocalDate): String =
    date.format(java.time.format.DateTimeFormatter.ofPattern("EEEE d MMMM", java.util.Locale.UK))

/** How long a finished row stays, crossed out, before it leaves the day. */
private const val LEAVE_MILLIS = 900L

/** A row just finished, and where it stood: its section, and its place in it. */
internal data class Leaving<T>(val item: T, val kind: DaySectionKind?, val index: Int)

/**
 * [items] with the rows still leaving put back where they stood, once the day
 * has dropped them; not one [elsewhere] still shows, in another section.
 */
internal fun <T> withLeaving(
    items: List<T>,
    leaving: Collection<Leaving<T>>,
    id: (T) -> String,
    elsewhere: Set<String> = emptySet(),
): List<T> {
    if (leaving.isEmpty()) return items
    val present = items.mapTo(HashSet(), id) + elsewhere
    val rows = items.toMutableList()
    for (row in leaving.sortedBy { it.index }) {
        if (id(row.item) !in present) rows.add(minOf(row.index, rows.size), row.item)
    }
    return rows
}

// region Header

/** What the day costs against what it has cost, and when it ends if worked straight through. */
@Composable
private fun DayHeader(day: DayState) {
    val ticking = day.session?.isTicking == true
    val now by rememberTicker(active = true, periodMillis = if (ticking) 30_000 else 60_000)
    val forecast = remember(day.cards, day.session, now) { DayShaping.forecast(day.cards, day.session, now) }
    // A drop-down: the finish time and the tally on one line, the bar and
    // the counts under it when it is opened.
    Disclosure(key = "today.forecast", tag = "today_forecast_toggle", header = {
        val finish = forecast.finishAt
        Text(
            if (finish != null) "done by ${Format.time(finish)}" else if (forecast.estimatedSeconds > 0) "All estimates spent" else "No finish time",
            style = TaktTheme.type.mono.copy(fontSize = TaktTheme.type.heading.fontSize),
            color = if (finish != null) TaktTheme.colors.primary else TaktTheme.colors.mutedText,
            maxLines = 1,
            modifier = Modifier.weight(1f).testTag("today_forecast"),
        )
        MonoText(forecast.spentText, color = TaktTheme.colors.ink, modifier = Modifier.testTag("today_tally"))
    }) {
        Column(
            Modifier.fillMaxWidth().padding(start = Metrics.lg, end = Metrics.lg, bottom = Metrics.md),
            verticalArrangement = Arrangement.spacedBy(Metrics.sm),
        ) {
            val fraction = if (forecast.estimatedSeconds > 0) minOf(1f, forecast.loggedSeconds.toFloat() / forecast.estimatedSeconds) else 0f
            Box(Modifier.fillMaxWidth().height(4.dp).background(TaktTheme.colors.well)) {
                Box(Modifier.fillMaxWidth(fraction).height(4.dp).background(TaktTheme.colors.primary))
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                MonoText(
                    "est ${DayForecast.hoursAndMinutes(forecast.estimatedSeconds)} · logged ${DayForecast.hoursAndMinutes(forecast.loggedSeconds)}",
                    style = TaktTheme.type.monoSmall,
                    modifier = Modifier.weight(1f),
                )
                Text(forecast.remainingText, style = TaktTheme.type.small, color = TaktTheme.colors.mutedText, maxLines = 1)
            }
            val progress = day.workProgress
            if (progress.week.seconds > 0 || progress.week.completed > 0) {
                Row {
                    Text(
                        if (progress.today.completed == 1) "1 done today" else "${progress.today.completed} done today",
                        style = TaktTheme.type.small, color = TaktTheme.colors.mutedText, modifier = Modifier.weight(1f),
                    )
                    MonoText(
                        "${Format.duration(progress.today.seconds)} today · ${Format.duration(progress.week.seconds)} this week",
                        style = TaktTheme.type.monoSmall,
                    )
                }
            }
        }
    }
    Hairline()
}

// endregion

// region Cards

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun DayCardRow(
    card: DayCard,
    isDragging: Boolean,
    isLeaving: Boolean,
    canStart: Boolean,
    onTick: () -> Unit,
    onStart: () -> Unit,
    menu: @Composable (dismiss: () -> Unit) -> Unit,
    handle: Modifier?,
    modifier: Modifier = Modifier,
) {
    var menuOpen by remember { mutableStateOf(false) }
    Column(modifier.background(if (isDragging) TaktTheme.colors.raised else rowFill)) {
        // No check circle: tapping the row finishes it, crossed out and then
        // gone; play starts a block on it; the options sit just before play
        // and the drag handle on the leading edge.
        val verb = if (card.dailyId != null) "Log today" else "Complete"
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = Metrics.rowHeight)
                .padding(start = if (handle == null) Metrics.md - Metrics.xs else 0.dp, end = Metrics.xs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (handle != null) {
                Box(
                    handle.size(Metrics.touchTarget).semantics { contentDescription = "Reorder ${card.title}" },
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(PIcons.DragHandle, null, tint = TaktTheme.colors.dimText, modifier = Modifier.size(16.dp))
                }
            }
            Column(
                Modifier
                    .weight(1f)
                    .clip(Metrics.row)
                    .combinedClickable(
                        enabled = !isLeaving,
                        onClick = onTick,
                        onClickLabel = "$verb ${card.title}",
                        onLongClick = { menuOpen = true },
                        onLongClickLabel = "Task actions",
                    )
                    .padding(vertical = Metrics.xs, horizontal = Metrics.xs),
            ) {
                Text(
                    card.title,
                    style = TaktTheme.type.body,
                    color = if (card.isBlocked || isLeaving) TaktTheme.colors.dimText else TaktTheme.colors.ink,
                    textDecoration = if (isLeaving) TextDecoration.LineThrough else null,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                CardDetail(card)
            }
            CardCost(card)
            Box {
                IconAction(Icons.Filled.MoreVert, "Options for ${card.title}") { menuOpen = true }
                ChalkMenu(menuOpen, onDismiss = { menuOpen = false }) { menu { menuOpen = false } }
            }
            if (canStart) {
                IconAction(Icons.Filled.PlayArrow, "Start focus on ${card.title}", tint = TaktTheme.colors.primary, onClick = onStart)
            }
        }
    }
}

@Composable
private fun CardDetail(card: DayCard) {
    val detail = DayShaping.detail(card) { Format.due(it) }
    val tint = when (card.reason) {
        DayPlanReason.OVERDUE -> TaktTheme.colors.danger
        else -> TaktTheme.colors.dimText
    }
    if (detail == null && card.listName == null && card.dailyId == null && !card.isBlocked && card.waiting == null) return
    Row(horizontalArrangement = Arrangement.spacedBy(Metrics.xs + Metrics.xxs), verticalAlignment = Alignment.CenterVertically) {
        if (card.isBlocked) Text("Blocked", style = TaktTheme.type.secondary, color = TaktTheme.colors.dimText, maxLines = 1)
        if (detail != null) Text(detail, style = TaktTheme.type.secondary, color = tint, maxLines = 1)
        WaitingMarkers(card.waiting)
        if (card.dailyId != null) {
            Icon(PIcons.Repeat, "Daily", tint = TaktTheme.colors.dimText, modifier = Modifier.size(12.dp))
        }
        card.listName?.let { name ->
            val dot = parseHexColor(card.listColorHex)
            if (dot != null) Box(Modifier.size(6.dp).clip(Metrics.control).background(dot))
            Text(name, style = TaktTheme.type.secondary, color = TaktTheme.colors.dimText, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable
private fun CardCost(card: DayCard) {
    val estimate = card.estimateSeconds
    val text = when {
        estimate != null && estimate > 0 && card.loggedSeconds > 0 -> "${Format.duration(card.loggedSeconds)}/${Format.duration(estimate)}"
        estimate != null && estimate > 0 -> Format.duration(estimate)
        card.loggedSeconds > 0 -> Format.duration(card.loggedSeconds)
        else -> return
    }
    val over = estimate != null && card.loggedSeconds > estimate
    MonoText(text, color = if (over) TaktTheme.colors.warning else TaktTheme.colors.mutedText, modifier = Modifier.padding(start = Metrics.xs))
}

@Composable
private fun CardMenu(
    card: DayCard,
    vm: TodayViewModel,
    dismiss: () -> Unit,
    onComplete: () -> Unit,
    onStart: () -> Unit,
    onHabit: () -> Unit,
    onMove: () -> Unit,
    onDelete: () -> Unit,
    onGoToList: (String) -> Unit,
) {
    // The list rows' menu where it applies here, after the day's own moves.
    ChalkMenuItem(if (card.dailyId != null) "Log today" else "Complete") { dismiss(); onComplete() }
    ChalkMenuItem(if (card.isPlanned) "Take off today" else "Plan for today") { dismiss(); vm.togglePlanned(card) }
    if (card.isPlanned) {
        ChalkMenuItem("Move up in the day") { dismiss(); vm.movePlanned(card.id, -1) }
        ChalkMenuItem("Move down in the day") { dismiss(); vm.movePlanned(card.id, 1) }
    }
    ChalkMenuItem("Due today") { dismiss(); vm.dueToday(card) }
    ChalkMenuItem("Due tomorrow") { dismiss(); vm.dueTomorrow(card) }
    if (card.dueAt != null) ChalkMenuItem("Clear due date") { dismiss(); vm.clearDue(card) }
    // Only a card here because it starts today is known to have a start.
    if (card.reason == DayPlanReason.STARTS_TODAY) ChalkMenuItem("Clear start date") { dismiss(); vm.clearStart(card) }
    ChalkMenuItem("Not today") { dismiss(); vm.deferToTomorrow(card) }
    ChalkMenuItem("Start focus") { dismiss(); onStart() }
    ChalkMenuItem("Habit…") { dismiss(); onHabit() }
    ChalkMenuItem("Open inspector") { dismiss(); vm.inspect(card) }
    ChalkMenuItem("Move to list…") { dismiss(); onMove() }
    card.listId?.let { listId -> ChalkMenuItem("Go to list") { dismiss(); onGoToList(listId) } }
    ChalkMenuItem("Delete…", destructive = true) { dismiss(); onDelete() }
}

/** The card you are on, grown: a live mono clock and the controls that only apply to it. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun RunningDayCard(
    card: DayCard,
    session: FocusSession,
    onPause: () -> Unit,
    onLog: () -> Unit,
    onDone: () -> Unit,
    onStop: () -> Unit,
    onOpen: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val paused = session.pausedAt != null
    val now by rememberTicker(active = session.isTicking)
    val reading = ClockReading.of(session, now)
    Column(
        modifier
            .fillMaxWidth()
            .background(TaktTheme.colors.primary.copy(alpha = 0.06f))
            .padding(horizontal = Metrics.lg, vertical = Metrics.md),
        verticalArrangement = Arrangement.spacedBy(Metrics.sm),
    ) {
        Text(
            card.title,
            style = TaktTheme.type.title,
            color = TaktTheme.colors.ink,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.clip(Metrics.control).combinedClickable(onClick = onOpen),
        )
        Row(verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            Text(
                reading.text,
                style = TaktTheme.type.monoLarge,
                color = when {
                    paused -> TaktTheme.colors.mutedText
                    reading.isOverrun -> TaktTheme.colors.warning
                    else -> TaktTheme.colors.primary
                },
                modifier = Modifier.testTag("today_clock"),
            )
            Text(
                when {
                    paused -> "paused"
                    session.openEnded -> "so far"
                    else -> "of ${Format.duration(session.workDurationSeconds)}"
                },
                style = TaktTheme.type.small,
                color = if (paused) TaktTheme.colors.warning else TaktTheme.colors.mutedText,
                modifier = Modifier.padding(bottom = Metrics.xs + Metrics.xxs),
            )
        }
        Row(horizontalArrangement = Arrangement.spacedBy(Metrics.sm), verticalAlignment = Alignment.CenterVertically) {
            BlockIconButton(if (paused) Icons.Filled.PlayArrow else PIcons.Pause, if (paused) "Resume" else "Pause", onPause)
            BlockIconButton(PIcons.Timer, "Log and keep", onLog)
            BlockIconButton(PIcons.Stop, "End session", onStop)
            Spacer(Modifier.weight(1f))
            PButton(
                "Done",
                Modifier.widthIn(min = 96.dp).semantics { contentDescription = "Done" },
                primary = true,
                icon = Icons.Filled.Check,
                onClick = onDone,
            )
        }
    }
    Hairline(color = TaktTheme.colors.borderMuted)
}

// endregion

// region Dailies

@Composable
private fun DailyRow(daily: DayDaily, isLeaving: Boolean, onToggle: () -> Unit, onOpen: () -> Unit, modifier: Modifier = Modifier) {
    val crossed = isLeaving || daily.isDone
    Column(modifier.background(rowFill)) {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = Metrics.touchTarget)
                .clickable(enabled = !isLeaving, onClickLabel = "Log today ${daily.title}", onClick = onToggle)
                .padding(start = Metrics.md, end = Metrics.xs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f).padding(end = Metrics.xs)) {
                Text(
                    daily.title,
                    style = TaktTheme.type.body,
                    color = if (crossed) TaktTheme.colors.dimText else TaktTheme.colors.ink,
                    textDecoration = if (crossed) TextDecoration.LineThrough else null,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                daily.progressFraction?.let { fraction ->
                    Box(Modifier.padding(top = Metrics.xs).width(120.dp).height(3.dp).background(TaktTheme.colors.well)) {
                        Box(Modifier.fillMaxWidth(fraction).height(3.dp).background(if (crossed) TaktTheme.colors.success else TaktTheme.colors.categoricalPurple))
                    }
                }
            }
            daily.progressText?.let { MonoText(it) }
            IconAction(Icons.Filled.MoreVert, "Options for ${daily.title}", onClick = onOpen)
        }
    }
}

// endregion
