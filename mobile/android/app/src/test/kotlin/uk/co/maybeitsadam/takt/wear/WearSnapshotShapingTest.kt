package uk.co.maybeitsadam.takt.wear

import kotlinx.collections.immutable.persistentListOf
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uk.co.maybeitsadam.takt.T0
import uk.co.maybeitsadam.takt.core.DayPlanReason
import uk.co.maybeitsadam.takt.core.FocusSessionPhase
import uk.co.maybeitsadam.takt.session
import uk.co.maybeitsadam.takt.ui.today.DayCard
import uk.co.maybeitsadam.takt.ui.today.DayDaily
import uk.co.maybeitsadam.takt.ui.today.DayState
import uk.co.maybeitsadam.takt.wear.contract.WearPhase

class WearSnapshotShapingTest {
    private fun card(id: String, reason: DayPlanReason?, running: Boolean = false, dailyId: String? = null, list: String? = "Work") = DayCard(
        id = id, title = "Task $id", listName = list, listColorHex = null, reason = reason, estimateSeconds = 900,
        loggedSeconds = 0, dueAt = null, isList = false, dailyId = dailyId, isDailyDoneToday = false, isRunning = running,
    )

    @Test
    fun readsInTodaysOrderWithTheRunningBlockFirstAndOwedDailiesLast() {
        val day = DayState(
            cards = persistentListOf(
                card("overdue", DayPlanReason.OVERDUE),
                card("planned", DayPlanReason.PLANNED),
                card("running", DayPlanReason.PLANNED, running = true),
                card("habit", DayPlanReason.PLANNED, dailyId = "D1"),
            ),
            dailies = persistentListOf(
                DayDaily("D2", "stretch", "Stretch", isDone = false, secondsToday = 600, targetSeconds = 1800),
                DayDaily("D3", "read", "Read", isDone = true, secondsToday = 0, targetSeconds = null),
            ),
            session = session("running", startedAt = T0, accumulated = 30),
            isLoaded = true,
        )
        val snapshot = WearSnapshotShaping.of(day, listOf("a1"), T0.plusSeconds(60))
        assertEquals(listOf("running", "planned", "habit", "overdue", "stretch"), snapshot.tasks.map { it.id })
        assertEquals("Work", snapshot.tasks[1].detail)
        assertEquals("Overdue", snapshot.tasks[3].detail)
        assertTrue(snapshot.tasks[2].isDaily)
        assertEquals("10m/30m", snapshot.tasks[4].detail)
        assertEquals(listOf("a1"), snapshot.handled)
        assertEquals(T0.plusSeconds(60).toEpochMilli(), snapshot.generatedAt)

        val block = snapshot.session!!
        assertEquals(WearPhase.RUNNING, block.phase)
        assertEquals("Task running", block.taskTitle)
        // Elapsed on the watch is the phone's own sum.
        assertEquals(day.session!!.elapsedSeconds(T0.plusSeconds(90)), block.elapsedSeconds(T0.plusSeconds(90).toEpochMilli()))
    }

    @Test
    fun aPausedBlockHoldsItsTime() {
        val paused = session("a", startedAt = T0, accumulated = 120, pausedAt = T0.plusSeconds(500))
        val block = WearSnapshotShaping.session(paused)!!
        assertEquals(WearPhase.PAUSED, block.phase)
        assertEquals(120, block.elapsedSeconds(T0.plusSeconds(9_000).toEpochMilli()))
        assertEquals(1500, block.plannedSeconds)
    }

    @Test
    fun breaksAndFinishedSessions() {
        assertEquals(WearPhase.ON_BREAK, WearSnapshotShaping.session(session(null, phase = FocusSessionPhase.ON_BREAK))?.phase)
        assertNull(WearSnapshotShaping.session(session("a", phase = FocusSessionPhase.FINISHED)))
        assertNull(WearSnapshotShaping.session(null))
        assertTrue(WearSnapshotShaping.session(session("a", openEnded = true))!!.openEnded)
        assertFalse(WearSnapshotShaping.session(session("a"))!!.openEnded)
    }
}
