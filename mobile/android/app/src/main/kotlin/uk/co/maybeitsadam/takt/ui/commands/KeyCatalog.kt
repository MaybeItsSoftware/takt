package uk.co.maybeitsadam.takt.ui.commands

import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEvent
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isMetaPressed
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.nativeKeyCode
import uniffi.takt_core.CommandEntry
import uniffi.takt_core.keyboardChord
import uniffi.takt_core.workspaceCommandCatalog

/**
 * The commands Android runs, by their id in the Mac's command catalogue. The
 * catalogue is the Rust core's (`core/src/keymap/catalogue.rs`): a command's
 * title, group and keys come from there, read once, so a key the Mac's
 * catalogue rebinds moves here too. Android takes the entries it can act on
 * — the shell takes the `anywhere` ones, a list screen the view switches and
 * the outline, board and matrix entries, a selected row the task entries —
 * and translates their keys for a hardware keyboard: the Mac's ⌘ is Ctrl
 * (or Meta), ⌥ is Alt. A Mac key held on Control (`ctrl+tab`) has no
 * translation, since Ctrl is already ⌘, and neither has a two-letter
 * sequence (`dd`).
 *
 * The four commands with [androidTitle] are Android's own: the catalogue has
 * no entry for them, because the Mac does them through its own list
 * behaviour or its menu bar.
 */
enum class KeyCommand(
    val id: String,
    private val androidTitle: String? = null,
    private val androidGroup: String? = null,
) {
    GO_TODAY("goToday"),
    GO_BOARD("goBoard"),
    GO_OUTLINE("goOutline"),
    GO_MATRIX("goMatrix"),
    GO_EVERYTHING("goEverything"),
    GO_FOCUS("goFocus"),
    GO_TIMELINE("goTimeline"),
    GO_SEARCH("goSearch"),
    GO_COMMAND_PALETTE("goCommandPalette"),
    TASK_NEW("taskNew"),
    TASK_NEW_ABOVE("taskNewAbove"),
    TASK_NEW_CHILD("taskNewChild"),
    TASK_COMPLETE("taskComplete"),
    TASK_INVALIDATE("taskInvalidate"),
    TASK_DELETE("taskDelete"),
    TASK_RENAME("taskRename"),
    TASK_DUE_TODAY("taskDueToday"),
    TASK_DUE_TOMORROW("taskDueTomorrow"),
    TASK_CLEAR_DUE("taskClearDue"),
    TASK_CLEAR_START("taskClearStart"),
    TASK_CLEAR_PRIORITY("taskClearPriority"),
    TASK_SEND_TO_BACK("taskSendToBack"),
    TASK_SET_PRIORITY("motionSetPriority"),
    TASK_TOGGLE_DAILY("taskToggleDaily"),
    TASK_HABIT("taskHabit"),
    TASK_TOGGLE_PLANNED_TODAY("taskTogglePlannedToday"),
    TASK_START_FOCUS("taskStartFocus"),
    TASK_MOVE("taskMove"),
    TASK_CONVERT_TO_LIST("taskConvertToList"),
    TASK_EXTRACT_BRANCH("taskExtractBranch"),
    TASK_TOGGLE_INSPECTOR("taskToggleInspector"),
    TASK_INDENT("taskIndent"),
    TASK_OUTDENT("taskOutdent"),
    TASK_MOVE_UP("taskMoveUp"),
    TASK_MOVE_DOWN("taskMoveDown"),
    PLAN_SELECT_NEXT("planSelectNext", "Select the next task", "Plan"),
    PLAN_SELECT_PREVIOUS("planSelectPrevious", "Select the previous task", "Plan"),
    PLAN_ENTER_TASK("planEnterTask"),
    PLAN_TOGGLE_FOLD("planToggleFold"),
    PLAN_FOLD_ALL("planFoldAll"),
    PLAN_UNFOLD_ALL("planUnfoldAll"),
    PLAN_BOARD_NEW_COLUMN("planBoardNewColumn"),
    PLAN_BOARD_MOVE_CARD_LEFT("planBoardMoveCardLeft"),
    PLAN_BOARD_MOVE_CARD_RIGHT("planBoardMoveCardRight"),
    PLAN_BOARD_REMOVE_COLUMN("planBoardRemoveColumn"),
    PLAN_MATRIX_PLACE("planMatrixPlace"),
    LIST_NEW("listNew"),
    LIST_RENAME("listRename"),
    LIST_ARCHIVE("listArchive"),
    LIST_COMPLETE("listComplete"),
    LIST_DELETE("listDelete"),
    LIST_SHARE("listShare"),
    LIST_JOIN("listJoin"),
    FOLDER_NEW("folderNew"),
    WINDOW_UNDO("windowUndo"),
    WINDOW_REDO("windowRedo"),
    WINDOW_HISTORY("windowHistory", "Show the undo history", "Window"),
    WINDOW_SETTINGS("windowSettings", "Open settings", "Window"),
    ;

    private val entry: CommandEntry? get() = KeyCatalog.entry(id)

    val title: String get() = entry?.title ?: androidTitle ?: id
    val group: String get() = entry?.group ?: androidGroup ?: ""

    /** The chords it answers to here, in the order the catalogue lists them. */
    val chords: List<Chord> get() = KeyCatalog.chords(this)

    /** Its keys as a hardware keyboard spells them, e.g. `Ctrl+Shift+Z / Ctrl+Y`. */
    val keys: String get() = chords.map { it.label }.filter { it.isNotEmpty() }.joinToString(" / ")

    /** Whether [event] is one of its chords: on a selected row, where a bare key is a command. */
    fun matches(event: KeyEvent): Boolean = chords.any { it.matches(event) }

    /**
     * Whether [event] is one of its chords that leaves typing alone: for a
     * whole screen, where a bare letter or Enter belongs to whatever field
     * has focus.
     */
    fun matchesShortcut(event: KeyEvent): Boolean = chords.any { !it.typing && it.matches(event) }

    /** Which of its chords [event] is, by position: the digit of `Alt+1…4`. */
    fun chordIndex(event: KeyEvent): Int = chords.indexOfFirst { it.matches(event) }

    /** A palette row for this command. */
    fun palette(run: () -> Unit) = PaletteCommand(id, title, group, keys, run)
}

