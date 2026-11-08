package uk.co.maybeitsadam.takt.data.sync

import java.io.IOException
import java.time.Duration
import java.time.Instant
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.Interceptor
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import okio.Buffer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/** Throws [error] if [block] returns, or throws anything other than an [E]. */
internal inline fun <reified E : Throwable> expect(block: () -> Unit): E {
    try {
        block()
    } catch (error: Throwable) {
        if (error is E) return error
        throw error
    }
    fail("expected ${E::class.simpleName}")
    error("unreachable")
}

/**
 * The transport against a scripted server: an interceptor that records each
 * request and answers with the next canned response, so nothing touches the
 * network. Shapes are docs/sync.md "Wire protocol" and sync-server/src/devices.rs.
 */
class OkHttpSyncTransportTest {
    private class Recorded(val method: String, val url: String, val authorization: String?, val device: String?, val body: JsonObject?)

    private class ScriptedServer : Interceptor {
        val requests = mutableListOf<Recorded>()
        private val answers = ArrayDeque<Pair<Int, String>>()

        fun answer(status: Int, body: String) = apply { answers.addLast(status to body) }

        val client: OkHttpClient get() = OkHttpClient.Builder().addInterceptor(this).build()

        override fun intercept(chain: Interceptor.Chain): Response {
            val request = chain.request()
            requests += Recorded(
                request.method, request.url.toString(), request.header("Authorization"),
                request.header("X-Priority-Device"), bodyOf(request),
            )
            val (status, body) = answers.removeFirstOrNull() ?: error("no answer scripted for ${request.url}")
            return Response.Builder()
                .request(request)
                .protocol(Protocol.HTTP_1_1)
                .code(status)
                .message("scripted")
                .body(body.toResponseBody("application/json".toMediaType()))
                .build()
        }

        private fun bodyOf(request: Request): JsonObject? {
            val body = request.body ?: return null
            val buffer = Buffer().also(body::writeTo)
            return Json.parseToJsonElement(buffer.readUtf8()).jsonObject
        }
    }

    /** A session that hands out `tok-1`, then `tok-2` once refreshed, unless told to refuse. */
    private class FakeTokens(var refusal: Throwable? = null) : SyncAccessTokens {
        var token = "tok-1"
        var refreshes = 0

        override suspend fun current(): String = token

        override suspend fun refresh(rejected: String): String {
            refusal?.let { throw it }
            refreshes += 1
            token = "tok-${refreshes + 1}"
            return token
        }
    }

    private val server = ScriptedServer()
    private val tokens = FakeTokens()
    private val base = "https://sync.example.com"
    private val deviceId = "7d1f3c2a-0000-4000-8000-000000000001"
    private val unauthorized = """{"error":"unauthorized"}"""

    private fun JsonObject.text(key: String): String? = this[key]?.jsonPrimitive?.content

    private fun device() = OkHttpSyncTransport(base, deviceId, tokens, server.client)

    @Test
    fun everyRequestCarriesTheBearerAndTheDevice(): Unit = runBlocking {
        server.answer(200, """{"accepted":0,"cursor":3}""")
            .answer(200, """{"rows":[],"cursor":3,"hasMore":false}""")
            .answer(200, """{"accountId":"acc-1","devices":[]}""")
            .answer(200, """{"ok":true}""")
            .answer(200, """{"ok":true}""")
        device().push(EMPTY_PUSH)
        device().changes(0, 100, 99)
        device().account()
        device().signOut()
        device().deleteAccount()

        assertEquals(
            listOf(
                "POST $base/v1/push",
                "GET $base/v1/changes?since=0&limit=100&wait=25",
                "GET $base/v1/account",
                "POST $base/v1/sign-out",
                "POST $base/v1/account/delete",
            ),
            server.requests.map { "${it.method} ${it.url}" },
        )
        server.requests.forEach {
            assertEquals("Bearer tok-1", it.authorization)
            assertEquals(deviceId, it.device)
        }
    }

    @Test
    fun registersTheDeviceWithItsOwnId(): Unit = runBlocking {
        server.answer(200, """{"ok":true}""")
        device().registerDevice("Pixel 9")
        val request = server.requests.single()
        assertEquals("POST", request.method)
        assertEquals("$base/v1/devices", request.url)
        assertEquals(deviceId, request.body!!.text("id"))
        assertEquals("Pixel 9", request.body.text("name"))
        assertEquals("android", request.body.text("platform"))
        assertEquals("Bearer tok-1", request.authorization)
    }

    @Test
    fun readsTheAccountWithItsDevices(): Unit = runBlocking {
        server.answer(
            200,
            """
            {"accountId":"acc-1","email":"me@example.com","devices":[
              {"id":"dev-0","name":"Adam's Mac","platform":"macos","createdAt":"2026-10-01T09:00:00Z","lastSeenAt":"2026-10-03T08:00:00Z","current":false},
              {"id":"dev-1","name":null,"platform":"android","createdAt":"2026-10-02T09:00:00Z","lastSeenAt":null,"current":true}
            ]}
            """.trimIndent(),
        )
        val account = device().account()
        assertEquals("me@example.com", account.email)
        assertEquals(listOf("dev-0", "dev-1"), account.devices.map { it.id })
        assertEquals("Adam's Mac", account.devices[0].name)
        assertNull(account.devices[1].name)
        assertNull(account.devices[1].lastSeenAt)
        assertTrue(account.devices[1].current)
    }

