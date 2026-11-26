package uk.co.maybeitsadam.takt.ui.conditions

import androidx.compose.runtime.Immutable
import uk.co.maybeitsadam.takt.core.TaskCondition
import uk.co.maybeitsadam.takt.core.TaskRequirement
import uk.co.maybeitsadam.takt.data.workspace.ConditionGroup
import uk.co.maybeitsadam.takt.data.workspace.ConditionGroups
import uniffi.takt_core.PaletteCandidate
import uniffi.takt_core.paletteRank

/** One condition as the picker draws it: ticked or not, and what is said beside it. */
@Immutable
data class ConditionPickerRow(
    val condition: TaskCondition,
    val isOn: Boolean,
    /** Under Location, where saying one holds replaces the last: drawn as a radio, not a box. */
    val isExclusive: Boolean,
    /** Its category, shown only while searching, when the rows leave their headings. */
    val category: String?,
    /**
     * Shown but not ticked by hand: a device condition while saying what
     * holds, since it holds by itself on a device of its kind and nowhere else.
     */
    val isFixed: Boolean = false,
)

/** Rows under one heading; a null title is the flat list of matches while searching. */
@Immutable
data class ConditionPickerSection(val title: String?, val rows: List<ConditionPickerRow>)

/** What the picker sheet shows for one query. */
@Immutable
data class ConditionPickerList(
    val sections: List<ConditionPickerSection>,
    /** The name typed, when no condition — archived ones included — is called that yet: offered as "New condition". */
    val newName: String?,
    val isSearching: Boolean,
) {
    val isEmpty: Boolean get() = sections.isEmpty() && newName == null
}

/**
 * The conditions picker on the phones: the Mac's `cc` picker
 * (`WorkspaceConditionPicker.swift`) for a thumb. With nothing typed, every
 * condition on offer under its category heading, as the core groups them;
 * typed into, one list ranked by the core's palette scoring — the name, then
 * the category — so a query lands on the same row on the iPhone. A name
 * nothing is called yet is offered as a new condition.
 */
object ConditionPicker {
    /** From this many conditions on offer the sheet opens with room to search; fewer fit at a glance. */
    const val SEARCH_THRESHOLD = 8

    /**
     * @param offered the headings shown with nothing typed.
     * @param searchable what a query looks through (it may reach beyond [offered]).
     * @param all every condition, for "is that name taken?".
     * @param exclusiveRadio whether Location rows are drawn as radios: only
     *   when saying where you are, where a place replaces the last.
     * @param devicesFixed whether the Device heading's rows are shown without
     *   a toggle: when saying where you are, which says nothing about them.
     *   A task can still require one, so the requirement pickers leave it off.
     */
    fun list(
        offered: List<ConditionGroup>,
        searchable: List<TaskCondition>,
        all: List<TaskCondition>,
        query: String,
        isOn: (String) -> Boolean,
        exclusiveRadio: Boolean,
        devicesFixed: Boolean = false,
    ): ConditionPickerList {
        val needle = query.trim()
        if (needle.isEmpty()) {
            val sections = offered.filter { it.conditions.isNotEmpty() }.map { group ->
                ConditionPickerSection(
                    group.title,
                    group.conditions.map {
                        ConditionPickerRow(it, isOn(it.id), exclusiveRadio && group.isExclusive, null, devicesFixed && group.isDevice)
                    },
                )
            }
            return ConditionPickerList(sections, newName = null, isSearching = false)
        }
        // Headings come from the core, so a row's category reads as it does
        // unsearched, and ties keep the order the headings give.
        val ordered = ConditionGroups.all(searchable).flatMap { group -> group.conditions.map { it to group } }
        val candidates = ordered.map { (condition, group) -> PaletteCandidate(condition.name, group.title, emptyList(), 0) }
        val rows = paletteRank(needle, candidates).map { index ->
            val (condition, group) = ordered[index.toInt()]
            ConditionPickerRow(condition, isOn(condition.id), exclusiveRadio && group.isExclusive, group.title, devicesFixed && group.isDevice)
        }
        val taken = all.any { it.name.equals(needle, ignoreCase = true) }
        return ConditionPickerList(
            sections = if (rows.isEmpty()) emptyList() else listOf(ConditionPickerSection(null, rows)),
            newName = if (taken) null else needle,
            isSearching = true,
        )
    }

