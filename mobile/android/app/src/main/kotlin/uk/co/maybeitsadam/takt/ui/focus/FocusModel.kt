package uk.co.maybeitsadam.takt.ui.focus

import androidx.compose.runtime.Immutable
import java.time.Instant
import java.time.LocalTime
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlin.math.ceil
import uk.co.maybeitsadam.takt.core.FocusPoints
import uk.co.maybeitsadam.takt.core.FocusSession
import uk.co.maybeitsadam.takt.core.NextUpCandidate
import uk.co.maybeitsadam.takt.core.NextUpReason
import uk.co.maybeitsadam.takt.core.ScoredNextUp
import uk.co.maybeitsadam.takt.core.TaskAvailabilityPolicy
import uk.co.maybeitsadam.takt.core.TaskCondition
import uk.co.maybeitsadam.takt.core.TaskRequirement
import uk.co.maybeitsadam.takt.core.coreMillis
import uk.co.maybeitsadam.takt.core.coreName
import uk.co.maybeitsadam.takt.core.FocusContext
import uk.co.maybeitsadam.takt.core.TaskUnavailableReason
import uk.co.maybeitsadam.takt.ui.components.Format

/** When a deferred task comes back. The Mac's `WorkspaceDeferral`. */
enum class FocusDeferral(val title: String) {
    AN_HOUR("In an hour"),
    THIS_AFTERNOON("This afternoon"),
    TOMORROW("Tomorrow morning"),
    NEXT_WEEK("Next week"),
    ;

    /** The core's arithmetic (`focus_deferral_date`), shared with the iPhone. */
    fun date(now: Instant, zone: ZoneId = ZoneId.systemDefault()): Instant {
        val kind = when (this) {
            AN_HOUR -> uniffi.takt_core.FocusDeferral.AN_HOUR
            THIS_AFTERNOON -> uniffi.takt_core.FocusDeferral.THIS_AFTERNOON
            TOMORROW -> uniffi.takt_core.FocusDeferral.TOMORROW
            NEXT_WEEK -> uniffi.takt_core.FocusDeferral.NEXT_WEEK
        }
        val nowMs = now.coreMillis
        val ms = uniffi.takt_core.focusDeferralDate(kind, nowMs, zone.coreName)
        // "An hour from now" keeps `now`'s sub-millisecond part.
        return if (ms - nowMs == 3_600_000L) now.plusSeconds(3_600) else Instant.ofEpochMilli(ms)
    }
}

/** The context's time limit, as chosen. */
sealed interface AvailableTime {
    data object Unlimited : AvailableTime
    data class Minutes(val minutes: Int) : AvailableTime
    data class Until(val time: LocalTime) : AvailableTime
}

/** The text and arithmetic behind the focus screen, kept pure for the JVM tests. */
object FocusText {
    val presetMinutes = listOf(15, 30, 60)

    /** When the time chosen runs out; "until" a time already past means that time tomorrow. */
    fun endsAt(time: AvailableTime, chosenAt: Instant, zone: ZoneId = ZoneId.systemDefault()): Instant? = when (time) {
        AvailableTime.Unlimited -> null
        is AvailableTime.Minutes -> chosenAt.plusSeconds(maxOf(1, time.minutes) * 60L)
        is AvailableTime.Until -> {
            val local = chosenAt.atZone(zone)
            val today = local.toLocalDate().atTime(time.time).atZone(zone).toInstant()
            if (today > chosenAt) today else local.toLocalDate().plusDays(1).atTime(time.time).atZone(zone).toInstant()
        }
    }

    /** The time control's label: `Any length`, or `25m left · until 14:30`. */
    fun timeTitle(endsAt: Instant?, now: Instant, zone: ZoneId = ZoneId.systemDefault()): String {
        if (endsAt == null) return "Any length"
        val minutes = maxOf(0L, (endsAt.epochSecond - now.epochSecond) / 60)
        return "${minutes}m left · until ${Format.time(endsAt, zone)}"
    }

    fun unavailable(reason: TaskUnavailableReason, conditions: List<TaskCondition>, zone: ZoneId = ZoneId.systemDefault()): String =
        when (reason) {
            is TaskUnavailableReason.StartsLater -> "Starts ${Format.day(reason.at, zone)} ${Format.time(reason.at, zone)}"
            is TaskUnavailableReason.MissingConditions -> "Needs " + reason.groups.joinToString(" and ") { group ->
                TaskRequirement.label(group) { id -> conditions.firstOrNull { it.id == id }?.name ?: "Missing condition" }
            }
            is TaskUnavailableReason.InsufficientTime -> "Needs at least ${ceil(reason.seconds / 60.0).toInt()} minutes"
            TaskUnavailableReason.NeedsEstimate -> "Needs a remaining estimate to check it fits"
            TaskUnavailableReason.ExpiredWindow -> "Your available time has run out"
            TaskUnavailableReason.DailyNotScheduled -> "This daily isn't scheduled today"
            TaskUnavailableReason.DailyAlreadyMet -> "Today's daily commitment is already met"
        }

