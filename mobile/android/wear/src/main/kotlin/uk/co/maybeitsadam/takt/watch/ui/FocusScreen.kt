package uk.co.maybeitsadam.takt.watch.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.wear.compose.foundation.lazy.ScalingLazyColumn
import androidx.wear.compose.foundation.lazy.rememberScalingLazyListState
import androidx.wear.compose.material3.CircularProgressIndicator
import androidx.wear.compose.material3.ProgressIndicatorDefaults
import androidx.wear.compose.material3.ScreenScaffold
import androidx.wear.compose.material3.Text
import uk.co.maybeitsadam.takt.wear.contract.WearPhase
import uk.co.maybeitsadam.takt.wear.contract.WearSession
import uk.co.maybeitsadam.takt.wear.contract.clock

/**
 * The running block: its task, the time in and the time left (counted here
 * from the block's start, not sent each second), and its controls.
 */
@Composable
fun FocusScreen(
    session: WearSession?,
    title: String?,
    onPause: () -> Unit,
    onResume: () -> Unit,
    onDone: () -> Unit,
    onStop: () -> Unit,
) {
    val listState = rememberScalingLazyListState()
    val now by rememberNow(active = session?.phase == WearPhase.RUNNING)
    val paused = session?.phase == WearPhase.PAUSED
    val hue = if (paused) WatchColors.amber else WatchColors.azure

    Box(Modifier.fillMaxSize()) {
        // A timer has no length, so no ring: it would imply an end.
        if (session != null && session.taskId != null && !session.openEnded && session.plannedSeconds > 0) {
            val fraction = session.elapsedSeconds(now).toFloat() / session.plannedSeconds
            CircularProgressIndicator(
                progress = { fraction.coerceIn(0f, 1f) },
                modifier = Modifier.fillMaxSize().padding(2.dp),
                colors = ProgressIndicatorDefaults.colors(
                    indicatorColor = if (fraction > 1f) WatchColors.raspberry else hue,
                    trackColor = WatchColors.border,
                ),
                strokeWidth = 4.dp,
            )
        }
        ScreenScaffold(scrollState = listState) { padding ->
            ScalingLazyColumn(
                state = listState,
                contentPadding = padding,
                modifier = Modifier.fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                if (session == null || session.taskId == null) {
                    item(key = "none") {
                        Text(
                            if (session?.phase == WearPhase.ON_BREAK) "On a break." else "No block running. Press play on a task in Today.",
                            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                            color = WatchColors.muted, fontSize = 14.sp, textAlign = TextAlign.Center,
                        )
                    }
                    return@ScalingLazyColumn
                }
                item(key = "state") { Caption(if (paused) "Paused" else "Focusing", color = hue) }
                item(key = "title") {
                    Text(
                        title ?: session.taskTitle ?: "Focus block",
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp),
                        color = WatchColors.ink, fontSize = 16.sp, fontWeight = FontWeight.Medium,
                        textAlign = TextAlign.Center, maxLines = 2, overflow = TextOverflow.Ellipsis,
                    )
                }
                item(key = "clock") {
                    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                        val elapsed = session.elapsedSeconds(now)
                        Text(clock(elapsed), color = WatchColors.ink, fontSize = 34.sp, fontFamily = Lilex)
                        if (session.openEnded) {
                            Text("so far", color = WatchColors.muted, fontSize = 13.sp, fontFamily = Lilex)
                        } else {
                            val remaining = session.plannedSeconds - elapsed
                            Text(
                                if (remaining >= 0) "${clock(remaining)} left" else "${clock(-remaining)} over",
                                color = if (remaining >= 0) WatchColors.muted else WatchColors.raspberry,
                                fontSize = 13.sp, fontFamily = Lilex,
                            )
                        }
                    }
                }
                item(key = "controls") {
                    ButtonRow(Modifier.fillMaxWidth().padding(horizontal = 8.dp)) {
                        if (paused) {
                            TintButton("Resume", WatchColors.azure, onResume, Modifier.weight(1f))
                        } else {
                            TintButton("Pause", WatchColors.amber, onPause, Modifier.weight(1f))
                        }
                        TintButton("Done", WatchColors.emerald, onDone, Modifier.weight(1f))
                    }
                }
                item(key = "stop") {
                    TintButton("Stop", WatchColors.raspberry, onStop, Modifier.fillMaxWidth().padding(horizontal = 24.dp))
                }
            }
        }
    }
}
