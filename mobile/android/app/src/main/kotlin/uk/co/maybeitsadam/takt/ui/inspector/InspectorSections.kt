package uk.co.maybeitsadam.takt.ui.inspector

import android.content.ActivityNotFoundException
import android.content.Intent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId
import uk.co.maybeitsadam.takt.core.MatrixQuadrant
import uk.co.maybeitsadam.takt.core.TaskRequirement
import uk.co.maybeitsadam.takt.core.TaskStatus
import uk.co.maybeitsadam.takt.core.WaitingFollowUp
import uk.co.maybeitsadam.takt.core.WorkspaceDaily
import uk.co.maybeitsadam.takt.core.WorkspaceTask
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorDraft
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorField
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorValues
import uk.co.maybeitsadam.takt.ui.components.Format
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.IconAction
import uk.co.maybeitsadam.takt.ui.components.MonoText
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.lists.WaitingBadge
import uk.co.maybeitsadam.takt.ui.lists.WaitingMarkers
import uk.co.maybeitsadam.takt.ui.components.Tag
import uk.co.maybeitsadam.takt.ui.conditions.TaskConditionsSheet
import uk.co.maybeitsadam.takt.ui.components.TaskCheck
import uk.co.maybeitsadam.takt.ui.components.priorityColor
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.navigation.Tab
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.PIcons

/** The form, in the order you work through a task: what it is, its dates, its plan, where it lives. */
@Composable
internal fun InspectorBody(
    vm: TaskInspectorViewModel,
    draft: TaskEditorDraft,
    around: InspectorSurroundings,
    task: WorkspaceTask,
    onClose: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val zone = remember { ZoneId.systemDefault() }
    val today = remember { LocalDate.now(zone) }
    val values = draft.values
    val problems = remember(draft) { InspectorValidation.problems(draft, zone) }
    fun problem(field: TaskEditorField): String? = problems.firstOrNull { it.field == field }?.message
    val edit: ((TaskEditorValues) -> TaskEditorValues) -> Unit = vm::edit

    Column(modifier) {
        if (draft.isUnavailable) {
            Hint(
                "This task was deleted elsewhere. Your edits are kept here but can't be saved.",
                color = TaktTheme.colors.warning,
                modifier = Modifier.padding(horizontal = Metrics.lg, vertical = Metrics.sm),
            )
        }
        ConflictBanners(vm, draft, around)
        TitleBlock(vm, values, task, around, problem(TaskEditorField.TITLE), onClose)

        WhenSection(values, today, zone, problem(TaskEditorField.START_AT), edit)
        PlanSection(vm, values, task, around, problem(TaskEditorField.ESTIMATE_MINUTES), edit)

        var fullscreenNotes by rememberSaveable(task.id) { mutableStateOf(false) }

        InspectorSection(
            label = "Notes",
            action = {
                IconAction(
                    PIcons.Expand,
                    contentDescription = "Expand notes",
                    tint = TaktTheme.colors.primary,
                    onClick = { fullscreenNotes = true },
                )
            },
        ) {
            ChalkField(
                values.notes,
                onValueChange = { text -> edit { it.copy(notes = text) } },
                placeholder = "Add notes",
                singleLine = false,
                minLines = 4,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                modifier = Modifier.testTag("inspector_notes"),
                contentDescription = "Notes",
            )
        }

        if (fullscreenNotes) {
            FullscreenNotesEditor(
                taskTitle = task.title,
                notes = values.notes,
                onNotesChange = { text -> edit { it.copy(notes = text) } },
                onDismiss = { fullscreenNotes = false },
            )
        }

        RepeatSection(values, zone, edit)
        ConditionsSection(vm, values, around, problem(TaskEditorField.MINIMUM_BLOCK), problem(TaskEditorField.SINGLE_SITTING), edit)
        TodaySection(vm, values, draft, around, edit)
        WaitingSection(vm, task, around, today, zone)
        BlockedBySection(vm, task, around)
        LinksSection(values, edit)
        PlacementSection(vm, around)
        AboutSection(task, around, zone)
        DeleteSection(task, onClose)
    }
}

