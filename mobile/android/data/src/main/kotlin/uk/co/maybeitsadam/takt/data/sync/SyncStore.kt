package uk.co.maybeitsadam.takt.data.sync

import java.time.Instant
import kotlinx.coroutines.flow.Flow
import uk.co.maybeitsadam.takt.core.SyncIncomingRow
import uk.co.maybeitsadam.takt.core.SyncValue
import uk.co.maybeitsadam.takt.data.db.Db
import uk.co.maybeitsadam.takt.data.db.WorkspaceDatabase
import uk.co.maybeitsadam.takt.data.db.WorkspaceSchema
import uniffi.takt_core.SyncPull
import uniffi.takt_core.SyncPullOutcome
import uniffi.takt_core.SyncPushBatch

/** What the device remembers about its sync. */
data class SyncLocalState(
    val deviceId: String,
    val cursor: Long,
    val hlc: String?,
    val serverURL: String?,
    val canonicalWorkspaceId: String?,
    val needsSnapshot: Boolean,
    val lastSyncedAt: Instant?,
    val isRecording: Boolean,
)

/** A row's local change, coalesced from its outbox entries and read from the live row. */
data class SyncOutgoingChange(
    val table: String,
    val rowId: String,
    val operation: Operation,
    /** The columns to push and their current values. Empty for a delete. */
    val values: Map<String, SyncValue>,
    /** When the newest of the coalesced edits was made, for stamping the clock. */
    val changedAtMs: Long,
) {
    enum class Operation(val wire: String) { UPSERT("upsert"), DELETE("delete") }
}

/** A batch of [SyncOutgoingChange]s and the newest outbox entry folded into it. */
data class PendingSyncChanges(val changes: List<SyncOutgoingChange>, val throughSeq: Long?)

/**
 * The store's half of sync (port of `WorkspaceStore+Sync.swift`): the outbox
 * the triggers fill, and the reads and writes [SyncEngine] drives.
 */
class SyncStore(private val database: WorkspaceDatabase) {

    /** The device's sync state, or nil while it has never been paired. */
    suspend fun syncState(): SyncLocalState? = database.read { readState(it) }

    fun observeSyncState(): Flow<SyncLocalState?> =
        database.observe(setOf("sync_state", "sync_control")) { readState(it) }

    /** How many outbox entries wait to be pushed; a write's debounce watches this. */
    fun observeOutboxSize(): Flow<Long> =
        database.observe(setOf("sync_outbox")) { it.long("SELECT COUNT(*) FROM sync_outbox") ?: 0L }

    private fun readState(db: Db): SyncLocalState? {
        val recording = (db.long("SELECT recording FROM sync_control WHERE id = 0") ?: 0L) != 0L
        return db.queryOne("SELECT * FROM sync_state WHERE id = 0") {
            SyncLocalState(
                deviceId = it.string("deviceId"),
                cursor = it.long("cursor"),
                hlc = it.stringOrNull("hlc"),
                serverURL = it.stringOrNull("serverURL"),
                canonicalWorkspaceId = it.stringOrNull("canonicalWorkspaceId"),
                needsSnapshot = it.long("needsSnapshot") != 0L,
                lastSyncedAt = it.instantOrNull("lastSyncedAt"),
                isRecording = recording,
            )
        }
    }

    /** Pairs the store with a server. The next cycle snapshots and pulls before it pushes. */
    // The writes and the outbox read are the Rust core's (core/src/sync.rs),
    // shared with the Mac. They run on the core's own connection, so each one
    // announces the tables it can change for the observers above.

    suspend fun beginSync(deviceId: String, serverURL: String) {
        database.coreWrite(SYNC_TABLES) { it.beginSync(deviceId, serverURL) }
    }

    suspend fun endSync() {
        database.coreWrite(SYNC_TABLES) { it.endSync() }
    }

    suspend fun enqueueSnapshot(now: Instant) {
        database.coreWrite(SYNC_TABLES) { it.enqueueSyncSnapshot(now.toEpochMilli()) }
    }

