package uk.co.maybeitsadam.takt.data.sync

import java.nio.file.Files
import java.util.Collections
import kotlin.time.Duration.Companion.milliseconds
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test
import uk.co.maybeitsadam.takt.data.db.WorkspaceDatabase
import uk.co.maybeitsadam.takt.data.workspace.WorkspaceRepository

/**
 * The Realtime channel's frames, and the scheduler giving up its long-poll
 * while the channel is joined (port of `sync-tests/SyncRealtimeTests.swift`).
 */
class SyncRealtimeTest {
    private val channel = "realtime:sync:6a1f"

    @Test
    fun theSocketAndChannelAreNamedAsSupabaseWants() {
        assertEquals(
            "wss://abc.supabase.co/realtime/v1/websocket?apikey=sb_publishable_x&vsn=1.0.0",
            SyncRealtime.socketURL("https://abc.supabase.co/", "sb_publishable_x"),
        )
        assertEquals(channel, SyncRealtime.channel("6a1f"))
    }

    @Test
    fun joiningAsksForThePrivateChannelWithTheToken() {
        val frame = Json.parseToJsonElement(SyncRealtime.join(channel, "jwt", "1")).jsonObject
        val payload = frame["payload"]!!.jsonObject
        assertEquals("phx_join", frame["event"]!!.jsonPrimitive.content)
        assertEquals(channel, frame["topic"]!!.jsonPrimitive.content)
        assertEquals("true", payload["config"]!!.jsonObject["private"]!!.jsonPrimitive.content)
        assertEquals("jwt", payload["access_token"]!!.jsonPrimitive.content)
    }

    @Test
    fun repliesBroadcastsAndClosesAreToldApart() {
        fun reply(status: String, ref: String) =
            """{"topic":"$channel","event":"phx_reply","ref":"$ref","payload":{"status":"$status","response":{"reason":"Unauthorized"}}}"""
        assertEquals(SyncRealtime.Incoming.Joined, SyncRealtime.classify(reply("ok", "1"), channel, "1"))
        assertEquals(SyncRealtime.Incoming.Ended("Unauthorized"), SyncRealtime.classify(reply("error", "1"), channel, "1"))
        assertEquals(SyncRealtime.Incoming.Other, SyncRealtime.classify(reply("ok", "7"), channel, "1"))
        val broadcast =
            """{"topic":"$channel","event":"broadcast","payload":{"type":"broadcast","event":"changed","payload":{"cursor":9}}}"""
        assertEquals(SyncRealtime.Incoming.News, SyncRealtime.classify(broadcast, channel, "1"))
        val elsewhere = """{"topic":"realtime:sync:other","event":"broadcast","payload":{}}"""
        assertEquals(SyncRealtime.Incoming.Other, SyncRealtime.classify(elsewhere, channel, "1"))
        val expired = """{"topic":"$channel","event":"system","payload":{"status":"error","message":"Token has expired"}}"""
        assertEquals(SyncRealtime.Incoming.Ended("Token has expired"), SyncRealtime.classify(expired, channel, "1"))
        assertEquals(SyncRealtime.Incoming.Other, SyncRealtime.classify("not json", channel, "1"))
    }

    /** Answers every pull with an empty page and records how long each asked to wait. */
    private class RecordingTransport : SyncTransport {
        val waits: MutableList<Int> = Collections.synchronizedList(mutableListOf())

        override suspend fun push(body: String) = Unit

        override suspend fun changes(since: Long, limit: Int, wait: Int): String {
            waits += wait
            if (wait > 0) delay(20) // a long-poll that hears nothing
            return """{"rows":[],"cursor":$since,"hasMore":false}"""
        }
    }

    private suspend fun waitUntil(condition: () -> Boolean) = withTimeout(5_000) {
        while (!condition()) delay(10)
    }

    @Test
    fun whileTheChannelIsJoinedThereIsNoLongPollOnlyACycleOnNews(): Unit = runBlocking {
        val dir = Files.createTempDirectory("realtime").toFile()
        val database = WorkspaceDatabase.open(dir.resolve("a.sqlite").path)
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
        try {
            WorkspaceRepository(database).bootstrapIfNeeded()
            val store = SyncStore(database)
            store.beginSync("a", "memory://")
            val transport = RecordingTransport()
            val engine = SyncEngine(store, transport, "a")
            engine.sync(0) // the first cycle's snapshot, out of the way
            val scheduler = SyncScheduler(engine, scope)

            scheduler.start()
            waitUntil { transport.waits.contains(25) }

            scheduler.realtimeJoined(true)
            waitUntil { transport.waits.last() == 0 }
            delay(100.milliseconds)
            val settled = transport.waits.size
            delay(200.milliseconds)
            assertEquals("joined and idle: no more requests", settled, transport.waits.size)

            scheduler.newsArrived()
            waitUntil { transport.waits.size == settled + 1 }
            assertEquals(0, transport.waits.last())

            scheduler.realtimeJoined(false)
            waitUntil { transport.waits.last() == 25 }
            scheduler.stop()
        } finally {
            scope.cancel()
            database.close()
            dir.deleteRecursively()
        }
    }
}