@Composable
private fun TitleBlock(
    vm: TaskInspectorViewModel,
    values: TaskEditorValues,
    task: WorkspaceTask,
    around: InspectorSurroundings,
    error: String?,
    onClose: () -> Unit,
) {
    val shell = LocalShell.current
    val commands = shell.container.commands
    val colors = TaktTheme.colors
    val focus = LocalFocusManager.current
    Column(
        Modifier.fillMaxWidth().padding(start = Metrics.xs, end = Metrics.lg, top = Metrics.sm, bottom = Metrics.md),
        verticalArrangement = Arrangement.spacedBy(Metrics.sm),
    ) {
        Row(verticalAlignment = Alignment.Top) {
            TaskCheck(task.status, isList = task.isList, label = task.title, onToggle = { commands.toggleComplete(task) })
            BasicTextField(
                value = values.title,
                onValueChange = { text -> vm.edit { it.copy(title = text.replace("\n", " ")) } },
                textStyle = TaktTheme.type.title.copy(color = if (task.status == TaskStatus.OPEN) colors.ink else colors.mutedText),
                cursorBrush = SolidColor(colors.primary),
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { focus.clearFocus() }),
                modifier = Modifier
                    .weight(1f)
                    .padding(top = Metrics.sm + Metrics.xxs)
                    .testTag("inspector_title")
                    .semantics { contentDescription = "Title" },
                decorationBox = { inner ->
                    if (values.title.isEmpty()) Text("Task title", style = TaktTheme.type.title, color = colors.dimText)
                    inner()
                },
            )
        }
        if (error != null) Hint(error, color = colors.danger, modifier = Modifier.padding(start = Metrics.sm))
        FlowRow(
            Modifier.padding(start = Metrics.sm),
            horizontalArrangement = Arrangement.spacedBy(Metrics.xs),
            verticalArrangement = Arrangement.spacedBy(Metrics.xs),
        ) {
            if (around.listName.isNotEmpty()) Tag(around.listName, leading = if (task.isList) PIcons.Outline else PIcons.ListIcon)
            when (task.status) {
                TaskStatus.COMPLETED -> Tag("Done", color = colors.success)
                TaskStatus.CANCELLED -> Tag("Invalidated", color = colors.danger)
                TaskStatus.OPEN -> Unit
            }
            if (around.facts.isPlannedToday) Tag("Today", color = colors.primary, leading = PIcons.Today)
            if (around.facts.daily != null) Tag("Daily", color = colors.success, leading = PIcons.Repeat)
            // Who it waits on and when to chase it, as the iPhone's header and the list rows say.
            val facts = around.facts
            val zone = remember { ZoneId.systemDefault() }
            WaitingMarkers(
                WaitingBadge.of(
                    facts.kanbanColumn, facts.waitingOn, facts.followUpAt, facts.followUpChase,
                    task.status == TaskStatus.OPEN, LocalDate.now(zone), zone,
                ),
                Modifier.align(Alignment.CenterVertically),
            )
        }
        Row(Modifier.padding(start = Metrics.sm), horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            if (!task.isList && task.status == TaskStatus.OPEN) {
                PButton("Start focus", primary = true, icon = PIcons.Focus, modifier = Modifier.weight(1f)) {
                    commands.startFocus(task.id)
                    shell.navigator.openTab(Tab.FOCUS)
                    onClose()
                }
            }
            PButton(
                when (task.status) {
                    TaskStatus.OPEN -> if (task.isList) "Complete list" else "Complete"
                    TaskStatus.COMPLETED -> "Reopen"
                    TaskStatus.CANCELLED -> "Reinstate"
                },
                modifier = Modifier.weight(1f),
            ) { commands.toggleComplete(task) }
        }
    }
    Hairline()
}

@Composable
private fun WhenSection(
    values: TaskEditorValues,
    today: LocalDate,
    zone: ZoneId,
    startError: String?,
    edit: ((TaskEditorValues) -> TaskEditorValues) -> Unit,
) {
    val mode = DueEditing.mode(values)
    InspectorSection("When") {
        FieldLabel("Start")
        ChalkDateField(
            label = "Start",
            date = values.startAt?.let { DateFieldMath.date(it, zone) },
            time = values.startAt?.let { DateFieldMath.time(it, zone) },
            today = today,
            placeholder = "Any time",
            onDate = { date ->
                edit { v ->
                    v.copy(
                        startAt = date?.let {
                            DateFieldMath.combine(it, v.startAt?.let { s -> DateFieldMath.time(s, zone) } ?: LocalTime.of(9, 0), zone)
                        },
                    )
                }
            },
            onTime = { time -> edit { v -> v.copy(startAt = v.startAt?.let { DateFieldMath.withTime(it, time, zone) }) } },
            testTag = "inspector_start",
        )
        if (startError != null) Hint(startError, color = TaktTheme.colors.danger)
        else Hint("It stays out of Next up until then.")
        Spacer(Modifier.size(Metrics.xs))
        FieldLabel("Due")
        ChalkDateField(
            label = "Due",
            date = DueEditing.day(values, zone),
            time = values.dueAt?.let { DateFieldMath.time(it, zone) },
            today = today,
            onDate = { date -> edit { DueEditing.setDay(it, date, zone) } },
            onTime = { time -> edit { DueEditing.setTime(it, time, zone) } },
            testTag = "inspector_due",
        )
        if (mode != DueMode.NONE) {
            SwitchRow(
                "At a set time",
                checked = mode == DueMode.TIME,
                onCheckedChange = { exact -> edit { DueEditing.setExact(it, exact, zone, today) } },
                detail = if (mode == DueMode.TIME) "Due at an exact time" else "Due any time that day, wherever you are",
            )
        }
    }
}

