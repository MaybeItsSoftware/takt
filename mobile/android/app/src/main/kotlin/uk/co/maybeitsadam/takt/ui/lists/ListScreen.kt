package uk.co.maybeitsadam.takt.ui.lists

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.heightIn
import uk.co.maybeitsadam.takt.ui.components.MicroLabel
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.ui.Alignment
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.testTag
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.collections.immutable.toImmutableList
import uk.co.maybeitsadam.takt.core.TaskListRole
import uk.co.maybeitsadam.takt.ui.commands.KeyCommand
import uk.co.maybeitsadam.takt.ui.commands.RegisterCommands
import uk.co.maybeitsadam.takt.ui.components.IconAction
import uk.co.maybeitsadam.takt.ui.components.TaktTopBar
import uk.co.maybeitsadam.takt.ui.components.Tag
import uk.co.maybeitsadam.takt.ui.navigation.ListRoute
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.quickadd.QuickAddBar
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.PIcons

/**
 * One list, or Everything when the route's list is null: a place of its own
 * when the sidebar opened it, otherwise pushed with a way back.
 */
@Composable
fun ListScreen(route: ListRoute) {
    val navigator = LocalShell.current.navigator
    ListContent(route, showBack = !route.root, onClose = { if (route.root) navigator.openList(null, root = true) else navigator.back() })
}

/**
 * The list's header, its Outline / Board / Matrix switcher, quick add, and
 * the chosen view. The Lists screen's detail pane draws the same thing.
 */
@Composable
internal fun ListContent(route: ListRoute, showBack: Boolean, onClose: () -> Unit, modifier: Modifier = Modifier) {
    val shell = LocalShell.current
    val container = shell.container
    val vm: ListViewModel = viewModel(key = "list:${route.listId ?: "everything"}") {
        ListViewModel(container, route.listId, null)
    }
    LaunchedEffect(route.revealTaskId) { route.revealTaskId?.let { vm.reveal(it) } }
    DisposableEffect(route.listId) {
        container.quickAdd.setContextList(route.listId)
        onDispose { container.quickAdd.setContextList(null) }
    }

    val list by vm.list.collectAsStateWithLifecycle()
    val mode by vm.viewMode.collectAsStateWithLifecycle()
    val hideCompleted by vm.hideCompleted.collectAsStateWithLifecycle()
    val outline by vm.outline.collectAsStateWithLifecycle()
    val selectedId by vm.selectedId.collectAsStateWithLifecycle()
    var dialog by remember { mutableStateOf<ListDialog?>(null) }
    val actions = remember(vm, container) { TaskActions(vm, container) { dialog = it } }
    val title = if (route.listId == null) "Everything" else list?.name ?: ""
    val canEditList = list != null && list?.systemRole != TaskListRole.INBOX
    val board by vm.board.collectAsStateWithLifecycle()
    val selectedRow = outline.rows.firstOrNull { it.id == selectedId }
        ?: board.columns.flatMap { it.cards }.firstOrNull { it.id == selectedId }?.row

    RegisterCommands(
        listOf(
            KeyCommand.GO_OUTLINE.palette { vm.setViewMode(ListViewMode.OUTLINE) },
            KeyCommand.GO_BOARD.palette { vm.setViewMode(ListViewMode.BOARD) },
            KeyCommand.GO_MATRIX.palette { vm.setViewMode(ListViewMode.MATRIX) },
            KeyCommand.PLAN_FOLD_ALL.palette { vm.foldAll() },
            KeyCommand.PLAN_UNFOLD_ALL.palette { vm.unfoldAll() },
            KeyCommand.PLAN_BOARD_NEW_COLUMN.palette { dialog = ListDialog.AddColumn },
            KeyCommand.PLAN_SELECT_NEXT.palette { vm.selectAdjacent(outline.rows, 1) },
            KeyCommand.PLAN_SELECT_PREVIOUS.palette { vm.selectAdjacent(outline.rows, -1) },
        ) + actions.paletteCommands(selectedRow) + if (canEditList) {
            listOf(
                KeyCommand.LIST_RENAME.palette { dialog = ListDialog.RenameList },
                KeyCommand.LIST_COMPLETE.palette { vm.toggleCompleted() },
                KeyCommand.LIST_ARCHIVE.palette {
                    vm.setArchived(true)
                    onClose()
                },
                KeyCommand.LIST_DELETE.palette { dialog = ListDialog.DeleteList },
            )
        } else {
            emptyList()
        },
    )

    Column(
        modifier
            .fillMaxSize()
            .background(TaktTheme.colors.paper)
            .onPreviewKeyEvent { event ->
                if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                when {
                    KeyCommand.GO_BOARD.matchesShortcut(event) -> vm.setViewMode(ListViewMode.BOARD)
                    KeyCommand.GO_OUTLINE.matchesShortcut(event) -> vm.setViewMode(ListViewMode.OUTLINE)
                    KeyCommand.GO_MATRIX.matchesShortcut(event) -> vm.setViewMode(ListViewMode.MATRIX)
                    KeyCommand.LIST_RENAME.matchesShortcut(event) && canEditList -> dialog = ListDialog.RenameList
                    KeyCommand.LIST_COMPLETE.matchesShortcut(event) && canEditList -> vm.toggleCompleted()
                    else -> return@onPreviewKeyEvent false
                }
                true
            },
    ) {
        TaktTopBar(
            title = title,
            subtitle = list?.completedAt?.let { "Completed" },
            sidebar = true,
            navigation = if (showBack) {
                { IconAction(Icons.AutoMirrored.Filled.ArrowBack, "Back", tint = TaktTheme.colors.ink, onClick = { shell.navigator.back() }) }
            } else {
                null
            },
        ) {
            // The Mac's mode strip, as one tag that steps through the three.
            ViewModeToggle(mode, vm::setViewMode)
            ListOverflow(vm, hideCompleted, canEditList, list?.completedAt != null, onArchive = onClose) { dialog = it }
        }
        QuickAddBar(route.listId, Modifier.fillMaxWidth())
        Box(Modifier.fillMaxSize()) {
            when (mode) {
                ListViewMode.OUTLINE -> OutlinePane(vm, actions)
                ListViewMode.BOARD -> BoardPane(vm, actions, show = { dialog = it })
                ListViewMode.MATRIX -> MatrixPane(vm, actions)
            }
        }
    }

    ListDialogHost(dialog, vm, actions, title, onClose) { dialog = null }
}

