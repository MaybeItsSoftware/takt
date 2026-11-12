package uk.co.maybeitsadam.takt.app

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.onStart
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import uk.co.maybeitsadam.takt.core.theme.BuiltInThemeSpecifications
import uk.co.maybeitsadam.takt.core.theme.ThemeFileLibrary
import uk.co.maybeitsadam.takt.core.theme.ThemeFileLoader
import uk.co.maybeitsadam.takt.core.theme.ThemeFileOutcome
import uk.co.maybeitsadam.takt.core.theme.ThemePlatform
import uk.co.maybeitsadam.takt.core.theme.ThemeRows
import uk.co.maybeitsadam.takt.core.theme.ThemeSpecification
import uk.co.maybeitsadam.takt.core.theme.ThemeTypographyOverride
import uk.co.maybeitsadam.takt.data.workspace.WorkspacePreferenceKey
import uk.co.maybeitsadam.takt.data.workspace.WorkspaceRepository
import uk.co.maybeitsadam.takt.data.workspace.deleteTheme
import uk.co.maybeitsadam.takt.data.workspace.observePreferences
import uk.co.maybeitsadam.takt.data.workspace.observeThemes
import uk.co.maybeitsadam.takt.data.workspace.preferences
import uk.co.maybeitsadam.takt.data.workspace.setPreference
import uk.co.maybeitsadam.takt.data.workspace.themes
import uk.co.maybeitsadam.takt.data.workspace.upsertTheme
import uk.co.maybeitsadam.takt.ui.theme.ThemeMode

/** One user theme: its row in the synced `themes` table, and the file name it loads under. */
data class UserThemeFile(val id: String, val name: String, val json: String)

/** A theme and appearance choice: `theme.selected` and `theme.appearance`. */
data class ThemeSelection(
    val selected: String = BuiltInThemeSpecifications.DEFAULT_IDENTIFIER,
    val appearance: ThemeMode = ThemeMode.SYSTEM,
)

/** Everything the theme settings and the app's root need, resolved for Android. */
data class ThemeLibraryState(
    val files: List<UserThemeFile> = emptyList(),
    val library: ThemeFileLibrary = ThemeFileLibrary.EMPTY,
    /** The choice every device shares, from the synced `preferences` table. */
    val shared: ThemeSelection = ThemeSelection(),
    /** This device's own choice, used when [useDeviceChoice] is on. Never synced. */
    val device: ThemeSelection = ThemeSelection(),
    val useDeviceChoice: Boolean = false,
    /**
     * The reader's faces and text size, laid over whichever theme is chosen
     * so a theme switch keeps them. Kept per device, like the Mac's.
     */
    val typography: ThemeTypographyOverride = ThemeTypographyOverride(),
) {
    /** The built-ins, Zed One (the default, as on the Mac) first. */
    val builtIns: List<ThemeSpecification>
        get() = BuiltInThemeSpecifications.all(ThemePlatform.ANDROID)
            .sortedBy { it.identifier != BuiltInThemeSpecifications.ONE_IDENTIFIER }

    /** Built-ins first, then every user theme that loaded. */
    val available: List<ThemeSpecification> get() = builtIns + library.themes

    val selection: ThemeSelection get() = if (useDeviceChoice) device else shared

    /** The theme chosen, as it ships. A choice this device does not know (yet) is the default until it does. */
    val themeSpecification: ThemeSpecification
        get() = available.firstOrNull { it.identifier == selection.selected }
            ?: BuiltInThemeSpecifications.defaultTheme(ThemePlatform.ANDROID)

    /** The theme in force: the chosen one with [typography] laid over it. */
    val specification: ThemeSpecification get() = typography.applied(themeSpecification)

    val mode: ThemeMode get() = selection.appearance

    companion object {
        /** The state for these table rows and DataStore values. */
        fun of(
            rows: Map<String, String>,
            preferences: Map<String, String?>,
            device: ThemeSelection,
            useDeviceChoice: Boolean,
            typography: ThemeTypographyOverride = ThemeTypographyOverride(),
        ): ThemeLibraryState {
            val files = rows.map { (id, json) -> UserThemeFile(id, ThemeRows.fileName(id, json), json) }.sortedBy { it.name }
            return ThemeLibraryState(
                files = files,
                library = ThemeFileLoader.load(ThemeRows.sources(rows), ThemePlatform.ANDROID),
                shared = ThemeSelection(
                    preferences[WorkspacePreferenceKey.THEME_SELECTED] ?: BuiltInThemeSpecifications.DEFAULT_IDENTIFIER,
                    ThemeMode.of(preferences[WorkspacePreferenceKey.THEME_APPEARANCE]),
                ),
                device = device,
                useDeviceChoice = useDeviceChoice,
                typography = typography,
            )
        }
    }
}

