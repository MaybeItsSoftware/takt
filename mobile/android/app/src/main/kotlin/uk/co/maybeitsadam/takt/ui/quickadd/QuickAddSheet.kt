package uk.co.maybeitsadam.takt.ui.quickadd

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.delay
import uk.co.maybeitsadam.takt.app.QuickAddRequest
import uk.co.maybeitsadam.takt.ui.SheetHandle
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.components.Tag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import uk.co.maybeitsadam.takt.ui.inspector.ChalkField
import uk.co.maybeitsadam.takt.ui.inspector.ChalkMenuItem
import uk.co.maybeitsadam.takt.ui.inspector.FieldLabel
import uk.co.maybeitsadam.takt.ui.inspector.MenuField
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.PIcons

/**
 * Quick add: one field that reads `45m #work ^mon @fri !1` off the end of what you
 * type and shows it as chips before you add, and where it goes. It stays open
 * after Add so a run of tasks can go in one sitting.
 */
@Composable
fun QuickAddSheet(request: QuickAddRequest, onDismiss: () -> Unit) {
    val container = LocalShell.current.container
    val vm: QuickAddViewModel = viewModel(key = "quick_add") { QuickAddViewModel(container) }
    val targets by vm.targets.collectAsStateWithLifecycle()
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)

    var text by rememberSaveable(request.nonce) { mutableStateOf(request.text) }
    var chosenListId by rememberSaveable(request.nonce) { mutableStateOf(request.listId) }
    var plansToday by rememberSaveable(request.nonce) { mutableStateOf(request.plansToday) }
    val added = remember(request.nonce) { mutableStateListOf<String>() }
    val preview by remember { derivedStateOf { CapturePreview.of(text) } }
    val targetId = chosenListId?.takeIf { id -> targets.lists.any { it.id == id } } ?: targets.inboxId
    val target = targets.named(targetId)

    val focus = remember { FocusRequester() }
    val keyboard = LocalSoftwareKeyboardController.current
    LaunchedEffect(request.nonce) {
        // The sheet animates in first; focusing before it is attached does nothing.
        delay(150)
        runCatching { focus.requestFocus() }
        keyboard?.show()
    }

    fun submit() {
        val listId = targetId ?: return
        if (!preview.canAdd) return
        // A parent only makes sense in the list it was asked for.
        val parent = request.parentTaskId?.takeIf { listId == (request.listId ?: targets.inboxId) }
        vm.add(text, listId, parent, plansToday)
        added.add(0, preview.title)
        text = ""
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = TaktTheme.colors.altRow,
        contentColor = TaktTheme.colors.ink,
        tonalElevation = 0.dp,
        shape = Metrics.sheetTop,
        dragHandle = { SheetHandle() },
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .imePadding()
                .navigationBarsPadding()
                .padding(horizontal = Metrics.lg)
                .padding(bottom = Metrics.lg)
                .testTag("quick_add_sheet"),
            verticalArrangement = Arrangement.spacedBy(Metrics.sm),
        ) {
            Text("Add a task", style = TaktTheme.type.title, color = TaktTheme.colors.ink)
            ChalkField(
                text,
                onValueChange = { text = it.replace("\n", " ") },
                placeholder = "What needs doing?",
                style = TaktTheme.type.body.copy(fontSize = TaktTheme.type.heading.fontSize),
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { submit() }),
                leading = { Icon(PIcons.Inbox, null, tint = TaktTheme.colors.mutedText, modifier = Modifier.size(18.dp)) },
                modifier = Modifier
                    .focusRequester(focus)
                    .onPreviewKeyEvent { event ->
                        val enter = event.key == Key.Enter || event.key == Key.NumPadEnter
                        if (enter && event.type == KeyEventType.KeyDown) {
                            submit()
                            true
                        } else {
                            enter
                        }
                    }
                    .testTag("quick_add_field"),
                contentDescription = "New task",
            )
            // The parsed result: the title that will be saved, then a chip per token.
            Row(
                Modifier.fillMaxWidth().heightIn(min = 28.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
            ) {
                if (preview.chips.isNotEmpty()) {
                    Text(
                        preview.title,
                        style = TaktTheme.type.small,
                        color = TaktTheme.colors.mutedText,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f, fill = false).testTag("quick_add_title_preview"),
                    )
                    CaptureChipRow(preview, Modifier.testTag("quick_add_chips"))
                }
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
                FieldLabel("Into")
                var open by remember { mutableStateOf(false) }
                MenuField(
                    target?.name ?: "Inbox",
                    expanded = open,
                    onExpandedChange = { open = it },
                    leading = if (target?.isInbox != false) PIcons.Inbox else PIcons.ListIcon,
                    contentDescription = "Add to ${target?.name ?: "Inbox"}",
                    modifier = Modifier.weight(1f).testTag("quick_add_target"),
                ) {
                    for (option in targets.lists) {
                        ChalkMenuItem(option.name, selected = option.id == targetId, onClick = {
                            chosenListId = option.id
                            open = false
                        })
                    }
                }
                PButton(
                    "Add",
                    primary = true,
                    enabled = preview.canAdd && targetId != null,
                    modifier = Modifier.testTag("quick_add_submit"),
                    onClick = ::submit,
                )
            }
            Tag(
                "Plan for today",
                selected = plansToday,
                color = if (plansToday) TaktTheme.colors.primary else TaktTheme.colors.mutedText,
                leading = if (plansToday) Icons.Filled.Check else PIcons.Today,
                modifier = Modifier
                    .heightIn(min = Metrics.touchTarget)
                    .testTag("quick_add_plan_today")
                    .semantics { contentDescription = if (plansToday) "Plan for today, on" else "Plan for today, off" },
                onClick = { plansToday = !plansToday },
            )
            Text(CapturePreview.HINT, style = TaktTheme.type.small, color = TaktTheme.colors.dimText)
            if (added.isNotEmpty()) {
                FieldLabel("Added")
                for ((index, title) in added.take(5).withIndex()) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
                        modifier = Modifier.testTag("quick_add_added_$index"),
                    ) {
                        Icon(Icons.Filled.Check, null, tint = TaktTheme.colors.success, modifier = Modifier.size(16.dp))
                        Text(title, style = TaktTheme.type.small, color = TaktTheme.colors.mutedText, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
        }
    }
}
