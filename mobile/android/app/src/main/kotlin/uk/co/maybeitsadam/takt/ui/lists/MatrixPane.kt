package uk.co.maybeitsadam.takt.ui.lists

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import uk.co.maybeitsadam.takt.app.CelebrationStyle
import uk.co.maybeitsadam.takt.ui.commands.KeyCommand
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.MicroLabel
import uk.co.maybeitsadam.takt.ui.components.VerticalHairline
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.PIcons

/** A quadrant's hue, as the Mac picks it: danger, primary, warning, dim. */
@Composable
internal fun MatrixCell.tint(): Color = when (this) {
    MatrixCell.DO_NOW -> TaktTheme.colors.danger
    MatrixCell.SCHEDULE -> TaktTheme.colors.primary
    MatrixCell.DELEGATE -> TaktTheme.colors.warning
    MatrixCell.ELIMINATE -> TaktTheme.colors.dimText
}

/**
 * The Eisenhower matrix over the scope's cards: unplaced tasks first, then
 * the four quadrants in a 2x2 grid, each its own scrolling list. Alt+1…4
 * files the selected task.
 */
@Composable
internal fun MatrixPane(vm: ListViewModel, actions: TaskActions, modifier: Modifier = Modifier) {
    val shape by vm.matrix.collectAsStateWithLifecycle()
    val selected by vm.selectedId.collectAsStateWithLifecycle()
    val celebration by vm.celebration.collectAsStateWithLifecycle()
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    val ordered = remember(shape) { (shape.unplaced + shape.quadrants.flatMap { it.cards }).map { it.row } }

    Column(
        modifier
            .fillMaxSize()
            .focusRequester(focus)
            .focusable()
            .onKeyEvent { event ->
                if (event.type != KeyEventType.KeyDown) return@onKeyEvent false
                val row = ordered.firstOrNull { it.id == selected }
                if (actions.holdKey(event, row)) return@onKeyEvent true
                // The catalogue lists ⌥1…⌥4 in quadrant order, so the chord's position is the quadrant.
                val placement = KeyCommand.PLAN_MATRIX_PLACE.chordIndex(event)
                when {
                    placement >= 0 -> row?.let { vm.place(it.id, MatrixCell.entries[placement]) }
                    KeyCommand.PLAN_SELECT_NEXT.matches(event) -> vm.selectAdjacent(ordered, 1)
                    KeyCommand.PLAN_SELECT_PREVIOUS.matches(event) -> vm.selectAdjacent(ordered, -1)
                    else -> return@onKeyEvent actions.handleKey(event, row)
                }
                true
            },
    ) {
        if (shape.unplaced.isNotEmpty()) {
            Row(Modifier.fillMaxWidth().heightIn(min = Metrics.paintedRow).padding(horizontal = Metrics.md), verticalAlignment = Alignment.CenterVertically) {
                Text("Not placed  ${shape.unplaced.size}", style = TaktTheme.type.body, color = TaktTheme.colors.mutedText, modifier = Modifier.weight(1f))
            }
            LazyColumn(Modifier.fillMaxWidth().heightIn(max = 168.dp).testTag("matrix_unplaced")) {
                items(shape.unplaced, key = { it.id }, contentType = { "matrix_row" }) { card ->
                    MatrixRow(card, card.id == selected, null, celebration, actions, vm)
                }
            }
            Hairline()
        }
        val cells = shape.quadrants
        Column(Modifier.fillMaxSize().testTag("matrix_grid")) {
            for (pair in cells.chunked(2)) {
                Row(Modifier.fillMaxWidth().weight(1f)) {
                    pair.forEachIndexed { index, quadrant ->
                        if (index > 0) VerticalHairline()
                        QuadrantView(quadrant, selected, celebration, actions, vm, Modifier.weight(1f).fillMaxHeight())
                    }
                }
                Hairline()
            }
        }
    }
}