/**
 * The list's view, at the top: one labelled tag saying which of outline,
 * board and matrix is shown. A tap moves to the next; holding it lists all
 * three. Labelled rather than three bare icons, because the icons alone
 * were easy to miss.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun ViewModeToggle(current: ListViewMode, onChange: (ListViewMode) -> Unit) {
    var menu by remember { mutableStateOf(false) }
    val colors = TaktTheme.colors
    val icon = when (current) {
        ListViewMode.OUTLINE -> PIcons.Outline
        ListViewMode.BOARD -> PIcons.Board
        ListViewMode.MATRIX -> PIcons.Grid
    }
    Box {
        Box(
            Modifier
                .heightIn(min = Metrics.touchTarget)
                .combinedClickable(
                    role = Role.Button,
                    onClickLabel = "Show ${current.next().title}",
                    onLongClickLabel = "Choose a view",
                    onLongClick = { menu = true },
                    onClick = { onChange(current.next()) },
                )
                .semantics { contentDescription = "View: ${current.title}. Tap for ${current.next().title}" }
                .testTag("view_toggle"),
            contentAlignment = Alignment.Center,
        ) {
            Row(
                Modifier
                    .border(Metrics.hairline, colors.border, Metrics.control)
                    .background(colors.paper, Metrics.control)
                    .padding(horizontal = Metrics.sm, vertical = Metrics.xs),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Metrics.xs),
            ) {
                Icon(icon, null, tint = colors.primary, modifier = Modifier.size(16.dp))
                MicroLabel(current.title, color = colors.ink)
                // Where it stands in the round: one mark per view, this one filled.
                Row(horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                    for (mode in ListViewMode.entries) {
                        Box(Modifier.size(4.dp).background(if (mode == current) colors.primary else colors.borderMuted, Metrics.control))
                    }
                }
            }
        }
        PMenu(menu, { menu = false }) {
            for (mode in ListViewMode.entries) {
                PMenuItem(
                    mode.title,
                    when (mode) {
                        ListViewMode.OUTLINE -> PIcons.Outline
                        ListViewMode.BOARD -> PIcons.Board
                        ListViewMode.MATRIX -> PIcons.Grid
                    },
                    selected = mode == current,
                    modifier = Modifier.testTag("view_${mode.raw}"),
                ) {
                    menu = false
                    onChange(mode)
                }
            }
        }
    }
}

@Composable
private fun ListOverflow(
    vm: ListViewModel,
    hideCompleted: Boolean,
    canEditList: Boolean,
    isCompleted: Boolean,
    onArchive: () -> Unit,
    show: (ListDialog) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    Box {
        IconAction(Icons.Filled.MoreVert, "List actions", onClick = { open = true })
        PMenu(open, { open = false }) {
            fun run(block: () -> Unit): () -> Unit = {
                open = false
                block()
            }
            PMenuItem(if (hideCompleted) "Show completed" else "Hide completed", Icons.Filled.Check, onClick = run { vm.setHideCompleted(!hideCompleted) })
            PMenuItem("Fold all", Icons.Filled.KeyboardArrowUp, trailing = "Ctrl+←", onClick = run { vm.foldAll() })
            PMenuItem("Unfold all", Icons.Filled.KeyboardArrowDown, trailing = "Ctrl+→", onClick = run { vm.unfoldAll() })
            PMenuItem("Add board column…", Icons.Filled.Add, onClick = run { show(ListDialog.AddColumn) })
            if (canEditList) {
                PMenuDivider()
                PMenuItem("Rename list…", Icons.Filled.Edit, trailing = "Ctrl+R", onClick = run { show(ListDialog.RenameList) })
                PMenuItem(if (isCompleted) "Reopen list" else "Complete list", Icons.Filled.Check, onClick = run { vm.toggleCompleted() })
                PMenuItem("Archive list", PIcons.Archive, onClick = run {
                    vm.setArchived(true)
                    onArchive()
                })
                PMenuItem("Delete list…", Icons.Filled.Delete, destructive = true, onClick = run { show(ListDialog.DeleteList) })
            }
        }
    }
}

/** The list screen's dialogs and sheets. */
@Composable
private fun ListDialogHost(
    dialog: ListDialog?,
    vm: ListViewModel,
    actions: TaskActions,
    listName: String,
    onClose: () -> Unit,
    dismiss: () -> Unit,
) {
    when (dialog) {
        null -> Unit
        is ListDialog.RenameTask -> NamePromptDialog("Rename task", dialog.title, onDismiss = dismiss) { actions.rename(dialog.id, it) }
        is ListDialog.DeleteTask -> ConfirmDialog(
            "Delete ${dialog.title}?", "This deletes the task and everything beneath it. You can undo it.", onDismiss = dismiss,
        ) { actions.delete(dialog.id) }
        is ListDialog.MoveTask -> {
            val lists by vm.allLists.collectAsStateWithLifecycle()
            MoveToListSheet(
                dialog.title, dialog.listId, lists, onDismiss = dismiss,
                onMove = { actions.moveToList(dialog.id, it) },
                onMoveToNew = { actions.moveToNewList(dialog.id, it) },
            )
        }
        is ListDialog.NewListForMove -> NamePromptDialog("New list", confirm = "Move", onDismiss = dismiss) { actions.moveToNewList(dialog.id, it) }
        is ListDialog.MoveCard -> {
            val board by vm.board.collectAsStateWithLifecycle()
            val current = board.columnOf(dialog.id)?.id
            val choices = remember(board, dialog) {
                board.columns.map { Choice(it.id, it.title, detail = "${it.cards.size} cards", icon = PIcons.Board, isCurrent = it.id == current) }
                    .toImmutableList()
            }
            ChoiceSheet("Move ${dialog.title} to column", choices, onDismiss = dismiss) { choice -> choice.id?.let { vm.moveCard(dialog.id, it) } }
        }
        is ListDialog.PlaceCard -> {
            val choices = remember { MatrixCell.entries.map { Choice(it.name, it.title, detail = it.detail, icon = PIcons.Grid) }.toImmutableList() }
            ChoiceSheet("Place ${dialog.title}", choices, onDismiss = dismiss) { choice ->
                vm.place(dialog.id, MatrixCell.entries.firstOrNull { it.name == choice.id })
            }
        }
        ListDialog.RenameList -> NamePromptDialog("Rename list", listName, onDismiss = dismiss) { vm.renameList(it) }
        ListDialog.DeleteList -> ConfirmDialog(
            "Delete $listName?", "This permanently deletes the list and all of its tasks. You can undo it.", onDismiss = dismiss,
        ) {
            vm.deleteList()
            onClose()
        }
        ListDialog.AddColumn -> NamePromptDialog("New board column", confirm = "Add", onDismiss = dismiss) { vm.addColumn(it) }
        is ListDialog.RenameColumn -> NamePromptDialog("Rename column", dialog.title, onDismiss = dismiss) { vm.renameColumn(dialog.id, it) }
        is ListDialog.RemoveColumn -> ConfirmDialog(
            "Remove ${dialog.title}?", "Its cards move to the first remaining column, in one undo step.", confirm = "Remove", onDismiss = dismiss,
        ) { vm.removeColumn(dialog.id) }
    }
}
