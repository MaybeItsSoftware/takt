package uk.co.maybeitsadam.takt.watch.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.wear.compose.foundation.lazy.ScalingLazyColumn
import androidx.wear.compose.foundation.lazy.rememberScalingLazyListState
import androidx.wear.compose.material3.ScreenScaffold
import androidx.wear.compose.material3.Text
import uk.co.maybeitsadam.takt.wear.contract.WearPhase
import uk.co.maybeitsadam.takt.wear.contract.WearSession
import uk.co.maybeitsadam.takt.wear.contract.WearSnapshot
import uk.co.maybeitsadam.takt.wear.contract.WearTask
import uk.co.maybeitsadam.takt.wear.contract.clock
import uk.co.maybeitsadam.takt.wear.contract.hoursAndMinutes
import uk.co.maybeitsadam.takt.watch.ageText

/**
 * The day as the phone's Today orders it. A tap ticks a row off (crossed
 * out, then gone: [onComplete] sends it after a beat); play starts a block
 * on it; the running row opens Focus.
 */
@Composable
fun TodayScreen(
    snapshot: WearSnapshot?,
    reachable: Boolean,
    queuedCount: Int,
    onComplete: (WearTask) -> Unit,
    onStart: (WearTask) -> Unit,
    onOpenFocus: () -> Unit,
) {
    val listState = rememberScalingLazyListState()
    val ticking = snapshot?.session?.phase == WearPhase.RUNNING
    val now by rememberNow(active = ticking)
    // Rows ticked here, drawn crossed out until the tick is sent and they leave.
    var leaving by remember { mutableStateOf(emptySet<String>()) }

    ScreenScaffold(scrollState = listState) { padding ->
        ScalingLazyColumn(
            state = listState,
            contentPadding = padding,
            modifier = Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            item(key = "head") { TodayHead(snapshot, reachable, queuedCount, now) }
            if (snapshot == null) {
                item(key = "none") { Empty("Open Takt on your phone to bring today here.") }
                return@ScalingLazyColumn
            }
            val tasks = snapshot.tasks.filterNot { it.isDaily && it.dailyDone }
            if (tasks.isEmpty()) item(key = "clear") { Empty("Nothing left today.") }
            val canStart = snapshot.session?.taskId == null
            val firstDaily = tasks.indexOfFirst { it.isDaily && !it.isRunning }
            tasks.forEachIndexed { index, task ->
                if (index == firstDaily && index > 0) item(key = "dailies") { Caption("Dailies", Modifier.padding(top = 6.dp)) }
                item(key = task.id) {
                    val session = snapshot.session
                    if (task.isRunning && session != null) {
                        RunningRow(task, session, now, onOpenFocus)
                    } else {
                        TaskRow(
                            task = task,
                            crossedOut = task.id in leaving,
                            canStart = canStart,
                            onTick = {
                                leaving = leaving + task.id
                                onComplete(task)
                            },
                            onStart = { onStart(task) },
                        )
                    }
                }
            }
        }
    }

    // A crossed-out row is forgotten once the day no longer has it.
    LaunchedEffect(snapshot) {
        val present = snapshot?.tasks?.mapTo(HashSet()) { it.id } ?: emptySet()
        leaving = leaving.filterTo(HashSet()) { it in present }
    }
}

@Composable
private fun TodayHead(snapshot: WearSnapshot?, reachable: Boolean, queuedCount: Int, now: Long) {
    Column(Modifier.fillMaxWidth().padding(bottom = 4.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Text("Today", fontSize = 18.sp, fontWeight = FontWeight.Medium, color = WatchColors.ink)
        if (snapshot != null) {
            val left = snapshot.openCount
            Caption(if (left == 0) "All done" else "$left left")
        }
        // Out of reach: say how old the day is, and what is waiting to go.
        if (!reachable || queuedCount > 0) {
            val parts = buildList {
                if (!reachable) add(if (snapshot == null) "Phone out of reach" else "From ${ageText(snapshot.generatedAt, now)}")
                if (queuedCount > 0) add("$queuedCount to send")
            }
            Caption(parts.joinToString(" · "), color = WatchColors.amber)
        }
    }
}

@Composable
private fun Empty(text: String) {
    Text(
        text, modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
        color = WatchColors.muted, fontSize = 14.sp, textAlign = TextAlign.Center,
    )
}

private val cardShape = RoundedCornerShape(WatchShape.card)

@Composable
private fun TaskRow(task: WearTask, crossedOut: Boolean, canStart: Boolean, onTick: () -> Unit, onStart: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = WatchShape.target)
            .background(WatchColors.raised, cardShape)
            .border(WatchShape.hairline, WatchColors.border, cardShape),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(
            Modifier
                .weight(1f)
                .heightIn(min = WatchShape.target)
                .clickable(enabled = !crossedOut, role = Role.Checkbox, onClickLabel = if (task.isDaily) "Log today" else "Complete", onClick = onTick)
                .padding(start = 12.dp, end = 4.dp, top = 6.dp, bottom = 6.dp),
            verticalArrangement = Arrangement.Center,
        ) {
            Text(
                task.title,
                color = if (crossedOut) WatchColors.dim else WatchColors.ink,
                fontSize = 15.sp,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                textDecoration = if (crossedOut) TextDecoration.LineThrough else null,
            )
            detailLine(task)?.let {
                Text(it, color = WatchColors.muted, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        if (canStart && !crossedOut) PlayButton(onStart)
    }
}

/** The detail under a title: why it is here or its list, then its estimate. */
private fun detailLine(task: WearTask): String? =
    listOfNotNull(task.detail, task.estimateSeconds?.takeIf { it > 0 }?.let(::hoursAndMinutes)).joinToString(" · ").ifEmpty { null }

@Composable
private fun PlayButton(onClick: () -> Unit) {
    // The painted square is 32dp; the target around it is the full 48dp.
    Box(
        Modifier
            .size(WatchShape.target)
            .clickable(role = Role.Button, onClickLabel = "Start focus", onClick = onClick)
            .semantics { contentDescription = "Start focus" },
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .size(32.dp)
                .background(WatchColors.azure.copy(alpha = 0.14f), RoundedCornerShape(WatchShape.control))
                .border(WatchShape.hairline, WatchColors.azure.copy(alpha = 0.5f), RoundedCornerShape(WatchShape.control)),
            contentAlignment = Alignment.Center,
        ) {
            Text("▶", color = WatchColors.azure, fontSize = 12.sp)
        }
    }
}

@Composable
private fun RunningRow(task: WearTask, session: WearSession, now: Long, onOpen: () -> Unit) {
    val paused = session.phase == WearPhase.PAUSED
    val hue = if (paused) WatchColors.amber else WatchColors.azure
    Column(
        Modifier
            .fillMaxWidth()
            .heightIn(min = WatchShape.target)
            .background(hue.copy(alpha = 0.10f), cardShape)
            .border(WatchShape.hairline, hue.copy(alpha = 0.6f), cardShape)
            .clickable(role = Role.Button, onClickLabel = "Open focus", onClick = onOpen)
            .padding(horizontal = 12.dp, vertical = 6.dp),
    ) {
        Text(task.title, color = WatchColors.ink, fontSize = 15.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
        val elapsed = session.elapsedSeconds(now)
        Text(
            (if (paused) "Paused · " else "") +
                if (session.openEnded) "${clock(elapsed)} so far" else "${clock(elapsed)} of ${hoursAndMinutes(session.plannedSeconds)}",
            color = hue, fontSize = 12.sp, fontFamily = Lilex, maxLines = 1,
        )
    }
}
