package uk.co.maybeitsadam.takt.data

import androidx.sqlite.SQLiteConnection
import androidx.sqlite.driver.bundled.BundledSQLiteDriver
import androidx.sqlite.execSQL
import java.nio.file.Files
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uk.co.maybeitsadam.takt.data.db.Db
import uk.co.maybeitsadam.takt.data.db.WorkspaceDatabase
import uk.co.maybeitsadam.takt.data.db.WorkspaceSchema

/**
 * Android opens through the Rust core's migrations (core/src/schema). These
 * hold the result to the fixture the core generates, on a new database and on
 * Android databases from before each of the last three migrations.
 */
class WorkspaceSchemaTest {
    private data class SchemaObject(val type: String, val name: String, val sql: String?)

    private fun schema(db: Db): Set<SchemaObject> =
        db.query("SELECT type, name, sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'") {
            SchemaObject(it.string("type"), it.string("name"), it.stringOrNull("sql"))
        }.toSet()

    /** What the fixture alone produces, executed by plain sqlite. */
    private fun fixtureSchema(): Set<SchemaObject> {
        val connection = BundledSQLiteDriver().open(":memory:")
        try {
            for (statement in SchemaFixture.splitStatements(SchemaFixture.sql())) {
                connection.execSQL(statement)
            }
            return schema(Db(connection))
        } finally {
            connection.close()
        }
    }

    @Test
    fun freshDatabaseIsTheFixtureWithItsSyncObjects(): Unit = runBlocking {
        TestWorkspace().use { workspace ->
            val actual = workspace.database.read { schema(it) }
            val fixture = fixtureSchema()
            assertTrue("every fixture object is present unchanged", actual.containsAll(fixture))

            // The core migrates every client the same way and the fixture is
            // generated from it, so nothing is left over on either side.
            assertEquals(emptySet<Pair<String, String>>(), (actual - fixture).map { it.type to it.name }.toSet())
            val present = actual.map { it.type to it.name }.toSet()
            val expected = buildSet {
                add("table" to "sync_control")
                add("table" to "sync_outbox")
                add("table" to "sync_state")
                add("index" to "sync_outbox_on_row")
                for ((table, _) in WorkspaceSchema.syncedTables) {
                    for (op in listOf("insert", "update", "delete")) add("trigger" to "sync_outbox_${table}_$op")
                }
            }
            assertTrue("missing: ${expected - present}", present.containsAll(expected))

            val migrations = workspace.database.read { it.strings("SELECT identifier FROM grdb_migrations") }
            assertEquals(35, migrations.size)
            assertTrue("v17_sync" in migrations)
            assertTrue("v18_themes_and_preferences" in migrations)
            assertTrue("v19_habit_options" in migrations)
            assertTrue("v20_waiting_follow_ups" in migrations)
            assertTrue("v21_task_dependencies" in migrations)
            assertTrue("v22_matrix_order" in migrations)
            assertTrue("v23_filter_lists" in migrations)
            assertTrue("v24_condition_categories" in migrations)
            assertTrue("v25_agent_instructions" in migrations)
            assertTrue("v26_commitment_missed_stack" in migrations)
            assertTrue("v27_condition_order_and_context" in migrations)
            assertTrue("v28_synced_focus_context" in migrations)
            assertTrue("v29_daily_contribution_skips" in migrations)
            assertTrue("v30_google_calendar" in migrations)
            assertTrue("v31_link_details" in migrations)
            assertTrue("v32_follow_up_repeat" in migrations)
            assertTrue("v33_recurrence_series" in migrations)
            assertTrue("v34_condition_provides" in migrations)
            assertTrue("v35_series_title" in migrations)
            assertEquals(listOf(0L to 0L), workspace.database.read { db ->
                db.query("SELECT recording, applying FROM sync_control") { it.long("recording") to it.long("applying") }
            })
            assertEquals(1L, workspace.database.read { it.long("SELECT suppressed FROM undo_control WHERE id = 0") })
        }
    }

    @Test
    fun reopeningDoesNotApplyV17Twice(): Unit = runBlocking {
        val dir = Files.createTempDirectory("priority-schema").toFile()
        val path = dir.resolve("priority.sqlite").path
        WorkspaceDatabase.open(path).close()
        val reopened = WorkspaceDatabase.open(path)
        val count = reopened.read { it.long("SELECT COUNT(*) FROM grdb_migrations WHERE identifier = 'v17_sync'") }
        assertEquals(1L, count)
        // The sync triggers are the core's, installed once each.
        assertEquals(60L, reopened.read {
            it.long("SELECT COUNT(*) FROM sqlite_master WHERE name LIKE 'sync_outbox_%' AND type = 'trigger'")
        })
        reopened.close()
        dir.deleteRecursively()
    }

