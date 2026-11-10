package uk.co.maybeitsadam.takt.wear.contract

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/**
 * The paths, keys and capabilities both sides of the Data Layer agree on.
 * The phone owns the workspace; the watch only ever reads [TODAY] and sends
 * [ACTION] (docs/wear-os.md).
 */
object WearPaths {
    /** The phone's `DataItem`: the day's snapshot, under [SNAPSHOT_KEY]. */
    const val TODAY = "/takt/today"

    /** A watch-to-phone message whose payload is one encoded [WearAction]. */
    const val ACTION = "/takt/action"

    /** A watch-to-phone message with no payload: publish the day again, now. */
    const val REFRESH = "/takt/refresh"

    /** The byte array in [TODAY]'s data map holding the encoded [WearSnapshot]. */
    const val SNAPSHOT_KEY = "snapshot"

    /** Declared by the phone app (`android_wear_capabilities`): the node that takes actions. */
    const val PHONE_CAPABILITY = "takt_phone"

    /** Declared by the watch app. */
    const val WATCH_CAPABILITY = "takt_watch"
}

/** One row of the watch's Today, in the order the phone's Today reads. */
@Serializable
data class WearTask(
    val id: String,
    val title: String,
    /** The list, or why the task is in the day (overdue, due today, starts today). */
    val detail: String? = null,
    /** A daily or habit: ticking it logs today rather than completing the task. */
    val isDaily: Boolean = false,
    val dailyDone: Boolean = false,
    val isRunning: Boolean = false,
    val estimateSeconds: Int? = null,
)

@Serializable
enum class WearPhase {
    @SerialName("running")
    RUNNING,

    @SerialName("paused")
    PAUSED,

    @SerialName("onBreak")
    ON_BREAK,
}

/**
 * The focus block, as the watch needs it to tick locally: elapsed is
 * [accumulated] plus, while [RUNNING][WearPhase.RUNNING], the time since
 * [startedAt], exactly as the phone's `FocusSession.elapsedSeconds`.
 */
@Serializable
data class WearSession(
    val taskId: String? = null,
    val taskTitle: String? = null,
    val phase: WearPhase,
    /** Epoch millis the block (re)started counting from. */
    val startedAt: Long,
    /** Seconds counted before [startedAt]. */
    val accumulated: Int = 0,
    /** The block's planned length, seconds. */
    val plannedSeconds: Int,
    /** Epoch millis a break ends, on a break. */
    val breakEndsAt: Long? = null,
    /**
     * A timer: the block has no length and only counts up. [plannedSeconds]
     * then carries the day the core stores for older clients, which a watch
     * that knows this flag never shows. Defaults false, so a phone that
     * predates it still decodes as a block.
     */
    val openEnded: Boolean = false,
) {
    fun elapsedSeconds(nowMillis: Long): Int {
        val running = if (phase == WearPhase.RUNNING) maxOf(0L, (nowMillis - startedAt) / 1000).toInt() else 0
        return maxOf(0, accumulated) + running
    }

    /** What is left of the plan; negative once the block runs over. */
    fun remainingSeconds(nowMillis: Long): Int = plannedSeconds - elapsedSeconds(nowMillis)

    val isTicking: Boolean get() = phase == WearPhase.RUNNING && taskId != null
}

/** Everything the watch shows, published by the phone whenever the day or the block changes. */
@Serializable
data class WearSnapshot(
    val version: Int = VERSION,
    val tasks: List<WearTask> = emptyList(),
    val session: WearSession? = null,
    /** Epoch millis the phone made this snapshot. */
    val generatedAt: Long = 0,
    /**
     * The ids of the latest actions the phone has applied, so the watch can
     * stop drawing them optimistically, and the phone can ignore a replay.
     */
    val handled: List<String> = emptyList(),
) {
    /** What is still open today: a daily already logged is done. */
    val openCount: Int get() = tasks.count { !(it.isDaily && it.dailyDone) }

    val runningTask: WearTask? get() = session?.taskId?.let { id -> tasks.firstOrNull { it.id == id } }

    /** The first open row that is not the running one. */
    val nextTask: WearTask? get() = tasks.firstOrNull { !it.isRunning && !(it.isDaily && it.dailyDone) }

    companion object {
        const val VERSION = 1
    }
}

@Serializable
enum class WearActionKind {
    @SerialName("complete")
    COMPLETE,

    @SerialName("logDaily")
    LOG_DAILY,

    @SerialName("start")
    START,

    @SerialName("pause")
    PAUSE,

    @SerialName("resume")
    RESUME,

    /** End the session; the time so far is logged, the task stays open. */
    @SerialName("stop")
    STOP,

    /** Credit the block and complete its task, as the focus notification's Done. */
    @SerialName("done")
    DONE,
}

