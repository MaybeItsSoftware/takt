package uk.co.maybeitsadam.takt.ui.lists

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.BasicAlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import uk.co.maybeitsadam.takt.core.TaskList
import uk.co.maybeitsadam.takt.core.TaskListRole
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.PIcons

/** Icons this slice needs beyond the shared set. */
internal object ListIcons {
    val Pin by lazy { PIcons.icon("Pin", "M16,9V4h1c0.55,0 1,-0.45 1,-1s-0.45,-1 -1,-1H7C6.45,2 6,2.45 6,3s0.45,1 1,1h1v5c0,1.66 -1.34,3 -3,3v2h5.97v7l1,1 1,-1v-7H19v-2c-1.66,0 -3,-1.34 -3,-3z") }
    val Everything by lazy { PIcons.icon("Everything", "M3,3h18v4H3zM3,10h18v4H3zM3,17h18v4H3z") }
    val Filter by lazy { PIcons.icon("Filter", "M10,18h4v-2h-4v2zM3,6v2h18V6H3zM6,13h12v-2H6v2z") }
    val Nested by lazy { PIcons.icon("Nested", "M4,5h2v2H4zM9,5h11v2H9zM8,11h2v2H8zM13,11h7v2h-7zM8,17h2v2H8zM13,17h7v2h-7z") }
    val FolderOpen by lazy { PIcons.icon("FolderOpen", "M20,6h-8l-2,-2H4c-1.1,0 -1.99,0.9 -1.99,2L2,18c0,1.1 0.9,2 2,2h16c1.1,0 2,-0.9 2,-2V8c0,-1.1 -0.9,-2 -2,-2zM20,18H4V8h16v10z") }
    val Restore by lazy { PIcons.icon("Restore", "M13,3c-4.97,0 -9,4.03 -9,9H1l4,4 4,-4H6c0,-3.87 3.13,-7 7,-7s7,3.13 7,7 -3.13,7 -7,7c-1.93,0 -3.68,-0.79 -4.94,-2.06l-1.42,1.42C8.27,19.99 10.51,21 13,21c4.97,0 9,-4.03 9,-9s-4.03,-9 -9,-9z") }
}

/**
 * A flat dropdown as the Mac's context menu: the panel fill, a hairline, the
 * popover radius (6), 4 of padding, no elevation.
 */
@Composable
internal fun PMenu(
    expanded: Boolean,
    onDismiss: () -> Unit,
    offset: DpOffset = DpOffset(0.dp, 0.dp),
    content: @Composable ColumnScope.() -> Unit,
) {
    DropdownMenu(
        expanded = expanded,
        onDismissRequest = onDismiss,
        offset = offset,
        shape = Metrics.popover,
        containerColor = TaktTheme.colors.altRow,
        tonalElevation = 0.dp,
        shadowElevation = 0.dp,
        border = BorderStroke(Metrics.hairline, TaktTheme.colors.border),
    ) {
        Column(Modifier.padding(horizontal = Metrics.xs)) { content() }
    }
}

/**
 * A menu item as the Mac's: a row at the item radius, its press the hover
 * fill; an icon in the icon colour, the words in ink (danger when
 * destructive), and a key or note on the right in the mono face, dim, at
 * the secondary size. Tall as the touch target, since items stack.
 */
@Composable
internal fun PMenuItem(
    text: String,
    icon: ImageVector? = null,
    destructive: Boolean = false,
    enabled: Boolean = true,
    trailing: String? = null,
    selected: Boolean = false,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    val colors = TaktTheme.colors
    val color = when {
        !enabled -> colors.dimText
        destructive -> colors.danger
        else -> colors.ink
    }
    Row(
        modifier
            .widthIn(min = 180.dp)
            .heightIn(min = Metrics.touchTarget)
            .clip(Metrics.card)
            .background(if (selected) colors.selected else Color.Transparent)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = Metrics.sm),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
    ) {
        if (icon != null) Icon(icon, null, tint = if (destructive) colors.danger else colors.icon, modifier = Modifier.size(16.dp))
        Text(text, style = TaktTheme.type.body, color = color, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
        if (trailing != null) {
            Spacer(Modifier.weight(1f))
            Text(trailing, style = TaktTheme.type.monoSecondary, color = colors.dimText, maxLines = 1)
        }
    }
}

