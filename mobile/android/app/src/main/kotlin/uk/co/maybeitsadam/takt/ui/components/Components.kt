package uk.co.maybeitsadam.takt.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Settings
import uk.co.maybeitsadam.takt.ui.theme.PIcons
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import uk.co.maybeitsadam.takt.core.TaskStatus
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics

/** A 1dp rule in the border colour: the app's only means of separation. */
@Composable
fun Hairline(modifier: Modifier = Modifier, color: Color = TaktTheme.colors.border) {
    Box(modifier.fillMaxWidth().height(Metrics.hairline).background(color))
}

/**
 * The quiet label over a group's blocked tasks — an outline parent's
 * children, a board column's cards — which sink below the rest. The theme's
 * micro-label and a hairline, nothing to tap or focus: TalkBack reads it as
 * one line and the keyboard walks the tasks either side as if it were not
 * there.
 */
@Composable
fun BlockedDivider(modifier: Modifier = Modifier, isBuried: Boolean = false) {
    val label = TaktTheme.microLabel
    val title = if (isBuried) "Buried" else "Blocked"
    Row(
        modifier
            .fillMaxWidth()
            .focusProperties { canFocus = false }
            .clearAndSetSemantics {
                contentDescription = if (isBuried) "Buried: these tasks start later" else "Blocked: these tasks wait on others"
            },
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
    ) {
        Text(
            if (label.uppercase) title.uppercase() else title,
            style = label.style,
            color = TaktTheme.colors.role(label.role),
            maxLines = 1,
        )
        Hairline(Modifier.weight(1f))
    }
}

@Composable
fun VerticalHairline(modifier: Modifier = Modifier, color: Color = TaktTheme.colors.border) {
    Box(modifier.fillMaxHeight().width(Metrics.hairline).background(color))
}

/**
 * The header band every screen starts with, as the Mac's title bar: one line
 * on the well, 40dp (the Mac's 34), the place in ink, a dim note beside it,
 * trailing tools, and a border hairline beneath. It pads itself below the
 * status bar, painting the well under it too, so screens draw edge to edge.
 */
@Composable
fun TaktTopBar(
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    navigation: (@Composable () -> Unit)? = null,
    showHistory: Boolean = true,
    insetStatusBar: Boolean = true,
    /** A top-level place: on a phone, a button opens the sidebar where the back arrow would be. */
    sidebar: Boolean = false,
    actions: @Composable RowScope.() -> Unit = {},
) {
    val shell = LocalShell.current
    val openSidebar = shell.openSidebar.takeIf { sidebar && navigation == null }
    Column(
        modifier
            .fillMaxWidth()
            .background(TaktTheme.colors.well)
            .then(if (insetStatusBar) Modifier.windowInsetsPadding(WindowInsets.statusBars.only(WindowInsetsSides.Top)) else Modifier),
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = Metrics.headerHeight)
                .padding(start = if (navigation == null && openSidebar == null) Metrics.md else Metrics.xxs, end = Metrics.xxs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            when {
                navigation != null -> navigation()
                openSidebar != null -> IconAction(PIcons.Sidebar, "Lists", tint = TaktTheme.colors.icon, onClick = openSidebar, modifier = Modifier.testTag("open_sidebar"))
            }
            Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    title, style = TaktTheme.type.body, color = TaktTheme.colors.ink,
                    maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false),
                )
                if (subtitle != null) {
                    Spacer(Modifier.width(Metrics.sm))
                    Text(subtitle, style = TaktTheme.type.secondary, color = TaktTheme.colors.dimText, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            actions()
            if (showHistory) HistoryActions(compact = !shell.layout.isWide)
        }
        Hairline()
    }
}

