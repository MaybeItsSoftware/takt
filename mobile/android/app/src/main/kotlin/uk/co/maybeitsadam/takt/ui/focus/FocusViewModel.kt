package uk.co.maybeitsadam.takt.ui.focus

import androidx.compose.runtime.Immutable
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.core.stringSetPreferencesKey
import java.time.Instant
import java.time.ZoneId
import uniffi.takt_core.StoredFocusContext
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.ImmutableMap
import kotlinx.collections.immutable.ImmutableSet
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.persistentMapOf
import kotlinx.collections.immutable.persistentSetOf
import kotlinx.collections.immutable.toImmutableList
import kotlinx.collections.immutable.toImmutableMap
import kotlinx.collections.immutable.toImmutableSet
import uniffi.takt_core.ConditionHolding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.app.AppContainer
import uk.co.maybeitsadam.takt.core.BlockedFocusTask
import uk.co.maybeitsadam.takt.core.FocusAward
import uk.co.maybeitsadam.takt.core.FocusContext
import uk.co.maybeitsadam.takt.core.FocusPointsSummary
import uk.co.maybeitsadam.takt.core.FocusQueueState
import uk.co.maybeitsadam.takt.core.FocusQueueTask
import uk.co.maybeitsadam.takt.core.FocusSession
import uk.co.maybeitsadam.takt.core.FocusSessionPhase
import uk.co.maybeitsadam.takt.core.FocusTimeMode
import uk.co.maybeitsadam.takt.core.ScoredNextUp
import uk.co.maybeitsadam.takt.core.TaskAvailabilityPolicy
import uk.co.maybeitsadam.takt.core.TaskCondition
import uk.co.maybeitsadam.takt.core.TaskPlanningError
import uk.co.maybeitsadam.takt.core.TaskPlanningException
import uk.co.maybeitsadam.takt.core.WorkspaceNextUpSnapshot
import uk.co.maybeitsadam.takt.data.workspace.ConditionHoldings
import uk.co.maybeitsadam.takt.data.workspace.FocusCompletionOutcome
import uk.co.maybeitsadam.takt.data.workspace.emptyFocusContext
import uk.co.maybeitsadam.takt.data.workspace.startClash
import uk.co.maybeitsadam.takt.ui.today.dayTicks
import uk.co.maybeitsadam.takt.ui.today.minuteTicks

/** A start the context rules out, held for the user to confirm. */
@Immutable
data class StartOverride(val taskId: String, val title: String, val plannedSeconds: Int, val explanation: String)

/** What the last finished block earned, shown until dismissed. */
@Immutable
data class LastBlock(val award: FocusAward?, val outcome: FocusCompletionOutcome)

/** Where you are, how long you have, and whether to make progress or finish. */
@Immutable
data class FocusContextState(
    /** The conditions said to hold: a place chosen, a box ticked. */
    val selected: ImmutableSet<String> = persistentSetOf(),
    /** Provided conditions switched off for now (housemates in: Quiet off). */
    val off: ImmutableSet<String> = persistentSetOf(),
    /** What holds: [selected], everything it provides, less [off]. The core's rule, as the Mac's. */
    val held: ImmutableSet<String> = persistentSetOf(),
    /** Each condition said, provided or off, by id: what its row notes ("from Home", "off for now"). */
    val holdings: ImmutableMap<String, ConditionHolding> = persistentMapOf(),
    val mode: FocusTimeMode = FocusTimeMode.PROGRESS,
    val time: AvailableTime = AvailableTime.Unlimited,
    val endsAt: Instant? = null,
    /** When the conditions on were said, here or on another device. */
    val conditionsSetAt: Instant? = null,
    /** Said on an earlier day: probably not where you are now. The core's rule, as the Mac's. */
    val isStale: Boolean = false,
) {
    /** Tasks are checked against what holds, what the settings provide included. */
    val context: FocusContext get() = FocusContext(held, endsAt, mode)
}

