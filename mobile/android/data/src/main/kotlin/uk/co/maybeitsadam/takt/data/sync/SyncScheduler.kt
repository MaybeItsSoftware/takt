package uk.co.maybeitsadam.takt.data.sync

import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.runningFold
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.takt_core.syncBackoffSeconds

/**
 * When to sync (port of `Sources/TaktSync/SyncScheduler.swift`): on
 * demand, [debounce] after the last local write, and while the app is in
 * front, whenever another device pushes, so its edit appears within a second
 * or two.
 *
 * With [realtime], news comes on the account's Supabase Realtime channel:
 * a quick cycle on each piece, and one every five minutes in case a message
 * was lost, with no request held open on the server, which is billed for
 * every second one is. Without it, or while the channel is down, the loop
 * holds a long-poll instead.
 *
 * A ViewModel or Application calls [start] on foreground and [stop] on
 * background; [watchLocalWrites] turns outbox growth into [noteLocalChange].
 */
class SyncScheduler(
    private val engine: SyncEngine,
    private val scope: CoroutineScope,
    private val debounce: Duration = 2.seconds,
    private val realtime: SyncRealtime? = null,
) {
    private var loop: Job? = null
    private var listener: Job? = null

    @Volatile
    private var realtimeUp = false

    @Volatile
    private var cycling = false

    @Volatile
    private var newsPending = false
    private var pending: Job? = null
    private var watcher: Job? = null
    private var failures = 0

    /** Set by a 401: the token is gone, so nothing here runs again until a new scheduler is made after signing in. */
    @Volatile
    var isSignedOut = false
        private set

    private val _lastOutcome = MutableStateFlow<SyncEngine.Outcome?>(null)

    /** The most recent successful cycle's outcome; the engine's [SyncEngine.status] carries the rest. */
    val lastOutcome: StateFlow<SyncEngine.Outcome?> = _lastOutcome.asStateFlow()

    /** Syncs now and keeps listening for news (or a long-poll open) until [stop]. */
    @Synchronized
    fun start() {
        if (loop != null || isSignedOut) return
        loop = launchLoop()
        if (realtime != null && listener == null) {
            listener = scope.launch {
                realtime.run { signal ->
                    when (signal) {
                        is SyncRealtime.Signal.Joined -> realtimeJoined(signal.joined)
                        SyncRealtime.Signal.News -> newsArrived()
                    }
                }
            }
        }
    }

    private fun launchLoop(): Job = scope.launch {
        while (isActive) {
            val joined = realtimeUp
            val backoff = runOnce(wait = if (joined) 0 else 25)
            if (newsPending && backoff == null) {
                newsPending = false
                continue
            }
            val gap = backoff ?: if (joined) REALTIME_SAFETY_INTERVAL else null
            if (gap != null) delay(gap)
        }
    }

    /** Runs a cycle now in a fresh loop, ending any long-poll. Keeps the listener. */
    @Synchronized
    private fun restartLoop() {
        if (loop == null) return
        loop?.cancel()
        loop = launchLoop()
    }

    /** The channel was joined (a cycle now, for anything missed, and no more long-poll) or lost. */
    @Synchronized
    fun realtimeJoined(joined: Boolean) {
        if (joined == realtimeUp) return
        realtimeUp = joined
        restartLoop()
    }

    /** The channel says another device pushed. */
    @Synchronized
    fun newsArrived() {
        if (loop == null) return
        if (cycling) newsPending = true else restartLoop()
    }

    /** Stops the long-poll and the channel. */
    @Synchronized
    fun stop() {
        loop?.cancel()
        loop = null
        listener?.cancel()
        listener = null
        realtimeUp = false
    }

    /** A local write happened: sync once things go quiet. */
    @Synchronized
    fun noteLocalChange() {
        if (isSignedOut) return
        pending?.cancel()
        pending = scope.launch {
            delay(debounce)
            flush()
        }
    }

    /** Calls [noteLocalChange] whenever the outbox grows, i.e. after every recorded local write. */
    @Synchronized
    fun watchLocalWrites(store: SyncStore) {
        if (watcher != null) return
        watcher = scope.launch {
            store.observeOutboxSize()
                .runningFold(0L to 0L) { (_, previous), size -> previous to size }
                .drop(2)
                .filter { (previous, size) -> size > previous }
                .collect { noteLocalChange() }
        }
    }

    /** One cycle, for background work or a "Sync now" button. True when it succeeded. */
    suspend fun syncNow(): Boolean = runOnce(wait = 0) == null

    /**
     * Pushes promptly. A cycle holding a long-poll would not push until the
     * poll returned, so the loop is restarted: the new cycle pushes first.
     */
    private suspend fun flush() {
        val restarted = synchronized(this) {
            if (loop != null) {
                restartLoop()
                true
            } else {
                false
            }
        }
        if (!restarted) runOnce(wait = 0)
    }

    /** Runs a cycle; returns how long to back off before the next, or null to go straight on. */
    private suspend fun runOnce(wait: Int): Duration? = if (isSignedOut) SIGNED_OUT_PAUSE else try {
        cycling = true
        _lastOutcome.value = engine.sync(wait)
        failures = 0
        null
    } catch (cancelled: CancellationException) {
        throw cancelled
    } catch (error: Exception) {
        failures += 1
        if (error is SyncException.Unauthorized) {
            isSignedOut = true
            pending?.cancel()
        }
        if (error is SyncException.Unauthorized || error is SyncException.NotPaired) stop()
        // 2, 4, 8 … seconds, capped at five minutes, so an outage costs nothing.
        syncBackoffSeconds(failures.toUInt()).toInt().seconds
    } finally {
        cycling = false
    }

    private companion object {
        val SIGNED_OUT_PAUSE = 300.seconds

        /** Between cycles while the channel is up and says nothing. */
        val REALTIME_SAFETY_INTERVAL = 300.seconds
    }
}