    /** The ranking's reason, with the conditions named when they are the reason. */
    fun explanation(rung: ScoredNextUp, selected: Set<String>, conditions: List<TaskCondition>): String {
        if (rung.reason != NextUpReason.CONDITION) return rung.explanation.capitalisedFirst()
        val names = rung.candidate.requirementGroups.joinToString(" + ") { group ->
            TaskRequirement.label(group.filter { TaskRequirement.parse(it).isMet(selected) }) { id ->
                conditions.firstOrNull { it.id == id }?.name ?: "Missing condition"
            }
        }
        return "$names available now"
    }

    /**
     * Why starting [candidate] for [plannedSeconds] would go against the
     * context; empty when it is fine. Mirrors the store's own refusal, so the
     * user is asked before it is refused.
     */
    fun startObjections(
        candidate: NextUpCandidate?,
        context: FocusContext,
        plannedSeconds: Int,
        now: Instant,
        conditions: List<TaskCondition>,
    ): List<String> {
        if (candidate == null) return listOf("This task isn't currently available for focus.")
        val result = TaskAvailabilityPolicy.reasons(candidate, context, now).map { unavailable(it, conditions) }.toMutableList()
        val end = context.endsAt
        if (end != null && plannedSeconds > end.epochSecond - now.epochSecond) result += "The block is longer than the time you have."
        if (plannedSeconds < maxOf(60, candidate.minimumBlockSeconds ?: 60) ||
            (candidate.requiresSingleSitting && plannedSeconds < (candidate.remainingSeconds ?: Int.MAX_VALUE))
        ) {
            result += "The block is shorter than this task needs."
        }
        return result
    }

    /** Where the score dial starts: an ordinary block. Also what a block closes at with focus points off. */
    const val NEUTRAL_MULTIPLIER = 1.0

    /** The dial counts in tenths, so ten steps up from ×1.0 land on exactly ×2.0. */
    const val NEUTRAL_TENTHS = 10

    /** The dial's range in tenths: ×0.0 to ×5.0, `FocusPoints.multiplierRange`. */
    val tenthsRange: IntRange =
        Math.round(FocusPoints.multiplierRange.start * 10).toInt()..Math.round(FocusPoints.multiplierRange.endInclusive * 10).toInt()

    /** One step of the dial, kept inside [tenthsRange]. */
    fun nudge(tenths: Int, by: Int): Int = (tenths + by).coerceIn(tenthsRange)

    /** `×1.0`, `×1.1`, `×0.0`: always one decimal place, so neighbouring values read as neighbours. */
    fun dial(tenths: Int): String = "×%d.%d".format(tenths / 10, tenths % 10)

    /** `×1`, `×1.5`, `×0.75`. */
    fun multiplier(value: Double): String =
        "×" + java.math.BigDecimal.valueOf(value).setScale(2, java.math.RoundingMode.HALF_UP).stripTrailingZeros().toPlainString()

    /** `12.5 points`, `1 point`. */
    fun points(seconds: Int, multiplier: Double): String {
        val score = FocusPoints.score(seconds, multiplier)
        return "${FocusPoints.formatted(score)} ${if (score == 1.0) "point" else "points"}"
    }

    private val hourMinute = DateTimeFormatter.ofPattern("HH:mm", Locale.UK)

    fun hourMinute(time: LocalTime): String = hourMinute.format(time)
}

/**
 * The running clock as it reads: elapsed, the fraction of the plan, and
 * whether it has overrun. A timer ([openEnded]) has no plan: it only counts
 * up, so it has no fraction to fill and can never overrun.
 */
@Immutable
data class ClockReading(val elapsedSeconds: Int, val plannedSeconds: Int, val openEnded: Boolean = false) {
    val text: String get() = Format.clock(elapsedSeconds)
    val fraction: Float get() = if (openEnded) 0f else minOf(1f, elapsedSeconds.toFloat() / maxOf(60, plannedSeconds))
    val isOverrun: Boolean get() = !openEnded && elapsedSeconds > maxOf(60, plannedSeconds)

    companion object {
        fun of(session: FocusSession, now: Instant) =
            ClockReading(session.elapsedSeconds(now), session.workDurationSeconds, session.openEnded)
    }
}

/**
 * A finished block held between pressing Done and saying how it went. The
 * seconds are taken at the press: the clock stops when the work stops, not
 * when the judgement arrives.
 */
@Immutable
data class PendingCompletion(
    val sessionId: String,
    val taskId: String,
    val title: String,
    val seconds: Int,
    val completeTask: Boolean,
    val blockId: String?,
    val wasPaused: Boolean,
)

internal fun String.capitalisedFirst(): String = if (isEmpty()) this else this[0].uppercaseChar() + substring(1)
