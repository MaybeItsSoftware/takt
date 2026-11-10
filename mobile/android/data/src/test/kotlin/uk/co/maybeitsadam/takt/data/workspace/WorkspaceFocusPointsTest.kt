package uk.co.maybeitsadam.takt.data.workspace

import java.time.LocalDateTime
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uk.co.maybeitsadam.takt.core.FocusPointsSummary
import uk.co.maybeitsadam.takt.core.FocusQuality
import uk.co.maybeitsadam.takt.core.TaskList
import uk.co.maybeitsadam.takt.core.TaskStatus
import uk.co.maybeitsadam.takt.core.quality
import uk.co.maybeitsadam.takt.data.TestWorkspace

/**
 * The store half of workspace-tests/WorkspaceFocusPointsTests.swift. The
 * arithmetic cases (minutes, clamping, formatting, presets) are core logic and
 * are ported in :core's FocusPointsTest.
 */
class WorkspaceFocusPointsTest {
    private fun fixture(test: suspend (TestWorkspace, TaskList) -> Unit): Unit = runBlocking {
        workspace().use { w ->
            val workspace = w.repository.bootstrapIfNeeded()
            test(w, w.repository.inbox(workspace.id)!!)
        }
    }

    @Test
    fun finishingABlockWithAQualityScoresItAgainstTheTaskTitle() = fixture { w, list ->
        val store = w.repository
        val task = store.createTask(listId = list.id, title = "Draft the brief")
        val session = store.startFocusSession(taskId = task.id)
        val completion = store.completeActiveFocusTask(
            sessionId = session.id, elapsedSeconds = 1_500, qualityMultiplier = FocusQuality.SHARP.multiplier,
        )
        val award = completion.award!!
        assertEquals("Draft the brief", award.taskTitle)
        assertEquals(25.0, award.minutes, 0.0)
        assertEquals(37.5, award.points, 0.0)
        assertEquals(FocusQuality.SHARP, award.quality)
        assertEquals(listOf(award.id), store.focusAwards().map { it.id })
    }

    @Test
    fun aTimerReadsAsOpenEndedAndABlockDoesNot() = fixture { w, list ->
        val store = w.repository
        val timer = store.startFocusTimer(taskId = store.createTask(listId = list.id, title = "Timed").id)
        assertTrue(timer.openEnded)
        assertTrue(store.focusQueue(timer.id).first().item.openEnded)
        store.finishFocusSession(timer.id)

        val block = store.startFocusSession(taskId = store.createTask(listId = list.id, title = "Blocked").id, plannedSeconds = 1_500)
        assertFalse(block.openEnded)
        assertEquals(1_500, block.workDurationSeconds)
    }

    @Test
    fun finishingWithoutAQualityScoresNothing() = fixture { w, list ->
        val store = w.repository
        val task = store.createTask(listId = list.id, title = "Tidy up")
        val session = store.startFocusSession(taskId = task.id)
        val completion = store.completeActiveFocusTask(sessionId = session.id, elapsedSeconds = 600)
        assertNull(completion.award)
        assertTrue(store.focusAwards().isEmpty())
    }

    @Test
    fun aBlockThatTookNoTimeScoresNothingEvenWithAQuality() = fixture { w, list ->
        val store = w.repository
        val task = store.createTask(listId = list.id, title = "Misfire")
        val session = store.startFocusSession(taskId = task.id)
        val completion = store.completeActiveFocusTask(
            sessionId = session.id, elapsedSeconds = 0, qualityMultiplier = FocusQuality.FLOW.multiplier,
        )
        assertNull(completion.award)
        assertTrue(store.focusAwards().isEmpty())
    }

    @Test
    fun timeCreditedToADailyIsStillScored() = fixture { w, list ->
        val store = w.repository
        val task = store.createTask(listId = list.id, title = "Write 500 words")
        store.makeDaily(taskId = task.id)
        val session = store.startFocusSession(taskId = task.id)
        val completion = store.completeActiveFocusTask(
            sessionId = session.id, elapsedSeconds = 1_800, qualityMultiplier = FocusQuality.SOLID.multiplier,
        )
        assertEquals(FocusCompletionOutcome.ContributionLogged(1_800), completion.outcome)
        assertEquals(TaskStatus.OPEN, store.task(task.id)?.status)
        assertEquals(30.0, completion.award!!.points, 0.0)
    }

    @Test
    fun deletingTheTaskLeavesTheScoreStanding() = fixture { w, list ->
        val store = w.repository
        val task = store.createTask(listId = list.id, title = "Ship the thing")
        val session = store.startFocusSession(taskId = task.id)
        store.completeActiveFocusTask(sessionId = session.id, elapsedSeconds = 1_200, qualityMultiplier = 2.0)
        store.deleteTask(task.id)
        val award = store.focusAwards().first()
        assertNull(award.taskId)
        assertEquals("Ship the thing", award.taskTitle)
        assertEquals(40.0, award.points, 0.0)
    }

    @Test
    fun theSummaryCountsTodayTheTrailingWeekAndEverything() = fixture { w, list ->
        val store = w.repository
        val task = store.createTask(listId = list.id, title = "Work")
        val now = LocalDateTime.of(2026, 6, 15, 17, 0).atZone(UTC).toInstant()

        suspend fun score(daysAgo: Long, seconds: Int, multiplier: Double) {
            val at = now.minusSeconds(daysAgo * 86_400)
            val session = store.startFocusSession(taskId = task.id, now = at)
            store.completeActiveFocusTask(
                sessionId = session.id, elapsedSeconds = seconds, qualityMultiplier = multiplier, now = at, zone = UTC,
            )
        }
        score(0, 1_500, 1.0)
        score(0, 600, 1.5)
        score(3, 1_800, 1.0)
        score(30, 3_600, 2.0)

        val summary = store.focusPointsSummary(now = now, zone = UTC)
        assertEquals(40.0, summary.today, 0.0)
        assertEquals(2, summary.blocksToday)
        assertEquals(70.0, summary.last7Days, 0.0)
        assertEquals(190.0, summary.allTime, 0.0)
        assertEquals(2, store.focusAwards(onDayOf = now, zone = UTC).size)
    }

    @Test
    fun anEmptyLedgerReportsZeroRatherThanFailing() = fixture { w, _ ->
        assertEquals(FocusPointsSummary.ZERO, w.repository.focusPointsSummary())
    }
}
