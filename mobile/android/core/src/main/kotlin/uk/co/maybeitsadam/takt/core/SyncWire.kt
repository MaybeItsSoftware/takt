package uk.co.maybeitsadam.takt.core

import kotlinx.serialization.Serializable

// The sync bodies themselves (docs/sync.md, "Wire protocol") are the Rust
// core's: `core/src/sync/wire.rs` makes a push body from the outbox and reads
// each pulled page, with the server's own structs. What is left here are the
// values the store hands its tests and the account the settings screen shows.

/** One SQLite value as it travels: `null`, a number or text, exactly as the column stores it. */
sealed interface SyncValue {
    data object Null : SyncValue
    data class Integer(val value: Long) : SyncValue
    data class Real(val value: Double) : SyncValue
    data class Text(val value: String) : SyncValue
}

/** A row as the server holds it, for applying rows without a page (tests). */
data class SyncIncomingRow(
    val table: String,
    val id: String,
    val deleted: Boolean,
    val values: Map<String, SyncValue> = emptyMap(),
    val hlc: String? = null,
)

/** One device signed in to the account, as `GET /v1/account` lists it. */
data class SyncDeviceInfo(
    val id: String,
    val name: String? = null,
    val platform: String? = null,
    val createdAt: String,
    val lastSeenAt: String? = null,
    /** The device asking. */
    val current: Boolean = false,
)

/** `GET /v1/account`, read by the core (`sync_decode_account`); [subscription] is null from a server that predates Takt Sync. */
data class SyncAccountInfo(
    val accountId: String,
    val email: String? = null,
    val devices: List<SyncDeviceInfo> = emptyList(),
    val subscription: SyncSubscription? = null,
)

/**
 * Takt Sync, the hosted server's £1-a-year subscription, as `GET /v1/account`
 * reports it (docs/sync.md, "Subscription"). A self-hosted server reports
 * [required] false.
 */
@Serializable
data class SyncSubscription(
    /** Whether this server charges at all. */
    val required: Boolean = false,
    /** Whether this account may sync here now. */
    val active: Boolean = true,
    val expiresAt: String? = null,
    /** Granted rather than bought. */
    val comp: Boolean = false,
) {
    companion object {
        /** What a lapsed account is told, wherever sync stops for it. */
        const val NEEDED = "Hosted sync needs Takt Sync (£1 a year). Subscribe here, or self-host the server."
    }
}

/**
 * Which server this device syncs with, as which device, and for whom. The
 * Supabase session that proves who is kept apart from this, and the device
 * id is the device's own, made once and kept across sign-ins.
 */
@Serializable
data class SyncCredentials(
    val serverURL: String,
    val deviceId: String,
    val email: String? = null,
    val accountId: String? = null,
)
