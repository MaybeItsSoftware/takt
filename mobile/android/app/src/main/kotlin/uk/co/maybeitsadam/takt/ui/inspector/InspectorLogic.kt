package uk.co.maybeitsadam.takt.ui.inspector

import java.time.DayOfWeek
import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime
import java.time.YearMonth
import java.time.ZoneId
import java.time.temporal.TemporalAdjusters
import uk.co.maybeitsadam.takt.core.MatrixGeometry
import uk.co.maybeitsadam.takt.core.MatrixQuadrant
import uk.co.maybeitsadam.takt.core.PeriodicSchedule
import uk.co.maybeitsadam.takt.core.TaskCalendarDate
import uk.co.maybeitsadam.takt.core.TaskMatrixPosition
import uk.co.maybeitsadam.takt.core.TaskPlanningError
import uk.co.maybeitsadam.takt.data.workspace.TaskDependencies
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorDraft
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorError
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorException
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorField
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorValues
import uk.co.maybeitsadam.takt.data.workspace.TaskSearchResult

// The inspector's pure pieces: date and time field arithmetic, due editing,
// repeat presets, requirement-group reducers and pre-save validation. Kept
// free of Compose so they are tested on the JVM.

object DateFieldMath {
    /**
     * The month as rows of seven, [firstDayOfWeek] first, with nulls padding
     * the days that belong to the neighbouring months.
     */
    fun monthGrid(month: YearMonth, firstDayOfWeek: DayOfWeek = DayOfWeek.MONDAY): List<List<LocalDate?>> {
        val first = month.atDay(1)
        val lead = Math.floorMod(first.dayOfWeek.value - firstDayOfWeek.value, 7)
        val cells = ArrayList<LocalDate?>()
        repeat(lead) { cells += null }
        for (day in 1..month.lengthOfMonth()) cells += month.atDay(day)
        while (cells.size % 7 != 0) cells += null
        return cells.chunked(7)
    }

    /** The weekday initials over the grid, in the grid's order. */
    fun weekdayHeaders(firstDayOfWeek: DayOfWeek = DayOfWeek.MONDAY): List<String> =
        (0 until 7).map { firstDayOfWeek.plus(it.toLong()) }.map {
            it.name.take(2).lowercase().replaceFirstChar(Char::uppercase)
        }

    /** Hours wrap round the day without touching the date. */
    fun stepHour(time: LocalTime, by: Int): LocalTime = time.withSecond(0).withNano(0).plusHours(by.toLong())

    /**
     * Minutes move in [step]s and snap to the step on the first press (09:07 up
     * is 09:10, down is 09:05); they wrap within the hour, leaving it alone.
     */
    fun stepMinute(time: LocalTime, by: Int, step: Int = 5): LocalTime {
        val minute = time.minute
        val snapped = when {
            by > 0 && minute % step != 0 -> (minute / step) * step + step * by
            by < 0 && minute % step != 0 -> (minute / step) * step + step * (by + 1)
            else -> minute + step * by
        }
        return LocalTime.of(time.hour, Math.floorMod(snapped, 60))
    }

    fun combine(date: LocalDate, time: LocalTime, zone: ZoneId): Instant = date.atTime(time).atZone(zone).toInstant()

    fun date(instant: Instant, zone: ZoneId): LocalDate = instant.atZone(zone).toLocalDate()

    fun time(instant: Instant, zone: ZoneId): LocalTime = instant.atZone(zone).toLocalTime().withSecond(0).withNano(0)

    /** The same wall-clock time on another day. */
    fun withDate(instant: Instant, date: LocalDate, zone: ZoneId): Instant = combine(date, time(instant, zone), zone)

    /** Another time on the same day. */
    fun withTime(instant: Instant, time: LocalTime, zone: ZoneId): Instant = combine(date(instant, zone), time, zone)

    /** The month a step of the grid's arrows lands on. */
    fun stepMonth(month: YearMonth, by: Int): YearMonth = month.plusMonths(by.toLong())

    /** Tomorrow at nine: when a start is added without saying when. */
    fun defaultStart(now: Instant, zone: ZoneId): Instant =
        combine(date(now, zone).plusDays(1), LocalTime.of(9, 0), zone)

    /** The time an exact due takes when switched on from a day. */
    val defaultDueTime: LocalTime = LocalTime.of(17, 0)
}

/** Due is nothing, a planning day (`dueDate`, a calendar date), or an exact time (`dueAt`). */
enum class DueMode { NONE, DAY, TIME }

object DueEditing {
    fun mode(values: TaskEditorValues): DueMode = when {
        values.dueAt != null -> DueMode.TIME
        values.dueDate != null -> DueMode.DAY
        else -> DueMode.NONE
    }

