package uk.co.maybeitsadam.takt.data.calendar

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import uk.co.maybeitsadam.takt.data.TestWorkspace
import uk.co.maybeitsadam.takt.data.workspace.setPreference
import uniffi.takt_core.GoogleCalendarAgendaKind
import uniffi.takt_core.GoogleCalendarHttpRequest
import uniffi.takt_core.GoogleCalendarSettings
import uniffi.takt_core.googleCalendarEventId

/** The pass loop and the agenda round the core, against a scripted Google. */
class GoogleCalendarRepositoryTest {
    private val calendar = "takt@group.calendar.google.com"

    /** Answers by method and URL; records what was sent. */
    private class FakeGoogle(val answer: (GoogleCalendarHttpRequest) -> GoogleCalendarAnswer) : GoogleCalendarSender {
        val sent = mutableListOf<GoogleCalendarHttpRequest>()
        override suspend fun send(request: GoogleCalendarHttpRequest): GoogleCalendarAnswer {
            sent += request
            return answer(request)
        }
    }

    private fun TestWorkspace.calendar(google: FakeGoogle) =
        GoogleCalendarRepository(repository, google) { clock.instant() }

    private suspend fun TestWorkspace.timedTask(title: String): String {
        val workspace = repository.bootstrapIfNeeded()
        val inbox = repository.inbox(workspace.id)!!
        // Named: positionally this resolves to the `capturing` overload, with the title as the list.
        return repository.createTask(listId = inbox.id, title = title, startAt = clock.instant().plusSeconds(3_600)).id
    }

    /** Two-way on, with timed tasks put on it as events (off by default). */
    private suspend fun TestWorkspace.enable(agenda: List<String> = emptyList()) {
        GoogleCalendarRepository(repository, FakeGoogle { error("no requests") }) { clock.instant() }
            .setSettings(GoogleCalendarSettings(calendar, false, agenda))
        repository.setPreference("googleCalendar.taskEvents", "true")
    }

    @Test
    fun aPassIsIdleWhileTwoWaySyncIsOff(): Unit = runBlocking {
        TestWorkspace().use { w ->
            w.repository.bootstrapIfNeeded()
            val google = FakeGoogle { error("nothing should be sent") }
            assertEquals(GoogleCalendarPassOutcome.Idle, w.calendar(google).pass("token"))
            assertTrue(google.sent.isEmpty())
        }
    }

    @Test
    fun aFirstPassListsEverythingCreatesTheEventAndKeepsTheSyncToken(): Unit = runBlocking {
        TestWorkspace().use { w ->
            val task = w.timedTask("Write report")
            w.enable()
            val google = FakeGoogle { request ->
                when (request.method) {
                    "GET" -> GoogleCalendarAnswer(200, """{"items":[],"nextSyncToken":"sync-1"}""")
                    "POST" -> GoogleCalendarAnswer(200, """{"id":"${googleCalendarEventId(task)}","updated":"2026-10-02T09:00:01Z"}""")
                    else -> GoogleCalendarAnswer(500, "{}")
                }
            }
            val outcome = w.calendar(google).pass("token")
            assertEquals(GoogleCalendarPassOutcome.Done(0, 0, 1, emptyList()), outcome)
            assertTrue("a full list has no sync token", !google.sent[0].url.contains("syncToken"))
            assertTrue(google.sent[0].headers.any { it.name == "Authorization" && it.value == "Bearer token" })
            assertEquals("POST", google.sent[1].method)

            // The next pass carries on from the stored token and has nothing to send.
            google.sent.clear()
            assertEquals(GoogleCalendarPassOutcome.Done(0, 0, 0, emptyList()), w.calendar(google).pass("token"))
            assertEquals(1, google.sent.size)
            assertTrue(google.sent[0].url.contains("syncToken=sync-1"))
        }
    }

