package uk.co.maybeitsadam.takt.ui.history

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import uk.co.maybeitsadam.takt.app.AppContainer
import uk.co.maybeitsadam.takt.data.workspace.HistoryEntry
import uk.co.maybeitsadam.takt.data.workspace.observeHistory
import uk.co.maybeitsadam.takt.ui.SheetHandle
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.MonoText
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.components.SectionHeader
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.PIcons

/** The journal split for drawing: steps redo would bring back, and steps undo would take back. */
@Immutable
data class HistoryView(
    val entries: ImmutableList<HistoryEntry> = persistentListOf(),
) {
    /** Newest first, so the next redo is the last of these. */
    val undone: List<HistoryEntry> get() = entries.filter { it.isUndone }
    val applied: List<HistoryEntry> get() = entries.filter { !it.isUndone }
}

class HistoryViewModel(private val container: AppContainer) : ViewModel() {
    val history: StateFlow<HistoryView> = container.withSession { it.repository.observeHistory() }
        .map { HistoryView(it.toImmutableList()) }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), HistoryView())

    fun travel(entry: HistoryEntry) = container.undo.travel(entry, history.value.entries)

    fun undo() = container.undo.undo()

    fun redo() = container.undo.redo()
}

/** What a step reads as: its journal label, or a plain stand-in for an unnamed one. */
fun stepTitle(entry: HistoryEntry): String = entry.label?.takeIf { it.isNotBlank() } ?: "Unnamed change"

/**
 * The named undo steps, newest first. Undone steps (what redo would bring
 * back) are dimmed above the line; tapping any step travels to it.
 */
@Composable
fun HistorySheet(onDismiss: () -> Unit) {
    val shell = LocalShell.current
    val model = viewModel(key = "history") { HistoryViewModel(shell.container) }
    val history by model.history.collectAsStateWithLifecycle()
    val labels by shell.container.undo.labels.collectAsStateWithLifecycle()
    val sheetState = rememberModalBottomSheetState()

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = TaktTheme.colors.altRow,
        tonalElevation = 0.dp,
        shape = Metrics.sheetTop,
        dragHandle = { SheetHandle() },
        modifier = Modifier.testTag("history_sheet"),
    ) {
        Column(Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.navigationBars)) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Metrics.lg),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("History", style = TaktTheme.type.title, color = TaktTheme.colors.ink, modifier = Modifier.weight(1f))
            }
            Row(
                Modifier.fillMaxWidth().padding(horizontal = Metrics.lg, vertical = Metrics.sm),
                horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
            ) {
                PButton(
                    "Undo",
                    icon = PIcons.Undo,
                    enabled = labels.undo != null,
                    modifier = Modifier.weight(1f).testTag("history_undo"),
                    onClick = model::undo,
                )
                PButton(
                    "Redo",
                    icon = PIcons.Redo,
                    enabled = labels.redo != null,
                    modifier = Modifier.weight(1f).testTag("history_redo"),
                    onClick = model::redo,
                )
            }
            Hairline()
            val undone = history.undone
            val applied = history.applied
            LazyColumn(Modifier.fillMaxWidth().heightIn(max = 520.dp).testTag("history_list")) {
                if (undone.isNotEmpty()) {
                    item(key = "undone-header", contentType = "header") { SectionHeader("Undone — tap to redo") }
                    items(undone, key = { it.groupId }, contentType = { "step" }) { entry ->
                        StepRow(entry) { model.travel(entry) }
                    }
                }
                item(key = "applied-header", contentType = "header") { SectionHeader("Done — tap to go back to here") }
                if (applied.isEmpty()) {
                    item(key = "empty", contentType = "empty") {
                        Text(
                            "Nothing to undo",
                            style = TaktTheme.type.body,
                            color = TaktTheme.colors.mutedText,
                            modifier = Modifier.padding(horizontal = Metrics.lg, vertical = Metrics.md),
                        )
                    }
                }
                items(applied, key = { it.groupId }, contentType = { "step" }) { entry ->
                    StepRow(entry) { model.travel(entry) }
                }
            }
        }
    }
}

@Composable
private fun StepRow(entry: HistoryEntry, onClick: () -> Unit) {
    val title = stepTitle(entry)
    val action = if (entry.isUndone) "Redo to $title" else "Go back to $title"
    Column {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = Metrics.touchTarget)
                .clickable(role = Role.Button, onClick = onClick)
                .semantics { contentDescription = action }
                .padding(horizontal = Metrics.lg)
                .testTag("history_step"),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(22.dp), contentAlignment = Alignment.Center) {
                Icon(
                    if (entry.isUndone) PIcons.Redo else PIcons.Undo,
                    null,
                    tint = if (entry.isUndone) TaktTheme.colors.dimText else TaktTheme.colors.mutedText,
                    modifier = Modifier.size(16.dp),
                )
            }
            Spacer(Modifier.width(Metrics.sm))
            Text(
                title,
                style = TaktTheme.type.body,
                color = if (entry.isUndone) TaktTheme.colors.dimText else TaktTheme.colors.ink,
                modifier = Modifier.weight(1f),
                maxLines = 1,
            )
            if (entry.changeCount > 1) MonoText("${entry.changeCount} rows", color = TaktTheme.colors.dimText)
        }
        Hairline(Modifier.padding(start = Metrics.lg).background(TaktTheme.colors.paper), color = TaktTheme.colors.borderMuted)
    }
}
