package uk.co.maybeitsadam.takt.data.workspace

import java.time.Instant
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uk.co.maybeitsadam.takt.core.FocusContext
import uk.co.maybeitsadam.takt.core.FocusSessionPhase
import uk.co.maybeitsadam.takt.core.FocusQueueState
import uk.co.maybeitsadam.takt.core.NextUpReason
import uk.co.maybeitsadam.takt.core.NextUpSelector
import uk.co.maybeitsadam.takt.core.TaskCalendarDate
import uk.co.maybeitsadam.takt.core.TaskList
import uk.co.maybeitsadam.takt.core.TaskStatus
import uk.co.maybeitsadam.takt.core.TaskUnavailableReason
import uk.co.maybeitsadam.takt.core.Workspace
import uk.co.maybeitsadam.takt.core.WorkspaceTask
import uk.co.maybeitsadam.takt.data.TestWorkspace

/**
 * Port of workspace-tests/WorkspaceConditionsTests.swift. Not ported:
 * `testOldDraftPayloadMigratesWithoutDiscardingUnsavedNotes` (the JSON draft
 * file store) and `testNewSchemaReplaysPreUpgradeMetadataSnapshotsAndBackfillsOnlyKnownWork`
 * (rewinds GRDB migrations, which the Android side does not run).
 */
class WorkspaceConditionsTest {
    private val now: Instant = epoch(1_789_560_000)

    private class Fixture(val w: TestWorkspace, val workspace: Workspace, val inbox: TaskList, val now: Instant) {
        val store get() = w.repository

        suspend fun task(title: String = "Task", estimate: Int? = 3600): WorkspaceTask {
            val task = store.createTask(listId = inbox.id, title = title, now = now)
            store.updateTask(task.id, title, "", null, estimate, now = now)
            return task
        }

        suspend fun edit(id: String) = TaskEditorDraft(store.taskEditorSnapshot(id))

        fun day(instant: Instant) = TaskCalendarDate.string(instant, UTC)
    }

    private fun fixture(test: suspend Fixture.() -> Unit): Unit = runBlocking {
        workspace().use { w ->
            val workspace = w.repository.bootstrapIfNeeded(now = now)
            Fixture(w, workspace, w.repository.inbox(workspace.id)!!, now).test()
        }
    }

    @Test
    fun focusTimelineIncludesUnscoredWorkAndKeepsTitlesAfterDeletion() = fixture {
        val first = task("Reading")
        val session = store.startFocusSession(taskId = first.id, now = now)
        val loggedAt = now.plusSeconds(600)
        store.completeActiveFocusTask(sessionId = session.id, elapsedSeconds = 420, completeTask = false, now = loggedAt)
        store.deleteTask(first.id)
        assertTrue(store.focusWorkBlocks(now, loggedAt).isEmpty())
        val included = store.focusWorkBlocks(loggedAt, loggedAt.plusSeconds(60))
        assertEquals(1, included.size)
        assertEquals("Reading", included.first().taskTitle)
        assertEquals(420, included.first().seconds)
        assertNull(included.first().taskId)
        assertEquals(first.id, included.first().originalTaskId)
    }

    @Test
    fun defaultConditionsSeedOnceAndRenamingRetainsTaskIdentityThroughUndo() = fixture {
        val conditions = store.conditions(workspace.id)
        assertEquals(setOf("Home", "Campus", "Private", "Floor space"), conditions.map { it.name }.toSet())
        store.bootstrapIfNeeded()
        assertEquals(4, store.conditions(workspace.id).size)
        val campus = conditions.first { it.name == "Campus" }
        val task = task()
        store.saveTaskEditor(edit(task.id).edit { copy(requirementGroups = listOf(listOf(campus.id))) })
        store.saveCondition(campus.id, "On campus", isLocation = true, isArchived = false)
        assertEquals(listOf(listOf(campus.id)), store.nextUpCandidates(now = now).first().requirementGroups)
        store.undo()
        assertEquals("Campus", store.conditions(workspace.id).first { it.id == campus.id }.name)
        store.redo()
        assertEquals("On campus", store.conditions(workspace.id).first { it.id == campus.id }.name)
    }

