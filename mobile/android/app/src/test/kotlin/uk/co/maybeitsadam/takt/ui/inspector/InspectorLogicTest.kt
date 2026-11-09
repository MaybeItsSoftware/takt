package uk.co.maybeitsadam.takt.ui.inspector

import java.time.DayOfWeek
import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime
import java.time.YearMonth
import java.time.ZoneId
import java.time.ZoneOffset
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uk.co.maybeitsadam.takt.core.MatrixQuadrant
import uk.co.maybeitsadam.takt.core.PeriodicSchedule
import uk.co.maybeitsadam.takt.core.TaskEditorMetadata
import uk.co.maybeitsadam.takt.core.TaskMatrixPosition
import uk.co.maybeitsadam.takt.core.TaskPlanning
import uk.co.maybeitsadam.takt.core.TaskPlanningError
import uk.co.maybeitsadam.takt.core.TaskList
import uk.co.maybeitsadam.takt.core.TaskStatus
import uk.co.maybeitsadam.takt.core.WorkspaceItemKind
import uk.co.maybeitsadam.takt.core.WorkspaceTask
import uk.co.maybeitsadam.takt.data.workspace.TaskDependencies
import uk.co.maybeitsadam.takt.data.workspace.TaskSearchResult
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorDraft
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorField
import uk.co.maybeitsadam.takt.data.workspace.TaskEditorSnapshot

class DateFieldMathTest {
    private val london: ZoneId = ZoneId.of("Europe/London")

    @Test
    fun monthGridPadsToWholeMondayFirstWeeks() {
        // October 2026 starts on a Thursday and has 31 days.
        val grid = DateFieldMath.monthGrid(YearMonth.of(2026, 10))
        assertEquals(5, grid.size)
        assertTrue(grid.all { it.size == 7 })
        assertEquals(listOf(null, null, null), grid[0].take(3))
        assertEquals(LocalDate.of(2026, 10, 1), grid[0][3])
        assertEquals(LocalDate.of(2026, 10, 31), grid[4][5])
        assertNull(grid[4][6])
        assertEquals(31, grid.flatten().count { it != null })
    }

    @Test
    fun monthGridStartingOnTheFirstDayHasNoLeadingPadding() {
        // February 2027 starts on a Monday and is exactly four weeks.
        val grid = DateFieldMath.monthGrid(YearMonth.of(2027, 2))
        assertEquals(4, grid.size)
        assertEquals(LocalDate.of(2027, 2, 1), grid[0][0])
        val sundayFirst = DateFieldMath.monthGrid(YearMonth.of(2027, 2), DayOfWeek.SUNDAY)
        assertNull(sundayFirst[0][0])
        assertEquals(LocalDate.of(2027, 2, 1), sundayFirst[0][1])
    }

    @Test
    fun weekdayHeadersFollowTheFirstDay() {
        assertEquals(listOf("Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"), DateFieldMath.weekdayHeaders())
        assertEquals("Su", DateFieldMath.weekdayHeaders(DayOfWeek.SUNDAY).first())
    }

    @Test
    fun hoursWrapRoundTheDay() {
        assertEquals(LocalTime.of(0, 30), DateFieldMath.stepHour(LocalTime.of(23, 30), 1))
        assertEquals(LocalTime.of(23, 0), DateFieldMath.stepHour(LocalTime.of(0, 0), -1))
    }

    @Test
    fun minutesSnapToTheStepThenWrapWithinTheHour() {
        assertEquals(LocalTime.of(9, 10), DateFieldMath.stepMinute(LocalTime.of(9, 7), 1))
        assertEquals(LocalTime.of(9, 5), DateFieldMath.stepMinute(LocalTime.of(9, 7), -1))
        assertEquals(LocalTime.of(9, 15), DateFieldMath.stepMinute(LocalTime.of(9, 10), 1))
        assertEquals(LocalTime.of(9, 0), DateFieldMath.stepMinute(LocalTime.of(9, 55), 1))
        assertEquals(LocalTime.of(9, 55), DateFieldMath.stepMinute(LocalTime.of(9, 0), -1))
    }

    @Test
    fun movingTheDayKeepsTheWallClockAcrossDaylightSaving() {
        // 17:00 BST on Friday 23 October 2026; the clocks go back on the 25th.
        val due = LocalDate.of(2026, 10, 23).atTime(17, 0).atZone(london).toInstant()
        val moved = DateFieldMath.withDate(due, LocalDate.of(2026, 10, 26), london)
        assertEquals(LocalTime.of(17, 0), DateFieldMath.time(moved, london))
        assertEquals(LocalDate.of(2026, 10, 26), DateFieldMath.date(moved, london))
        val retimed = DateFieldMath.withTime(moved, LocalTime.of(8, 15), london)
        assertEquals(LocalDate.of(2026, 10, 26).atTime(8, 15).atZone(london).toInstant(), retimed)
    }

