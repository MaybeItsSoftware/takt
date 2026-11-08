package uk.co.maybeitsadam.takt.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
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
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Settings
import uk.co.maybeitsadam.takt.ui.theme.PIcons
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
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
 * The header band every screen starts with: one height across panes, a title,
 * an optional muted subtitle, trailing actions, and a hairline beneath. It
 * pads itself below the status bar, so screens draw edge to edge.
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
            .background(TaktTheme.colors.paper)
            .then(if (insetStatusBar) Modifier.windowInsetsPadding(WindowInsets.statusBars.only(WindowInsetsSides.Top)) else Modifier),
    ) {
        // One line, as the Mac's title bar: the place, a muted note beside it, then its tools.
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = Metrics.headerHeight)
                .padding(start = if (navigation == null && openSidebar == null) Metrics.lg else Metrics.xxs, end = Metrics.xxs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            when {
                navigation != null -> navigation()
                openSidebar != null -> IconAction(PIcons.Sidebar, "Lists", tint = TaktTheme.colors.ink, onClick = openSidebar, modifier = Modifier.testTag("open_sidebar"))
            }
            Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    title, style = TaktTheme.type.bodyStrong, color = TaktTheme.colors.ink,
                    maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false),
                )
                if (subtitle != null) {
                    Spacer(Modifier.width(Metrics.sm))
                    Text(subtitle, style = TaktTheme.type.small, color = TaktTheme.colors.mutedText, maxLines = 1, overflow = TextOverflow.Ellipsis)
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

/** A 48dp icon button with no fill: the toolbar's only button style. */
@Composable
fun IconAction(
    icon: ImageVector,
    contentDescription: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    tint: Color = TaktTheme.colors.mutedText,
    onClick: () -> Unit,
) {
    IconButton(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier.size(Metrics.touchTarget),
        colors = IconButtonDefaults.iconButtonColors(contentColor = tint, disabledContentColor = TaktTheme.colors.dimText),
    ) {
        Icon(icon, contentDescription = contentDescription, modifier = Modifier.size(20.dp))
    }
}

/** The settings gear for a top bar. */
@Composable
fun SettingsAction() {
    val shell = LocalShell.current
    IconAction(Icons.Filled.Settings, "Settings", onClick = { shell.navigator.openSettings() })
}

/** A sentence-case muted label over a section, with an optional trailing control. */
@Composable
fun SectionHeader(
    text: String,
    modifier: Modifier = Modifier,
    detail: String? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    Row(
        modifier.fillMaxWidth().heightIn(min = 36.dp).padding(horizontal = Metrics.lg),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text, style = TaktTheme.type.label, color = TaktTheme.colors.mutedText, modifier = Modifier.weight(1f, fill = false))
        if (detail != null) {
            Spacer(Modifier.width(Metrics.sm))
            Text(detail, style = TaktTheme.type.monoSmall, color = TaktTheme.colors.dimText)
        }
        Spacer(Modifier.weight(1f))
        trailing?.invoke()
    }
}

/**
 * A squarish 6dp tag: tinted fill, border and text of one hue, never a capsule.
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
    val shape = Metrics.control
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
            Text(text, style = if (mono) TaktTheme.type.monoSmall.copy(fontSize = TaktTheme.type.small.fontSize) else TaktTheme.type.small, color = if (color == TaktTheme.colors.mutedText) TaktTheme.colors.ink else color, maxLines = 1)
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

/** A flat button: filled primary for the main action, bordered otherwise. 6dp, 48dp tall, no elevation. */
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
        !enabled -> colors.well
        primary -> accent
        else -> colors.raised
    }
    val textColor = when {
        !enabled -> colors.dimText
        primary -> colors.onAccent
        destructive -> colors.danger
        else -> colors.ink
    }
    Row(
        modifier
            .heightIn(min = Metrics.touchTarget)
            .clip(Metrics.control)
            .background(fill)
            .border(BorderStroke(Metrics.hairline, if (primary && enabled) accent else colors.inputBorder), Metrics.control)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(contentPadding),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Metrics.xs + Metrics.xxs, Alignment.CenterHorizontally),
    ) {
        if (icon != null) Icon(icon, null, tint = textColor, modifier = Modifier.size(18.dp))
        Text(text, style = TaktTheme.type.bodyStrong.copy(fontSize = TaktTheme.type.small.fontSize.times(1.05f)), color = textColor, maxLines = 1)
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
 * The task's tick: [TaskBox] — a hairline square, never a circle — inside a
 * 48dp target. The painted box is 20dp.
 */
@Composable
fun TaskCheck(
    status: TaskStatus,
    modifier: Modifier = Modifier,
    isList: Boolean = false,
    size: Dp = 20.dp,
    /** The task's title, so the action reads "Complete Write report" to TalkBack and tests. */
    label: String? = null,
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
            .size(Metrics.touchTarget)
            .clip(Metrics.control)
            .clickable(role = Role.Checkbox) {
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
 * A task's completion box without its action: a hairline square at the
 * control radius scaled down to the box (never round), empty when open; done
 * is the box tinted emerald with its border and a check, cancelled the same in
 * raspberry with a cross. A list's open box holds the outline glyph, so a list
 * still reads as a list beside its tasks.
 */
@Composable
fun TaskBox(status: TaskStatus, isList: Boolean = false, size: Dp = 20.dp) {
    val colors = TaktTheme.colors
    val shape = RoundedCornerShape(minOf(Metrics.controlRadius, size / 4))
    val hue = when (status) {
        TaskStatus.OPEN -> null
        TaskStatus.COMPLETED -> colors.success
        TaskStatus.CANCELLED -> colors.danger
    }
    Box(
        Modifier
            .size(size)
            .clip(shape)
            .background(hue?.copy(alpha = 0.10f) ?: Color.Transparent)
            .border(BorderStroke(if (hue == null) 1.5.dp else Metrics.hairline.coerceAtLeast(1.dp), hue?.copy(alpha = 0.4f) ?: colors.inputBorder), shape),
        contentAlignment = Alignment.Center,
    ) {
        when (status) {
            TaskStatus.OPEN -> if (isList) Icon(PIcons.Outline, null, tint = colors.mutedText, modifier = Modifier.size(size * 0.6f))
            TaskStatus.COMPLETED -> Icon(Icons.Filled.Check, null, tint = colors.success, modifier = Modifier.size(size * 0.7f))
            TaskStatus.CANCELLED -> Icon(Icons.Filled.Close, null, tint = colors.danger, modifier = Modifier.size(size * 0.7f))
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

/** A raised card: white on paper, a hairline border, 8dp corners, no shadow. */
@Composable
fun Card(modifier: Modifier = Modifier, selected: Boolean = false, content: @Composable () -> Unit) {
    Box(
        modifier
            .clip(Metrics.card)
            .background(TaktTheme.colors.raised)
            .border(BorderStroke(Metrics.hairline, if (selected) TaktTheme.colors.primary else TaktTheme.colors.border), Metrics.card),
    ) { content() }
}

/** Monospaced numbers: counts, clocks, estimates. */
@Composable
fun MonoText(text: String, modifier: Modifier = Modifier, style: TextStyle = TaktTheme.type.mono, color: Color = TaktTheme.colors.mutedText) {
    Text(text, modifier = modifier, style = style, color = color, maxLines = 1)
}
