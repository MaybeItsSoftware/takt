package uk.co.maybeitsadam.takt.core

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.ZoneOffset

enum class NextUpReason(val raw: String, val explanation: String) {
    DAILY("daily", "today's contribution is still outstanding"),
    OVERDUE("overdue", "it is past its due date"),
    DUE_TODAY("dueToday", "it is due today"),
    DUE_SOON("dueSoon", "it is due shortly"),
    TODAY("today", "you put it in Today"),
    IMPORTANCE("importance", "it is important on the matrix"),
    PRIORITY("priority", "it carries a high priority"),
    ORDER("order", "it is next in order"),
    CONDITION("condition", "its required conditions are available now"),
    STARTED("started", "its start time has arrived"),
    DEADLINE_RISK("deadlineRisk", "its remaining work puts the deadline at risk");

    companion object {
        fun of(raw: String): NextUpReason? = entries.firstOrNull { it.raw == raw }
    }
}

enum class FocusTimeMode(val raw: String) {
    PROGRESS("progress"),
    FINISH("finish");

    companion object {
        fun of(raw: String): FocusTimeMode? = entries.firstOrNull { it.raw == raw }
    }
}

data class FocusContext(
    val conditionIDs: Set<String> = emptySet(),
    val endsAt: Instant? = null,
    val mode: FocusTimeMode = FocusTimeMode.PROGRESS,
)

/** Swift's `Date.distantPast` (0001-01-01T00:00:00Z). */
val DISTANT_PAST: Instant = Instant.parse("0001-01-01T00:00:00Z")

data class NextUpCandidate(
    val id: String,
    val title: String,
    val isDailyDueToday: Boolean = false,
    val dueAt: Instant? = null,
    val startAt: Instant? = null,
    val matrixUrgency: Int? = null,
    val matrixImportance: Int? = null,
    val priority: Int? = null,
    val estimateSeconds: Int? = null,
    val kanbanColumn: String? = null,
    val focusRank: Int? = null,
    val sortOrder: Int = 0,
    val createdAt: Instant = DISTANT_PAST,
    val dueDate: String? = null,
    val requirementGroups: List<List<String>> = emptyList(),
    val loggedSeconds: Int = 0,
    val minimumBlockSeconds: Int? = null,
    val requiresSingleSitting: Boolean = false,
    val dailyRemainingSeconds: Int? = null,
    val dailyUnavailable: TaskUnavailableReason? = null,
    /** Its list's place among the prioritised lists, 1 the most important. */
    val listPriority: Int? = null,
    /** How committed its board column is, 0 for Today; the core's `column_rank`. */
    val columnRank: Int? = null,
) {
    val remainingSeconds: Int?
        get() {
            dailyRemainingSeconds?.let { return maxOf(0, it) }
            return estimateSeconds?.let { maxOf(0, it - maxOf(0, loggedSeconds)) }
        }

    fun effectiveDeadline(zone: ZoneId = ZoneId.systemDefault()): Instant? {
        val due = dueDate
        if (due != null) {
            TaskCalendarDate.date(due, zone)?.let { return it.atZone(zone).plusDays(1).toInstant() }
        }
        return dueAt
    }
}

/** Calendar dates stay dates across daylight-saving and time zone changes. */
object TaskCalendarDate {
    fun string(date: Instant, zone: ZoneId = ZoneId.systemDefault()): String {
        val d = date.atZone(zone).toLocalDate()
        return "%04d-%02d-%02d".format(d.year, d.monthValue, d.dayOfMonth)
    }

    /** The start of the named day, or null if it does not round-trip (e.g. `2026-02-31`). */
    fun date(value: String, zone: ZoneId = ZoneId.systemDefault()): Instant? {
        val pieces = value.split("-").mapNotNull { it.toIntOrNull() }
        if (pieces.size != 3) return null
        val date = try {
            LocalDate.of(pieces[0], 1, 1).plusMonths((pieces[1] - 1).toLong()).plusDays((pieces[2] - 1).toLong())
        } catch (_: Exception) {
            return null
        }
        val instant = date.atStartOfDay(zone).toInstant()
        return if (string(instant, zone) == value) instant else null
    }
}

sealed interface TaskUnavailableReason {
    data class StartsLater(val at: Instant) : TaskUnavailableReason
    data class MissingConditions(val groups: List<List<String>>) : TaskUnavailableReason
    data class InsufficientTime(val seconds: Int) : TaskUnavailableReason
    data object NeedsEstimate : TaskUnavailableReason
    data object ExpiredWindow : TaskUnavailableReason
    data object DailyNotScheduled : TaskUnavailableReason
    data object DailyAlreadyMet : TaskUnavailableReason
}

data class BlockedFocusTask(val candidate: NextUpCandidate, val reasons: List<TaskUnavailableReason>) {
    val id: String get() = candidate.id
}

data class FocusRanking(
    val ranked: List<ScoredNextUp>,
    val blocked: List<BlockedFocusTask>,
    val nextEvaluationAt: Instant?,
)

