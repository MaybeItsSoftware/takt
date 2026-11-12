package uk.co.maybeitsadam.takt.core.theme

// Port of Sources/TaktCore/Theming/ThemePalette.swift.

import uniffi.takt_core.themeChromeColor
import uniffi.takt_core.themeStatesRole

enum class ThemeAppearance(val raw: String) {
    LIGHT("light"),
    DARK("dark");

    val opposite: ThemeAppearance get() = if (this == LIGHT) DARK else LIGHT

    companion object {
        fun of(raw: String?): ThemeAppearance? = entries.firstOrNull { it.raw == raw }
    }
}

/**
 * Every colour a component may name. Components ask for a role, never a hex
 * value, so a theme changes everything drawn in a role by changing the role.
 * Declaration order is the Swift `allCases` order.
 */
enum class ThemeColorRole(val raw: String) {
    // Neutral spine
    PAPER("paper"),
    RAISED("raised"),
    ALT_ROW("altRow"),
    HOVER("hover"),
    WELL("well"),
    BORDER("border"),
    BORDER_MUTED("borderMuted"),
    INPUT_BORDER("inputBorder"),
    INK("ink"),
    MUTED_TEXT("mutedText"),
    DIM_TEXT("dimText"),

    // Status: the fixed four-way convention
    PRIMARY("primary"),
    SUCCESS("success"),
    DANGER("danger"),
    WARNING("warning"),

    // Categorical extras: identity colour only, never chrome
    CATEGORICAL_PURPLE("categoricalPurple"),
    CATEGORICAL_PINK("categoricalPink"),
    CATEGORICAL_ORANGE("categoricalOrange"),

    // Theme-invariant media surfaces
    MEDIA_LETTERBOX("mediaLetterbox"),
    MEDIA_SCRIM("mediaScrim"),
    MEDIA_SCRIM_INK("mediaScrimInk");

    /** Media surfaces do not flip: they are read from `light` whatever is in force. */
    val isThemeInvariant: Boolean get() = this in THEME_INVARIANT

    companion object {
        val THEME_INVARIANT: Set<ThemeColorRole> = setOf(MEDIA_LETTERBOX, MEDIA_SCRIM, MEDIA_SCRIM_INK)

        /** The roles running text is set in, held to 4.5:1 by the audit. */
        val BODY_TEXT_ROLES: List<ThemeColorRole> = listOf(INK, MUTED_TEXT)

        fun of(raw: String?): ThemeColorRole? = entries.firstOrNull { it.raw == raw }
    }
}

/**
 * Zed's chrome, which a theme may state and need not: the selected row, the
 * hairlines under a tree's levels, the status bar, the outline round the
 * keyboard's row and an icon at rest. The core derives one a theme leaves out
 * (`CoreThemePalette::chrome`), as Zed draws a theme that names none.
 */
enum class ThemeChromeRole(val raw: String) {
    SELECTED("selected"),
    INDENT_GUIDE("indentGuide"),
    STATUS_BAR("statusBar"),
    FOCUS("focus"),
    ICON("icon");

    companion object {
        fun of(raw: String?): ThemeChromeRole? = entries.firstOrNull { it.raw == raw }
    }
}

/**
 * A role → colour table for each appearance, plus whichever chrome roles the
 * theme states ([chromeLight], [chromeDark]); [chrome] resolves those through
 * the core, deriving the rest.
 */
data class ThemePalette(
    val light: Map<ThemeColorRole, ThemeColorValue>,
    val dark: Map<ThemeColorRole, ThemeColorValue>,
    val chromeLight: Map<ThemeChromeRole, ThemeColorValue> = emptyMap(),
    val chromeDark: Map<ThemeChromeRole, ThemeColorValue> = emptyMap(),
) {
    /** A chrome role, opaque: as the theme states it, else derived by the core from the other roles. */
    fun chrome(role: ThemeChromeRole, appearance: ThemeAppearance): ThemeColorValue =
        ThemeColorValue.of(themeChromeColor(core, role.raw, appearance.core))

    /** Whether the theme states [role] itself, rather than leaving it to be derived. */
    fun states(role: ThemeChromeRole): Boolean = themeStatesRole(core, role.raw)

    /**
     * The role's colour in an appearance: that table, then the other one (a
     * reported error, but not unpainted), then debug magenta.
     */
    fun color(role: ThemeColorRole, appearance: ThemeAppearance): ThemeColorValue {
        if (role.isThemeInvariant) return light[role] ?: dark[role] ?: ThemeColorValue.UNRESOLVED
        return table(appearance)[role] ?: table(appearance.opposite)[role] ?: ThemeColorValue.UNRESOLVED
    }

    fun table(appearance: ThemeAppearance): Map<ThemeColorRole, ThemeColorValue> =
        if (appearance == ThemeAppearance.LIGHT) light else dark

    /** Roles with no value of their own in `appearance`, by name. */
    fun missingRoles(appearance: ThemeAppearance): List<ThemeColorRole> = ThemeColorRole.entries
        .filter { role ->
            if (role.isThemeInvariant) appearance == ThemeAppearance.LIGHT && light[role] == null
            else table(appearance)[role] == null
        }
        .sortedBy { it.raw }
}
