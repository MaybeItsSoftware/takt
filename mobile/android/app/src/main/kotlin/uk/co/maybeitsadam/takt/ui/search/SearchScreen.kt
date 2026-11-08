package uk.co.maybeitsadam.takt.ui.search

import androidx.compose.foundation.BorderStroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.Color
import uk.co.maybeitsadam.takt.ui.commands.PhoneCommands
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.clickable
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.Search
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
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import java.time.LocalDate
import java.time.ZoneId
import uk.co.maybeitsadam.takt.core.TaskStatus
import uk.co.maybeitsadam.takt.data.workspace.TaskSearchResult
import uk.co.maybeitsadam.takt.ui.components.Format
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.IconAction
import uk.co.maybeitsadam.takt.ui.components.TaktTopBar
import uk.co.maybeitsadam.takt.ui.components.SectionHeader
import uk.co.maybeitsadam.takt.ui.components.Tag
import uk.co.maybeitsadam.takt.ui.components.TaskCheck
import uk.co.maybeitsadam.takt.ui.lists.OutlineShaping
import uk.co.maybeitsadam.takt.ui.lists.WaitingBadge
import uk.co.maybeitsadam.takt.ui.lists.WaitingMarkers
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.PIcons
import uk.co.maybeitsadam.takt.ui.theme.parseHexColor

/**
 * Find: the Mac's command palette, list navigator and search in one field.
 * What you type is matched against the phone's commands and every list by
 * the core's palette scoring, and against task titles and notes by
 * full-text search. With nothing typed it is a launcher: recent commands
 * first, then every place. Enter runs the best row; Up and Down walk the
 * tasks.
 */