    /** The day due falls on, whichever way it is held. */
    fun day(values: TaskEditorValues, zone: ZoneId): LocalDate? =
        values.dueAt?.let { DateFieldMath.date(it, zone) }
            ?: values.dueDate?.let { raw -> TaskCalendarDate.date(raw, zone)?.let { DateFieldMath.date(it, zone) } }

    /** Moves due to [date], keeping an exact time's clock; null clears it. */
    fun setDay(values: TaskEditorValues, date: LocalDate?, zone: ZoneId): TaskEditorValues {
        if (date == null) return values.copy(dueAt = null, dueDate = null)
        val dueAt = values.dueAt
        return if (dueAt != null) {
            values.copy(dueAt = DateFieldMath.withDate(dueAt, date, zone), dueDate = null)
        } else {
            values.copy(dueDate = TaskCalendarDate.string(date.atStartOfDay(zone).toInstant(), zone), dueAt = null)
        }
    }

    fun setTime(values: TaskEditorValues, time: LocalTime, zone: ZoneId): TaskEditorValues {
        val day = day(values, zone) ?: return values
        return values.copy(dueAt = DateFieldMath.combine(day, time, zone), dueDate = null)
    }

    /** The desktop's "At a set time" switch: a day becomes that day at five, and back. */
    fun setExact(values: TaskEditorValues, exact: Boolean, zone: ZoneId, today: LocalDate): TaskEditorValues {
        val day = day(values, zone) ?: today
        return if (exact) {
            if (values.dueAt != null) values
            else values.copy(dueAt = DateFieldMath.combine(day, DateFieldMath.defaultDueTime, zone), dueDate = null)
        } else {
            values.copy(dueDate = TaskCalendarDate.string(day.atStartOfDay(zone).toInstant(), zone), dueAt = null)
        }
    }
}

/** A repeat the inspector offers in one tap; [rule] is what `recurrenceRule` stores. */
data class RecurrencePreset(val rule: String, val label: String)

object RecurrencePresets {
    val all: List<RecurrencePreset> = listOf(
        RecurrencePreset("daily", "Daily"),
        RecurrencePreset("weekdays", "Weekdays"),
        RecurrencePreset("weekly", "Weekly"),
        RecurrencePreset("every 2 weeks", "Every 2 weeks"),
        RecurrencePreset("every 3 days", "Every 3 days"),
    )

    /** "Every <weekday>" for the day due falls on (Monday when there is none). */
    fun weekdayPreset(day: LocalDate?): RecurrencePreset {
        val weekday = (day ?: LocalDate.of(2024, 1, 1)).dayOfWeek
        val name = weekday.name.lowercase()
        return RecurrencePreset("every $name", "Every ${name.replaceFirstChar(Char::uppercase)}")
    }

    fun presets(dueDay: LocalDate?): List<RecurrencePreset> = all + weekdayPreset(dueDay)

    /** The preset [raw] amounts to, by cadence, so `every week` selects Weekly. */
    fun selected(raw: String, presets: List<RecurrencePreset>): RecurrencePreset? {
        val cadence = PeriodicSchedule.parse(raw)?.cadence ?: return null
        return presets.firstOrNull { PeriodicSchedule.parse(it.rule)?.cadence == cadence }
    }

    /** What the raw rule means, in PeriodicSchedule's words. */
    fun describe(raw: String): RuleDescription {
        if (raw.isBlank()) return RuleDescription("Does not repeat", valid = true)
        val schedule = PeriodicSchedule.parse(raw)
            ?: return RuleDescription("Not a repeat the app understands. Try daily, weekdays, every 2 weeks or every monday.", valid = false)
        return RuleDescription("Repeats: ${schedule.displayLabel}", valid = true)
    }
}

data class RuleDescription(val text: String, val valid: Boolean)

/**
 * Edits to `requirementGroups`: every group must be met, any one condition
 * within a group will do. Empty groups drop out, and no groups at all is null,
 * as `TaskPlanning.normalized` stores it.
 */
object RequirementGroups {
    /** A new group holding just [conditionId]; refused if that exact group exists. */
    fun addGroup(groups: List<List<String>>?, conditionId: String): List<List<String>>? {
        val current = groups.orEmpty()
        if (current.any { it == listOf(conditionId) }) return groups
        return current + listOf(listOf(conditionId))
    }

    /** "Or …": another way to meet group [index]. */
    fun addAlternative(groups: List<List<String>>?, index: Int, conditionId: String): List<List<String>>? {
        val current = groups.orEmpty()
        if (index !in current.indices || conditionId in current[index]) return groups
        val updated = current.toMutableList().also { it[index] = it[index] + conditionId }
        // Two groups with the same members are one requirement twice; the store refuses that.
        if (updated.map { it.sorted() }.toSet().size != updated.size) return groups
        return updated
    }

