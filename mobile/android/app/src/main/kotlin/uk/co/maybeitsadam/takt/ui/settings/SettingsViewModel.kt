package uk.co.maybeitsadam.takt.ui.settings

import uk.co.maybeitsadam.takt.ui.components.CardStyle
import android.net.Uri
import android.provider.OpenableColumns
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uk.co.maybeitsadam.takt.app.AppContainer
import uk.co.maybeitsadam.takt.app.CelebrationStyle
import uk.co.maybeitsadam.takt.settings.SyncUiState
import uk.co.maybeitsadam.takt.app.ThemeLibraryState
import uk.co.maybeitsadam.takt.core.WorkspaceExportFormat
import uk.co.maybeitsadam.takt.core.theme.ThemeTypographyOverride
import uk.co.maybeitsadam.takt.data.workspace.exportDocument
import uk.co.maybeitsadam.takt.ui.theme.ThemeMode
import uniffi.takt_core.coreVersion

/** `Synced just now`, `Synced at 14:05`, `Couldn't sync: …`. */
object SyncStatusText {
    private val clock = DateTimeFormatter.ofPattern("HH:mm", Locale.UK)
    private val dayClock = DateTimeFormatter.ofPattern("d MMM HH:mm", Locale.UK)

    fun describe(state: SyncUiState, now: Instant = Instant.now(), zone: ZoneId = ZoneId.systemDefault()): String = when (state) {
        SyncUiState.Unpaired -> "Not signed in"
        SyncUiState.SessionExpired -> "Signed out — sign in again"
        SyncUiState.Syncing -> "Syncing…"
        is SyncUiState.Failed -> "Couldn't sync: ${state.message}"
        SyncUiState.NeedsSubscription -> "Paused — needs Takt Sync"
        is SyncUiState.Idle -> {
            val last = state.lastSyncedAt
            when {
                last == null -> "Signed in"
                now.epochSecond - last.epochSecond < 60 -> "Synced just now"
                last.atZone(zone).toLocalDate() == now.atZone(zone).toLocalDate() -> "Synced at ${clock.format(last.atZone(zone))}"
                else -> "Synced ${dayClock.format(last.atZone(zone))}"
            }
        }
    }
}

class SettingsViewModel(private val container: AppContainer) : ViewModel() {
    private val settings = container.settings

    /** Every theme, what loading each file reported, and the choice in force. */
    val themes: StateFlow<ThemeLibraryState> =
        container.themes.state.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), ThemeLibraryState())

    val celebration: StateFlow<CelebrationStyle> =
        settings.celebrationStyle.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), CelebrationStyle.STRIKE)

    private val _themeError = MutableStateFlow<String?>(null)
    val themeError: StateFlow<String?> = _themeError

    fun setMode(mode: ThemeMode) = viewModelScope.launch { container.themes.setAppearance(mode) }

    fun selectTheme(identifier: String) = viewModelScope.launch { container.themes.select(identifier) }

    fun setUseDeviceTheme(enabled: Boolean) = viewModelScope.launch { container.themes.setUseDeviceChoice(enabled) }

    fun setTypography(override: ThemeTypographyOverride) = viewModelScope.launch { container.themes.setTypography(override) }

    fun setCelebration(style: CelebrationStyle) = viewModelScope.launch { settings.setCelebrationStyle(style) }

    val cardStyle: StateFlow<CardStyle> =
        settings.cardStyle.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), CardStyle.DEFAULT)

    fun setCardStyle(style: CardStyle) = viewModelScope.launch { settings.setCardStyle(style) }

    val scoresEachFocusBlock: StateFlow<Boolean> = container.scoresEachFocusBlock

    fun setScoresEachFocusBlock(enabled: Boolean) = viewModelScope.launch { settings.setScoresEachFocusBlock(enabled) }

    /**
     * Imports a theme file. It is kept whatever loading it reports, so the
     * issues show under it and a fix is one re-import; it is switched to only
     * when it loaded.
     */
    fun import(uri: Uri) = viewModelScope.launch {
        _themeError.value = null
        val resolver = container.context.contentResolver
        val read = withContext(Dispatchers.IO) {
            runCatching {
                val name = resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                    if (cursor.moveToFirst()) cursor.getString(0) else null
                }
                val text = resolver.openInputStream(uri)?.use { it.readBytes().toString(Charsets.UTF_8) }
                text?.let { (name ?: uri.lastPathSegment ?: "theme.json") to it }
            }.getOrNull()
        }
        if (read == null) {
            _themeError.value = "That file could not be read."
            return@launch
        }
        val outcome = container.themes.import(read.first, read.second)
        val loaded = outcome?.specification
        if (loaded != null) {
            container.themes.select(loaded.identifier)
            container.undo.say("Using ${loaded.name}")
        } else {
            _themeError.value = "${read.first} was not loaded: ${outcome?.skippedReason ?: "it could not be read"}."
        }
    }

    /** The last export's outcome, as the Mac shows it under the buttons. */
    data class ExportStatus(val message: String, val isError: Boolean)

    private val _exportStatus = MutableStateFlow<ExportStatus?>(null)
    val exportStatus: StateFlow<ExportStatus?> = _exportStatus

    /**
     * Writes the whole workspace to [uri], a document the system's save
     * dialog just created, in the Mac's format so the files are
     * interchangeable.
     */
    fun export(format: WorkspaceExportFormat, uri: Uri) = viewModelScope.launch {
        _exportStatus.value = runCatching {
            val session = container.awaitSession()
            val document = session.repository.exportDocument(session.workspace.id, format)
            val resolver = container.context.contentResolver
            withContext(Dispatchers.IO) {
                checkNotNull(resolver.openOutputStream(uri, "wt")) { "the file could not be opened" }
                    .use { it.write(document.toByteArray(Charsets.UTF_8)) }
            }
            val name = withContext(Dispatchers.IO) {
                resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                    if (cursor.moveToFirst()) cursor.getString(0) else null
                }
            }
            ExportStatus("Saved ${name ?: format.suggestedFileName}.", isError = false)
        }.getOrElse { ExportStatus("Could not save: ${it.message ?: it.javaClass.simpleName}", isError = true) }
    }

    fun removeTheme(id: String) = viewModelScope.launch {
        _themeError.value = null
        container.themes.remove(id)
    }

    fun versionName(): String = runCatching {
        container.context.packageManager.getPackageInfo(container.context.packageName, 0).versionName
    }.getOrNull() ?: "?"

    /**
     * The Rust core's version, read across the UniFFI boundary. On screen it is
     * the proof a minified release still reaches the native library; a failure
     * shows as "unavailable" rather than taking Settings down with it.
     */
    fun coreVersionName(): String = runCatching { coreVersion() }.getOrNull() ?: "unavailable"

    /** The bundled licence files, by name. */
    suspend fun licences(): List<Pair<String, String>> = withContext(Dispatchers.IO) {
        val assets = container.context.assets
        (assets.list("licenses") ?: emptyArray()).sorted().map { file ->
            file.removeSuffix(".txt") to assets.open("licenses/$file").use { it.readBytes().toString(Charsets.UTF_8) }
        }
    }
}
