package uk.co.maybeitsadam.takt.quickadd

import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.material3.Text
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uk.co.maybeitsadam.takt.app.ThemeLibraryState
import uk.co.maybeitsadam.takt.appContainer
import uk.co.maybeitsadam.takt.core.theme.ThemeAppearance
import uk.co.maybeitsadam.takt.ui.quickadd.CaptureChipRow
import uk.co.maybeitsadam.takt.ui.quickadd.CapturePreview
import uk.co.maybeitsadam.takt.ui.theme.ResolvedTheme
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme

/**
 * Adding a task from anywhere: a small card over whatever is on screen, the
 * keyboard already up. Return files what was typed into the Inbox — read as
 * the add field reads it, `45m #work @fri !1` and all — and the card goes,
 * leaving the app underneath where it was. A tap outside, or Back, puts it
 * away. It is its own launcher entry ("Add to Takt"), so a phone that opens an
 * app on a side-key double press or a back tap can open this, and the Quick
 * Settings tile and the icon's long-press shortcut open it too.
 */
class QuickCaptureActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val container = appContainer
        setContent {
            val themes by container.themes.state.collectAsStateWithLifecycle(ThemeLibraryState())
            val dark = ResolvedTheme.appearance(themes.specification, themes.mode, isSystemInDarkTheme()) ==
                ThemeAppearance.DARK
            TaktTheme(spec = themes.specification, mode = themes.mode) {
                QuickCaptureCard(
                    dark = dark,
                    onAdd = { text -> add(text) },
                    onDismiss = { finish() },
                )
            }
        }
    }

    private fun add(text: String) {
        val container = appContainer
        val app = applicationContext
        container.scope.launch {
            val session = container.awaitSession()
            container.undo.performNow(announce = false) {
                it.createTask(capturing = text, listId = session.inboxId)
            }
            val title = CapturePreview.of(text).title
            withContext(Dispatchers.Main) {
                Toast.makeText(app, "Added “$title” to the Inbox", Toast.LENGTH_SHORT).show()
            }
        }
        finish()
    }
}

@Composable
private fun QuickCaptureCard(dark: Boolean, onAdd: (String) -> Unit, onDismiss: () -> Unit) {
    val colors = TaktTheme.colors
    val type = TaktTheme.type
    var text by remember { mutableStateOf("") }
    val preview by remember { derivedStateOf { CapturePreview.of(text) } }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }

    fun submit() {
        if (preview.canAdd) onAdd(text)
    }

    Box(
        Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = if (dark) 0.5f else 0.3f))
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { onDismiss() }
            .imePadding()
            .navigationBarsPadding(),
        contentAlignment = Alignment.BottomCenter,
    ) {
        Column(
            Modifier
                .padding(16.dp)
                .fillMaxWidth()
                .background(colors.raised, TaktTheme.radii.panelShape)
                .border(1.dp, colors.border, TaktTheme.radii.panelShape)
                // Taps on the card are the card's, not the scrim's.
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) {}
                .padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text("ADD TO TAKT", style = type.label, color = colors.mutedText)
            BasicTextField(
                value = text,
                onValueChange = { text = it.replace("\n", " ") },
                singleLine = true,
                textStyle = type.body.copy(color = colors.ink),
                cursorBrush = SolidColor(colors.primary),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { submit() }),
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 44.dp)
                    .border(1.dp, colors.inputBorder, TaktTheme.radii.controlShape)
                    .padding(horizontal = 10.dp, vertical = 12.dp)
                    .focusRequester(focus)
                    .testTag("quick_capture_field")
                    .semantics { contentDescription = "New task" },
                decorationBox = { field ->
                    Box {
                        if (text.isEmpty()) {
                            Text("What needs doing?", style = type.body, color = colors.dimText)
                        }
                        field()
                    }
                },
            )
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("→ Inbox", style = type.secondary, color = colors.mutedText)
                CaptureChipRow(preview, Modifier.weight(1f))
                Text(
                    "Add",
                    style = type.bodyStrong,
                    color = if (preview.canAdd) colors.primary else colors.dimText,
                    modifier = Modifier
                        .heightIn(min = 44.dp)
                        .clickable(enabled = preview.canAdd) { submit() }
                        .padding(horizontal = 12.dp, vertical = 12.dp)
                        .testTag("quick_capture_add"),
                )
            }
        }
    }
}
