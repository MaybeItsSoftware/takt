package uk.co.maybeitsadam.takt.widget

import android.content.Context
import android.content.Intent
import androidx.compose.runtime.Composable
import androidx.compose.ui.unit.sp
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.provideContent
import androidx.glance.background
import androidx.glance.color.ColorProvider
import androidx.glance.unit.ColorProvider
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.text.FontFamily
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import uk.co.maybeitsadam.takt.MainActivity
import uk.co.maybeitsadam.takt.appContainer
import uk.co.maybeitsadam.takt.core.FocusContext
import uk.co.maybeitsadam.takt.core.WorkspaceNextUpSnapshot
import uk.co.maybeitsadam.takt.ui.ShellIntents
import uk.co.maybeitsadam.takt.app.ThemeLibraryState
import uk.co.maybeitsadam.takt.ui.theme.ResolvedTheme

/**
 * Next up, and how much of today is open, in the theme in force (raised
 * inside a hairline, ink and muted text). It flips with the system's day and
 * night unless the theme or the appearance setting holds one.
 * The whole widget opens Today; the task row opens its inspector.
 */
class NextUpWidget : GlanceAppWidget() {
    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val summary = runCatching {
            val session = context.appContainer.awaitSession()
            val running = session.repository.activeFocusSession()?.activeTaskId
            NextUpSummary.of(session.repository.nextUpSnapshot(session.workspace.id, FocusContext(), running, ladderLimit = WorkspaceNextUpSnapshot.fallbackDayLength), running)
        }.getOrDefault(NextUpSummary.EMPTY)
        val theme = WidgetTheme.of(context.appContainer.theme.value)
        provideContent { NextUpContent(context, summary, theme) }
    }
}

class NextUpWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = NextUpWidget()
}

/**
 * The theme's roles as Glance colour providers, with the day and night halves
 * chosen the way the app chooses its appearance, plus its structure in dp/sp.
 */
private class WidgetTheme(private val day: ResolvedTheme, private val night: ResolvedTheme) {
    private fun of(role: (ResolvedTheme) -> androidx.compose.ui.graphics.Color): ColorProvider =
        ColorProvider(day = role(day), night = role(night))

    val border = of { it.colors.border }
    val raised = of { it.colors.raised }
    val well = of { it.colors.well }
    val ink = of { it.colors.ink }
    val muted = of { it.colors.mutedText }
    val primary = of { it.colors.primary }
    // The Mac's card radius, 4, whatever the theme gives a pane (Zed One's are square).
    val panelRadius = day.radii.control
    val controlRadius = day.radii.control
    val hairline = day.borders.hairline
    val spacing = day.spacing
    val caption = day.type.scale.caption.sp
    val body = day.type.scale.body.sp

    companion object {
        fun of(state: ThemeLibraryState): WidgetTheme {
            val spec = state.specification
            fun resolved(systemDark: Boolean) =
                ResolvedTheme(spec, ResolvedTheme.appearance(spec, state.mode, systemDark))
            return WidgetTheme(day = resolved(false), night = resolved(true))
        }
    }
}

private fun openApp(context: Context, taskId: String? = null): Intent =
    Intent(context, MainActivity::class.java)
        .setAction(if (taskId == null) "uk.co.maybeitsadam.priority.widget.TODAY" else "uk.co.maybeitsadam.priority.widget.TASK.$taskId")
        .putExtra(ShellIntents.EXTRA_TAB, "TODAY")
        .apply { if (taskId != null) putExtra(ShellIntents.EXTRA_TASK_ID, taskId) }
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)

@Composable
private fun NextUpContent(context: Context, summary: NextUpSummary, theme: WidgetTheme) {
    val spacing = theme.spacing
    // A hairline border painted as the outer box, the raised card inside it.
    Box(
        GlanceModifier
            .fillMaxSize()
            .background(theme.border)
            .cornerRadius(theme.panelRadius)
            .padding(theme.hairline)
            .clickable(actionStartActivity(openApp(context))),
    ) {
        Column(
            GlanceModifier.fillMaxSize().background(theme.raised).cornerRadius(theme.panelRadius).padding(spacing.md),
        ) {
            Row(GlanceModifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    if (summary.isRunning) "Focusing" else "Next up",
                    style = TextStyle(color = if (summary.isRunning) theme.primary else theme.muted, fontSize = theme.caption, fontWeight = FontWeight.Medium),
                    modifier = GlanceModifier.defaultWeight(),
                )
                Box(
                    GlanceModifier.background(theme.well).cornerRadius(theme.controlRadius).padding(horizontal = spacing.sm, vertical = spacing.xxs),
                ) {
                    Text(
                        "${summary.todayCount} today",
                        style = TextStyle(color = theme.ink, fontSize = theme.caption, fontFamily = FontFamily.Monospace),
                    )
                }
            }
            Spacer(GlanceModifier.height(spacing.sm))
            val taskId = summary.taskId
            if (taskId == null || summary.title == null) {
                Text("Nothing to do", style = TextStyle(color = theme.ink, fontSize = theme.body, fontWeight = FontWeight.Medium))
                Text("Add a task, or plan one for today.", style = TextStyle(color = theme.muted, fontSize = theme.caption))
            } else {
                Column(GlanceModifier.fillMaxWidth().clickable(actionStartActivity(openApp(context, taskId)))) {
                    Text(
                        summary.title,
                        maxLines = 2,
                        style = TextStyle(color = theme.ink, fontSize = theme.body, fontWeight = FontWeight.Medium),
                    )
                    summary.reason?.let {
                        Text(it, maxLines = 1, style = TextStyle(color = theme.muted, fontSize = theme.caption))
                    }
                }
            }
        }
    }
}