@Composable
fun SearchScreen() {
    val shell = LocalShell.current
    val model = viewModel { SearchViewModel(shell.container) }
    val query by model.query.collectAsStateWithLifecycle()
    val includeCompleted by model.includeCompleted.collectAsStateWithLifecycle()
    val state by model.state.collectAsStateWithLifecycle()
    val palette by model.palette.collectAsStateWithLifecycle()
    val focus = remember { FocusRequester() }
    val focusManager = LocalFocusManager.current
    var selected by rememberSaveable(state.query) { mutableStateOf<Int?>(null) }
    val listState = rememberLazyListState()
    val results = state.results
    val trimmed = query.trim()

    fun reveal(result: TaskSearchResult) {
        focusManager.clearFocus()
        shell.navigator.openList(result.list.id, revealTaskId = result.task.id)
    }

    fun runCommand(id: String) {
        focusManager.clearFocus()
        model.recordCommand(id)
        model.setQuery("")
        PhoneCommands.run(id, shell, shell.container.session.value?.inboxId)
    }

    fun openPlace(place: FindPlace) {
        focusManager.clearFocus()
        model.setQuery("")
        when (place.kind) {
            FindPlaceKind.FILTER -> place.id?.let(shell.navigator::openFilterList)
            else -> shell.navigator.openList(place.id)
        }
    }

    /** Enter: a picked task, else the best command, else the best place, else the first task. */
    fun submit() {
        val picked = selected?.let { results.getOrNull(it) }
        when {
            picked != null -> reveal(picked)
            trimmed.isEmpty() -> Unit
            palette.commands.isNotEmpty() -> runCommand(palette.commands.first().id)
            palette.places.isNotEmpty() -> openPlace(palette.places.first())
            else -> results.firstOrNull()?.let(::reveal)
        }
    }

    LaunchedEffect(Unit) { focus.requestFocus() }

    Column(Modifier.fillMaxSize().background(TaktTheme.colors.paper).imePadding()) {
        TaktTopBar("Find", sidebar = true)
        Column(Modifier.padding(horizontal = Metrics.lg, vertical = Metrics.sm)) {
            SearchField(
                value = query,
                onValueChange = model::setQuery,
                modifier = Modifier
                    .focusRequester(focus)
                    .onPreviewKeyEvent { event ->
                        if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                        when (event.key) {
                            Key.DirectionDown -> { selected = SearchMatching.move(selected, 1, results.size); true }
                            Key.DirectionUp -> { selected = SearchMatching.move(selected, -1, results.size); true }
                            Key.Enter, Key.NumPadEnter -> { submit(); true }
                            else -> false
                        }
                    },
                onSubmit = ::submit,
            )
        }
        Hairline()
        LazyColumn(Modifier.fillMaxSize().testTag("search_results"), state = listState) {
            if (palette.commands.isNotEmpty()) {
                item(key = "commands", contentType = "header") { SectionHeader("Commands") }
                items(palette.commands, key = { "command-${it.id}" }, contentType = { "command" }) { command ->
                    FindRow(
                        icon = Icons.AutoMirrored.Filled.KeyboardArrowRight,
                        title = command.title,
                        detail = command.group,
                        tag = "find_command_${command.id}",
                        onClick = { runCommand(command.id) },
                    )
                }
            }
            if (palette.places.isNotEmpty()) {
                item(key = "places", contentType = "header") { SectionHeader("Lists", detail = palette.places.size.toString()) }
                items(palette.places, key = { "place-${it.kind}-${it.id}" }, contentType = { "place" }) { place ->
                    FindRow(
                        icon = when (place.kind) {
                            FindPlaceKind.EVERYTHING -> PIcons.Inbox
                            FindPlaceKind.FILTER -> Icons.Filled.Search
                            FindPlaceKind.LIST -> PIcons.ListIcon
                        },
                        iconTint = parseHexColor(place.colorHex),
                        title = place.name,
                        detail = place.detail.takeIf { it.isNotBlank() },
                        tag = "find_place",
                        onClick = { openPlace(place) },
                    )
                }
            }
            if (trimmed.isNotEmpty()) {
                item(key = "tasks", contentType = "header") {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        SectionHeader(
                            when {
                                state.query != trimmed -> "Tasks"
                                results.size == 1 -> "1 task"
                                else -> "${results.size} tasks"
                            },
                            modifier = Modifier.weight(1f),
                        )
                        Tag(
                            "Include completed",
                            selected = includeCompleted,
                            color = if (includeCompleted) TaktTheme.colors.primary else TaktTheme.colors.mutedText,
                            leading = if (includeCompleted) Icons.Filled.Check else null,
                            modifier = Modifier
                                .padding(end = Metrics.lg)
                                .testTag("search_include_completed")
                                .semantics { contentDescription = if (includeCompleted) "Include completed, on" else "Include completed, off" },
                            onClick = { model.setIncludeCompleted(!includeCompleted) },
                        )
                    }
                }
                if (results.isEmpty() && state.query == trimmed) {
                    item(key = "no-tasks", contentType = "empty") {
                        Text(
                            if (includeCompleted) "No task matches" else "No open task matches",
                            style = TaktTheme.type.small,
                            color = TaktTheme.colors.mutedText,
                            modifier = Modifier.padding(horizontal = Metrics.lg, vertical = Metrics.md),
                        )
                    }
                }
                itemsIndexed(results, key = { _, r -> r.task.id }, contentType = { _, _ -> "result" }) { index, result ->
                    ResultRow(
                        result = result,
                        query = state.query.orEmpty(),
                        isSelected = index == selected,
                        isBlocked = result.task.id in state.blockedIds,
                        starts = state.buriedStarts[result.task.id]?.let {
                            OutlineShaping.startsLabel(it, ZoneId.systemDefault(), LocalDate.now())
                        },
                        waiting = state.waiting[result.task.id],
                        onOpen = { reveal(result) },
                        onInspect = { model.inspect(result.task.id) },
                        onToggle = { model.toggle(result) },
                    )
                }
            }
        }
    }
}

/** A command or a place: a glyph, its name and where it belongs. */
@Composable
private fun FindRow(
    icon: ImageVector,
    title: String,
    detail: String?,
    tag: String,
    onClick: () -> Unit,
    iconTint: Color? = null,
) {
    val colors = TaktTheme.colors
    Column(Modifier.fillMaxWidth().clickable(onClick = onClick).testTag(tag)) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = Metrics.touchTarget + Metrics.xs).padding(horizontal = Metrics.lg),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Metrics.md),
        ) {
            Icon(icon, null, tint = iconTint ?: colors.mutedText, modifier = Modifier.size(18.dp))
            Column(Modifier.weight(1f)) {
                Text(title, style = TaktTheme.type.body, color = colors.ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (detail != null) {
                    Text(detail, style = TaktTheme.type.small, color = colors.mutedText, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
        Hairline(Modifier.padding(start = Metrics.lg + 18.dp + Metrics.md), color = colors.borderMuted)
    }
}

@Composable
private fun SearchField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    onSubmit: () -> Unit,
) {
    val colors = TaktTheme.colors
    BasicTextField(
        value = value,
        onValueChange = onValueChange,
        singleLine = true,
        textStyle = TaktTheme.type.field.copy(color = colors.ink),
        cursorBrush = SolidColor(colors.primary),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Text, imeAction = ImeAction.Search),
        keyboardActions = KeyboardActions(onSearch = { onSubmit() }),
        modifier = modifier
            .fillMaxWidth()
            .testTag("search_field")
            .semantics { contentDescription = "Search tasks" },
        decorationBox = { inner ->
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = Metrics.touchTarget)
                    .background(colors.raised, Metrics.control)
                    .border(BorderStroke(Metrics.hairline, colors.inputBorder), Metrics.control)
                    .padding(start = Metrics.md),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(Icons.Filled.Search, null, tint = colors.mutedText, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(Metrics.sm))
                Box(Modifier.weight(1f)) {
                    if (value.isEmpty()) Text("Search tasks", style = TaktTheme.type.field, color = colors.dimText)
                    inner()
                }
                if (value.isNotEmpty()) IconAction(Icons.Filled.Clear, "Clear search") { onValueChange("") }
            }
        },
    )
}