    /** What a task needs, in one line: "Home or Campus + not Floor space", or "Anytime". */
    fun requirementSummary(groups: List<List<String>>?, name: (String) -> String): String =
        groups.orEmpty().filter { it.isNotEmpty() }.takeIf { it.isNotEmpty() }
            ?.joinToString(" + ") { group -> TaskRequirement.label(group, name) }
            ?: "Anytime"

    /** The other conditions sharing a group with [id] — "Campus" for Home in Home-or-Campus — or null. */
    fun alternatives(id: String, groups: List<List<String>>?, name: (String) -> String): String? {
        val names = { member: String -> TaskRequirement.parse(member).conditionId == id }
        return groups.orEmpty().firstOrNull { group -> group.any(names) && group.size > 1 }
            ?.filterNot(names)?.let { TaskRequirement.label(it, name) }
    }

    /** What holds now, in the order the headings put it: "Home · Laptop", or null for nothing. */
    fun nowSummary(conditions: List<TaskCondition>, selected: Set<String>): String? =
        ConditionGroups.all(conditions.filter { it.id in selected }).flatMap { it.conditions }
            .takeIf { it.isNotEmpty() }?.joinToString(" · ") { it.name }
}

/**
 * Ticking a condition on or off in a task's requirements, as the Mac's `cc`
 * picker does: a port of `TaskRequirementToggle` (`Sources/TaktCore`), which
 * the iPhone calls directly. A tick is a group of its own — "and needs Floor
 * space" — except for a place: you are in one place at a time, so a second
 * place joins the group the first is in ("Home or Campus") rather than asking
 * for both, which nothing could satisfy. A condition can also be ticked as
 * *not* holding ("not Home", stored `!home`): always a group of its own.
 */
object RequirementToggle {
    /** Whether [id] is required by any group, either way. */
    fun requires(id: String, groups: List<List<String>>?): Boolean = requirement(id, groups) != null

    /** How [id] is first required — as itself, or negated — or null when it is not. */
    fun requirement(id: String, groups: List<List<String>>?): TaskRequirement? =
        groups.orEmpty().asSequence().flatten().map(TaskRequirement::parse).firstOrNull { it.conditionId == id }

    /**
     * [groups] with [id] ticked on, or off if it was already required either
     * way. Ticking off takes it out of every group and drops a group it leaves
     * empty. [places] are the ids of the conditions that are places. Null when
     * nothing is left.
     */
    fun toggling(id: String, groups: List<List<String>>?, places: Set<String>): List<List<String>>? {
        var next = groups.orEmpty()
        if (requires(id, next)) {
            next = removing(id, next)
        } else {
            val placeGroup = if (id in places) next.indexOfFirst { group -> group.all { it in places } } else -1
            next = if (placeGroup >= 0) {
                next.toMutableList().also { it[placeGroup] = it[placeGroup] + id }
            } else {
                next + listOf(listOf(id))
            }
        }
        return next.ifEmpty { null }
    }

    /**
     * [groups] with [id] ticked as not holding: required as itself it turns
     * into "not", and already "not" it is ticked off. Null when nothing is left.
     */
    fun negating(id: String, groups: List<List<String>>?): List<List<String>>? {
        val wasNegated = requirement(id, groups)?.isNegated == true
        val next = removing(id, groups.orEmpty())
        return (if (wasNegated) next else next + listOf(listOf(TaskRequirement(id, isNegated = true).member))).ifEmpty { null }
    }

    private fun removing(id: String, groups: List<List<String>>): List<List<String>> =
        groups.map { group -> group.filter { TaskRequirement.parse(it).conditionId != id } }.filter { it.isNotEmpty() }
}
