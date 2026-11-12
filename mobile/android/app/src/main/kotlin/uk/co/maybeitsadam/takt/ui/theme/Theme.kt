package uk.co.maybeitsadam.takt.ui.theme

import androidx.compose.foundation.LocalIndication
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import uk.co.maybeitsadam.takt.R
import uk.co.maybeitsadam.takt.core.theme.BuiltInThemeSpecifications
import uk.co.maybeitsadam.takt.core.theme.ThemeAppearance
import uk.co.maybeitsadam.takt.core.theme.ThemeColorRole
import uk.co.maybeitsadam.takt.core.theme.ThemeFontDesign
import uk.co.maybeitsadam.takt.core.theme.ThemeFontFace
import uk.co.maybeitsadam.takt.core.theme.ThemeFontWeight
import uk.co.maybeitsadam.takt.core.theme.ThemePlatform
import uk.co.maybeitsadam.takt.core.theme.ThemeSpecification
import uk.co.maybeitsadam.takt.core.theme.ThemeTypeScale

/** Whether the app follows the system, or holds one appearance. Stored as `theme.appearance`. */
enum class ThemeMode(val raw: String, val title: String) {
    SYSTEM("system", "System"),
    LIGHT("light", "Light"),
    DARK("dark", "Dark");

    companion object {
        fun of(raw: String?): ThemeMode = entries.firstOrNull { it.raw == raw } ?: SYSTEM
    }
}

/**
 * The faces Takt bundles — IBM Plex Sans and Lilex (Zed's pair), Inter,
 * Geist, Geist Mono, JetBrains Mono and Arvo — the same set as the Mac and
 * iPhone. A theme names families; these are the ones that resolve to bundled
 * files.
 */
object Fonts {
    val sans = FontFamily(
        Font(R.font.ibmplexsans_regular, FontWeight.Normal),
        Font(R.font.ibmplexsans_italic, FontWeight.Normal, FontStyle.Italic),
        Font(R.font.ibmplexsans_medium, FontWeight.Medium),
        Font(R.font.ibmplexsans_semibold, FontWeight.SemiBold),
        Font(R.font.ibmplexsans_bold, FontWeight.Bold),
    )
    val mono = FontFamily(
        Font(R.font.lilex_regular, FontWeight.Normal),
        Font(R.font.lilex_medium, FontWeight.Medium),
        Font(R.font.lilex_bold, FontWeight.Bold),
    )

    val inter = FontFamily(
        Font(R.font.inter_regular, FontWeight.Normal),
        Font(R.font.inter_italic, FontWeight.Normal, FontStyle.Italic),
        Font(R.font.inter_medium, FontWeight.Medium),
        Font(R.font.inter_semibold, FontWeight.SemiBold),
        Font(R.font.inter_bold, FontWeight.Bold),
    )
    val geist = FontFamily(
        Font(R.font.geist_regular, FontWeight.Normal),
        Font(R.font.geist_italic, FontWeight.Normal, FontStyle.Italic),
        Font(R.font.geist_medium, FontWeight.Medium),
        Font(R.font.geist_semibold, FontWeight.SemiBold),
        Font(R.font.geist_bold, FontWeight.Bold),
    )
    val geistMono = FontFamily(
        Font(R.font.geistmono_regular, FontWeight.Normal),
        Font(R.font.geistmono_medium, FontWeight.Medium),
        Font(R.font.geistmono_bold, FontWeight.Bold),
    )
    val jetBrainsMono = FontFamily(
        Font(R.font.jetbrainsmono_regular, FontWeight.Normal),
        Font(R.font.jetbrainsmono_medium, FontWeight.Medium),
        Font(R.font.jetbrainsmono_bold, FontWeight.Bold),
    )
    val arvo = FontFamily(
        Font(R.font.arvo_regular, FontWeight.Normal),
        Font(R.font.arvo_italic, FontWeight.Normal, FontStyle.Italic),
        Font(R.font.arvo_bold, FontWeight.Bold),
        Font(R.font.arvo_bolditalic, FontWeight.Bold, FontStyle.Italic),
    )

    private val bundled = mapOf(
        "ibm plex sans" to sans, "lilex" to mono, "inter" to inter, "geist" to geist,
        "geist mono" to geistMono, "jetbrains mono" to jetBrainsMono, "arvo" to arvo,
    )