/** Something done on the watch, for the phone to do through its own undo stack. */
@Serializable
data class WearAction(
    /** Unique per action, so a queued action sent twice is applied once. */
    val id: String,
    val action: WearActionKind,
    val taskId: String? = null,
    /** Epoch millis it was done on the watch: the day a queued daily is logged against. */
    val issuedAt: Long,
) {
    /**
     * Whether the action still makes sense at [nowMillis]. A pause tapped an
     * hour ago, landing now, would stop a block the user has since got on
     * with; ticking a task off stays true however late it arrives. The
     * iPhone's `WatchAction.isCurrent(at:)`.
     */
    fun isCurrent(nowMillis: Long): Boolean = when (action) {
        WearActionKind.COMPLETE, WearActionKind.LOG_DAILY -> true
        WearActionKind.START, WearActionKind.PAUSE, WearActionKind.RESUME, WearActionKind.STOP, WearActionKind.DONE ->
            nowMillis - issuedAt <= TIMER_ACTION_LIFETIME_MILLIS
    }

    companion object {
        /** How long a queued timer action stays worth applying. */
        const val TIMER_ACTION_LIFETIME_MILLIS = 10 * 60 * 1000L
    }
}

/** The one encoding both sides use: JSON, tolerant of fields a newer side adds. */
object WearCodec {
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    fun encode(snapshot: WearSnapshot): ByteArray = json.encodeToString(WearSnapshot.serializer(), snapshot).encodeToByteArray()

    fun decodeSnapshot(bytes: ByteArray): WearSnapshot? =
        runCatching { json.decodeFromString(WearSnapshot.serializer(), bytes.decodeToString()) }.getOrNull()

    fun encode(action: WearAction): ByteArray = json.encodeToString(WearAction.serializer(), action).encodeToByteArray()

    fun decodeAction(bytes: ByteArray): WearAction? =
        runCatching { json.decodeFromString(WearAction.serializer(), bytes.decodeToString()) }.getOrNull()

    fun encodeActions(actions: List<WearAction>): String = json.encodeToString(actionList, actions)

    fun decodeActions(text: String): List<WearAction> = runCatching { json.decodeFromString(actionList, text) }.getOrDefault(emptyList())

    private val actionList = kotlinx.serialization.builtins.ListSerializer(WearAction.serializer())
}

/**
 * What the watch draws while an action is on its way: the snapshot as it
 * will be once the phone has done [action]. The phone's next snapshot
 * replaces it, so this only has to be close.
 */
fun WearSnapshot.applying(action: WearAction): WearSnapshot {
    val nowMillis = action.issuedAt
    val running = session
    return when (action.action) {
        WearActionKind.COMPLETE, WearActionKind.LOG_DAILY -> {
            if (action.taskId != null && action.taskId == running?.taskId) return applying(action.copy(action = WearActionKind.DONE))
            copy(tasks = tasks.filterNot { it.id == action.taskId })
        }
        WearActionKind.START -> {
            val task = tasks.firstOrNull { it.id == action.taskId } ?: return this
            // A block already running takes the task into its queue on the phone.
            if (running != null && running.taskId != null) return this
            val started = WearSession(
                taskId = task.id, taskTitle = task.title, phase = WearPhase.RUNNING, startedAt = nowMillis,
                plannedSeconds = task.estimateSeconds ?: DEFAULT_BLOCK_SECONDS,
            )
            copy(
                tasks = listOf(task.copy(isRunning = true)) + tasks.filterNot { it.id == task.id },
                session = started,
            )
        }
        WearActionKind.PAUSE -> {
            if (running == null || running.phase != WearPhase.RUNNING) return this
            copy(session = running.copy(phase = WearPhase.PAUSED, accumulated = running.elapsedSeconds(nowMillis), startedAt = nowMillis))
        }
        WearActionKind.RESUME -> {
            if (running == null || running.phase != WearPhase.PAUSED) return this
            copy(session = running.copy(phase = WearPhase.RUNNING, startedAt = nowMillis))
        }
        WearActionKind.STOP -> copy(tasks = tasks.map { it.copy(isRunning = false) }, session = null)
        WearActionKind.DONE -> copy(tasks = tasks.filterNot { it.id == running?.taskId }, session = null)
    }
}

/**
 * The snapshot with every action the phone has not yet reported [handled][WearSnapshot.handled]
 * drawn on top, oldest first.
 */
fun WearSnapshot.withPending(pending: List<WearAction>): WearSnapshot {
    val done = handled.toSet()
    return pending.filterNot { it.id in done }.sortedBy { it.issuedAt }.fold(this) { snapshot, action -> snapshot.applying(action) }
}

/** The phone's own default block length, for a task with no estimate. */
const val DEFAULT_BLOCK_SECONDS = 25 * 60

/** `25m`, `1h 20m`: as the phone's `DayForecast.hoursAndMinutes`. */
fun hoursAndMinutes(seconds: Int): String {
    val minutes = maxOf(0, seconds) / 60
    if (minutes < 60) return "${minutes}m"
    val rest = minutes % 60
    return if (rest == 0) "${minutes / 60}h" else "${minutes / 60}h ${rest}m"
}

/** `4:05`, `1:02:03`; a leading minus once over. */
fun clock(seconds: Int): String {
    val sign = if (seconds < 0) "-" else ""
    val total = kotlin.math.abs(seconds)
    val h = total / 3600
    val m = (total % 3600) / 60
    val s = total % 60
    return if (h > 0) "$sign$h:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}" else "$sign$m:${s.toString().padStart(2, '0')}"
}
