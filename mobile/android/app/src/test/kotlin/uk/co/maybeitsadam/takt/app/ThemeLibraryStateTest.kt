package uk.co.maybeitsadam.takt.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uk.co.maybeitsadam.takt.core.theme.ThemeTypographyOverride
import uk.co.maybeitsadam.takt.core.theme.BuiltInThemeSpecifications
import uk.co.maybeitsadam.takt.core.theme.ThemeAppearance
import uk.co.maybeitsadam.takt.core.theme.ThemePlatform
import uk.co.maybeitsadam.takt.data.workspace.WorkspacePreferenceKey
import uk.co.maybeitsadam.takt.ui.theme.ResolvedTheme
import uk.co.maybeitsadam.takt.ui.theme.ThemeMode

class ThemeLibraryStateTest {
    private val duskJson = """{ "name": "Dusk", "palette": { "light": { "primary": "#7a4de8" } } }"""
    private val rows = mapOf("user.dusk" to duskJson)

    private fun state(
        preferences: Map<String, String?>,
        device: ThemeSelection = ThemeSelection(),
        useDevice: Boolean = false,
    ) = ThemeLibraryState.of(rows, preferences, device, useDevice)

    @Test
    fun aChosenRowIsInForceWithAndroidStructure() {
        val s = state(mapOf(WorkspacePreferenceKey.THEME_SELECTED to "user.dusk"))
        assertEquals("Dusk", s.specification.name)
        assertEquals(15.0, s.specification.structure.typography.bodySize, 0.0)
        assertEquals(listOf(UserThemeFile("user.dusk", "dusk.json", duskJson)), s.files)
        assertEquals(
            listOf("native.theme.priority", "native.theme.chalk", "native.theme.chalk.dark", "native.theme.grape", "native.theme.one", "user.dusk"),
            s.available.map { it.identifier },
        )
        assertEquals(listOf("Takt", "Zed", "Zed Dark", "Grape", "Zed One"), s.builtIns.map { it.name })
    }

    @Test
    fun anUnknownOrClearedChoiceShowsTheDefaultUntilItArrives() {
        val s = state(mapOf(WorkspacePreferenceKey.THEME_SELECTED to "user.not-here", WorkspacePreferenceKey.THEME_APPEARANCE to "dark"))
        assertEquals(BuiltInThemeSpecifications.DEFAULT_IDENTIFIER, s.specification.identifier)
        assertEquals(ThemeMode.DARK, s.mode)
        assertEquals(BuiltInThemeSpecifications.DEFAULT_IDENTIFIER, state(mapOf(WorkspacePreferenceKey.THEME_SELECTED to null)).specification.identifier)
    }

    @Test
    fun nothingChosenIsPriorityAndAChosenZedIsKept() {
        assertEquals(BuiltInThemeSpecifications.DEFAULT_IDENTIFIER, ThemeLibraryState().specification.identifier)
        assertEquals(BuiltInThemeSpecifications.DEFAULT_IDENTIFIER, state(emptyMap()).specification.identifier)
        val zed = state(mapOf(WorkspacePreferenceKey.THEME_SELECTED to BuiltInThemeSpecifications.CHALK_IDENTIFIER))
        assertEquals(BuiltInThemeSpecifications.CHALK_IDENTIFIER, zed.specification.identifier)
        assertEquals("Zed", zed.specification.name)
    }

    @Test
    fun theDeviceChoiceWinsOnlyWhenOptedIn() {
        val shared = mapOf(WorkspacePreferenceKey.THEME_SELECTED to "user.dusk", WorkspacePreferenceKey.THEME_APPEARANCE to "light")
        val device = ThemeSelection(BuiltInThemeSpecifications.CHALK_DARK_IDENTIFIER, ThemeMode.SYSTEM)
        assertEquals("user.dusk", state(shared, device).specification.identifier)
        val local = state(shared, device, useDevice = true)
        assertEquals(BuiltInThemeSpecifications.CHALK_DARK_IDENTIFIER, local.specification.identifier)
        assertEquals(ThemeMode.SYSTEM, local.mode)
    }

