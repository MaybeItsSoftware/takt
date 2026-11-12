package uk.co.maybeitsadam.takt.ui.focus

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import uk.co.maybeitsadam.takt.core.FocusPoints
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.inspector.InspectorIcons
import uk.co.maybeitsadam.takt.ui.inspector.StepButton
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme

/**
 * How did that block go? As on the Mac (`WorkspaceFocusQualityPrompt`), one
 * multiplier that starts at ×1.0, an ordinary block, and moves a tenth at a
 * time: the answer is a small adjustment from "it was fine", not a pick from
 * a list of adjectives. The running total shows while choosing. "Doesn't
 * count" logs the block at ×0 in one tap, for menial work; "Keep working"
 * drops the question and resumes the clock.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun QualityPrompt(
    pending: PendingCompletion,
    onScore: (Double) -> Unit,
    onCancel: () -> Unit,
    /** Today's points so far, for the running total; null leaves the total out. */
    todayPoints: Double? = null,
) {
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var tenths by rememberSaveable(pending) { mutableIntStateOf(FocusText.NEUTRAL_TENTHS) }
    val multiplier = tenths / 10.0
    val points = FocusPoints.score(pending.seconds, multiplier)
    val colors = TaktTheme.colors
    ModalBottomSheet(
        onDismissRequest = onCancel,
        sheetState = sheet,
        containerColor = TaktTheme.colors.altRow,
        contentColor = colors.ink,
        tonalElevation = 0.dp,
        shape = Metrics.sheetTop,
        dragHandle = { uk.co.maybeitsadam.takt.ui.SheetHandle() },
        modifier = Modifier.testTag("quality_prompt"),
    ) {
        Column(
            Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).navigationBarsPadding().padding(horizontal = Metrics.lg),
            verticalArrangement = Arrangement.spacedBy(Metrics.md),
        ) {
            Column(verticalArrangement = Arrangement.spacedBy(Metrics.xxs)) {
                Text(
                    if (pending.completeTask) "Complete task · how did that go?" else "Log progress · how did that go?",
                    style = TaktTheme.type.label,
                    color = colors.mutedText,
                )
                Text(pending.title, style = TaktTheme.type.title, color = colors.ink, maxLines = 2, overflow = TextOverflow.Ellipsis)
                Text(
                    "${FocusPoints.formatted(FocusPoints.minutes(pending.seconds))} minutes of focused work",
                    style = TaktTheme.type.small,
                    color = colors.mutedText,
                )
            }

            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Metrics.md)) {
                StepButton(InspectorIcons.Minus, "Less", enabled = tenths > FocusText.tenthsRange.first) {
                    tenths = FocusText.nudge(tenths, -1)
                }
                Text(
                    FocusText.dial(tenths),
                    style = TaktTheme.type.monoLarge,
                    color = if (tenths == FocusText.NEUTRAL_TENTHS) colors.ink else colors.primary,
                    modifier = Modifier
                        .widthIn(min = 96.dp)
                        .testTag("quality_multiplier")
                        .semantics {
                            contentDescription = "Multiplier ${FocusText.dial(tenths).drop(1)}"
                            progressBarRangeInfo = ProgressBarRangeInfo(
                                tenths.toFloat(),
                                FocusText.tenthsRange.first.toFloat()..FocusText.tenthsRange.last.toFloat(),
                            )
                            setProgress { target ->
                                tenths = Math.round(target).coerceIn(FocusText.tenthsRange)
                                true
                            }
                        },
                )
                StepButton(Icons.Filled.Add, "More", enabled = tenths < FocusText.tenthsRange.last) {
                    tenths = FocusText.nudge(tenths, 1)
                }
                Spacer(Modifier.weight(1f))
                if (tenths != FocusText.NEUTRAL_TENTHS) {
                    PButton("Reset", Modifier.testTag("quality_reset")) { tenths = FocusText.NEUTRAL_TENTHS }
                }
            }

            Hairline(color = colors.borderMuted)

            Row(verticalAlignment = Alignment.Bottom) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Metrics.xxs)) {
                    Text("This block", style = TaktTheme.type.label, color = colors.mutedText)
                    Text(
                        "${FocusPoints.formatted(points)} pts",
                        style = TaktTheme.type.monoLarge,
                        color = colors.primary,
                        modifier = Modifier.testTag("quality_points"),
                    )
                }
                if (todayPoints != null) {
                    Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(Metrics.xxs)) {
                        Text("Today", style = TaktTheme.type.label, color = colors.mutedText)
                        Text("${FocusPoints.formatted(todayPoints + points)} pts", style = TaktTheme.type.monoTitle, color = colors.ink)
                    }
                }
            }

            PButton("Log it", Modifier.fillMaxWidth().testTag("quality_log"), primary = true) { onScore(multiplier) }
            Row(horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
                PButton("Doesn't count (×0)", Modifier.weight(1f).testTag("quality_zero")) { onScore(0.0) }
                PButton("Keep working", Modifier.weight(1f)) { onCancel() }
            }
            Box(Modifier.heightIn(min = Metrics.lg))
        }
    }
}
