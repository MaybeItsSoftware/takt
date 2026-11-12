package uk.co.maybeitsadam.takt.ui.settings

import androidx.compose.runtime.LaunchedEffect
import uk.co.maybeitsadam.takt.ui.navigation.LocalShell
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import uk.co.maybeitsadam.takt.app.UserThemeFile
import uk.co.maybeitsadam.takt.app.ThemeLibraryState
import uk.co.maybeitsadam.takt.core.theme.ThemeAppearance
import uk.co.maybeitsadam.takt.core.theme.ThemeFileIssue
import uk.co.maybeitsadam.takt.core.theme.ThemeFileOutcome
import uk.co.maybeitsadam.takt.core.theme.ThemeIssueSeverity
import uk.co.maybeitsadam.takt.core.theme.ThemeSpecification
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.components.Tag
import uk.co.maybeitsadam.takt.ui.inspector.SwitchRow
import uk.co.maybeitsadam.takt.ui.review.Segmented
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme
import uk.co.maybeitsadam.takt.ui.theme.ThemeColors
import uk.co.maybeitsadam.takt.ui.theme.ThemeMode

/**
 * Settings → Theme: the appearance, every theme with a swatch of its
 * palette, what each imported file reported, import and remove, and this
 * device's opt-out from the shared choice.
 */
@Composable
internal fun ThemeSection(model: SettingsViewModel) {
    val state by model.themes.collectAsStateWithLifecycle()
    val error by model.themeError.collectAsStateWithLifecycle()
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri -> uri?.let(model::import) }
    val locked = state.specification.lockedAppearance

    Column(verticalArrangement = Arrangement.spacedBy(Metrics.sm)) {
        Text("Appearance", style = TaktTheme.type.label, color = TaktTheme.colors.mutedText)
        Segmented(
            options = ThemeMode.entries,
            selected = state.mode,
            label = { it.title },
            tag = { "settings_mode_${it.raw}" },
            onSelect = model::setMode,
        )
        if (locked != null) {
            Text(
                "${state.specification.name} is always ${locked.raw}, whatever this is set to.",
                style = TaktTheme.type.small, color = TaktTheme.colors.mutedText,
            )
        }
    }

    // Themes added on another device (the Mac shares every theme file it
    // loads, Zed's included) arrive by sync: ask for them as this opens.
    val sync = LocalShell.current.container.sync
    LaunchedEffect(Unit) { sync.syncNow() }

    Section(
        "Theme",
        footer = "A theme is a JSON file: colours for light and dark, and sizes. Anything it leaves out comes from Zed One, the default. See docs/themes.md.",
    ) {
        state.builtIns.forEachIndexed { index, theme ->
            if (index > 0) Hairline(color = TaktTheme.colors.borderMuted)
            ThemeRow(theme, state, onSelect = model::selectTheme, tag = "settings_theme_${theme.identifier}")
        }
        Hairline(color = TaktTheme.colors.borderMuted)
        SwitchRow(
            label = "Use a different theme on this device",
            detail = "Your choice of theme and appearance follows you to your other devices unless this is on.",
            checked = state.useDeviceChoice,
            onCheckedChange = model::setUseDeviceTheme,
            modifier = Modifier.padding(horizontal = Metrics.md).testTag("settings_theme_device"),
        )
    }

    // A file can hold a family (a Zed file often does): every theme in it is
    // listed, not only the last, each selectable on its own.
    val outcomes = state.library.outcomes.groupBy { it.source }
    Section(
        "Your themes",
        footer = "Themes from your Mac and any you import, shared with every device on your account.",
    ) {
        if (state.files.isEmpty()) {
            Text(
                "None yet. Themes in the Mac's themes folder, or Zed's, appear here once it has synced.",
                style = TaktTheme.type.small, color = TaktTheme.colors.mutedText,
                modifier = Modifier.padding(Metrics.md),
            )
        }
        state.files.forEachIndexed { index, file ->
            if (index > 0) Hairline(color = TaktTheme.colors.borderMuted)
            UserThemeRow(file, outcomes[file.name].orEmpty(), state, model)
        }
        Hairline(color = TaktTheme.colors.borderMuted)
        Row(Modifier.padding(Metrics.md), horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            PButton("Import theme…", modifier = Modifier.testTag("settings_import_theme")) {
                picker.launch(arrayOf("application/json", "text/*"))
            }
        }
        error?.let {
            Text(
                it, style = TaktTheme.type.small, color = TaktTheme.colors.danger,
                modifier = Modifier.padding(start = Metrics.md, end = Metrics.md, bottom = Metrics.md),
            )
        }
    }
}

