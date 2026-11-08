package uk.co.maybeitsadam.takt.app

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** The task whose inspector is open: a sheet on phones, a pane on wide screens. */
class InspectorController {
    private val _taskId = MutableStateFlow<String?>(null)
    val taskId: StateFlow<String?> = _taskId.asStateFlow()

    fun open(taskId: String) {
        _taskId.value = taskId
    }

    fun close() {
        _taskId.value = null
    }
}

/** Where a quick add should file its task; null fields mean the Inbox. */
data class QuickAddRequest(
    val listId: String? = null,
    val parentTaskId: String? = null,
    /** Text to start with, e.g. from a share or the search field. */
    val text: String = "",
    /** Opened from Today: the task is planned for today, so it lands where it was typed. */
    val plansToday: Boolean = false,
    /** Bumped on every request so the same request twice still reopens the sheet. */
    val nonce: Long = System.nanoTime(),
)

/** Opens the quick-add sheet from anywhere: the FAB, the tile, the launcher shortcut, Ctrl+N. */
class QuickAddController {
    private val _contextListId = MutableStateFlow<String?>(null)

    /** The list on screen, if any; the FAB and Ctrl+N file into it rather than the Inbox. */
    val contextListId: StateFlow<String?> = _contextListId.asStateFlow()

    fun setContextList(listId: String?) {
        _contextListId.value = listId
    }

    private val _request = MutableStateFlow<QuickAddRequest?>(null)
    val request: StateFlow<QuickAddRequest?> = _request.asStateFlow()

    fun open(request: QuickAddRequest = QuickAddRequest()) {
        _request.value = request
    }

    fun close() {
        _request.value = null
    }
}

/** What the habit form opens on: [taskId]'s habit, a new one made from it, or (null) a standalone habit. */
data class HabitRequest(val taskId: String? = null, val nonce: Long = System.nanoTime())

/** Opens and closes the habit form, from wherever it is asked for. */
class HabitFormController {
    private val _request = MutableStateFlow<HabitRequest?>(null)
    val request: StateFlow<HabitRequest?> = _request.asStateFlow()

    fun open(taskId: String? = null) {
        _request.value = HabitRequest(taskId)
    }

    fun close() {
        _request.value = null
    }
}

/** A create prompt the Lists tree should open when it is next on screen: Find's "Create a list". */
enum class ListsPrompt { NEW_LIST, NEW_FOLDER }

/** Carries a [ListsPrompt] from wherever it was asked for to the Lists tree, which takes it once. */
class ListsPromptController {
    private val _pending = MutableStateFlow<ListsPrompt?>(null)
    val pending: StateFlow<ListsPrompt?> = _pending.asStateFlow()

    fun request(prompt: ListsPrompt) {
        _pending.value = prompt
    }

    /** The prompt, once: the tree opens it and clears it. */
    fun take(): ListsPrompt? = _pending.value.also { _pending.value = null }

    private val _join = MutableStateFlow<String?>(null)

    /** An invite to join (a followed `takt://join/…` link, or "" from a button): the shell opens the join sheet with it. */
    val join: StateFlow<String?> = _join.asStateFlow()

    fun requestJoin(text: String) {
        _join.value = text
    }

    /** The join sheet closed. */
    fun clearJoin() {
        _join.value = null
    }
}