@Composable
private fun PlanSection(
    vm: TaskInspectorViewModel,
    values: TaskEditorValues,
    task: WorkspaceTask,
    around: InspectorSurroundings,
    estimateError: String?,
    edit: ((TaskEditorValues) -> TaskEditorValues) -> Unit,
) {
    val colors = TaktTheme.colors
    InspectorSection("Plan") {
        FieldLabel("Estimate")
        ChalkField(
            values.estimateMinutes,
            onValueChange = { text -> edit { it.copy(estimateMinutes = text) } },
            placeholder = "Minutes",
            style = TaktTheme.type.mono.copy(fontSize = TaktTheme.type.body.fontSize),
            isError = estimateError != null,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal, imeAction = ImeAction.Done),
            trailing = { Text("min", style = TaktTheme.type.small, color = colors.mutedText) },
            modifier = Modifier.testTag("inspector_estimate"),
            contentDescription = "Estimate in minutes",
        )
        if (estimateError != null) Hint(estimateError, color = colors.danger)
        val logged = around.facts.loggedSeconds
        if (logged > 0) {
            val estimate = task.estimateSeconds
            val line = buildString {
                append("Worked ").append(Format.duration(logged))
                if (estimate != null) append(if (logged >= estimate) " · estimate used up" else " · about ${Format.duration(estimate - logged)} left")
            }
            MonoText(line, color = if (estimate != null && logged >= estimate) colors.warning else colors.mutedText)
        }

        // Progress without finishing: time worked away from the timer is
        // written straight away, as placement is, not held for Save.
        FieldLabel("Log time worked")
        var logText by rememberSaveable(task.id) { mutableStateOf("") }
        val logSeconds = Format.parseSpan(logText)
        val log = {
            val seconds = logSeconds ?: 0
            if (seconds > 0) {
                vm.logWork(seconds)
                logText = ""
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            ChalkField(
                logText,
                onValueChange = { logText = it },
                placeholder = "25m or 1h 20m",
                style = TaktTheme.type.mono.copy(fontSize = TaktTheme.type.body.fontSize),
                isError = logSeconds == null,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { log() }),
                modifier = Modifier.weight(1f).testTag("inspector_log"),
                contentDescription = "Time worked",
            )
            PButton("Log", enabled = (logSeconds ?: 0) > 0, onClick = log, modifier = Modifier.testTag("inspector_log_button"))
        }
        Hint(if (logSeconds == null) "Try 25, 25m or 1h 20m." else "Work done without the timer; the task stays open.", color = if (logSeconds == null) colors.danger else colors.dimText)

        FieldLabel("Priority")
        Segmented(
            options = PriorityLabels.order.map { p ->
                Segment(PriorityLabels.label(p), p, tint = if (p == 0) null else priorityColor(p), description = PriorityLabels.description(p))
            },
            selected = values.priority,
            onSelect = { p -> edit { it.copy(priority = p) } },
            mono = true,
            modifier = Modifier.testTag("inspector_priority"),
        )
        // Behind every task with no priority, -1 the very back: a second
        // row, as nine segments do not fit a phone's width.
        Segmented(
            options = PriorityLabels.later.map { p ->
                Segment(if (p == -1) "!-1 last" else PriorityLabels.label(p), p, tint = priorityColor(p), description = PriorityLabels.description(p))
            },
            selected = values.priority,
            onSelect = { p -> edit { it.copy(priority = p) } },
            mono = true,
            modifier = Modifier.testTag("inspector_priority_later"),
        )

        FieldLabel("Tags")
        TagEditor(values.tags) { text -> edit { it.copy(tags = text) } }
    }
}

