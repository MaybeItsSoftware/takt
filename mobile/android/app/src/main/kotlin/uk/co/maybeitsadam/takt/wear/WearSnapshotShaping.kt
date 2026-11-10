package uk.co.maybeitsadam.takt.wear

import java.time.Instant
import uk.co.maybeitsadam.takt.core.DayPlanReason
import uk.co.maybeitsadam.takt.core.FocusSession
import uk.co.maybeitsadam.takt.core.FocusSessionPhase
import uk.co.maybeitsadam.takt.ui.today.DayCard
import uk.co.maybeitsadam.takt.ui.today.DayDaily
import uk.co.maybeitsadam.takt.ui.today.DayState
import uk.co.maybeitsadam.takt.wear.contract.WearPhase
import uk.co.maybeitsadam.takt.wear.contract.WearSession
import uk.co.maybeitsadam.takt.wear.contract.WearSnapshot
import uk.co.maybeitsadam.takt.wear.contract.WearTask

/** The phone's Today reduced to what the watch draws. Pure, so it is tested on the JVM. */
object WearSnapshotShaping {
    /**
     * The day's cards in the order Today reads them (the running block, then
     * planned, overdue, due today, starts today), then the dailies still owed.
     */
    fun of(day: DayState, handled: List<String>, now: Instant): WearSnapshot {
        val cards = day.sections.flatMap { it.cards }.map(::task)
        val owed = day.dailies.filterNot { it.isDone }.map(::task)
        val seen = HashSet<String>()
        val tasks = (cards + owed).filter { seen.add(it.id) }
        return WearSnapshot(
            tasks = tasks,
            session = session(day.session, tasks),
            generatedAt = now.toEpochMilli(),
            handled = handled,
        )
    }

    private fun task(card: DayCard) = WearTask(
        id = card.id,
        title = card.title,
        detail = when (card.reason) {
            DayPlanReason.OVERDUE, DayPlanReason.DUE_TODAY, DayPlanReason.STARTS_TODAY -> card.reason.label
            else -> card.listName
        },
        isDaily = card.dailyId != null,
        dailyDone = card.isDailyDoneToday,
        isRunning = card.isRunning,
        estimateSeconds = card.estimateSeconds,
    )

    private fun task(daily: DayDaily) = WearTask(
        id = daily.taskId,
        title = daily.title,
        detail = daily.progressText ?: "Daily",
        isDaily = true,
        dailyDone = daily.isDone,
        estimateSeconds = daily.targetSeconds,
    )

    fun session(session: FocusSession?, tasks: List<WearTask> = emptyList()): WearSession? {
        if (session == null || session.phase == FocusSessionPhase.FINISHED) return null
        val phase = when {
            session.phase == FocusSessionPhase.ON_BREAK -> WearPhase.ON_BREAK
            session.pausedAt != null -> WearPhase.PAUSED
            else -> WearPhase.RUNNING
        }
        return WearSession(
            taskId = session.activeTaskId,
            taskTitle = session.activeTaskId?.let { id -> tasks.firstOrNull { it.id == id }?.title },
            phase = phase,
            startedAt = (if (phase == WearPhase.PAUSED) session.pausedAt!! else session.activeTaskStartedAt).toEpochMilli(),
            accumulated = maxOf(0, session.accumulatedSeconds ?: 0),
            plannedSeconds = maxOf(60, session.workDurationSeconds),
            breakEndsAt = session.breakEndsAt?.toEpochMilli(),
            openEnded = session.openEnded,
        )
    }
}