/** A user file: every theme it loaded, or its name and why not; then its issues and a remove button. */
@Composable
private fun UserThemeRow(file: UserThemeFile, outcomes: List<ThemeFileOutcome>, state: ThemeLibraryState, model: SettingsViewModel) {
    val loaded = outcomes.mapNotNull { it.specification }
    Column {
        if (loaded.isNotEmpty()) {
            loaded.forEachIndexed { index, theme ->
                if (index > 0) Hairline(color = TaktTheme.colors.borderMuted, modifier = Modifier.padding(start = Metrics.md))
                ThemeRow(
                    theme, state, onSelect = model::selectTheme, tag = "settings_theme_${theme.identifier}",
                    detail = if (loaded.size > 1) "${file.name} · ${index + 1} of ${loaded.size}" else file.name,
                )
            }
        } else {
            Column(Modifier.fillMaxWidth().padding(horizontal = Metrics.md, vertical = Metrics.sm)) {
                Text(file.name, style = TaktTheme.type.body, color = TaktTheme.colors.ink)
                Text(
                    "Not loaded: ${outcomes.firstNotNullOfOrNull { it.skippedReason } ?: "it could not be read"}",
                    style = TaktTheme.type.small, color = TaktTheme.colors.danger,
                )
            }
        }
        val issues = outcomes.flatMap { it.issues }
            .filter { it.severity == ThemeIssueSeverity.ERROR || loaded.isEmpty() }
            .filterNot { loaded.isEmpty() && it.message.startsWith("not loaded:") }
            .distinctBy { it.message }
        if (issues.isNotEmpty()) ThemeIssues(issues)
        Row(
            Modifier.fillMaxWidth().padding(start = Metrics.md, end = Metrics.md, bottom = Metrics.sm),
            horizontalArrangement = Arrangement.End,
        ) {
            PButton("Remove", destructive = true, modifier = Modifier.testTag("settings_theme_remove_${file.name}")) {
                model.removeTheme(file.id)
            }
        }
    }
}

/** Errors, then warnings, then audit notes, each tagged by severity in its status hue. */
@Composable
private fun ThemeIssues(issues: List<ThemeFileIssue>) {
    val colors = TaktTheme.colors
    Column(
        Modifier.fillMaxWidth().padding(horizontal = Metrics.md, vertical = Metrics.xs),
        verticalArrangement = Arrangement.spacedBy(Metrics.xs),
    ) {
        for (issue in issues) {
            val (label, hue) = when (issue.severity) {
                ThemeIssueSeverity.ERROR -> "Error" to colors.danger
                ThemeIssueSeverity.WARNING -> "Warning" to colors.warning
                ThemeIssueSeverity.NOTE -> "Audit" to colors.primary
            }
            Row(horizontalArrangement = Arrangement.spacedBy(Metrics.sm), verticalAlignment = Alignment.Top) {
                Tag(label, color = hue)
                Text(issue.message, style = TaktTheme.type.small, color = colors.mutedText, modifier = Modifier.weight(1f))
            }
        }
    }
}

/** A selectable theme: a swatch of its palette, its name and summary, and a radio ring. */
@Composable
private fun ThemeRow(
    theme: ThemeSpecification,
    state: ThemeLibraryState,
    onSelect: (String) -> Unit,
    tag: String,
    detail: String? = null,
) {
    val selected = state.selection.selected == theme.identifier
    val colors = TaktTheme.colors
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = Metrics.touchTarget)
            .clickable(role = Role.RadioButton) { onSelect(theme.identifier) }
            .semantics { this.selected = selected }
            .testTag(tag)
            .padding(horizontal = Metrics.md, vertical = Metrics.sm),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Metrics.md),
    ) {
        PaletteSwatch(theme)
        Column(Modifier.weight(1f)) {
            Text(theme.name, style = TaktTheme.type.body, color = colors.ink)
            val caption = listOfNotNull(
                theme.lockedAppearance?.let { if (it == ThemeAppearance.DARK) "Always dark" else "Always light" },
                detail,
            ).joinToString(" · ")
            if (caption.isNotEmpty()) Text(caption, style = TaktTheme.type.small, color = colors.mutedText)
        }
        RadioRing(selected)
    }
}

/**
 * The theme's paper with its ink and four status hues on it, in each
 * appearance it can be drawn in: both halves for a theme that follows the
 * system, one for a locked theme.
 */
@Composable
private fun PaletteSwatch(theme: ThemeSpecification) {
    val appearances = theme.lockedAppearance?.let { listOf(it) } ?: ThemeAppearance.entries
    Row(
        Modifier
            .clip(TaktTheme.radii.controlShape)
            .border(BorderStroke(Metrics.hairline, TaktTheme.colors.border), TaktTheme.radii.controlShape),
    ) {
        for (appearance in appearances) {
            val c = ThemeColors.of(theme, appearance)
            Column(
                Modifier.background(c.paper).padding(Metrics.xs).width(SWATCH_HALF_WIDTH * 2 / appearances.size),
                verticalArrangement = Arrangement.spacedBy(Metrics.xxs),
            ) {
                Box(Modifier.fillMaxWidth().heightIn(min = Metrics.xs).background(c.ink, TaktTheme.radii.controlShape))
                Row(horizontalArrangement = Arrangement.spacedBy(Metrics.xxs)) {
                    for (hue in listOf(c.primary, c.success, c.danger, c.warning)) Dot(hue)
                }
            }
        }
    }
}

@Composable
private fun Dot(color: Color) {
    Box(Modifier.size(Metrics.sm).background(color, CircleShape))
}

/** A ring that fills with primary when chosen. */
@Composable
internal fun RadioRing(selected: Boolean) {
    val colors = TaktTheme.colors
    Box(
        Modifier.size(RADIO_SIZE).border(
            BorderStroke(RADIO_STROKE, if (selected) colors.primary else colors.inputBorder), CircleShape,
        ),
        contentAlignment = Alignment.Center,
    ) {
        if (selected) Box(Modifier.size(RADIO_SIZE / 2).background(colors.primary, CircleShape))
    }
}

// Layout constants, not theme values: the swatch's width and the radio's glyph.
private val SWATCH_HALF_WIDTH = 22.dp
private val RADIO_SIZE = 20.dp
private val RADIO_STROKE = 1.5.dp
