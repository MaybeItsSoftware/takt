package uk.co.maybeitsadam.takt.core

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import uniffi.takt_core.FilterDefinition

// Port of Sources/TaktWorkspace/WorkspaceModels.swift (and the record types in
// TaskPlanning.swift). `Date` becomes `Instant`; a Swift `Calendar` becomes the
// `ZoneId` it would resolve days in. Weekdays keep Calendar numbering: 1 = Sunday.

data class Workspace(
    val id: String,
    val name: String,
    val createdAt: Instant,
    val updatedAt: Instant,
)

data class ListFolder(
    val id: String,
    val workspaceId: String,
    val parentFolderId: String?,
    val name: String,
    val sortOrder: Int,
    val createdAt: Instant,
    val updatedAt: Instant,
)

/** A job the workspace relies on a list to do. The name is the user's; the role is the workspace's. */
enum class TaskListRole(val raw: String) {
    INBOX("inbox");

    companion object {
        fun of(raw: String?): TaskListRole? = entries.firstOrNull { it.raw == raw }
    }
}

data class TaskList(
    val id: String,
    val workspaceId: String,
    val folderId: String?,
    val name: String,
    val colorHex: String?,
    val sortOrder: Int,
    val isArchived: Boolean,
    val systemRole: TaskListRole?,
    /** An imported wrapper whose children form the list's visible root. */
    val visibleRootTaskId: String?,
    val completedAt: Instant?,
    val createdAt: Instant,
    val updatedAt: Instant,
    /**
     * Its place among the prioritised lists, 1 the most important; null when
     * it is not prioritised. As stored: [prioritised] gives the order.
     */
    val priorityRank: Int? = null,
    /**
     * The conditions it requires, as a task keeps them (an AND of OR-groups,
     * "!id" for must not hold). Every task in it, and in the nested lists
     * under it, inherits them (`conditions/inherited.rs`).
     */
    val requirementGroups: List<List<String>> = emptyList(),
) {
    val isSystemList: Boolean get() = systemRole != null

    companion object {
        /**
         * The prioritised lists among [lists], most important first, as the
         * core orders them (`records::prioritised_lists`): by rank, a tie a
         * sync merge left broken by sidebar order. Their place is their
         * index plus one.
         */
        fun prioritised(lists: List<TaskList>): List<TaskList> = lists
            .filter { it.priorityRank != null && !it.isArchived && it.completedAt == null }
            .sortedWith(compareBy<TaskList>({ it.priorityRank }, { it.sortOrder }, { it.createdAt }, { it.id }))
    }
}

enum class TaskStatus(val raw: String) {
    OPEN("open"),
    COMPLETED("completed"),
    CANCELLED("cancelled");

    companion object {
        fun of(raw: String): TaskStatus = entries.firstOrNull { it.raw == raw } ?: OPEN
    }
}

enum class WorkspaceItemKind(val raw: String) {
    TASK("task"),
    LIST("list");

    companion object {
        fun of(raw: String?): WorkspaceItemKind? = entries.firstOrNull { it.raw == raw }
    }
}

data class WorkspaceTask(
    val id: String,
    val listId: String,
    val parentTaskId: String?,
    val title: String,
    val notes: String,
    val status: TaskStatus,
    val sortOrder: Int,
    val dueAt: Instant?,
    val estimateSeconds: Int?,
    /** Where an imported task came from; nil for a task typed into Priority. */
    val sourceSystem: String?,
    val sourceId: String?,
    val itemKind: WorkspaceItemKind?,
    val isPromoted: Boolean?,
    val archivedAt: Instant?,
    /** When the task was closed; nil whenever `status` is open. */
    val completedAt: Instant? = null,
    val createdAt: Instant,
    val updatedAt: Instant,
) {
    val isList: Boolean get() = itemKind == WorkspaceItemKind.LIST
}

data class TaskOutlineItem(val task: WorkspaceTask, val depth: Int) {
    val id: String get() = task.id
}

