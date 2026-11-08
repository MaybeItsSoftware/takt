package uk.co.maybeitsadam.takt.ui.navigation

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import uk.co.maybeitsadam.takt.app.AppContainer

/**
 * What the app shell offers every screen: the container, navigation, the
 * shared sheets, and the window's shape. Provided once at the root.
 */
@Immutable
class Shell(
    val container: AppContainer,
    val navigator: AppNavigator,
    val layout: WindowLayout,
    val showHistory: () -> Unit,
    val showPalette: () -> Unit,
    /** Opens the sidebar drawer on a phone; null where the sidebar is always shown. */
    val openSidebar: (() -> Unit)? = null,
)

/** The window's width class, decided once at the root from its constraints. */
@Immutable
data class WindowLayout(val widthDp: Int) {
    /** Tablets and unfolded foldables: a rail, two panes. */
    val isWide: Boolean get() = widthDp >= 600

    /** Wide enough that the inspector sits beside the content rather than over it. */
    val inspectorAsPane: Boolean get() = widthDp >= 720
}

val LocalShell = staticCompositionLocalOf<Shell> { error("No Shell provided") }
