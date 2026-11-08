package uk.co.maybeitsadam.takt.settings

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Bundle
import com.google.android.gms.auth.api.identity.AuthorizationRequest
import com.google.android.gms.auth.api.identity.Identity
import com.google.android.gms.common.api.ApiException
import com.google.android.gms.common.api.Scope
import com.google.android.gms.tasks.Task
import kotlinx.coroutines.suspendCancellableCoroutine
import uk.co.maybeitsadam.takt.data.drive.GoogleDriveFiles

/**
 * Google's Picker for Android (docs/links.md "Phones"): the Authorization API
 * asked for `drive.file` alone with the `PICKER_OAUTH_TRIGGER` resource
 * parameter, against the package's Android OAuth client as Calendar is, so no
 * client id or API key is needed. The consent screen shows the person their
 * Drive; the files picked come back as `picked_file_ids` in the result's token
 * response parameters, with an access token that reaches just those files.
 */
object GoogleDriveAuth {
    private val scope = Scope("https://www.googleapis.com/auth/drive.file")

    /** What the Picker answered: a token for the files picked, and their ids. */
    data class Picked(val accessToken: String, val fileIds: List<String>)

    /** The Picker to launch from an activity, then read with [pickedFrom]; null when Google answered without it. */
    suspend fun picker(context: Context): PendingIntent? {
        val request = AuthorizationRequest.builder()
            .setRequestedScopes(listOf(scope))
            // A token for drive.file alone, as the Picker requires.
            .setOptOutIncludingGrantedScopes(true)
            .setPrompt(AuthorizationRequest.Prompt.CONSENT)
            .addResourceParameter(AuthorizationRequest.ResourceParameter.PICKER_OAUTH_TRIGGER, "true")
            .addResourceParameter(AuthorizationRequest.ResourceParameter.PICKER_ALLOW_MULTIPLE, "true")
            .addResourceParameter(AuthorizationRequest.ResourceParameter.PICKER_ALLOW_FOLDER_SELECTION, "true")
            .build()
        val result = Identity.getAuthorizationClient(context).authorize(request).await()
        return result.pendingIntent?.takeIf { result.hasResolution() }
    }

    /** The files picked, from the Picker's answer; null when the person backed out. */
    fun pickedFrom(context: Context, data: Intent?): Picked? = try {
        val result = Identity.getAuthorizationClient(context).getAuthorizationResultFromIntent(data)
        val token = result.accessToken
        if (token == null) null
        else Picked(token, GoogleDriveFiles.pickedIds(pickedIds(result.tokenResponseParams)))
    } catch (_: ApiException) {
        null
    }

    /** `picked_file_ids`, comma-separated, whether Google sends one string or a list. */
    private fun pickedIds(params: Bundle?): String? {
        if (params == null) return null
        params.getString(PICKED_FILE_IDS)?.let { return it }
        return params.getStringArrayList(PICKED_FILE_IDS)?.joinToString(",")
    }

    private const val PICKED_FILE_IDS = "picked_file_ids"

    private suspend fun <T> Task<T>.await(): T = suspendCancellableCoroutine { continuation ->
        addOnSuccessListener { continuation.resumeWith(Result.success(it)) }
        addOnFailureListener { continuation.resumeWith(Result.failure(it)) }
        addOnCanceledListener { continuation.cancel() }
    }
}
