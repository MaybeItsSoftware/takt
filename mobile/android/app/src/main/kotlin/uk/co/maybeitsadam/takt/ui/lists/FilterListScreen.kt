package uk.co.maybeitsadam.takt.ui.lists

import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import java.time.Instant
import uk.co.maybeitsadam.takt.core.TaskStatus
import uk.co.maybeitsadam.takt.core.WorkspaceTask
import uk.co.maybeitsadam.takt.ui.components.EmptyState
import uk.co.maybeitsadam.takt.ui.components.Format
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.IconAction
import uk.co.maybeitsadam.takt.ui.components.SectionHeader
import uk.co.maybeitsadam.takt.ui.components.TaktTopBar
import uk.co.maybeitsadam.takt.ui.components.Tag
import uk.co.maybeitsadam.takt.ui.components.TaskBox
import uk.co.maybeitsadam.takt.ui.navigation.FilterListRoute
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.parseHexColor

/** A filter list pushed on the back stack. */
@Composable
fun FilterListScreen(route: FilterListRoute) {
    FilterListContent(route.id, showBack = !route.root)
}

/**
 * A filter list's matching tasks, grouped by the list each lives in, under
 * the core's sentence for its rules. Read-only: its rules are edited on the
 * Mac. Tapping a task reveals it in its list; a long press opens its details.
 */
@Composable
internal fun FilterListContent(id: String, showBack: Boolean, modifier: Modifier = Modifier) {
    val shell = LocalShell.current
    val vm: FilterListViewModel = viewModel(key = "filter:$id") { FilterListViewModel(shell.container, id) }
    val state by vm.state.collectAsStateWithLifecycle()

    Column(modifier.fillMaxSize().background(TaktTheme.colors.paper)) {
        TaktTopBar(
            title = state.name.ifEmpty { "Filter list" },
            subtitle = state.summary.ifEmpty { null },
            sidebar = true,
            navigation = if (showBack) {
                { IconAction(Icons.AutoMirrored.Filled.ArrowBack, "Back", tint = TaktTheme.colors.ink, onClick = { shell.navigator.back() }) }
            } else {
                null
            },
        )
        when {
            !state.isLoaded -> Unit
            state.isMissing -> EmptyState("This filter list is gone", detail = "It was deleted, here or on another device.")
            state.sections.isEmpty() -> EmptyState("Nothing matches", detail = state.summary.ifEmpty { null })
            else -> LazyColumn(Modifier.fillMaxSize().testTag("filter_list")) {
                for (section in state.sections) {
                    item(key = "list:${section.list.id}", contentType = "header") {
                        SectionHeader(section.list.name, detail = section.tasks.size.toString())
                    }
                    items(section.tasks, key = { it.id }, contentType = { "task" }) { task ->
                        FilterTaskRow(
                            task = task,
                            listColorHex = section.list.colorHex,
                            isBlocked = task.id in state.blockedTaskIds,
                            waiting = state.waiting[task.id],
                            onOpen = { shell.navigator.openList(task.listId, task.id) },
                            onInspect = { vm.inspect(task.id) },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun FilterTaskRow(
    task: WorkspaceTask,
    listColorHex: String?,
    isBlocked: Boolean,
    waiting: WaitingBadge?,
    onOpen: () -> Unit,
    onInspect: () -> Unit,
) {
    val colors = TaktTheme.colors
    val open = task.status == TaskStatus.OPEN
    Column(
        Modifier
            .fillMaxWidth()
            .background(colors.paper)
            .combinedClickable(onClickLabel = "Reveal in list", onLongClickLabel = "Open details", onLongClick = onInspect, onClick = onOpen)
            .semantics { contentDescription = "Filter list task ${task.title}" }
            .testTag("filter_list_task"),
    ) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = Metrics.touchTarget).padding(end = Metrics.lg),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(Metrics.touchTarget), contentAlignment = Alignment.Center) { TaskBox(task.status, isList = task.isList) }
            Column(Modifier.weight(1f).padding(vertical = Metrics.xs), verticalArrangement = Arrangement.spacedBy(Metrics.xxs)) {
                Text(
                    task.title,
                    style = TaktTheme.type.body,
                    color = if (open && !isBlocked) colors.ink else colors.mutedText,
                    textDecoration = if (open) null else TextDecoration.LineThrough,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                if (isBlocked) Text("Blocked", style = TaktTheme.type.small, color = colors.dimText)
                WaitingMarkers(waiting)
            }
            task.dueAt?.let { due ->
                val overdue = open && due.isBefore(Instant.now())
                Tag(Format.due(due), color = if (overdue) colors.danger else colors.mutedText, mono = true, modifier = Modifier.padding(start = Metrics.sm))
            }
            parseHexColor(listColorHex)?.let { hue ->
                Spacer(Modifier.width(Metrics.sm))
                Box(Modifier.size(8.dp).clip(CircleShape).background(hue))
            }
        }
        Hairline(Modifier.padding(start = Metrics.touchTarget), color = colors.borderMuted)
    }
}