/**
 * Themes and the choice of theme. User themes are rows in the synced
 * `themes` table and the shared choice is `theme.selected` and
 * `theme.appearance` in `preferences` (docs/themes.md, "The chosen theme
 * follows you"). Only the device opt-out and this device's own choice stay in
 * DataStore.
 */
class ThemeStore(private val settings: SettingsStore, private val container: AppContainer) {
    private suspend fun repository(): WorkspaceRepository = container.repository()

    val state: Flow<ThemeLibraryState> = container.withSession { session ->
        val repo = session.repository
        combine(repo.observeThemes(), repo.observePreferences(), settings.data) { rows, preferences, prefs ->
            ThemeLibraryState.of(
                rows.associate { it.id to it.json },
                preferences,
                ThemeSelection(
                    prefs[DEVICE_SELECTED] ?: BuiltInThemeSpecifications.DEFAULT_IDENTIFIER,
                    ThemeMode.of(prefs[DEVICE_APPEARANCE]),
                ),
                prefs[USE_DEVICE] == "true",
                ThemeTypographyOverride.fromJson(prefs[TYPOGRAPHY]),
            )
        }.onStart { migrateLocal(repo) }
    }
        .distinctUntilChanged()
        .flowOn(Dispatchers.Default)

    /**
     * Adds a theme file as a row under the identifier it loads as, replacing
     * any row with that identifier, and returns what loading it gave.
     */
    suspend fun import(fileName: String, text: String): ThemeFileOutcome? {
        val repo = repository()
        val id = ThemeRows.identifier(fileName, text)
        repo.upsertTheme(id, text)
        val rows = repo.themes().associate { it.id to it.json }
        val name = ThemeRows.fileName(id, text)
        return ThemeFileLoader.load(ThemeRows.sources(rows), ThemePlatform.ANDROID).outcomes.firstOrNull { it.source == name }
    }

    suspend fun remove(id: String) {
        repository().deleteTheme(id)
    }

    /** Chooses a theme: for every device, or this one alone when it has opted out. */
    suspend fun select(identifier: String) {
        if (useDevice()) {
            settings.edit { it[DEVICE_SELECTED] = identifier }
        } else {
            repository().setPreference(WorkspacePreferenceKey.THEME_SELECTED, identifier)
        }
    }

    suspend fun setAppearance(mode: ThemeMode) {
        if (useDevice()) {
            settings.edit { it[DEVICE_APPEARANCE] = mode.raw }
        } else {
            repository().setPreference(WorkspacePreferenceKey.THEME_APPEARANCE, mode.raw)
        }
    }

    /** Opting out starts this device's choice from the shared one, so nothing changes until it is changed. */
    suspend fun setUseDeviceChoice(enabled: Boolean) {
        val shared = repository().preferences()
        settings.edit { prefs ->
            if (enabled && prefs[USE_DEVICE] != "true") {
                prefs[DEVICE_SELECTED] = shared[WorkspacePreferenceKey.THEME_SELECTED] ?: BuiltInThemeSpecifications.DEFAULT_IDENTIFIER
                prefs[DEVICE_APPEARANCE] = shared[WorkspacePreferenceKey.THEME_APPEARANCE] ?: ThemeMode.SYSTEM.raw
            }
            prefs[USE_DEVICE] = enabled.toString()
        }
    }

    /** Sets the faces and text size laid over every theme; an empty override is removed. */
    suspend fun setTypography(override: ThemeTypographyOverride) {
        settings.edit { prefs ->
            if (override.isEmpty) prefs.remove(TYPOGRAPHY) else prefs[TYPOGRAPHY] = override.toJson()
        }
    }

    private suspend fun useDevice(): Boolean = settings.data.first()[USE_DEVICE] == "true"

