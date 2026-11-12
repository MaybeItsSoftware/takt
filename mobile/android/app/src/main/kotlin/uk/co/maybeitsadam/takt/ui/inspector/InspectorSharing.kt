package uk.co.maybeitsadam.takt.ui.inspector

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import uk.co.maybeitsadam.takt.core.WorkspaceTask
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uniffi.takt_core.SharedListMember
import uniffi.takt_core.TaskSharing
import uniffi.takt_core.sharedMemberName

/**
 * In a shared list: who the task is assigned to, with a menu to change it,
 * and who added and finished it. Nothing for a task in a list of your own.
 */
@Composable
internal fun SharingSection(vm: TaskInspectorViewModel, task: WorkspaceTask) {
    var refresh by remember(task.id) { mutableIntStateOf(0) }
    var sharing by remember(task.id) { mutableStateOf<TaskSharing?>(null) }
    LaunchedEffect(task, refresh) { sharing = vm.sharing(task.id) }
    val shared = sharing ?: return
    var menu by remember { mutableStateOf(false) }
    val colors = TaktTheme.colors
    InspectorSection("Shared", modifier = Modifier.testTag("inspector_sharing")) {
        Box {
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = Metrics.touchTarget)
                    .clickable(role = Role.Button, onClickLabel = "Assign") { menu = true },
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("Assigned to", style = TaktTheme.type.body, color = colors.mutedText, modifier = Modifier.weight(1f))
                Text(
                    shared.assignee?.let { name(it).replaceFirstChar(Char::uppercase) } ?: "Nobody",
                    style = TaktTheme.type.body, color = colors.ink, maxLines = 1, overflow = TextOverflow.Ellipsis,
                )
            }
            ChalkMenu(menu, onDismiss = { menu = false }) {
                ChalkMenuItem("Nobody: anyone in the list", onClick = {
                    menu = false
                    vm.assign(null) { refresh++ }
                })
                for (member in shared.members) {
                    ChalkMenuItem(if (member.you) "Me" else member.email ?: "Someone", onClick = {
                        menu = false
                        vm.assign(member.accountId) { refresh++ }
                    })
                }
            }
        }
        val said = listOfNotNull(
            shared.createdBy?.let { "added by ${name(it)}" },
            shared.completedBy?.let { "completed by ${name(it)}" },
        )
        if (said.isNotEmpty()) Hint(said.joinToString(", ").replaceFirstChar(Char::uppercase), color = colors.mutedText)
        Hint("A task assigned to someone else stays out of your Today and focus.")
    }
}

private fun name(member: SharedListMember): String = sharedMemberName(member)