    @Test
    fun aLockedAppearanceOverridesTheSettingAndTheSystem() {
        val dark = BuiltInThemeSpecifications.chalkDark(ThemePlatform.ANDROID)
        assertEquals(ThemeAppearance.DARK, ResolvedTheme.appearance(dark, ThemeMode.LIGHT, systemDark = false))
        val chalk = BuiltInThemeSpecifications.chalk(ThemePlatform.ANDROID)
        assertEquals(ThemeAppearance.LIGHT, ResolvedTheme.appearance(chalk, ThemeMode.SYSTEM, systemDark = false))
        assertEquals(ThemeAppearance.DARK, ResolvedTheme.appearance(chalk, ThemeMode.SYSTEM, systemDark = true))
        assertEquals(ThemeAppearance.LIGHT, ResolvedTheme.appearance(chalk, ThemeMode.LIGHT, systemDark = true))
    }

    @Test
    fun dataStoreThemesAndTheirChoiceBecomeRows() {
        val files = """[{"name":"dusk.json","json":"{ \"name\": \"Dusk\" }"},{"name":"mine.json","json":"{ \"identifier\": \"mine\" }"}]"""
        val migration = LocalThemeMigration.of(
            mapOf(LocalThemeMigration.FILES to files, LocalThemeMigration.SELECTED to "mine", LocalThemeMigration.APPEARANCE to "dark"),
        )
        assertEquals(setOf("user.dusk", "mine"), migration.rows.keys)
        assertEquals("mine", migration.selected)
        assertEquals("dark", migration.appearance)
    }

    @Test
    fun theFirstSettingsKeysMigrateToo() {
        val imported = """{ "name": "Paper" }"""
        val active = LocalThemeMigration.of(mapOf(LocalThemeMigration.LEGACY_ACTIVE to imported, LocalThemeMigration.LEGACY_MODE to "light"))
        assertEquals(mapOf("user.imported" to imported), active.rows)
        assertEquals("user.imported", active.selected)
        assertEquals("light", active.appearance)

        val chalkDark = """{"identifier":"native.theme.chalk.dark","extends":"native.theme.chalk.dark"}"""
        val dark = LocalThemeMigration.of(mapOf(LocalThemeMigration.LEGACY_ACTIVE to chalkDark, LocalThemeMigration.LEGACY_LIBRARY to imported))
        assertEquals(mapOf("user.imported" to imported), dark.rows)
        assertEquals(BuiltInThemeSpecifications.CHALK_DARK_IDENTIFIER, dark.selected)
        assertNull(dark.appearance)
    }

    @Test
    fun theTypeOverrideLaysOverWhicheverThemeIsChosen() {
        val override = ThemeTypographyOverride(bodyFamily = "Arvo", textScale = 1.2)
        val zed = ThemeLibraryState.of(
            rows, mapOf(WorkspacePreferenceKey.THEME_SELECTED to BuiltInThemeSpecifications.CHALK_IDENTIFIER),
            ThemeSelection(), false, override,
        )
        assertEquals("Zed", zed.specification.name)
        assertEquals("Arvo", zed.specification.structure.typography.body.families.first())
        val base = zed.themeSpecification.structure.typography
        val type = zed.specification.structure.typography
        assertEquals(base.body.families.filter { it != "Arvo" }, type.body.families.drop(1))
        assertTrue(type.bodySize > base.bodySize)
        assertEquals(base.display, type.display)
        // The chosen theme itself is untouched, for the settings' "Theme default".
        assertEquals(override.applied(zed.themeSpecification), zed.specification)
        assertEquals(zed.themeSpecification, ThemeLibraryState.of(rows, mapOf(WorkspacePreferenceKey.THEME_SELECTED to BuiltInThemeSpecifications.CHALK_IDENTIFIER), ThemeSelection(), false).specification)
    }
}