    @Test
    fun anExpiredSyncTokenStartsAFullListAgain(): Unit = runBlocking {
        TestWorkspace().use { w ->
            w.repository.bootstrapIfNeeded()
            w.enable()
            w.repository.database.core.setGoogleCalendarSyncToken(calendar, "old", 0)
            val google = FakeGoogle { request ->
                if (request.url.contains("syncToken=old")) {
                    GoogleCalendarAnswer(410, """{"error":{"message":"Sync token is no longer valid"}}""")
                } else {
                    GoogleCalendarAnswer(200, """{"items":[],"nextSyncToken":"fresh"}""")
                }
            }
            w.calendar(google).pass("token")
            assertEquals(2, google.sent.size)
            assertTrue(!google.sent[1].url.contains("syncToken"))
            assertEquals(
                "fresh",
                w.repository.database.read { it.string("SELECT syncToken FROM calendar_sync_tokens WHERE calendarId = ?", calendar) },
            )
        }
    }

    @Test
    fun anInsertThatClashesIsSentAgainAsTheFallback(): Unit = runBlocking {
        TestWorkspace().use { w ->
            w.timedTask("Call Sam")
            w.enable()
            val google = FakeGoogle { request ->
                when (request.method) {
                    "GET" -> GoogleCalendarAnswer(200, """{"items":[],"nextSyncToken":"s"}""")
                    "POST" -> GoogleCalendarAnswer(409, """{"error":{"message":"The requested identifier already exists."}}""")
                    else -> GoogleCalendarAnswer(200, """{"updated":"2026-10-02T09:00:01Z"}""")
                }
            }
            val outcome = w.calendar(google).pass("token") as GoogleCalendarPassOutcome.Done
            assertEquals(1, outcome.pushed)
            assertEquals(listOf("GET", "POST", "PATCH"), google.sent.map { it.method })
        }
    }

    @Test
    fun aRefusedTokenStopsThePass(): Unit = runBlocking {
        TestWorkspace().use { w ->
            w.repository.bootstrapIfNeeded()
            w.enable()
            val google = FakeGoogle { GoogleCalendarAnswer(401, "{}") }
            try {
                w.calendar(google).pass("token")
                fail("expected Unauthorized")
            } catch (_: GoogleCalendarException.Unauthorized) {
            }
            assertNull(w.repository.database.read { it.string("SELECT syncToken FROM calendar_sync_tokens") })
        }
    }

    @Test
    fun theAgendaInterleavesEventsWithTimedTasks(): Unit = runBlocking {
        TestWorkspace().use { w ->
            val task = w.timedTask("Write report")
            w.enable(agenda = listOf("primary"))
            val events = """{"items":[
                {"id":"standup","status":"confirmed","summary":"Standup",
                 "start":{"dateTime":"2026-10-02T09:30:00Z"},"end":{"dateTime":"2026-10-02T09:45:00Z"}},
                {"id":"holiday","status":"confirmed","summary":"Bank holiday",
                 "start":{"date":"2026-10-02"},"end":{"date":"2026-10-03"}}
            ]}"""
            val google = FakeGoogle { request ->
                if (request.url.contains("/calendars/primary/")) GoogleCalendarAnswer(200, events) else GoogleCalendarAnswer(200, """{"items":[]}""")
            }
            val agenda = w.calendar(google).agenda("token", java.time.LocalDate.of(2026, 10, 2))
            assertEquals(listOf("Bank holiday", "Standup", "Write report"), agenda.map { it.title })
            assertEquals(GoogleCalendarAgendaKind.TASK, agenda.last().kind)
            assertEquals(task, agenda.last().id)
            assertEquals(2, google.sent.size)
        }
    }

    @Test
    fun settingsAreStoredAndObserved(): Unit = runBlocking {
        TestWorkspace().use { w ->
            w.repository.bootstrapIfNeeded()
            val repository = w.calendar(FakeGoogle { error("no requests") })
            assertEquals(GoogleCalendarSettings(null, false, emptyList()), repository.settings())
            repository.setSettings(GoogleCalendarSettings(calendar, true, listOf("primary")))
            assertEquals(GoogleCalendarSettings(calendar, true, listOf("primary")), repository.observeSettings().first())
        }
    }
}
