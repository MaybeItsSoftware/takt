package uk.co.maybeitsadam.takt.watch.ui

import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.wear.compose.material3.ColorScheme
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.Typography
import uk.co.maybeitsadam.takt.watch.R

/**
 * Zed One Dark, as hex, for the watch: the phone's and the Mac's default
 * theme (`one_palette()` in core/src/theme/builtins.rs). The phone's theme
 * tokens come from the Rust core, which the watch never loads, so these are
 * the hexes themselves. The names keep their roles: [azure] is the accent,
 * [emerald], [raspberry] and [amber] the status hues.
 */
object WatchColors {
    val page = Color(0xFF282C33)
    val raised = Color(0xFF2F343E)
    val hover = Color(0xFF363C46)
    val border = Color(0xFF464B57)
    val ink = Color(0xFFDCE0E5)
    val muted = Color(0xFFA9AFBC)
    val dim = Color(0xFF878A98)
    val azure = Color(0xFF74ADE8)
    val emerald = Color(0xFFA1C181)
    val raspberry = Color(0xFFD07277)
    val amber = Color(0xFFDEC184)
}

/** The Mac's radii: 6dp for cards, 4dp for buttons; nothing between. */
object WatchShape {
    val card = 6.dp
    val control = 4.dp
    val hairline = 1.dp
    /** Every tappable thing is at least this tall. */
    val target = 48.dp
}

val PlexSans = FontFamily(
    Font(R.font.ibmplexsans_regular, FontWeight.Normal),
    Font(R.font.ibmplexsans_medium, FontWeight.Medium),
)

val Lilex = FontFamily(Font(R.font.lilex_medium, FontWeight.Medium))

private val scheme = ColorScheme(
    primary = WatchColors.azure,
    primaryDim = WatchColors.azure,
    primaryContainer = WatchColors.raised,
    onPrimary = WatchColors.page,
    onPrimaryContainer = WatchColors.ink,
    secondary = WatchColors.muted,
    secondaryContainer = WatchColors.raised,
    onSecondary = WatchColors.page,
    onSecondaryContainer = WatchColors.ink,
    tertiary = WatchColors.emerald,
    surfaceContainerLow = WatchColors.page,
    surfaceContainer = WatchColors.raised,
    surfaceContainerHigh = WatchColors.hover,
    onSurface = WatchColors.ink,
    onSurfaceVariant = WatchColors.muted,
    outline = WatchColors.border,
    outlineVariant = WatchColors.border,
    background = WatchColors.page,
    onBackground = WatchColors.ink,
    error = WatchColors.raspberry,
    onError = Color.White,
)

@Composable
fun WatchTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = scheme, typography = Typography(defaultFontFamily = PlexSans), content = content)
}