/** Availability and block lengths; the rules are the Rust core's (core/src/focus.rs). */
object TaskAvailabilityPolicy {
    fun reasons(task: NextUpCandidate, context: FocusContext, now: Instant): List<TaskUnavailableReason> =
        uniffi.takt_core.availabilityReasons(task.core, context.core, now.coreMillis).map { it.reason }

    fun plannedSeconds(task: NextUpCandidate, requested: Int?, context: FocusContext, now: Instant): Int =
        uniffi.takt_core.plannedBlockSeconds(task.core, requested?.toLong(), context.core, now.coreMillis).toInt()

    fun suggestedSeconds(task: NextUpCandidate, context: FocusContext, now: Instant): Int =
        uniffi.takt_core.suggestedBlockSeconds(task.core, context.core, now.coreMillis).toInt()
}

data class ScoredNextUp(
    val candidate: NextUpCandidate,
    val score: Double,
    val reason: NextUpReason,
    val explanation: String = reason.explanation,
) {
    val id: String get() = candidate.id

    companion object {
        fun of(candidate: NextUpCandidate, score: Double, reason: NextUpReason, explanation: String?) =
            ScoredNextUp(candidate, score, reason, explanation ?: reason.explanation)
    }
}

/**
 * Availability is evaluated before a deterministic precedence tuple. Numeric
 * scores are retained for compatibility; they never override deadline ordering.
 * The ranking itself is the Rust core's (core/src/ranking.rs), the same one the
 * Mac and iPhone call.
 */
object NextUpSelector {
    const val todayColumnID = "today"
    const val dueHorizonDays = 14

    fun next(candidates: List<NextUpCandidate>, now: Instant = Instant.now(), zone: ZoneId = ZoneId.systemDefault()): ScoredNextUp? =
        rank(candidates, now, zone).firstOrNull()

    fun rank(
        candidates: List<NextUpCandidate>,
        now: Instant = Instant.now(),
        zone: ZoneId = ZoneId.systemDefault(),
        context: FocusContext = FocusContext(),
    ): List<ScoredNextUp> = evaluate(candidates, now, zone, context).ranked

    fun evaluate(
        candidates: List<NextUpCandidate>,
        now: Instant = Instant.now(),
        zone: ZoneId = ZoneId.systemDefault(),
        context: FocusContext = FocusContext(),
    ): FocusRanking {
        // The core hands back copies; the callers' own candidates go back out.
        val originals = candidates.associateBy { it.id }
        fun original(core: uniffi.takt_core.Candidate) = originals[core.id] ?: NextUpCandidate(core)
        val ranking = uniffi.takt_core.rankNextUp(candidates.map { it.core }, now.coreMillis, zone.coreName, context.core)
        return FocusRanking(
            ranked = ranking.ranked.map { it.scored(original(it.candidate)) },
            blocked = ranking.blocked.map { blocked ->
                BlockedFocusTask(original(blocked.candidate), blocked.reasons.map { it.reason })
            },
            nextEvaluationAt = ranking.nextEvaluationAtMs?.let(Instant::ofEpochMilli),
        )
    }

    /**
     * Reads the candidates, plans the day from all of them and ranks them in
     * one call to the Rust core (`next_up::next_up`), as the Mac and iPhone
     * do, so the candidates cross once, ranked, rather than out and back in
     * twice. With a [ladderLimit] the ladder holds its first so many entries
     * plus every task in the day further down.
     */
    fun read(
        core: uniffi.takt_core.CoreWorkspace,
        now: Instant,
        zone: ZoneId,
        context: FocusContext,
        runningId: String?,
        ladderLimit: Int? = null,
    ): Pair<List<DayPlanEntry>, FocusRanking> {
        val read = core.nextUp(now.coreMillis, zone.coreName, context.core, runningId, ladderLimit?.coerceAtLeast(0)?.toUInt())
        // A candidate blocked and ranked never both; each crosses once.
        val ranking = FocusRanking(
            ranked = read.ranked.map { it.scored(NextUpCandidate(it.candidate)) },
            blocked = read.blocked.map { blocked ->
                BlockedFocusTask(NextUpCandidate(blocked.candidate), blocked.reasons.map { it.reason })
            },
            nextEvaluationAt = read.nextEvaluationAtMs?.let(Instant::ofEpochMilli),
        )
        val plan = read.dayPlan.map { DayPlanEntry(it.id, DayPlanReason.of(it.reason) ?: DayPlanReason.PLANNED) }
        return plan to ranking
    }

    fun score(task: NextUpCandidate, now: Instant = Instant.now(), zone: ZoneId = ZoneId.systemDefault()): ScoredNextUp =
        uniffi.takt_core.scoreNextUp(task.core, now.coreMillis, zone.coreName).scored(task)
}

private fun uniffi.takt_core.Scored.scored(candidate: NextUpCandidate): ScoredNextUp =
    ScoredNextUp.of(candidate, score, NextUpReason.of(reason) ?: NextUpReason.ORDER, explanation)

