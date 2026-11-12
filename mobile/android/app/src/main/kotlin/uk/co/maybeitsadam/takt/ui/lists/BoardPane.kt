package uk.co.maybeitsadam.takt.ui.lists

import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.PageSize
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.BorderStroke
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.app.CelebrationStyle
import uk.co.maybeitsadam.takt.ui.commands.Chord
import uk.co.maybeitsadam.takt.ui.components.BlockedDivider
import uk.co.maybeitsadam.takt.ui.components.Card
import uk.co.maybeitsadam.takt.ui.components.EmptyState
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.IconAction
import uk.co.maybeitsadam.takt.ui.components.MonoText
import uk.co.maybeitsadam.takt.ui.components.Tag
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics

/** Where a card's second line starts: flush with its title, past the status mark's strip. */
private val CARD_TEXT_INSET = 28.dp

/** How many subtask rows a card draws before "+N more", as on the Mac. */
private const val VISIBLE_SUBTASK_ROWS = 12

/**
 * The board: a pager of kanban columns (several abreast on a wide window),
 * column tabs above, cards with their subtasks folded as the outline folds
 * them, and moving cards between columns from a menu or Alt+Shift+←/→.
 */
@Composable
internal fun BoardPane(vm: ListViewModel, actions: TaskActions, show: (ListDialog) -> Unit, modifier: Modifier = Modifier) {
    val shape by vm.board.collectAsStateWithLifecycle()
    val selected by vm.selectedId.collectAsStateWithLifecycle()
    val celebration by vm.celebration.collectAsStateWithLifecycle()
    val wide = LocalShell.current.layout.isWide
    val columns = shape.columns
    val pager = rememberPagerState { columns.size }
    val scope = rememberCoroutineScope()
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }

    // Follow the selected card when it moves column.
    LaunchedEffect(selected, shape) {
        val index = columns.indexOfFirst { c -> c.cards.any { it.id == selected } }
        if (index >= 0 && index != pager.currentPage && !wide) pager.animateScrollToPage(index)
    }

    Column(
        modifier
            .fillMaxSize()
            .focusRequester(focus)
            .focusable()
            .onKeyEvent { event ->
                if (event.type != KeyEventType.KeyDown) return@onKeyEvent false
                val current = columns.getOrNull(pager.currentPage)
                val cards = (columns.firstOrNull { c -> c.cards.any { it.id == selected } } ?: current)?.cards.orEmpty()
                val card = cards.firstOrNull { it.id == selected }
                if (actions.holdKey(event, card?.row)) return@onKeyEvent true
                when {
                    Chord(Key.DirectionLeft, shift = true, alt = true).matches(event) -> card?.let { vm.moveCardBy(it.id, -1) }
                    Chord(Key.DirectionRight, shift = true, alt = true).matches(event) -> card?.let { vm.moveCardBy(it.id, 1) }
                    Chord(Key.C, ctrl = true, shift = true).matches(event) -> show(ListDialog.AddColumn)
                    Chord(Key.J).matches(event) || Chord(Key.DirectionDown).matches(event) -> vm.selectAdjacent(cards.map { it.row }, 1)
                    Chord(Key.K).matches(event) || Chord(Key.DirectionUp).matches(event) -> vm.selectAdjacent(cards.map { it.row }, -1)
                    Chord(Key.DirectionLeft).matches(event) -> scope.launch { pager.animateScrollToPage((pager.currentPage - 1).coerceAtLeast(0)) }
                    Chord(Key.DirectionRight).matches(event) -> scope.launch { pager.animateScrollToPage((pager.currentPage + 1).coerceAtMost(columns.size - 1)) }
                    Chord(Key.DirectionLeft, ctrl = true).matches(event) -> vm.foldAll()
                    Chord(Key.DirectionRight, ctrl = true).matches(event) -> vm.unfoldAll()
                    else -> return@onKeyEvent actions.handleKey(event, card?.row)
                }
                true
            },
    ) {
        // Column tabs: the page indicator, and a way to jump.
        Row(Modifier.fillMaxWidth().padding(start = Metrics.sm), verticalAlignment = Alignment.CenterVertically) {
            LazyRow(
                Modifier.weight(1f).testTag("board_tabs"),
                horizontalArrangement = Arrangement.spacedBy(Metrics.xs),
                contentPadding = PaddingValues(vertical = Metrics.xxs),
            ) {
                itemsIndexed(columns, key = { _, c -> c.id }, contentType = { _, _ -> "tab" }) { index, column ->
                    Tag(
                        "${column.title} ${column.cards.size}",
                        selected = index == pager.currentPage,
                        color = if (index == pager.currentPage) TaktTheme.colors.primary else TaktTheme.colors.mutedText,
                        onClick = { scope.launch { pager.animateScrollToPage(index) } },
                    )
                }
            }
            IconAction(Icons.Filled.Add, "Add a board column", onClick = { show(ListDialog.AddColumn) })
            val current = columns.getOrNull(pager.currentPage)
            ColumnMenu(
                canRemove = columns.size > 1,
                canMoveLeft = current != null && pager.currentPage > 0,
                canMoveRight = current != null && pager.currentPage < columns.size - 1,
                onRename = { current?.let { show(ListDialog.RenameColumn(it.id, it.title)) } },
                onMove = { offset ->
                    current?.let {
                        vm.moveColumn(it.id, offset)
                        scope.launch { pager.animateScrollToPage((pager.currentPage + offset).coerceIn(0, columns.size - 1)) }
                    }
                },
                onRemove = { current?.let { show(ListDialog.RemoveColumn(it.id, it.title)) } },
            )
        }
        Hairline()
        if (shape.isLoaded && columns.isEmpty()) {
            EmptyState("No columns")
            return@Column
        }
        HorizontalPager(
            state = pager,
            modifier = Modifier.fillMaxSize().testTag("board_pager"),
            pageSize = if (wide) PageSize.Fixed(320.dp) else PageSize.Fill,
            beyondViewportPageCount = 1,
            key = { columns.getOrNull(it)?.id ?: it },
        ) { page ->
            val column = columns.getOrNull(page) ?: return@HorizontalPager
            BoardColumnView(column, selected, celebration, actions, vm)
        }
    }
}

