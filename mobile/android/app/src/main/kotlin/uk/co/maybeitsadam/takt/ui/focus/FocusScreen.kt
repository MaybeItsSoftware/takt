package uk.co.maybeitsadam.takt.ui.focus

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
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
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.LocationOn
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import java.time.Instant
import java.time.LocalTime
import uk.co.maybeitsadam.takt.core.FocusPoints
import uk.co.maybeitsadam.takt.core.FocusTimeMode
import uk.co.maybeitsadam.takt.core.ScoredNextUp
import uk.co.maybeitsadam.takt.core.TaskCondition
import uk.co.maybeitsadam.takt.data.workspace.ConditionGroups
import uk.co.maybeitsadam.takt.data.workspace.ConditionHoldings
import uk.co.maybeitsadam.takt.ui.commands.KeyCommand
import uk.co.maybeitsadam.takt.ui.commands.PaletteCommand
import uk.co.maybeitsadam.takt.ui.commands.RegisterCommands
import uk.co.maybeitsadam.takt.ui.conditions.NowConditionsSheet
import uk.co.maybeitsadam.takt.ui.components.Card
import uk.co.maybeitsadam.takt.ui.components.EmptyState
import uk.co.maybeitsadam.takt.ui.components.Format
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.IconAction
import uk.co.maybeitsadam.takt.ui.components.MonoText
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.components.SectionHeader
import uk.co.maybeitsadam.takt.ui.components.Tag
import uk.co.maybeitsadam.takt.ui.components.TaktTopBar
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.PIcons
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.today.ChalkMenu
import uk.co.maybeitsadam.takt.ui.today.ChalkMenuItem

/**
 * Focus: say where you are and how long you have, pick a rung of the ranked
 * ladder, set the block's length, begin; then the running block, its clock,
 * and the question of how it went. The Mac's `WorkspaceFocusScreen`.
 */
@Composable
fun FocusScreen() {
    val container = LocalShell.current.container
    val vm = viewModel { FocusViewModel(container) }
    val state by vm.state.collectAsStateWithLifecycle()
    val context by vm.contextState.collectAsStateWithLifecycle()
    val pending by vm.pendingCompletion.collectAsStateWithLifecycle()
    val override by vm.startOverride.collectAsStateWithLifecycle()
    val gate = rememberNotificationGate()

    val session = state.session
    RegisterCommands(
        buildList {
            if (session != null) {
                add(PaletteCommand("focusPause", if (session.pausedAt == null) "Pause the block" else "Resume the block", "Focus") { vm.togglePause() })
                add(PaletteCommand("focusLogKeep", "Log the block and keep the task", "Focus") { vm.requestCompletion(false) })
                add(PaletteCommand("focusFinish", "Finish the block", "Focus") { vm.requestCompletion(true) })
                add(PaletteCommand("focusEnd", "End the session", "Focus") { vm.endSession() })
            } else {
                state.ladder.firstOrNull()?.let { top ->
                    add(KeyCommand.TASK_START_FOCUS.palette { gate { vm.begin(top.id, suggestedSeconds(top, context)) } })
                }
                if (state.hasManualOrder) add(PaletteCommand("focusReset", "Reset the focus order", "Focus") { vm.resetOrder() })
            }
        },
    )

    Column(Modifier.fillMaxSize().background(TaktTheme.colors.paper)) {
        TaktTopBar(
            "Focus",
            sidebar = true,
            subtitle = if (session != null) null else FocusText.timeTitle(context.endsAt, Instant.now()),
            actions = {
                if (session == null && state.hasManualOrder) IconAction(Icons.Filled.Refresh, "Reset order", onClick = vm::resetOrder)
            },
        )
        if (session != null) {
            RunningBlock(state, vm)
        } else {
            Planning(state, context, vm, gate)
        }
    }

    pending?.let { QualityPrompt(it, onScore = vm::confirmCompletion, onCancel = vm::cancelCompletion, todayPoints = state.points.today) }
    override?.let { held ->
        ChalkDialog(
            "Start anyway?",
            onDismiss = vm::dismissOverride,
            confirm = "Start anyway",
            onConfirm = { gate { vm.confirmOverride() } },
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(Metrics.xs)) {
                Text(held.title, style = TaktTheme.type.bodyStrong, color = TaktTheme.colors.ink)
                Text(held.explanation, style = TaktTheme.type.small, color = TaktTheme.colors.mutedText)
            }
        }
    }
}