@Composable
internal fun PMenuDivider() {
    Hairline(Modifier.padding(vertical = Metrics.xs), color = TaktTheme.colors.borderMuted)
}

/**
 * A small flat dialog as the Mac's sheets: the panel fill, a hairline, the
 * sheet radius (8), 16 of padding and 12 between; the title at 17.
 */
@Composable
internal fun PDialog(
    title: String,
    onDismiss: () -> Unit,
    buttons: @Composable () -> Unit,
    body: @Composable ColumnScope.() -> Unit,
) {
    BasicAlertDialog(onDismissRequest = onDismiss) {
        Column(
            Modifier
                .clip(Metrics.sheet)
                .background(TaktTheme.colors.altRow)
                .border(BorderStroke(Metrics.hairline, TaktTheme.colors.border), Metrics.sheet)
                .padding(Metrics.lg),
            verticalArrangement = Arrangement.spacedBy(Metrics.md),
        ) {
            Text(title, style = TaktTheme.type.title.copy(fontWeight = FontWeight.Medium), color = TaktTheme.colors.ink)
            body()
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(Metrics.sm, Alignment.End)) { buttons() }
        }
    }
}

/**
 * A single-line field as the Mac's inline field: the paper in a hairline at
 * the item radius; 16sp (no iOS-style zoom on Android either), and as tall
 * as a finger needs.
 */
@Composable
internal fun PTextField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    modifier: Modifier = Modifier,
    imeAction: ImeAction = ImeAction.Done,
    onSubmit: () -> Unit = {},
) {
    BasicTextField(
        value = value,
        onValueChange = onValueChange,
        singleLine = true,
        textStyle = TaktTheme.type.field.copy(color = TaktTheme.colors.ink),
        cursorBrush = SolidColor(TaktTheme.colors.primary),
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = imeAction),
        keyboardActions = KeyboardActions(onAny = { onSubmit() }),
        modifier = modifier,
        decorationBox = { inner ->
            Box(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = Metrics.touchTarget)
                    .clip(Metrics.card)
                    .background(TaktTheme.colors.paper)
                    .border(BorderStroke(Metrics.hairline, TaktTheme.colors.border), Metrics.card)
                    .padding(horizontal = Metrics.sm + Metrics.xxs),
                contentAlignment = Alignment.CenterStart,
            ) {
                if (value.isEmpty()) Text(placeholder, style = TaktTheme.type.field, color = TaktTheme.colors.dimText)
                inner()
            }
        },
    )
}

/** Asks for a name: new list, new folder, rename, new column, new list for a move. */
@Composable
internal fun NamePromptDialog(
    title: String,
    initial: String = "",
    confirm: String = "Save",
    placeholder: String = "Name",
    onDismiss: () -> Unit,
    onConfirm: (String) -> Unit,
) {
    var text by remember { mutableStateOf(initial) }
    val focus = remember { FocusRequester() }
    val submit = {
        if (text.isNotBlank()) {
            onConfirm(text.trim())
            onDismiss()
        }
    }
    PDialog(
        title = title,
        onDismiss = onDismiss,
        buttons = {
            PButton("Cancel", onClick = onDismiss)
            PButton(confirm, primary = true, enabled = text.isNotBlank(), onClick = submit)
        },
    ) {
        PTextField(text, { text = it }, placeholder, Modifier.fillMaxWidth().focusRequester(focus).testTag("name_prompt_field"), onSubmit = submit)
    }
    LaunchedEffect(Unit) { focus.requestFocus() }
}

@Composable
internal fun ConfirmDialog(
    title: String,
    message: String,
    confirm: String = "Delete",
    destructive: Boolean = true,
    onDismiss: () -> Unit,
    onConfirm: () -> Unit,
) {
    PDialog(
        title = title,
        onDismiss = onDismiss,
        buttons = {
            PButton("Cancel", onClick = onDismiss)
            PButton(confirm, primary = true, destructive = destructive, onClick = {
                onConfirm()
                onDismiss()
            })
        },
    ) {
        Text(message, style = TaktTheme.type.body, color = TaktTheme.colors.mutedText)
    }
}