/** Tags as chips (tap to remove) and a field that adds one on Enter. */
@Composable
private fun TagEditor(raw: String, onChange: (String) -> Unit) {
    val colors = TaktTheme.colors
    val tags = remember(raw) { tagList(raw) }
    var adding by rememberSaveable { mutableStateOf("") }
    fun commit() {
        val added = tagList(adding)
        if (added.isNotEmpty()) onChange(joinTags(tagList(joinTags(tags + added))))
        adding = ""
    }
    if (tags.isNotEmpty()) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(Metrics.xs), verticalArrangement = Arrangement.spacedBy(Metrics.xs)) {
            for (tag in tags) {
                Tag(
                    "#$tag",
                    color = colors.categoricalPurple,
                    leading = Icons.Filled.Close,
                    onClick = { onChange(joinTags(tags - tag)) },
                    modifier = Modifier.semantics { contentDescription = "Remove tag $tag" },
                )
            }
        }
    }
    ChalkField(
        adding,
        onValueChange = { text ->
            if (text.endsWith(",")) {
                adding = text.dropLast(1)
                commit()
            } else {
                adding = text
            }
        },
        placeholder = "Add a tag",
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = { commit() }),
        leading = { Icon(PIcons.Tag, null, tint = colors.mutedText, modifier = Modifier.size(18.dp)) },
        trailing = if (adding.isNotBlank()) {
            { StepButton(Icons.Filled.Add, "Add tag ${adding.trim()}", onClick = ::commit) }
        } else {
            null
        },
        modifier = Modifier.testTag("inspector_tag_field"),
        contentDescription = "Add a tag",
    )
}

@Composable
private fun RepeatSection(values: TaskEditorValues, zone: ZoneId, edit: ((TaskEditorValues) -> TaskEditorValues) -> Unit) {
    val colors = TaktTheme.colors
    val presets = remember(values.dueAt, values.dueDate) { RecurrencePresets.presets(DueEditing.day(values, zone)) }
    val selected = remember(values.recurrenceRule, presets) { RecurrencePresets.selected(values.recurrenceRule, presets) }
    val description = remember(values.recurrenceRule) { RecurrencePresets.describe(values.recurrenceRule) }
    InspectorSection("Repeat") {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(Metrics.xs), verticalArrangement = Arrangement.spacedBy(Metrics.xs)) {
            Tag("Never", selected = values.recurrenceRule.isBlank(), onClick = { edit { it.copy(recurrenceRule = "") } })
            for (preset in presets) {
                Tag(
                    preset.label,
                    color = colors.primary,
                    selected = preset == selected,
                    onClick = { edit { it.copy(recurrenceRule = preset.rule) } },
                    modifier = Modifier.semantics { contentDescription = "Repeat ${preset.label.lowercase()}" },
                )
            }
        }
        ChalkField(
            values.recurrenceRule,
            onValueChange = { text -> edit { it.copy(recurrenceRule = text) } },
            placeholder = "every monday",
            style = TaktTheme.type.mono.copy(fontSize = TaktTheme.type.body.fontSize),
            isError = !description.valid,
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, imeAction = ImeAction.Done),
            leading = { Icon(PIcons.Repeat, null, tint = colors.mutedText, modifier = Modifier.size(18.dp)) },
            modifier = Modifier.testTag("inspector_repeat"),
            contentDescription = "Repeat rule",
        )
        Hint(description.text, color = if (description.valid) colors.mutedText else colors.warning)
    }
}

