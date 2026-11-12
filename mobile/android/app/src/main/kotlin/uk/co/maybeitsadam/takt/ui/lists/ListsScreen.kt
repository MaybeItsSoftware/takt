package uk.co.maybeitsadam.takt.ui.lists

import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
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
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.adaptive.currentWindowAdaptiveInfo
import androidx.compose.material3.adaptive.layout.AnimatedPane
import androidx.compose.material3.adaptive.layout.ListDetailPaneScaffoldRole
import androidx.compose.material3.adaptive.layout.calculatePaneScaffoldDirectiveWithTwoPanesOnMediumWidth
import androidx.compose.material3.adaptive.navigation.NavigableListDetailPaneScaffold
import androidx.compose.material3.adaptive.navigation.rememberListDetailPaneScaffoldNavigator
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.draw.drawBehind
import androidx.compose.material.icons.filled.Settings
import androidx.compose.ui.graphics.Color
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.navigation.Tab
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.app.ListsPrompt
import uk.co.maybeitsadam.takt.ui.commands.Chord
import uk.co.maybeitsadam.takt.ui.commands.KeyCommand
import uk.co.maybeitsadam.takt.ui.commands.RegisterCommands
import uk.co.maybeitsadam.takt.ui.components.EmptyState
import uk.co.maybeitsadam.takt.ui.components.IconAction
import uk.co.maybeitsadam.takt.ui.components.MonoText
import uk.co.maybeitsadam.takt.ui.components.TaktTopBar
import uk.co.maybeitsadam.takt.ui.components.VerticalHairline
import uk.co.maybeitsadam.takt.ui.navigation.ListRoute
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.PIcons

/** The Lists tree's dialogs and sheets. */
private sealed interface ListsDialog {
    data class NewList(val folderId: String?) : ListsDialog
    data class NewFolder(val parentId: String?) : ListsDialog
    data class RenameList(val id: String, val name: String) : ListsDialog
    data class RenameFolder(val id: String, val name: String) : ListsDialog
    data class DeleteList(val id: String, val name: String) : ListsDialog
    data class DeleteFolder(val id: String, val name: String) : ListsDialog
    data class MoveList(val id: String, val name: String, val folderId: String?) : ListsDialog
    data class MoveFolder(val id: String, val name: String, val parentId: String?) : ListsDialog
    data class Share(val id: String, val name: String) : ListsDialog
}

/**
 * What the detail pane shows, as one saveable string: a list (or Everything)
 * and a task to reveal, or a filter list.
 */
internal object DetailKey {
    private const val EVERYTHING = "*"
    private const val FILTER = "filter:"

    fun encode(listId: String?, revealTaskId: String? = null): String = "${listId ?: EVERYTHING}|${revealTaskId.orEmpty()}"

    /** A filter list's key; also what the tree compares a filter list row against. */
    fun filterKey(id: String): String = "$FILTER$id"

    /** The filter list a key shows, or null for a list. */
    fun filterListId(key: String): String? = if (key.startsWith(FILTER)) key.removePrefix(FILTER) else null

    /** What the tree highlights for [key]: a list's id, "" for Everything, or a filter list's key. */
    fun selection(key: String): String = if (filterListId(key) != null) key else decode(key).listId ?: ""

    fun decode(key: String): ListRoute {
        val (list, reveal) = key.split('|', limit = 2).let { it[0] to it.getOrElse(1) { "" } }
        return ListRoute(list.takeIf { it != EVERYTHING }, reveal.ifEmpty { null })
    }
}

/**
 * Folders and lists. On a phone, tapping a list pushes it; on a wide window
 * the tree and the list sit side by side in a list-detail scaffold, so
 * tapping a list does not navigate away.
 */