@Composable
private fun ColumnMenu(
    canRemove: Boolean,
    canMoveLeft: Boolean,
    canMoveRight: Boolean,
    onRename: () -> Unit,
    onMove: (Int) -> Unit,
    onRemove: () -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    Box {
        IconAction(Icons.Filled.MoreVert, "Column actions", onClick = { open = true })
        PMenu(open, { open = false }) {
            PMenuItem("Rename…", Icons.Filled.Edit, onClick = {
                open = false
                onRename()
            })
            PMenuItem("Move left", Icons.AutoMirrored.Filled.KeyboardArrowLeft, enabled = canMoveLeft, onClick = {
                open = false
                onMove(-1)
            })
            PMenuItem("Move right", Icons.AutoMirrored.Filled.KeyboardArrowRight, enabled = canMoveRight, onClick = {
                open = false
                onMove(1)
            })
            PMenuDivider()
            PMenuItem("Remove this column", Icons.Filled.Delete, destructive = true, enabled = canRemove, onClick = {
                open = false
                onRemove()
            })
        }
    }
}

@Composable
private fun BoardColumnView(
    column: BoardColumnModel,
    selected: String?,
    celebration: CelebrationStyle,
    actions: TaskActions,
    vm: ListViewModel,
) {
    // The Mac's column: the panel fill at radius 6 in the muted border, 8 in
    // from its neighbours, the muted title and the count at its head.
    Column(
        Modifier
            .fillMaxSize()
            .padding(Metrics.sm)
            .clip(Metrics.popover)
            .background(TaktTheme.colors.altRow)
            .border(BorderStroke(Metrics.hairline, TaktTheme.colors.borderMuted), Metrics.popover),
    ) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = Metrics.paintedRow + Metrics.xs).padding(horizontal = Metrics.sm),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(column.title, style = TaktTheme.type.body, color = TaktTheme.colors.mutedText, maxLines = 1, modifier = Modifier.weight(1f))
            // Filled by its rules: a card dropped here is edited to match them.
            if (column.isRuleColumn) {
                MonoText("rules", style = TaktTheme.type.monoSmall, color = TaktTheme.colors.dimText, modifier = Modifier.padding(end = Metrics.sm))
            }
            Text(column.cards.size.toString(), style = TaktTheme.type.secondary, color = TaktTheme.colors.dimText)
        }
        LazyColumn(
            Modifier.fillMaxSize().testTag("board_column_${column.id}"),
            contentPadding = PaddingValues(start = Metrics.xs, end = Metrics.xs, bottom = Metrics.xl),
            verticalArrangement = Arrangement.spacedBy(Metrics.xs),
        ) {
            if (column.cards.isEmpty()) {
                item(key = "empty", contentType = "empty") {
                    Text("No cards", style = TaktTheme.type.secondary, color = TaktTheme.colors.dimText, modifier = Modifier.padding(Metrics.sm))
                }
            }
            for (card in column.cards) {
                // The label over the column's blocked cards, which sink to its
                // foot, and over its buried ones, which sink below them.
                if (card.id == column.blockedLabelId) {
                    item(key = "blocked:label", contentType = "blocked") {
                        BlockedDivider(Modifier.animateItem().padding(start = Metrics.xs, top = Metrics.xs))
                    }
                }
                if (card.id == column.buriedLabelId) {
                    item(key = "buried:label", contentType = "blocked") {
                        BlockedDivider(Modifier.animateItem().padding(start = Metrics.xs, top = Metrics.xs), isBuried = true)
                    }
                }
                item(key = card.id, contentType = "card") {
                    BoardCardView(card, card.id == selected, celebration, actions, vm, Modifier.animateItem())
                }
            }
        }
    }
}