@Composable
private fun ConditionsSection(
    vm: TaskInspectorViewModel,
    values: TaskEditorValues,
    around: InspectorSurroundings,
    minimumError: String?,
    sittingError: String?,
    edit: ((TaskEditorValues) -> TaskEditorValues) -> Unit,
) {
    val colors = TaktTheme.colors
    val groups = values.requirementGroups.orEmpty()
    InspectorSection("Conditions", detail = if (groups.isEmpty()) null else "${groups.size} needed") {
        if (groups.isEmpty()) Hint("Anytime, anywhere. Add a condition to hold it back until you're somewhere or have something.")
        groups.forEachIndexed { index, group ->
            var menuOpen by remember { mutableStateOf(false) }
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(Metrics.xs),
                verticalArrangement = Arrangement.spacedBy(Metrics.xs),
                itemVerticalAlignment = Alignment.CenterVertically,
            ) {
                Text(if (index == 0) "Needs" else "and", style = TaktTheme.type.small, color = colors.mutedText)
                group.forEachIndexed { position, id ->
                    if (position > 0) Text("or", style = TaktTheme.type.small, color = colors.mutedText)
                    val name = TaskRequirement.label(id, around::conditionName)
                    Tag(
                        name,
                        color = colors.categoricalPurple,
                        leading = Icons.Filled.Close,
                        onClick = { edit { it.copy(requirementGroups = RequirementGroups.remove(it.requirementGroups, index, id)) } },
                        modifier = Modifier.semantics { contentDescription = "Remove condition $name" },
                    )
                }
                val alternatives = RequirementGroups.candidatesForGroup(group, around.conditions, { it.id }, { it.isArchived })
                if (alternatives.isNotEmpty()) {
                    Box {
                        Tag("Or…", color = colors.primary, onClick = { menuOpen = true })
                        ChalkMenu(expanded = menuOpen, onDismiss = { menuOpen = false }) {
                            for (condition in alternatives) {
                                ChalkMenuItem(condition.name, onClick = {
                                    menuOpen = false
                                    edit { it.copy(requirementGroups = RequirementGroups.addAlternative(it.requirementGroups, index, condition.id)) }
                                })
                            }
                        }
                    }
                }
                Tag(
                    "Remove group",
                    color = colors.mutedText,
                    onClick = { edit { it.copy(requirementGroups = RequirementGroups.removeGroup(it.requirementGroups, index)) } },
                )
            }
        }
        // Every condition in one sheet, ticked under its category, searched,
        // and made there when it does not exist yet: the Mac's `cc` picker.
        var picking by rememberSaveable { mutableStateOf(false) }
        PButton(
            if (groups.isEmpty()) "Choose conditions" else "Change conditions",
            icon = Icons.Filled.Add,
            modifier = Modifier.fillMaxWidth().testTag("inspector_conditions"),
            onClick = { picking = true },
        )
        if (picking) {
            TaskConditionsSheet(
                groups = values.requirementGroups,
                onGroupsChange = { change -> edit { it.copy(requirementGroups = change(it.requirementGroups)) } },
                onDismiss = { picking = false },
            )
        }
        Hint("Every group must be met; any condition within a group will do.")

        FieldLabel("Minimum useful block")
        ChalkField(
            values.minimumBlockMinutes ?: "",
            onValueChange = { text -> edit { it.copy(minimumBlockMinutes = text.ifEmpty { null }) } },
            placeholder = "Minutes",
            style = TaktTheme.type.mono.copy(fontSize = TaktTheme.type.body.fontSize),
            isError = minimumError != null,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal, imeAction = ImeAction.Done),
            trailing = { Text("min", style = TaktTheme.type.small, color = colors.mutedText) },
            contentDescription = "Minimum useful block in minutes",
        )
        if (minimumError != null) Hint(minimumError, color = colors.danger)
        SwitchRow(
            "Finish in one sitting",
            checked = values.requiresSingleSitting == true,
            onCheckedChange = { on -> edit { it.copy(requiresSingleSitting = if (on) true else null) } },
            detail = "Only offered when there's time for the whole estimate",
        )
        if (sittingError != null) Hint(sittingError, color = colors.danger)
        PButton("Apply planning to subtasks", icon = PIcons.Indent, onClick = vm::applyPlanningToSubtasks)
        Hint("Copies conditions, start and block rules (not due dates) to every subtask. Saves first.")
    }
}

@Composable
private fun TodaySection(
    vm: TaskInspectorViewModel,
    values: TaskEditorValues,
    draft: TaskEditorDraft,
    around: InspectorSurroundings,
    edit: ((TaskEditorValues) -> TaskEditorValues) -> Unit,
) {
    val daily = around.facts.daily
    InspectorSection("Today and dailies") {
        SwitchRow(
            "Planned for today",
            checked = around.facts.isPlannedToday,
            onCheckedChange = vm::setPlannedToday,
            detail = "Saves straight away, separate from Save",
            modifier = Modifier.testTag("inspector_planned"),
        )
        SwitchRow(
            "Daily progress",
            checked = values.dailyProgress,
            onCheckedChange = { on -> edit { it.copy(dailyProgress = on) } },
            detail = "Show it in Dailies without completing the task",
            modifier = Modifier.testTag("inspector_daily"),
        )
        when {
            values.dailyProgress && daily != null -> DailyScheduleEditor(vm, daily)
            values.dailyProgress && !draft.baseline.dailyProgress -> Hint("Save to make it a daily, then set its schedule here.")
            !values.dailyProgress && draft.baseline.dailyProgress -> Hint("Save to stop it being a daily. Its history is kept.")
        }
    }
}

/**
 * Waiting on: who or what the task waits on, and when to chase it. Written
 * straight away by its own button, like placement, and filing the task in
 * Waiting on; at the follow-up time a follow-up task lands in Today.
 */