    // Rewinding a fixture database to how an older install had it. A real
    // install that lacks one migration lacks every later one too, since they
    // only ever apply in order, so each rewind takes the later ones off first.

    private fun rewindV35(connection: SQLiteConnection) {
        // The journal's and the outbox's metadata triggers name the new
        // column, so they come off first; v35 reinstalls them.
        for (op in listOf("insert", "update", "delete")) {
            connection.execSQL("DROP TRIGGER IF EXISTS change_log_task_metadata_$op")
            connection.execSQL("DROP TRIGGER IF EXISTS sync_outbox_task_metadata_$op")
        }
        connection.execSQL("ALTER TABLE task_metadata DROP COLUMN seriesTitle")
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v35_series_title'")
    }

    private fun rewindV34(connection: SQLiteConnection) {
        rewindV35(connection)
        // The condition triggers, and the context's outbox triggers, name the
        // new columns, so they come off first; v34 reinstalls them.
        for (op in listOf("insert", "update", "delete")) {
            connection.execSQL("DROP TRIGGER IF EXISTS change_log_task_conditions_$op")
            connection.execSQL("DROP TRIGGER IF EXISTS sync_outbox_task_conditions_$op")
            connection.execSQL("DROP TRIGGER IF EXISTS sync_outbox_focus_contexts_$op")
        }
        connection.execSQL("ALTER TABLE task_conditions DROP COLUMN providesJSON")
        connection.execSQL("ALTER TABLE focus_contexts DROP COLUMN offIdsJSON")
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v34_condition_provides'")
    }

    private fun rewindV33(connection: SQLiteConnection) {
        rewindV34(connection)
        // The journal's and the outbox's metadata triggers name the new
        // column, so they come off first; v33 reinstalls them.
        for (op in listOf("insert", "update", "delete")) {
            connection.execSQL("DROP TRIGGER IF EXISTS change_log_task_metadata_$op")
            connection.execSQL("DROP TRIGGER IF EXISTS sync_outbox_task_metadata_$op")
        }
        connection.execSQL("ALTER TABLE task_metadata DROP COLUMN seriesTaskId")
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v33_recurrence_series'")
    }

    private fun rewindV32(connection: SQLiteConnection) {
        rewindV33(connection)
        // The journal's and the outbox's metadata triggers name the new
        // columns, so they come off first; v32 reinstalls them.
        for (op in listOf("insert", "update", "delete")) {
            connection.execSQL("DROP TRIGGER IF EXISTS change_log_task_metadata_$op")
            connection.execSQL("DROP TRIGGER IF EXISTS sync_outbox_task_metadata_$op")
        }
        for (column in listOf("waitingFollowUpEveryDays", "waitingFollowUpCount")) {
            connection.execSQL("ALTER TABLE task_metadata DROP COLUMN $column")
        }
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v32_follow_up_repeat'")
    }

    private fun rewindV31(connection: SQLiteConnection) {
        rewindV32(connection)
        // The journal's and the outbox's metadata triggers name the new
        // column, so they come off first; v31 reinstalls them.
        for (op in listOf("insert", "update", "delete")) {
            connection.execSQL("DROP TRIGGER IF EXISTS change_log_task_metadata_$op")
            connection.execSQL("DROP TRIGGER IF EXISTS sync_outbox_task_metadata_$op")
        }
        connection.execSQL("ALTER TABLE task_metadata DROP COLUMN linkDetailsJSON")
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v31_link_details'")
    }

    private fun rewindV30(connection: SQLiteConnection) {
        rewindV31(connection)
        for (op in listOf("insert", "update", "delete")) {
            connection.execSQL("DROP TRIGGER sync_outbox_calendar_event_links_$op")
        }
        connection.execSQL("DROP TABLE calendar_event_links")
        connection.execSQL("DROP TABLE calendar_sync_tokens")
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v30_google_calendar'")
    }

    private fun rewindV29(connection: SQLiteConnection) {
        rewindV30(connection)
        // The journal's and the outbox's contribution triggers name the new
        // column, so they come off first; v29 reinstalls them.
        for (op in listOf("insert", "update", "delete")) {
            connection.execSQL("DROP TRIGGER change_log_daily_contributions_$op")
            connection.execSQL("DROP TRIGGER sync_outbox_daily_contributions_$op")
        }
        connection.execSQL("ALTER TABLE daily_contributions DROP COLUMN skippedAt")
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v29_daily_contribution_skips'")
    }