@Composable
fun ListsScreen() {
    val shell = LocalShell.current
    if (!shell.layout.isWide) {
        ListsTree(
            selectedKey = null,
            onOpen = { listId, reveal -> shell.navigator.openList(listId, reveal) },
            onOpenFilterList = { shell.navigator.openFilterList(it) },
        )
        return
    }
    val directive = calculatePaneScaffoldDirectiveWithTwoPanesOnMediumWidth(currentWindowAdaptiveInfo())
    val navigator = rememberListDetailPaneScaffoldNavigator<String>(scaffoldDirective = directive)
    val scope = rememberCoroutineScope()
    val current = navigator.currentDestination?.contentKey
    NavigableListDetailPaneScaffold(
        navigator = navigator,
        listPane = {
            AnimatedPane {
                ListsTree(
                    selectedKey = current?.let { DetailKey.selection(it) },
                    onOpen = { listId, reveal ->
                        scope.launch { navigator.navigateTo(ListDetailPaneScaffoldRole.Detail, DetailKey.encode(listId, reveal)) }
                    },
                    onOpenFilterList = { id ->
                        scope.launch { navigator.navigateTo(ListDetailPaneScaffoldRole.Detail, DetailKey.filterKey(id)) }
                    },
                    trailingRule = true,
                )
            }
        },
        detailPane = {
            AnimatedPane {
                val key = navigator.currentDestination?.contentKey
                if (key == null) {
                    Column(Modifier.fillMaxSize().background(TaktTheme.colors.paper)) {
                        TaktTopBar("Lists", showHistory = false)
                        EmptyState("Choose a list", detail = "Its outline, board and matrix open here.")
                    }
                } else {
                    val filterListId = DetailKey.filterListId(key)
                    if (filterListId != null) {
                        FilterListContent(filterListId, showBack = false)
                    } else {
                        ListContent(DetailKey.decode(key), showBack = false, onClose = { scope.launch { navigator.navigateBack() } })
                    }
                }
            }
        },
    )
}

/**
 * The tree: Everything, Inbox, the saved filter lists, pinned nested lists,
 * folders (collapsible) with their lists, each list's nested lists beneath
 * it, and an Archived section. Long-press a row for its menu.
 */
@Composable
private fun ListsTree(
    selectedKey: String?,
    onOpen: (listId: String?, revealTaskId: String?) -> Unit,
    onOpenFilterList: (id: String) -> Unit,
    trailingRule: Boolean = false,
) {
    val container = LocalShell.current.container
    val vm: ListsViewModel = viewModel { ListsViewModel(container) }
    val state by vm.tree.collectAsStateWithLifecycle()
    var dialog by remember { mutableStateOf<ListsDialog?>(null) }
    val prompt by container.listsPrompt.pending.collectAsStateWithLifecycle()
    LaunchedEffect(prompt) {
        when (container.listsPrompt.take()) {
            ListsPrompt.NEW_LIST -> dialog = ListsDialog.NewList(null)
            ListsPrompt.NEW_FOLDER -> dialog = ListsDialog.NewFolder(null)
            null -> Unit
        }
    }

    RegisterCommands(
        listOf(
            KeyCommand.LIST_NEW.palette { dialog = ListsDialog.NewList(null) },
            KeyCommand.FOLDER_NEW.palette { dialog = ListsDialog.NewFolder(null) },
            KeyCommand.GO_EVERYTHING.palette { onOpen(null, null) },
            KeyCommand.LIST_JOIN.palette { container.listsPrompt.requestJoin("") },
        ),
    )

    Row(Modifier.fillMaxSize().background(TaktTheme.colors.paper)) {
        Column(
            Modifier
                .weight(1f)
                .fillMaxSize()
                .onPreviewKeyEvent { event ->
                    if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                    when {
                        Chord(Key.N, ctrl = true, shift = true).matches(event) -> dialog = ListsDialog.NewList(null)
                        Chord(Key.N, ctrl = true, alt = true).matches(event) -> dialog = ListsDialog.NewFolder(null)
                        else -> return@onPreviewKeyEvent false
                    }
                    true
                },
        ) {
            TaktTopBar("Lists", sidebar = true) {
                IconAction(PIcons.Shared, "Join a shared list", onClick = { container.listsPrompt.requestJoin("") })
                IconAction(PIcons.Folder, "New folder", onClick = { dialog = ListsDialog.NewFolder(null) })
                IconAction(Icons.Filled.Add, "New list", onClick = { dialog = ListsDialog.NewList(null) })
            }
            LazyColumn(Modifier.fillMaxSize().testTag("lists_tree")) {
                items(state.rows, key = { it.key }, contentType = { it::class.simpleName }) { row ->
                    TreeRowView(row, selectedKey, vm, onOpen, onOpenFilterList) { dialog = it }
                }
            }
        }
        if (trailingRule) VerticalHairline()
    }

    ListsDialogHost(dialog, vm, state.folders) { dialog = null }
}