/** Epoch milliseconds, rounded to the nearest, the way the Swift side sends them. */
val Instant.coreMillis: Long
    get() = Math.addExact(Math.multiplyExact(epochSecond, 1000L), Math.round(nano / 1_000_000.0))

/**
 * The zone's name for the core: an IANA name as it is, and a fixed offset as
 * `GMT+0100`, which the core reads as the matching `Etc/GMT` zone.
 */
val ZoneId.coreName: String
    get() {
        val offset = normalized() as? ZoneOffset ?: return id
        val total = offset.totalSeconds
        if (total == 0) return "UTC"
        val magnitude = Math.abs(total)
        return "GMT%s%02d%02d".format(if (total > 0) "+" else "-", magnitude / 3600, magnitude % 3600 / 60)
    }

internal val FocusContext.core: uniffi.takt_core.FocusContext
    get() = uniffi.takt_core.FocusContext(
        conditionIds = conditionIDs.sorted(),
        endsAtMs = endsAt?.coreMillis,
        mode = mode.raw,
    )

internal val NextUpCandidate.core: uniffi.takt_core.Candidate
    get() = uniffi.takt_core.Candidate(
        id = id,
        title = title,
        isDailyDueToday = isDailyDueToday,
        dueAtMs = dueAt?.coreMillis,
        startAtMs = startAt?.coreMillis,
        matrixUrgency = matrixUrgency?.toLong(),
        matrixImportance = matrixImportance?.toLong(),
        priority = priority?.toLong(),
        estimateSeconds = estimateSeconds?.toLong(),
        kanbanColumn = kanbanColumn,
        focusRank = focusRank?.toLong(),
        sortOrder = sortOrder.toLong(),
        createdAtMs = createdAt.coreMillis,
        dueDate = dueDate,
        requirementGroups = requirementGroups,
        loggedSeconds = loggedSeconds.toLong(),
        minimumBlockSeconds = minimumBlockSeconds?.toLong(),
        requiresSingleSitting = requiresSingleSitting,
        dailyRemainingSeconds = dailyRemainingSeconds?.toLong(),
        dailyUnavailable = when (dailyUnavailable) {
            TaskUnavailableReason.DailyNotScheduled -> "dailyNotScheduled"
            TaskUnavailableReason.DailyAlreadyMet -> "dailyAlreadyMet"
            else -> null
        },
        listPriority = listPriority?.toLong(),
        columnRank = columnRank?.toLong(),
    )

/** A candidate the core read from the workspace. */
fun NextUpCandidate(core: uniffi.takt_core.Candidate): NextUpCandidate = NextUpCandidate(
    id = core.id,
    title = core.title,
    isDailyDueToday = core.isDailyDueToday,
    dueAt = core.dueAtMs?.let(Instant::ofEpochMilli),
    startAt = core.startAtMs?.let(Instant::ofEpochMilli),
    matrixUrgency = core.matrixUrgency?.toInt(),
    matrixImportance = core.matrixImportance?.toInt(),
    priority = core.priority?.toInt(),
    estimateSeconds = core.estimateSeconds?.toInt(),
    kanbanColumn = core.kanbanColumn,
    focusRank = core.focusRank?.toInt(),
    sortOrder = core.sortOrder.toInt(),
    createdAt = Instant.ofEpochMilli(core.createdAtMs),
    dueDate = core.dueDate,
    requirementGroups = core.requirementGroups,
    loggedSeconds = core.loggedSeconds.toInt(),
    minimumBlockSeconds = core.minimumBlockSeconds?.toInt(),
    requiresSingleSitting = core.requiresSingleSitting,
    dailyRemainingSeconds = core.dailyRemainingSeconds?.toInt(),
    dailyUnavailable = when (core.dailyUnavailable) {
        "dailyNotScheduled" -> TaskUnavailableReason.DailyNotScheduled
        "dailyAlreadyMet" -> TaskUnavailableReason.DailyAlreadyMet
        else -> null
    },
    listPriority = core.listPriority?.toInt(),
    columnRank = core.columnRank?.toInt(),
)

private val uniffi.takt_core.Unavailable.reason: TaskUnavailableReason
    get() = when (this) {
        is uniffi.takt_core.Unavailable.StartsLater -> TaskUnavailableReason.StartsLater(Instant.ofEpochMilli(atMs))
        is uniffi.takt_core.Unavailable.MissingConditions -> TaskUnavailableReason.MissingConditions(groups)
        is uniffi.takt_core.Unavailable.InsufficientTime -> TaskUnavailableReason.InsufficientTime(seconds.toInt())
        uniffi.takt_core.Unavailable.NeedsEstimate -> TaskUnavailableReason.NeedsEstimate
        uniffi.takt_core.Unavailable.ExpiredWindow -> TaskUnavailableReason.ExpiredWindow
        uniffi.takt_core.Unavailable.DailyNotScheduled -> TaskUnavailableReason.DailyNotScheduled
        uniffi.takt_core.Unavailable.DailyAlreadyMet -> TaskUnavailableReason.DailyAlreadyMet
    }
