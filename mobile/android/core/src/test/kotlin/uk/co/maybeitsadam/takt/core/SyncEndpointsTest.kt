package uk.co.maybeitsadam.takt.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/** "Use a different server": what the three typed fields resolve to. Mirrors `SyncEndpointsTests.swift`. */
class SyncEndpointsTest {
    private val hosted = SyncEndpoints("https://takt-sync.up.railway.app", "https://ref.supabase.co", "sb_publishable_hosted")

    private fun resolve(server: String = "", project: String = "", key: String = "") =
        SyncEndpoints.resolve(server, project, key, hosted)

    @Test
    fun theHostedServersOldAddressesFollowIt() {
        val now = "https://api.takt.maybeitssoftware.co.uk"
        assertEquals(now, SyncEndpoints.currentServer("https://takt-sync.up.railway.app", now))
        assertEquals(now, SyncEndpoints.currentServer("https://priority-sync.up.railway.app/", now))
        assertEquals("https://sync.example.com", SyncEndpoints.currentServer("https://sync.example.com", now))
    }

    @Test
    fun blankFieldsAreTaktsOwn() {
        assertEquals(hosted, resolve())
        assertEquals(hosted, resolve(" ", " \n", ""))
        assertTrue(hosted.usesAccountsOf(hosted))
    }

    @Test
    fun aServerAloneKeepsTaktsAccounts() {
        val endpoints = resolve(server = "sync.example.com/")
        assertEquals("https://sync.example.com", endpoints.serverURL)
        assertTrue(endpoints.usesAccountsOf(hosted))
    }

    @Test
    fun allThreeAreNormalised() {
        val endpoints = resolve(" https://sync.example.com/takt/?x=1 ", "abc.supabase.co/rest/v1/", "  sb_publishable_own ")
        assertEquals("https://sync.example.com/takt", endpoints.serverURL)
        assertEquals("https://abc.supabase.co", endpoints.supabaseURL)
        assertEquals("sb_publishable_own", endpoints.supabaseKey)
        assertFalse(endpoints.usesAccountsOf(hosted))

        val local = resolve("http://10.0.2.2:8080", "http://10.0.2.2:54321/auth/v1", "eyJhbGciOi.x.y")
        assertEquals("http://10.0.2.2:8080", local.serverURL)
        assertEquals("http://10.0.2.2:54321", local.supabaseURL)
    }

    @Test
    fun whatCantBeUsedSaysWhy() {
        listOf(
            Triple("ftp://sync.example.com", "", "") to "sync server address",
            Triple("", "https://abc.supabase.co", "") to "publishable key",
            Triple("", "", "sb_publishable_own") to "URL as well",
            Triple("", "ftp://abc", "sb_publishable_own") to "Supabase URL isn't",
            Triple("", "https://abc.supabase.co", "sb_secret_oops") to "secret key",
            Triple("", "https://abc.supabase.co", "sb_publishable own") to "space",
        ).forEach { (typed, expected) ->
            try {
                resolve(typed.first, typed.second, typed.third)
                fail("$typed resolved")
            } catch (error: InvalidSyncEndpoints) {
                assertTrue("${error.message} should mention $expected", error.message.orEmpty().contains(expected))
            }
        }
    }

    @Test
    fun onlyHttpAddressesWithAHost() {
        assertNull(SyncEndpoints.httpURL(""))
        assertNull(SyncEndpoints.httpURL("https://"))
        assertNull(SyncEndpoints.httpURL("mailto:me@example.com"))
        assertEquals("https://sync.example.com", SyncEndpoints.httpURL("HTTPS://sync.example.com"))
    }
}