/** What the sidebar highlights: a list's id, "" for Everything, a filter list's key, or one of these. */
object SidebarKeys {
    const val TODAY = "place:today"
    fun tab(tab: Tab): String = "place:${tab.name}"
}

/**
 * The Mac's left dock: Today, Everything, the folders and lists, then the
 * places beside them. A phone shows it as a drawer; a wide window keeps it
 * open. [onDone] runs after a row is chosen, so a drawer can close.
 */
@Composable
fun Sidebar(selectedKey: String?, onDone: () -> Unit, modifier: Modifier = Modifier) {
    val shell = LocalShell.current
    val navigator = shell.navigator
    val vm: ListsViewModel = viewModel { ListsViewModel(shell.container) }
    val state by vm.tree.collectAsStateWithLifecycle()
    var dialog by remember { mutableStateOf<ListsDialog?>(null) }
    val colors = TaktTheme.colors
    val go: (() -> Unit) -> Unit = { block ->
        block()
        onDone()
    }
    Column(modifier.fillMaxSize().background(colors.altRow)) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = Metrics.headerHeight).padding(start = Metrics.md, end = Metrics.xxs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("Takt", style = TaktTheme.type.body, color = colors.ink, modifier = Modifier.weight(1f))
            IconAction(PIcons.Folder, "New folder", onClick = { dialog = ListsDialog.NewFolder(null) })
            IconAction(Icons.Filled.Add, "New list", onClick = { dialog = ListsDialog.NewList(null) })
        }
        Hairline()
        LazyColumn(Modifier.weight(1f).fillMaxWidth().testTag("sidebar"), contentPadding = PaddingValues(vertical = Metrics.xs)) {
            item(key = SidebarKeys.TODAY, contentType = "place") {
                TreeLine(
                    title = "Today", icon = PIcons.Today, depth = 0, count = null,
                    selected = selectedKey == SidebarKeys.TODAY, onClick = { go { navigator.openTab(Tab.TODAY) } },
                )
            }
            items(state.rows, key = { it.key }, contentType = { it::class.simpleName }) { row ->
                TreeRowView(
                    row, selectedKey, vm,
                    onOpen = { listId, reveal -> go { navigator.openList(listId, reveal, root = true) } },
                    onOpenFilterList = { id -> go { navigator.openFilterList(id, root = true) } },
                ) { dialog = it }
            }
        }
        Hairline()
        Column(Modifier.padding(vertical = Metrics.xs)) {
            for (tab in listOf(Tab.FOCUS, Tab.REVIEW, Tab.SEARCH)) {
                TreeLine(
                    title = tab.title, icon = tab.icon, depth = 0, count = null,
                    selected = selectedKey == SidebarKeys.tab(tab), onClick = { go { navigator.openTab(tab) } },
                )
            }
            TreeLine(title = "History", icon = PIcons.History, depth = 0, count = null, selected = false, onClick = { go { shell.showHistory() } })
            TreeLine(title = "Settings", icon = Icons.Filled.Settings, depth = 0, count = null, selected = false, onClick = { go { navigator.openSettings() } })
        }
    }
    ListsDialogHost(dialog, vm, state.folders) { dialog = null }
}