/**
 * Where to move a task: every open list, the one it is in marked, or a new
 * one named on the spot. The list screens' and Today's "Move to list…".
 */
@Composable
internal fun MoveToListSheet(
    title: String,
    currentListId: String?,
    lists: List<TaskList>,
    onDismiss: () -> Unit,
    onMove: (String) -> Unit,
    onMoveToNew: (String) -> Unit,
) {
    var creating by remember { mutableStateOf(false) }
    if (creating) {
        NamePromptDialog("New list", confirm = "Move", onDismiss = onDismiss, onConfirm = onMoveToNew)
        return
    }
    val choices = remember(lists, currentListId) {
        (
            listOf(Choice(NEW_LIST_CHOICE, "New list…", icon = Icons.Filled.Add)) +
                lists.filter { it.completedAt == null }.map {
                    Choice(
                        it.id, it.name, icon = if (it.systemRole == TaskListRole.INBOX) PIcons.Inbox else PIcons.ListIcon,
                        isCurrent = it.id == currentListId,
                    )
                }
            ).toImmutableList()
    }
    ChoiceSheet("Move $title to", choices, onDismiss = { if (!creating) onDismiss() }) { choice ->
        if (choice.id == NEW_LIST_CHOICE) creating = true else choice.id?.let(onMove)
    }
}

private const val NEW_LIST_CHOICE = "\u0000new"

/** One choice in a picker sheet. */
@Immutable
data class Choice(
    val id: String?,
    val title: String,
    val detail: String? = null,
    val depth: Int = 0,
    val icon: ImageVector? = null,
    val isCurrent: Boolean = false,
    val tint: Color? = null,
)

/** A bottom sheet of choices: a folder, a list, a column, a quadrant. */
@Composable
internal fun ChoiceSheet(
    title: String,
    choices: ImmutableList<Choice>,
    onDismiss: () -> Unit,
    leading: (@Composable () -> Unit)? = null,
    onChoose: (Choice) -> Unit,
) {
    val state = rememberModalBottomSheetState(skipPartiallyExpanded = false)
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = state,
        containerColor = TaktTheme.colors.altRow,
        tonalElevation = 0.dp,
        shape = Metrics.sheetTop,
    ) {
        Column(Modifier.navigationBarsPadding().imePadding()) {
            Text(title, style = TaktTheme.type.title.copy(fontWeight = FontWeight.Medium), color = TaktTheme.colors.ink, modifier = Modifier.padding(horizontal = Metrics.lg, vertical = Metrics.sm))
            Hairline()
            leading?.invoke()
            LazyColumn(Modifier.fillMaxWidth().testTag("choice_sheet")) {
                items(choices, key = { "${it.id}:${it.title}" }, contentType = { "choice" }) { choice ->
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .padding(horizontal = Metrics.xs)
                            .heightIn(min = Metrics.touchTarget)
                            .clip(Metrics.card)
                            .background(if (choice.isCurrent) TaktTheme.colors.selected else Color.Transparent)
                            .clickable(enabled = !choice.isCurrent, role = Role.Button) {
                                onChoose(choice)
                                onDismiss()
                            }
                            .padding(start = Metrics.md + Metrics.indent * choice.depth, end = Metrics.md),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        if (choice.icon != null) {
                            Icon(choice.icon, null, tint = choice.tint ?: TaktTheme.colors.icon, modifier = Modifier.size(16.dp))
                            Spacer(Modifier.width(Metrics.sm))
                        }
                        Column(Modifier.weight(1f)) {
                            Text(
                                choice.title, style = TaktTheme.type.body,
                                color = if (choice.isCurrent) TaktTheme.colors.dimText else TaktTheme.colors.ink,
                                maxLines = 1, overflow = TextOverflow.Ellipsis,
                            )
                            if (choice.detail != null) Text(choice.detail, style = TaktTheme.type.secondary, color = TaktTheme.colors.dimText, maxLines = 1)
                        }
                        if (choice.isCurrent) Text("Current", style = TaktTheme.type.secondary, color = TaktTheme.colors.dimText)
                    }
                }
            }
        }
    }
}
