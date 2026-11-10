package uk.co.maybeitsadam.takt.focus

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uk.co.maybeitsadam.takt.T0
import uk.co.maybeitsadam.takt.core.FocusSessionPhase
import uk.co.maybeitsadam.takt.session

class FocusNotificationModelTest {
    @Test fun runningCountsFromBlockStartIncludingAccumulated() {
        val now = T0.plusSeconds(100)
        val model = FocusNotificationModel.of(session("a", accumulated = 200), "Write", now)
        assertTrue(model.isCounting)
        assertEquals("Write", model.title)
        assertEquals(now.toEpochMilli() - 300_000, model.chronometerBaseMillis)
        assertEquals("Focusing · 25m planned", model.text)
    }

    @Test fun pausedShowsFrozenTime() {
        val model = FocusNotificationModel.of(session("a", accumulated = 125, pausedAt = T0), "Write", T0.plusSeconds(999))
        assertFalse(model.isCounting)
        assertTrue(model.isPaused)
        assertEquals("Paused at 02:05 of 25m", model.text)
    }

    @Test fun aTimerNamesNoPlan() {
        val running = FocusNotificationModel.of(session("a", planned = 86_400, openEnded = true), "Write", T0.plusSeconds(60))
        assertTrue(running.isCounting)
        assertEquals("Timing", running.text)
        val paused = FocusNotificationModel.of(session("a", accumulated = 125, pausedAt = T0, openEnded = true), "Write", T0)
        assertEquals("Paused at 02:05", paused.text)
    }

    @Test fun noTaskHasNoActions() {
        val model = FocusNotificationModel.of(session(null), null, T0)
        assertFalse(model.hasActions)
        assertEquals("Nothing in the queue can run now", model.text)
    }

    @Test fun serviceNeededUntilFinished() {
        assertTrue(session("a").needsForegroundService())
        assertFalse(session("a", phase = FocusSessionPhase.FINISHED).needsForegroundService())
        assertFalse(null.needsForegroundService())
    }
}
