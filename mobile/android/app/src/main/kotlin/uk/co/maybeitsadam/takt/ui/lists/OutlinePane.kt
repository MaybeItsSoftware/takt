package uk.co.maybeitsadam.takt.ui.lists

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableFloatState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.border
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.app.CelebrationStyle
import uk.co.maybeitsadam.takt.ui.components.SwipeAction
import uk.co.maybeitsadam.takt.ui.components.SwipeRow
import uk.co.maybeitsadam.takt.ui.commands.Chord
import uk.co.maybeitsadam.takt.ui.components.BlockedDivider
import uk.co.maybeitsadam.takt.ui.components.EmptyState
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.MonoText
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.PIcons

/** The outline's drag in progress: which row, and how far it has moved. */
private class OutlineDrag {
    var id by mutableStateOf<String?>(null)
    val offset: MutableFloatState = mutableFloatStateOf(0f)
}

/**
 * The Checkvist-style outline: folding in place, swipes to complete and
 * cancel, a long-press menu with every task command, drag to reorder among
 * siblings, an inline add field, and the hardware keyboard.
 */
@Composable
internal fun OutlinePane(vm: ListViewModel, actions: TaskActions, modifier: Modifier = Modifier) {
    val shape by vm.outline.collectAsStateWithLifecycle()
    val selected by vm.selectedId.collectAsStateWithLifecycle()
    val celebration by vm.celebration.collectAsStateWithLifecycle()
    val scrollTo by vm.scrollTo.collectAsStateWithLifecycle()
    val listState = rememberLazyListState()
    val focus = remember { FocusRequester() }
    val drag = remember { OutlineDrag() }
    val navigator = LocalShell.current.navigator

    LaunchedEffect(scrollTo, shape) {
        val id = scrollTo ?: return@LaunchedEffect
        val index = shape.entries.indexOfFirst { it.key == id }
        if (index >= 0) {
            val visible = listState.layoutInfo.visibleItemsInfo
            val shown = visible.any { it.index == index && it.offset >= 0 && it.offset + it.size <= listState.layoutInfo.viewportEndOffset }
            if (!shown) listState.animateScrollToItem(index, scrollOffset = -listState.layoutInfo.viewportSize.height / 3)
            vm.consumeScroll()
        }
    }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }

    Box(
        modifier
            .fillMaxSize()
            .focusRequester(focus)
            .focusable()
            .onKeyEvent { event ->
                if (event.type != KeyEventType.KeyDown) return@onKeyEvent false
                val rows = shape.rows
                val row = rows.firstOrNull { it.id == selected }
                if (actions.holdKey(event, row)) return@onKeyEvent true
                when {
                    Chord(Key.J).matches(event) || Chord(Key.DirectionDown).matches(event) -> vm.selectAdjacent(rows, 1)
                    Chord(Key.K).matches(event) || Chord(Key.DirectionUp).matches(event) -> vm.selectAdjacent(rows, -1)
                    Chord(Key.DirectionLeft, ctrl = true).matches(event) -> vm.foldAll()
                    Chord(Key.DirectionRight, ctrl = true).matches(event) -> vm.unfoldAll()
                    else -> return@onKeyEvent actions.handleKey(event, row)
                }
                true
            },
    ) {
        if (shape.isLoaded && shape.entries.isEmpty()) {
            Box(Modifier.fillMaxSize()) {
                LazyColumn(Modifier.fillMaxSize().testTag("outline_list").imePadding(), state = listState) {
                    item(key = "empty", contentType = "empty") {
                        EmptyState(if (vm.isEverything) "Nothing open anywhere" else "No tasks yet", detail = "Add one below, or with the field above.")
                    }
                    item(key = "add", contentType = "add") { AddField(vm, focusAfter = focus) }
                }
            }
            return@Box
        }
        LazyColumn(Modifier.fillMaxSize().testTag("outline_list").imePadding(), state = listState) {
            items(shape.entries, key = { it.key }, contentType = { it.contentType }) { entry ->
                when (entry) {
                    is OutlineEntry.Header -> ListHeaderRow(entry) { navigator.openList(entry.listId) }
                    is OutlineEntry.BlockedLabel -> BlockedLabelRow(entry, Modifier.animateItem())
                    is OutlineEntry.Task -> {
                        val row = entry.row
                        val isDragging = drag.id == row.id
                        OutlineRowView(
                            row = row,
                            isSelected = row.id == selected,
                            celebration = celebration,
                            actions = actions,
                            onTap = {
                                actions.tap(row.id)
                                runCatching { focus.requestFocus() }
                            },
                            dragHandle = Modifier.pointerInput(row.id) {
                                detectDragGestures(
                                    onDragStart = {
                                        drag.id = row.id
                                        drag.offset.floatValue = 0f
                                    },
                                    onDragEnd = {
                                        finishDrag(vm, shape.rows, row.id, drag, listState)
                                    },
                                    onDragCancel = {
                                        drag.id = null
                                        drag.offset.floatValue = 0f
                                    },
                                    onDrag = { change, amount ->
                                        change.consume()
                                        drag.offset.floatValue += amount.y
                                    },
                                )
                            },
                            modifier = Modifier
                                .then(if (isDragging) Modifier.zIndex(1f).graphicsLayer { translationY = drag.offset.floatValue } else Modifier.animateItem()),
                        )
                    }
                }
            }
            item(key = "add", contentType = "add") { AddField(vm, focusAfter = focus) }
        }
    }
}

