package uk.co.maybeitsadam.takt.ui.inspector

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.IntentSenderRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import com.google.android.gms.common.api.ApiException
import com.google.android.gms.common.api.CommonStatusCodes
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.data.drive.GoogleDriveFiles
import uk.co.maybeitsadam.takt.data.workspace.addLink
import uk.co.maybeitsadam.takt.settings.GoogleDriveAuth
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell

/**
 * "Pick from Google Drive" under the inspector's links (docs/links.md
 * "Phones"): Google's Picker through the Authorization API, then each file
 * read from Drive for its name and type and linked as an "Add Link" step.
 * [beforeWriting] saves the draft first, so a pending edit to the address
 * list isn't lost under the new links.
 */
@Composable
internal fun DriveLinkButton(taskId: String, beforeWriting: suspend () -> Unit) {
    val context = LocalContext.current
    val undo = LocalShell.current.container.undo
    val scope = rememberCoroutineScope()
    var picking by remember { mutableStateOf(false) }
    var note by remember { mutableStateOf<String?>(null) }

    val picker = rememberLauncherForActivityResult(ActivityResultContracts.StartIntentSenderForResult()) { result ->
        val picked = GoogleDriveAuth.pickedFrom(context, result.data)
        if (picked == null || picked.fileIds.isEmpty()) {
            picking = false
            note = "Nothing linked from Drive."
            return@rememberLauncherForActivityResult
        }
        scope.launch {
            try {
                val files = GoogleDriveFiles.describe(picked.accessToken, picked.fileIds)
                beforeWriting()
                undo.performNow { repo ->
                    for (file in files) repo.addLink(taskId, file.url, file.name, file.mimeType)
                }
                note = if (files.size == 1) "Linked from Drive." else "Linked ${files.size} files from Drive."
            } finally {
                picking = false
            }
        }
    }

    PButton(
        if (picking) "Choosing in Google Drive…" else "Pick from Google Drive",
        enabled = !picking,
        modifier = Modifier.testTag("inspector_links_drive"),
    ) {
        picking = true
        note = null
        scope.launch {
            try {
                val intent = GoogleDriveAuth.picker(context)
                if (intent == null) {
                    picking = false
                    note = "Google answered without showing the Drive picker."
                } else {
                    picker.launch(IntentSenderRequest.Builder(intent).build())
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Exception) {
                picking = false
                note = notSetUp(error) ?: (error.message ?: "Couldn't open Google Drive.")
            }
        }
    }
    note?.let { Hint(it) }
}

/** This build's package and signing key aren't registered with Google: Drive linking isn't set up. */
private fun notSetUp(error: Exception): String? =
    (error as? ApiException)?.takeIf { it.statusCode == CommonStatusCodes.DEVELOPER_ERROR }?.let {
        "Google Drive linking isn't set up for this build: its Android OAuth client is missing."
    }
