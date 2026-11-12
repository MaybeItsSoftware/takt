package uk.co.maybeitsadam.takt.app

import uk.co.maybeitsadam.takt.ui.components.CardStyle
import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.MutablePreferences
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.core.stringSetPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map

private val Context.settingsDataStore: DataStore<Preferences> by preferencesDataStore(name = "settings")

/** How finishing something is marked; the Mac's celebration plugins by the same names. */
enum class CelebrationStyle(val raw: String, val title: String, val detail: String) {
    STRIKE("strike", "Strike", "A rule drawn through the finished row."),
    SPARK("spark", "Spark", "A brief spark where the row was."),
    FOLD("fold", "Fold", "The row folds away."),
    NONE("none", "None", "Nothing but the tick.");

    companion object {
        fun key(name: String): Preferences.Key<String> = stringPreferencesKey(name)

        fun of(raw: String?): CelebrationStyle = entries.firstOrNull { it.raw == raw } ?: STRIKE
    }
}

/**
 * App preferences, in a DataStore. Typed flows for what the shell reads, and a
 * small string/string-set API so features can keep their own keys here without
 * editing this file.
 */
class SettingsStore(context: Context) {
    private val store = context.applicationContext.settingsDataStore

    /** Every preference, for stores that read several keys together (see [ThemeStore]). */
    val data: Flow<Preferences> get() = store.data

    val celebrationStyle: Flow<CelebrationStyle> = string(CELEBRATION).map { CelebrationStyle.of(it) }

    suspend fun setCelebrationStyle(style: CelebrationStyle) = putString(CELEBRATION, style.raw)

    /**
     * The Mac's "Focus points" preference (`scoresEachFocusBlock`): on, the end
     * of a block asks how it went and points are shown; off, a block closes at
     * ×1.0 with no question and no points are shown anywhere. On by default.
     */
    val scoresEachFocusBlock: Flow<Boolean> = string(SCORES_EACH_FOCUS_BLOCK).map { it != "false" }

    suspend fun setScoresEachFocusBlock(enabled: Boolean) = putString(SCORES_EACH_FOCUS_BLOCK, enabled.toString())

    /** How rows are framed on this device ([CardStyle]); every task a card unless chosen otherwise. */
    val cardStyle: Flow<CardStyle> = string(CARD_STYLE).map { CardStyle.of(it) }

    suspend fun setCardStyle(style: CardStyle) = putString(CARD_STYLE, style.raw)

    fun string(key: String): Flow<String?> = store.data.map { it[stringPreferencesKey(key)] }.distinctUntilChanged()

    suspend fun putString(key: String, value: String?) {
        store.edit { prefs ->
            if (value == null) prefs.remove(stringPreferencesKey(key)) else prefs[stringPreferencesKey(key)] = value
        }
    }

    fun stringSet(key: String): Flow<Set<String>> =
        store.data.map { it[stringSetPreferencesKey(key)] ?: emptySet() }.distinctUntilChanged()

    /** Several keys in one transaction. */
    suspend fun edit(block: (MutablePreferences) -> Unit) {
        store.edit { block(it) }
    }

    suspend fun putStringSet(key: String, value: Set<String>) {
        store.edit { it[stringSetPreferencesKey(key)] = value }
    }

    companion object {
        fun key(name: String): Preferences.Key<String> = stringPreferencesKey(name)

        const val CELEBRATION = "celebrationStyle"
        const val CARD_STYLE = "cardStyle"
        const val SCORES_EACH_FOCUS_BLOCK = "scoresEachFocusBlock"
    }
}
