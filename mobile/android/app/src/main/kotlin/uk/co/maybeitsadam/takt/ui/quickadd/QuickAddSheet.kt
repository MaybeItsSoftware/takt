package uk.co.maybeitsadam.takt.ui.quickadd

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.withFrameNanos
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material.icons.filled.LocationOn
import androidx.compose.ui.focus.onFocusChanged
import java.time.DayOfWeek
import java.time.LocalDate
import java.time.ZoneId
import java.time.temporal.TemporalAdjusters
import uk.co.maybeitsadam.takt.ui.conditions.RequirementToggle
import uk.co.maybeitsadam.takt.ui.conditions.conditionsViewModel
import uk.co.maybeitsadam.takt.ui.inspector.MonthGrid
import uk.co.maybeitsadam.takt.app.QuickAddRequest
import uk.co.maybeitsadam.takt.ui.SheetHandle
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.components.Tag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import uk.co.maybeitsadam.takt.ui.inspector.ChalkField
import uk.co.maybeitsadam.takt.ui.inspector.ChalkMenuItem
import uk.co.maybeitsadam.takt.ui.inspector.FieldLabel
import uk.co.maybeitsadam.takt.ui.inspector.MenuField
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.PIcons

/**
 * Quick add: the field, then a row of taps for what most tasks need — due
 * day, estimate, priority, the conditions it needs, planning it for today —
 * each opening its choices in place below the row, a month to tap for the
 * day. Typing `45m #work ^mon @fri !1` on the end still works; a tap adds the
 * same token behind the scenes, so the text stays the one thing filed. It
 * stays open after Add so a run of tasks can go in one sitting.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun QuickAddSheet(request: QuickAddRequest, onDismiss: () -> Unit) {
    val container = LocalShell.current.container
    val vm: QuickAddViewModel = viewModel(key = "quick_add") { QuickAddViewModel(container) }
    val targets by vm.targets.collectAsStateWithLifecycle()
    val conditionsVm = conditionsViewModel()
    val conditionsState by conditionsVm.state.collectAsStateWithLifecycle()
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)

    var text by rememberSaveable(request.nonce) { mutableStateOf(request.text) }
    var chosenListId by rememberSaveable(request.nonce) { mutableStateOf(request.listId) }
    var plansToday by rememberSaveable(request.nonce) { mutableStateOf(request.plansToday) }
    // What the taps chose, as the tokens they stand for.
    var dueDay by rememberSaveable(request.nonce) { mutableStateOf<String?>(null) }
    var estimate by rememberSaveable(request.nonce) { mutableStateOf<String?>(null) }
    var priority by rememberSaveable(request.nonce) { mutableStateOf<Int?>(null) }
    var needs by remember(request.nonce) { mutableStateOf<List<List<String>>?>(null) }
    var panel by rememberSaveable(request.nonce) { mutableStateOf<String?>(null) }
    val added = remember(request.nonce) { mutableStateListOf<String>() }
    val filed by remember {
        derivedStateOf {
            listOfNotNull(text.trim(), estimate, dueDay?.let { "@$it" }, priority?.let { "!$it" })
                .filter { it.isNotEmpty() }.joinToString(" ")
        }
    }
    val preview by remember { derivedStateOf { CapturePreview.of(filed) } }
    val targetId = chosenListId?.takeIf { id -> targets.lists.any { it.id == id } } ?: targets.inboxId
    val target = targets.named(targetId)
    val today = remember { LocalDate.now(ZoneId.systemDefault()) }

    val focus = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    val focusManager = LocalFocusManager.current
    // Focus as soon as the field is laid out, so the keyboard rises with the
    // sheet rather than after it.
    LaunchedEffect(request.nonce) {
        // The field attaches a frame or two after the sheet opens; try each
        // frame until it takes, rather than waiting out the slide.
        repeat(30) {
            if (runCatching { focus.requestFocus() }.isSuccess) {
                keyboard?.show()
                return@LaunchedEffect
            }
            withFrameNanos { }
        }
    }

    fun openPanel(name: String) {
        panel = if (panel == name) null else name
        // A panel is for tapping: the keyboard steps aside until the field is touched again.
        if (panel != null) {
            focusManager.clearFocus()
            keyboard?.hide()
        }
    }

    fun submit() {
        val listId = targetId ?: return
        if (!preview.canAdd) return
        // A parent only makes sense in the list it was asked for.
        val parent = request.parentTaskId?.takeIf { listId == (request.listId ?: targets.inboxId) }
        vm.add(filed, listId, parent, plansToday, needs)
        added.add(0, preview.title)
        text = ""
        dueDay = null
        estimate = null
        priority = null
        needs = null
        panel = null
        runCatching { focus.requestFocus() }
        keyboard?.show()
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = TaktTheme.colors.altRow,
        contentColor = TaktTheme.colors.ink,
        tonalElevation = 0.dp,
        shape = Metrics.sheetTop,
        dragHandle = { SheetHandle() },
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .imePadding()
                .navigationBarsPadding()
                .padding(horizontal = Metrics.lg)
                .padding(bottom = Metrics.lg)
                .testTag("quick_add_sheet"),
            verticalArrangement = Arrangement.spacedBy(Metrics.sm),
        ) {
            Text("Add a task", style = TaktTheme.type.title, color = TaktTheme.colors.ink)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
                ChalkField(
                    text,
                    onValueChange = { text = it.replace("\n", " ") },
                    placeholder = "What needs doing?",
                    style = TaktTheme.type.body.copy(fontSize = TaktTheme.type.heading.fontSize),
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { submit() }),
                    modifier = Modifier
                        .weight(1f)
                        .focusRequester(focus)
                        .onFocusChanged { if (it.isFocused) panel = null }
                        .onPreviewKeyEvent { event ->
                            val enter = event.key == Key.Enter || event.key == Key.NumPadEnter
                            if (enter && event.type == KeyEventType.KeyDown) {
                                submit()
                                true
                            } else {
                                enter
                            }
                        }
                        .testTag("quick_add_field"),
                    contentDescription = "New task",
                )
                PButton(
                    "Add",
                    primary = true,
                    enabled = preview.canAdd && targetId != null,
                    modifier = Modifier.testTag("quick_add_submit"),
                    onClick = ::submit,
                )
            }

            // The taps. Each shows what is set — tapped or typed — in its own hue.
            val dueChip = preview.chips.firstOrNull { it.kind == CaptureChipKind.DUE }
            val estimateChip = preview.chips.firstOrNull { it.kind == CaptureChipKind.ESTIMATE }
            val priorityChip = preview.chips.firstOrNull { it.kind == CaptureChipKind.PRIORITY }
            val needsLabel = needs?.flatten()?.mapNotNull { id -> conditionsState.conditions.firstOrNull { it.id == id }?.name }
                ?.joinToString(", ")?.ifEmpty { null }
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(Metrics.xs),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                PickTag(
                    if (plansToday) "Today" else "Not today",
                    on = plansToday, open = false, icon = if (plansToday) Icons.Filled.Check else PIcons.Today, tag = "quick_add_plan_today",
                ) { plansToday = !plansToday }
                PickTag(dueChip?.label ?: "Due", on = dueChip != null, open = panel == "due", icon = PIcons.Calendar, tag = "quick_add_due") { openPanel("due") }
                PickTag(estimateChip?.label ?: "Estimate", on = estimateChip != null, open = panel == "estimate", icon = PIcons.Timer, tag = "quick_add_estimate") { openPanel("estimate") }
                PickTag(priorityChip?.label ?: "Priority", on = priorityChip != null, open = panel == "priority", icon = PIcons.Flag, tag = "quick_add_priority") { openPanel("priority") }
                PickTag(needsLabel ?: "Needs", on = needsLabel != null, open = panel == "needs", icon = Icons.Filled.LocationOn, tag = "quick_add_needs") { openPanel("needs") }
            }

            when (panel) {
                "due" -> PickPanel {
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(Metrics.xs), verticalArrangement = Arrangement.spacedBy(Metrics.xs)) {
                        val saturday = today.with(TemporalAdjusters.nextOrSame(DayOfWeek.SATURDAY))
                        val monday = today.with(TemporalAdjusters.next(DayOfWeek.MONDAY))
                        for ((label, day) in listOf("Today" to today, "Tomorrow" to today.plusDays(1), "Weekend" to saturday, "Next week" to monday)) {
                            Choice(label, dueDay == day.toString()) { dueDay = day.toString(); panel = null }
                        }
                        if (dueDay != null) Choice("No due day", false) { dueDay = null; panel = null }
                    }
                    MonthGrid(
                        selected = dueDay?.let(LocalDate::parse),
                        today = today,
                        onPick = { dueDay = it.toString(); panel = null },
                        modifier = Modifier.testTag("quick_add_month"),
                    )
                }
                "estimate" -> PickPanel {
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(Metrics.xs), verticalArrangement = Arrangement.spacedBy(Metrics.xs)) {
                        for (option in listOf("5m", "15m", "30m", "45m", "1h", "1h30m", "2h", "3h", "4h")) {
                            Choice(option, estimate == option, mono = true) { estimate = option; panel = null }
                        }
                        if (estimate != null) Choice("None", false) { estimate = null; panel = null }
                    }
                }
                "priority" -> PickPanel {
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(Metrics.xs), verticalArrangement = Arrangement.spacedBy(Metrics.xs)) {
                        for (level in 1..3) {
                            Choice("!$level", priority == level, mono = true) { priority = level; panel = null }
                        }
                        if (priority != null) Choice("None", false) { priority = null; panel = null }
                    }
                }
                "needs" -> PickPanel {
                    val offered = conditionsState.conditions.filter { !it.isArchived }
                    if (offered.isEmpty()) {
                        Text(
                            "No conditions yet. Add places and tools in Today's where-you-are bar, then tick what a task needs here.",
                            style = TaktTheme.type.small, color = TaktTheme.colors.mutedText,
                        )
                    }
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(Metrics.xs), verticalArrangement = Arrangement.spacedBy(Metrics.xs)) {
                        val places = offered.filter { it.isLocation }.map { it.id }.toSet()
                        for (condition in offered) {
                            Choice(condition.name, RequirementToggle.requires(condition.id, needs)) {
                                needs = RequirementToggle.toggling(condition.id, needs, places)
                            }
                        }
                    }
                }
            }

            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
                FieldLabel("Into")
                var open by remember { mutableStateOf(false) }
                MenuField(
                    target?.name ?: "Inbox",
                    expanded = open,
                    onExpandedChange = { open = it },
                    leading = if (target?.isInbox != false) PIcons.Inbox else PIcons.ListIcon,
                    contentDescription = "Add to ${target?.name ?: "Inbox"}",
                    modifier = Modifier.weight(1f).testTag("quick_add_target"),
                ) {
                    for (option in targets.lists) {
                        ChalkMenuItem(option.name, selected = option.id == targetId, onClick = {
                            chosenListId = option.id
                            open = false
                        })
                    }
                }
            }
            if (preview.problem != null) {
                Text(preview.problem!!, style = TaktTheme.type.small, color = TaktTheme.colors.danger)
            }
            if (added.isNotEmpty()) {
                FieldLabel("Added")
                for ((index, title) in added.take(5).withIndex()) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
                        modifier = Modifier.testTag("quick_add_added_$index"),
                    ) {
                        Icon(Icons.Filled.Check, null, tint = TaktTheme.colors.success, modifier = Modifier.size(16.dp))
                        Text(title, style = TaktTheme.type.small, color = TaktTheme.colors.mutedText, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
        }
    }
}

/** One of the row's taps: its icon and what is set, primary when set, a chevron that turns while its panel is open. */
@Composable
private fun PickTag(label: String, on: Boolean, open: Boolean, icon: androidx.compose.ui.graphics.vector.ImageVector, tag: String, onClick: () -> Unit) {
    Tag(
        label,
        selected = on || open,
        color = if (on || open) TaktTheme.colors.primary else TaktTheme.colors.mutedText,
        leading = icon,
        modifier = Modifier
            .heightIn(min = Metrics.touchTarget)
            .testTag(tag)
            .semantics { contentDescription = if (open) "$label, open" else label },
        onClick = onClick,
    )
}

/** A panel of choices under the row: a bordered card on the paper. */
@Composable
private fun PickPanel(content: @Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit) {
    Column(
        Modifier
            .fillMaxWidth()
            .background(TaktTheme.colors.paper, TaktTheme.radii.popoverShape)
            .border(Metrics.hairline, TaktTheme.colors.border, TaktTheme.radii.popoverShape)
            .padding(Metrics.md),
        verticalArrangement = Arrangement.spacedBy(Metrics.sm),
        content = content,
    )
}

@Composable
private fun Choice(label: String, selected: Boolean, mono: Boolean = false, onClick: () -> Unit) {
    Tag(
        label,
        selected = selected,
        mono = mono,
        color = if (selected) TaktTheme.colors.primary else TaktTheme.colors.ink,
        modifier = Modifier.heightIn(min = Metrics.touchTarget),
        onClick = onClick,
    )
}