    @Test
    fun categoriesGroupConditionsAndWhereYouAreOffersOnlyWhatOpenTasksNeed() = fixture {
        val home = store.conditions(workspace.id).first { it.name == "Home" }
        assertEquals("the seeded places are filed under Location", ConditionGroups.LOCATION, home.category)
        val mat = store.createCondition(workspace.id, "Mat", category = "Equipment", now = now)
        assertEquals("Equipment", mat.category)
        store.setConditionCategory(mat.id, "location", now = now)
        val moved = store.conditions(workspace.id).first { it.id == mat.id }
        assertEquals(ConditionGroups.LOCATION, moved.category)
        assertTrue("Location is what makes it a place", moved.isLocation)
        store.undo()
        assertEquals("Equipment", store.conditions(workspace.id).first { it.id == mat.id }.category)

        val all = store.conditions(workspace.id)
        assertEquals(listOf("Location", "Equipment", "Other"), ConditionGroups.all(all).map { it.title })
        assertEquals(listOf("Location", "Equipment"), ConditionGroups.categories(all))
        assertTrue(ConditionGroups.available(all, store.requiredConditionIds(workspace.id), emptySet()).isEmpty())

        val gym = task("Gym")
        store.saveTaskEditor(edit(gym.id).edit { copy(requirementGroups = listOf(listOf(mat.id))) })
        val required = store.requiredConditionIds(workspace.id)
        assertEquals(setOf(mat.id), required)
        val offered = ConditionGroups.available(all, required, setOf(home.id))
        assertEquals(listOf("Location", "Equipment"), offered.map { it.title })
        assertEquals(listOf(listOf(home.id), listOf(mat.id)), offered.map { group -> group.conditions.map { it.id } })
        assertTrue(offered.first().isExclusive)

        store.setStatus(TaskStatus.COMPLETED, gym.id, now = now)
        assertTrue("a done task needs nothing", store.requiredConditionIds(workspace.id).isEmpty())
    }

    @Test
    fun whatAPlaceProvidesHoldsUntilSwitchedOffAndComesBackWithTheNextPlace() = fixture {
        val conditions = store.conditions(workspace.id)
        val home = conditions.first { it.name == "Home" }
        val campus = conditions.first { it.name == "Campus" }
        val floor = conditions.first { it.name == "Floor space" }
        assertTrue(store.setConditionProvides(home.id, listOf(floor.id), now = now))
        val stretch = task("Stretch")
        store.saveTaskEditor(edit(stretch.id).edit { copy(requirementGroups = listOf(listOf(floor.id))) })
        val all = store.conditions(workspace.id)
        assertEquals(listOf(floor.id), all.first { it.id == home.id }.provides)

        // At Home, Floor space holds without being said, and the task is offered.
        val held = ConditionHoldings.held(all, setOf(home.id), emptySet())
        assertEquals(setOf(home.id, floor.id), held)
        assertEquals("from Home", ConditionHoldings.note(ConditionHoldings.holdings(all, setOf(home.id), emptySet())[floor.id], all))
        val ranked = NextUpSelector.evaluate(store.nextUpCandidates(now = now), now = now, zone = UTC, context = FocusContext(held))
        assertTrue(ranked.ranked.any { it.id == stretch.id })
        // Home offers itself under "Where you are": it provides what the task needs.
        val offered = ConditionGroups.available(all, store.requiredConditionIds(workspace.id), held)
        assertTrue(offered.first().conditions.any { it.id == home.id })

        // Switched off for now: an override, and the task waits.
        val off = ConditionHoldings.switching(all, setOf(home.id), emptySet(), floor.id)
        assertEquals(listOf(home.id), off.conditionIds)
        assertEquals(listOf(floor.id), off.offIds)
        assertEquals(setOf(home.id), ConditionHoldings.held(all, off.conditionIds.toSet(), off.offIds.toSet()))

        // Saying another place replaces Home, and Home's override goes with it.
        val moved = ConditionHoldings.switching(all, off.conditionIds.toSet(), off.offIds.toSet(), campus.id)
        assertEquals(listOf(campus.id), moved.conditionIds)
        assertTrue(moved.offIds.isEmpty())
        val back = ConditionHoldings.switching(all, moved.conditionIds.toSet(), emptySet(), home.id)
        assertEquals(setOf(home.id, floor.id), ConditionHoldings.held(all, back.conditionIds.toSet(), back.offIds.toSet()))

        // The override is kept with the context, and read back.
        store.saveFocusContext(workspace.id, emptyFocusContext().copy(conditionIds = off.conditionIds, offIds = off.offIds), now)
        assertEquals(listOf(floor.id), store.focusContext(workspace.id)!!.offIds)
    }