    /** Android's own family aliases, which a theme may name directly. */
    private val system = mapOf(
        "sans-serif" to FontFamily.SansSerif, "serif" to FontFamily.Serif, "monospace" to FontFamily.Monospace,
        "cursive" to FontFamily.Cursive, "roboto" to FontFamily.SansSerif,
    )

    /**
     * The first family that resolves, bundled or an Android alias, then the
     * design's system face. An app cannot see arbitrary installed fonts on
     * Android, so "Arvo" without a bundled file is its design's face.
     */
    fun resolve(face: ThemeFontFace): FontFamily = face.families.firstNotNullOfOrNull {
        val key = it.trim().lowercase()
        bundled[key] ?: system[key]
    } ?: when (face.design) {
        ThemeFontDesign.SERIF -> FontFamily.Serif
        ThemeFontDesign.SANS, ThemeFontDesign.ROUNDED -> FontFamily.SansSerif
        ThemeFontDesign.MONOSPACED -> FontFamily.Monospace
    }
}

/** The radius scale, in dp. `docs/themes.md`: panel, row, control, pill, shell. */
@Immutable
data class ThemeRadii(val panel: Dp, val row: Dp, val control: Dp, val pill: Dp, val shell: Dp) {
    val panelShape = RoundedCornerShape(panel)
    val rowShape = RoundedCornerShape(row)
    val controlShape = RoundedCornerShape(control)
    val pillShape = RoundedCornerShape(pill)

    /**
     * The Mac's three steps, grown from the control radius as it grows them
     * from 4: 4 for rows, cards, buttons and fields ([item]); 6 for board
     * columns, menus, tooltips, popovers and settings controls ([popover]);
     * 8 for dialogs and sheets ([sheet]).
     */
    val item: Dp = control
    val popover: Dp = control + 2.dp
    val sheet: Dp = control + 4.dp
    val itemShape = RoundedCornerShape(item)
    val popoverShape = RoundedCornerShape(popover)
    val sheetShape = RoundedCornerShape(sheet)
    val sheetTopShape = RoundedCornerShape(topStart = sheet, topEnd = sheet)
}

@Immutable
data class ThemeBorders(val hairline: Dp, val emphasis: Dp, val focusRing: Dp)

/** The six-step spacing scale every padding comes from. */
@Immutable
data class ThemeSpacing(val xxs: Dp, val xs: Dp, val sm: Dp, val md: Dp, val lg: Dp, val xl: Dp)

/** The families a theme resolved to. */
@Immutable
data class ThemeFonts(val display: FontFamily, val body: FontFamily, val mono: FontFamily)

/** The micro-label: its style, its colour role and whether its text is uppercased. */
@Immutable
data class ThemeMicroLabelStyle(val style: TextStyle, val role: ThemeColorRole, val uppercase: Boolean)

/**
 * The text styles the app uses, by role rather than Material's size names,
 * every one derived from the theme's scale and faces.
 */
@Immutable
data class TaktType(
    val scale: ThemeTypeScale,
    /** `scale.title`, display face. Pane and sheet headings. */
    val title: TextStyle,
    /** `scale.body`, medium. Section headings, row titles that lead. */
    val heading: TextStyle,
    val body: TextStyle,
    val bodyStrong: TextStyle,
    /** `scale.caption`. Secondary lines and captions. */
    val small: TextStyle,
    /**
     * The Mac's 11 against its 13 body, on this body: a task's chips, counts,
     * status words and list section headings.
     */
    val secondary: TextStyle,
    /** [secondary] in the mono face: done/total, keys in a menu. */
    val monoSecondary: TextStyle,
    /** The theme's micro-label, minus its colour: section headers, column heads, tabs. */
    val label: TextStyle,
    /** The theme's mono face at caption size: times, counts and keys beside body text. */
    val mono: TextStyle,
    /** The mono face a step under caption, for the tightest numerals. */
    val monoSmall: TextStyle,
    /** The mono face at `scale.display`: the large numerals. */
    val monoLarge: TextStyle,
    /** The mono face at `scale.hero`: the focus timer. */
    val hero: TextStyle,
    /** A fifth over `scale.title`, display face: the one heading a screen is about (the focused task). */
    val headline: TextStyle,
    /** The mono face at `scale.body`: a value read on its own, like a time. */
    val monoBody: TextStyle,
    /** The mono face a tenth over `scale.title`: a value being adjusted. */
    val monoTitle: TextStyle,
    /**
     * Body text in an editable field, never under 16sp: smaller and some
     * keyboards zoom, which the house style rules out on touch.
     */
    val field: TextStyle,
)