    private fun rewindV28(connection: SQLiteConnection) {
        rewindV29(connection)
        // The table stays; only its outbox triggers are v28's.
        for (op in listOf("insert", "update", "delete")) {
            connection.execSQL("DROP TRIGGER IF EXISTS sync_outbox_focus_contexts_$op")
        }
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v28_synced_focus_context'")
    }

    private fun rewindV27(connection: SQLiteConnection) {
        rewindV28(connection)
        // The condition triggers name the new column, so they come off first;
        // v27 reinstalls them, and v24's rewind finds them gone.
        for (op in listOf("insert", "update", "delete")) {
            connection.execSQL("DROP TRIGGER IF EXISTS change_log_task_conditions_$op")
            connection.execSQL("DROP TRIGGER IF EXISTS sync_outbox_task_conditions_$op")
        }
        connection.execSQL("ALTER TABLE task_conditions DROP COLUMN sortOrder")
        connection.execSQL("DROP TABLE focus_contexts")
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v27_condition_order_and_context'")
    }

    private fun rewindV26(connection: SQLiteConnection) {
        rewindV27(connection)
        // The journal's and the outbox's dailies triggers name the new
        // column, so they come off first; v19's rewind finds them gone.
        for (op in listOf("insert", "update", "delete")) {
            connection.execSQL("DROP TRIGGER change_log_dailies_$op")
            connection.execSQL("DROP TRIGGER sync_outbox_dailies_$op")
        }
        connection.execSQL("ALTER TABLE dailies DROP COLUMN stacksMissedFrom")
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v26_commitment_missed_stack'")
    }

    private fun rewindV25(connection: SQLiteConnection) {
        rewindV26(connection)
        // Dropping the table drops its journal and outbox triggers with it.
        connection.execSQL("DROP TABLE agent_instructions")
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v25_agent_instructions'")
    }

    private fun rewindV24(connection: SQLiteConnection) {
        rewindV25(connection)
        // The journal's and the outbox's condition triggers name the new
        // column, so they come off first.
        for (op in listOf("insert", "update", "delete")) {
            connection.execSQL("DROP TRIGGER IF EXISTS change_log_task_conditions_$op")
            connection.execSQL("DROP TRIGGER IF EXISTS sync_outbox_task_conditions_$op")
        }
        connection.execSQL("ALTER TABLE task_conditions DROP COLUMN category")
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v24_condition_categories'")
    }

    private fun rewindV23(connection: SQLiteConnection) {
        rewindV24(connection)
        // Dropping the table drops its index and its journal and outbox triggers with it.
        connection.execSQL("DROP TABLE filter_lists")
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v23_filter_lists'")
    }

    private fun rewindV22(connection: SQLiteConnection) {
        rewindV23(connection)
        for (op in listOf("insert", "update", "delete")) {
            connection.execSQL("DROP TRIGGER IF EXISTS change_log_task_metadata_$op")
            connection.execSQL("DROP TRIGGER IF EXISTS sync_outbox_task_metadata_$op")
        }
        connection.execSQL("ALTER TABLE task_metadata DROP COLUMN matrixRank")
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v22_matrix_order'")
    }

    private fun rewindV21(connection: SQLiteConnection) {
        rewindV22(connection)
        // Dropping the table drops its journal and outbox triggers with it.
        connection.execSQL("DROP TABLE task_dependencies")
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v21_task_dependencies'")
    }

    private fun rewindV20(connection: SQLiteConnection) {
        rewindV21(connection)
        for (op in listOf("insert", "update", "delete")) {
            connection.execSQL("DROP TRIGGER IF EXISTS change_log_task_metadata_$op")
            connection.execSQL("DROP TRIGGER IF EXISTS sync_outbox_task_metadata_$op")
        }
        for (column in listOf("waitingOn", "waitingFollowUpAt", "waitingFollowUpTaskId", "followUpOfTaskId")) {
            connection.execSQL("ALTER TABLE task_metadata DROP COLUMN $column")
        }
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v20_waiting_follow_ups'")
    }

