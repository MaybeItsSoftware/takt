package uk.co.maybeitsadam.takt.ui.conditions

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.LocationOn
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
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
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import uk.co.maybeitsadam.takt.data.workspace.ConditionGroups
import uk.co.maybeitsadam.takt.ui.SheetHandle
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.MonoText
import uk.co.maybeitsadam.takt.ui.components.Disclosure
import androidx.compose.runtime.key
import uk.co.maybeitsadam.takt.ui.components.SectionHeader
import uk.co.maybeitsadam.takt.ui.components.Tag
import uk.co.maybeitsadam.takt.ui.inspector.ChalkField
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme

/** The one [ConditionsViewModel] a screen's bar and its sheets share. */
@Composable
fun conditionsViewModel(): ConditionsViewModel {
    val container = LocalShell.current.container
    return viewModel(key = "conditions") { ConditionsViewModel(container) }
}

/**
 * Where you are, from Today: a drop-down whose line says what holds now —
 * "Home · Laptop", or that nothing does — and which opens below into the
 * conditions to tick, in place. Searching every condition, or making one,
 * is a row at its foot that opens the sheet. Absent when no open task needs
 * a condition and none is on, because then the answer changes nothing.
 */
@Composable
fun NowConditionsBar(modifier: Modifier = Modifier) {
    val vm = conditionsViewModel()
    val state by vm.state.collectAsStateWithLifecycle()
    var sheet by rememberSaveable { mutableStateOf(false) }
    var devices by rememberSaveable { mutableStateOf(false) }
    if (sheet) NowConditionsSheet(onDismiss = { sheet = false })
    if (devices) ConditionDevicesSheet(onDismiss = { devices = false })
    if (!state.isLoaded) return
    val colors = TaktTheme.colors
    // No conditions yet: still a row, so there is somewhere to start.
    if (state.available.isEmpty()) {
        Column(modifier) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = Metrics.touchTarget)
                    .clickable(onClickLabel = "Set up conditions") { sheet = true }
                    .padding(horizontal = Metrics.lg)
                    .testTag("today_conditions"),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
            ) {
                Icon(Icons.Filled.LocationOn, null, tint = colors.mutedText, modifier = Modifier.size(18.dp))
                Text("Say where you are", style = TaktTheme.type.body, color = colors.mutedText, modifier = Modifier.weight(1f))
                Tag("Add places and tools", color = colors.primary, onClick = { sheet = true })
            }
            Hairline()
        }
        return
    }
    val summary = ConditionPicker.nowSummary(state.conditions, state.held)
    val list = remember(state) {
        ConditionPicker.list(
            offered = state.available,
            searchable = state.conditions.filter { !it.isArchived || it.id in state.selected },
            all = state.conditions,
            query = "",
            isOn = { it in state.held },
            exclusiveRadio = true,
            devicesFixed = true,
        )
    }
    Column(modifier) {
        Disclosure(
            key = "today.conditions",
            tag = "today_conditions",
            header = {
                Icon(Icons.Filled.LocationOn, null, tint = if (summary != null) colors.primary else colors.mutedText, modifier = Modifier.size(18.dp))
                Text(
                    summary ?: "Say where you are",
                    style = TaktTheme.type.body,
                    color = if (summary != null) colors.ink else colors.mutedText,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f).semantics { contentDescription = "Where you are: ${summary ?: "nothing said"}" },
                )
                if (state.isStale) Tag("Still true?", color = colors.warning)
            },
        ) {
            Hairline(color = colors.borderMuted)
            ConditionRowsInline(
                list,
                detail = { row -> state.note(row.condition.id) },
                count = { row -> state.usage[row.condition.id] },
                onToggle = { row -> vm.toggleNow(row.condition) },
            )
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Metrics.lg, vertical = Metrics.sm),
                horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Tag("Search or add…", color = colors.primary, onClick = { sheet = true }, modifier = Modifier.testTag("now_conditions_more"))
                Tag("Devices…", color = colors.mutedText, onClick = { devices = true }, modifier = Modifier.testTag("now_conditions_devices"))
                if (state.selected.isNotEmpty() || state.off.isNotEmpty()) {
                    Tag("Clear", color = colors.mutedText, onClick = vm::clearNow, modifier = Modifier.testTag("now_conditions_clear_inline"))
                }
            }
        }
        Hairline()
    }
}

