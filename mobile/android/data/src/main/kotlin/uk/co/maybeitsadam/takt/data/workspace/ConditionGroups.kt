package uk.co.maybeitsadam.takt.data.workspace

import uk.co.maybeitsadam.takt.core.TaskCondition
import uniffi.takt_core.ConditionHolding
import uniffi.takt_core.ConditionSection
import uniffi.takt_core.ConditionsSaid

/** Conditions under one heading: a category, or "Other" for the uncategorised. */
data class ConditionGroup(
    /** The category, or null for the uncategorised. */
    val category: String?,
    /** What the heading says. */
    val title: String,
    /** Whether saying one holds replaces the rest: the Location heading. */
    val isExclusive: Boolean,
    /** A setting rather than a property: Location, or a category whose conditions provide others. */
    val isSetting: Boolean = isExclusive,
    val conditions: List<TaskCondition>,
    /**
     * The Device heading: conditions that hold on a device of their kind, so
     * nobody says they hold and they cannot be ticked on or off.
     */
    val isDevice: Boolean = false,
)

/**
 * The headings conditions are shown under and what "Where you are" offers.
 * The rules are the Rust core's (`core/src/conditions.rs`), so the phones and
 * the Mac group alike.
 */
object ConditionGroups {
    /** The category places are filed under. */
    const val LOCATION = "Location"

    /** Every condition under its heading: the settings first (Location, then any that provide), then the rest, then "Other". */
    fun all(conditions: List<TaskCondition>): List<ConditionGroup> =
        uniffi.takt_core.conditionSections(conditions.map { it.toRow() }).toGroups(conditions)

    /**
     * What "Where you are" offers: only the conditions an open task requires
     * (and the settings that provide one), plus any that is on so it can be
     * turned off — pass every condition said, provided or off for now as
     * [active]. Empty when nothing needs one.
     */
    fun available(conditions: List<TaskCondition>, required: Set<String>, active: Set<String>): List<ConditionGroup> =
        uniffi.takt_core.availableConditionSections(
            conditions.map { it.toRow() }, required.toList(), active.toList(),
        ).toGroups(conditions)

    /** The categories a condition can be filed under: Location, then every other one in use. */
    fun categories(conditions: List<TaskCondition>): List<String> =
        uniffi.takt_core.conditionCategories(conditions.map { it.toRow() })

    private fun List<ConditionSection>.toGroups(conditions: List<TaskCondition>): List<ConditionGroup> {
        val byId = conditions.associateBy { it.id }
        return map { section ->
            ConditionGroup(
                category = section.category,
                title = section.title,
                isExclusive = section.isExclusive,
                isSetting = section.isSetting,
                isDevice = section.isDevice,
                conditions = section.conditions.mapNotNull { byId[it.id] },
            )
        }
    }
}

/**
 * What holds now: the conditions said, everything they provide ("from
 * Home"), less any provided one switched off for now. The rules are the Rust
 * core's (`core/src/conditions/provides.rs`), the same set the Mac, the
 * iPhone and the CLI check tasks against.
 */
object ConditionHoldings {
    /** Each condition said, provided or switched off, by id. */
    fun holdings(conditions: List<TaskCondition>, said: Set<String>, off: Set<String>): Map<String, ConditionHolding> =
        uniffi.takt_core.conditionHoldings(conditions.map { it.toRow() }, said.sorted(), off.sorted())
            .associateBy { it.conditionId }

    /** The ids that hold: what availability, ranking and the waiting counts are worked out from. */
    fun held(conditions: List<TaskCondition>, said: Set<String>, off: Set<String>): Set<String> =
        uniffi.takt_core.effectiveConditionIds(conditions.map { it.toRow() }, said.sorted(), off.sorted()).toSet()

    /**
     * [said] and [off] with [conditionId] turned on, off, or (null) flipped:
     * a provided one turned off is off for now, a place replaces the last
     * place, and what a replaced setting provided comes back.
     */
    fun switching(
        conditions: List<TaskCondition>,
        said: Set<String>,
        off: Set<String>,
        conditionId: String,
        on: Boolean? = null,
    ): ConditionsSaid = uniffi.takt_core.focusContextSwitch(
        conditions.map { it.toRow() }, ConditionsSaid(said.sorted(), off.sorted()), conditionId, on,
    )

    /** What a condition's row says beside it: "this device", "from Home", "off for now", or nothing. */
    fun note(holding: ConditionHolding?, conditions: List<TaskCondition>): String? = when {
        holding == null -> null
        holding.isDevice -> "this device"
        holding.isOff -> "off for now"
        !holding.isSet && holding.providedBy.isNotEmpty() ->
            "from " + holding.providedBy.joinToString(", ") { id -> conditions.firstOrNull { it.id == id }?.name ?: id }
        else -> null
    }
}