    @Test
    fun planningSaveAndUndoAreAtomicAndReopenRetainsAllFields() = fixture {
        val task = task()
        val conditions = store.conditions(workspace.id)
        val draft = edit(task.id).edit {
            copy(
                startAt = now.plusSeconds(3600),
                dueDate = day(now.plusSeconds(86_400)),
                minimumBlockMinutes = "10.5",
                requiresSingleSitting = true,
                requirementGroups = listOf(listOf(conditions[0].id, conditions[1].id), listOf(conditions[2].id)),
                title = "Planned",
            )
        }
        val saved = store.saveTaskEditor(draft)
        assertEquals(630, saved.planning?.minimumBlockSeconds)
        assertEquals("Edit Task", store.undo())
        assertEquals(draft.baseline, store.taskEditorSnapshot(task.id))
        store.redo()
        w.reopened { assertEquals(saved, it.taskEditorSnapshot(task.id)) }
    }

    @Test
    fun invalidConditionAndLateInjectedFailureRollBackPlanningAndHistory() = fixture {
        val task = task()
        var draft = edit(task.id).edit { copy(title = "Lost?", requirementGroups = listOf(listOf("missing"))) }
        val label = store.undoableLabel()
        thrown { store.saveTaskEditor(draft) }
        assertEquals(draft.baseline, store.taskEditorSnapshot(task.id))
        assertEquals(label, store.undoableLabel())
        val required = listOf(store.conditions(workspace.id).first().id)
        draft = draft.edit { copy(requirementGroups = listOf(required, required)) }
        thrown { store.saveTaskEditor(draft) }
        assertEquals(draft.baseline, store.taskEditorSnapshot(task.id))
        draft = draft.edit { copy(requirementGroups = listOf(required), dailyProgress = true) }
        w.otherConnection {
            it.execute(
                "CREATE TRIGGER fail_conditions_daily BEFORE INSERT ON dailies BEGIN SELECT RAISE(ABORT, 'failure'); END",
            )
        }
        thrown { store.saveTaskEditor(draft) }
        assertEquals(draft.baseline, store.taskEditorSnapshot(task.id))
        assertEquals(label, store.undoableLabel())
    }

    @Test
    fun invalidScheduleMinimumAndOneSittingEstimateAreRejected() = fixture {
        val task = task(estimate = null)
        var draft = edit(task.id).edit { copy(dueAt = now, startAt = now.plusSeconds(1)) }
        thrown { store.saveTaskEditor(draft) }
        draft = draft.edit { copy(startAt = null, minimumBlockMinutes = "0.5") }
        thrown { store.saveTaskEditor(draft) }
        draft = draft.edit { copy(minimumBlockMinutes = null, requiresSingleSitting = true) }
        thrown { store.saveTaskEditor(draft) }
        assertEquals(draft.baseline, store.taskEditorSnapshot(task.id))
    }

    @Test
    fun archivedRequirementsRemainButCannotBeNewlyAssigned() = fixture {
        val condition = store.conditions(workspace.id).first()
        val first = task("First")
        val second = task("Second")
        store.saveTaskEditor(edit(first.id).edit { copy(requirementGroups = listOf(listOf(condition.id))) })
        store.saveCondition(condition.id, condition.name, condition.isLocation, isArchived = true)
        store.saveTaskEditor(edit(first.id).edit { copy(title = "Retained") })
        thrown { store.saveTaskEditor(edit(second.id).edit { copy(requirementGroups = listOf(listOf(condition.id))) }) }
    }