data class TaskMetadata(
    val taskId: String,
    val priority: Int?,
    val startAt: Instant?,
    val tagsJSON: String,
    val recurrenceRule: String?,
    val matrixUrgency: Int?,
    val matrixImportance: Int?,
    val kanbanColumn: String?,
    val externalLinksJSON: String,
    /** Hand-placed position in the focus ladder; nil means ranked by score. */
    val focusRank: Int? = null,
    val updatedAt: Instant,
    val planningJSON: String? = null,
    /** Who or what the task waits on, while it is in `waiting-on`. See [WaitingFollowUp]. */
    val waitingOn: String? = null,
    /** When to chase it. */
    val waitingFollowUpAt: Instant? = null,
    /** The follow-up already made for this task, so it is made once per time. */
    val waitingFollowUpTaskId: String? = null,
    /** On a follow-up task: the waiting task it chases. */
    val followUpOfTaskId: String? = null,
    /** Its place within its matrix quadrant, lowest first (`v22_matrix_order`). */
    val matrixRank: Int? = null,
    /** Chase again every this many days after a follow-up is done; null is once (`v32_follow_up_repeat`). */
    val waitingFollowUpEveryDays: Int? = null,
    /** How many follow-ups this spell of waiting has made. */
    val waitingFollowUpCount: Int = 0,
) {
    /** What a card adds after the follow-up time: `#3 · every 3d`, or null. See [WaitingFollowUp.chase]. */
    val followUpChase: String?
        get() = waitingFollowUpAt?.let {
            WaitingFollowUp.chase(taskId, it, waitingFollowUpTaskId, waitingFollowUpCount, waitingFollowUpEveryDays)
        }
}

data class TaskEditorMetadata(
    val priority: Int? = null,
    val tags: List<String> = emptyList(),
    val recurrenceRule: String? = null,
    val externalLinks: List<String> = emptyList(),
)

/**
 * A board column. The task's column is independent of its parent's column.
 * [rules] make it a rule column: a task they match is drawn in it whatever
 * column it is filed in, and nothing is filed in it (the Rust core's
 * `board_columns`).
 */
data class WorkspaceKanbanColumn(val id: String, val title: String, val rules: FilterDefinition? = null) {
    val isRuleColumn: Boolean get() = rules != null

    companion object {
        val blitzitDefaults = listOf(
            WorkspaceKanbanColumn("backlog", "Backlog"),
            WorkspaceKanbanColumn("in-progress", "In progress"),
            WorkspaceKanbanColumn("this-week", "This week"),
            WorkspaceKanbanColumn("waiting-on", "Waiting on"),
            WorkspaceKanbanColumn("today", "Today"),
        )
    }
}

/**
 * A board's columns as the Rust core resolves them (`board_columns`): the
 * [layout] an edit saves, where it comes from ("own", "list", "default" or
 * "builtin"), the [columns] drawn left to right, and the tasks drawn
 * somewhere other than their filed column says, by a rule column or clamped
 * to an entered card's [floorColumnId].
 */
data class BoardColumnsRead(
    val key: String,
    val layout: List<WorkspaceKanbanColumn>,
    val source: String,
    val columns: List<WorkspaceKanbanColumn>,
    val hiddenColumnIds: Set<String> = emptySet(),
    val floorColumnId: String? = null,
    val placements: Map<String, String> = emptyMap(),
) {
    /** The column a task is drawn in: its placement, else its filed column when drawn, else the first. */
    fun columnId(taskId: String, filed: String?): String? {
        placements[taskId]?.let { placed -> if (columns.any { it.id == placed }) return placed }
        val wanted = filed ?: WorkspaceKanbanColumn.blitzitDefaults[0].id
        return if (columns.any { it.id == wanted }) wanted else columns.firstOrNull()?.id
    }

    companion object {
        /** The stock columns, for a board not read from the core yet. */
        fun builtin(key: String) = BoardColumnsRead(
            key, WorkspaceKanbanColumn.blitzitDefaults, "builtin", WorkspaceKanbanColumn.blitzitDefaults,
        )
    }
}

/** A task's matrix quadrant, and its place within it: [rank] lowest first, null after the ranked ones. */
data class TaskMatrixPosition(val urgency: Int?, val importance: Int?, val rank: Int? = null)

enum class FocusSessionPhase(val raw: String) {
    RUNNING("running"),
    ON_BREAK("onBreak"),
    FINISHED("finished");

    companion object {
        fun of(raw: String): FocusSessionPhase = entries.firstOrNull { it.raw == raw } ?: FINISHED
    }
}

