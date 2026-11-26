package uk.co.maybeitsadam.takt.ui.conditions

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.app.AppContainer
import uk.co.maybeitsadam.takt.core.TaskCondition
import uk.co.maybeitsadam.takt.data.workspace.ConditionGroups
import uk.co.maybeitsadam.takt.ui.lists.Choice
import uk.co.maybeitsadam.takt.ui.lists.ChoiceSheet
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell

/** The kinds of device a condition can mean, as the core stores them, with what they are called. */
object DeviceKinds {
    val all: List<Pair<String, String>> = listOf("laptop" to "Laptop", "phone" to "Phone", "tablet" to "Tablet")

    /** What a stored kind is called: "Laptop", or the kind itself for one this build does not know. */
    fun label(kind: String?): String? = kind?.let { stored -> all.firstOrNull { it.first == stored }?.second ?: stored }
}

/**
 * Which conditions mean a kind of device. A device condition is never said
 * to hold: it holds by itself on a device of its kind (this one is a phone,
 * or a tablet) and nowhere else, so a task needing "Laptop" waits on the
 * phone and is offered on the Mac at the same moment. Tapping a condition
 * picks its kind, or none; a name nothing is called yet makes one first.
 */
@Composable
fun ConditionDevicesSheet(onDismiss: () -> Unit) {
    val vm = conditionsViewModel()
    val state by vm.state.collectAsStateWithLifecycle()
    var query by rememberSaveable { mutableStateOf("") }
    var choosing by remember { mutableStateOf<TaskCondition?>(null) }
    val live = remember(state.conditions) { state.conditions.filter { !it.isArchived } }
    val list = remember(live, state.conditions, query) {
        ConditionPicker.list(
            offered = ConditionGroups.all(live),
            searchable = live,
            all = state.conditions,
            query = query,
            isOn = { id -> live.firstOrNull { it.id == id }?.isDevice == true },
            exclusiveRadio = false,
        )
    }
    val here = DeviceKinds.label(uniffi.takt_core.thisDeviceKind())
    ConditionPickerSheet(
        title = "Devices",
        subtitle = "A device condition holds by itself on that kind of device." + (here?.let { " This is a ${it.lowercase()}." } ?: ""),
        tag = "condition_devices_sheet",
        query = query,
        onQueryChange = { query = it },
        offeredCount = live.size,
        list = list,
        emptyText = "No conditions yet. Type one — Laptop, Phone — to make it a device.",
        detail = { row -> DeviceKinds.label(row.condition.deviceKind) },
        onToggle = { row -> choosing = row.condition },
        onCreate = { name ->
            query = ""
            vm.createCondition(name) { made -> choosing = made }
        },
        onDismiss = onDismiss,
    )
    choosing?.let { condition ->
        val choices = remember(condition) {
            (
                listOf(Choice(null, "Not a device", isCurrent = !condition.isDevice)) +
                    DeviceKinds.all.map { (kind, label) -> Choice(kind, label, isCurrent = condition.deviceKind == kind) }
                ).toImmutableList()
        }
        ChoiceSheet("${condition.name} means", choices, onDismiss = { choosing = null }) { choice ->
            vm.setDeviceKind(condition, choice.id)
        }
    }
}

/**
 * The conditions a list requires, which every task in it — and in the lists
 * nested under it — inherits: the task's own picker, pointed at the list.
 * [listId] is a standalone list's id or a nested list's task id.
 */
@Composable
fun ListConditionsSheet(listId: String, listName: String, onDismiss: () -> Unit) {
    val container = LocalShell.current.container
    val vm = viewModel(key = "list-conditions-$listId") { ListConditionsViewModel(container, listId) }
    val groups by vm.groups.collectAsStateWithLifecycle()
    TaskConditionsSheet(
        groups = groups,
        onGroupsChange = vm::change,
        onDismiss = onDismiss,
        title = "Everything in $listName needs",
        tag = "list_conditions_sheet",
    )
}

/**
 * A list's requirements while its picker is open. A tick changes them here
 * at once, so the next tick builds on it, and the write follows; writes go
 * one at a time, the latest wanted each time, so quick ticks cannot land out
 * of order. What another device or the CLI changes comes in while nothing
 * of this sheet's is waiting to be written.
 */
class ListConditionsViewModel(private val container: AppContainer, private val listId: String) : ViewModel() {
    private val current = MutableStateFlow<List<List<String>>?>(null)
    private val wanted = MutableStateFlow<List<List<String>>?>(null)
    private var writing = false

    val groups: StateFlow<List<List<String>>?> = current.asStateFlow()

    init {
        viewModelScope.launch {
            container.awaitSession().repository.observeListRequirements(listId).collect { stored ->
                if (!writing && wanted.value == null) current.value = stored.ifEmpty { null }
            }
        }
        viewModelScope.launch {
            wanted.filterNotNull().collect { next ->
                writing = true
                container.undo.performNow(announce = false) { it.setListRequirements(listId, next) }
                if (wanted.value == next) wanted.value = null
                writing = false
            }
        }
    }

    fun change(edit: (List<List<String>>?) -> List<List<String>>?) {
        val next = edit(current.value)
        current.value = next
        wanted.value = next.orEmpty()
    }
}