private fun suggestedSeconds(rung: ScoredNextUp, context: FocusContextState): Int =
    uk.co.maybeitsadam.takt.core.TaskAvailabilityPolicy.suggestedSeconds(rung.candidate, context.context, Instant.now())

// region Planning

@Composable
private fun Planning(state: FocusUiState, context: FocusContextState, vm: FocusViewModel, gate: (() -> Unit) -> Unit) {
    val staged by vm.staged.collectAsStateWithLifecycle()
    val stagedMinutes by vm.stagedMinutes.collectAsStateWithLifecycle()
    val lastBlock by vm.lastBlock.collectAsStateWithLifecycle()
    var showBlocked by rememberSaveable { mutableStateOf(false) }
    LazyColumn(
        Modifier.fillMaxSize().testTag("focus_ladder"),
        contentPadding = PaddingValues(bottom = 32.dp),
    ) {
        item(key = "points", contentType = "points") {
            val scoring by vm.scoresEachFocusBlock.collectAsStateWithLifecycle()
            PointsStrip(state, lastBlock, scoring, onDismiss = vm::dismissLastBlock)
        }
        item(key = "context", contentType = "context") { ContextControls(state, context, vm) }
        val stagedId = staged
        if (stagedId != null) {
            val rung = state.ladder.firstOrNull { it.id == stagedId }
            val title = rung?.candidate?.title ?: state.blocked.firstOrNull { it.id == stagedId }?.candidate?.title
            if (title != null) {
                item(key = "staged", contentType = "staged") {
                    StagedCard(
                        title = title,
                        explanation = rung?.let { FocusText.explanation(it, context.held, state.conditions) },
                        minutes = stagedMinutes,
                        onMinutes = vm::setStagedMinutes,
                        onBegin = { gate { vm.beginStaged() } },
                        onBack = vm::unstage,
                    )
                }
            }
        }
        item(key = "ladder-header", contentType = "section") {
            SectionHeader("Ladder", detail = state.ladder.size.toString(), trailing = if (state.hasManualOrder) {
                { Tag("Reset order", onClick = vm::resetOrder) }
            } else {
                null
            })
        }
        if (state.isLoaded && state.ladder.isEmpty()) {
            item(key = "ladder-empty", contentType = "empty") {
                EmptyState("Nothing fits", detail = "Nothing fits this context and time. Change them, or add a task.")
            }
        }
        itemsIndexed(state.ladder, key = { _, rung -> rung.id }, contentType = { _, _ -> "rung" }) { index, rung ->
            RungRow(
                rung = rung,
                index = index,
                count = state.ladder.size,
                explanation = FocusText.explanation(rung, context.held, state.conditions),
                isStaged = staged == rung.id,
                vm = vm,
                onBegin = { gate { vm.begin(rung.id, suggestedSeconds(rung, context)) } },
                modifier = Modifier.animateItem(),
            )
        }
        if (state.blocked.isNotEmpty()) {
            item(key = "blocked-header", contentType = "section") {
                SectionHeader(
                    "Not available now",
                    detail = state.blocked.size.toString(),
                    trailing = { Tag(if (showBlocked) "Hide" else "Show", onClick = { showBlocked = !showBlocked }) },
                )
            }
            if (showBlocked) {
                items(state.blocked, key = { "blocked-${it.id}" }, contentType = { "blocked" }) { blocked ->
                    BlockedRow(
                        title = blocked.candidate.title,
                        reasons = blocked.reasons.joinToString(" · ") { FocusText.unavailable(it, state.conditions) },
                        onStage = { vm.stage(blocked.id, blocked.candidate.title) },
                        onInspect = { vm.inspect(blocked.id) },
                    )
                }
            }
        }
    }
}