/**
 * A theme resolved for this device: one appearance of one
 * [ThemeSpecification], in Compose units. Everything the UI draws comes from
 * here, through [TaktTheme].
 */
@Immutable
class ResolvedTheme(
    val specification: ThemeSpecification,
    val appearance: ThemeAppearance,
) {
    private val structure = specification.structure

    val colors: ThemeColors = ThemeColors.of(specification, appearance)

    val radii = structure.radius.let { ThemeRadii(it.panel.dp, it.row.dp, it.control.dp, it.pill.dp, it.shell.dp) }

    val borders = structure.border.let { ThemeBorders(it.hairline.dp, it.emphasis.dp, it.focusRing.dp) }

    val spacing = structure.spacing.let { ThemeSpacing(it.xxs.dp, it.xs.dp, it.sm.dp, it.md.dp, it.lg.dp, it.xl.dp) }

    /** The minimum hit area a control is grown to; 0 for none. */
    val touchTarget: Dp = structure.touchTarget.dp

    val fonts = structure.typography.let {
        ThemeFonts(display = Fonts.resolve(it.display), body = Fonts.resolve(it.body), mono = Fonts.resolve(it.mono))
    }

    val microLabel: ThemeMicroLabelStyle = structure.typography.microLabel.let { label ->
        ThemeMicroLabelStyle(
            style = TextStyle(
                fontFamily = fonts.body,
                fontWeight = label.weight.compose,
                fontSize = label.size.sp,
                lineHeight = lineHeight(label.size),
                letterSpacing = label.tracking.em,
            ),
            role = label.role,
            uppercase = label.isUppercased,
        )
    }

    val type: TaktType = structure.typography.scale.let { scale ->
        fun style(family: FontFamily, weight: FontWeight, size: Double, display: Boolean = false) = TextStyle(
            fontFamily = family, fontWeight = weight, fontSize = size.sp,
            lineHeight = if (display) (size * DISPLAY_LEADING).sp else lineHeight(size),
        )
        TaktType(
            scale = scale,
            title = style(fonts.display, FontWeight.SemiBold, scale.title),
            heading = style(fonts.body, FontWeight.Medium, scale.body),
            body = style(fonts.body, FontWeight.Normal, scale.body),
            bodyStrong = style(fonts.body, FontWeight.Medium, scale.body),
            small = style(fonts.body, FontWeight.Normal, scale.caption),
            secondary = style(fonts.body, FontWeight.Normal, scale.body * SECONDARY_RATIO),
            monoSecondary = style(fonts.mono, FontWeight.Normal, scale.body * SECONDARY_RATIO),
            label = microLabel.style,
            mono = style(fonts.mono, FontWeight.Normal, scale.caption),
            monoSmall = style(fonts.mono, FontWeight.Normal, kotlin.math.round(scale.caption * MONO_SMALL_RATIO)),
            monoLarge = style(fonts.mono, FontWeight.Medium, scale.display, display = true),
            hero = style(fonts.mono, FontWeight.Medium, scale.hero, display = true),
            headline = style(fonts.display, FontWeight.SemiBold, kotlin.math.round(scale.title * HEADLINE_RATIO), display = true),
            monoBody = style(fonts.mono, FontWeight.Normal, scale.body),
            monoTitle = style(fonts.mono, FontWeight.Normal, kotlin.math.round(scale.title * MONO_TITLE_RATIO)),
            field = style(fonts.body, FontWeight.Normal, maxOf(scale.body, MINIMUM_FIELD_SIZE)),
        )
    }

    val colorScheme: ColorScheme = colors.toColorScheme()

    // Stock components take the Mac's steps: menus and chips the control
    // radius, popovers 6, dialogs and sheets 8.
    val shapes = Shapes(
        extraSmall = radii.controlShape,
        small = radii.controlShape,
        medium = radii.popoverShape,
        large = radii.sheetShape,
        extraLarge = radii.sheetShape,
    )

    val materialTypography: Typography = Typography().let { base ->
        fun TextStyle.themed() = copy(fontFamily = fonts.display)
        Typography(
            displayLarge = base.displayLarge.themed(), displayMedium = base.displayMedium.themed(),
            displaySmall = base.displaySmall.themed(), headlineLarge = base.headlineLarge.themed(),
            headlineMedium = base.headlineMedium.themed(), headlineSmall = base.headlineSmall.themed(),
            titleLarge = type.title, titleMedium = type.heading, titleSmall = type.bodyStrong,
            bodyLarge = type.body, bodyMedium = type.body, bodySmall = type.small,
            labelLarge = type.bodyStrong, labelMedium = type.small.copy(fontWeight = FontWeight.Medium),
            labelSmall = type.monoSmall.copy(fontFamily = fonts.body),
        )
    }

    companion object {
        private const val LEADING = 1.35
        private const val DISPLAY_LEADING = 1.18
        private const val MONO_SMALL_RATIO = 0.9
        private const val HEADLINE_RATIO = 1.2
        private const val MONO_TITLE_RATIO = 1.1
        private const val MINIMUM_FIELD_SIZE = 16.0

        /** The Mac's secondary text (11) over its body (13). */
        private const val SECONDARY_RATIO = 11.0 / 13.0

        private fun lineHeight(size: Double): TextUnit = kotlin.math.round(size * LEADING).sp

        /** The appearance a theme is drawn in: its lock, else the setting, else the system. */
        fun appearance(spec: ThemeSpecification, mode: ThemeMode, systemDark: Boolean): ThemeAppearance =
            spec.lockedAppearance ?: when (mode) {
                ThemeMode.LIGHT -> ThemeAppearance.LIGHT
                ThemeMode.DARK -> ThemeAppearance.DARK
                ThemeMode.SYSTEM -> if (systemDark) ThemeAppearance.DARK else ThemeAppearance.LIGHT
            }

        val defaultLight by lazy { ResolvedTheme(BuiltInThemeSpecifications.defaultTheme(ThemePlatform.ANDROID), ThemeAppearance.LIGHT) }
    }
}