@Composable
private fun TreeRowView(
    row: TreeRow,
    selectedKey: String?,
    vm: ListsViewModel,
    onOpen: (String?, String?) -> Unit,
    onOpenFilterList: (String) -> Unit,
    show: (ListsDialog) -> Unit,
) {
    when (row) {
        is TreeRow.Everything -> TreeLine(
            title = "Everything", icon = ListIcons.Everything, depth = 0, count = row.openCount,
            selected = selectedKey == "", onClick = { onOpen(null, null) },
        )
        is TreeRow.FilterListRow -> TreeLine(
            title = row.name, icon = ListIcons.Filter, depth = row.depth, count = null,
            selected = selectedKey == DetailKey.filterKey(row.id), onClick = { onOpenFilterList(row.id) },
        )
        is TreeRow.ListRow -> {
            var menu by remember { mutableStateOf(false) }
            val shared by vm.sharedIds.collectAsStateWithLifecycle()
            val isShared = row.id in shared
            Box {
                TreeLine(
                    title = row.name,
                    icon = when {
                        row.isInbox -> PIcons.Inbox
                        isShared -> PIcons.Shared
                        else -> PIcons.ListIcon
                    },
                    depth = row.depth,
                    count = row.openCount, selected = selectedKey == row.id, muted = row.isCompleted,
                    onClick = { onOpen(row.id, null) }, onLongClick = { menu = true },
                )
                PMenu(menu, { menu = false }) {
                    ListMenu(row, isShared, vm, show) { menu = false }
                }
            }
        }
        is TreeRow.NestedRow -> {
            var menu by remember { mutableStateOf(false) }
            Box {
                TreeLine(
                    title = row.title, icon = if (row.isPinned) ListIcons.Pin else ListIcons.Nested, depth = row.depth,
                    count = row.openCount, selected = false,
                    onClick = { onOpen(row.listId, row.taskId) }, onLongClick = { menu = true },
                )
                PMenu(menu, { menu = false }) {
                    PMenuItem(if (row.isPinned) "Unpin" else "Pin to the top", ListIcons.Pin, onClick = {
                        menu = false
                        vm.setNestedPinned(row.taskId, !row.isPinned)
                    })
                    PMenuItem("Archive nested list", PIcons.Archive, onClick = {
                        menu = false
                        vm.setNestedArchived(row.taskId, true)
                    })
                }
            }
        }
        is TreeRow.FolderRow -> {
            var menu by remember { mutableStateOf(false) }
            Box {
                TreeLine(
                    title = row.name, icon = PIcons.Folder, depth = row.depth, count = null, selected = false,
                    disclosure = row.isExpanded,
                    onClick = { vm.toggleFolder(row.id) }, onLongClick = { menu = true },
                )
                PMenu(menu, { menu = false }) {
                    FolderMenu(row, vm, show) { menu = false }
                }
            }
        }
        is TreeRow.ArchivedHeader -> TreeLine(
            title = "Archived", icon = PIcons.Archive, depth = 0, count = row.count, selected = false, muted = true,
            disclosure = row.isExpanded, onClick = { vm.toggleArchived() },
        )
        is TreeRow.ArchivedList -> {
            var menu by remember { mutableStateOf(false) }
            Box {
                TreeLine(
                    title = row.name, icon = PIcons.ListIcon, depth = row.depth, count = null, selected = false, muted = true,
                    onClick = { menu = true }, onLongClick = { menu = true },
                )
                PMenu(menu, { menu = false }) {
                    PMenuItem("Restore", ListIcons.Restore, onClick = {
                        menu = false
                        vm.setArchived(row.id, false)
                    })
                    PMenuItem("Delete…", Icons.Filled.Delete, destructive = true, onClick = {
                        menu = false
                        show(ListsDialog.DeleteList(row.id, row.name))
                    })
                }
            }
        }
    }
}

/**
 * A sidebar row as the Mac's: a 16dp icon in the icon colour 12 in (and 20
 * more a level), the title in ink, the count dim at the secondary size; the
 * current place on the selection's fill. Each ancestor level draws Zed's
 * indent guide, a hairline under its icon, so a level's line runs on
 * unbroken. A folder shows open or shut by its icon, as Zed's does.
 */