/** Where the dragged row's centre now sits among the drawn rows, and the move that makes. */
private fun finishDrag(vm: ListViewModel, rows: List<OutlineRow>, id: String, drag: OutlineDrag, state: LazyListState) {
    val visible = state.layoutInfo.visibleItemsInfo
    val dragged = visible.firstOrNull { it.key == id }
    val from = rows.indexOfFirst { it.id == id }
    if (dragged != null && from >= 0) {
        val centre = dragged.offset + drag.offset.floatValue + dragged.size / 2f
        val rowIds = rows.withIndex().associate { it.value.id to it.index }
        val candidates = visible.filter { it.key != id && it.key in rowIds }
        val below = candidates.firstOrNull { it.offset + it.size / 2f > centre }
        val to = when {
            below != null -> rowIds.getValue(below.key as String)
            candidates.isNotEmpty() -> rowIds.getValue(candidates.last().key as String) + 1
            else -> from
        }
        vm.drop(rows, from, to)
    }
    drag.id = null
    drag.offset.floatValue = 0f
}

@Composable
private fun ListHeaderRow(header: OutlineEntry.Header, onOpen: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .background(TaktTheme.colors.paper)
            .heightIn(min = 40.dp)
            .clickable(onClick = onOpen)
            .padding(horizontal = Metrics.lg)
            .semantics { contentDescription = "Open ${header.name}" },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(header.name, style = TaktTheme.type.label, color = TaktTheme.colors.mutedText, modifier = Modifier.weight(1f))
        MonoText(header.count.toString(), style = TaktTheme.type.monoSmall, color = TaktTheme.colors.dimText)
    }
    Hairline(color = TaktTheme.colors.borderMuted)
}

/** The label over a sibling group's blocked tail, its text in the title column of the rows it heads. */
@Composable
private fun BlockedLabelRow(label: OutlineEntry.BlockedLabel, modifier: Modifier = Modifier) {
    BlockedDivider(
        isBuried = label.isBuried,
        modifier = modifier
            .background(TaktTheme.colors.paper)
            .padding(
                start = Metrics.xs + Metrics.indent * label.depth + 32.dp + Metrics.touchTarget,
                end = Metrics.lg,
                top = Metrics.sm,
                bottom = Metrics.xxs,
            ),
    )
}

