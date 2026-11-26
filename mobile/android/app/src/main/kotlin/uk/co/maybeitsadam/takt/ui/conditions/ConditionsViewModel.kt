package uk.co.maybeitsadam.takt.ui.conditions

import androidx.compose.runtime.Immutable
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import java.time.Instant
import java.time.ZoneId
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.ImmutableMap
import kotlinx.collections.immutable.ImmutableSet
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.persistentMapOf
import kotlinx.collections.immutable.persistentSetOf
import kotlinx.collections.immutable.toImmutableList
import kotlinx.collections.immutable.toImmutableMap
import kotlinx.collections.immutable.toImmutableSet
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.app.AppContainer
import uk.co.maybeitsadam.takt.core.TaskCondition
import uk.co.maybeitsadam.takt.data.workspace.ConditionGroup
import uk.co.maybeitsadam.takt.data.workspace.ConditionGroups
import uk.co.maybeitsadam.takt.data.workspace.ConditionHoldings
import uniffi.takt_core.ConditionHolding
import uk.co.maybeitsadam.takt.data.workspace.emptyFocusContext
import uk.co.maybeitsadam.takt.data.workspace.observeDayStart
import uk.co.maybeitsadam.takt.ui.today.minuteTicks

/** The conditions, how many open tasks need each, and which hold now. */
@Immutable
data class ConditionsState(
    val conditions: ImmutableList<TaskCondition> = persistentListOf(),
    /** Open tasks requiring each condition, by id; one nothing needs is absent. */
    val usage: ImmutableMap<String, Int> = persistentMapOf(),
    /** What holds now, as the workspace keeps it, with an expired context set aside. */
    val selected: ImmutableSet<String> = persistentSetOf(),
    /** Provided conditions switched off for now. */
    val off: ImmutableSet<String> = persistentSetOf(),
    /** What holds: [selected], everything it provides, less [off]. The core's rule. */
    val held: ImmutableSet<String> = persistentSetOf(),
    /** Each condition said, provided or off, by id. */
    val holdings: ImmutableMap<String, ConditionHolding> = persistentMapOf(),
    /** Said on an earlier day: probably not where you are now. The core's rule. */
    val isStale: Boolean = false,
    /** How each day starts, as the account keeps it; set on the Mac or the CLI. */
    val dayStart: uniffi.takt_core.DayStartSettings? = null,
    val isLoaded: Boolean = false,
) {
    /**
     * One line on how each day starts: "Each day starts at 04:00 with Home",
     * or null while what holds carries over.
     */
    val dayStartSummary: String? get() {
        val day = dayStart?.takeIf { it.resets } ?: return null
        val names = conditions.filter { it.id in day.conditionIds && !it.isArchived }.map { it.name }
        val hour = "%02d:00".format(day.hour)
        return if (names.isEmpty()) "Each day starts at $hour with nothing on" else "Each day starts at $hour with ${names.joinToString(" · ")}"
    }

    val live: List<TaskCondition> get() = conditions.filter { !it.isArchived }

    /** What "where you are" offers: what an open task needs, and whatever is on. The core's rule. */
    val available: List<ConditionGroup> get() = ConditionGroups.available(conditions, usage.keys, holdings.keys)

    /**
     * What a condition's row says beside it: "this device" or "not this
     * device" for a device condition, "from Home", "off for now", or nothing.
     */
    fun note(id: String): String? = ConditionHoldings.note(holdings[id], conditions)
        ?: if (conditions.firstOrNull { it.id == id }?.isDevice == true) "not this device" else null

    fun name(id: String): String = conditions.firstOrNull { it.id == id }?.name ?: "Missing condition"
}

/**
 * The conditions sheets' state: saying what holds now (shared through the
 * workspace's focus context with Focus, the Mac, the iPhone and the agent),
 * and making a condition from the sheet's search field.
 */
class ConditionsViewModel(private val container: AppContainer) : ViewModel() {
    val state: StateFlow<ConditionsState> = container.withSession { session ->
        val repo = session.repository
        val workspace = session.workspace.id
        combine(
            repo.observeConditions(workspace),
            repo.observeConditionUsage(workspace),
            repo.observeFocusContext(workspace),
            repo.observeDayStart(),
            minuteTicks(),
        ) { conditions, usage, saved, dayStart, _ ->
            val now = Instant.now()
            val context = saved?.let { uniffi.takt_core.focusContextInForce(it, now.toEpochMilli()) } ?: emptyFocusContext()
            val said = context.conditionIds.toSet()
            val off = context.offIds.toSet()
            ConditionsState(
                conditions = conditions.toImmutableList(),
                usage = usage.toImmutableMap(),
                selected = said.toImmutableSet(),
                off = off.toImmutableSet(),
                held = ConditionHoldings.held(conditions, said, off).toImmutableSet(),
                holdings = ConditionHoldings.holdings(conditions, said, off).toImmutableMap(),
                isStale = uniffi.takt_core.focusContextIsStale(context, now.toEpochMilli(), ZoneId.systemDefault().id),
                dayStart = dayStart,
                isLoaded = true,
            )
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ConditionsState())

    /**
     * Says [condition] holds now, or no longer does. A place replaces the
     * last, and a provided one turned off is off for now: the core's rules,
     * as Focus's chips.
     */
    fun toggleNow(condition: TaskCondition) {
        // A device condition holds by itself on its kind of device; the core
        // would ignore saying it, so nothing is saved.
        if (condition.isDevice) return
        val current = state.value
        val next = ConditionHoldings.switching(current.conditions, current.selected, current.off, condition.id)
        saveContext { it.copy(conditionIds = next.conditionIds, offIds = next.offIds, suggestedIds = emptyList()) }
    }

    /** Nothing holds any more: every condition off. */
    fun clearNow() {
        saveContext { it.copy(conditionIds = emptyList(), offIds = emptyList(), suggestedIds = emptyList()) }
    }

    /**
     * Makes a condition with no category (filed later, as on the Mac), one
     * undo step, then hands it to [then] — to turn it on, or require it.
     */
    fun createCondition(name: String, then: (TaskCondition) -> Unit) {
        val trimmed = name.trim()
        if (trimmed.isEmpty()) return
        viewModelScope.launch {
            var made: TaskCondition? = null
            container.undo.performNow { repo -> made = repo.createCondition(container.awaitSession().workspace.id, trimmed, null) }
            made?.let(then)
        }
    }

    /** Marks [condition] as meaning a kind of device ("laptop", "phone", "tablet"), or none: one undo step. */
    fun setDeviceKind(condition: TaskCondition, kind: String?) {
        container.undo.perform { it.setConditionDeviceKind(condition.id, kind) }
    }

    /** Saves the context into the workspace, changed by [change]. Not an undo step. */
    private fun saveContext(change: (uniffi.takt_core.StoredFocusContext) -> uniffi.takt_core.StoredFocusContext) {
        viewModelScope.launch {
            val session = container.awaitSession()
            val repo = session.repository
            val current = repo.focusContext(session.workspace.id) ?: emptyFocusContext()
            repo.saveFocusContext(session.workspace.id, change(current))
        }
    }
}
