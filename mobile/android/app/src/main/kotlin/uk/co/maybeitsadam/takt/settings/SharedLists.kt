package uk.co.maybeitsadam.takt.settings

import uk.co.maybeitsadam.takt.app.AppContainer
import uk.co.maybeitsadam.takt.data.workspace.forgetSharedSpace
import uk.co.maybeitsadam.takt.data.workspace.recordSharedSpace
import uk.co.maybeitsadam.takt.data.workspace.shareableListName
import uk.co.maybeitsadam.takt.data.workspace.sharedList
import uniffi.takt_core.SharedList
import uniffi.takt_core.SyncInvitePreview
import uniffi.takt_core.syncAcceptInvitePath
import uniffi.takt_core.syncDecodeInvite
import uniffi.takt_core.syncDecodeInvitePreview
import uniffi.takt_core.syncInviteCode
import uniffi.takt_core.syncInviteCodeGrouped
import uniffi.takt_core.syncInviteLink
import uniffi.takt_core.syncInvitePath
import uniffi.takt_core.syncShareListBody
import uniffi.takt_core.syncSpaceInvitesPath
import uniffi.takt_core.syncSpaceMemberPath
import uniffi.takt_core.syncSpacePath
import uniffi.takt_core.syncSpacesPath

/** An invite to send: the link, and the code in groups for reading out. */
data class SharedListInvite(val listName: String, val link: String, val code: String)

/**
 * Shared lists (`docs/shared-lists-plan.md`) on Android: sharing a list,
 * inviting, joining, leaving and stopping sharing. Each is one or two
 * requests through sync; the core makes the bodies and keeps the answers,
 * and the next sync brings or sends the rows.
 */
class SharedLists(private val container: AppContainer) {
    private val sync get() = container.sync

    private suspend fun repository() = container.repository()

    /** Shares [listId] (the first time) and makes a new invite for it. */
    suspend fun invite(listId: String): Result<SharedListInvite> = sync.sharing("Couldn't share the list.") { transport, credentials ->
        val repository = repository()
        val name = repository.shareableListName(listId)
        var shared = repository.sharedList(listId)
        if (shared == null) {
            shared = repository.recordSharedSpace(transport.request("POST", syncSpacesPath(), syncShareListBody(listId, name)))
        }
        val invite = syncDecodeInvite(transport.request("POST", syncSpaceInvitesPath(shared.spaceId)))
        sync.syncNow()
        SharedListInvite(name, syncInviteLink(credentials.serverURL, invite.code), syncInviteCodeGrouped(invite.code))
    }

    /** What an invite link or code is for, before joining. */
    suspend fun preview(text: String): Result<SyncInvitePreview> {
        val code = syncInviteCode(text) ?: return Result.failure(IllegalArgumentException("That isn't an invite link or code."))
        return sync.sharing("Couldn't read the invite.") { transport, _ ->
            syncDecodeInvitePreview(transport.request("GET", syncInvitePath(code)))
        }
    }

    /** Joins the list an invite is for. Its tasks arrive with the next sync. */
    suspend fun join(text: String): Result<SharedList> {
        val code = syncInviteCode(text) ?: return Result.failure(IllegalArgumentException("That isn't an invite link or code."))
        return sync.sharing("Couldn't join the list.") { transport, _ ->
            val joined = repository().recordSharedSpace(transport.request("POST", syncAcceptInvitePath(code)))
            sync.syncNow()
            joined
        }
    }

    /** Leaves a list someone else shared: it goes from this device and the account's others. */
    suspend fun leave(listId: String): Result<Unit> = sync.sharing("Couldn't leave the list.") { transport, _ ->
        val shared = repository().sharedList(listId) ?: return@sharing
        val you = shared.members.firstOrNull { it.you }?.accountId ?: error("This account isn't in the list.")
        transport.request("DELETE", syncSpaceMemberPath(shared.spaceId, you))
        repository().forgetSharedSpace(shared.spaceId, stoppedSharing = false)
        sync.syncNow()
    }

    /** Stops sharing a list you own: it stays yours and goes from everyone else's devices. */
    suspend fun stopSharing(listId: String): Result<Unit> = sync.sharing("Couldn't stop sharing the list.") { transport, _ ->
        val shared = repository().sharedList(listId) ?: return@sharing
        transport.request("DELETE", syncSpacePath(shared.spaceId))
        repository().forgetSharedSpace(shared.spaceId, stoppedSharing = true)
        sync.syncNow()
    }

    /** Takes someone out of a list you own. */
    suspend fun remove(listId: String, accountId: String): Result<Unit> = sync.sharing("Couldn't remove them.") { transport, _ ->
        val shared = repository().sharedList(listId) ?: return@sharing
        transport.request("DELETE", syncSpaceMemberPath(shared.spaceId, accountId))
        sync.syncNow()
    }
}