    @Test
    fun startChangedOutsideEditorConflictsWithoutLosingNotes() = fixture {
        val task = task()
        var draft = edit(task.id).edit { copy(startAt = now, notes = "Unsaved notes") }
        store.scheduleTask(task.id, now.plusSeconds(3600))
        thrown { store.saveTaskEditor(draft) }
        draft = draft.reconciled(store.taskEditorSnapshot(task.id))
        assertTrue(TaskEditorField.START_AT in draft.conflicts)
        assertEquals("Unsaved notes", draft.values.notes)
        draft = draft.resolved(TaskEditorField.START_AT, useSaved = true)
        store.saveTaskEditor(draft)
        assertEquals(now.plusSeconds(3600), store.taskEditorSnapshot(task.id).planning?.startAt)
    }

    @Test
    fun progressCreditsUnscoredWorkAndKeepsTaskOpenWithRemainingEstimate() = fixture {
        val task = task()
        val session = store.startFocusSession(taskId = task.id, now = now)
        val result = store.completeActiveFocusTask(
            sessionId = session.id, elapsedSeconds = 900, completeTask = false,
            expectedBlockId = session.activeBlockId, now = now.plusSeconds(900),
        )
        assertEquals(FocusCompletionOutcome.ProgressLogged(900), result.outcome)
        assertEquals(TaskStatus.OPEN, store.task(task.id)?.status)
        assertEquals(listOf(900), store.workBlocks(task.id).map { it.seconds })
        assertEquals(2700, store.nextUpCandidates(now = now).first().remainingSeconds)
        assertTrue(store.focusAwards().isEmpty())
    }

    @Test
    fun duplicateCompletionCannotCreditTwiceOrCompleteNextQueuedTask() = fixture {
        val first = task("First")
        val second = task("Second")
        val session = store.startFocusSession(taskId = first.id, plannedSeconds = 600, now = now)
        store.addToFocusQueue(session.id, second.id, plannedSeconds = 2400, now = now)
        val result = store.completeActiveFocusTask(
            sessionId = session.id, elapsedSeconds = 600, qualityMultiplier = 1.0,
            expectedBlockId = session.activeBlockId, now = now.plusSeconds(600),
        )
        assertEquals(2400, result.session.workDurationSeconds)
        assertEquals(second.id, result.session.activeTaskId)
        val retry = store.completeActiveFocusTask(
            sessionId = session.id, elapsedSeconds = 600, qualityMultiplier = 1.0,
            expectedBlockId = session.activeBlockId, now = now.plusSeconds(601),
        )
        assertEquals(second.id, retry.session.activeTaskId)
        assertEquals(1, store.workBlocks(first.id).size)
        assertEquals(1, store.focusAwards().size)
        assertEquals(TaskStatus.OPEN, store.task(second.id)?.status)
    }

    @Test
    fun pauseResumeAndInterruptedRecoveryExcludeInactiveTime() = fixture {
        val task = task()
        val session = store.startFocusSession(taskId = task.id, now = now)
        store.pauseFocusSession(session.id, now = now.plusSeconds(300))
        assertEquals(300, store.activeFocusSession()?.elapsedSeconds(now.plusSeconds(600)))
        store.resumeFocusSession(session.id, now = now.plusSeconds(600))
        store.checkpointFocusSession(session.id, now = now.plusSeconds(720))
        store.recoverInterruptedFocus()
        val recovered = store.activeFocusSession()!!
        assertNotNull(recovered.pausedAt)
        assertEquals(420, recovered.elapsedSeconds(now.plusSeconds(9000)))
        store.completeActiveFocusTask(
            sessionId = session.id, elapsedSeconds = recovered.elapsedSeconds(now), completeTask = false,
            now = now.plusSeconds(9000),
        )
        assertEquals(420, store.workBlocks(task.id).first().seconds)
    }

