package uk.co.maybeitsadam.takt.ui.components

import android.content.Context
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import androidx.core.content.edit
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme

/**
 * A drop-down that opens in place: a [header] row at the touch target's
 * height ending in a chevron, and under it [content], which slides open
 * below rather than covering the page or opening a sheet.
 *
 * Whether it is open is remembered on the device by [key], so the Today
 * header you left open is open next time. TalkBack hears the header as a
 * button that is expanded or collapsed. Compose scales the animation by the
 * system's setting, so "Remove animations" makes it an instant change.
 */
@Composable
fun Disclosure(
    key: String,
    modifier: Modifier = Modifier,
    opensByDefault: Boolean = false,
    tag: String? = null,
    headerPadding: PaddingValues = PaddingValues(horizontal = Metrics.lg),
    onOpenChange: (Boolean) -> Unit = {},
    header: @Composable RowScope.(open: Boolean) -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    val context = LocalContext.current.applicationContext
    val preferences = remember(context) { context.getSharedPreferences(DISCLOSURE_PREFERENCES, Context.MODE_PRIVATE) }
    var open by remember(key) { mutableStateOf(preferences.getBoolean(key, opensByDefault)) }
    val turn by animateFloatAsState(if (open) 180f else 0f, label = "chevron")
    Column(modifier) {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = Metrics.touchTarget)
                .clickable(role = Role.Button, onClickLabel = if (open) "Collapse" else "Expand") {
                    open = !open
                    preferences.edit { putBoolean(key, open) }
                    onOpenChange(open)
                }
                .semantics { stateDescription = if (open) "Expanded" else "Collapsed" }
                .then(if (tag != null) Modifier.testTag(tag) else Modifier)
                .padding(headerPadding),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
        ) {
            header(open)
            Box(Modifier.size(20.dp), contentAlignment = Alignment.Center) {
                Icon(
                    Icons.Filled.KeyboardArrowDown, contentDescription = null, tint = TaktTheme.colors.mutedText,
                    modifier = Modifier.size(20.dp).rotate(turn),
                )
            }
        }
        AnimatedVisibility(open, enter = expandVertically() + fadeIn(), exit = shrinkVertically() + fadeOut()) {
            Column(Modifier.fillMaxWidth()) { content() }
        }
    }
}

/** The file every disclosure's open or closed state is kept in; Settings' groups already used it. */
const val DISCLOSURE_PREFERENCES = "settings_disclosure"
