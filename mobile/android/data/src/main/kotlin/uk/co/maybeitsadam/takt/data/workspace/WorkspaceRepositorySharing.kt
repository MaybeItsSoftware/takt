package uk.co.maybeitsadam.takt.data.workspace

import kotlinx.coroutines.flow.Flow
import uniffi.takt_core.SharedList

// Shared lists (docs/shared-lists-plan.md): what this device knows of the
// lists its account shares or was shared, as the last pull told it. The
// requests themselves are the sync controller's; these keep what they answer.

private val SHARING_TABLES = setOf("sync_spaces", "sync_row_spaces", "sync_outbox", "task_lists", "tasks", "task_metadata", "task_dependencies", "agent_instructions")

/** Every shared list this device knows, with its members. */
suspend fun WorkspaceRepository.sharedLists(): List<SharedList> = coreRead { it.sharedLists() }

/** The shared list [listId] is, or null when it isn't shared. */
suspend fun WorkspaceRepository.sharedList(listId: String): SharedList? = coreRead { it.sharedList(listId) }

/** The name to share [listId] under; throws when it can't be shared (the Inbox, someone else's list). */
suspend fun WorkspaceRepository.shareableListName(listId: String): String = coreRead { it.shareableListName(listId) }

/** Keeps the space the server answered sharing a list or joining one with. */
suspend fun WorkspaceRepository.recordSharedSpace(body: String): SharedList =
    database.coreWrite(SHARING_TABLES) { it.recordSharedSpace(body) }

/** After leaving a list (its rows go) or stopping sharing one you own (it stays yours). */
suspend fun WorkspaceRepository.forgetSharedSpace(spaceId: String, stoppedSharing: Boolean) {
    database.coreWrite(SHARING_TABLES) { it.forgetSharedSpace(spaceId, stoppedSharing) }
}

/** The ids of the lists that are shared, for the tree's icons and menus. */
fun WorkspaceRepository.observeSharedListIds(): Flow<Set<String>> =
    database.observe(setOf("sync_spaces")) { db -> db.strings("SELECT listId FROM sync_spaces").toSet() }