private val ThemeFontWeight.compose: FontWeight
    get() = when (this) {
        ThemeFontWeight.REGULAR -> FontWeight.Normal
        ThemeFontWeight.MEDIUM -> FontWeight.Medium
        ThemeFontWeight.SEMIBOLD -> FontWeight.SemiBold
        ThemeFontWeight.BOLD -> FontWeight.Bold
        ThemeFontWeight.BLACK -> FontWeight.Black
    }

val LocalPriorityTheme = staticCompositionLocalOf { ResolvedTheme.defaultLight }

/**
 * The theme in force. `TaktTheme.colors.ink`, `TaktTheme.type.body`,
 * `TaktTheme.spacing.md`, `TaktTheme.radii.controlShape`.
 */
object TaktTheme {
    val current: ResolvedTheme
        @Composable @ReadOnlyComposable get() = LocalPriorityTheme.current
    val colors: ThemeColors
        @Composable @ReadOnlyComposable get() = LocalPriorityTheme.current.colors
    val type: TaktType
        @Composable @ReadOnlyComposable get() = LocalPriorityTheme.current.type
    val spacing: ThemeSpacing
        @Composable @ReadOnlyComposable get() = LocalPriorityTheme.current.spacing
    val radii: ThemeRadii
        @Composable @ReadOnlyComposable get() = LocalPriorityTheme.current.radii
    val borders: ThemeBorders
        @Composable @ReadOnlyComposable get() = LocalPriorityTheme.current.borders
    val fonts: ThemeFonts
        @Composable @ReadOnlyComposable get() = LocalPriorityTheme.current.fonts
    val microLabel: ThemeMicroLabelStyle
        @Composable @ReadOnlyComposable get() = LocalPriorityTheme.current.microLabel
    val touchTarget: Dp
        @Composable @ReadOnlyComposable get() = LocalPriorityTheme.current.touchTarget

    /**
     * How far something that cannot be acted on yet steps back — a blocked
     * task's whole row. The Mac's `Theme.disabledOpacity`, so the two read alike.
     */
    const val DISABLED_ALPHA: Float = 0.45f
}

/**
 * Spacing, radii and hairlines by the names the screens already use, read
 * from the theme in force, plus the few layout constants that are not the
 * theme's to set.
 */