@Composable
private fun QuadrantView(
    quadrant: MatrixQuadrantModel,
    selected: String?,
    celebration: CelebrationStyle,
    actions: TaskActions,
    vm: ListViewModel,
    modifier: Modifier,
) {
    // The Mac's quadrant: its number, name and count in a micro-label, no hue.
    Column(modifier.testTag("matrix_${quadrant.cell.name.lowercase()}")) {
        Row(Modifier.fillMaxWidth().heightIn(min = Metrics.paintedRow).padding(horizontal = Metrics.sm), verticalAlignment = Alignment.CenterVertically) {
            MicroLabel("${quadrant.cell.ordinal + 1}  ${quadrant.cell.title}  ${quadrant.cards.size}", Modifier.weight(1f), color = TaktTheme.colors.dimText)
        }
        if (quadrant.cards.isEmpty()) {
            Text(quadrant.cell.detail, style = TaktTheme.type.secondary, color = TaktTheme.colors.dimText, modifier = Modifier.padding(horizontal = Metrics.sm))
        }
        LazyColumn(Modifier.fillMaxSize()) {
            items(quadrant.cards, key = { it.id }, contentType = { "matrix_row" }) { card ->
                MatrixRow(card, card.id == selected, quadrant.cell, celebration, actions, vm)
            }
        }
    }
}

@Composable
private fun MatrixRow(
    card: BoardCard,
    isSelected: Boolean,
    cell: MatrixCell?,
    celebration: CelebrationStyle,
    actions: TaskActions,
    vm: ListViewModel,
) {
    var menu by remember { mutableStateOf(false) }
    val row = card.row
    val colors = TaktTheme.colors
    val open = row.status == uk.co.maybeitsadam.takt.core.TaskStatus.OPEN
    // The Mac's matrix chip: a box 20 high (23dp) at radius 4 in the muted
    // border, a 6dp dot and the title; selected, the selection's fill and the
    // focus outline. The row around it is a finger tall; the dot completes.
    Box(Modifier.fillMaxWidth().heightIn(min = Metrics.touchTarget).padding(horizontal = Metrics.xs), contentAlignment = Alignment.CenterStart) {
        Row(
            Modifier
                .heightIn(min = 23.dp)
                .clip(Metrics.row)
                .background(if (isSelected) colors.selected else colors.paper)
                .border(BorderStroke(Metrics.hairline, if (isSelected) colors.focus else colors.borderMuted), Metrics.row)
                .combinedClickable(onClick = { actions.tap(row.id) }, onLongClick = { menu = true }, onLongClickLabel = "Task actions")
                .padding(end = Metrics.xs + Metrics.xxs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                Modifier
                    .size(width = 20.dp, height = Metrics.touchTarget)
                    .clickable(role = Role.Checkbox, indication = null, interactionSource = null) { actions.toggleComplete(row.id) }
                    .semantics { contentDescription = (if (open) "Complete " else "Reopen ") + row.title },
                contentAlignment = Alignment.Center,
            ) {
                Box(Modifier.size(6.dp).clip(Metrics.pill).background(if (isSelected) colors.primary else if (open) colors.mutedText else colors.dimText))
            }
            TaskTitle(row.title, row.status, celebration, style = TaktTheme.type.small, maxLines = 1, isBlocked = row.isBlocked || row.isBuried)
        }
        PMenu(menu, { menu = false }) {
            MatrixCell.entries.forEachIndexed { index, target ->
                PMenuItem(
                    target.title, PIcons.Grid, trailing = "Alt+${index + 1}", enabled = target != cell,
                    onClick = {
                        menu = false
                        vm.place(row.id, target)
                    },
                )
            }
            if (cell != null) {
                PMenuItem("Take off the matrix", Icons.Filled.Clear, onClick = {
                    menu = false
                    vm.place(row.id, null)
                })
            }
            PMenuDivider()
            TaskMenuItems(row, actions, close = { menu = false }, structure = false)
        }
    }
}