/**
 * A picker's rows laid straight into the page rather than a lazy list, for
 * the drop-downs that open in place: a heading per category, a whole-width
 * toggle per condition.
 */
@Composable
internal fun ConditionRowsInline(
    list: ConditionPickerList,
    onToggle: (ConditionPickerRow) -> Unit,
    detail: (ConditionPickerRow) -> String? = { null },
    count: (ConditionPickerRow) -> Int? = { null },
    label: (ConditionPickerRow) -> String = { it.condition.name },
    action: (@Composable (ConditionPickerRow) -> Unit)? = null,
) {
    Column(Modifier.fillMaxWidth()) {
        for (section in list.sections) {
            if (section.title != null) SectionHeader(section.title)
            for (row in section.rows) {
                key(row.condition.id) {
                    ConditionRow(row, label(row), detail(row), count(row), action, onToggle = { onToggle(row) })
                }
            }
        }
    }
}

/**
 * Say what holds now: every condition an open task needs, under its
 * category, one tap each. Location is one place at a time, so its rows are
 * radios. What a setting provides is ticked with it and says so ("from
 * Home"); unticking one is an override, "off for now", until the setting
 * changes. Search reaches every condition, and a name nothing is called yet
 * makes one and turns it on.
 */
@Composable
fun NowConditionsSheet(onDismiss: () -> Unit) {
    val vm = conditionsViewModel()
    val state by vm.state.collectAsStateWithLifecycle()
    var query by rememberSaveable { mutableStateOf("") }
    val list = remember(state, query) {
        ConditionPicker.list(
            offered = state.available,
            searchable = state.conditions.filter { !it.isArchived || it.id in state.selected },
            all = state.conditions,
            query = query,
            isOn = { it in state.held },
            exclusiveRadio = true,
            devicesFixed = true,
        )
    }
    val colors = TaktTheme.colors
    ConditionPickerSheet(
        title = "Where you are",
        tag = "now_conditions_sheet",
        query = query,
        onQueryChange = { query = it },
        offeredCount = state.available.sumOf { it.conditions.size },
        list = list,
        emptyText = if (state.live.isEmpty()) {
            "No conditions yet. Type one — Home, Campus, Laptop — to make it."
        } else {
            "No open task needs a condition. Search to turn one on anyway."
        },
        trailing = {
            if (state.selected.isNotEmpty() || state.off.isNotEmpty()) {
                Tag("Clear", color = colors.primary, onClick = vm::clearNow, modifier = Modifier.testTag("now_conditions_clear"))
            }
        },
        notice = {
            if (state.isStale) {
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = Metrics.lg),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
                ) {
                    Icon(Icons.Filled.Warning, null, tint = colors.warning, modifier = Modifier.size(16.dp))
                    Text("Set on an earlier day — still true?", style = TaktTheme.type.small, color = colors.mutedText)
                }
            }
            // Read-only here: how each day starts is set on the Mac or with `takt ws day-start`.
            state.dayStartSummary?.let {
                Text(
                    it,
                    style = TaktTheme.type.small,
                    color = colors.mutedText,
                    modifier = Modifier.fillMaxWidth().padding(horizontal = Metrics.lg),
                )
            }
        },
        detail = { row -> state.note(row.condition.id) },
        count = { row -> state.usage[row.condition.id] },
        onToggle = { row -> vm.toggleNow(row.condition) },
        onCreate = { name ->
            query = ""
            vm.createCondition(name) { vm.toggleNow(it) }
        },
        onDismiss = onDismiss,
    )
}

/**
 * The conditions a task needs, ticked: the Mac's `cc` picker. Every live
 * condition under its category — and any archived one the task still needs,
 * so it can be ticked off. A tick is "and needs"; a second place joins the
 * first as "or", since you are in one place at a time. A name nothing is
 * called yet makes the condition and requires it.
 *
 * [groups] are the requirements as the task holds them now; [onGroupsChange]
 * is handed a change to apply to them.
 */
