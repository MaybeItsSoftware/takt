package uk.co.maybeitsadam.takt.ui.lists

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.data.workspace.sharedList
import uk.co.maybeitsadam.takt.settings.SharedListInvite
import uk.co.maybeitsadam.takt.ui.SheetHandle
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.MonoText
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uniffi.takt_core.SharedList
import uniffi.takt_core.SharedListMember
import uniffi.takt_core.SyncInvitePreview
import uniffi.takt_core.syncInviteCode

/** What the sheet is asking the user to confirm. */
private sealed interface SharingConfirm {
    data object Unshare : SharingConfirm
    data class Remove(val member: SharedListMember) : SharingConfirm
}

/**
 * A list's sharing: who is in it, an invite to send, and leaving or
 * stopping sharing. For a list not yet shared, making the first invite
 * shares it.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun SharedListSheet(listId: String, listName: String, onDismiss: () -> Unit) {
    val container = LocalShell.current.container
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val colors = TaktTheme.colors
    var shared by remember { mutableStateOf<SharedList?>(null) }
    var invite by remember { mutableStateOf<SharedListInvite?>(null) }
    var working by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }
    var confirm by remember { mutableStateOf<SharingConfirm?>(null) }
    val signedIn by container.sync.credentials.collectAsStateWithLifecycle()

    suspend fun reload() {
        shared = container.repository().sharedList(listId)
    }
    LaunchedEffect(listId) { reload() }

    fun run(block: suspend () -> Result<*>) {
        working = true
        problem = null
        scope.launch {
            block().onFailure { problem = it.message }
            reload()
            working = false
        }
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = TaktTheme.colors.altRow,
        contentColor = colors.ink,
        tonalElevation = 0.dp,
        shape = Metrics.sheetTop,
        dragHandle = { SheetHandle() },
        modifier = Modifier.testTag("shared_list_sheet"),
    ) {
        Column(
            Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).navigationBarsPadding().imePadding().padding(horizontal = Metrics.lg),
            verticalArrangement = Arrangement.spacedBy(Metrics.md),
        ) {
            val current = shared
            Column(verticalArrangement = Arrangement.spacedBy(Metrics.xxs)) {
                Text(listName, style = TaktTheme.type.title, color = colors.ink)
                Text(
                    when {
                        current == null -> "Share this list with other Takt accounts. Everyone in it can add, edit and complete its tasks; where it sits in your lists, your board columns and your conditions stay yours."
                        current.role == "owner" -> "You share this list. Everyone in it can add, edit and complete its tasks."
                        else -> "Shared with you. Its tasks are everyone's to edit; leaving takes it off your devices."
                    },
                    style = TaktTheme.type.small, color = colors.mutedText,
                )
            }
            if (signedIn == null) {
                Text("Sign in to sync first, in Settings: shared lists travel through it.", style = TaktTheme.type.small, color = colors.danger)
            }
            if (current != null) {
                Column {
                    MonoText("PEOPLE", color = colors.mutedText)
                    for (member in current.members) {
                        Row(Modifier.fillMaxWidth().heightIn(min = Metrics.touchTarget), verticalAlignment = Alignment.CenterVertically) {
                            Column(Modifier.weight(1f)) {
                                Text(member.email ?: "Someone", style = TaktTheme.type.body, color = colors.ink)
                                Text(
                                    listOfNotNull(if (member.role == "owner") "Owner" else null, if (member.you) "You" else null).joinToString(" · ").ifEmpty { "Member" },
                                    style = TaktTheme.type.small, color = colors.mutedText,
                                )
                            }
                            if (current.role == "owner" && !member.you) {
                                PButton("Remove", destructive = true, enabled = !working) { confirm = SharingConfirm.Remove(member) }
                            }
                        }
                        Hairline()
                    }
                }
            }
            if (current == null || current.role == "owner") {
                val made = invite
                if (made != null) {
                    Column(verticalArrangement = Arrangement.spacedBy(Metrics.xs)) {
                        MonoText("INVITE", color = colors.mutedText)
                        Text(made.link, style = TaktTheme.type.small, color = colors.ink)
                        Text("Or the code ${made.code}. It works once, for seven days.", style = TaktTheme.type.small, color = colors.mutedText)
                        Row(horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
                            PButton("Send…", primary = true) { shareInvite(context, made) }
                            PButton("Copy") { copyInvite(context, made) }
                        }
                    }
                }
                PButton(
                    if (current == null) "Share and make an invite" else "Make another invite",
                    Modifier.fillMaxWidth().testTag("shared_list_invite"),
                    primary = made == null, enabled = !working && signedIn != null,
                ) {
                    run { container.sharedLists.invite(listId).onSuccess { invite = it } }
                }
            }
            if (current != null) {
                PButton(
                    if (current.role == "owner") "Stop sharing" else "Leave the list",
                    Modifier.fillMaxWidth(), destructive = true, enabled = !working,
                ) { confirm = SharingConfirm.Unshare }
            }
            problem?.let { Text(it, style = TaktTheme.type.small, color = colors.danger) }
            Box(Modifier.heightIn(min = Metrics.lg))
        }
    }

    when (val asking = confirm) {
        null -> Unit
        SharingConfirm.Unshare -> {
            val owner = shared?.role == "owner"
            ConfirmDialog(
                if (owner) "Stop sharing $listName?" else "Leave $listName?",
                if (owner) "It stays in your lists and goes from everyone else's devices." else "It goes from your devices. The others keep it.",
                confirm = if (owner) "Stop sharing" else "Leave",
                onDismiss = { confirm = null },
            ) {
                run {
                    val result = if (owner) container.sharedLists.stopSharing(listId) else container.sharedLists.leave(listId)
                    result.onSuccess {
                        container.undo.say(if (owner) "$listName is yours alone again." else "You left $listName.")
                        onDismiss()
                    }
                }
            }
        }
        is SharingConfirm.Remove -> ConfirmDialog(
            "Remove ${asking.member.email ?: "them"}?",
            "$listName goes from their devices. You can invite them again.",
            confirm = "Remove",
            onDismiss = { confirm = null },
        ) { run { container.sharedLists.remove(listId, asking.member.accountId) } }
    }
}

/** Joining a list from an invite link or code: what it is, then Join. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun JoinListSheet(initial: String, onDismiss: () -> Unit) {
    val container = LocalShell.current.container
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val colors = TaktTheme.colors
    var text by remember { mutableStateOf(initial.ifBlank { clipboardInvite(context).orEmpty() }) }
    var preview by remember { mutableStateOf<SyncInvitePreview?>(null) }
    var working by remember { mutableStateOf(false) }
    var problem by remember { mutableStateOf<String?>(null) }

    LaunchedEffect(text) {
        preview = null
        problem = null
        if (syncInviteCode(text) != null) {
            container.sharedLists.preview(text).onSuccess { preview = it }.onFailure { problem = it.message }
        }
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = TaktTheme.colors.altRow,
        contentColor = colors.ink,
        tonalElevation = 0.dp,
        shape = Metrics.sheetTop,
        dragHandle = { SheetHandle() },
        modifier = Modifier.testTag("join_list_sheet"),
    ) {
        Column(
            Modifier.fillMaxWidth().navigationBarsPadding().imePadding().padding(horizontal = Metrics.lg),
            verticalArrangement = Arrangement.spacedBy(Metrics.md),
        ) {
            Text("Join a shared list", style = TaktTheme.type.title, color = colors.ink)
            PTextField(text, { text = it }, "Invite link or code", Modifier.fillMaxWidth().testTag("join_list_field"))
            val found = preview
            if (found != null) {
                Text(
                    if (found.joined) {
                        "You're already in ${found.name}."
                    } else {
                        "${found.name}, shared by ${found.ownerEmail ?: "someone"}. Its tasks arrive with the next sync."
                    },
                    style = TaktTheme.type.small, color = colors.mutedText,
                )
            }
            problem?.let { Text(it, style = TaktTheme.type.small, color = colors.danger) }
            PButton(
                "Join", Modifier.fillMaxWidth().testTag("join_list_join"), primary = true,
                enabled = !working && found != null && !found.joined,
            ) {
                working = true
                scope.launch {
                    container.sharedLists.join(text)
                        .onSuccess {
                            container.undo.say("Joined ${it.name}.")
                            onDismiss()
                        }
                        .onFailure { problem = it.message }
                    working = false
                }
            }
            Box(Modifier.heightIn(min = Metrics.lg))
        }
    }
}

private fun shareInvite(context: Context, invite: SharedListInvite) {
    val send = Intent(Intent.ACTION_SEND).apply {
        type = "text/plain"
        putExtra(Intent.EXTRA_SUBJECT, "Join ${invite.listName} in Takt")
        putExtra(Intent.EXTRA_TEXT, "Join ${invite.listName} in Takt: ${invite.link}\n\nOr enter the code ${invite.code}.")
    }
    context.startActivity(Intent.createChooser(send, "Send the invite").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
}

private fun copyInvite(context: Context, invite: SharedListInvite) {
    val clipboard = context.getSystemService(ClipboardManager::class.java) ?: return
    clipboard.setPrimaryClip(ClipData.newPlainText("Takt invite", invite.link))
}

/** The clipboard, when it holds an invite. */
private fun clipboardInvite(context: Context): String? {
    val clipboard = context.getSystemService(ClipboardManager::class.java) ?: return null
    val text = clipboard.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(context)?.toString() ?: return null
    return text.takeIf { syncInviteCode(it) != null }
}