    @Test
    fun blockedQueueRetainsEntriesAndResumesAfterConditionChange() = fixture {
        val first = task("Laptop")
        val second = task("Home")
        val home = store.conditions(workspace.id).first { it.name == "Home" }
        store.saveTaskEditor(edit(second.id).edit { copy(requirementGroups = listOf(listOf(home.id))) })
        val session = store.startFocusSession(taskId = first.id, now = now)
        store.addToFocusQueue(session.id, second.id, plannedSeconds = 1200, now = now)
        val result = store.completeActiveFocusTask(sessionId = session.id, now = now)
        assertNull(result.session.activeTaskId)
        assertEquals(FocusSessionPhase.RUNNING, result.session.phase)
        assertEquals(FocusQueueState.QUEUED, store.focusQueue(session.id).last().item.state)
        store.resumeEligibleFocusQueue(FocusContext(conditionIDs = setOf(home.id)), now = now)
        assertEquals(second.id, store.activeFocusSession()?.activeTaskId)
        assertEquals(1200, store.activeFocusSession()?.workDurationSeconds)
    }

    @Test
    fun dailyPartialProgressUsesTodaysTargetWithoutDoubleCountingAwards() = fixture {
        val task = task()
        val daily = store.makeDaily(taskId = task.id, targetSeconds = 1800, now = now)
        val session = store.startFocusSession(taskId = task.id, now = now)
        store.completeActiveFocusTask(
            sessionId = session.id, elapsedSeconds = 600, qualityMultiplier = 1.0, completeTask = false,
            now = now.plusSeconds(600),
        )
        assertEquals(1200, store.nextUpCandidates(now = now).first().remainingSeconds)
        assertEquals(600, store.workBlocks(task.id).sumOf { it.seconds })
        assertEquals(600, store.contributionHistory(daily.id, 1, endingOn = now).first().secondsLogged)
        val second = store.startFocusSession(taskId = task.id, now = now.plusSeconds(600))
        store.completeActiveFocusTask(
            sessionId = second.id, elapsedSeconds = 1200, completeTask = false, now = now.plusSeconds(1800),
        )
        assertTrue(store.nextUpCandidates(now = now).isEmpty())
        assertEquals(TaskStatus.OPEN, store.task(task.id)?.status)
        assertEquals(1800, store.workBlocks(task.id).sumOf { it.seconds })
    }

    @Test
    fun historySurvivesTaskDeletion() = fixture {
        val task = task()
        val session = store.startFocusSession(taskId = task.id, now = now)
        store.completeActiveFocusTask(sessionId = session.id, elapsedSeconds = 600, completeTask = false, now = now)
        store.deleteTask(task.id)
        val blocks = w.otherConnection { db ->
            db.query("SELECT taskTitle, taskId, seconds FROM focus_work_blocks") {
                Triple(it.string("taskTitle"), it.stringOrNull("taskId"), it.int("seconds"))
            }
        }
        assertEquals("Task", blocks.first().first)
        assertNull(blocks.first().second)
        assertEquals(600, blocks.first().third)
        store.undo()
        assertEquals(3000, store.nextUpCandidates(now = now).first().remainingSeconds)
    }

    @Test
    fun crossWorkspaceConditionAndNoOpHistoryAreHandled() = fixture {
        val task = task()
        val otherId = newId()
        val foreignId = newId()
        w.otherConnection { db ->
            db.execute("INSERT INTO workspaces (id, name, createdAt, updatedAt) VALUES (?, 'Other', ?, ?)", otherId, now, now)
            db.execute(
                "INSERT INTO task_conditions (id, workspaceId, name, isLocation, isArchived, createdAt, updatedAt) " +
                    "VALUES (?, ?, 'Other', 0, 0, ?, ?)",
                foreignId, otherId, now, now,
            )
        }
        var draft = edit(task.id).edit { copy(requirementGroups = listOf(listOf(foreignId))) }
        thrown { store.saveTaskEditor(draft) }
        val local = store.conditions(workspace.id).first().id
        draft = draft.edit { copy(requirementGroups = listOf(listOf(local))) }
        store.saveTaskEditor(draft)
        store.undo()
        store.saveTaskEditor(edit(task.id))
        assertEquals("Edit Task", store.redoableLabel())
    }

