package uk.co.maybeitsadam.takt.data.sync

import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonObject
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import uniffi.takt_core.syncBackoffSeconds

/**
 * Waits for other devices' news on the account's Supabase Realtime channel
 * (`docs/sync.md`, "News on Supabase Realtime"; port of
 * `Sources/TaktSync/SyncRealtime.swift`), so [SyncScheduler] need not hold
 * a long-poll open on the sync server, which Cloud Run bills for every
 * second it is open. The server broadcasts `changed` on `sync:<account>`
 * after every push that writes; this only passes that on.
 *
 * Realtime speaks the Phoenix channel protocol: JSON frames of `{topic,
 * event, payload, ref, join_ref}`, a `phx_join` answered by a `phx_reply`,
 * and a heartbeat on the `phoenix` topic or the socket is dropped.
 */
class SyncRealtime(
    private val serverURL: String,
    supabaseURL: String,
    supabaseKey: String,
    accountId: String,
    private val tokens: SyncAccessTokens,
    private val client: OkHttpClient = socketClient,
) {
    sealed interface Signal {
        /** The channel was joined (true) or lost (false). */
        data class Joined(val joined: Boolean) : Signal

        /** Another device, or this one, pushed: time to pull. */
        data object News : Signal
    }

    private val socketURL = socketURL(supabaseURL, supabaseKey)
    private val channel = channel(accountId)

    /**
     * Listens until cancelled, telling [signal] what happens. While the
     * server doesn't broadcast, or the channel can't be joined, the
     * scheduler long-polls; this tries again later. Returns when the session
     * is refused for good.
     */
    suspend fun run(signal: suspend (Signal) -> Unit) {
        var failures = 0u
        while (currentCoroutineContext().isActive) {
            if (!serverBroadcasts()) {
                delay(600.seconds)
                continue
            }
            val ended = listen(signal)
            signal(Signal.Joined(false))
            when (ended) {
                Ended.Unauthorized -> return
                is Ended.Failed -> {
                    failures = if (ended.joined) 1u else failures + 1u
                    delay(maxOf(2u, syncBackoffSeconds(failures)).toLong().seconds)
                }
            }
        }
    }

    private suspend fun serverBroadcasts(): Boolean = withContext(Dispatchers.IO) {
        runCatching {
            val url = serverURL.trim().toHttpUrl().newBuilder().addPathSegment("health").build()
            client.newCall(Request.Builder().url(url).build()).execute().use { response ->
                response.isSuccessful &&
                    Json.parseToJsonElement(response.body.string()).jsonObject["realtime"]
                        ?.jsonPrimitive?.booleanOrNull == true
            }
        }.getOrDefault(false)
    }

    private sealed interface Ended {
        data object Unauthorized : Ended

        data class Failed(val joined: Boolean) : Ended
    }

    /** One connection: join, then read until it breaks. */
    private suspend fun listen(signal: suspend (Signal) -> Unit): Ended = coroutineScope {
        val token = try {
            tokens.current()
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (refused: SyncException.Unauthorized) {
            return@coroutineScope Ended.Unauthorized
        } catch (other: Exception) {
            return@coroutineScope Ended.Failed(joined = false)
        }
        // Frames as they come; null when the socket ends.
        val incoming = Channel<String?>(Channel.UNLIMITED)
        val socket = client.newWebSocket(
            Request.Builder().url(socketURL).build(),
            object : WebSocketListener() {
                override fun onMessage(webSocket: WebSocket, text: String) {
                    incoming.trySend(text)
                }

                override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                    incoming.trySend(null)
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    incoming.trySend(null)
                }
            },
        )
        val joined = AtomicBoolean(false)
        try {
            socket.send(join(channel, token, JOIN_REF))
            val keepAlive = launch {
                var sent = token
                var reference = 1
                while (isActive) {
                    delay(25.seconds)
                    socket.send(heartbeat((++reference).toString()))
                    val current = runCatching { tokens.current() }.getOrNull()
                    if (current != null && current != sent) {
                        sent = current
                        socket.send(accessToken(channel, current, (++reference).toString(), JOIN_REF))
                    }
                }
            }
            val joinTimeout = launch {
                delay(15.seconds)
                if (!joined.get()) incoming.trySend(null)
            }
            try {
                for (text in incoming) {
                    if (text == null) break
                    when (val frame = classify(text, channel, JOIN_REF)) {
                        Incoming.Joined -> {
                            joined.set(true)
                            signal(Signal.Joined(true))
                        }
                        Incoming.News -> signal(Signal.News)
                        is Incoming.Ended -> break
                        Incoming.Other -> Unit
                    }
                }
            } finally {
                keepAlive.cancel()
                joinTimeout.cancel()
            }
            Ended.Failed(joined.get())
        } finally {
            socket.cancel()
        }
    }

    sealed interface Incoming {
        data object Joined : Incoming

        data class Ended(val reason: String) : Incoming

        data object News : Incoming

        data object Other : Incoming
    }

    companion object {
        private const val JOIN_REF = "1"

        /** No read timeout: the socket is quiet between heartbeats by design. */
        val socketClient: OkHttpClient = OkHttpSyncTransport.defaultClient.newBuilder()
            .readTimeout(0, TimeUnit.SECONDS)
            .build()

        fun socketURL(supabaseURL: String, key: String): String {
            val base = supabaseURL.trim().trimEnd('/')
                .replaceFirst("https://", "wss://")
                .replaceFirst("http://", "ws://")
            val escaped = java.net.URLEncoder.encode(key, "UTF-8")
            return "$base/realtime/v1/websocket?apikey=$escaped&vsn=1.0.0"
        }

        fun channel(accountId: String): String = "realtime:sync:$accountId"

        fun join(channel: String, token: String, ref: String): String = buildJsonObject {
            put("topic", channel)
            put("event", "phx_join")
            putJsonObject("payload") {
                putJsonObject("config") {
                    putJsonObject("broadcast") {
                        put("self", false)
                        put("ack", false)
                    }
                    putJsonObject("presence") { put("key", "") }
                    put("postgres_changes", JsonArray(emptyList()))
                    put("private", true)
                }
                put("access_token", token)
            }
            put("ref", ref)
            put("join_ref", ref)
        }.toString()

        fun heartbeat(ref: String): String = buildJsonObject {
            put("topic", "phoenix")
            put("event", "heartbeat")
            put("payload", JsonObject(emptyMap()))
            put("ref", ref)
        }.toString()

        fun accessToken(channel: String, token: String, ref: String, joinRef: String): String = buildJsonObject {
            put("topic", channel)
            put("event", "access_token")
            putJsonObject("payload") { put("access_token", token) }
            put("ref", ref)
            put("join_ref", joinRef)
        }.toString()

        fun classify(text: String, channel: String, joinRef: String): Incoming {
            val frame = runCatching { Json.parseToJsonElement(text).jsonObject }.getOrNull() ?: return Incoming.Other
            if (frame.string("topic") != channel) return Incoming.Other
            val payload = frame["payload"] as? JsonObject ?: JsonObject(emptyMap())
            val reason = (payload["response"] as? JsonObject)?.string("reason")
                ?: payload.string("message") ?: "refused"
            return when (frame.string("event")) {
                "phx_reply" ->
                    when {
                        frame.string("ref") != joinRef -> Incoming.Other
                        payload.string("status") == "ok" -> Incoming.Joined
                        else -> Incoming.Ended(reason)
                    }
                "broadcast" -> Incoming.News
                "phx_close" -> Incoming.Ended("the channel closed")
                "phx_error" -> Incoming.Ended("the channel failed")
                "system" -> if (payload.string("status") == "error") Incoming.Ended(reason) else Incoming.Other
                else -> Incoming.Other
            }
        }

        private fun JsonObject.string(key: String): String? = (this[key] as? JsonPrimitive)?.contentOrNull
    }
}