    fun remove(groups: List<List<String>>?, index: Int, conditionId: String): List<List<String>>? {
        val current = groups.orEmpty()
        if (index !in current.indices) return groups
        val updated = current.toMutableList().also { it[index] = it[index] - conditionId }
            .filter { it.isNotEmpty() }
            .distinctBy { it.sorted() }
        return updated.ifEmpty { null }
    }

    fun removeGroup(groups: List<List<String>>?, index: Int): List<List<String>>? {
        val current = groups.orEmpty()
        if (index !in current.indices) return groups
        return current.filterIndexed { i, _ -> i != index }.ifEmpty { null }
    }

    /** Conditions that could start a new group: live ones not already a group on their own. */
    fun <C> candidatesForNewGroup(groups: List<List<String>>?, conditions: List<C>, id: (C) -> String, archived: (C) -> Boolean): List<C> =
        conditions.filter { !archived(it) && groups.orEmpty().none { g -> g == listOf(id(it)) } }

    fun <C> candidatesForGroup(group: List<String>, conditions: List<C>, id: (C) -> String, archived: (C) -> Boolean): List<C> =
        conditions.filter { !archived(it) && id(it) !in group }
}

/** A field the inspector will not save yet, and why. */
data class InspectorProblem(val field: TaskEditorField?, val message: String)

/**
 * What the store would refuse, checked before Save so the message sits beside
 * the field. Mirrors `TaskEditorDraft.validatedSnapshot` and `updatePlanning`.
 */
object InspectorValidation {
    fun problems(draft: TaskEditorDraft, zone: ZoneId): List<InspectorProblem> {
        val values = draft.values
        val baseline = draft.baseline
        val problems = mutableListOf<InspectorProblem>()
        if (values.title.isBlank()) problems += InspectorProblem(TaskEditorField.TITLE, "A task needs a title.")

        val estimate: Int? = if (values.estimateMinutes != baseline.values.estimateMinutes) {
            try {
                TaskEditorValues.parseEstimate(values.estimateMinutes)
            } catch (_: TaskEditorException) {
                problems += InspectorProblem(TaskEditorField.ESTIMATE_MINUTES, TaskEditorError.INVALID_ESTIMATE.message)
                null
            }
        } else {
            baseline.estimateSeconds
        }

        val minimum: Int? = if (values.minimumBlockMinutes != baseline.values.minimumBlockMinutes) {
            try {
                TaskEditorValues.parseEstimate(values.minimumBlockMinutes ?: "")
            } catch (_: TaskEditorException) {
                problems += InspectorProblem(TaskEditorField.MINIMUM_BLOCK, TaskPlanningError.INVALID_MINIMUM.message)
                null
            }
        } else {
            baseline.planning?.minimumBlockSeconds
        }
        if (minimum != null && minimum < 60) {
            problems += InspectorProblem(TaskEditorField.MINIMUM_BLOCK, TaskPlanningError.INVALID_MINIMUM.message)
        }
        if (values.requiresSingleSitting == true && (estimate == null || estimate <= 0 || estimate < (minimum ?: 60))) {
            problems += InspectorProblem(TaskEditorField.SINGLE_SITTING, TaskPlanningError.ESTIMATE_REQUIRED.message)
        }

        val scheduleChanged = values.startAt != baseline.values.startAt || values.dueDate != baseline.values.dueDate ||
            values.dueAt != baseline.values.dueAt
        val start = values.startAt
        val deadline = values.dueDate?.let { TaskCalendarDate.date(it, zone) }?.atZone(zone)?.plusDays(1)?.toInstant()
            ?: values.dueAt.takeIf { values.dueDate == null }
        if (scheduleChanged && start != null && deadline != null && start >= deadline) {
            problems += InspectorProblem(TaskEditorField.START_AT, TaskPlanningError.INVALID_SCHEDULE.message)
        }
        if (values.priority !in -4..4) problems += InspectorProblem(TaskEditorField.PRIORITY, TaskEditorError.INVALID_PRIORITY.message)
        return problems
    }
}

/** The matrix picker: a quadrant, or unplaced. */
object MatrixPicker {
    fun quadrant(position: TaskMatrixPosition): MatrixQuadrant? {
        val urgency = position.urgency ?: return null
        val importance = position.importance ?: return null
        if (!MatrixGeometry.isPlaced(urgency.toDouble(), importance.toDouble())) return null
        return MatrixGeometry.quadrant(urgency.toDouble(), importance.toDouble())
    }

