package uk.co.maybeitsadam.takt.data.drive

import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request

/** A file picked in Google's Picker, as a task links it. */
data class PickedDriveFile(val url: String, val name: String, val mimeType: String?)

/**
 * The files Google's Picker handed back, described for linking (docs/links.md
 * "Phones"). Takt asks for `drive.file`, which reaches only what was picked, so
 * the ids come from the Picker and each is read here with Drive's `files.get`
 * for its name, type and the `https://` address the link is stored by.
 */
object GoogleDriveFiles {
    private const val FOLDER = "application/vnd.google-apps.folder"
    private val json = Json { ignoreUnknownKeys = true }
    private val client by lazy { OkHttpClient.Builder().callTimeout(30, TimeUnit.SECONDS).build() }

    /** A Drive id as Google makes them; anything else is not asked about. */
    fun isId(id: String): Boolean = id.isNotEmpty() && id.all { it in 'a'..'z' || it in 'A'..'Z' || it in '0'..'9' || it == '-' || it == '_' }

    /** The comma-separated `picked_file_ids` the Picker answers with. */
    fun pickedIds(value: String?): List<String> =
        value.orEmpty().split(',').map { it.trim() }.filter { it.isNotEmpty() }

    /** Drive's own viewing address when it is a Drive or Docs one, else the one made from the id. */
    fun linkUrl(id: String, mimeType: String?, webViewLink: String?): String? {
        val link = webViewLink?.trim()
        if (link != null && (link.startsWith("https://docs.google.com/") || link.startsWith("https://drive.google.com/"))) {
            return link
        }
        if (!isId(id)) return null
        return if (mimeType == FOLDER) "https://drive.google.com/drive/folders/$id" else "https://drive.google.com/file/d/$id/view"
    }

    /** One `files.get` answer as a link; null when it isn't one. */
    fun read(id: String, body: String): PickedDriveFile? {
        val file = runCatching { json.parseToJsonElement(body).jsonObject }.getOrNull() ?: return null
        val mimeType = file.text("mimeType")
        val url = linkUrl(id, mimeType, file.text("webViewLink")) ?: return null
        val name = file.text("name")?.trim().orEmpty().ifEmpty { "Untitled" }
        return PickedDriveFile(url, name, mimeType)
    }

    /** The picked [ids] described with [accessToken]; a file Drive won't describe is still linked by its id. */
    suspend fun describe(accessToken: String, ids: List<String>): List<PickedDriveFile> = withContext(Dispatchers.IO) {
        ids.filter(::isId).mapNotNull { id ->
            val url = "https://www.googleapis.com/drive/v3/files/$id".toHttpUrl().newBuilder()
                .addQueryParameter("fields", "id,name,mimeType,webViewLink")
                .addQueryParameter("supportsAllDrives", "true")
                .build()
            val request = Request.Builder().url(url).header("Authorization", "Bearer $accessToken").build()
            val described = try {
                client.newCall(request).execute().use { response ->
                    if (response.isSuccessful) read(id, response.body.string()) else null
                }
            } catch (_: IOException) {
                null
            }
            described ?: linkUrl(id, null, null)?.let { PickedDriveFile(it, "Google Drive file", null) }
        }
    }

    private fun JsonObject.text(key: String): String? = this[key]?.jsonPrimitive?.contentOrNull
}