/** Toolbar undo, redo and the history sheet, labelled with what each would do. */
@Composable
fun HistoryActions(compact: Boolean = false) {
    val shell = LocalShell.current
    val labels by shell.container.undo.labels.collectAsStateWithLifecycle()
    IconAction(
        PIcons.Undo,
        labels.undo?.let { "Undo $it" } ?: "Nothing to undo",
        enabled = labels.undo != null,
        onClick = { shell.container.undo.undo() },
    )
    // A phone keeps only undo in its bar; redo and the history are in the sidebar.
    if (compact) return
    IconAction(
        PIcons.Redo,
        labels.redo?.let { "Redo $it" } ?: "Nothing to redo",
        enabled = labels.redo != null,
        onClick = { shell.container.undo.redo() },
    )
    IconAction(PIcons.History, "History", onClick = { shell.showHistory() })
}

/**
 * An icon button with no fill, as the Mac's bar buttons: an 18dp icon in a
 * 40dp square at the item radius, the press its hover fill. Compose grows
 * the hit area of anything under 48dp to 48 where nothing else claims the
 * touch, so the bar keeps the Mac's height without a smaller target.
 */
@Composable
fun IconAction(
    icon: ImageVector,
    contentDescription: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    tint: Color = TaktTheme.colors.icon,
    onClick: () -> Unit,
) {
    Box(
        modifier
            .size(Metrics.headerHeight)
            .clip(Metrics.card)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .semantics { this.contentDescription = contentDescription },
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = null, tint = if (enabled) tint else TaktTheme.colors.dimText, modifier = Modifier.size(Metrics.barIcon))
    }
}

/** The settings gear for a top bar. */
@Composable
fun SettingsAction() {
    val shell = LocalShell.current
    IconAction(Icons.Filled.Settings, "Settings", onClick = { shell.navigator.openSettings() })
}

/**
 * A list's section heading, as the Mac's: secondary size, dim, sentence case,
 * "Title · n", no rule; 12 in from the side, 12 above and 4 below. An
 * optional control sits at the end.
 */
@Composable
fun SectionHeader(
    text: String,
    modifier: Modifier = Modifier,
    detail: String? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    Row(
        modifier
            .fillMaxWidth()
            .padding(start = Metrics.md, end = if (trailing == null) Metrics.md else Metrics.xs, top = Metrics.md, bottom = Metrics.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            if (detail == null) text else "$text · $detail",
            style = TaktTheme.type.secondary,
            color = TaktTheme.colors.dimText,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        trailing?.invoke()
    }
}

/**
 * The micro-label: the theme's small bold capitals, in its role (the Mac's
 * 10 bold, dim). Settings group headings and matrix quadrant titles.
 */
@Composable
fun MicroLabel(text: String, modifier: Modifier = Modifier, color: Color? = null) {
    val label = TaktTheme.microLabel
    Text(
        if (label.uppercase) text.uppercase() else text,
        style = label.style,
        color = color ?: TaktTheme.colors.role(label.role),
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = modifier,
    )
}

/**
 * A task's chip as the Mac draws one: bare secondary-size text in one hue,
 * no box (blocked, ⌛ who, #tag, ↻, P1, a due day, done/total, ¶).
 */
@Composable
fun Chip(text: String, color: Color, modifier: Modifier = Modifier, mono: Boolean = false) {
    Text(
        text,
        style = if (mono) TaktTheme.type.monoSecondary else TaktTheme.type.secondary,
        color = color,
        maxLines = 1,
        softWrap = false,
        modifier = modifier,
    )
}

/**
 * A squarish tag at the item radius: tinted fill, border and text of one
 * hue, never a capsule.
 * [selected] fills it a little more; [onClick] makes it a 48dp-tall target.
 */
@Composable
fun Tag(
    text: String,
    modifier: Modifier = Modifier,
    color: Color = TaktTheme.colors.mutedText,
    selected: Boolean = false,
    mono: Boolean = false,
    leading: ImageVector? = null,
    onClick: (() -> Unit)? = null,
) {
    val shape = Metrics.card
    val fill = if (selected) color.copy(alpha = 0.18f) else color.copy(alpha = 0.08f)
    val border = if (selected) color.copy(alpha = 0.7f) else color.copy(alpha = 0.35f)
    val content = @Composable {
        Row(
            Modifier
                .clip(shape)
                .background(fill)
                .border(BorderStroke(Metrics.hairline, border), shape)
                .padding(horizontal = Metrics.sm, vertical = Metrics.xxs + Metrics.hairline),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Metrics.xs),
        ) {
            if (leading != null) Icon(leading, null, tint = color, modifier = Modifier.size(14.dp))
            Text(text, style = if (mono) TaktTheme.type.monoSecondary else TaktTheme.type.secondary, color = if (color == TaktTheme.colors.mutedText) TaktTheme.colors.ink else color, maxLines = 1)
        }
    }
    if (onClick == null) {
        Box(modifier) { content() }
    } else {
        Box(
            modifier
                .sizeIn(minHeight = Metrics.touchTarget)
                .clip(shape)
                .clickable(role = Role.Button, onClick = onClick),
            contentAlignment = Alignment.Center,
        ) { content() }
    }
}