    @Test
    fun automaticStartRechecksConditionsAndRequestedDurationInTransaction() = fixture {
        val task = task()
        val home = store.conditions(workspace.id).first { it.name == "Home" }
        store.saveTaskEditor(
            edit(task.id).edit { copy(requirementGroups = listOf(listOf(home.id)), minimumBlockMinutes = "10") },
        )
        thrown { store.startFocusSession(taskId = task.id, plannedSeconds = 600, context = FocusContext(), now = now) }
        assertNull(store.activeFocusSession())
        thrown {
            store.startFocusSession(
                taskId = task.id, plannedSeconds = 300, context = FocusContext(conditionIDs = setOf(home.id)), now = now,
            )
        }
        store.startFocusSession(
            taskId = task.id, plannedSeconds = 600, context = FocusContext(conditionIDs = setOf(home.id)), now = now,
        )
    }

    @Test
    fun singleSittingQueueUsesFullRequiredDurationAndCanRequeuePartialWork() = fixture {
        val first = task("First")
        val second = task("Second")
        store.saveTaskEditor(edit(second.id).edit { copy(requiresSingleSitting = true) })
        val session = store.startFocusSession(taskId = first.id, plannedSeconds = 600, now = now)
        store.addToFocusQueue(session.id, second.id, plannedSeconds = 300, now = now)
        val result = store.completeActiveFocusTask(
            sessionId = session.id, elapsedSeconds = 600, completeTask = false, now = now,
        )
        assertEquals(3600, result.session.workDurationSeconds)
        store.addToFocusQueue(session.id, first.id, plannedSeconds = 600, now = now)
        assertEquals(3, store.focusQueue(session.id).size)
    }

    @Test
    fun applyingSavedPlanningToSubtasksDoesNotChangeTheirDeadlinesOrEstimates() = fixture {
        val parent = task("Parent")
        val child = store.createTask(listId = inbox.id, title = "Child", parentTaskId = parent.id, now = now)
        store.updateTask(child.id, child.title, "", now.plusSeconds(7200), 900, now = now)
        val home = store.conditions(workspace.id).first()
        store.saveTaskEditor(
            edit(parent.id).edit {
                copy(requirementGroups = listOf(listOf(home.id)), startAt = now, dueDate = day(now.plusSeconds(86_400)))
            },
        )
        store.applyPlanningToDescendants(parent.id)
        val saved = store.taskEditorSnapshot(child.id)
        assertEquals(listOf(listOf(home.id)), saved.planning?.requirementGroups)
        assertEquals(now, saved.planning?.startAt)
        assertNull(saved.planning?.dueDate)
        assertEquals(now.plusSeconds(7200), saved.dueAt)
        assertEquals(900, saved.estimateSeconds)
        store.undo()
        assertNull(store.taskEditorSnapshot(child.id).planning)
    }

    @Test
    fun meetingDailyTargetDoesNotHideItsOutstandingDeadline() = fixture {
        val task = task()
        store.updateTask(task.id, task.title, "", now.minusSeconds(86_400), 3600)
        val daily = store.makeDaily(taskId = task.id, targetSeconds = 600, now = now)
        store.logContribution(dailyId = daily.id, seconds = 600, now = now)
        val ranking = NextUpSelector.evaluate(store.nextUpCandidates(now = now), now = now, zone = UTC)
        assertTrue(ranking.ranked.isEmpty())
        assertEquals(task.id, ranking.blocked.first().id)
        assertEquals(listOf(TaskUnavailableReason.DailyAlreadyMet), ranking.blocked.first().reasons)
        assertEquals(NextUpReason.OVERDUE, NextUpSelector.score(ranking.blocked.first().candidate, now = now, zone = UTC).reason)
    }

    @Test
    fun rebasingWallClockKeepsActualWorkAndPausedTimeStable() = fixture {
        val task = task()
        val session = store.startFocusSession(taskId = task.id, now = now)
        val adjustedNow = now.plusSeconds(7201)
        store.rebaseFocusClock(session.id, elapsedSeconds = 601, now = adjustedNow)
        assertEquals(604, store.activeFocusSession()?.elapsedSeconds(adjustedNow.plusSeconds(3)))
        store.pauseFocusSession(session.id, now = adjustedNow.plusSeconds(3))
        store.rebaseFocusClock(session.id, elapsedSeconds = 9000, now = adjustedNow.plusSeconds(9000))
        val paused = store.activeFocusSession()!!
        assertEquals(604, paused.elapsedSeconds(adjustedNow.plusSeconds(9000)))
        store.completeActiveFocusTask(sessionId = session.id, elapsedSeconds = 604, completeTask = false, now = adjustedNow)
        assertEquals(604, store.workBlocks(task.id).first().seconds)
    }