/** A key event's chord, for matching without repeating the modifier checks everywhere. */
data class Chord(
    val key: Key,
    val ctrl: Boolean = false,
    val shift: Boolean = false,
    val alt: Boolean = false,
    /** How the chord is shown, e.g. `Ctrl+Shift+Z`; empty for a second key of the same chord. */
    val label: String = "",
    /** A bare key a text field would type or edit with. */
    val typing: Boolean = false,
) {
    fun matches(event: KeyEvent): Boolean =
        event.key == key && (event.isCtrlPressed || event.isMetaPressed) == ctrl &&
            event.isShiftPressed == shift && event.isAltPressed == alt
}

/** The core's catalogue, read once, and its keys turned into Compose chords, also once. */
object KeyCatalog {
    private val entries: Map<String, CommandEntry> by lazy { workspaceCommandCatalog().associateBy { it.id } }

    /**
     * Keys Android adds to a command, in the catalogue's spelling. Each stands
     * where the Mac's key is a sequence or held on Control, which a hardware
     * keyboard here cannot type, or is a convention of this platform (Ctrl+Y).
     */
    private val androidKeys: Map<String, List<String>> = mapOf(
        "planSelectNext" to listOf("j", "down"),
        "planSelectPrevious" to listOf("k", "up"),
        "planEnterTask" to listOf("enter"),
        "planToggleFold" to listOf("z"),
        "taskStartFocus" to listOf("f"),
        "taskTogglePlannedToday" to listOf("option+t"),
        "listRename" to listOf("cmd+r"),
        "windowRedo" to listOf("cmd+y"),
        "windowSettings" to listOf("cmd+comma"),
    )

    private val chordCache = HashMap<KeyCommand, List<Chord>>()

    fun entry(id: String): CommandEntry? = entries[id]

    /**
     * The catalogue's two-key sequences a hardware keyboard here can type, by
     * the command each runs: the minus ones (`-1`–`-4`), which are two plain
     * keys here too. A letter pair (`dd`) still has no translation.
     */
    val sequences: Map<String, KeyCommand> by lazy {
        KeyCommand.entries.flatMap { command ->
            entries[command.id]?.keys.orEmpty().filter(::isMinusDigit).map { it to command }
        }.toMap()
    }