data class FocusSession(
    val id: String,
    val startedAt: Instant,
    val endedAt: Instant?,
    val phase: FocusSessionPhase,
    val activeTaskId: String?,
    /** When the current task's block began, as distinct from the session's own start. */
    val activeTaskStartedAt: Instant,
    val workDurationSeconds: Int,
    val breakDurationSeconds: Int,
    val breakEndsAt: Instant?,
    val activeBlockId: String?,
    val accumulatedSeconds: Int?,
    val pausedAt: Instant?,
    val checkpointAt: Instant?,
    /**
     * A timer: the block in hand has no length and counts up until it is
     * finished. [workDurationSeconds] then holds only a day, a stand-in for
     * clients that predate the flag, never a length to count down from.
     */
    val openEnded: Boolean = false,
) {
    fun elapsedSeconds(now: Instant): Int {
        val running = if (pausedAt == null) {
            maxOf(0.0, secondsBetween(activeTaskStartedAt, now)).toInt()
        } else {
            0
        }
        return maxOf(0, accumulatedSeconds ?: 0) + running
    }
}

enum class FocusQueueState(val raw: String) {
    QUEUED("queued"),
    COMPLETED("completed"),
    SKIPPED("skipped");

    companion object {
        fun of(raw: String): FocusQueueState = entries.firstOrNull { it.raw == raw } ?: QUEUED
    }
}

data class FocusQueueItem(
    val id: String,
    val sessionId: String,
    val taskId: String,
    val sortOrder: Int,
    val state: FocusQueueState,
    /** What you committed to when you started this sitting. */
    val plannedSeconds: Int?,
    val completedAt: Instant?,
    val skippedAt: Instant?,
    val createdAt: Instant,
    /** Runs as a timer, counting up, rather than as a block of a length. */
    val openEnded: Boolean = false,
)

data class FocusQueueTask(val item: FocusQueueItem, val task: WorkspaceTask) {
    val id: String get() = item.id
}

/**
 * A standing commitment to move one task forward today. Fixed weekdays, or a
 * rotating interval, never both; `intervalDays` wins when set.
 */
data class WorkspaceDaily(
    val id: String,
    val taskId: String,
    /** Calendar weekday numbering (1 = Sunday) as a bitmask, bit `n - 1` per weekday. */
    val activeWeekdaysMask: Int = ALL_WEEKDAYS_MASK,
    val intervalDays: Int? = null,
    val intervalAnchor: Instant? = null,
    val targetSeconds: Int? = null,
    val sortOrder: Int,
    val archivedAt: Instant? = null,
    val legacyDailyId: String? = null,
    val createdAt: Instant,
    val updatedAt: Instant,
    /** The task this habit was made from. Nil for a standalone habit and every plain daily. */
    val sourceTaskId: String? = null,
    /** The board column each appearance lands in ([HabitPlacement]). Nil is a plain daily. */
    val placementColumn: String? = null,
    /** Whether an appearance not done by the end of its day is dropped rather than carried. */
    val dropsAtDayEnd: Boolean = true,
    /** [HabitExpiry.rule]: "source", "date" or "never". */
    val expiryRule: String = "never",
    /** The day a "date" expiry takes effect. */
    val expiresAt: Instant? = null,
) {
    val isArchived: Boolean get() = archivedAt != null

    val activeWeekdays: Set<Int> get() = (1..7).filter { activeWeekdaysMask and (1 shl (it - 1)) != 0 }.toSet()

    /** A daily made through the habit form: it has a column to land in. */
    val isHabit: Boolean get() = placement != null

    val placement: HabitPlacement? get() = HabitPlacement.of(placementColumn)

    val expiry: HabitExpiry get() = HabitExpiry.of(expiryRule, expiresAt)

    val frequency: HabitFrequency get() = HabitFrequency.of(activeWeekdays, intervalDays)

    /** The schedule and options as [HabitPolicy] reads them. */
    val habitRule: HabitRule
        get() = HabitRule(
            weekdays = activeWeekdays, intervalDays = intervalDays, anchor = intervalAnchor ?: createdAt,
            dropsAtDayEnd = dropsAtDayEnd, expiry = expiry, placement = placement ?: HabitPlacement.TODAY,
        )

    /** Whether this daily is expected on `day`: the Rust core's rule, the one its next-up read applies. */
    fun isDue(day: Instant, zone: ZoneId = ZoneId.systemDefault()): Boolean {
        if (isArchived) return false
        return uniffi.takt_core.workspaceDailyIsDue(
            weekdaysMask = activeWeekdaysMask.toLong(),
            intervalDays = intervalDays?.toLong(),
            anchorMs = (intervalAnchor ?: createdAt).coreMillis,
            dayMs = day.coreMillis,
            zone = zone.coreName,
        )
    }

    companion object {
        const val ALL_WEEKDAYS_MASK = 0b111_1111

        /** The normalising initialiser: a zero mask means every day, the interval is clamped to 1...366. */
        fun make(
            id: String,
            taskId: String,
            activeWeekdaysMask: Int = ALL_WEEKDAYS_MASK,
            intervalDays: Int? = null,
            intervalAnchor: Instant? = null,
            targetSeconds: Int? = null,
            sortOrder: Int,
            archivedAt: Instant? = null,
            legacyDailyId: String? = null,
            createdAt: Instant,
            updatedAt: Instant,
        ) = WorkspaceDaily(
            id = id,
            taskId = taskId,
            activeWeekdaysMask = if (activeWeekdaysMask == 0) ALL_WEEKDAYS_MASK else activeWeekdaysMask,
            intervalDays = intervalDays?.coerceIn(1, 366),
            intervalAnchor = intervalAnchor,
            targetSeconds = targetSeconds,
            sortOrder = sortOrder,
            archivedAt = archivedAt,
            legacyDailyId = legacyDailyId,
            createdAt = createdAt,
            updatedAt = updatedAt,
        )

        fun mask(weekdays: Set<Int>): Int = weekdays.fold(0) { acc, d -> acc or (1 shl (d - 1)) }
    }
}

