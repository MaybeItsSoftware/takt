package uk.co.maybeitsadam.takt.data.sync

import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import okhttp3.Call
import okhttp3.Callback
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import uk.co.maybeitsadam.takt.core.SyncAccountInfo
import uk.co.maybeitsadam.takt.core.SyncDeviceInfo
import uniffi.takt_core.syncDecodeAccount
import uniffi.takt_core.syncFailureMessage
import uniffi.takt_core.syncRegisterDeviceBody
import uniffi.takt_core.syncSentence
import uniffi.takt_core.syncServerMessage

/**
 * The two calls a sync cycle makes, moving bodies the Rust core makes and
 * reads (`core/src/sync/wire.rs`): a push body out, and each page of the feed
 * back as the server sent it. The engine knows nothing else about the server.
 */
interface SyncTransport {
    /** Sends a `POST /v1/push` body. */
    suspend fun push(body: String)

    /** One `GET /v1/changes` page's body. */
    suspend fun changes(since: Long, limit: Int, wait: Int): String
}

sealed class SyncException(message: String) : Exception(message) {
    /**
     * The server said no. The message is the server's own `{"error": "..."}`
     * text when it sent one, which is written to be shown to a person.
     */
    class Server(val status: Int, val body: String) : SyncException(describe(status, body))

    /**
     * The account's session is over: Supabase refused to refresh it (signed
     * out elsewhere, or the account deleted), or the server refused even a
     * freshly refreshed token. Retrying cannot help; the user signs in again.
     */
    class Unauthorized : SyncException("Signed out. Sign in again.")

    class NotPaired : SyncException("This device is not signed in to a sync server.")

    /** `402`: the hosted server wants Takt Sync, and this account hasn't it. Not an error to retry hot. */
    class SubscriptionRequired : SyncException(uk.co.maybeitsadam.takt.core.SyncSubscription.NEEDED)

    companion object {
        /** The `error` field of a server error body, or null when there isn't one. The Rust core's. */
        fun serverMessage(body: String): String? = syncServerMessage(body).trim().takeIf { it.isNotEmpty() }

        /** `deleting accounts isn't set up` reads as `Deleting accounts isn't set up.` */
        internal fun describe(status: Int, body: String): String = syncFailureMessage(status, body)

        /** Capitalised, and ending in a full stop unless it already ends a sentence. The Rust core's. */
        fun sentence(message: String): String = syncSentence(message)
    }
}

/**
 * HTTPS transport (docs/sync.md "Wire protocol"). Every request carries
 * `Authorization: Bearer <Supabase access token>` and `X-Priority-Device`.
 * The token comes from [tokens] afresh for each request; a 401 refreshes it
 * once and retries. Only a refresh Supabase refuses, or a 401 to the
 * refreshed token, is [SyncException.Unauthorized], which reads as signed out.
 */
class OkHttpSyncTransport(
    val serverURL: String,
    val deviceId: String,
    private val tokens: SyncAccessTokens,
    private val client: OkHttpClient = defaultClient,
) : SyncTransport {

    override suspend fun push(body: String) {
        send(endpoint("v1/push")) { post(body.toRequestBody(JSON)) }
    }

    override suspend fun changes(since: Long, limit: Int, wait: Int): String {
        val url = endpoint("v1/changes").newBuilder()
            .addQueryParameter("since", since.toString())
            .addQueryParameter("limit", limit.toString())
            .addQueryParameter("wait", wait.coerceIn(0, 25).toString())
            .build()
        return send(url) { get() }
    }

    /** Records this device on the account (`POST /v1/devices`). Sent after every sign-in. */
    suspend fun registerDevice(name: String, platform: String = PLATFORM) {
        val body = syncRegisterDeviceBody(deviceId, name, platform)
        send(endpoint("v1/devices")) { post(body.toRequestBody(JSON)) }
    }

    /** Who this device is signed in as, and every device on the account (`GET /v1/account`). */
    suspend fun account(): SyncAccountInfo {
        val body = send(endpoint("v1/account")) { get() }
        val account = syncDecodeAccount(body)
        val subscription = runCatching {
            subscriptionJson.decodeFromString<AccountSubscription>(body).subscription
        }.getOrNull()
        return SyncAccountInfo(
            subscription = subscription,
            accountId = account.accountId,
            email = account.email,
            devices = account.devices.map {
                SyncDeviceInfo(it.id, it.name, it.platform, it.createdAt, it.lastSeenAt, it.current)
            },
        )
    }

    /** Takes this device off the account's list (`POST /v1/sign-out`). The device keeps its workspace. */
    suspend fun signOut() {
        send(endpoint("v1/sign-out")) { post(EMPTY_OBJECT.toRequestBody(JSON)) }
    }

    /** Deletes the account's rows, its devices and the Supabase user (`POST /v1/account/delete`). */
    suspend fun deleteAccount() {
        send(endpoint("v1/account/delete")) { post(EMPTY_OBJECT.toRequestBody(JSON)) }
    }

    /**
     * Any other request to the server, signed in: the shared-list calls
     * (`docs/sync.md`, "Shared lists"), whose paths and bodies the core makes.
     */
    suspend fun request(method: String, path: String, body: String? = null): String =
        send(endpoint(path)) { method(method, body?.toRequestBody(JSON) ?: if (method == "POST") EMPTY_OBJECT.toRequestBody(JSON) else null) }

    private fun endpoint(path: String): HttpUrl = serverURL.trim().toHttpUrl().newBuilder().addPathSegments(path).build()

    /** The answer's body, after one refresh and retry on a 401. */
    private suspend inline fun send(url: HttpUrl, crossinline method: Request.Builder.() -> Request.Builder): String {
        val request = { token: String ->
            Request.Builder().url(url)
                .header("Authorization", "Bearer $token")
                .header(DEVICE_HEADER, deviceId)
                .method()
                .build()
        }
        val token = tokens.current()
        val first = execute(request(token))
        val answer = if (first.status == 401) execute(request(tokens.refresh(rejected = token))) else first
        return when {
            answer.status == 401 -> throw SyncException.Unauthorized()
            answer.status == 402 -> throw SyncException.SubscriptionRequired()
            answer.status !in 200..299 -> throw SyncException.Server(answer.status, answer.body)
            else -> answer.body
        }
    }

    private class Answer(val status: Int, val body: String)

    private suspend fun execute(request: Request): Answer = withContext(Dispatchers.IO) {
        client.newCall(request).await().use { Answer(it.code, it.body.string()) }
    }

    companion object {
        const val DEVICE_HEADER = "X-Priority-Device"
        const val PLATFORM = "android"
        private val JSON = "application/json".toMediaType()
        private const val EMPTY_OBJECT = "{}"

        /** Long polls wait up to 25 s on the server, so reads get headroom past that. */
        val defaultClient: OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(15, TimeUnit.SECONDS)
            .readTimeout(40, TimeUnit.SECONDS)
            .build()

        private suspend fun Call.await(): Response = suspendCancellableCoroutine { continuation ->
            continuation.invokeOnCancellation { cancel() }
            enqueue(object : Callback {
                override fun onResponse(call: Call, response: Response) {
                    continuation.resumeWith(Result.success(response))
                }

                override fun onFailure(call: Call, e: IOException) {
                    continuation.resumeWith(Result.failure(e))
                }
            })
        }
    }
}

/** Just the subscription out of `GET /v1/account`, which the core's account record predates. */
@kotlinx.serialization.Serializable
private class AccountSubscription(val subscription: uk.co.maybeitsadam.takt.core.SyncSubscription? = null)

private val subscriptionJson = kotlinx.serialization.json.Json { ignoreUnknownKeys = true }