    /**
     * Before themes synced, they and the choice lived in DataStore (and,
     * earlier still, as one imported JSON). Their files become rows; the
     * choice becomes the shared one only if no device has set it yet.
     */
    private suspend fun migrateLocal(repo: WorkspaceRepository) {
        var plan: LocalThemeMigration? = null
        settings.edit { prefs ->
            val values = MIGRATED_KEYS.associate { it.name to prefs[it] }
            if (values.values.all { it == null }) return@edit
            plan = LocalThemeMigration.of(values)
        }
        val migration = plan ?: return
        for ((id, json) in migration.rows) repo.upsertTheme(id, json)
        val shared = repo.preferences()
        if (!shared.containsKey(WorkspacePreferenceKey.THEME_SELECTED)) {
            migration.selected?.let { repo.setPreference(WorkspacePreferenceKey.THEME_SELECTED, it) }
            migration.appearance?.let { repo.setPreference(WorkspacePreferenceKey.THEME_APPEARANCE, it) }
        }
        settings.edit { prefs -> MIGRATED_KEYS.forEach { prefs.remove(it) } }
    }

    companion object {
        val USE_DEVICE = SettingsStore.key("theme.device.enabled")
        val DEVICE_SELECTED = SettingsStore.key("theme.device.selected")
        val DEVICE_APPEARANCE = SettingsStore.key("theme.device.appearance")

        /** The Mac's `themeTypographyOverride`, as the same JSON. */
        val TYPOGRAPHY = SettingsStore.key("theme.typography")

        // Before the synced tables: the file list and the choice in DataStore,
        // and before that one imported JSON under two keys.
        private val LOCAL_FILES = SettingsStore.key(LocalThemeMigration.FILES)
        private val LOCAL_SELECTED = SettingsStore.key(LocalThemeMigration.SELECTED)
        private val LOCAL_APPEARANCE = SettingsStore.key(LocalThemeMigration.APPEARANCE)
        private val LEGACY_MODE = SettingsStore.key(LocalThemeMigration.LEGACY_MODE)
        private val LEGACY_ACTIVE = SettingsStore.key(LocalThemeMigration.LEGACY_ACTIVE)
        private val LEGACY_LIBRARY = SettingsStore.key(LocalThemeMigration.LEGACY_LIBRARY)
        private val MIGRATED_KEYS =
            listOf(LOCAL_FILES, LOCAL_SELECTED, LOCAL_APPEARANCE, LEGACY_MODE, LEGACY_ACTIVE, LEGACY_LIBRARY)
    }
}

/** What the DataStore-era theme keys become in the synced tables. Pure, so it is tested on its own. */
data class LocalThemeMigration(
    val rows: Map<String, String>,
    val selected: String?,
    val appearance: String?,
) {
    @Serializable
    private data class LocalFile(val name: String, val json: String)

    companion object {
        const val FILES = "theme.files"
        const val SELECTED = "theme.selected"
        const val APPEARANCE = "theme.appearance"
        const val LEGACY_MODE = "themeMode"
        const val LEGACY_ACTIVE = "themeJSON"
        const val LEGACY_LIBRARY = "importedThemeLibrary"

        private const val LEGACY_FILE_NAME = "imported.json"
        private val json = Json { ignoreUnknownKeys = true }

        fun of(values: Map<String, String?>): LocalThemeMigration {
            val rows = LinkedHashMap<String, String>()
            values[FILES]?.let { text ->
                runCatching { json.decodeFromString(ListSerializer(LocalFile.serializer()), text) }.getOrNull()
                    ?.forEach { rows[ThemeRows.identifier(it.name, it.json)] = it.json }
            }
            val legacyActive = values[LEGACY_ACTIVE]
            val activeIsChalkDark = legacyActive?.contains(BuiltInThemeSpecifications.CHALK_DARK_IDENTIFIER) == true
            val legacyImported = values[LEGACY_LIBRARY] ?: legacyActive?.takeUnless { activeIsChalkDark }
            legacyImported?.let { rows.putIfAbsent(ThemeRows.identifier(LEGACY_FILE_NAME, it), it) }

            val selected = values[SELECTED] ?: legacyActive?.let {
                if (activeIsChalkDark) BuiltInThemeSpecifications.CHALK_DARK_IDENTIFIER else ThemeRows.identifier(LEGACY_FILE_NAME, it)
            }
            return LocalThemeMigration(rows, selected, values[APPEARANCE] ?: values[LEGACY_MODE])
        }
    }
}