@Composable
fun TaskConditionsSheet(
    groups: List<List<String>>?,
    onGroupsChange: ((List<List<String>>?) -> List<List<String>>?) -> Unit,
    onDismiss: () -> Unit,
    title: String = "Needs",
    tag: String = "task_conditions_sheet",
) {
    val vm = conditionsViewModel()
    val state by vm.state.collectAsStateWithLifecycle()
    var query by rememberSaveable { mutableStateOf("") }
    val offeredConditions = remember(state.conditions, groups) {
        state.conditions.filter { !it.isArchived || RequirementToggle.requires(it.id, groups) }
    }
    val list = remember(offeredConditions, state.conditions, groups, query) {
        ConditionPicker.list(
            offered = ConditionGroups.all(offeredConditions),
            searchable = offeredConditions,
            all = state.conditions,
            query = query,
            isOn = { RequirementToggle.requires(it, groups) },
            exclusiveRadio = false,
        )
    }
    fun toggle(id: String) {
        val places = state.conditions.filter { it.isLocation }.map { it.id }.toSet()
        onGroupsChange { current -> RequirementToggle.toggling(id, current, places) }
    }
    fun negated(id: String) = RequirementToggle.requirement(id, groups)?.isNegated == true
    ConditionPickerSheet(
        title = title,
        subtitle = ConditionPicker.requirementSummary(groups, state::name),
        tag = tag,
        query = query,
        onQueryChange = { query = it },
        offeredCount = offeredConditions.size,
        list = list,
        emptyText = "No conditions yet. Type one — Home, Campus, Floor space — to make it and require it.",
        detail = { row ->
            listOfNotNull(
                ConditionPicker.alternatives(row.condition.id, groups, state::name)?.let { "or $it" },
                if (row.condition.isArchived) "Archived" else null,
            ).joinToString(" · ").ifEmpty { null }
        },
        count = { row -> state.usage[row.condition.id] },
        label = { row -> if (negated(row.condition.id)) "not ${row.condition.name}" else row.condition.name },
        // "Not": the task waits while the condition holds, rather than until.
        action = { row ->
            val on = negated(row.condition.id)
            Tag(
                "Not",
                color = if (on) TaktTheme.colors.primary else TaktTheme.colors.mutedText,
                selected = on,
                onClick = { onGroupsChange { current -> RequirementToggle.negating(row.condition.id, current) } },
                modifier = Modifier
                    .testTag("condition_not_${row.condition.name}")
                    .semantics { contentDescription = if (on) "Needed not to hold: tap to remove" else "Need it not to hold" },
            )
        },
        onToggle = { row -> toggle(row.condition.id) },
        onCreate = { name ->
            query = ""
            vm.createCondition(name) { made -> toggle(made.id) }
        },
        onDismiss = onDismiss,
    )
}

/**
 * The conditions a task needs, as ticks laid into the inspector's drop-down:
 * the same rows, toggles and "Not" as [TaskConditionsSheet], without the
 * search, which stays in the sheet.
 */
@Composable
fun TaskConditionsInline(
    groups: List<List<String>>?,
    onGroupsChange: ((List<List<String>>?) -> List<List<String>>?) -> Unit,
) {
    val vm = conditionsViewModel()
    val state by vm.state.collectAsStateWithLifecycle()
    val offeredConditions = remember(state.conditions, groups) {
        state.conditions.filter { !it.isArchived || RequirementToggle.requires(it.id, groups) }
    }
    val list = remember(offeredConditions, state.conditions, groups) {
        ConditionPicker.list(
            offered = ConditionGroups.all(offeredConditions),
            searchable = offeredConditions,
            all = state.conditions,
            query = "",
            isOn = { RequirementToggle.requires(it, groups) },
            exclusiveRadio = false,
        )
    }
    fun negated(id: String) = RequirementToggle.requirement(id, groups)?.isNegated == true
    ConditionRowsInline(
        list,
        onToggle = { row ->
            val places = state.conditions.filter { it.isLocation }.map { it.id }.toSet()
            onGroupsChange { current -> RequirementToggle.toggling(row.condition.id, current, places) }
        },
        detail = { row -> ConditionPicker.alternatives(row.condition.id, groups, state::name)?.let { "or $it" } },
        label = { row -> if (negated(row.condition.id)) "not ${row.condition.name}" else row.condition.name },
        action = { row ->
            val on = negated(row.condition.id)
            Tag(
                "Not",
                color = if (on) TaktTheme.colors.primary else TaktTheme.colors.mutedText,
                selected = on,
                onClick = { onGroupsChange { current -> RequirementToggle.negating(row.condition.id, current) } },
                modifier = Modifier.semantics { contentDescription = if (on) "Needed not to hold: tap to remove" else "Need it not to hold" },
            )
        },
    )
}