    @Test
    fun defaultStartIsTomorrowAtNine() {
        val now = Instant.parse("2026-10-02T21:30:00Z")
        assertEquals(Instant.parse("2026-10-03T09:00:00Z"), DateFieldMath.defaultStart(now, ZoneOffset.UTC))
    }
}

class DueEditingTest {
    private val utc = ZoneOffset.UTC
    private val values = snapshot().values

    @Test
    fun aDayIsStoredAsACalendarDateAndAnExactTimeAsAnInstant() {
        val day = DueEditing.setDay(values, LocalDate.of(2026, 10, 9), utc)
        assertEquals("2026-10-09", day.dueDate)
        assertNull(day.dueAt)
        assertEquals(DueMode.DAY, DueEditing.mode(day))

        val exact = DueEditing.setExact(day, true, utc, LocalDate.of(2026, 10, 2))
        assertEquals(Instant.parse("2026-10-09T17:00:00Z"), exact.dueAt)
        assertNull(exact.dueDate)
        assertEquals(DueMode.TIME, DueEditing.mode(exact))

        val moved = DueEditing.setDay(exact, LocalDate.of(2026, 10, 12), utc)
        assertEquals(Instant.parse("2026-10-12T17:00:00Z"), moved.dueAt)

        val back = DueEditing.setExact(moved, false, utc, LocalDate.of(2026, 10, 2))
        assertEquals("2026-10-12", back.dueDate)
        assertNull(back.dueAt)

        val cleared = DueEditing.setDay(back, null, utc)
        assertEquals(DueMode.NONE, DueEditing.mode(cleared))
    }

    @Test
    fun settingATimeOnADayMakesItExact() {
        val day = DueEditing.setDay(values, LocalDate.of(2026, 10, 9), utc)
        val timed = DueEditing.setTime(day, LocalTime.of(8, 30), utc)
        assertEquals(Instant.parse("2026-10-09T08:30:00Z"), timed.dueAt)
        assertNull(timed.dueDate)
        // No day yet: nothing to put a time on.
        assertSame(values, DueEditing.setTime(values, LocalTime.of(8, 30), utc))
    }
}

class RecurrencePresetsTest {
    @Test
    fun everyPresetRoundTripsThroughPeriodicSchedule() {
        for (day in 0L until 7L) {
            for (preset in RecurrencePresets.presets(LocalDate.of(2026, 10, 5).plusDays(day))) {
                val schedule = PeriodicSchedule.parse(preset.rule)
                requireNotNull(schedule) { "${preset.rule} does not parse" }
                assertEquals(preset.rule, schedule.raw)
                assertEquals(preset.label, schedule.displayLabel)
            }
        }
    }

    @Test
    fun theWeekdayPresetFollowsTheDueDay() {
        assertEquals("every friday", RecurrencePresets.weekdayPreset(LocalDate.of(2026, 10, 9)).rule)
        assertEquals("every monday", RecurrencePresets.weekdayPreset(null).rule)
    }

    @Test
    fun aRuleSelectsThePresetWithTheSameCadence() {
        val presets = RecurrencePresets.presets(null)
        assertEquals("weekly", RecurrencePresets.selected("every week", presets)?.rule)
        assertEquals("daily", RecurrencePresets.selected("Every Day", presets)?.rule)
        assertEquals("every monday", RecurrencePresets.selected("mon", presets)?.rule)
        assertNull(RecurrencePresets.selected("every 5 days", presets))
        assertNull(RecurrencePresets.selected("", presets))
    }

    @Test
    fun theDescriptionIsPeriodicSchedulesOwn() {
        assertEquals(RuleDescription("Does not repeat", true), RecurrencePresets.describe(" "))
        assertEquals(RuleDescription("Repeats: Every 4 weeks", true), RecurrencePresets.describe("every 4 wks"))
        assertFalse(RecurrencePresets.describe("fortnightly").valid)
    }
}

