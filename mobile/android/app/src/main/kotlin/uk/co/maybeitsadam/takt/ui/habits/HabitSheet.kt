package uk.co.maybeitsadam.takt.ui.habits

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import uk.co.maybeitsadam.takt.ui.inspector.InspectorIcons
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import uk.co.maybeitsadam.takt.app.HabitRequest
import uk.co.maybeitsadam.takt.core.HabitExpiry
import uk.co.maybeitsadam.takt.core.HabitFrequency
import uk.co.maybeitsadam.takt.core.HabitPlacement
import uk.co.maybeitsadam.takt.data.workspace.HabitFormContext
import uk.co.maybeitsadam.takt.data.workspace.habitFormContext
import uk.co.maybeitsadam.takt.data.workspace.saveHabit
import uk.co.maybeitsadam.takt.ui.SheetHandle
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.components.Tag
import uk.co.maybeitsadam.takt.ui.inspector.ChalkField
import uk.co.maybeitsadam.takt.ui.inspector.FieldLabel
import uk.co.maybeitsadam.takt.ui.inspector.Hint
import uk.co.maybeitsadam.takt.ui.inspector.Segment
import uk.co.maybeitsadam.takt.ui.inspector.Segmented
import uk.co.maybeitsadam.takt.ui.inspector.StepButton
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme

/**
 * The habit form, as a bottom sheet: the Mac's `gb` overlay for a phone.
 * Opened on a task it makes a habit from that task; on a habit it edits it;
 * with no task it makes a standalone habit. A habit lands in a board column
 * each day it is due, disappears at the end of a missed day or stays until
 * done, and ends with its task, on a date, or never.
 */
@Composable
fun HabitSheet(request: HabitRequest, onDismiss: () -> Unit) {
    val container = LocalShell.current.container
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var context by remember(request.nonce) { mutableStateOf<HabitFormContext?>(null) }
    LaunchedEffect(request.nonce) {
        context = runCatching { container.repository().habitFormContext(request.taskId) }
            .onFailure { container.undo.report(it) }
            .getOrNull()
        if (context == null) onDismiss()
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
        val loaded = context ?: return@ModalBottomSheet
        HabitFormBody(loaded) { draft ->
            container.undo.performNow { repo -> repo.saveHabit(draft, loaded.habitTaskId) }.also { saved ->
                if (saved) onDismiss()
            }
        }
    }
}