@Composable
private fun PointsStrip(state: FocusUiState, last: LastBlock?, scoring: Boolean, onDismiss: () -> Unit) {
    // With focus points off there is nothing to show but what the last block did.
    if (!scoring && last == null) return
    Column(Modifier.fillMaxWidth().padding(horizontal = Metrics.lg, vertical = Metrics.md), verticalArrangement = Arrangement.spacedBy(Metrics.sm)) {
        if (last != null) {
            val warning = TaktTheme.colors.warning
            Row(
                Modifier
                    .fillMaxWidth()
                    .clip(Metrics.card)
                    .background(warning.copy(alpha = 0.10f))
                    .border(BorderStroke(Metrics.hairline, warning.copy(alpha = 0.4f)), Metrics.card)
                    .padding(start = Metrics.md),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val award = last.award
                MonoText(if (award != null && scoring) "+${FocusPoints.formatted(award.points)} points" else "Logged", color = TaktTheme.colors.ink)
                Spacer(Modifier.width(Metrics.sm))
                Text(FocusActions.outcomeText(last.outcome), style = TaktTheme.type.small, color = TaktTheme.colors.mutedText, modifier = Modifier.weight(1f))
                IconAction(Icons.Filled.Close, "Dismiss", onClick = onDismiss)
            }
        }
        if (scoring) Row(Modifier.fillMaxWidth().testTag("focus_points")) {
            Stat("Today", FocusPoints.formatted(state.points.today), Modifier.weight(1f))
            Stat("7 days", FocusPoints.formatted(state.points.last7Days), Modifier.weight(1f))
            Stat("All time", FocusPoints.formatted(state.points.allTime), Modifier.weight(1f))
            Stat("Blocks today", state.points.blocksToday.toString(), Modifier.weight(1f))
        }
    }
    Hairline()
}

@Composable
private fun Stat(label: String, value: String, modifier: Modifier = Modifier) {
    Column(modifier) {
        MonoText(value, style = TaktTheme.type.monoBody, color = TaktTheme.colors.ink)
        Text(label, style = TaktTheme.type.small, color = TaktTheme.colors.mutedText, maxLines = 1)
    }
}

