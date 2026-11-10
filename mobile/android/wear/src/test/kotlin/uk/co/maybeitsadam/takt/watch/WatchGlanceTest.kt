package uk.co.maybeitsadam.takt.watch

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uk.co.maybeitsadam.takt.wear.contract.WearPhase
import uk.co.maybeitsadam.takt.wear.contract.WearSession
import uk.co.maybeitsadam.takt.wear.contract.WearSnapshot
import uk.co.maybeitsadam.takt.wear.contract.WearTask

class WatchGlanceTest {
    private val now = 1_800_000_000_000L
    private val write = WearTask(id = "a", title = "Write the report", estimateSeconds = 1800)
    private val call = WearTask(id = "b", title = "Call the bank")
    private val stretch = WearTask(id = "c", title = "Stretch", isDaily = true, dailyDone = true)

    @Test
    fun withNoDayItSaysToOpenThePhone() {
        val glance = WatchGlance.of(null, now)
        assertNull(glance.title)
        assertEquals("Open Takt on your phone", glance.footer)
    }

    @Test
    fun withNothingRunningItNamesTheNextTaskAndCountsWhatIsOpen() {
        val glance = WatchGlance.of(WearSnapshot(tasks = listOf(write, call, stretch), generatedAt = now), now)
        assertEquals("Next", glance.eyebrow)
        assertEquals("Write the report", glance.title)
        assertEquals(2, glance.openCount)
        assertEquals("2 left today", glance.footer)
        assertFalse(glance.isFocusing)
    }

    @Test
    fun aRunningBlockCountsFromItsStart() {
        val session = WearSession(taskId = "a", taskTitle = "Write the report", phase = WearPhase.RUNNING, startedAt = now - 90_000, accumulated = 30, plannedSeconds = 1800)
        val glance = WatchGlance.of(WearSnapshot(tasks = listOf(write.copy(isRunning = true), call), session = session), now)
        assertTrue(glance.isFocusing)
        assertEquals("Focusing", glance.eyebrow)
        assertEquals(120, glance.elapsedSeconds)
        assertEquals(now - 120_000, glance.countingFrom)
    }

    @Test
    fun aPausedBlockStopsCounting() {
        val session = WearSession(taskId = "a", phase = WearPhase.PAUSED, startedAt = now - 600_000, accumulated = 300, plannedSeconds = 1800)
        val glance = WatchGlance.of(WearSnapshot(tasks = listOf(write.copy(isRunning = true)), session = session), now)
        assertEquals("Paused", glance.eyebrow)
        assertNull(glance.countingFrom)
        assertEquals(300, glance.elapsedSeconds)
        assertEquals("Write the report", glance.title)
    }

    @Test
    fun aTimerCountsUpWithNoPlannedLength() {
        val session = WearSession(
            taskId = "a", phase = WearPhase.RUNNING, startedAt = now - 90_000, plannedSeconds = 86_400, openEnded = true,
        )
        val glance = WatchGlance.of(WearSnapshot(tasks = listOf(write.copy(isRunning = true), call), session = session), now)
        assertEquals(now - 90_000, glance.countingFrom)
        assertNull(glance.plannedSeconds)
        assertEquals("Timer · 2 left today", glance.footer)
    }

    @Test
    fun anEmptyDayIsAllDone() {
        val glance = WatchGlance.of(WearSnapshot(tasks = listOf(stretch)), now)
        assertNull(glance.title)
        assertEquals("All done", glance.footer)
    }

    @Test
    fun theAgeReadsInWholeUnits() {
        assertEquals("just now", ageText(now - 20_000, now))
        assertEquals("4m ago", ageText(now - 4 * 60_000, now))
        assertEquals("2h ago", ageText(now - 2 * 3_600_000, now))
        assertEquals("1d ago", ageText(now - 26 * 3_600_000, now))
    }
}