/**
 * A button as the Mac's: 30dp (its 26) at the popover radius, a hairline
 * border on the panel fill, the press its hover. The main action is the
 * selection's fill with an accent border; a destructive one is set in the
 * danger hue. Under 48dp, its hit area is grown by Compose, not painted.
 */
@Composable
fun PButton(
    text: String,
    modifier: Modifier = Modifier,
    primary: Boolean = false,
    destructive: Boolean = false,
    enabled: Boolean = true,
    icon: ImageVector? = null,
    contentPadding: PaddingValues = PaddingValues(horizontal = Metrics.md),
    onClick: () -> Unit,
) {
    val colors = TaktTheme.colors
    val accent = if (destructive) colors.danger else colors.primary
    val fill = when {
        !enabled -> colors.altRow
        primary -> colors.selected
        else -> colors.altRow
    }
    val textColor = when {
        !enabled -> colors.dimText
        destructive -> colors.danger
        else -> colors.ink
    }
    Row(
        modifier
            .heightIn(min = Metrics.controlHeight)
            .clip(Metrics.popover)
            .background(fill)
            .border(BorderStroke(Metrics.hairline, if (primary && enabled) accent else colors.border), Metrics.popover)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(contentPadding),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Metrics.xs + Metrics.xxs, Alignment.CenterHorizontally),
    ) {
        if (icon != null) Icon(icon, null, tint = if (enabled && !destructive) colors.icon else textColor, modifier = Modifier.size(16.dp))
        Text(text, style = TaktTheme.type.small, color = textColor, maxLines = 1)
    }
}

/**
 * A switch as the Mac's settings draw one, a little larger: a 34×20 track
 * and a 14dp knob. On is the accent with a paper knob; off is the hover fill
 * in a border outline with a muted knob. Only the knob and track are round.
 */
@Composable
fun PSwitch(
    checked: Boolean,
    onCheckedChange: ((Boolean) -> Unit)?,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    val colors = TaktTheme.colors
    val track = if (checked) colors.primary else colors.hover
    val edge = if (checked) colors.primary else colors.border
    val knob = if (checked) colors.paper else colors.mutedText
    val clickable = if (onCheckedChange != null) {
        Modifier.toggleable(value = checked, enabled = enabled, role = Role.Switch, onValueChange = onCheckedChange)
    } else {
        Modifier
    }
    Box(
        modifier
            .then(clickable)
            .alpha(if (enabled) 1f else TaktTheme.DISABLED_ALPHA)
            .size(width = 34.dp, height = 20.dp)
            .clip(Metrics.pill)
            .background(track)
            .border(BorderStroke(Metrics.hairline, edge), Metrics.pill)
            .padding(3.dp),
        contentAlignment = if (checked) Alignment.CenterEnd else Alignment.CenterStart,
    ) {
        Box(Modifier.size(14.dp).clip(Metrics.pill).background(knob))
    }
}

/** A quiet centred message for an empty screen or section. */
@Composable
fun EmptyState(title: String, modifier: Modifier = Modifier, detail: String? = null, action: (@Composable () -> Unit)? = null) {
    Column(
        modifier.fillMaxWidth().padding(horizontal = Metrics.xl, vertical = Metrics.xl + Metrics.lg),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(Metrics.sm),
    ) {
        Text(title, style = TaktTheme.type.heading, color = TaktTheme.colors.ink)
        if (detail != null) Text(detail, style = TaktTheme.type.small, color = TaktTheme.colors.mutedText)
        action?.invoke()
    }
}

