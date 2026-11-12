package uk.co.maybeitsadam.takt.ui.review

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import uk.co.maybeitsadam.takt.ui.components.VerticalHairline
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics

/**
 * A flat segmented control, as the Mac's view switch: a bordered strip at the
 * popover radius on the panel fill, hairlines between the options, the chosen
 * one on the selection's fill in ink. 34dp tall (the Mac's 26 and a little
 * more for a finger; Compose grows the hit area the rest of the way).
 */
@Composable
fun <T> Segmented(
    options: List<T>,
    selected: T,
    label: (T) -> String,
    modifier: Modifier = Modifier,
    tag: (T) -> String = { "" },
    onSelect: (T) -> Unit,
) {
    Row(
        modifier
            .fillMaxWidth()
            .height(Metrics.controlHeight + Metrics.xs)
            .clip(Metrics.popover)
            .border(BorderStroke(Metrics.hairline, TaktTheme.colors.border), Metrics.popover),
    ) {
        options.forEachIndexed { index, option ->
            if (index > 0) VerticalHairline(color = TaktTheme.colors.border)
            val isSelected = option == selected
            Box(
                Modifier
                    .weight(1f)
                    .fillMaxHeight()
                    .background(if (isSelected) TaktTheme.colors.selected else TaktTheme.colors.altRow)
                    .clickable(role = Role.Tab) { onSelect(option) }
                    .semantics { this.selected = isSelected }
                    .then(tag(option).takeIf { it.isNotEmpty() }?.let { Modifier.testTag(it) } ?: Modifier),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    label(option),
                    style = TaktTheme.type.small,
                    color = if (isSelected) TaktTheme.colors.ink else TaktTheme.colors.mutedText,
                    maxLines = 1,
                )
            }
        }
    }
}
