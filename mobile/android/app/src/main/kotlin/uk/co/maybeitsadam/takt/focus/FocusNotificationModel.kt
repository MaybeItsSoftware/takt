package uk.co.maybeitsadam.takt.focus

import java.time.Instant
import uk.co.maybeitsadam.takt.core.FocusSession
import uk.co.maybeitsadam.takt.core.FocusSessionPhase
import uk.co.maybeitsadam.takt.ui.components.Format

/**
 * What the focus notification says, worked out without Android so it can be
 * tested: its title and text, whether it counts, and where its chronometer
 * starts from.
 */
data class FocusNotificationModel(
    val title: String,
    val text: String,
    /** Counting up: a chronometer from [chronometerBaseMillis]. False shows [text]'s frozen time. */
    val isCounting: Boolean,
    /** Wall-clock millis the chronometer counts from, so it reads the block's elapsed time. */
    val chronometerBaseMillis: Long,
    val isPaused: Boolean,
    /** Whether Pause/Resume and Done apply (a block with a task). */
    val hasActions: Boolean,
) {
    companion object {
        /**
         * The block's elapsed time includes seconds accumulated before the last
         * resume, so the chronometer's base is now minus the whole elapsed time,
         * not the moment the block (re)started.
         */
        fun of(session: FocusSession, taskTitle: String?, now: Instant): FocusNotificationModel {
            val elapsed = session.elapsedSeconds(now)
            val base = chronometerBase(now.toEpochMilli(), elapsed)
            val planned = Format.duration(maxOf(60, session.workDurationSeconds))
            return when {
                session.phase == FocusSessionPhase.ON_BREAK -> FocusNotificationModel(
                    title = "On a break",
                    text = session.breakEndsAt?.let { "Back at ${Format.time(it)}" } ?: "Take a breather",
                    isCounting = false, chronometerBaseMillis = base, isPaused = false, hasActions = false,
                )
                session.activeTaskId == null -> FocusNotificationModel(
                    title = "Focus session",
                    text = "Nothing in the queue can run now",
                    isCounting = false, chronometerBaseMillis = base, isPaused = false, hasActions = false,
                )
                session.pausedAt != null -> FocusNotificationModel(
                    title = taskTitle ?: "Focus block",
                    text = if (session.openEnded) "Paused at ${Format.clock(elapsed)}" else "Paused at ${Format.clock(elapsed)} of $planned",
                    isCounting = false, chronometerBaseMillis = base, isPaused = true, hasActions = true,
                )
                else -> FocusNotificationModel(
                    title = taskTitle ?: "Focus block",
                    // A timer only counts up: there is no plan to name.
                    text = if (session.openEnded) "Timing" else "Focusing · $planned planned",
                    isCounting = true, chronometerBaseMillis = base, isPaused = false, hasActions = true,
                )
            }
        }

        fun chronometerBase(nowMillis: Long, elapsedSeconds: Int): Long = nowMillis - maxOf(0, elapsedSeconds) * 1000L
    }
}

/** Whether a session needs the foreground service: anything not finished. */
fun FocusSession?.needsForegroundService(): Boolean = this != null && phase != FocusSessionPhase.FINISHED
