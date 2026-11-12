package uk.co.maybeitsadam.takt.core

import kotlinx.serialization.Serializable
import uniffi.takt_core.SyncEndpointsRecord
import uniffi.takt_core.syncCurrentServer
import uniffi.takt_core.syncNormalisedUrl
import uniffi.takt_core.syncResolveEndpoints

/**
 * Where a device syncs: the sync server, and the Supabase project whose
 * accounts that server trusts. Takt's own unless "Use a different server"
 * names a self-hosted pair (docs/self-hosting.md). Typed endpoints are read
 * by the Rust core (`core/src/sync/endpoints.rs`), as on the Mac and iPhone.
 *
 * The three travel together because they only work together: the server
 * checks every access token against one Supabase project, so a session from
 * another project gets a token it refuses.
 */
@Serializable
data class SyncEndpoints(
    val serverURL: String,
    val supabaseURL: String,
    /** The project's publishable key (`sb_publishable_…`) or legacy anon key, made to ship in apps. */
    val supabaseKey: String,
) {
    /** Accounts are [hosted]'s project, whatever server holds the rows. */
    fun usesAccountsOf(hosted: SyncEndpoints): Boolean = supabaseURL == hosted.supabaseURL && supabaseKey == hosted.supabaseKey

    companion object {
        /**
         * The endpoints as typed under "Use a different server". A blank
         * server is [hosted]'s; a blank Supabase URL *and* key are [hosted]'s
         * project. Throws [InvalidSyncEndpoints], with something to show, for
         * anything else that can't be used.
         */
        fun resolve(server: String, supabaseURL: String, supabaseKey: String, hosted: SyncEndpoints): SyncEndpoints {
            val resolution = syncResolveEndpoints(
                server,
                supabaseURL,
                supabaseKey,
                SyncEndpointsRecord(hosted.serverURL, hosted.supabaseURL, hosted.supabaseKey),
            )
            val resolved = resolution.endpoints
                ?: throw InvalidSyncEndpoints(resolution.problem ?: "Those addresses can't be used.")
            return SyncEndpoints(resolved.serverUrl, resolved.supabaseUrl, resolved.supabaseKey)
        }

        /**
         * [server], or [hosted] when [server] is one of the hosted server's
         * old addresses on Railway (which answer 404 to everything now), so a
         * device that signed in there follows it to its new one.
         */
        fun currentServer(server: String, hosted: String): String = syncCurrentServer(server, hosted)

        /**
         * [typed] as an http(s) address: `https://` added when it has no
         * scheme, and without a query, fragment, trailing slash or any of
         * [droppingSuffixes] (a Supabase URL copied from an API example often
         * ends `/rest/v1`). Null when it still isn't one with a host.
         */
        fun httpURL(typed: String, droppingSuffixes: List<String> = emptyList()): String? =
            syncNormalisedUrl(typed, droppingSuffixes)
    }
}

/** Typed endpoints that can't be used, with the reason in words to show. */
class InvalidSyncEndpoints(message: String) : IllegalArgumentException(message)