/**
 * The sheet both pickers are: a title, a search field that is also how a
 * condition is made, and the rows under their headings — each a whole-width
 * 48dp toggle.
 */
@Composable
internal fun ConditionPickerSheet(
    title: String,
    tag: String,
    query: String,
    onQueryChange: (String) -> Unit,
    offeredCount: Int,
    list: ConditionPickerList,
    emptyText: String,
    onToggle: (ConditionPickerRow) -> Unit,
    onCreate: (String) -> Unit,
    onDismiss: () -> Unit,
    subtitle: String? = null,
    trailing: @Composable () -> Unit = {},
    notice: @Composable () -> Unit = {},
    detail: (ConditionPickerRow) -> String? = { null },
    count: (ConditionPickerRow) -> Int? = { null },
    label: (ConditionPickerRow) -> String = { it.condition.name },
    action: (@Composable (ConditionPickerRow) -> Unit)? = null,
) {
    val colors = TaktTheme.colors
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = offeredCount >= ConditionPicker.SEARCH_THRESHOLD)
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = TaktTheme.colors.altRow,
        tonalElevation = 0.dp,
        shape = Metrics.sheetTop,
        dragHandle = { SheetHandle() },
        modifier = Modifier.testTag(tag),
    ) {
        Column(
            Modifier.fillMaxWidth().imePadding().windowInsetsPadding(WindowInsets.navigationBars),
            verticalArrangement = Arrangement.spacedBy(Metrics.sm),
        ) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Metrics.lg),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
            ) {
                Column(Modifier.weight(1f)) {
                    Text(title, style = TaktTheme.type.title, color = colors.ink)
                    if (subtitle != null) {
                        Text(subtitle, style = TaktTheme.type.small, color = colors.mutedText, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    }
                }
                trailing()
                Tag("Done", color = colors.primary, selected = true, onClick = onDismiss, modifier = Modifier.testTag("${tag}_done"))
            }
            notice()
            Box(Modifier.padding(horizontal = Metrics.lg)) {
                ChalkField(
                    query,
                    onValueChange = onQueryChange,
                    placeholder = if (offeredCount >= ConditionPicker.SEARCH_THRESHOLD) "Search or add a condition" else "Add a condition",
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = {
                        // Return takes the best match, or makes the name typed.
                        val first = list.sections.firstOrNull()?.rows?.firstOrNull()
                        when {
                            first != null && list.isSearching -> { onToggle(first); onQueryChange("") }
                            list.newName != null -> onCreate(list.newName)
                        }
                    }),
                    leading = { Icon(Icons.Filled.Search, null, tint = colors.mutedText, modifier = Modifier.size(18.dp)) },
                    trailing = {
                        if (query.isNotEmpty()) {
                            Box(
                                Modifier.size(Metrics.touchTarget - Metrics.md).clip(Metrics.control)
                                    .clickable(role = Role.Button) { onQueryChange("") }
                                    .semantics { contentDescription = "Clear search" },
                                contentAlignment = Alignment.Center,
                            ) { Icon(Icons.Filled.Close, null, tint = colors.mutedText, modifier = Modifier.size(16.dp)) }
                        }
                    },
                    contentDescription = "Search or add a condition",
                    modifier = Modifier.testTag("${tag}_search"),
                )
            }
            Hairline()
            LazyColumn(Modifier.fillMaxWidth().heightIn(max = 480.dp).testTag("${tag}_list")) {
                if (list.isEmpty) {
                    item(key = "empty", contentType = "empty") {
                        Text(
                            if (list.isSearching) "No condition matches." else emptyText,
                            style = TaktTheme.type.body,
                            color = colors.mutedText,
                            modifier = Modifier.padding(horizontal = Metrics.lg, vertical = Metrics.md),
                        )
                    }
                }
                for (section in list.sections) {
                    if (section.title != null) {
                        item(key = "header-${section.title}", contentType = "header") { SectionHeader(section.title) }
                    }
                    items(section.rows, key = { "row-${it.condition.id}" }, contentType = { "row" }) { row ->
                        ConditionRow(row, label(row), detail(row), count(row), action, onToggle = { onToggle(row) })
                    }
                }
                list.newName?.let { name ->
                    item(key = "new", contentType = "new") {
                        Row(
                            Modifier
                                .fillMaxWidth()
                                .heightIn(min = Metrics.touchTarget)
                                .clickable(role = Role.Button) { onCreate(name) }
                                .padding(horizontal = Metrics.lg)
                                .testTag("${tag}_new"),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(Metrics.md),
                        ) {
                            Box(Modifier.size(22.dp), contentAlignment = Alignment.Center) {
                                Icon(Icons.Filled.Add, null, tint = colors.primary, modifier = Modifier.size(18.dp))
                            }
                            Text(
                                "New condition “$name”",
                                style = TaktTheme.type.body,
                                color = colors.primary,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
                }
            }
            Spacer(Modifier.size(Metrics.sm))
        }
    }
}