@Composable
private fun ContextControls(state: FocusUiState, context: FocusContextState, vm: FocusViewModel) {
    var addingCondition by rememberSaveable { mutableStateOf(false) }
    var choosing by rememberSaveable { mutableStateOf(false) }
    var customMinutes by rememberSaveable { mutableStateOf(false) }
    var pickingUntil by rememberSaveable { mutableStateOf(false) }
    // Every condition said, provided or off for now stays offered, so it can
    // be turned off, or back on.
    val groups = remember(state.conditions, state.requiredConditionIds, context.holdings) {
        ConditionGroups.available(state.conditions, state.requiredConditionIds, context.holdings.keys)
    }
    Column(Modifier.fillMaxWidth().padding(vertical = Metrics.sm), verticalArrangement = Arrangement.spacedBy(Metrics.xs)) {
        Text("Where you are", style = TaktTheme.type.label, color = TaktTheme.colors.mutedText, modifier = Modifier.padding(horizontal = Metrics.lg))
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = Metrics.lg),
            horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (groups.isEmpty()) {
                Text(
                    "No open tasks need a condition",
                    style = TaktTheme.type.small,
                    color = TaktTheme.colors.mutedText,
                    modifier = Modifier.testTag("focus_conditions_empty"),
                )
            }
            // Every condition in one sheet, searched, for when the chips are many.
            Tag("All…", leading = Icons.Filled.Search, onClick = { choosing = true }, modifier = Modifier.testTag("focus_conditions_all"))
            Tag("New condition", leading = Icons.Filled.Add, onClick = { addingCondition = true })
        }
        // Only what an open task needs, under its category: a condition nothing
        // waits on changes nothing, so it is not asked about.
        for (group in groups) {
            Text(group.title, style = TaktTheme.type.small, color = TaktTheme.colors.mutedText, modifier = Modifier.padding(horizontal = Metrics.lg))
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = Metrics.lg),
                horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                for (condition in group.conditions) {
                    val on = condition.id in context.held
                    val note = ConditionHoldings.note(context.holdings[condition.id], state.conditions)
                    Tag(
                        condition.name,
                        color = if (on) TaktTheme.colors.primary else TaktTheme.colors.mutedText,
                        selected = on,
                        leading = if (group.isExclusive) Icons.Filled.LocationOn else null,
                        modifier = Modifier
                            .testTag("focus_condition_${condition.name}")
                            .semantics { if (note != null) stateDescription = note },
                        onClick = { vm.toggleCondition(condition) },
                    )
                }
            }
        }
        // What the settings provide, and what is off for now: "Desk, Quiet
        // from Home · Floor space off for now". The chips are too small to say it.
        ProvidedNote(context, state.conditions)
        // Said on an earlier day, here or on another device: probably not
        // where you are now. The Mac's notice, in the same words.
        if (context.isStale) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Metrics.lg).testTag("focus_context_stale"),
                horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(Icons.Filled.Warning, contentDescription = null, tint = TaktTheme.colors.warning, modifier = Modifier.size(16.dp))
                Text(
                    "Set on an earlier day — still true?",
                    style = TaktTheme.type.small,
                    color = TaktTheme.colors.mutedText,
                    modifier = Modifier.weight(1f),
                )
                Tag("Clear", color = TaktTheme.colors.primary, onClick = { vm.clearConditions() })
            }
        }
        Text("Free time", style = TaktTheme.type.label, color = TaktTheme.colors.mutedText, modifier = Modifier.padding(start = Metrics.lg, end = Metrics.lg, top = Metrics.xs))
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = Metrics.lg),
            horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
        ) {
            val time = context.time
            TimeTag("Any", time == AvailableTime.Unlimited) { vm.setAvailable(AvailableTime.Unlimited) }
            for (minutes in FocusText.presetMinutes) {
                TimeTag("${minutes}m", time == AvailableTime.Minutes(minutes), mono = true) { vm.setAvailable(AvailableTime.Minutes(minutes)) }
            }
            val custom = (time as? AvailableTime.Minutes)?.takeIf { it.minutes !in FocusText.presetMinutes }
            TimeTag(custom?.let { "${it.minutes}m" } ?: "Custom…", custom != null, mono = custom != null) { customMinutes = true }
            val until = time as? AvailableTime.Until
            TimeTag(until?.let { "Until ${FocusText.hourMinute(it.time)}" } ?: "Until…", until != null) { pickingUntil = true }
        }
        Text("Mode", style = TaktTheme.type.label, color = TaktTheme.colors.mutedText, modifier = Modifier.padding(start = Metrics.lg, end = Metrics.lg, top = Metrics.xs))
        Row(Modifier.padding(horizontal = Metrics.lg), horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            TimeTag("Make progress", context.mode == FocusTimeMode.PROGRESS) { vm.setMode(FocusTimeMode.PROGRESS) }
            TimeTag("Finish something", context.mode == FocusTimeMode.FINISH) { vm.setMode(FocusTimeMode.FINISH) }
        }
    }
    Hairline()

    if (choosing) NowConditionsSheet(onDismiss = { choosing = false })
    if (addingCondition) {
        NewConditionDialog(
            categories = remember(state.conditions) { ConditionGroups.categories(state.conditions) },
            onCreate = { name, category -> vm.createCondition(name, category); addingCondition = false },
            onDismiss = { addingCondition = false },
        )
    }
    if (customMinutes) {
        MinutesDialog(
            "Time you have",
            initial = (context.time as? AvailableTime.Minutes)?.minutes ?: 45,
            onPick = { vm.setAvailable(AvailableTime.Minutes(it)); customMinutes = false },
            onDismiss = { customMinutes = false },
        )
    }
    if (pickingUntil) {
        TimeOfDayDialog(
            "Until",
            initial = (context.time as? AvailableTime.Until)?.time ?: LocalTime.now().plusHours(1).withMinute(0),
            onPick = { vm.setAvailable(AvailableTime.Until(it)); pickingUntil = false },
            onDismiss = { pickingUntil = false },
        )
    }
}

