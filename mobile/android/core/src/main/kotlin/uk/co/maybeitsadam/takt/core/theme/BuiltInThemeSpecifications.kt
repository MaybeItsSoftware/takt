package uk.co.maybeitsadam.takt.core.theme

import uniffi.takt_core.themeBuiltins
import java.util.concurrent.ConcurrentHashMap

/**
 * Priority (the default, shown as Takt), then Zed, Zed Dark, Grape and Zed One,
 * resolved per platform.
 *
 * They are defined once, in the Rust core (`core/src/theme/builtins.rs`),
 * which the Mac and the iPhone read them from too; `shared/themes/` holds
 * them as complete files. So the apps cannot drift apart on a hex value.
 */
object BuiltInThemeSpecifications {
    /**
     * Priority, the default: what a fresh install shows, what a theme file
     * extends unless it says otherwise, and what stands in for a theme that
     * will not load.
     */
    const val PRIORITY_IDENTIFIER = "native.theme.priority"

    /**
     * The Zed look. The identifiers still say Chalk, its old name, because
     * they are stored as people's choice and synced between devices.
     */
    const val CHALK_IDENTIFIER = "native.theme.chalk"
    const val CHALK_DARK_IDENTIFIER = "native.theme.chalk.dark"

    /** Grape: the house design language — Zed's palette with Arvo, Geist Mono and small capitals. */
    const val GRAPE_IDENTIFIER = "native.theme.grape"

    /** Zed One: Zed's own One Light and One Dark, the Rust Mac app's look. */
    const val ONE_IDENTIFIER = "native.theme.one"

    /** The identifier a device uses when nothing has been chosen. */
    const val DEFAULT_IDENTIFIER = PRIORITY_IDENTIFIER

    private val cache = ConcurrentHashMap<ThemePlatform, List<ThemeSpecification>>()

    /** Priority, then Zed, Zed Dark and Grape: the default first. */
    fun all(platform: ThemePlatform): List<ThemeSpecification> =
        cache.getOrPut(platform) { themeBuiltins(platform.core).map { it.local() } }

    /** The default theme, resolved for [platform]. */
    fun defaultTheme(platform: ThemePlatform): ThemeSpecification = priority(platform)

    fun priority(platform: ThemePlatform): ThemeSpecification = all(platform)[0]

    /** Zed, under its old identifier. */
    fun chalk(platform: ThemePlatform): ThemeSpecification = all(platform)[1]

    /** Zed Dark, under its old identifier. */
    fun chalkDark(platform: ThemePlatform): ThemeSpecification = all(platform)[2]

    fun grape(platform: ThemePlatform): ThemeSpecification = all(platform)[3]

    fun specification(identifier: String, platform: ThemePlatform): ThemeSpecification? =
        all(platform).firstOrNull { it.identifier == identifier }
}
