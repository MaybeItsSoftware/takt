package uk.co.maybeitsadam.takt.ui.inspector

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.MicroLabel
import uk.co.maybeitsadam.takt.ui.components.PSwitch
import uk.co.maybeitsadam.takt.ui.lists.PMenu
import uk.co.maybeitsadam.takt.ui.lists.PMenuItem
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics

/** A block of the inspector, as a Mac settings group: a micro-label, its controls, a muted hairline under it. */
@Composable
fun InspectorSection(
    label: String,
    modifier: Modifier = Modifier,
    detail: String? = null,
    action: (@Composable () -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(modifier.fillMaxWidth()) {
        Column(
            Modifier.fillMaxWidth().padding(horizontal = Metrics.lg, vertical = Metrics.md),
            verticalArrangement = Arrangement.spacedBy(Metrics.sm),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                MicroLabel(label, Modifier.weight(1f))
                if (action != null) {
                    action()
                } else if (detail != null) {
                    Text(detail, style = TaktTheme.type.monoSecondary, color = TaktTheme.colors.dimText)
                }
            }
            content()
        }
        Hairline(color = TaktTheme.colors.borderMuted)
    }
}

/** A sub-label inside a section: smaller, still muted, still sentence case. */
@Composable
fun FieldLabel(text: String, modifier: Modifier = Modifier) {
    Text(text, style = TaktTheme.type.secondary, color = TaktTheme.colors.mutedText, modifier = modifier)
}

/** A line of explanation under a control. */
@Composable
fun Hint(text: String, modifier: Modifier = Modifier, color: Color = TaktTheme.colors.dimText) {
    Text(text, style = TaktTheme.type.secondary, color = color, modifier = modifier)
}

/**
 * The app's text field, as the Mac's inline field: the paper in a hairline at
 * the item radius that takes the focus outline when focused and the danger
 * hue on an error, 48dp tall. No Material underline.
 */
@Composable
fun ChalkField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String = "",
    singleLine: Boolean = true,
    minLines: Int = 1,
    maxLines: Int = if (singleLine) 1 else Int.MAX_VALUE,
    style: TextStyle = TaktTheme.type.body,
    isError: Boolean = false,
    keyboardOptions: KeyboardOptions = KeyboardOptions.Default,
    keyboardActions: KeyboardActions = KeyboardActions.Default,
    leading: (@Composable () -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
    contentDescription: String? = null,
) {
    val colors = TaktTheme.colors
    val interaction = remember { MutableInteractionSource() }
    val focused by interaction.collectIsFocusedAsState()
    val borderColor = when {
        isError -> colors.danger
        focused -> colors.focus
        else -> colors.border
    }
    BasicTextField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier
            .fillMaxWidth()
            .then(if (contentDescription != null) Modifier.semantics { this.contentDescription = contentDescription } else Modifier),
        singleLine = singleLine,
        minLines = minLines,
        maxLines = maxLines,
        textStyle = style.copy(color = colors.ink),
        cursorBrush = SolidColor(colors.primary),
        keyboardOptions = keyboardOptions,
        keyboardActions = keyboardActions,
        interactionSource = interaction,
        decorationBox = { inner ->
            Row(
                Modifier
                    .heightIn(min = Metrics.touchTarget)
                    .clip(Metrics.card)
                    .background(colors.paper)
                    .border(BorderStroke(Metrics.hairline, borderColor), Metrics.card)
                    .padding(horizontal = Metrics.sm + Metrics.xxs, vertical = Metrics.sm + Metrics.xxs),
                verticalAlignment = if (singleLine) Alignment.CenterVertically else Alignment.Top,
                horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
            ) {
                leading?.invoke()
                Box(Modifier.weight(1f)) {
                    if (value.isEmpty() && placeholder.isNotEmpty()) {
                        Text(placeholder, style = style, color = colors.dimText, maxLines = if (singleLine) 1 else Int.MAX_VALUE)
                    }
                    inner()
                }
                trailing?.invoke()
            }
        },
    )
}

/** A whole-row switch: the label (and detail) on the left, a Chalk switch on the right. */
@Composable
fun SwitchRow(
    label: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    detail: String? = null,
    enabled: Boolean = true,
) {
    val colors = TaktTheme.colors
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = Metrics.touchTarget)
            .clip(Metrics.control)
            .toggleable(value = checked, enabled = enabled, role = Role.Switch, onValueChange = onCheckedChange),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f).padding(vertical = Metrics.xs)) {
            Text(label, style = TaktTheme.type.body, color = if (enabled) colors.ink else colors.dimText)
            if (detail != null) Text(detail, style = TaktTheme.type.secondary, color = colors.mutedText)
        }
        Spacer(Modifier.width(Metrics.sm))
        PSwitch(checked = checked, onCheckedChange = null, enabled = enabled)
    }
}