object Metrics {
    val xxs: Dp @Composable @ReadOnlyComposable get() = TaktTheme.spacing.xxs
    val xs: Dp @Composable @ReadOnlyComposable get() = TaktTheme.spacing.xs
    val sm: Dp @Composable @ReadOnlyComposable get() = TaktTheme.spacing.sm
    val md: Dp @Composable @ReadOnlyComposable get() = TaktTheme.spacing.md
    val lg: Dp @Composable @ReadOnlyComposable get() = TaktTheme.spacing.lg
    val xl: Dp @Composable @ReadOnlyComposable get() = TaktTheme.spacing.xl
    val hairline: Dp @Composable @ReadOnlyComposable get() = TaktTheme.borders.hairline
    val emphasis: Dp @Composable @ReadOnlyComposable get() = TaktTheme.borders.emphasis
    val focusRing: Dp @Composable @ReadOnlyComposable get() = TaktTheme.borders.focusRing

    /** `radius.panel`: a whole pane's edge (0 in Zed One, as the Mac's panes are square). */
    val panelRadius: Dp @Composable @ReadOnlyComposable get() = TaktTheme.radii.panel

    /** `radius.control`: inputs, chips and key hints. */
    val controlRadius: Dp @Composable @ReadOnlyComposable get() = TaktTheme.radii.control

    /** The Mac's 4: rows, cards, buttons, tabs, key hints, inline fields. */
    val cardRadius: Dp @Composable @ReadOnlyComposable get() = TaktTheme.radii.item
    val card: RoundedCornerShape @Composable @ReadOnlyComposable get() = TaktTheme.radii.itemShape
    val control: RoundedCornerShape @Composable @ReadOnlyComposable get() = TaktTheme.radii.controlShape

    /** A list row's selection and press, at the Mac's 4. */
    val row: RoundedCornerShape @Composable @ReadOnlyComposable get() = TaktTheme.radii.itemShape

    /** The Mac's 6: board columns, menus, tooltips, popovers and settings controls. */
    val popover: RoundedCornerShape @Composable @ReadOnlyComposable get() = TaktTheme.radii.popoverShape

    /** The Mac's 8: dialogs and sheets. */
    val sheet: RoundedCornerShape @Composable @ReadOnlyComposable get() = TaktTheme.radii.sheetShape
    val sheetTop: RoundedCornerShape @Composable @ReadOnlyComposable get() = TaktTheme.radii.sheetTopShape

    /** `radius.pill`: genuinely round things only (dots, switch knobs). */
    val pill: RoundedCornerShape @Composable @ReadOnlyComposable get() = TaktTheme.radii.pillShape

    /** The theme's minimum hit area. */
    val touchTarget: Dp @Composable @ReadOnlyComposable get() = TaktTheme.touchTarget

    // Layout, not theme: the Mac's measures, each scaled by [MAC] onto the
    // phone's larger body (15 against the Mac's 13).

    /** One Mac pixel on the phone: its 13 body against this 15. */
    const val MAC: Float = 15f / 13f

    /**
     * A row as a finger needs it. The Mac's is 26 (30dp here); a phone row
     * is the touch target, painting nothing but its selection and hairlines,
     * so a tap never lands between two.
     */
    val rowHeight: Dp = 48.dp

    /** The Mac's 26-high row, painted: a selection's fill, a board card's line. */
    val paintedRow: Dp = 30.dp

    /** Indent per outline level, as the Mac's 18. */
    val indent: Dp = 18.dp

    /** The Mac's 10-wide disclosure triangle and 12-wide status mark. */
    val disclosure: Dp = 12.dp
    val statusMark: Dp = 14.dp

    /** The title bar: the Mac's 34. */
    val headerHeight: Dp = 40.dp

    /** The status bar: the Mac's 30. */
    val statusBarHeight: Dp = 34.dp

    /** A button, segmented control or field in a row: the Mac's 26. */
    val controlHeight: Dp = 30.dp

    /** A settings row: the Mac's 38. */
    val settingsRowHeight: Dp = 44.dp

    /** A menu item: the Mac's 24, painted; its hit area grows to the touch target. */
    val menuItemHeight: Dp = 28.dp

    /** An icon in a bar: the Mac's 14–16. */
    val barIcon: Dp = 18.dp