    /** The middle of the box, which is where the Mac's matrix drops a card. */
    fun position(quadrant: MatrixQuadrant?): TaskMatrixPosition {
        if (quadrant == null) return TaskMatrixPosition(null, null)
        val coordinate = quadrant.representativeCoordinate
        return TaskMatrixPosition(coordinate.urgency.toInt(), coordinate.importance.toInt())
    }
}

/** Links typed one per line, as the store will keep them. */
fun linkLines(raw: String): List<String> = raw.split(Regex("\\R")).map { it.trim() }.filter { it.isNotEmpty() }

/** Tags typed comma-separated, as the store will keep them. */
fun tagList(raw: String): List<String> {
    val seen = LinkedHashMap<String, String>()
    for (tag in raw.split(",").map { it.trim().removePrefix("#").trim() }.filter { it.isNotEmpty() }) {
        seen.putIfAbsent(tag.lowercase(), tag)
    }
    return seen.values.toList()
}

fun joinTags(tags: List<String>): String = tags.joinToString(", ")

/**
 * Priority as the capture syntax writes it: `!1` (most urgent) to `!4`, or
 * none. 1 is the most urgent everywhere, as the colours say: raspberry, amber,
 * azure, muted. `!-4` to `!-1` rank behind every task with none, `!-1` the
 * very back, and are muted: they mean later.
 */
object PriorityLabels {
    val order: List<Int> = listOf(0, 1, 2, 3, 4)

    /** The second row of the picker: behind no priority, -1 last. */
    val later: List<Int> = listOf(-4, -3, -2, -1)

    fun isPriority(priority: Int?): Boolean = priority != null && (priority in 1..4 || priority in -4..-1)

    fun label(priority: Int): String = if (isPriority(priority)) "!$priority" else "None"

    /** What a screen reader says: "Priority !2", "Priority -1, last". */
    fun description(priority: Int): String = when {
        priority == -1 -> "Priority -1, last"
        priority < 0 && isPriority(priority) -> "Priority $priority, behind no priority"
        else -> "Priority ${label(priority)}"
    }
}

/** The daily's schedule controls: weekday chips, an every-N-days stepper, a target stepper. */
object DailySchedule {
    /** Monday first, in Calendar numbering (1 = Sunday), with the chip's label. */
    val weekdayChips: List<Pair<Int, String>> = listOf(
        2 to "Mon", 3 to "Tue", 4 to "Wed", 5 to "Thu", 6 to "Fri", 7 to "Sat", 1 to "Sun",
    )

    /** Toggles [weekday]; the last day left cannot be switched off. */
    fun toggle(weekdays: Set<Int>, weekday: Int): Set<Int> {
        val updated = if (weekday in weekdays) weekdays - weekday else weekdays + weekday
        return updated.ifEmpty { weekdays }
    }

    /** Every N days, kept within the store's 1...366. */
    fun stepInterval(days: Int, by: Int): Int = (days + by).coerceIn(1, 366)

    /** The target moves in five-minute steps, snapping first; stepping to nothing clears it. */
    fun stepTarget(seconds: Int?, by: Int, stepMinutes: Int = 5): Int? {
        val minutes = (seconds ?: 0) / 60
        val snapped = when {
            minutes % stepMinutes == 0 -> minutes + by * stepMinutes
            by > 0 -> (minutes / stepMinutes) * stepMinutes + by * stepMinutes
            else -> (minutes / stepMinutes) * stepMinutes + (by + 1) * stepMinutes
        }
        return if (snapped <= 0) null else minOf(snapped, 24 * 60) * 60
    }
}

/** The Monday on or before [date]. */
fun weekStart(date: LocalDate): LocalDate = date.with(TemporalAdjusters.previousOrSame(DayOfWeek.MONDAY))

/** The "Blocked by" picker's choices among search results. */
object BlockerPicker {
    const val LIMIT = 8

    /**
     * Results [taskId] could wait on: tasks (not lists), not itself, not one it
     * already waits on, and not one waiting on it directly (a cycle the store
     * would refuse; a longer one it refuses with its own message).
     */
    fun candidates(
        results: List<TaskSearchResult>,
        taskId: String,
        dependencies: TaskDependencies,
        limit: Int = LIMIT,
    ): List<TaskSearchResult> {
        val excluded = HashSet<String>().apply {
            add(taskId)
            dependencies.blockers.mapTo(this) { it.id }
            dependencies.blocking.mapTo(this) { it.id }
        }
        return results.asSequence().filter { !it.task.isList && it.task.id !in excluded }.take(limit).toList()
    }
}
