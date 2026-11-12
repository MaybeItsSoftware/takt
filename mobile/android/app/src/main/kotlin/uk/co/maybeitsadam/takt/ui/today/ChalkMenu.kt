package uk.co.maybeitsadam.takt.ui.today

import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.runtime.Composable
import uk.co.maybeitsadam.takt.ui.lists.PMenu
import uk.co.maybeitsadam.takt.ui.lists.PMenuItem

/** A dropdown as the Mac's: [PMenu]'s panel fill, hairline and popover radius. */
@Composable
fun ChalkMenu(expanded: Boolean, onDismiss: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    PMenu(expanded = expanded, onDismiss = onDismiss, content = content)
}

/** One row of a [ChalkMenu]; [destructive] sets it in the danger hue. */
@Composable
fun ChalkMenuItem(text: String, destructive: Boolean = false, enabled: Boolean = true, onClick: () -> Unit) {
    PMenuItem(text, destructive = destructive, enabled = enabled, onClick = onClick)
}
