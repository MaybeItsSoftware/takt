package uk.co.maybeitsadam.takt.ui.lists

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import uk.co.maybeitsadam.takt.app.CelebrationStyle
import uk.co.maybeitsadam.takt.core.TaskStatus
import uk.co.maybeitsadam.takt.ui.components.Chip
import uk.co.maybeitsadam.takt.ui.components.TaskCheck
import uk.co.maybeitsadam.takt.ui.components.priorityColor
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.PIcons

/**
 * The Mac's disclosure: a ▸ or ▾ in the dim text, in a column as narrow as
 * the Mac's 10 (a little wider for a thumb) and as tall as the row.
 * "Fold <title>" when open, "Unfold <title>" when folded.
 */
@Composable
internal fun FoldChevron(title: String, isFolded: Boolean, onToggle: () -> Unit, modifier: Modifier = Modifier) {
    val description = if (isFolded) "Unfold $title" else "Fold $title"
    Box(
        modifier
            .width(FOLD_COLUMN)
            .heightIn(min = Metrics.touchTarget)
            .clickable(role = Role.Button, indication = null, interactionSource = null, onClick = onToggle)
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) {
        Text(if (isFolded) "▸" else "▾", style = TaktTheme.type.secondary, color = TaktTheme.colors.dimText, maxLines = 1)
    }
}

/** The fold column's width, which a row without children leaves blank. */
internal val FOLD_COLUMN = 22.dp

/** The tick, with the Spark celebration drawn round it when it is newly finished. */
@Composable
internal fun CelebratedCheck(
    status: TaskStatus,
    isList: Boolean,
    title: String,
    celebration: CelebrationStyle,
    onToggle: () -> Unit,
) {
    val spark = remember { Animatable(0f) }
    var previous by remember { mutableStateOf(status) }
    LaunchedEffect(status) {
        if (previous == TaskStatus.OPEN && status == TaskStatus.COMPLETED && celebration == CelebrationStyle.SPARK) {
            spark.snapTo(0f)
            spark.animateTo(1f, tween(520))
            spark.snapTo(0f)
        }
        previous = status
    }
    val color = TaktTheme.colors.success
    Box(
        Modifier.drawBehind {
            val t = spark.value
            if (t > 0f) {
                val radius = 10.dp.toPx() + 16.dp.toPx() * t
                drawCircle(color.copy(alpha = (1f - t) * 0.8f), radius, style = Stroke(width = 2.dp.toPx() * (1f - t) + 0.5f))
                for (i in 0 until 6) {
                    val angle = Math.toRadians(i * 60.0 + 30.0)
                    val inner = radius * 0.7f
                    val outer = radius * 1.05f
                    drawLine(
                        color.copy(alpha = 1f - t),
                        Offset(center.x + inner * Math.cos(angle).toFloat(), center.y + inner * Math.sin(angle).toFloat()),
                        Offset(center.x + outer * Math.cos(angle).toFloat(), center.y + outer * Math.sin(angle).toFloat()),
                        strokeWidth = 1.5.dp.toPx(),
                    )
                }
            }
        },
    ) {
        TaskCheck(status, isList = isList, label = title, onToggle = onToggle)
    }
}

/**
 * The title. A closed row is dim, as the Mac's; with Strike, Spark or Fold it
 * also carries a rule through it, drawn across when the row is newly
 * finished. An open row waiting on an open task ([isBlocked]) is dim too,
 * without the rule.
 */
@Composable
internal fun TaskTitle(
    title: String,
    status: TaskStatus,
    celebration: CelebrationStyle,
    modifier: Modifier = Modifier,
    style: TextStyle = TaktTheme.type.body,
    maxLines: Int = 2,
    isBlocked: Boolean = false,
) {
    val closed = status != TaskStatus.OPEN
    val struck = closed && celebration != CelebrationStyle.NONE
    val fraction = remember { Animatable(if (struck) 1f else 0f) }
    var previous by remember { mutableStateOf(status) }
    LaunchedEffect(status, celebration) {
        when {
            !struck -> fraction.snapTo(0f)
            previous == TaskStatus.OPEN && celebration == CelebrationStyle.STRIKE -> {
                fraction.snapTo(0f)
                fraction.animateTo(1f, tween(320))
            }
            else -> fraction.snapTo(1f)
        }
        previous = status
    }
    val ruleColor = TaktTheme.colors.dimText
    Text(
        title,
        style = style,
        color = if (closed || isBlocked) TaktTheme.colors.dimText else TaktTheme.colors.ink,
        maxLines = maxLines,
        overflow = TextOverflow.Ellipsis,
        modifier = modifier.drawWithContent {
            drawContent()
            val f = fraction.value
            if (f > 0f) {
                // One rule through the first line, as the Mac's strike plugin draws it.
                val y = minOf(size.height / 2f, 11.dp.toPx())
                drawLine(ruleColor, Offset(0f, y), Offset(size.width * f, y), strokeWidth = 1.dp.toPx())
            }
        },
    )
}

/**
 * The Mac's chips after the title (`widgets::detail_chips`), bare text at the
 * secondary size in its order: blocked (warning) or later (dim), ⌛ who
 * (muted), #tags up to three (accent), ↻ (dim), P1 (danger) / P2 (warning) /
 * other (muted), the due day (dim, danger overdue), the estimate in mono,
 * and ¶ when the task has notes. A phone also marks a task planned for today.
 */
@Composable
internal fun RowMarkers(row: OutlineRow, modifier: Modifier = Modifier, maxTags: Int = 3) {
    Row(modifier.fillMaxHeight(), horizontalArrangement = Arrangement.spacedBy(Metrics.xs + Metrics.xxs), verticalAlignment = Alignment.CenterVertically) {
        val colors = TaktTheme.colors
        val open = row.status == TaskStatus.OPEN
        when {
            row.isBlocked && open -> Chip("blocked", colors.warning, Modifier.semantics { contentDescription = "Blocked by another task" })
            row.isBuried && open -> Chip(row.starts ?: "later", colors.dimText)
        }
        row.waitingOn?.let { Chip("⌛ $it", colors.mutedText, Modifier.semantics { contentDescription = "Waiting on $it" }) }
        row.followUp?.let { Chip(it, colors.dimText, mono = true) }
        for (tag in row.tags.take(maxTags)) Chip("#$tag", colors.primary)
        if (row.tags.size > maxTags) Chip("+${row.tags.size - maxTags}", colors.dimText)
        if (row.isDaily) Chip("↻", colors.dimText, Modifier.semantics { contentDescription = "Daily" })
        if (row.isPlanned && open) Chip("☀", colors.dimText, Modifier.semantics { contentDescription = "Planned for today" })
        row.priority?.let { Chip("P$it", priorityColor(it)) }
        row.due?.let { due -> Chip(due.text, if (open && due.isOverdue) colors.danger else colors.dimText) }
        row.estimate?.let { Chip(it, colors.dimText, mono = true) }
        if (row.hasNotes) Chip("¶", colors.dimText, Modifier.semantics { contentDescription = "Has notes" })
    }
}