@Composable
private fun WaitingSection(
    vm: TaskInspectorViewModel,
    task: WorkspaceTask,
    around: InspectorSurroundings,
    today: LocalDate,
    zone: ZoneId,
) {
    val facts = around.facts
    val savedTag = facts.waitingOn.orEmpty()
    val savedAt = facts.followUpAt
    var tag by rememberSaveable(task.id, savedTag) { mutableStateOf(savedTag) }
    var day by rememberSaveable(task.id, savedAt) { mutableStateOf(savedAt?.let { DateFieldMath.date(it, zone) }) }
    var time by rememberSaveable(task.id, savedAt) {
        mutableStateOf(savedAt?.let { DateFieldMath.time(it, zone) } ?: LocalTime.of(9, 0))
    }
    val savedEvery = facts.followUpEveryDays
    var every by rememberSaveable(task.id, savedEvery) { mutableStateOf(savedEvery) }
    val followUpAt = day?.let { DateFieldMath.combine(it, time, zone) }
    // A repeat needs a time; without one it is dropped, as the core drops it.
    val everyDays = every.takeIf { followUpAt != null }
    val isWaiting = facts.kanbanColumn == WaitingFollowUp.WAITING_COLUMN_ID
    val changed = WaitingFollowUp.normalizedTag(tag) != facts.waitingOn || followUpAt != savedAt ||
        everyDays != savedEvery
    InspectorSection(
        "Waiting on",
        detail = if (isWaiting) savedAt?.let { WaitingFollowUp.label(it, today, zone, facts.followUpChase) } else null,
    ) {
        ChalkField(
            tag,
            onValueChange = { tag = it.take(WaitingFollowUp.MAXIMUM_TAG_LENGTH) },
            placeholder = "Who or what — Sam, Legal, the invoice",
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words, imeAction = ImeAction.Done),
            modifier = Modifier.testTag("inspector_waiting_on"),
            contentDescription = "Waiting on",
        )
        FieldLabel("Follow up")
        // The iPhone's quick choices: a morning one, three or seven days on.
        Row(horizontalArrangement = Arrangement.spacedBy(Metrics.xs)) {
            for ((title, days) in FOLLOW_UP_CHOICES) {
                PButton(title, contentPadding = PaddingValues(horizontal = Metrics.sm), onClick = {
                    day = today.plusDays(days)
                    time = LocalTime.of(9, 0)
                })
            }
        }
        ChalkDateField(
            label = "Follow up",
            date = day,
            time = time,
            today = today,
            placeholder = "No follow-up",
            onDate = { day = it },
            onTime = { time = it },
            testTag = "inspector_follow_up",
        )
        if (followUpAt != null) {
            FollowUpRepeat(every, onChange = { every = it })
        }
        Row(horizontalArrangement = Arrangement.spacedBy(Metrics.sm), verticalAlignment = Alignment.CenterVertically) {
            if (facts.waitingOn != null || savedAt != null) {
                PButton("Clear", onClick = { vm.setWaiting(null, null, null) }, modifier = Modifier.testTag("inspector_waiting_clear"))
            }
            Spacer(Modifier.weight(1f))
            PButton(
                if (isWaiting) "Save" else "Move to Waiting on",
                primary = true,
                enabled = changed || !isWaiting,
                onClick = { vm.setWaiting(tag, followUpAt, everyDays) },
                modifier = Modifier.testTag("inspector_waiting_save"),
            )
        }
        Hint(
            if (followUpAt == null) "Files it in Waiting on. Pick a time to have a follow-up land in Today."
            else if (everyDays == null) "If it is still waiting then, “${WaitingFollowUp.title(task.title, tag)}” lands in Today."
            else "If it is still waiting then, “${WaitingFollowUp.title(task.title, tag)}” lands in Today; " +
                "once that is done, the next lands ${WaitingFollowUp.repeatText(everyDays).lowercase()} later, numbered #2, #3.",
        )
    }
}

/** The follow-up's quick choices, 9:00 that many days on, as the iPhone's `WaitingForm.quickChoices`. */
private val FOLLOW_UP_CHOICES = listOf("Tomorrow 9:00" to 1L, "In 3 days" to 3L, "Next week" to 7L)

/** Once, or every so many days: the follow-up's repeat, saved with the rest of the section. */
@Composable
private fun FollowUpRepeat(every: Int?, onChange: (Int?) -> Unit) {
    val colors = TaktTheme.colors
    FieldLabel("Repeat")
    Segmented(
        options = listOf(Segment("Once", false), Segment("Every few days", true)),
        selected = every != null,
        onSelect = { repeats -> onChange(if (repeats) every ?: 3 else null) },
    )
    if (every != null) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            StepButton(InspectorIcons.Minus, "Fewer days between follow-ups", enabled = every > WaitingFollowUp.EVERY_DAYS_RANGE.first) {
                onChange(WaitingFollowUp.stepEveryDays(every, -1))
            }
            Text(
                WaitingFollowUp.repeatText(every),
                style = TaktTheme.type.mono.copy(fontSize = TaktTheme.type.body.fontSize),
                color = colors.ink,
                modifier = Modifier.weight(1f).testTag("inspector_follow_up_every"),
            )
            StepButton(Icons.Filled.Add, "More days between follow-ups", enabled = every < WaitingFollowUp.EVERY_DAYS_RANGE.last) {
                onChange(WaitingFollowUp.stepEveryDays(every, 1))
            }
        }
    }
}

