package uk.co.maybeitsadam.takt.ui.theme

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import uk.co.maybeitsadam.takt.core.theme.ThemeAppearance
import uk.co.maybeitsadam.takt.core.theme.ThemeChromeRole
import uk.co.maybeitsadam.takt.core.theme.ThemeColorRole
import uk.co.maybeitsadam.takt.core.theme.ThemeColorValue
import uk.co.maybeitsadam.takt.core.theme.ThemeSpecification

/**
 * Every colour role in `docs/themes.md`, resolved for one appearance of one
 * theme. Components name a role, never a hex value, so a theme changes
 * everything drawn in a role by changing the role.
 */
@Immutable
data class ThemeColors(
    val paper: Color,
    val raised: Color,
    val altRow: Color,
    val hover: Color,
    val well: Color,
    val border: Color,
    val borderMuted: Color,
    val inputBorder: Color,
    val ink: Color,
    val mutedText: Color,
    val dimText: Color,
    val primary: Color,
    val success: Color,
    val danger: Color,
    val warning: Color,
    val categoricalPurple: Color,
    val categoricalPink: Color,
    val categoricalOrange: Color,
    /** Behind a photo. Theme-invariant: the content underneath is not ours to theme. */
    val mediaLetterbox: Color,
    /** Under chrome sitting on an image. Theme-invariant. */
    val mediaScrim: Color,
    /** Text on [mediaScrim]. Theme-invariant. */
    val mediaScrimInk: Color,
    /** Zed's chrome, resolved by the core (stated by the theme, else derived): the selected row's fill. */
    val selected: Color,
    /** The hairline under each level of a tree, in the sidebar. */
    val indentGuide: Color,
    /** The status bar's ground; its words are [ink]. */
    val statusBar: Color,
    /** The 1dp outline round the row the inspector is showing, and the keyboard's. */
    val focus: Color,
    /** An icon at rest. */
    val icon: Color,
    val isDark: Boolean,
) {
    /**
     * Text drawn on a filled accent (primary buttons, the running block): the
     * scrim's light ink or the letterbox's dark, whichever reads on [primary].
     */
    val onAccent: Color
        get() = if (contrast(mediaScrimInk, primary) >= contrast(mediaLetterbox, primary)) mediaScrimInk else mediaLetterbox

    /** A role by its docs/themes.md name, for things a theme names by role (the micro-label). */
    fun role(role: ThemeColorRole): Color = when (role) {
        ThemeColorRole.PAPER -> paper
        ThemeColorRole.RAISED -> raised
        ThemeColorRole.ALT_ROW -> altRow
        ThemeColorRole.HOVER -> hover
        ThemeColorRole.WELL -> well
        ThemeColorRole.BORDER -> border
        ThemeColorRole.BORDER_MUTED -> borderMuted
        ThemeColorRole.INPUT_BORDER -> inputBorder
        ThemeColorRole.INK -> ink
        ThemeColorRole.MUTED_TEXT -> mutedText
        ThemeColorRole.DIM_TEXT -> dimText
        ThemeColorRole.PRIMARY -> primary
        ThemeColorRole.SUCCESS -> success
        ThemeColorRole.DANGER -> danger
        ThemeColorRole.WARNING -> warning
        ThemeColorRole.CATEGORICAL_PURPLE -> categoricalPurple
        ThemeColorRole.CATEGORICAL_PINK -> categoricalPink
        ThemeColorRole.CATEGORICAL_ORANGE -> categoricalOrange
        ThemeColorRole.MEDIA_LETTERBOX -> mediaLetterbox
        ThemeColorRole.MEDIA_SCRIM -> mediaScrim
        ThemeColorRole.MEDIA_SCRIM_INK -> mediaScrimInk
    }

    companion object {
        /** The theme's colours in one appearance. */
        fun of(spec: ThemeSpecification, appearance: ThemeAppearance): ThemeColors {
            fun c(role: ThemeColorRole) = spec.color(role, appearance).compose
            fun chrome(role: ThemeChromeRole) = spec.palette.chrome(role, appearance).compose
            return ThemeColors(
                paper = c(ThemeColorRole.PAPER),
                raised = c(ThemeColorRole.RAISED),
                altRow = c(ThemeColorRole.ALT_ROW),
                hover = c(ThemeColorRole.HOVER),
                well = c(ThemeColorRole.WELL),
                border = c(ThemeColorRole.BORDER),
                borderMuted = c(ThemeColorRole.BORDER_MUTED),
                inputBorder = c(ThemeColorRole.INPUT_BORDER),
                ink = c(ThemeColorRole.INK),
                mutedText = c(ThemeColorRole.MUTED_TEXT),
                dimText = c(ThemeColorRole.DIM_TEXT),
                primary = c(ThemeColorRole.PRIMARY),
                success = c(ThemeColorRole.SUCCESS),
                danger = c(ThemeColorRole.DANGER),
                warning = c(ThemeColorRole.WARNING),
                categoricalPurple = c(ThemeColorRole.CATEGORICAL_PURPLE),
                categoricalPink = c(ThemeColorRole.CATEGORICAL_PINK),
                categoricalOrange = c(ThemeColorRole.CATEGORICAL_ORANGE),
                mediaLetterbox = c(ThemeColorRole.MEDIA_LETTERBOX),
                mediaScrim = c(ThemeColorRole.MEDIA_SCRIM),
                mediaScrimInk = c(ThemeColorRole.MEDIA_SCRIM_INK),
                selected = chrome(ThemeChromeRole.SELECTED),
                indentGuide = chrome(ThemeChromeRole.INDENT_GUIDE),
                statusBar = chrome(ThemeChromeRole.STATUS_BAR),
                focus = chrome(ThemeChromeRole.FOCUS),
                icon = chrome(ThemeChromeRole.ICON),
                isDark = appearance == ThemeAppearance.DARK,
            )
        }

        private fun contrast(a: Color, b: Color): Float {
            val (lighter, darker) = a.luminance().let { la -> b.luminance().let { lb -> maxOf(la, lb) to minOf(la, lb) } }
            return (lighter + 0.05f) / (darker + 0.05f)
        }
    }
}

/** A core colour as Compose draws it. */
val ThemeColorValue.compose: Color get() = Color(argb)

/** `#rgb`, `#rrggbb` or `#rrggbbaa`; null for anything else. */
fun parseHexColor(hex: String?): Color? = hex?.let { ThemeColorValue.hex(it) }?.compose