@Composable
private fun HabitFormBody(context: HabitFormContext, save: suspend (uk.co.maybeitsadam.takt.data.workspace.HabitDraft) -> Boolean) {
    var draft by remember(context) { mutableStateOf(context.draft) }
    var estimateText by remember(context) { mutableStateOf(HabitForm.estimateText(context.draft.estimateSeconds)) }
    var expiryDateText by remember(context) { mutableStateOf(context.draft.expiry.date?.let { HabitForm.dayText(it) } ?: "") }
    var error by remember(context) { mutableStateOf<HabitFormResult.Invalid?>(null) }
    val scope = rememberCoroutineScope()

    Column(
        Modifier
            .fillMaxWidth()
            .verticalScroll(rememberScrollState())
            .imePadding()
            .navigationBarsPadding()
            .padding(horizontal = Metrics.lg)
            .padding(bottom = Metrics.lg)
            .testTag("habit_sheet"),
        verticalArrangement = Arrangement.spacedBy(Metrics.md),
    ) {
        Text(if (context.habitTaskId == null) "New habit" else "Edit habit", style = TaktTheme.type.title, color = TaktTheme.colors.ink)

        FormRow("Habit") {
            ChalkField(
                draft.title,
                onValueChange = { draft = draft.copy(title = it.replace("\n", " ")) },
                placeholder = "Practise drums",
                style = TaktTheme.type.field,
                isError = error?.field == HabitField.TITLE,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                modifier = Modifier.testTag("habit_title"),
            )
        }

        FormRow("How often") {
            Segmented(
                HabitFrequencyKind.entries.map { Segment(it.label, it) },
                selected = HabitForm.kind(draft.frequency),
                onSelect = { draft = draft.copy(frequency = HabitForm.withKind(draft.frequency, it)) },
            )
            when (val frequency = draft.frequency) {
                is HabitFrequency.Weekdays -> Row(horizontalArrangement = Arrangement.spacedBy(Metrics.xs)) {
                    for (weekday in HabitForm.weekdayOrder) {
                        Tag(
                            HabitForm.weekdayName(weekday),
                            color = if (weekday in frequency.days) TaktTheme.colors.primary else TaktTheme.colors.mutedText,
                            selected = weekday in frequency.days,
                            modifier = Modifier.weight(1f),
                            onClick = { draft = draft.copy(frequency = HabitForm.toggleWeekday(draft.frequency, weekday)) },
                        )
                    }
                }
                is HabitFrequency.EveryNDays -> Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
                ) {
                    StepButton(InspectorIcons.Minus, "Fewer days", enabled = frequency.days > 2) {
                        draft = draft.copy(frequency = HabitForm.stepInterval(draft.frequency, -1))
                    }
                    Text(frequency.label, style = TaktTheme.type.body, color = TaktTheme.colors.ink, modifier = Modifier.weight(1f))
                    StepButton(Icons.Filled.Add, "More days", enabled = frequency.days < 366) {
                        draft = draft.copy(frequency = HabitForm.stepInterval(draft.frequency, 1))
                    }
                }
                else -> Unit
            }
        }

        FormRow("Appears in") {
            Segmented(
                HabitPlacement.entries.map { Segment(it.label, it) },
                selected = draft.placement,
                onSelect = { draft = draft.copy(placement = it) },
            )
        }

        FormRow("End of day") {
            Segmented(
                listOf(Segment("Disappears", true), Segment("Stays until done", false)),
                selected = draft.dropsAtDayEnd,
                onSelect = { draft = draft.copy(dropsAtDayEnd = it) },
            )
        }

        FormRow("Estimate") {
            ChalkField(
                estimateText,
                onValueChange = { estimateText = it },
                placeholder = "30m",
                style = TaktTheme.type.field,
                isError = error?.field == HabitField.ESTIMATE,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii),
                modifier = Modifier.testTag("habit_estimate"),
            )
        }

        FormRow("Ends") {
            val kinds = HabitForm.expiryKinds(draft.sourceTaskId != null)
            val kind = HabitForm.expiryKind(draft.expiry)
            Segmented(
                kinds.map { Segment(HabitForm.expiryLabel(it), it) },
                selected = kind,
                onSelect = { option ->
                    draft = draft.copy(
                        expiry = when (option) {
                            HabitExpiryKind.SOURCE -> HabitExpiry.WhenSourceCompleted
                            HabitExpiryKind.NEVER -> HabitExpiry.Never
                            HabitExpiryKind.DATE -> {
                                val date = HabitForm.defaultEndDate()
                                if (expiryDateText.isEmpty()) expiryDateText = HabitForm.dayText(date)
                                HabitExpiry.On(date)
                            }
                        },
                    )
                },
            )
            if (kind == HabitExpiryKind.SOURCE) Hint(HabitForm.sourceHint(context.sourceTitle))
            if (kind == HabitExpiryKind.DATE) {
                ChalkField(
                    expiryDateText,
                    onValueChange = { expiryDateText = it },
                    placeholder = "2026-12-31",
                    style = TaktTheme.type.field,
                    isError = error?.field == HabitField.EXPIRY_DATE,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii),
                    modifier = Modifier.testTag("habit_end_date"),
                )
                Hint("A date, 3w, or a weekday.")
            }
        }

        error?.let { Text(it.message, style = TaktTheme.type.small, color = TaktTheme.colors.danger) }

        PButton("Save", Modifier.fillMaxWidth().testTag("habit_save"), primary = true) {
            when (val result = HabitForm.validate(draft, estimateText, expiryDateText)) {
                is HabitFormResult.Invalid -> error = result
                is HabitFormResult.Valid -> {
                    error = null
                    scope.launch { save(result.draft) }
                }
            }
        }
    }
}

@Composable
private fun FormRow(label: String, content: @Composable ColumnScope.() -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(Metrics.xs)) {
        FieldLabel(label)
        content()
    }
}
