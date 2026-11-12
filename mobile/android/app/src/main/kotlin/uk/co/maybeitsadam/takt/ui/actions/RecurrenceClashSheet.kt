package uk.co.maybeitsadam.takt.ui.actions

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import uk.co.maybeitsadam.takt.data.workspace.RecurrenceClashChoice
import uk.co.maybeitsadam.takt.ui.SheetHandle
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme

/**
 * A repeating task moved onto the day it comes round again: stack on that
 * day (the default, first and filled), keep both, or cancel. A bottom sheet,
 * as the iPhone's confirmation dialog is; swiping it away is Cancel.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RecurrenceClashSheet(pending: PendingRecurrenceClash, onChoose: (RecurrenceClashChoice) -> Unit) {
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val colors = TaktTheme.colors
    ModalBottomSheet(
        onDismissRequest = { onChoose(RecurrenceClashChoice.CANCEL) },
        sheetState = sheet,
        containerColor = TaktTheme.colors.altRow,
        contentColor = colors.ink,
        tonalElevation = 0.dp,
        shape = Metrics.sheetTop,
        dragHandle = { SheetHandle() },
        modifier = Modifier.testTag("recurrence_clash"),
    ) {
        Column(
            Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).navigationBarsPadding().padding(horizontal = Metrics.lg),
            verticalArrangement = Arrangement.spacedBy(Metrics.md),
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(Metrics.xxs)) {
                Text(pending.title, style = TaktTheme.type.title, color = colors.ink)
                Text(pending.message, style = TaktTheme.type.small, color = colors.mutedText)
            }
            RecurrenceClashChoice.entries.forEach { choice ->
                Column(verticalArrangement = Arrangement.spacedBy(Metrics.xxs)) {
                    PButton(
                        choice.title,
                        Modifier.fillMaxWidth().testTag("recurrence_clash_${choice.name.lowercase()}"),
                        primary = choice == RecurrenceClashChoice.STACK,
                    ) { onChoose(choice) }
                    choice.detail?.let { Text(it, style = TaktTheme.type.small, color = colors.mutedText) }
                }
            }
            Box(Modifier.heightIn(min = Metrics.lg))
        }
    }
}