@Composable
private fun TimeTag(text: String, selected: Boolean, mono: Boolean = false, onClick: () -> Unit) {
    Tag(text, color = if (selected) TaktTheme.colors.primary else TaktTheme.colors.mutedText, selected = selected, mono = mono, onClick = onClick)
}

@Composable
private fun StagedCard(
    title: String,
    explanation: String?,
    minutes: Int,
    onMinutes: (Int) -> Unit,
    onBegin: () -> Unit,
    onBack: () -> Unit,
) {
    Card(Modifier.fillMaxWidth().padding(horizontal = Metrics.lg, vertical = Metrics.sm), selected = true) {
        Column(Modifier.padding(Metrics.lg), verticalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            Text(title, style = TaktTheme.type.title, color = TaktTheme.colors.ink, maxLines = 3, overflow = TextOverflow.Ellipsis)
            if (explanation != null) Text(explanation, style = TaktTheme.type.small, color = TaktTheme.colors.mutedText)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
                MonoText(Format.duration(minutes * 60), style = TaktTheme.type.monoTitle, color = TaktTheme.colors.ink, modifier = Modifier.widthIn(min = 72.dp).testTag("focus_block_length"))
                BlockIconButton(MinusIcon, "Five minutes shorter", onClick = { onMinutes(minutes - 5) })
                BlockIconButton(Icons.Filled.Add, "Five minutes longer", onClick = { onMinutes(minutes + 5) })
            }
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
                for (preset in listOf(15, 25, 45, 60, 90)) {
                    TimeTag("${preset}m", minutes == preset, mono = true) { onMinutes(preset) }
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
                PButton(
                    "Begin",
                    Modifier.weight(1f).semantics { contentDescription = "Begin $title" }.testTag("focus_begin"),
                    primary = true,
                    icon = Icons.Filled.PlayArrow,
                    onClick = onBegin,
                )
                PButton("Back", onClick = onBack)
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun RungRow(
    rung: ScoredNextUp,
    index: Int,
    count: Int,
    explanation: String,
    isStaged: Boolean,
    vm: FocusViewModel,
    onBegin: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var menu by remember { mutableStateOf(false) }
    var pickingTime by rememberSaveable { mutableStateOf(false) }
    val title = rung.candidate.title
    Column(modifier.background(if (isStaged) TaktTheme.colors.primary.copy(alpha = 0.08f) else TaktTheme.colors.paper)) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(start = Metrics.lg, end = Metrics.xs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            MonoText("${index + 1}", color = if (index == 0) TaktTheme.colors.primary else TaktTheme.colors.dimText, modifier = Modifier.width(24.dp))
            Box(Modifier.weight(1f)) {
                Column(
                    Modifier
                        .fillMaxWidth()
                        .clip(Metrics.control)
                        .combinedClickable(onClick = { vm.stage(rung) }, onLongClick = { menu = true }, onClickLabel = "Stage", onLongClickLabel = "Task actions")
                        .padding(vertical = Metrics.sm, horizontal = Metrics.xs),
                ) {
                    Text(title, style = if (index == 0) TaktTheme.type.bodyStrong else TaktTheme.type.body, color = TaktTheme.colors.ink, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    Row(horizontalArrangement = Arrangement.spacedBy(Metrics.xs + Metrics.xxs), verticalAlignment = Alignment.CenterVertically) {
                        if (rung.candidate.isDailyDueToday) Icon(PIcons.Repeat, "Daily", tint = TaktTheme.colors.categoricalPurple, modifier = Modifier.size(12.dp))
                        if (rung.candidate.focusRank != null) Icon(PIcons.Flag, "Pinned", tint = TaktTheme.colors.mutedText, modifier = Modifier.size(12.dp))
                        Text(explanation, style = TaktTheme.type.small, color = TaktTheme.colors.mutedText, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    }
                }
                ChalkMenu(menu, onDismiss = { menu = false }) {
                    ChalkMenuItem("Stage") { menu = false; vm.stage(rung) }
                    ChalkMenuItem("Begin now") { menu = false; onBegin() }
                    ChalkMenuItem(if (rung.candidate.isDailyDueToday) "Log today's daily" else "Tick off") { menu = false; vm.completeWithoutSession(rung) }
                    for (deferral in FocusDeferral.entries) {
                        ChalkMenuItem("Later: ${deferral.title.lowercase()}") { menu = false; vm.deferTask(rung.id, deferral) }
                    }
                    ChalkMenuItem("Later: pick a time…") { menu = false; pickingTime = true }
                    ChalkMenuItem("Move up", enabled = index > 0) { menu = false; vm.moveRung(rung.id, -1) }
                    ChalkMenuItem("Move down", enabled = index < count - 1) { menu = false; vm.moveRung(rung.id, 1) }
                    if (rung.candidate.focusRank != null) ChalkMenuItem("Unpin") { menu = false; vm.unpin(rung.id) }
                    ChalkMenuItem("Open inspector") { menu = false; vm.inspect(rung.id) }
                }
            }
            rung.candidate.remainingSeconds?.takeIf { it > 0 }?.let {
                MonoText(Format.duration(it), modifier = Modifier.padding(horizontal = Metrics.xs))
            }
            IconAction(Icons.Filled.PlayArrow, "Start focus on $title", tint = TaktTheme.colors.primary, onClick = onBegin)
        }
        Hairline(color = TaktTheme.colors.borderMuted)
    }
    if (pickingTime) {
        TimeOfDayDialog(
            "Defer until",
            initial = LocalTime.now().plusHours(2).withMinute(0),
            onPick = { time ->
                pickingTime = false
                val at = FocusText.endsAt(AvailableTime.Until(time), Instant.now()) ?: return@TimeOfDayDialog
                vm.deferTo(rung.id, at, "${Format.day(at).lowercase()} ${Format.time(at)}")
            },
            onDismiss = { pickingTime = false },
        )
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun BlockedRow(title: String, reasons: String, onStage: () -> Unit, onInspect: () -> Unit) {
    var menu by remember { mutableStateOf(false) }
    Column {
        Box {
            Column(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = Metrics.touchTarget)
                    .combinedClickable(onClick = { menu = true }, onLongClick = { menu = true })
                    .padding(horizontal = Metrics.lg, vertical = Metrics.sm),
            ) {
                Text(title, style = TaktTheme.type.body, color = TaktTheme.colors.mutedText, maxLines = 2, overflow = TextOverflow.Ellipsis)
                Text(reasons, style = TaktTheme.type.small, color = TaktTheme.colors.dimText, maxLines = 3)
            }
            ChalkMenu(menu, onDismiss = { menu = false }) {
                ChalkMenuItem("Stage anyway") { menu = false; onStage() }
                ChalkMenuItem("Open inspector") { menu = false; onInspect() }
            }
        }
        Hairline(color = TaktTheme.colors.borderMuted)
    }
}

// endregion

// region Running

@Composable
private fun RunningBlock(state: FocusUiState, vm: FocusViewModel) {
    val session = state.session ?: return
    val paused = session.pausedAt != null
    val now by rememberTicker(active = session.isTicking)
    LaunchedEffect(now) { vm.tick(now) }
    val reading = ClockReading.of(session, now)
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Metrics.lg),
        verticalArrangement = Arrangement.spacedBy(Metrics.lg),
    ) {
        if (session.activeTaskId == null) {
            EmptyState("Nothing in the queue can run now", detail = "The rest of this session's queue doesn't fit the current context.")
            PButton("End session", Modifier.fillMaxWidth()) { vm.endSession() }
            return@Column
        }
        Column(verticalArrangement = Arrangement.spacedBy(Metrics.xs)) {
            Text(if (paused) "Paused" else "Focusing", style = TaktTheme.type.label, color = if (paused) TaktTheme.colors.warning else TaktTheme.colors.primary)
            Text(state.activeTitle ?: "", style = TaktTheme.type.headline, color = TaktTheme.colors.ink)
        }
        Column(verticalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            Text(
                reading.text,
                style = TaktTheme.type.hero,
                color = when {
                    paused -> TaktTheme.colors.mutedText
                    reading.isOverrun -> TaktTheme.colors.warning
                    else -> TaktTheme.colors.ink
                },
                modifier = Modifier.testTag("focus_clock").semantics { contentDescription = "Elapsed ${reading.text}" },
            )
            if (session.openEnded) {
                MonoText("${Format.clock(reading.elapsedSeconds)} so far · timer")
            } else {
                Box(Modifier.fillMaxWidth().height(4.dp).background(TaktTheme.colors.well)) {
                    Box(Modifier.fillMaxWidth(reading.fraction).height(4.dp).background(if (reading.isOverrun) TaktTheme.colors.warning else TaktTheme.colors.primary))
                }
                MonoText("${Format.clock(reading.elapsedSeconds)} / ${Format.duration(maxOf(60, session.workDurationSeconds))} planned")
            }
        }
        PButton(
            "Finish",
            Modifier.fillMaxWidth().semantics { contentDescription = "Finish" }.testTag("focus_finish"),
            primary = true,
            icon = Icons.Filled.Check,
        ) { vm.requestCompletion(completeTask = true) }
        Row(horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            PButton(
                if (paused) "Resume" else "Pause",
                Modifier.weight(1f).semantics { contentDescription = if (paused) "Resume" else "Pause" },
                icon = if (paused) Icons.Filled.PlayArrow else PIcons.Pause,
            ) { vm.togglePause() }
            PButton(
                "Log and keep",
                Modifier.weight(1f).semantics { contentDescription = "Log and keep" },
                icon = PIcons.Timer,
            ) { vm.requestCompletion(completeTask = false) }
        }
        Text(
            "End session",
            style = TaktTheme.type.body,
            color = TaktTheme.colors.mutedText,
            modifier = Modifier
                .clip(Metrics.control)
                .clickable(role = Role.Button, onClick = vm::endSession)
                .heightIn(min = Metrics.touchTarget)
                .padding(horizontal = Metrics.sm, vertical = Metrics.md + Metrics.xxs),
        )
        if (state.queue.isNotEmpty()) {
            Column {
                Text("Up next in this session", style = TaktTheme.type.label, color = TaktTheme.colors.mutedText, modifier = Modifier.padding(bottom = Metrics.xs))
                for (entry in state.queue) {
                    Row(Modifier.fillMaxWidth().heightIn(min = 40.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text(entry.task.title, style = TaktTheme.type.body, color = TaktTheme.colors.ink, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
                        if (entry.item.openEnded) MonoText("timer") else entry.item.plannedSeconds?.let { MonoText(Format.duration(it)) }
                    }
                    Hairline(color = TaktTheme.colors.borderMuted)
                }
            }
        }
    }
}

private val MinusIcon by lazy { PIcons.icon("Minus", "M19,13H5v-2h14v2z") }

// endregion

/** One line under the chips saying where the provided conditions come from, and which are off for now. */
@Composable
private fun ProvidedNote(context: FocusContextState, conditions: List<TaskCondition>) {
    val name = { id: String -> conditions.firstOrNull { it.id == id }?.name ?: id }
    val provided = context.holdings.values.filter { it.holds && !it.isSet && it.providedBy.isNotEmpty() }
        .groupBy { holding -> holding.providedBy.joinToString(", ") { name(it) } }
        .map { (from, held) -> held.joinToString(", ") { name(it.conditionId) } + " from " + from }
    val off = context.holdings.values.filter { it.isOff }.map { name(it.conditionId) }
    val parts = provided + listOfNotNull(off.takeIf { it.isNotEmpty() }?.let { it.joinToString(", ") + " off for now" })
    if (parts.isEmpty()) return
    Text(
        parts.joinToString(" · "),
        style = TaktTheme.type.small,
        color = TaktTheme.colors.mutedText,
        modifier = Modifier.padding(horizontal = Metrics.lg).testTag("focus_conditions_provided"),
    )
}