    @Test
    fun legacyTaskUpdateSwitchesCalendarDeadlineToExactTimeWithoutLosingConditions() = fixture {
        val task = task()
        val home = store.conditions(workspace.id).first()
        store.saveTaskEditor(
            edit(task.id).edit { copy(dueDate = day(now.plusSeconds(86_400)), requirementGroups = listOf(listOf(home.id))) },
        )
        val deadline = now.plusSeconds(7200)
        store.updateTask(task.id, task.title, "Changed", deadline, 3600)
        val saved = store.taskEditorSnapshot(task.id)
        assertNull(saved.planning?.dueDate)
        assertEquals(deadline, saved.dueAt)
        assertEquals(listOf(listOf(home.id)), saved.planning?.requirementGroups)
        store.undo()
        assertNotNull(store.taskEditorSnapshot(task.id).planning?.dueDate)
    }

    @Test
    fun focusDeferralCannotSilentlyScheduleAfterDeadlineAndNoOpKeepsRedo() = fixture {
        val task = task()
        store.updateTask(task.id, task.title, "", now.plusSeconds(3600), 3600)
        thrown { store.scheduleTask(task.id, now.plusSeconds(7200)) }
        assertNull(store.taskEditorSnapshot(task.id).planning)
        store.scheduleTask(task.id, now.plusSeconds(1800))
        store.undo()
        store.scheduleTask(task.id, null)
        assertEquals("Schedule Task", store.redoableLabel())
    }

    /**
     * A device condition holds on a device of its kind without being said,
     * heads the conditions under Device, and holds nowhere else. The kind is
     * the process's, so it is put back for the tests that follow.
     */
    @Test
    fun aDeviceConditionHoldsOnlyOnThatKindOfDevice() = fixture {
        val campus = store.conditions(workspace.id).first { it.name == "Campus" }
        val tablet = store.createCondition(workspace.id, "Tablet", null, now)
        assertTrue(store.setConditionDeviceKind(tablet.id, "tablet", now))
        val all = store.conditions(workspace.id)
        assertEquals("tablet", all.first { it.id == tablet.id }.deviceKind)
        uniffi.takt_core.setThisDeviceKind("tablet")
        try {
            val holdings = ConditionHoldings.holdings(all, setOf(campus.id), emptySet())
            assertTrue(holdings.getValue(tablet.id).isDevice)
            assertEquals("this device", ConditionHoldings.note(holdings[tablet.id], all))
            assertTrue(tablet.id in ConditionHoldings.held(all, setOf(campus.id), emptySet()))
            val device = ConditionGroups.all(all).first()
            assertTrue(device.isDevice)
            assertEquals(listOf(tablet.id), device.conditions.map { it.id })
        } finally {
            uniffi.takt_core.setThisDeviceKind(null)
        }
        assertTrue(tablet.id !in ConditionHoldings.held(all, setOf(campus.id), emptySet()))
    }

    /** What a list requires, every task in it needs as well as its own. */
    @Test
    fun aListsRequirementsAreInheritedByItsTasks() = fixture {
        val campus = store.conditions(workspace.id).first { it.name == "Campus" }
        val revise = task("Revise")
        assertTrue(store.setListRequirements(inbox.id, listOf(listOf(campus.id)), now))
        assertEquals(listOf(listOf(campus.id)), store.observeListRequirements(inbox.id).first())
        assertEquals(listOf(listOf(campus.id)), store.lists(workspace.id).first { it.id == inbox.id }.requirementGroups)
        val inherited = store.observeInheritedRequirements(revise.id).first()
        assertEquals(listOf(inbox.id), inherited.map { it.listId })
        assertEquals(listOf(listOf(campus.id)), store.nextUpCandidates(now = now).first { it.id == revise.id }.requirementGroups)
        assertTrue(store.setListRequirements(inbox.id, emptyList(), now))
        assertTrue(store.observeInheritedRequirements(revise.id).first().isEmpty())
    }
}