    /** The outbox, coalesced per row and read from the live rows, up to [limit] rows. */
    suspend fun pendingChanges(limit: Int = 500): PendingSyncChanges {
        val pending = database.coreRead { it.pendingSyncChanges(limit.coerceAtLeast(0).toUInt()) }
        return PendingSyncChanges(
            pending.changes.map { change ->
                SyncOutgoingChange(
                    table = change.table,
                    rowId = change.rowId,
                    operation = if (change.operation == "delete") SyncOutgoingChange.Operation.DELETE else SyncOutgoingChange.Operation.UPSERT,
                    values = change.values.mapValues { (_, value) -> value.fromCore() },
                    changedAtMs = change.changedAtMs,
                )
            },
            pending.throughSeq,
        )
    }

    suspend fun acknowledge(throughSeq: Long) {
        database.coreWrite(SYNC_TABLES) { it.acknowledgeSyncChanges(throughSeq) }
    }

    /** Writes a pull into the workspace; whether anything changed. */
    suspend fun applyRemoteRows(rows: List<SyncIncomingRow>, cursor: Long, hlc: String?, now: Instant): Boolean {
        val incoming = rows.map { row ->
            uniffi.takt_core.IncomingRow(row.table, row.id, row.deleted, row.values.mapValues { (_, v) -> v.toCore() }, row.hlc)
        }
        return database.coreWrite(SYNC_TABLES) { it.applyRemoteRows(incoming, cursor, hlc, now.toEpochMilli()) }
    }

    suspend fun recordProgress(cursor: Long? = null, hlc: String?, now: Instant) {
        database.coreWrite(SYNC_TABLES) { it.recordSyncProgress(cursor, hlc, now.toEpochMilli()) }
    }

    // The wire (core/src/sync/wire.rs): the core makes the push body from the
    // outbox and reads the pulled pages, so no row crosses as a record.

    /** The outbox's next batch as a `POST /v1/push` body, stamped from the stored clock; null when nothing waits. */
    suspend fun preparePush(limit: Int, now: Instant): SyncPushBatch? =
        database.coreRead { it.prepareSyncPush(limit.coerceAtLeast(0).toUInt(), now.toEpochMilli()) }

    /** The server has [batch]: its outbox entries go and its clock is kept, in one transaction. */
    suspend fun finishPush(batch: SyncPushBatch, now: Instant) {
        database.coreWrite(SYNC_TABLES) { it.finishSyncPush(batch.throughSeq, batch.hlc, now.toEpochMilli()) }
    }

    /** Writes the pages gathered in [pull] in one transaction, the clock moved past every stamp in them. */
    suspend fun applyPull(pull: SyncPull, now: Instant): SyncPullOutcome =
        database.coreWrite(SYNC_TABLES) { it.applySyncPull(pull, now.toEpochMilli(), now.toEpochMilli()) }

    private companion object {
        /** Every table a sync write can change: the synced ones and sync's own. */
        val SYNC_TABLES: Set<String> = WorkspaceSchema.syncedTables.map { it.first }.toSet() +
            setOf("sync_state", "sync_control", "sync_outbox", "change_log", "sync_spaces")
    }
}

private fun SyncValue.toCore(): uniffi.takt_core.SyncValue = when (this) {
    SyncValue.Null -> uniffi.takt_core.SyncValue.Null
    is SyncValue.Integer -> uniffi.takt_core.SyncValue.Integer(value)
    is SyncValue.Real -> uniffi.takt_core.SyncValue.Real(value)
    is SyncValue.Text -> uniffi.takt_core.SyncValue.Text(value)
}

private fun uniffi.takt_core.SyncValue.fromCore(): SyncValue = when (this) {
    is uniffi.takt_core.SyncValue.Null -> SyncValue.Null
    is uniffi.takt_core.SyncValue.Integer -> SyncValue.Integer(value)
    is uniffi.takt_core.SyncValue.Real -> SyncValue.Real(value)
    is uniffi.takt_core.SyncValue.Text -> SyncValue.Text(value)
}