/**
 * The task's status mark with its action: the Mac's glyph ([TaskBox]) in a
 * strip as tall as the row and [width] wide, so the mark stays small and the
 * title stays near the edge while a finger still finds it.
 */
@Composable
fun TaskCheck(
    status: TaskStatus,
    modifier: Modifier = Modifier,
    isList: Boolean = false,
    size: Dp = Metrics.statusMark,
    /** The task's title, so the action reads "Complete Write report" to TalkBack and tests. */
    label: String? = null,
    width: Dp = 28.dp,
    onToggle: () -> Unit,
) {
    val verb = when (status) {
        TaskStatus.OPEN -> "Complete"
        TaskStatus.COMPLETED -> "Reopen"
        TaskStatus.CANCELLED -> "Reinstate"
    }
    val description = if (label == null) verb else "$verb $label"
    val haptics = LocalHapticFeedback.current
    Box(
        modifier
            .width(width)
            .heightIn(min = Metrics.touchTarget)
            .clickable(role = Role.Checkbox, indication = null, interactionSource = null) {
                haptics.performHapticFeedback(if (status == TaskStatus.OPEN) HapticFeedbackType.ToggleOn else HapticFeedbackType.ToggleOff)
                onToggle()
            }
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) {
        TaskBox(status, isList = isList, size = size)
    }
}

/**
 * A task's status mark without its action, as the Mac draws it: a glyph,
 * not a checkbox. Cancelled is ✕ and a list ≡; a finished task has no mark
 * (its title is struck through instead). An open task, which on the Mac has
 * none either, gets a 12dp hairline square at radius 2 in the dim text, the
 * one affordance a phone without a keyboard needs to finish it.
 */
@Composable
fun TaskBox(status: TaskStatus, isList: Boolean = false, size: Dp = Metrics.statusMark) {
    val colors = TaktTheme.colors
    Box(Modifier.size(size), contentAlignment = Alignment.Center) {
        when {
            status == TaskStatus.CANCELLED -> Text("✕", style = TaktTheme.type.secondary, color = colors.dimText, maxLines = 1)
            status == TaskStatus.COMPLETED -> Unit
            isList -> Text("≡", style = TaktTheme.type.secondary, color = colors.mutedText, maxLines = 1)
            else -> Box(
                Modifier
                    .size(size * (12f / 14f))
                    .border(BorderStroke(1.dp, colors.dimText), RoundedCornerShape(2.dp)),
            )
        }
    }
}

/** `!1` … `!4`, coloured raspberry, amber, azure, muted; `!-4` … `!-1`, which mean later, muted too. */
@Composable
fun priorityColor(priority: Int?): Color = when (priority) {
    1 -> TaktTheme.colors.danger
    2 -> TaktTheme.colors.warning
    3 -> TaktTheme.colors.primary
    else -> TaktTheme.colors.mutedText
}

/**
 * A card as the Mac's board draws one: the paper on its panel, a muted
 * hairline, the item radius, no shadow; selected, the selection's fill with
 * the focus outline.
 */
@Composable
fun Card(modifier: Modifier = Modifier, selected: Boolean = false, content: @Composable () -> Unit) {
    val colors = TaktTheme.colors
    Box(
        modifier
            .clip(Metrics.card)
            .background(if (selected) colors.selected else colors.paper)
            .border(BorderStroke(Metrics.hairline, if (selected) colors.focus else colors.borderMuted), Metrics.card),
    ) { content() }
}

/** Monospaced numbers: counts, clocks, estimates. */
@Composable
fun MonoText(text: String, modifier: Modifier = Modifier, style: TextStyle = TaktTheme.type.mono, color: Color = TaktTheme.colors.mutedText) {
    Text(text, modifier = modifier, style = style, color = color, maxLines = 1)
}
