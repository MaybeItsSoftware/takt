package uk.co.maybeitsadam.takt.data.sync

import java.time.Clock
import java.time.Instant
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.takt_core.SyncPull

/**
 * The client half of docs/sync.md (port of `Sources/TaktSync/SyncEngine.swift`).
 *
 * One cycle at a time: a second [sync] call waits for the first to finish
 * rather than racing it. The scheduling (foreground, 2 s after a write, a
 * long-poll loop while in front, background refresh) belongs to the caller.
 */
class SyncEngine(
    private val store: SyncStore,
    private val transport: SyncTransport,
    /** This device. The clock the core stamps with is the stored one, made on the id the store was paired with. */
    @Suppress("unused") private val deviceId: String,
    private val wallClock: Clock = Clock.systemUTC(),
) {
    data class Outcome(val pushed: Int, val pulled: Int, val changedWorkspace: Boolean)

    sealed interface Status {
        data object Idle : Status
        data object Syncing : Status
        data class Synced(val at: Instant) : Status
        data class Failed(val message: String) : Status

        /** The server no longer knows this device's token. Retrying cannot help; the user has to sign in again. */
        data object SignedOut : Status

        /** The hosted server wants Takt Sync. Not an error: sync waits. */
        data object NeedsSubscription : Status
    }

    private val cycle = Mutex()
    private val _status = MutableStateFlow<Status>(Status.Idle)
    val status: StateFlow<Status> = _status.asStateFlow()

    /**
     * One push-then-pull cycle. The first cycle after pairing snapshots the
     * workspace and pulls first, so a device joining an account adopts it.
     * [wait] long-polls the first pull page for up to that many seconds.
     */
    suspend fun sync(wait: Int = 0): Outcome = cycle.withLock {
        val state = store.syncState() ?: throw SyncException.NotPaired()
        _status.value = Status.Syncing
        try {
            var outcome = Outcome(0, 0, false)
            if (state.needsSnapshot) {
                store.enqueueSnapshot(wallClock.instant())
                outcome = outcome.plus(pull(state.cursor, 0))
                outcome = outcome.plus(push())
            } else {
                outcome = outcome.plus(push())
                val cursor = store.syncState()?.cursor ?: state.cursor
                outcome = outcome.plus(pull(cursor, wait))
            }
            _status.value = Status.Synced(wallClock.instant())
            outcome
        } catch (error: SyncException.Unauthorized) {
            _status.value = Status.SignedOut
            throw error
        } catch (error: SyncException.SubscriptionRequired) {
            _status.value = Status.NeedsSubscription
            throw error
        } catch (error: Throwable) {
            _status.value = Status.Failed(error.message ?: error.toString())
            throw error
        }
    }

    private fun Outcome.plus(other: Outcome) =
        Outcome(pushed + other.pushed, pulled + other.pulled, changedWorkspace || other.changedWorkspace)

    /**
     * Sends the outbox in batches. The core makes each body, stamped from the
     * stored clock in the order the edits were made, and keeps the clock when
     * the batch is acknowledged.
     */
    private suspend fun push(): Outcome {
        var pushed = 0
        while (true) {
            val batch = store.preparePush(PUSH_BATCH, wallClock.instant()) ?: break
            transport.push(batch.body)
            store.finishPush(batch, wallClock.instant())
            pushed += batch.count.toInt()
        }
        return Outcome(pushed, 0, false)
    }

    /**
     * Gathers every page in the core, then applies them in one transaction: a
     * task can arrive a page before its list, and only the end of the whole
     * pull is a consistent state.
     */
    private suspend fun pull(cursor: Long, wait: Int): Outcome {
        val pull = SyncPull()
        try {
            var next = cursor
            var firstPage = true
            while (true) {
                val body = transport.changes(next, PULL_PAGE, if (firstPage) wait else 0)
                firstPage = false
                val page = pull.addPage(body)
                next = page.cursor
                if (!page.hasMore) break
            }
            val applied = store.applyPull(pull, wallClock.instant())
            return Outcome(0, applied.pulled.toInt(), applied.changed)
        } finally {
            pull.close()
        }
    }

    companion object {
        /**
         * Small enough that one push finishes inside the read timeout even when
         * the server's database is far away: the server merges a push row by
         * row, a couple of round trips to Postgres each, and a batch it cannot
         * finish in time is rolled back whole and sent again for ever. Each
         * accepted batch empties its part of the outbox, so a first sync of a
         * big workspace makes progress batch by batch instead.
         */
        const val PUSH_BATCH = 100
        const val PULL_PAGE = 1000
    }
}