class RequirementGroupsTest {
    @Test
    fun groupsAreAddedAlternativesJoinThemAndEmptiesCollapseToNull() {
        var groups: List<List<String>>? = null
        groups = RequirementGroups.addGroup(groups, "desk")
        groups = RequirementGroups.addGroup(groups, "online")
        assertEquals(listOf(listOf("desk"), listOf("online")), groups)

        // The same single-condition group twice is refused.
        assertSame(groups, RequirementGroups.addGroup(groups, "desk"))

        groups = RequirementGroups.addAlternative(groups, 0, "office")
        assertEquals(listOf(listOf("desk", "office"), listOf("online")), groups)
        assertSame(groups, RequirementGroups.addAlternative(groups, 0, "office"))
        assertSame(groups, RequirementGroups.addAlternative(groups, 7, "office"))

        groups = RequirementGroups.remove(groups, 0, "desk")
        assertEquals(listOf(listOf("office"), listOf("online")), groups)
        groups = RequirementGroups.remove(groups, 1, "online")
        assertEquals(listOf(listOf("office")), groups)
        groups = RequirementGroups.removeGroup(groups, 0)
        assertNull(groups)
    }

    @Test
    fun anAlternativeThatWouldDuplicateAnotherGroupIsRefused() {
        val groups = listOf(listOf("desk", "office"), listOf("desk"))
        assertSame(groups, RequirementGroups.addAlternative(groups, 1, "office"))
    }

    @Test
    fun removingFromOneGroupMergesIntoAnIdenticalGroup() {
        val groups = listOf(listOf("desk", "office"), listOf("desk"))
        assertEquals(listOf(listOf("desk")), RequirementGroups.remove(groups, 0, "office"))
    }

    @Test
    fun candidatesSkipArchivedAndAlreadyPresentConditions() {
        data class C(val id: String, val archived: Boolean = false)
        val all = listOf(C("desk"), C("office"), C("old", archived = true))
        val groups = listOf(listOf("desk"))
        assertEquals(listOf(C("office")), RequirementGroups.candidatesForNewGroup(groups, all, C::id, C::archived))
        assertEquals(listOf(C("office")), RequirementGroups.candidatesForGroup(groups[0], all, C::id, C::archived))
    }
}

class InspectorValidationTest {
    private val utc = ZoneOffset.UTC

    @Test
    fun aCleanDraftHasNoProblems() {
        assertEquals(emptyList<InspectorProblem>(), InspectorValidation.problems(TaskEditorDraft(snapshot()), utc))
    }

    @Test
    fun titleEstimateAndMinimumBlockAreCheckedBeforeSaving() {
        val draft = TaskEditorDraft(snapshot()).let {
            it.copy(values = it.values.copy(title = "  ", estimateMinutes = "ten", minimumBlockMinutes = "0.5"))
        }
        val fields = InspectorValidation.problems(draft, utc).map { it.field }
        assertEquals(listOf(TaskEditorField.TITLE, TaskEditorField.ESTIMATE_MINUTES, TaskEditorField.MINIMUM_BLOCK), fields)
    }

    @Test
    fun oneSittingNeedsAnEstimateAtLeastTheMinimumBlock() {
        val base = TaskEditorDraft(snapshot(estimateSeconds = null))
        val noEstimate = base.copy(values = base.values.copy(requiresSingleSitting = true))
        assertEquals(
            TaskPlanningError.ESTIMATE_REQUIRED.message,
            InspectorValidation.problems(noEstimate, utc).single().message,
        )
        val enough = noEstimate.copy(values = noEstimate.values.copy(estimateMinutes = "30", minimumBlockMinutes = "20"))
        assertEquals(emptyList<InspectorProblem>(), InspectorValidation.problems(enough, utc))
        val tooShort = enough.copy(values = enough.values.copy(minimumBlockMinutes = "45"))
        assertEquals(TaskEditorField.SINGLE_SITTING, InspectorValidation.problems(tooShort, utc).single().field)
    }

    @Test
    fun startMustBeBeforeTheDeadlineButOnlyWhenTheScheduleChanged() {
        val draft = TaskEditorDraft(snapshot())
        val late = draft.copy(
            values = draft.values.copy(dueDate = "2026-10-09", startAt = Instant.parse("2026-10-10T00:00:00Z")),
        )
        assertEquals(TaskEditorField.START_AT, InspectorValidation.problems(late, utc).single().field)
        // The due day runs to midnight, so a start that evening is fine.
        val sameDay = late.copy(values = late.values.copy(startAt = Instant.parse("2026-10-09T20:00:00Z")))
        assertEquals(emptyList<InspectorProblem>(), InspectorValidation.problems(sameDay, utc))

        val saved = snapshot(planning = TaskPlanning(startAt = Instant.parse("2026-10-10T00:00:00Z"), dueDate = "2026-10-09"))
        val untouched = TaskEditorDraft(saved).let { it.copy(values = it.values.copy(notes = "More")) }
        assertEquals(emptyList<InspectorProblem>(), InspectorValidation.problems(untouched, utc))
    }
}