/** One day's progress against a daily's task, keyed by `yyyy-MM-dd` in the user's zone. */
data class DailyContribution(
    val id: String,
    val dailyId: String,
    val taskId: String,
    val dayKey: String,
    val secondsLogged: Int,
    val completedAt: Instant?,
    val createdAt: Instant,
) {
    val isComplete: Boolean get() = completedAt != null

    companion object {
        private val dayKeyFormatter = DateTimeFormatter.ofPattern("yyyy-MM-dd")

        fun dayKey(date: Instant, zone: ZoneId = ZoneId.systemDefault()): String =
            date.atZone(zone).toLocalDate().format(dayKeyFormatter)
    }
}

data class DailyItem(
    val daily: WorkspaceDaily,
    val task: WorkspaceTask,
    val contribution: DailyContribution?,
) {
    val id: String get() = daily.id
    val isDoneToday: Boolean get() = contribution?.isComplete ?: false
    val secondsLoggedToday: Int get() = contribution?.secondsLogged ?: 0
}

data class TaskCondition(
    val id: String,
    val workspaceId: String,
    val name: String,
    val isLocation: Boolean,
    val isArchived: Boolean,
    val createdAt: Instant,
    val updatedAt: Instant,
    /** The category as stored ("Location", "Equipment"), or null; the core's `condition_sections` reads it. */
    val category: String? = null,
    /** Where it sits in the conditions' order, workspace-wide; null for one nobody has placed. */
    val sortOrder: Long? = null,
    /** The conditions it switches on while it holds (Home: Floor space, Desk), by id. */
    val provides: List<String> = emptyList(),
    /**
     * The kind of device it means ("laptop", "phone", "tablet"), or null for
     * an ordinary condition. A device condition is never said to hold: it
     * holds on a device of its kind and nowhere else (`conditions/device.rs`).
     */
    val deviceKind: String? = null,
) {
    val isDevice: Boolean get() = !deviceKind.isNullOrBlank()
}

data class FocusWorkBlock(
    val id: String,
    val sessionId: String?,
    val taskId: String?,
    val taskTitle: String,
    val seconds: Int,
    val recordedAt: Instant,
    val originalTaskId: String?,
)

data class FocusAward(
    val id: String,
    val sessionId: String?,
    val taskId: String?,
    val taskTitle: String,
    val seconds: Int,
    val minutes: Double,
    val multiplier: Double,
    val points: Double,
    val awardedAt: Instant,
) {
    /** Extended in FocusPoints.kt with `earned(...)`, Swift's deriving initialiser. */
    companion object
}

/** `Calendar.component(.weekday)`: 1 = Sunday ... 7 = Saturday. */
fun calendarWeekday(date: LocalDate): Int = date.dayOfWeek.value % 7 + 1

/** `Date.timeIntervalSince` as fractional seconds. */
fun secondsBetween(from: Instant, to: Instant): Double =
    (to.epochSecond - from.epochSecond) + (to.nano - from.nano) / 1_000_000_000.0

/** `Calendar.current.firstWeekday` for the default locale: 1 = Sunday, 2 = Monday. */
fun defaultFirstWeekday(locale: java.util.Locale = java.util.Locale.getDefault()): Int =
    calendarWeekday(LocalDate.of(2024, 1, 1).with(java.time.temporal.WeekFields.of(locale).dayOfWeek(), 1))