@Composable
private fun DailyScheduleEditor(vm: TaskInspectorViewModel, daily: WorkspaceDaily) {
    val colors = TaktTheme.colors
    val interval = daily.intervalDays
    Column(verticalArrangement = Arrangement.spacedBy(Metrics.sm)) {
        Segmented(
            options = listOf(Segment("On set days", false), Segment("Every few days", true)),
            selected = interval != null,
            onSelect = { rotating ->
                if (rotating) vm.setDailyInterval(daily.id, interval ?: 2) else vm.setDailyWeekdays(daily.id, daily.activeWeekdays)
            },
        )
        if (interval == null) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(Metrics.xs), verticalArrangement = Arrangement.spacedBy(Metrics.xs)) {
                for ((weekday, name) in DailySchedule.weekdayChips) {
                    val on = weekday in daily.activeWeekdays
                    Tag(
                        name,
                        color = if (on) colors.primary else colors.mutedText,
                        selected = on,
                        onClick = { vm.setDailyWeekdays(daily.id, DailySchedule.toggle(daily.activeWeekdays, weekday)) },
                        modifier = Modifier.semantics { contentDescription = "$name, ${if (on) "on" else "off"}" },
                    )
                }
            }
        } else {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
                StepButton(InspectorIcons.Minus, "Fewer days between", enabled = interval > 1) {
                    vm.setDailyInterval(daily.id, DailySchedule.stepInterval(interval, -1))
                }
                Text(
                    if (interval == 1) "Every day" else "Every $interval days",
                    style = TaktTheme.type.mono.copy(fontSize = TaktTheme.type.body.fontSize),
                    color = colors.ink,
                    modifier = Modifier.weight(1f),
                )
                StepButton(Icons.Filled.Add, "More days between") {
                    vm.setDailyInterval(daily.id, DailySchedule.stepInterval(interval, 1))
                }
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            FieldLabel("Target", Modifier.width(56.dp))
            StepButton(InspectorIcons.Minus, "Shorter target", enabled = daily.targetSeconds != null) {
                vm.setDailyTarget(daily.id, DailySchedule.stepTarget(daily.targetSeconds, -1))
            }
            Text(
                daily.targetSeconds?.let { Format.duration(it) } ?: "None",
                style = TaktTheme.type.mono.copy(fontSize = TaktTheme.type.body.fontSize),
                color = if (daily.targetSeconds == null) colors.dimText else colors.ink,
                modifier = Modifier.weight(1f),
            )
            StepButton(Icons.Filled.Add, "Longer target") {
                vm.setDailyTarget(daily.id, DailySchedule.stepTarget(daily.targetSeconds, 1))
            }
        }
        Hint("The schedule saves straight away.")
    }
}

@Composable
private fun LinksSection(values: TaskEditorValues, edit: ((TaskEditorValues) -> TaskEditorValues) -> Unit) {
    val context = LocalContext.current
    val undo = LocalShell.current.container.undo
    val colors = TaktTheme.colors
    val links = remember(values.links) { linkLines(values.links) }
    InspectorSection("Links") {
        ChalkField(
            values.links,
            onValueChange = { text -> edit { it.copy(links = text) } },
            placeholder = "Paste a Drive, Obsidian or web address, one per line",
            singleLine = false,
            minLines = 2,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
            modifier = Modifier.testTag("inspector_links"),
            contentDescription = "Links, one per line",
        )
        for (link in links) {
            // What the link is, read by the core from the address (docs/links.md):
            // a Drive file's kind, an Obsidian note's name, a page's host.
            val described = remember(link) { runCatching { uniffi.takt_core.linkDescribe(link, null, null) }.getOrNull() }
            val host = described?.title ?: runCatching { link.toUri().host }.getOrNull() ?: link
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = Metrics.touchTarget)
                    .clip(Metrics.control)
                    .clickable(role = Role.Button, onClickLabel = "Open link") {
                        try {
                            context.startActivity(Intent(Intent.ACTION_VIEW, link.toUri()).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                        } catch (_: ActivityNotFoundException) {
                            undo.say("No app can open $link")
                        } catch (_: SecurityException) {
                            undo.say("That link can't be opened")
                        }
                    }
                    .semantics { contentDescription = "Open $link" },
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
            ) {
                Icon(InspectorIcons.OpenLink, null, tint = colors.primary, modifier = Modifier.size(18.dp))
                Text(host, style = TaktTheme.type.body.copy(textDecoration = TextDecoration.Underline), color = colors.ink, maxLines = 1)
                described?.subtitle?.takeIf { it != host }?.let {
                    Text(it, style = TaktTheme.type.small, color = colors.mutedText, maxLines = 1)
                }
            }
        }
    }
}

