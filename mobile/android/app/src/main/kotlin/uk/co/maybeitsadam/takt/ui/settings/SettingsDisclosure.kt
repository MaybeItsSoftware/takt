package uk.co.maybeitsadam.takt.ui.settings

import android.content.Context
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
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
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.content.edit
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme

/**
 * A long settings group folded behind its title: the label, what is chosen
 * now, and a chevron, across the page's width at the touch target's height.
 * Tapping it opens the card and its footer in place. Settings is one long
 * page on Android, so the groups that run to a screen of rows (every theme,
 * every celebration, every calendar) start closed and the page reads as a
 * short list of headings; a group of a few rows stays a plain [Section].
 *
 * Whether each is open is remembered on the device, by [key]. TalkBack hears
 * it as a heading that is expanded or collapsed.
 */
@Composable
internal fun CollapsibleSection(
    title: String,
    key: String,
    summary: String? = null,
    footer: String? = null,
    opensByDefault: Boolean = false,
    content: @Composable ColumnScope.() -> Unit,
) {
    val context = LocalContext.current.applicationContext
    val preferences = remember(context) { context.getSharedPreferences(DISCLOSURE_PREFERENCES, Context.MODE_PRIVATE) }
    var open by remember(key) { mutableStateOf(preferences.getBoolean(key, opensByDefault)) }
    val turn by animateFloatAsState(if (open) 180f else 0f, label = "chevron")
    Column(verticalArrangement = Arrangement.spacedBy(Metrics.sm)) {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = Metrics.touchTarget)
                .clickable(role = Role.Button, onClickLabel = if (open) "Collapse" else "Expand") {
                    open = !open
                    preferences.edit { putBoolean(key, open) }
                }
                .semantics {
                    heading()
                    stateDescription = if (open) "Expanded" else "Collapsed"
                }
                .testTag("settings_section_$key"),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
        ) {
            Text(title, style = TaktTheme.type.label, color = TaktTheme.colors.mutedText, modifier = Modifier.weight(1f))
            if (summary != null && !open) {
                Text(
                    summary, style = TaktTheme.type.small, color = TaktTheme.colors.mutedText,
                    maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false),
                )
            }
            Icon(
                Icons.Filled.KeyboardArrowDown, contentDescription = null, tint = TaktTheme.colors.mutedText,
                modifier = Modifier.size(20.dp).rotate(turn),
            )
        }
        // Compose scales these by the system's animation setting, so "Remove
        // animations" collapses them to an instant change.
        AnimatedVisibility(open, enter = expandVertically() + fadeIn(), exit = shrinkVertically() + fadeOut()) {
            Column(verticalArrangement = Arrangement.spacedBy(Metrics.sm)) {
                SettingsRows(content)
                if (footer != null) Text(footer, style = TaktTheme.type.secondary, color = TaktTheme.colors.dimText)
            }
        }
    }
}

private const val DISCLOSURE_PREFERENCES = "settings_disclosure"