/** One condition: a box (or, under Location when saying where you are, a radio), its name, and what is said beside it. */
@Composable
private fun ConditionRow(
    row: ConditionPickerRow,
    label: String,
    detail: String?,
    count: Int?,
    action: (@Composable (ConditionPickerRow) -> Unit)?,
    onToggle: () -> Unit,
) {
    val colors = TaktTheme.colors
    val condition = row.condition
    Column {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = Metrics.touchTarget)
                .then(
                    // A fixed row reads as one line, its state in the detail
                    // ("this device"), and offers nothing to tap.
                    if (row.isFixed) {
                        Modifier.semantics(mergeDescendants = true) {}
                    } else {
                        Modifier.toggleable(value = row.isOn, role = if (row.isExclusive) Role.RadioButton else Role.Checkbox, onValueChange = { onToggle() })
                    },
                )
                .padding(horizontal = Metrics.lg, vertical = Metrics.xs)
                .testTag("condition_row_${condition.name}"),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Metrics.md),
        ) {
            Indicator(row.isOn, round = row.isExclusive, fixed = row.isFixed)
            Column(Modifier.weight(1f)) {
                Text(
                    label,
                    style = if (row.isOn) TaktTheme.type.bodyStrong else TaktTheme.type.body,
                    color = if (condition.isArchived) colors.mutedText else colors.ink,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                val line = listOfNotNull(row.category, detail).joinToString(" · ")
                if (line.isNotEmpty()) Text(line, style = TaktTheme.type.small, color = colors.mutedText, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            if (count != null && count > 0) {
                MonoText(
                    "$count",
                    color = colors.dimText,
                    modifier = Modifier.semantics { contentDescription = if (count == 1) "1 open task needs it" else "$count open tasks need it" },
                )
            }
            action?.invoke(row)
        }
        Hairline(Modifier.padding(start = Metrics.lg), color = colors.borderMuted)
    }
}

/**
 * A 20dp box, or a round radio for a place: filled azure with a tick when on,
 * a hairline when not. A fixed one (a device condition) is muted, so it does
 * not look like something to tap.
 */
@Composable
private fun Indicator(on: Boolean, round: Boolean, fixed: Boolean = false) {
    val colors = TaktTheme.colors
    val shape = if (round) Metrics.pill else Metrics.control
    val fill = if (fixed) colors.mutedText else colors.primary
    Box(
        Modifier
            .size(22.dp)
            .clip(shape)
            .background(if (on) fill else colors.raised)
            .border(BorderStroke(Metrics.hairline, if (on) fill else colors.inputBorder), shape),
        contentAlignment = Alignment.Center,
    ) {
        if (on) {
            if (round) {
                Box(Modifier.size(8.dp).clip(Metrics.pill).background(colors.onAccent))
            } else {
                Icon(Icons.Filled.Check, null, tint = colors.onAccent, modifier = Modifier.size(16.dp))
            }
        }
    }
}
