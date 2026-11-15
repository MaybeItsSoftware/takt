package uk.co.maybeitsadam.takt.ui.lists

import android.os.SystemClock
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.type
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.app.AppContainer
import uk.co.maybeitsadam.takt.core.TaskStatus
import uk.co.maybeitsadam.takt.ui.commands.KeyCatalog
import uk.co.maybeitsadam.takt.ui.commands.KeyCommand
import uk.co.maybeitsadam.takt.ui.commands.KeySequence
import uk.co.maybeitsadam.takt.ui.commands.PaletteCommand
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.theme.PIcons

/** The dialogs and sheets a list screen can show. */
sealed interface ListDialog {
    data class RenameTask(val id: String, val title: String) : ListDialog
    data class DeleteTask(val id: String, val title: String) : ListDialog
    data class MoveTask(val id: String, val title: String, val listId: String) : ListDialog
    data class NewListForMove(val id: String) : ListDialog
    data class MoveCard(val id: String, val title: String) : ListDialog
    data class PlaceCard(val id: String, val title: String) : ListDialog
    data object RenameList : ListDialog
    data object DeleteList : ListDialog
    data object AddColumn : ListDialog
    data class RemoveColumn(val id: String, val title: String) : ListDialog
    data class RenameColumn(val id: String, val title: String) : ListDialog
}

/**
 * Every task command a row offers, routed through the shared [TaskCommands]
 * (so a menu, a key and the palette do the same thing) or the screen's view
 * model where the screen owns the state (folding, the Fold celebration).
 */