/** Everything the Focus screen draws. */
@Immutable
data class FocusUiState(
    val ladder: ImmutableList<ScoredNextUp> = persistentListOf(),
    val blocked: ImmutableList<BlockedFocusTask> = persistentListOf(),
    val conditions: ImmutableList<TaskCondition> = persistentListOf(),
    /** The conditions an open task requires: only these are offered under "Where you are". */
    val requiredConditionIds: ImmutableSet<String> = persistentSetOf(),
    val hasManualOrder: Boolean = false,
    val session: FocusSession? = null,
    val activeTitle: String? = null,
    val queue: ImmutableList<FocusQueueTask> = persistentListOf(),
    val points: FocusPointsSummary = FocusPointsSummary.ZERO,
    val isLoaded: Boolean = false,
) {
    /** A block with a task: the running view replaces the ladder. */
    val isRunning: Boolean get() = session != null && session.phase != FocusSessionPhase.FINISHED
}

// Where the context lived before it was the workspace's; moved in once.
private const val CONDITIONS_KEY = "focus.conditions"
private const val MODE_KEY = "focus.mode"

/** The Focus screen: the context, the ladder ranked for it, the staged task, and the running block. */
@OptIn(ExperimentalCoroutinesApi::class)
class FocusViewModel(private val container: AppContainer) : ViewModel() {
    private val actions = FocusActions(container)
    private val settings = container.settings

    /** How the time you have was chosen here ("30m"), with the end it gave, so the control keeps its label. */
    private val time = MutableStateFlow<Pair<AvailableTime, Instant?>>(AvailableTime.Unlimited to null)

    /**
     * What holds now, as the workspace keeps it: shared through sync with the
     * Mac and the iPhone, and set by the CLI and the agent too.
     */
    private val stored: StateFlow<StoredFocusContext?> = container.withSession { session ->
        session.repository.observeFocusContext(session.workspace.id)
    }.stateIn(viewModelScope, SharingStarted.Eagerly, null)

    /** Every condition, for working out what the ones said provide. */
    private val allConditions: StateFlow<List<TaskCondition>> = container.withSession { session ->
        session.repository.observeConditions(session.workspace.id)
    }.stateIn(viewModelScope, SharingStarted.Eagerly, emptyList())

    val contextState: StateFlow<FocusContextState> = combine(
        stored, allConditions, time, minuteTicks(),
    ) { saved, conditions, (available, chosenEnd), _ ->
        val now = Instant.now()
        val context = saved?.let { uniffi.takt_core.focusContextInForce(it, now.toEpochMilli()) } ?: emptyFocusContext()
        val until = context.availableUntilMs?.let(Instant::ofEpochMilli)
        // A window said on another device reads as "until" its end.
        val shown = when {
            until == chosenEnd -> available
            until == null -> AvailableTime.Unlimited
            else -> AvailableTime.Until(until.atZone(ZoneId.systemDefault()).toLocalTime().withSecond(0).withNano(0))
        }
        // The time you have runs to whichever comes first, the window or the context's expiry.
        val ends = listOfNotNull(until, context.expiresAtMs?.let(Instant::ofEpochMilli)).minOrNull()
        val said = context.conditionIds.toSet()
        val off = context.offIds.toSet()
        FocusContextState(
            selected = said.toImmutableSet(),
            off = off.toImmutableSet(),
            held = ConditionHoldings.held(conditions, said, off).toImmutableSet(),
            holdings = ConditionHoldings.holdings(conditions, said, off).toImmutableMap(),
            mode = FocusTimeMode.of(context.mode) ?: FocusTimeMode.PROGRESS,
            time = shown,
            endsAt = ends,
            conditionsSetAt = context.conditionsSetAtMs?.let(Instant::ofEpochMilli),
            isStale = uniffi.takt_core.focusContextIsStale(context, now.toEpochMilli(), ZoneId.systemDefault().id),
        )
    }.stateIn(viewModelScope, SharingStarted.Eagerly, FocusContextState())

    private val _staged = MutableStateFlow<String?>(null)
    val staged: StateFlow<String?> = _staged.asStateFlow()