    /** The sidebar, as a phone's drawer and a wide window's dock. */
    val sidebarWidth: Dp = 288.dp
    val inspectorWidth: Dp = 380.dp
}

/**
 * Material 3 themed down to the resolved theme: every surface tone is paper
 * or raised (no tonal elevation), the outline is the hairline border, the
 * shapes are the radius scale and the type is the theme's, so stock
 * components follow a theme the way Priority's own do.
 */
@Composable
fun TaktTheme(
    spec: ThemeSpecification = BuiltInThemeSpecifications.defaultTheme(ThemePlatform.ANDROID),
    mode: ThemeMode = ThemeMode.SYSTEM,
    content: @Composable () -> Unit,
) {
    val appearance = ResolvedTheme.appearance(spec, mode, isSystemInDarkTheme())
    val theme = remember(spec, appearance) { ResolvedTheme(spec, appearance) }
    TaktTheme(theme, content)
}

/** The same, for an already-resolved theme (previews, tests). */
@Composable
fun TaktTheme(theme: ResolvedTheme, content: @Composable () -> Unit) {
    CompositionLocalProvider(
        LocalPriorityTheme provides theme,
        LocalMinimumInteractiveComponentSize provides if (theme.touchTarget > 0.dp) theme.touchTarget else Dp.Unspecified,
    ) {
        MaterialTheme(
            colorScheme = theme.colorScheme,
            typography = theme.materialTypography,
            shapes = theme.shapes,
        ) {
            // A press is the Mac's hover fill, not a ripple.
            val press = remember(theme.colors.hover) { PressFill(theme.colors.hover) }
            CompositionLocalProvider(LocalIndication provides press, content = content)
        }
    }
}

private fun ThemeColors.toColorScheme(): ColorScheme {
    // Selection and status containers are made from the accent at low alpha
    // where they are drawn, a touch stronger in the dark.
    val tint = if (isDark) 0.16f else 0.12f
    val scrim = mediaLetterbox.copy(alpha = if (isDark) 0.6f else 0.4f)
    val onSecondary = if (isDark) paper else raised
    return if (isDark) {
        darkColorScheme(
            primary = primary, onPrimary = onAccent, primaryContainer = primary.copy(alpha = tint), onPrimaryContainer = ink,
            secondary = mutedText, onSecondary = onSecondary, secondaryContainer = well, onSecondaryContainer = ink,
            tertiary = categoricalPurple, onTertiary = onAccent, tertiaryContainer = well, onTertiaryContainer = ink,
            background = paper, onBackground = ink, surface = paper, onSurface = ink, surfaceVariant = well,
            onSurfaceVariant = mutedText, surfaceTint = Color.Transparent, inverseSurface = ink, inverseOnSurface = paper,
            inversePrimary = primary, error = danger, onError = onAccent, errorContainer = danger.copy(alpha = tint),
            onErrorContainer = ink, outline = inputBorder, outlineVariant = border, scrim = scrim,
            surfaceBright = raised, surfaceDim = paper, surfaceContainer = raised, surfaceContainerHigh = raised,
            surfaceContainerHighest = raised, surfaceContainerLow = raised, surfaceContainerLowest = raised,
        )
    } else {
        lightColorScheme(
            primary = primary, onPrimary = onAccent, primaryContainer = primary.copy(alpha = tint), onPrimaryContainer = ink,
            secondary = mutedText, onSecondary = onSecondary, secondaryContainer = well, onSecondaryContainer = ink,
            tertiary = categoricalPurple, onTertiary = onAccent, tertiaryContainer = well, onTertiaryContainer = ink,
            background = paper, onBackground = ink, surface = paper, onSurface = ink, surfaceVariant = well,
            onSurfaceVariant = mutedText, surfaceTint = Color.Transparent, inverseSurface = ink, inverseOnSurface = paper,
            inversePrimary = primary, error = danger, onError = onAccent, errorContainer = danger.copy(alpha = tint),
            onErrorContainer = ink, outline = inputBorder, outlineVariant = border, scrim = scrim,
            surfaceBright = raised, surfaceDim = paper, surfaceContainer = raised, surfaceContainerHigh = raised,
            surfaceContainerHighest = raised, surfaceContainerLow = raised, surfaceContainerLowest = raised,
        )
    }
}