@Stable
class TaskActions(
    private val vm: ListViewModel,
    private val container: AppContainer,
    private val show: (ListDialog) -> Unit,
) {
    private val commands get() = container.commands

    /** The list the screen shows; null in Everything, where every row's list is elsewhere. */
    val screenListId: String? get() = vm.listId

    fun select(id: String) = vm.select(id)

    /** Tap: select the row and open its inspector. */
    fun tap(id: String) {
        vm.select(id)
        container.inspector.open(id)
    }

    fun openInspector(id: String) = container.inspector.open(id)
    fun toggleComplete(id: String) = vm.toggleComplete(id)
    fun toggleInvalidate(id: String) = vm.toggleInvalidate(id)
    fun toggleFold(id: String) = vm.toggleFold(id)
    fun indent(id: String) = commands.indent(id)
    fun outdent(id: String) = commands.outdent(id)
    fun moveUp(id: String) = commands.moveUp(id)
    fun moveDown(id: String) = commands.moveDown(id)
    fun toggleList(id: String) = vm.toggleList(id)
    fun extractBranch(id: String) = commands.extractBranch(id)
    fun dueToday(id: String) = commands.dueToday(id)
    fun dueTomorrow(id: String) = commands.dueTomorrow(id)
    fun clearDue(id: String) = commands.clearDue(id)
    fun clearStart(id: String) = commands.clearStart(id)
    fun setPriority(id: String, priority: Int?) = commands.setPriority(id, priority)
    fun togglePlannedToday(id: String) = commands.togglePlannedToday(id)
    fun toggleDaily(id: String) = commands.toggleDaily(id)
    fun openHabit(id: String) = container.habits.open(id)
    fun startFocus(id: String) = commands.startFocus(id)
    fun moveToList(id: String, listId: String) = commands.moveToList(id, listId)
    fun moveToNewList(id: String, name: String) = commands.moveToNewList(id, name)
    fun rename(id: String, title: String) = commands.rename(id, title)
    fun delete(id: String) = commands.delete(id)

    fun askRename(row: OutlineRow) = show(ListDialog.RenameTask(row.id, row.title))
    fun askDelete(row: OutlineRow) = show(ListDialog.DeleteTask(row.id, row.title))
    fun askMove(row: OutlineRow) = show(ListDialog.MoveTask(row.id, row.title, row.listId))
    fun askMoveCard(row: OutlineRow) = show(ListDialog.MoveCard(row.id, row.title))
    fun askPlace(row: OutlineRow) = show(ListDialog.PlaceCard(row.id, row.title))

    private val sequence = KeySequence(KeyCatalog.sequences.keys)

    /** The task a held key was pressed on, which it runs on when released. */
    private var heldTaskId: String? = null
    private var expiry: Job? = null

    /**
     * Holds `-` to see whether a digit follows, as the Mac does: `-2`–`-4`
     * set those priorities, and `-` on its own sets −1 once the hold times
     * out or another key arrives. Every view calls this first, before its
     * own keys, so that a key it keeps for itself (j/k) still releases the
     * hold before it moves the selection. Returns true when it consumed the
     * event.
     */
    fun holdKey(event: KeyEvent, row: OutlineRow?): Boolean {
        if (event.type != KeyEventType.KeyDown) return false
        val key = if (row == null) "" else KeyCatalog.sequenceKey(event)
        if (key.isEmpty() && sequence.prefix.isEmpty()) return false
        val now = SystemClock.uptimeMillis()
        return when (val result = sequence.advance(key, now)) {
            KeySequence.Result.Pass -> false
            KeySequence.Result.Pending -> {
                hold(row!!.id, now)
                true
            }
            is KeySequence.Result.Command -> {
                val id = heldTaskId
                release()
                val command = KeyCatalog.sequences[result.sequence]
                if (id != null && command == KeyCommand.TASK_SET_PRIORITY) {
                    setPriority(id, result.sequence.toInt())
                }
                true
            }
            is KeySequence.Result.Flush -> {
                runHeld(result.held)
                false
            }
            is KeySequence.Result.FlushAndHold -> {
                runHeld(result.held)
                hold(row!!.id, now)
                true
            }
        }
    }

    private fun hold(id: String, at: Long) {
        expiry?.cancel()
        heldTaskId = id
        expiry = vm.viewModelScope.launch {
            delay(KeySequence.TIMEOUT_MS)
            sequence.expire(at)?.let(::runHeld)
        }
    }

    private fun release() {
        expiry?.cancel()
        expiry = null
        heldTaskId = null
    }

    /** A held key on its own: `-` is the only one, send to the back. */
    private fun runHeld(key: String) {
        val id = heldTaskId
        release()
        if (id != null && key == "-") setPriority(id, -1)
    }

    /**
     * The keys every task view shares, on the selected [row]. Returns true when
     * it handled the event. Navigation (j/k) is the view's own, and a held
     * `-` is [holdKey]'s.
     */
    fun handleKey(event: KeyEvent, row: OutlineRow?): Boolean {
        if (event.type != KeyEventType.KeyDown || row == null) return false
        val id = row.id
        val action: (() -> Unit)? = when {
            KeyCommand.PLAN_ENTER_TASK.matches(event) -> { { openInspector(id) } }
            KeyCommand.TASK_TOGGLE_INSPECTOR.matches(event) -> { { openInspector(id) } }
            KeyCommand.TASK_COMPLETE.matches(event) -> { { toggleComplete(id) } }
            KeyCommand.TASK_INVALIDATE.matches(event) -> { { toggleInvalidate(id) } }
            KeyCommand.TASK_INDENT.matches(event) -> { { indent(id) } }
            KeyCommand.TASK_OUTDENT.matches(event) -> { { outdent(id) } }
            KeyCommand.TASK_MOVE_UP.matches(event) -> { { moveUp(id) } }
            KeyCommand.TASK_MOVE_DOWN.matches(event) -> { { moveDown(id) } }
            KeyCommand.PLAN_TOGGLE_FOLD.matches(event) -> { { toggleFold(id) } }
            KeyCommand.TASK_DELETE.matches(event) -> { { askDelete(row) } }
            KeyCommand.TASK_RENAME.matches(event) -> { { askRename(row) } }
            KeyCommand.TASK_START_FOCUS.matches(event) -> { { startFocus(id) } }
            KeyCommand.TASK_TOGGLE_PLANNED_TODAY.matches(event) -> { { togglePlannedToday(id) } }
            KeyCommand.TASK_DUE_TODAY.matches(event) -> { { dueToday(id) } }
            KeyCommand.TASK_DUE_TOMORROW.matches(event) -> { { dueTomorrow(id) } }
            KeyCommand.TASK_TOGGLE_DAILY.matches(event) -> { { toggleDaily(id) } }
            KeyCommand.TASK_HABIT.matches(event) -> { { openHabit(id) } }
            KeyCommand.TASK_CONVERT_TO_LIST.matches(event) -> { { toggleList(id) } }
            KeyCommand.TASK_MOVE.matches(event) -> { { askMove(row) } }
            KeyCommand.TASK_CLEAR_PRIORITY.matches(event) -> { { setPriority(id, null) } }
            KeyCommand.TASK_SEND_TO_BACK.matches(event) -> { { setPriority(id, -1) } }
            KeyCommand.TASK_SET_PRIORITY.chordIndex(event) >= 0 -> {
                val priority = KeyCommand.TASK_SET_PRIORITY.chordIndex(event) + 1
                { setPriority(id, priority) }
            }
            else -> null
        }
        action?.invoke()
        return action != null
    }

    /** The selected task's commands for the palette (Ctrl+K). */
    fun paletteCommands(row: OutlineRow?): List<PaletteCommand> {
        if (row == null) return emptyList()
        val id = row.id
        return listOf(
            KeyCommand.PLAN_ENTER_TASK.palette { openInspector(id) },
            KeyCommand.TASK_COMPLETE.palette { toggleComplete(id) },
            KeyCommand.TASK_INVALIDATE.palette { toggleInvalidate(id) },
            KeyCommand.TASK_INDENT.palette { indent(id) },
            KeyCommand.TASK_OUTDENT.palette { outdent(id) },
            KeyCommand.TASK_MOVE_UP.palette { moveUp(id) },
            KeyCommand.TASK_MOVE_DOWN.palette { moveDown(id) },
            KeyCommand.TASK_MOVE.palette { askMove(row) },
            KeyCommand.TASK_CONVERT_TO_LIST.palette { toggleList(id) },
            KeyCommand.TASK_EXTRACT_BRANCH.palette { extractBranch(id) },
            KeyCommand.TASK_DUE_TODAY.palette { dueToday(id) },
            KeyCommand.TASK_DUE_TOMORROW.palette { dueTomorrow(id) },
            KeyCommand.TASK_CLEAR_DUE.palette { clearDue(id) },
            KeyCommand.TASK_CLEAR_START.palette { clearStart(id) },
            KeyCommand.TASK_CLEAR_PRIORITY.palette { setPriority(id, null) },
            KeyCommand.TASK_SEND_TO_BACK.palette { setPriority(id, -1) },
            KeyCommand.TASK_TOGGLE_PLANNED_TODAY.palette { togglePlannedToday(id) },
            KeyCommand.TASK_TOGGLE_DAILY.palette { toggleDaily(id) },
            KeyCommand.TASK_HABIT.palette { openHabit(id) },
            KeyCommand.TASK_START_FOCUS.palette { startFocus(id) },
            KeyCommand.TASK_TOGGLE_INSPECTOR.palette { openInspector(id) },
            KeyCommand.PLAN_TOGGLE_FOLD.palette { toggleFold(id) },
            KeyCommand.TASK_RENAME.palette { askRename(row) },
            KeyCommand.TASK_DELETE.palette { askDelete(row) },
        )
    }
}