    private val _stagedMinutes = MutableStateFlow(25)
    val stagedMinutes: StateFlow<Int> = _stagedMinutes.asStateFlow()

    /** The "Focus points" setting: off hides every points display. */
    val scoresEachFocusBlock: StateFlow<Boolean> = container.scoresEachFocusBlock

    private val _pending get() = container.pendingBlock
    val pendingCompletion: StateFlow<PendingCompletion?> = _pending.asStateFlow()

    private val _override = MutableStateFlow<StartOverride?>(null)
    val startOverride: StateFlow<StartOverride?> = _override.asStateFlow()

    private val _lastBlock = MutableStateFlow<LastBlock?>(null)
    val lastBlock: StateFlow<LastBlock?> = _lastBlock.asStateFlow()

    private var expiryPromptedBlockId: String? = null

    val state: StateFlow<FocusUiState> = container.withSession { session ->
        val repo = session.repository
        val active = repo.observeActiveFocusSession()
        val context = contextState.map { it.context }.distinctUntilChanged()
        val snapshot: Flow<WorkspaceNextUpSnapshot> =
            combine(context, active.map { it?.activeTaskId }.distinctUntilChanged()) { c, running -> c to running }
                .flatMapLatest { (c, running) ->
                    minuteTicks().flatMapLatest { repo.observeNextUpSnapshot(session.workspace.id, c, running) }
                }
        val queue = active.map { it?.id }.distinctUntilChanged().flatMapLatest { id ->
            if (id == null) flowOf(emptyList()) else repo.observeFocusQueue(id)
        }
        val title = active.map { it?.activeTaskId }.distinctUntilChanged().flatMapLatest { id ->
            if (id == null) flowOf(null) else repo.observeTask(id).map { it?.title }
        }
        val points = dayTicks().flatMapLatest { repo.observeFocusPointsSummary() }
        combine(snapshot, active, queue, title, points) { next, focus, queued, activeTitle, summary ->
            FocusUiState(
                // The running task is the block, not a rung to choose again.
                ladder = next.ranking.ranked.filter { it.id != focus?.activeTaskId }.toImmutableList(),
                blocked = next.ranking.blocked.toImmutableList(),
                conditions = next.conditions.toImmutableList(),
                requiredConditionIds = next.requiredConditionIds.toImmutableSet(),
                hasManualOrder = next.hasManualFocusOrder,
                session = focus,
                activeTitle = activeTitle,
                queue = queued.filter { it.item.state == FocusQueueState.QUEUED && it.task.id != focus?.activeTaskId }.toImmutableList(),
                points = summary,
                isLoaded = true,
            )
        }
    }.flowOn(Dispatchers.Default).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), FocusUiState())

    init {
        StaleFocus.resolveOnce(container)
        adoptPhoneContext()
    }

    // region Context

    /**
     * Moves the conditions and mode this phone kept in its own settings into
     * the workspace, once, if the workspace has no context yet; then forgets them.
     */
    private fun adoptPhoneContext() {
        viewModelScope.launch {
            val ids = settings.stringSet(CONDITIONS_KEY).first()
            val mode = settings.string(MODE_KEY).first()
            if (ids.isEmpty() && mode == null) return@launch
            val session = container.awaitSession()
            session.repository.adoptPhoneFocusContext(session.workspace.id, ids, mode)
            settings.edit {
                it.remove(stringSetPreferencesKey(CONDITIONS_KEY))
                it.remove(stringPreferencesKey(MODE_KEY))
            }
        }
    }

    /** Saves the context into the workspace, changed by [change]. Not an undo step. */
    private fun saveContext(change: (StoredFocusContext) -> StoredFocusContext) {
        viewModelScope.launch {
            val session = container.awaitSession()
            val repo = session.repository
            val current = repo.focusContext(session.workspace.id) ?: emptyFocusContext()
            repo.saveFocusContext(session.workspace.id, change(current))
        }
    }

    /**
     * Turns a condition on or off. One place at a time: choosing a location
     * replaces the last one, and what it provided with it. A provided one
     * turned off is off for now, until what provides it changes. The rules
     * are the core's, shared with the Mac and iPhone.
     */
    fun toggleCondition(condition: TaskCondition) {
        // A device condition holds by itself on its kind of device: nothing to say.
        if (condition.isDevice) return
        val current = contextState.value
        val next = ConditionHoldings.switching(allConditions.value, current.selected, current.off, condition.id)
        saveContext { it.copy(conditionIds = next.conditionIds, offIds = next.offIds, suggestedIds = emptyList()) }
    }

    /** Nothing holds any more: every condition off. Clears a stale answer. */
    fun clearConditions() {
        saveContext { it.copy(conditionIds = emptyList(), offIds = emptyList(), suggestedIds = emptyList()) }
    }

    fun setMode(mode: FocusTimeMode) {
        saveContext { it.copy(mode = mode.raw) }
    }

    fun setAvailable(available: AvailableTime, now: Instant = Instant.now()) {
        val ends = FocusText.endsAt(available, now)
        time.value = available to ends
        saveContext { it.copy(availableUntilMs = ends?.toEpochMilli()) }
    }

    /** Makes a condition under a category (null for none; "Location" makes it a place). */
    fun createCondition(name: String, category: String?) {
        val trimmed = name.trim()
        if (trimmed.isEmpty()) return
        container.undo.perform { repo -> repo.createCondition(container.awaitSession().workspace.id, trimmed, category) }
    }

    // endregion

    // region The ladder

    /**
     * With a block running, staging a task queues it behind the block;
     * otherwise it becomes the task about to begin, its length seeded from
     * what it knows about itself.
     */
    fun stage(rung: ScoredNextUp) = stage(rung.id, rung.candidate.title)

    fun stage(taskId: String, title: String) {
        val session = state.value.session
        if (session != null && session.phase == FocusSessionPhase.RUNNING && session.activeTaskId != null) {
            container.undo.perform(announce = false) { repo ->
                repo.addToFocusQueue(session.id, taskId)
                container.undo.say("Queued $title after the running block")
            }
            return
        }
        _staged.value = taskId
        val candidate = state.value.ladder.firstOrNull { it.id == taskId }?.candidate
            ?: state.value.blocked.firstOrNull { it.id == taskId }?.candidate
        val seconds = candidate?.let { TaskAvailabilityPolicy.suggestedSeconds(it, contextState.value.context, Instant.now()) } ?: (25 * 60)
        _stagedMinutes.value = maxOf(1, Math.round(seconds / 60.0).toInt())
    }

    fun unstage() {
        _staged.value = null
    }

    fun setStagedMinutes(minutes: Int) {
        _stagedMinutes.value = minutes.coerceIn(1, 480)
    }

    /** Begins the staged task; a start the context rules out is held for confirmation. */
    fun beginStaged(override: Boolean = false) {
        val taskId = _staged.value ?: return
        begin(taskId, _stagedMinutes.value * 60, override)
    }

    fun begin(taskId: String, requestedSeconds: Int, override: Boolean = false) {
        val context = contextState.value.context
        viewModelScope.launch {
            val repo = container.repository()
            val now = Instant.now()
            val candidate = runCatching { repo.nextUpCandidates(now) }.getOrDefault(emptyList()).firstOrNull { it.id == taskId }
            val planned = if (candidate != null && !override) {
                TaskAvailabilityPolicy.plannedSeconds(candidate, requestedSeconds, context, now)
            } else {
                requestedSeconds
            }
            val title = candidate?.title ?: repo.task(taskId)?.title ?: "This task"
            val objections = FocusText.startObjections(candidate, context, planned, now, state.value.conditions)
            if (!override && objections.isNotEmpty()) {
                _override.value = StartOverride(taskId, title, planned, objections.joinToString("\n"))
                return@launch
            }
            _override.value = null
            val started = container.undo.performNow(announce = false) { store ->
                try {
                    store.startFocusSession(taskId, plannedSeconds = planned, context = context, overrideAvailability = override, now = now)
                    container.undo.say("Focus started on $title")
                } catch (refused: TaskPlanningException) {
                    if (refused.error != TaskPlanningError.UNAVAILABLE) throw refused
                    _override.value = StartOverride(taskId, title, planned, refused.error.message)
                }
            }
            if (started && _override.value == null) {
                _staged.value = null
                _lastBlock.value = null
            }
        }
    }

    fun confirmOverride() {
        val pending = _override.value ?: return
        _override.value = null
        begin(pending.taskId, pending.plannedSeconds, override = true)
    }

    fun dismissOverride() {
        _override.value = null
    }

    fun deferTask(taskId: String, deferral: FocusDeferral) = deferTo(taskId, deferral.date(Instant.now()), deferral.title.lowercase())

    /** A repeating task deferred onto its own next turn asks first (`RecurrenceClashes`). */
    fun deferTo(taskId: String, at: Instant, label: String) = container.clashes.move(
        taskId,
        message = "Deferred to $label",
        find = { it.startClash(taskId, at) },
        move = { it.scheduleTask(taskId, at) },
        then = { if (_staged.value == taskId) _staged.value = null },
    )

    /** Moves a rung by hand: only the task that moved is pinned, so a nudge stays a nudge. */
    fun moveRung(taskId: String, offset: Int) {
        val ladder = state.value.ladder
        val index = ladder.indexOfFirst { it.id == taskId }
        val target = index + offset
        if (index < 0 || target !in ladder.indices) return
        container.undo.perform { it.pinTask(taskId, target) }
    }

    fun unpin(taskId: String) = container.undo.perform { it.unpinTask(taskId) }

    /** Gives the ladder back to the ranking. */
    fun resetOrder() = container.undo.perform { it.clearFocusOrder() }

    /** Ticks a task off without a block: a daily logs today's contribution, anything else completes. */
    fun completeWithoutSession(rung: ScoredNextUp) {
        if (_staged.value == rung.id) _staged.value = null
        if (rung.candidate.isDailyDueToday) {
            container.undo.perform { repo ->
                val daily = repo.daily(rung.id) ?: return@perform
                repo.logContribution(daily.id)
            }
        } else {
            container.commands.complete(rung.id)
        }
    }

    fun inspect(taskId: String) = container.inspector.open(taskId)

    // endregion

    // region The running block

    fun togglePause() {
        state.value.session?.let(actions::togglePause)
    }

    fun requestCompletion(completeTask: Boolean, now: Instant = Instant.now()) {
        if (_pending.value != null) return
        val current = state.value
        val session = current.session ?: return
        _pending.value = actions.requestCompletion(
            session, current.activeTitle ?: "Focus block", completeTask, now, contextState.value.context,
        ) { completion -> _lastBlock.value = LastBlock(completion.award, completion.outcome) }
    }

    fun confirmCompletion(multiplier: Double) {
        val pending = _pending.value ?: return
        _pending.value = null
        actions.confirm(pending, multiplier, contextState.value.context) { completion ->
            _lastBlock.value = LastBlock(completion.award, completion.outcome)
        }
    }

    fun cancelCompletion() {
        val pending = _pending.value ?: return
        _pending.value = null
        actions.cancel(pending)
    }

    fun endSession() {
        _pending.value = null
        state.value.session?.let { actions.endSession(it, contextState.value.context) }
    }

    fun dismissLastBlock() {
        _lastBlock.value = null
    }

    /**
     * Each second of a running block: once the planned time is up, ask how it
     * went (once per block). A timer has no planned time, so it never asks.
     */
    fun tick(now: Instant) {
        val session = state.value.session ?: return
        if (!session.isTicking || _pending.value != null || session.openEnded) return
        if (session.elapsedSeconds(now) >= session.workDurationSeconds && expiryPromptedBlockId != session.activeBlockId) {
            expiryPromptedBlockId = session.activeBlockId
            requestCompletion(completeTask = false, now = now)
        }
    }

    // endregion
}