@Composable
private fun TreeLine(
    title: String,
    icon: ImageVector,
    depth: Int,
    count: Int?,
    selected: Boolean,
    muted: Boolean = false,
    /** Non-null for a collapsible row: whether it is open. */
    disclosure: Boolean? = null,
    onClick: () -> Unit,
    onLongClick: (() -> Unit)? = null,
) {
    val colors = TaktTheme.colors
    val guide = colors.indentGuide
    Row(
        Modifier
            .fillMaxWidth()
            .background(if (selected) colors.selected else Color.Transparent)
            .heightIn(min = Metrics.touchTarget)
            .drawBehind {
                val inset = SIDEBAR_INSET.toPx()
                val step = SIDEBAR_INDENT.toPx()
                val half = SIDEBAR_ICON.toPx() / 2f
                for (level in 0 until depth) {
                    val x = inset + level * step + half
                    drawLine(guide, Offset(x, 0f), Offset(x, size.height), strokeWidth = 1.dp.toPx())
                }
            }
            .combinedClickable(onClick = onClick, onLongClick = onLongClick, onLongClickLabel = onLongClick?.let { "$title actions" })
            .padding(start = SIDEBAR_INSET + SIDEBAR_INDENT * depth, end = Metrics.md)
            .semantics {
                if (disclosure != null) contentDescription = if (disclosure) "Collapse $title" else "Expand $title"
            },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
    ) {
        val shown = if (icon == PIcons.Folder && disclosure == true) ListIcons.FolderOpen else icon
        Icon(shown, null, tint = colors.icon, modifier = Modifier.size(SIDEBAR_ICON))
        Text(
            title, style = TaktTheme.type.body, color = if (muted) colors.dimText else colors.ink,
            maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
        )
        if (count != null && count > 0) Text(count.toString(), style = TaktTheme.type.secondary, color = colors.dimText, maxLines = 1)
    }
}

/** The Mac's sidebar measures: its 10 inset and 20 a level, and a 16 icon. */
private val SIDEBAR_INSET = 12.dp
private val SIDEBAR_INDENT = 20.dp
private val SIDEBAR_ICON = 16.dp

@Composable
private fun ListMenu(row: TreeRow.ListRow, isShared: Boolean, vm: ListsViewModel, show: (ListsDialog) -> Unit, close: () -> Unit) {
    fun run(block: () -> Unit): () -> Unit = {
        close()
        block()
    }
    if (row.isInbox) {
        PMenuItem("The Inbox is permanent", enabled = false, onClick = close)
        return
    }
    PMenuItem("Rename…", Icons.Filled.Edit, onClick = run { show(ListsDialog.RenameList(row.id, row.name)) })
    PMenuItem("Move to folder…", PIcons.Folder, onClick = run { show(ListsDialog.MoveList(row.id, row.name, row.folderId)) })
    PMenuItem("Move up", Icons.Filled.KeyboardArrowUp, enabled = row.canMoveUp, onClick = run { vm.moveListBy(row.id, -1) })
    PMenuItem("Move down", Icons.Filled.KeyboardArrowDown, enabled = row.canMoveDown, onClick = run { vm.moveListBy(row.id, 1) })
    PMenuDivider()
    PMenuItem(if (row.isCompleted) "Reopen list" else "Complete list", Icons.Filled.Check, onClick = run { vm.setCompleted(row.id, !row.isCompleted) })
    PMenuItem("Archive", PIcons.Archive, onClick = run { vm.setArchived(row.id, true) })
    PMenuItem(if (isShared) "Sharing…" else "Share…", PIcons.Shared, onClick = run { show(ListsDialog.Share(row.id, row.name)) })
    // A shared list is left or unshared from its sharing sheet, not deleted.
    if (!isShared) {
        PMenuItem("Convert to task", ListIcons.Nested, onClick = run { vm.convertListToTask(row.id) })
        PMenuItem("Delete…", Icons.Filled.Delete, destructive = true, onClick = run { show(ListsDialog.DeleteList(row.id, row.name)) })
    }
}