@Composable
private fun ResultRow(
    result: TaskSearchResult,
    query: String,
    isSelected: Boolean,
    isBlocked: Boolean,
    starts: String?,
    waiting: WaitingBadge?,
    onOpen: () -> Unit,
    onInspect: () -> Unit,
    onToggle: () -> Unit,
) {
    val colors = TaktTheme.colors
    val task = result.task
    val open = task.status == TaskStatus.OPEN
    val highlight = SpanStyle(color = colors.ink, fontWeight = FontWeight.SemiBold, background = colors.warning.copy(alpha = 0.22f))
    val title = remember(task.title, query, highlight) { highlighted(task.title, query, highlight) }
    val snippet = result.notesSnippet?.takeIf { it.isNotBlank() }
    val listColor = parseHexColor(result.list.colorHex) ?: colors.mutedText
    Column(
        Modifier
            .fillMaxWidth()
            .background(if (isSelected) colors.hover else colors.paper)
            .combinedClickable(onClickLabel = "Reveal in list", onLongClickLabel = "Open details", onLongClick = onInspect, onClick = onOpen)
            .semantics { contentDescription = "Search result ${task.title}" }
            .testTag("search_result"),
    ) {
        Row(Modifier.fillMaxWidth().padding(end = Metrics.lg, top = Metrics.xs, bottom = Metrics.xs), verticalAlignment = Alignment.Top) {
            TaskCheck(task.status, isList = task.isList, label = task.title, onToggle = onToggle)
            Column(Modifier.weight(1f).padding(top = Metrics.md), verticalArrangement = Arrangement.spacedBy(Metrics.xxs)) {
                Text(
                    title,
                    style = TaktTheme.type.body,
                    color = if (open && !isBlocked && starts == null) colors.ink else colors.mutedText,
                    textDecoration = if (open) null else TextDecoration.LineThrough,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                if (snippet != null) {
                    Text(
                        remember(snippet, query, highlight) { highlighted(snippet, query, highlight) },
                        style = TaktTheme.type.small,
                        color = colors.mutedText,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Metrics.xs)) {
                    Icon(PIcons.ListIcon, null, tint = listColor, modifier = Modifier.size(12.dp))
                    Text(result.list.name, style = TaktTheme.type.small, color = listColor, maxLines = 1)
                    if (isBlocked) {
                        Spacer(Modifier.width(Metrics.xs))
                        Text("Blocked", style = TaktTheme.type.small, color = colors.dimText)
                    }
                    if (starts != null) {
                        Spacer(Modifier.width(Metrics.xs))
                        Text(starts, style = TaktTheme.type.monoSmall, color = colors.warning, maxLines = 1)
                    }
                    if (waiting != null) {
                        Spacer(Modifier.width(Metrics.xs))
                        WaitingMarkers(waiting)
                    }
                    statusLabel(task.status)?.let { label ->
                        Spacer(Modifier.width(Metrics.xs))
                        Text(label, style = TaktTheme.type.small, color = colors.dimText)
                    }
                }
            }
            task.dueAt?.let { due ->
                val overdue = open && due.isBefore(java.time.Instant.now())
                Tag(
                    Format.due(due),
                    color = if (overdue) colors.danger else colors.mutedText,
                    mono = true,
                    modifier = Modifier.padding(top = Metrics.md, start = Metrics.sm),
                )
            }
        }
        Hairline(Modifier.padding(start = Metrics.touchTarget), color = colors.borderMuted)
    }
}

private fun statusLabel(status: TaskStatus): String? = when (status) {
    TaskStatus.OPEN -> null
    TaskStatus.COMPLETED -> "Done"
    TaskStatus.CANCELLED -> "Cancelled"
}

private fun highlighted(text: String, query: String, style: SpanStyle): AnnotatedString = buildAnnotatedString {
    append(text)
    for (range in SearchMatching.ranges(text, query)) addStyle(style, range.first, range.last + 1)
}