    @Test
    fun a402IsTaktSyncAndTheAccountSaysWhetherItIsPaid(): Unit = runBlocking {
        server.answer(402, """{"error":"Hosted sync needs Takt Sync (£1 a year).","subscription":"required"}""")
        expect<SyncException.SubscriptionRequired> { device().push(EMPTY_PUSH) }
        assertEquals("a 402 is not refreshed", 0, tokens.refreshes)

        server.answer(
            200,
            """{"accountId":"a","email":null,"devices":[],"subscription":{"required":true,"active":false,"expiresAt":null,"comp":false}}""",
        )
        val unpaid = device().account().subscription
        assertEquals(true, unpaid?.required)
        assertEquals(false, unpaid?.active)
        server.answer(200, """{"accountId":"a","email":null,"devices":[]}""")
        assertNull("a server from before Takt Sync says nothing", device().account().subscription)
    }

    @Test
    fun aRefusedTokenIsRefreshedOnceAndTheRequestRetried(): Unit = runBlocking {
        server.answer(401, unauthorized).answer(200, """{"accepted":2,"cursor":9}""")
        device().push(EMPTY_PUSH)

        assertEquals(1, tokens.refreshes)
        assertEquals(listOf("Bearer tok-1", "Bearer tok-2"), server.requests.map { it.authorization })
        assertEquals(listOf("$base/v1/push", "$base/v1/push"), server.requests.map { it.url })
        assertEquals(deviceId, server.requests[1].device)
    }

    @Test
    fun aFailedRefreshIsASignOutAndNothingIsRetried(): Unit = runBlocking {
        tokens.refusal = SyncException.Unauthorized()
        server.answer(401, unauthorized)
        expect<SyncException.Unauthorized> { runBlocking { device().changes(0, 100, 0) } }
        assertEquals(1, server.requests.size)
    }

    @Test
    fun aRefreshedTokenTheServerStillRefusesIsASignOut(): Unit = runBlocking {
        server.answer(401, unauthorized).answer(401, unauthorized)
        expect<SyncException.Unauthorized> { runBlocking { device().account() } }
        assertEquals(1, tokens.refreshes)
        assertEquals(2, server.requests.size)
    }

    @Test
    fun aRefreshThatCannotReachSupabaseIsAFailureNotASignOut(): Unit = runBlocking {
        tokens.refusal = IOException("offline")
        server.answer(401, unauthorized)
        expect<IOException> { runBlocking { device().push(EMPTY_PUSH) } }
    }

    @Test
    fun otherErrorsSayWhatTheServerSaidWithoutRefreshing(): Unit = runBlocking {
        server.answer(503, """{"error":"deleting accounts isn't set up on this server"}""").answer(500, "<html>")
        val unavailable = expect<SyncException.Server> { runBlocking { device().deleteAccount() } }
        assertEquals("Deleting accounts isn't set up on this server.", unavailable.message)
        val opaque = expect<SyncException.Server> { runBlocking { device().account() } }
        assertEquals("The sync server answered 500.", opaque.message)
        assertEquals(0, tokens.refreshes)
    }
}

/** When [RefreshingAccessTokens] refreshes, and what its failures mean. */
class RefreshingAccessTokensTest {
    private var now = Instant.parse("2026-10-03T12:00:00Z")
    private var session: AccessToken? = AccessToken("tok-1", now.plus(Duration.ofHours(1)))
    private var refreshes = 0
    private var refusal: Throwable? = null

    private val tokens = RefreshingAccessTokens(
        session = { session },
        refreshSession = {
            refusal?.let { throw it }
            refreshes += 1
            AccessToken("tok-${refreshes + 1}", now.plus(Duration.ofHours(1))).also { session = it }
        },
        now = { now },
    )

    @Test
    fun aTokenWithTimeLeftIsSentAsItIs(): Unit = runBlocking {
        assertEquals("tok-1", tokens.current())
        assertEquals(0, refreshes)
    }

    @Test
    fun aTokenAboutToExpireIsRefreshedBeforeTheRequest(): Unit = runBlocking {
        now = now.plus(Duration.ofMinutes(59).plusSeconds(30))
        assertEquals("tok-2", tokens.current())
        assertEquals(1, refreshes)
    }

    @Test
    fun aRejectedTokenIsRefreshedOnlyOnce(): Unit = runBlocking {
        assertEquals("tok-2", tokens.refresh(rejected = "tok-1"))
        // A second request that was refused with the same old token takes the new one.
        assertEquals("tok-2", tokens.refresh(rejected = "tok-1"))
        assertEquals(1, refreshes)
    }

    @Test
    fun noSessionIsSignedOut(): Unit = runBlocking {
        session = null
        expect<SyncException.Unauthorized> { runBlocking { tokens.current() } }
        expect<SyncException.Unauthorized> { runBlocking { tokens.refresh("tok-1") } }
    }

    @Test
    fun aRefusedRefreshIsSignedOutAndAnUnreachableOneIsNot(): Unit = runBlocking {
        refusal = SyncException.Unauthorized()
        expect<SyncException.Unauthorized> { runBlocking { tokens.refresh("tok-1") } }
        refusal = IOException("offline")
        expect<IOException> { runBlocking { tokens.refresh("tok-1") } }
    }
}

private const val EMPTY_PUSH = """{"changes":[]}"""