/**
 * Every task command, as menu items, for a long-press menu. [structure]
 * offers indent/outdent and reorder, which only make sense where the row
 * sits in its tree.
 */
@Composable
internal fun ColumnScope.TaskMenuItems(
    row: OutlineRow,
    actions: TaskActions,
    close: () -> Unit,
    structure: Boolean = true,
) {
    fun run(block: () -> Unit): () -> Unit = {
        close()
        block()
    }
    val id = row.id
    PMenuItem("Open", Icons.Filled.Info, trailing = "Enter", onClick = run { actions.openInspector(id) })
    PMenuItem(
        if (row.status == TaskStatus.OPEN) "Complete" else "Reopen", Icons.Filled.Check, trailing = "Space",
        onClick = run { actions.toggleComplete(id) },
    )
    PMenuItem(
        if (row.status == TaskStatus.CANCELLED) "Reinstate" else "Cancel task", Icons.Filled.Close,
        onClick = run { actions.toggleInvalidate(id) },
    )
    PMenuDivider()
    if (structure) {
        PMenuItem("Indent", PIcons.Indent, trailing = "Tab", onClick = run { actions.indent(id) })
        PMenuItem("Outdent", PIcons.Outdent, trailing = "Shift+Tab", onClick = run { actions.outdent(id) })
        PMenuItem("Move up", Icons.Filled.KeyboardArrowUp, trailing = "Alt+Up", onClick = run { actions.moveUp(id) })
        PMenuItem("Move down", Icons.Filled.KeyboardArrowDown, trailing = "Alt+Down", onClick = run { actions.moveDown(id) })
    }
    PMenuItem("Move to list…", PIcons.ListIcon, onClick = run { actions.askMove(row) })
    if (row.listId != actions.screenListId) {
        val navigator = LocalShell.current.navigator
        PMenuItem("Go to list", PIcons.ListIcon, onClick = run { navigator.openList(row.listId, id) })
    }
    PMenuItem(if (row.isList) "Convert to task" else "Convert to list", ListIcons.Nested, onClick = run { actions.toggleList(id) })
    PMenuItem("Extract branch as list", PIcons.Folder, onClick = run { actions.extractBranch(id) })
    PMenuDivider()
    PMenuItem("Due today", PIcons.Calendar, onClick = run { actions.dueToday(id) })
    PMenuItem("Due tomorrow", PIcons.Calendar, onClick = run { actions.dueTomorrow(id) })
    if (row.due != null) PMenuItem("Clear due date", Icons.Filled.Clear, onClick = run { actions.clearDue(id) })
    if (row.starts != null) PMenuItem("Clear start date", Icons.Filled.Clear, onClick = run { actions.clearStart(id) })
    for (priority in 1..4) {
        PMenuItem("Priority $priority", PIcons.Flag, trailing = if (row.priority == priority) "Set" else null, onClick = run { actions.setPriority(id, priority) })
    }
    // Behind every task with no priority, and to the bottom of its list.
    PMenuItem("Send to the back (!-1)", PIcons.Flag, trailing = if (row.priority == -1) "Set" else null, onClick = run { actions.setPriority(id, -1) })
    if (row.priority != null) PMenuItem("No priority", PIcons.Flag, onClick = run { actions.setPriority(id, null) })
    PMenuDivider()
    PMenuItem(if (row.isPlanned) "Take off today" else "Plan for today", PIcons.Today, onClick = run { actions.togglePlannedToday(id) })
    PMenuItem(if (row.isDaily) "Stop daily" else "Make daily", PIcons.Repeat, onClick = run { actions.toggleDaily(id) })
    PMenuItem(if (row.isDaily) "Habit options…" else "Make a habit…", PIcons.Repeat, onClick = run { actions.openHabit(id) })
    PMenuItem("Start focus", Icons.Filled.PlayArrow, trailing = "F", onClick = run { actions.startFocus(id) })
    PMenuDivider()
    PMenuItem("Rename…", Icons.Filled.Edit, trailing = "F2", onClick = run { actions.askRename(row) })
    PMenuItem("Delete…", Icons.Filled.Delete, destructive = true, onClick = run { actions.askDelete(row) })
}