    private fun rewindV19(connection: SQLiteConnection) {
        rewindV20(connection)
        // The journal's and the outbox's dailies triggers name the new
        // columns, so they come off first.
        for (op in listOf("insert", "update", "delete")) {
            connection.execSQL("DROP TRIGGER IF EXISTS change_log_dailies_$op")
            connection.execSQL("DROP TRIGGER IF EXISTS sync_outbox_dailies_$op")
        }
        for (column in listOf("sourceTaskId", "placementColumn", "dropsAtDayEnd", "expiryRule", "expiresAt")) {
            connection.execSQL("ALTER TABLE dailies DROP COLUMN $column")
        }
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v19_habit_options'")
    }

    private fun rewindV18(connection: SQLiteConnection) {
        rewindV19(connection)
        for (table in listOf("themes", "preferences")) {
            for (op in listOf("insert", "update", "delete")) connection.execSQL("DROP TRIGGER sync_outbox_${table}_$op")
            connection.execSQL("DROP TABLE $table")
        }
        connection.execSQL("DELETE FROM grdb_migrations WHERE identifier = 'v18_themes_and_preferences'")
    }

    /**
     * Writes the fixture to a new file, rewinds it with [rewind], opens it
     * through the core, and checks it comes out as the fixture again, with
     * every migration recorded once.
     */
    private fun assertUpgradesToTheFixture(name: String, rewind: (SQLiteConnection) -> Unit): Unit = runBlocking {
        val expected = fixtureSchema()
        val dir = Files.createTempDirectory("priority-$name").toFile()
        val path = dir.resolve("priority.sqlite").path
        val connection = BundledSQLiteDriver().open(path)
        try {
            for (statement in SchemaFixture.splitStatements(SchemaFixture.sql())) connection.execSQL(statement)
            rewind(connection)
        } finally {
            connection.close()
        }

        val upgraded = WorkspaceDatabase.open(path)
        try {
            assertEquals(expected, upgraded.read { schema(it) })
            val migrations = upgraded.read { it.strings("SELECT identifier FROM grdb_migrations") }
            assertEquals(35, migrations.size)
            assertEquals(35, migrations.toSet().size)
        } finally {
            upgraded.close()
            dir.deleteRecursively()
        }
    }

    /** A v25 database takes `stacksMissedFrom` on `dailies`. */
    @Test
    fun aV25DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v25", ::rewindV26)

    /** A v34 database takes `seriesTitle` on `task_metadata`. */
    @Test
    fun aV34DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v34", ::rewindV35)

    /** A v33 database takes what a condition provides and the context's overrides. */
    @Test
    fun aV33DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v33", ::rewindV34)

    /** A v32 database takes `seriesTaskId` on `task_metadata`. */
    @Test
    fun aV32DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v32", ::rewindV33)

    /** A v31 database takes the follow-up repeat and count on `task_metadata`. */
    @Test
    fun aV31DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v31", ::rewindV32)

    /** A v30 database takes `linkDetailsJSON` on `task_metadata`. */
    @Test
    fun aV30DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v30", ::rewindV31)

    /** A v29 database takes the calendar links and sync tokens. */
    @Test
    fun aV29DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v29", ::rewindV30)

    /** A v28 database takes `skippedAt` on `daily_contributions`. */
    @Test
    fun aV28DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v28", ::rewindV29)

    /** A v27 database takes the context's outbox triggers. */
    @Test
    fun aV27DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v27", ::rewindV28)

    /** A v26 database takes the conditions' `sortOrder` and `focus_contexts`. */
    @Test
    fun aV26DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v26", ::rewindV27)

    /** A v24 database takes `agent_instructions` and its triggers. */
    @Test
    fun aV24DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v24", ::rewindV25)

    /** A v23 database takes `category` on `task_conditions`. */
    @Test
    fun aV23DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v23", ::rewindV24)

    /** A v22 database takes `filter_lists` and its triggers. */
    @Test
    fun aV22DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v22", ::rewindV23)

    /** A v21 database takes `matrixRank` on `task_metadata`. */
    @Test
    fun aV21DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v21", ::rewindV22)

    /** A v20 database takes `task_dependencies` and its triggers. */
    @Test
    fun aV20DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v20", ::rewindV21)

    /** A v19 database takes the four waiting columns on `task_metadata`. */
    @Test
    fun aV19DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v19", ::rewindV20)

    /** A v18 database takes the habit columns, then the waiting ones. */
    @Test
    fun aV18DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v18", ::rewindV19)

    /** A v17 database takes themes and preferences, then v19 and v20. */
    @Test
    fun aV17DatabaseUpgradesToTheFixture() = assertUpgradesToTheFixture("v17", ::rewindV18)
}