@Composable
private fun FolderMenu(row: TreeRow.FolderRow, vm: ListsViewModel, show: (ListsDialog) -> Unit, close: () -> Unit) {
    fun run(block: () -> Unit): () -> Unit = {
        close()
        block()
    }
    PMenuItem("New list here…", Icons.Filled.Add, onClick = run { show(ListsDialog.NewList(row.id)) })
    PMenuItem("New folder here…", PIcons.Folder, onClick = run { show(ListsDialog.NewFolder(row.id)) })
    PMenuItem("Rename…", Icons.Filled.Edit, onClick = run { show(ListsDialog.RenameFolder(row.id, row.name)) })
    PMenuItem("Move to folder…", PIcons.Folder, onClick = run { show(ListsDialog.MoveFolder(row.id, row.name, row.parentId)) })
    PMenuItem("Move up", Icons.Filled.KeyboardArrowUp, enabled = row.canMoveUp, onClick = run { vm.moveFolderBy(row.id, -1) })
    PMenuItem("Move down", Icons.Filled.KeyboardArrowDown, enabled = row.canMoveDown, onClick = run { vm.moveFolderBy(row.id, 1) })
    PMenuDivider()
    PMenuItem("Delete…", Icons.Filled.Delete, destructive = true, onClick = run { show(ListsDialog.DeleteFolder(row.id, row.name)) })
}

@Composable
private fun ListsDialogHost(dialog: ListsDialog?, vm: ListsViewModel, folders: ImmutableList<FolderChoice>, dismiss: () -> Unit) {
    fun folderChoices(current: String?, excluding: String? = null): ImmutableList<Choice> =
        (
            listOf(Choice(null, "Top level", icon = PIcons.ListIcon, isCurrent = current == null)) +
                folders.filter { it.id != excluding }.map { Choice(it.id, it.name, depth = it.depth, icon = PIcons.Folder, isCurrent = it.id == current) }
            ).toImmutableList()
    when (dialog) {
        null -> Unit
        is ListsDialog.NewList -> NamePromptDialog("New list", confirm = "Create", onDismiss = dismiss) { vm.createList(it, dialog.folderId) }
        is ListsDialog.NewFolder -> NamePromptDialog("New folder", confirm = "Create", onDismiss = dismiss) { vm.createFolder(it, dialog.parentId) }
        is ListsDialog.RenameList -> NamePromptDialog("Rename list", dialog.name, onDismiss = dismiss) { vm.renameList(dialog.id, it) }
        is ListsDialog.RenameFolder -> NamePromptDialog("Rename folder", dialog.name, onDismiss = dismiss) { vm.renameFolder(dialog.id, it) }
        is ListsDialog.DeleteList -> ConfirmDialog(
            "Delete ${dialog.name}?", "This permanently deletes the list and all of its tasks. You can undo it.", onDismiss = dismiss,
        ) { vm.deleteList(dialog.id) }
        is ListsDialog.DeleteFolder -> ConfirmDialog(
            "Delete ${dialog.name}?", "Lists remain, but move to the top level. Nested folders are deleted.", onDismiss = dismiss,
        ) { vm.deleteFolder(dialog.id) }
        is ListsDialog.MoveList -> ChoiceSheet("Move ${dialog.name} to", folderChoices(dialog.folderId), onDismiss = dismiss) {
            vm.moveListToFolder(dialog.id, it.id)
        }
        is ListsDialog.MoveFolder -> ChoiceSheet("Move ${dialog.name} to", folderChoices(dialog.parentId, dialog.id), onDismiss = dismiss) {
            vm.moveFolderTo(dialog.id, it.id)
        }
        is ListsDialog.Share -> SharedListSheet(dialog.id, dialog.name, onDismiss = dismiss)
    }
}