/** One option of a [Segmented] control. */
data class Segment<T>(val label: String, val value: T, val tint: Color? = null, val description: String? = null)

/**
 * A row of joined segments with hairline dividers, as the Mac's settings
 * control: 30dp (its 26) at the popover radius on the panel fill. The
 * selected one takes the selection's fill, or a tint of its own hue when the
 * option names one.
 */
@Composable
fun <T> Segmented(
    options: List<Segment<T>>,
    selected: T,
    onSelect: (T) -> Unit,
    modifier: Modifier = Modifier,
    mono: Boolean = false,
) {
    val colors = TaktTheme.colors
    Row(
        modifier
            .fillMaxWidth()
            .height(IntrinsicSize.Min)
            .heightIn(min = Metrics.controlHeight + Metrics.xs)
            .clip(Metrics.popover)
            .border(BorderStroke(Metrics.hairline, colors.border), Metrics.popover),
    ) {
        options.forEachIndexed { index, option ->
            if (index > 0) Box(Modifier.width(Metrics.hairline).fillMaxHeight().background(colors.border))
            val isSelected = option.value == selected
            Box(
                Modifier
                    .weight(1f)
                    .fillMaxHeight()
                    .background(
                        when {
                            isSelected && option.tint != null -> option.tint.copy(alpha = 0.14f)
                            isSelected -> colors.selected
                            else -> colors.altRow
                        },
                    )
                    .selectable(selected = isSelected, role = Role.Tab, onClick = { onSelect(option.value) })
                    .then(if (option.description != null) Modifier.semantics { contentDescription = option.description } else Modifier),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    option.label,
                    style = if (mono) TaktTheme.type.monoSecondary else TaktTheme.type.small,
                    color = when {
                        isSelected && option.tint != null -> option.tint
                        isSelected -> colors.ink
                        else -> colors.mutedText
                    },
                    maxLines = 1,
                )
            }
        }
    }
}

/** A field-shaped button that opens a menu: the current value, a chevron. */
@Composable
fun MenuField(
    text: String,
    expanded: Boolean,
    onExpandedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    leading: ImageVector? = null,
    contentDescription: String? = null,
    menu: @Composable ColumnScope.() -> Unit,
) {
    val colors = TaktTheme.colors
    Box(modifier) {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = Metrics.controlHeight + Metrics.xs)
                .clip(Metrics.popover)
                .background(colors.altRow)
                .border(BorderStroke(Metrics.hairline, if (expanded) colors.focus else colors.border), Metrics.popover)
                .clickable(role = Role.DropdownList) { onExpandedChange(!expanded) }
                .then(if (contentDescription != null) Modifier.semantics { this.contentDescription = contentDescription } else Modifier)
                .padding(horizontal = Metrics.md),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
        ) {
            if (leading != null) Icon(leading, null, tint = colors.icon, modifier = Modifier.size(16.dp))
            Text(text, style = TaktTheme.type.body, color = colors.ink, maxLines = 1, modifier = Modifier.weight(1f))
            Icon(
                androidx.compose.material.icons.Icons.Filled.KeyboardArrowDown, null,
                tint = colors.icon, modifier = Modifier.size(16.dp),
            )
        }
        ChalkMenu(expanded = expanded, onDismiss = { onExpandedChange(false) }, content = menu)
    }
}

/** A dropdown as the Mac's: [PMenu]'s panel fill, hairline and popover radius. */
@Composable
fun ChalkMenu(expanded: Boolean, onDismiss: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    PMenu(expanded = expanded, onDismiss = onDismiss, content = content)
}

/** One choice in a [ChalkMenu]; the one in force takes the selection's fill. */
@Composable
fun ChalkMenuItem(text: String, onClick: () -> Unit, selected: Boolean = false, modifier: Modifier = Modifier) {
    PMenuItem(text, selected = selected, modifier = modifier, onClick = onClick)
}

/** A small square button for steppers, as the Mac's button: a hairline box at the popover radius on the panel fill. */
@Composable
fun StepButton(icon: ImageVector, description: String, modifier: Modifier = Modifier, enabled: Boolean = true, onClick: () -> Unit) {
    val colors = TaktTheme.colors
    Box(
        modifier
            .size(Metrics.headerHeight)
            .clip(Metrics.popover)
            .border(BorderStroke(Metrics.hairline, colors.border), Metrics.popover)
            .background(colors.altRow)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, null, tint = if (enabled) colors.icon else colors.dimText, modifier = Modifier.size(16.dp))
    }
}