@Composable
private fun PlacementSection(vm: TaskInspectorViewModel, around: InspectorSurroundings) {
    val colors = TaktTheme.colors
    val quadrant = MatrixPicker.quadrant(around.facts.matrix)
    val column = around.facts.kanbanColumn
    InspectorSection("Placement") {
        FieldLabel("Matrix")
        FlowRow(horizontalArrangement = Arrangement.spacedBy(Metrics.xs), verticalArrangement = Arrangement.spacedBy(Metrics.xs)) {
            Tag("Unplaced", selected = quadrant == null, onClick = { vm.setMatrix(null) })
            for (q in MatrixQuadrant.entries) {
                Tag(
                    q.title,
                    color = quadrantColor(q),
                    selected = q == quadrant,
                    onClick = { vm.setMatrix(q) },
                    modifier = Modifier.semantics { contentDescription = "Matrix ${q.title}${if (q == quadrant) ", selected" else ""}" },
                )
            }
        }
        FieldLabel("Board column")
        FlowRow(horizontalArrangement = Arrangement.spacedBy(Metrics.xs), verticalArrangement = Arrangement.spacedBy(Metrics.xs)) {
            Tag("None", selected = column == null, onClick = { vm.setColumn(null) })
            for (option in around.boardColumns) {
                Tag(
                    option.title,
                    color = colors.primary,
                    selected = option.id == column,
                    onClick = { vm.setColumn(option.id) },
                    modifier = Modifier.semantics { contentDescription = "Column ${option.title}${if (option.id == column) ", selected" else ""}" },
                )
            }
            if (column != null && around.boardColumns.none { it.id == column }) Tag(column, color = colors.primary, selected = true)
        }
        Hint("Placement saves straight away.")
    }
}

@Composable
private fun quadrantColor(quadrant: MatrixQuadrant) = when (quadrant) {
    MatrixQuadrant.DO_NOW -> TaktTheme.colors.danger
    MatrixQuadrant.SCHEDULE -> TaktTheme.colors.primary
    MatrixQuadrant.DELEGATE -> TaktTheme.colors.warning
    MatrixQuadrant.ELIMINATE -> TaktTheme.colors.mutedText
}

@Composable
private fun AboutSection(task: WorkspaceTask, around: InspectorSurroundings, zone: ZoneId) {
    InspectorSection("About") {
        FactRow("List", around.listName.ifEmpty { "—" })
        FactRow("Created", "${Format.day(task.createdAt, zone)} ${Format.time(task.createdAt, zone)}", mono = true)
        task.completedAt?.let { FactRow(if (task.status == TaskStatus.CANCELLED) "Invalidated" else "Completed", "${Format.day(it, zone)} ${Format.time(it, zone)}", mono = true) }
        val facts = around.facts
        FactRow(
            "Logged",
            if (facts.loggedSeconds == 0) "Nothing yet"
            else "${Format.duration(facts.loggedSeconds)} in ${facts.workBlockCount} ${if (facts.workBlockCount == 1) "block" else "blocks"}",
            mono = facts.loggedSeconds > 0,
        )
    }
}

@Composable
private fun FactRow(label: String, value: String, mono: Boolean = false) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, style = TaktTheme.type.small, color = TaktTheme.colors.mutedText, modifier = Modifier.width(96.dp))
        Text(
            value,
            style = if (mono) TaktTheme.type.mono else TaktTheme.type.body,
            color = TaktTheme.colors.ink,
            modifier = Modifier.weight(1f),
            maxLines = 2,
        )
    }
}

@Composable
private fun DeleteSection(task: WorkspaceTask, onClose: () -> Unit) {
    val commands = LocalShell.current.container.commands
    var confirming by rememberSaveable(task.id) { mutableStateOf(false) }
    Column(
        Modifier.fillMaxWidth().padding(horizontal = Metrics.lg, vertical = Metrics.lg),
        verticalArrangement = Arrangement.spacedBy(Metrics.sm),
    ) {
        if (!confirming) {
            PButton(if (task.isList) "Delete list" else "Delete task", destructive = true, icon = Icons.Filled.Delete, modifier = Modifier.fillMaxWidth()) {
                confirming = true
            }
        } else {
            Text(
                "Delete “${task.title}” and everything under it? You can undo this.",
                style = TaktTheme.type.body,
                color = TaktTheme.colors.ink,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
                PButton("Cancel", modifier = Modifier.weight(1f)) { confirming = false }
                PButton("Delete", primary = true, destructive = true, modifier = Modifier.weight(1f).testTag("inspector_delete_confirm")) {
                    commands.delete(task.id)
                    onClose()
                }
            }
        }
    }
}