class MatrixPickerTest {
    @Test
    fun quadrantsRoundTripThroughTheirRepresentativePosition() {
        for (quadrant in MatrixQuadrant.entries) {
            assertEquals(quadrant, MatrixPicker.quadrant(MatrixPicker.position(quadrant)))
        }
        assertEquals(TaskMatrixPosition(null, null), MatrixPicker.position(null))
        assertNull(MatrixPicker.quadrant(TaskMatrixPosition(0, 0)))
        assertNull(MatrixPicker.quadrant(TaskMatrixPosition(null, 3)))
    }
}

class TextListsTest {
    @Test
    fun linksAndTagsAreSplitAsTheStoreKeepsThem() {
        assertEquals(listOf("https://a.example", "obsidian://x"), linkLines(" https://a.example \n\n obsidian://x\r\n"))
        assertEquals(listOf("work", "launch"), tagList("work, #launch, Work, "))
        assertEquals("work, launch", joinTags(listOf("work", "launch")))
        assertEquals("!1", PriorityLabels.label(1))
        assertEquals("None", PriorityLabels.label(0))
        // Behind every task with none: written as the capture syntax writes it.
        assertEquals("!-1", PriorityLabels.label(-1))
        assertEquals("None", PriorityLabels.label(-5))
        assertEquals("Priority -1, last", PriorityLabels.description(-1))
        assertEquals(listOf(-4, -3, -2, -1), PriorityLabels.later)
    }
}

class DailyScheduleTest {
    @Test
    fun weekdaysToggleButNeverToNone() {
        assertEquals(setOf(2, 3), DailySchedule.toggle(setOf(2), 3))
        assertEquals(setOf(3), DailySchedule.toggle(setOf(2, 3), 2))
        assertEquals(setOf(3), DailySchedule.toggle(setOf(3), 3))
        assertEquals(listOf(2, 3, 4, 5, 6, 7, 1), DailySchedule.weekdayChips.map { it.first })
    }

    @Test
    fun intervalStaysInRange() {
        assertEquals(1, DailySchedule.stepInterval(1, -1))
        assertEquals(3, DailySchedule.stepInterval(2, 1))
        assertEquals(366, DailySchedule.stepInterval(366, 1))
    }

    @Test
    fun targetStepsInFivesAndClearsAtZero() {
        assertEquals(300, DailySchedule.stepTarget(null, 1))
        assertNull(DailySchedule.stepTarget(300, -1))
        assertEquals(1_200, DailySchedule.stepTarget(1_080, 1))
        assertEquals(900, DailySchedule.stepTarget(1_080, -1))
        assertNull(DailySchedule.stepTarget(null, -1))
    }
}

internal fun snapshot(
    estimateSeconds: Int? = 1800,
    planning: TaskPlanning? = null,
) = TaskEditorSnapshot(
    workspaceId = "W", taskId = "T", title = "Write report", notes = "", dueAt = null,
    estimateSeconds = estimateSeconds, metadata = TaskEditorMetadata(), dailyProgress = false, planning = planning,
)

class BlockerPickerTest {
    private val t0 = Instant.parse("2026-01-01T00:00:00Z")
    private val list = TaskList("home", "w", null, "Home", null, 0, false, null, null, null, t0, t0)

    private fun task(id: String, kind: WorkspaceItemKind? = null) = WorkspaceTask(
        id, "home", null, id, "", TaskStatus.OPEN, 0, null, null, null, null, kind, null, null,
        createdAt = t0, updatedAt = t0,
    )

    @Test
    fun offersOtherTasksNotAlreadyLinkedEitherWay() {
        val results = listOf("me", "blocker", "waiter", "free", "nested").map {
            TaskSearchResult(task(it, kind = if (it == "nested") WorkspaceItemKind.LIST else null), list, null)
        }
        val dependencies = TaskDependencies(blockers = listOf(task("blocker")), blocking = listOf(task("waiter")))
        assertEquals(listOf("free"), BlockerPicker.candidates(results, "me", dependencies).map { it.task.id })
        val many = (1..20).map { TaskSearchResult(task("t$it"), list, null) }
        assertEquals(BlockerPicker.LIMIT, BlockerPicker.candidates(many, "me", TaskDependencies()).size)
    }
}