@Composable
private fun OutlineRowView(
    row: OutlineRow,
    isSelected: Boolean,
    celebration: CelebrationStyle,
    actions: TaskActions,
    onTap: () -> Unit,
    dragHandle: Modifier,
    modifier: Modifier = Modifier,
) {
    var menu by remember { mutableStateOf(false) }
    // The same two swipes as Today (docs/mobile-ux.md): right completes,
    // left puts it on today. Cancelling is in the long-press menu.
    val open = row.status == uk.co.maybeitsadam.takt.core.TaskStatus.OPEN
    SwipeRow(
        start = SwipeAction(if (open) "Complete" else "Reopen", Icons.Filled.Check, TaktTheme.colors.success) { actions.toggleComplete(row.id) },
        end = if (open) {
            SwipeAction(if (row.isPlanned) "Take off today" else "Today", PIcons.Today, TaktTheme.colors.primary) { actions.togglePlannedToday(row.id) }
        } else {
            null
        },
        modifier = modifier,
    ) {
        val colors = TaktTheme.colors
        // The Mac's row: [indent][▸][mark][title, truncated][chips], the
        // selection a box at radius 4 in the selection colour with the focus
        // outline. The row is a finger tall; only the selection is painted.
        Box(Modifier.background(colors.paper)) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = Metrics.rowHeight)
                    .padding(horizontal = Metrics.xxs, vertical = Metrics.xxs)
                    .then(
                        if (isSelected) {
                            Modifier
                                .clip(Metrics.row)
                                .background(colors.selected)
                                .border(BorderStroke(Metrics.hairline, colors.focus), Metrics.row)
                        } else {
                            Modifier.clip(Metrics.row)
                        },
                    )
                    .combinedClickable(onClick = onTap, onLongClick = { menu = true }, onLongClickLabel = "Task actions")
                    .padding(start = Metrics.indent * row.depth),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (row.hasChildren) {
                    FoldChevron(row.title, row.isFolded, onToggle = { actions.toggleFold(row.id) })
                } else {
                    Spacer(Modifier.width(FOLD_COLUMN))
                }
                CelebratedCheck(row.status, row.isList, row.title, celebration) { actions.toggleComplete(row.id) }
                TaskTitle(
                    row.title, row.status, celebration,
                    maxLines = 1,
                    isBlocked = row.isBlocked || row.isBuried,
                    modifier = Modifier.weight(1f, fill = true),
                )
                RowMarkers(row, Modifier.padding(start = Metrics.sm).heightIn(min = 24.dp))
                Box(
                    dragHandle
                        .width(32.dp)
                        .heightIn(min = Metrics.touchTarget - Metrics.xs)
                        .semantics { contentDescription = "Reorder ${row.title}" },
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(PIcons.DragHandle, null, tint = colors.dimText, modifier = Modifier.size(16.dp))
                }
            }
            PMenu(expanded = menu, onDismiss = { menu = false }) {
                TaskMenuItems(row, actions, close = { menu = false })
            }
        }
    }
}

/**
 * The inline add field at the foot of the outline. Enter files the task and
 * keeps the field, so the next one goes in after it, Checkvist-style.
 */
@Composable
private fun AddField(vm: ListViewModel, focusAfter: FocusRequester) {
    var text by remember { mutableStateOf("") }
    var lastAdded by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    val submit = {
        val value = text
        if (value.isNotBlank()) {
            text = ""
            scope.launch { lastAdded = vm.addTask(value, lastAdded) ?: lastAdded }
        }
    }
    Row(
        Modifier.fillMaxWidth().padding(horizontal = Metrics.md, vertical = Metrics.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PTextField(
            value = text,
            onValueChange = { text = it },
            placeholder = if (vm.isEverything) "Add a task to the Inbox" else "Add a task",
            modifier = Modifier
                .weight(1f)
                .testTag("outline_add_field")
                .onKeyEvent { event ->
                    // Escape leaves the field for the outline's own keys.
                    if (event.type == KeyEventType.KeyDown && event.key == Key.Escape) {
                        lastAdded = null
                        runCatching { focusAfter.requestFocus() }
                        true
                    } else {
                        false
                    }
                },
            imeAction = ImeAction.Done,
            onSubmit = submit,
        )
    }
}