    private fun isMinusDigit(token: String) = token.length == 2 && token[0] == '-' && token[1] in '0'..'9'

    /**
     * [event] as a key a [KeySequence] can hold or complete: `-` or a digit,
     * typed bare; empty for anything else, which can only end a hold.
     */
    fun sequenceKey(event: KeyEvent): String {
        if (event.isCtrlPressed || event.isMetaPressed || event.isAltPressed || event.isShiftPressed) return ""
        val code = event.key.nativeKeyCode
        return when {
            event.key == Key.Minus || event.key == Key.NumPadSubtract -> "-"
            code in android.view.KeyEvent.KEYCODE_0..android.view.KeyEvent.KEYCODE_9 ->
                (code - android.view.KeyEvent.KEYCODE_0).toString()
            code in android.view.KeyEvent.KEYCODE_NUMPAD_0..android.view.KeyEvent.KEYCODE_NUMPAD_9 ->
                (code - android.view.KeyEvent.KEYCODE_NUMPAD_0).toString()
            else -> ""
        }
    }

    fun chords(command: KeyCommand): List<Chord> = synchronized(chordCache) {
        chordCache.getOrPut(command) {
            val keys = entries[command.id]?.keys.orEmpty() + androidKeys[command.id].orEmpty()
            keys.flatMap(::chordsFor).distinct()
        }
    }

    /** One catalogue key as Compose chords; none when it has no translation here. */
    fun chordsFor(token: String): List<Chord> {
        val chord = keyboardChord(token) ?: return emptyList()
        if (chord.ctrl) return emptyList()
        val keys = composeKeys(chord.key)
        val label = buildList {
            if (chord.cmd) add("Ctrl")
            if (chord.option) add("Alt")
            if (chord.shift) add("Shift")
            add(keyLabel(chord.key))
        }.joinToString("+")
        return keys.mapIndexed { index, key ->
            Chord(
                key, ctrl = chord.cmd, shift = chord.shift, alt = chord.option,
                label = if (index == 0) label else "", typing = chord.belongsToTyping,
            )
        }
    }

    private fun keyLabel(key: String): String = when (key) {
        "comma" -> ","
        "up" -> "Up"
        "down" -> "Down"
        "left" -> "Left"
        "right" -> "Right"
        "f2" -> "F2"
        "pageup" -> "PageUp"
        "pagedown" -> "PageDown"
        else -> if (key.length == 1) key.uppercase() else key.replaceFirstChar { it.uppercase() }
    }

    private fun composeKeys(key: String): List<Key> = when (key) {
        "up" -> listOf(Key.DirectionUp)
        "down" -> listOf(Key.DirectionDown)
        "left" -> listOf(Key.DirectionLeft)
        "right" -> listOf(Key.DirectionRight)
        "enter" -> listOf(Key.Enter, Key.NumPadEnter)
        "tab" -> listOf(Key.Tab)
        "space" -> listOf(Key.Spacebar)
        "delete" -> listOf(Key.Delete, Key.Backspace)
        "escape" -> listOf(Key.Escape)
        "f2" -> listOf(Key.F2)
        "comma" -> listOf(Key.Comma)
        "home" -> listOf(Key.MoveHome)
        "end" -> listOf(Key.MoveEnd)
        "pageup" -> listOf(Key.PageUp)
        "pagedown" -> listOf(Key.PageDown)
        "-" -> listOf(Key.Minus)
        "=" -> listOf(Key.Equals)
        "/" -> listOf(Key.Slash)
        "[" -> listOf(Key.LeftBracket)
        "]" -> listOf(Key.RightBracket)
        else -> {
            val c = key.singleOrNull()
            when {
                c == null -> emptyList()
                c in 'a'..'z' -> listOf(Key(android.view.KeyEvent.KEYCODE_A + (c - 'a')))
                c in '0'..'9' -> listOf(Key(android.view.KeyEvent.KEYCODE_0 + (c - '0')))
                else -> emptyList()
            }
        }
    }
}