@Composable
private fun BoardCardView(
    card: BoardCard,
    isSelected: Boolean,
    celebration: CelebrationStyle,
    actions: TaskActions,
    vm: ListViewModel,
    modifier: Modifier = Modifier,
) {
    var menu by remember { mutableStateOf(false) }
    val row = card.row
    Box(modifier) {
        Card(Modifier.fillMaxWidth(), selected = isSelected) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .combinedClickable(onClick = { actions.tap(row.id) }, onLongClick = { menu = true }, onLongClickLabel = "Card actions")
                    .padding(end = Metrics.sm, bottom = Metrics.xs),
            ) {
                // The Mac's card: the mark and title, then the list (or the
                // parent) in dim secondary text and the chips, flush with the title.
                Row(verticalAlignment = Alignment.CenterVertically) {
                    CelebratedCheck(row.status, row.isList, row.title, celebration) { actions.toggleComplete(row.id) }
                    TaskTitle(
                        row.title, row.status, celebration, Modifier.weight(1f).padding(vertical = Metrics.xs),
                        maxLines = 2, isBlocked = row.isBlocked || row.isBuried,
                    )
                    if (row.hasChildren) FoldChevron(row.title, row.isFolded, onToggle = { actions.toggleFold(row.id) })
                }
                val detail = card.parentTitle?.let { "Under $it" } ?: card.listName
                if (detail != null) {
                    Text(detail, style = TaktTheme.type.secondary, color = TaktTheme.colors.dimText, maxLines = 1, modifier = Modifier.padding(start = CARD_TEXT_INSET))
                }
                Box(Modifier.padding(start = CARD_TEXT_INSET).heightIn(max = 24.dp)) { RowMarkers(row) }
                for (sub in card.subtasks.take(VISIBLE_SUBTASK_ROWS)) {
                    SubtaskRow(sub, celebration, actions, cardIsBlocked = row.isBlocked || row.isBuried)
                }
                if (card.subtasks.size > VISIBLE_SUBTASK_ROWS) {
                    Text(
                        "+${card.subtasks.size - VISIBLE_SUBTASK_ROWS} more", style = TaktTheme.type.secondary, color = TaktTheme.colors.dimText,
                        modifier = Modifier.padding(start = CARD_TEXT_INSET, top = Metrics.xxs),
                    )
                } else if (row.isFolded && card.subtaskCount > 0) {
                    Text(
                        "${card.subtaskCount} folded", style = TaktTheme.type.secondary, color = TaktTheme.colors.dimText,
                        modifier = Modifier.padding(start = CARD_TEXT_INSET, top = Metrics.xxs),
                    )
                }
            }
        }
        PMenu(menu, { menu = false }) {
            PMenuItem("Move to column…", uk.co.maybeitsadam.takt.ui.theme.PIcons.Board, trailing = "Alt+Shift+←/→", onClick = {
                menu = false
                actions.askMoveCard(row)
            })
            PMenuItem("Column to the left", onClick = {
                menu = false
                vm.moveCardBy(row.id, -1)
            })
            PMenuItem("Column to the right", onClick = {
                menu = false
                vm.moveCardBy(row.id, 1)
            })
            PMenuDivider()
            TaskMenuItems(row, actions, close = { menu = false }, structure = card.parentTitle == null)
        }
    }
}

@Composable
private fun SubtaskRow(row: OutlineRow, celebration: CelebrationStyle, actions: TaskActions, cardIsBlocked: Boolean) {
    // The Mac's compact subtask row, marked └ and set in the secondary size;
    // a blocked one is dim.
    Row(
        Modifier
            .fillMaxWidth()
            .padding(start = Metrics.sm + Metrics.indent * row.depth),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("└", style = TaktTheme.type.secondary, color = TaktTheme.colors.dimText)
        CelebratedCheck(row.status, row.isList, row.title, celebration) { actions.toggleComplete(row.id) }
        TaskTitle(
            row.title, row.status, celebration, Modifier.weight(1f), style = TaktTheme.type.secondary, maxLines = 1,
            isBlocked = row.isBlocked || row.isBuried || cardIsBlocked,
        )
        if (row.hasChildren) {
            FoldChevron(row.title, row.isFolded, onToggle = { actions.toggleFold(row.id) })
        } else {
            Spacer(Modifier.width(Metrics.xs))
        }
    }
}
