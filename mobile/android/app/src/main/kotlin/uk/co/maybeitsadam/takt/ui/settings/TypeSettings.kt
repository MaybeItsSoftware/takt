package uk.co.maybeitsadam.takt.ui.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import uk.co.maybeitsadam.takt.core.theme.BundledFontFamily
import uk.co.maybeitsadam.takt.core.theme.ThemeFontFace
import uk.co.maybeitsadam.takt.core.theme.ThemeFontRole
import uk.co.maybeitsadam.takt.core.theme.ThemeTypographyOverride
import uk.co.maybeitsadam.takt.ui.SheetHandle
import uk.co.maybeitsadam.takt.ui.components.Hairline
import uk.co.maybeitsadam.takt.ui.components.PButton
import uk.co.maybeitsadam.takt.ui.review.Segmented
import uk.co.maybeitsadam.takt.ui.theme.Fonts
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme

/**
 * Settings → Type: a face for the interface, headings and numerals, and a
 * text size, laid over whichever theme is chosen so a theme switch keeps
 * them. The Mac's Appearance → Type, with the bundled faces only: an Android
 * app cannot use the fonts installed on the device.
 */
@Composable
internal fun TypeSection(model: SettingsViewModel) {
    val state by model.themes.collectAsStateWithLifecycle()
    val override = state.typography
    val themeType = state.themeSpecification.structure.typography
    var picking by rememberSaveable { mutableStateOf<ThemeFontRole?>(null) }

    CollapsibleSection(
        "Type",
        key = "type",
        summary = if (override.isEmpty) "The theme's" else "Your own",
        footer = "Inter, Geist, IBM Plex Sans, Arvo, Lilex, JetBrains Mono and Geist Mono ship with Takt, so they " +
            "look the same on every device. Your choices are kept on this device and apply over every theme.",
    ) {
        ThemeFontRole.entries.forEachIndexed { index, role ->
            if (index > 0) Hairline(color = TaktTheme.colors.borderMuted)
            FontRow(role, override.family(role), role.face(themeType)) { picking = role }
        }
        Hairline(color = TaktTheme.colors.borderMuted)
        Column(Modifier.padding(Metrics.md), verticalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            Text("Text size", style = TaktTheme.type.body, color = TaktTheme.colors.ink)
            Segmented(
                options = ThemeTypographyOverride.TEXT_SCALES,
                selected = ThemeTypographyOverride.nearestTextScale(override.effectiveTextScale),
                label = { "${Math.round(it * 100)}%" },
                tag = { "settings_text_size_${Math.round(it * 100)}" },
                onSelect = { scale -> model.setTypography(override.copy(textScale = scale.takeUnless { it == 1.0 })) },
            )
            Text(
                "Scales every size the theme sets, in proportion: ${points(themeType.bodySize * override.effectiveTextScale)} " +
                    "body text. Android's own font size applies on top.",
                style = TaktTheme.type.small, color = TaktTheme.colors.mutedText,
            )
        }
        Hairline(color = TaktTheme.colors.borderMuted)
        TypePreview()
        Hairline(color = TaktTheme.colors.borderMuted)
        Row(
            Modifier.fillMaxWidth().padding(Metrics.md),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(Metrics.sm),
        ) {
            Text(
                if (override.isEmpty) "Every face and size is the theme's." else "Your choices apply over every theme.",
                style = TaktTheme.type.small, color = TaktTheme.colors.mutedText, modifier = Modifier.weight(1f),
            )
            PButton("Reset to theme", enabled = !override.isEmpty, modifier = Modifier.testTag("settings_type_reset")) {
                model.setTypography(ThemeTypographyOverride())
            }
        }
    }

    picking?.let { role ->
        FontSheet(
            role = role,
            selection = override.family(role),
            themeFace = role.face(themeType),
            onChoose = { family ->
                model.setTypography(override.withFamily(role, family))
                picking = null
            },
            onDismiss = { picking = null },
        )
    }
}

/** A role, what it covers, and the current choice written in its own face. */
@Composable
private fun FontRow(role: ThemeFontRole, family: String?, themeFace: ThemeFontFace, onClick: () -> Unit) {
    val colors = TaktTheme.colors
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = Metrics.touchTarget)
            .clickable(role = Role.Button, onClick = onClick)
            .testTag("settings_font_${role.name.lowercase()}")
            .padding(horizontal = Metrics.md, vertical = Metrics.sm),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Metrics.md),
    ) {
        Column(Modifier.weight(1f)) {
            Text(role.title, style = TaktTheme.type.body, color = colors.ink)
            Text(role.detail, style = TaktTheme.type.small, color = colors.mutedText)
        }
        Text(
            family ?: "Theme · ${describe(themeFace)}",
            style = TaktTheme.type.body.copy(fontFamily = preview(family, themeFace)),
            color = if (family == null) colors.mutedText else colors.ink,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** The theme's default, then every bundled family, those suited to the role first, each set in itself. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun FontSheet(
    role: ThemeFontRole,
    selection: String?,
    themeFace: ThemeFontFace,
    onChoose: (String?) -> Unit,
    onDismiss: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = TaktTheme.colors.altRow,
        contentColor = TaktTheme.colors.ink,
        tonalElevation = 0.dp,
        shape = Metrics.sheetTop,
        dragHandle = { SheetHandle() },
    ) {
        Column(Modifier.verticalScroll(rememberScrollState()).navigationBarsPadding().padding(bottom = Metrics.md)) {
            Text(
                "${role.title} font",
                style = TaktTheme.type.label, color = TaktTheme.colors.mutedText,
                modifier = Modifier.padding(horizontal = Metrics.lg, vertical = Metrics.xs),
            )
            FontChoice(
                title = "Theme default",
                note = describe(themeFace),
                family = Fonts.resolve(themeFace),
                selected = selection == null,
                tag = "settings_font_choice_default",
            ) { onChoose(null) }
            for (bundled in BundledFontFamily.ordered(role)) {
                Hairline(color = TaktTheme.colors.borderMuted)
                FontChoice(
                    title = bundled.name,
                    note = bundled.note,
                    family = Fonts.resolve(ThemeFontFace(listOf(bundled.name), bundled.design)),
                    selected = selection == bundled.name,
                    tag = "settings_font_choice_${bundled.name}",
                ) { onChoose(bundled.name) }
            }
        }
    }
}

@Composable
private fun FontChoice(title: String, note: String, family: FontFamily, selected: Boolean, tag: String, onClick: () -> Unit) {
    val colors = TaktTheme.colors
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = Metrics.touchTarget)
            .clickable(role = Role.RadioButton, onClick = onClick)
            .semantics { this.selected = selected }
            .testTag(tag)
            .padding(horizontal = Metrics.lg, vertical = Metrics.sm),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(Metrics.md),
    ) {
        Column(Modifier.weight(1f)) {
            // The family's name in the family itself: the preview is the point.
            Text(title, style = TaktTheme.type.title.copy(fontFamily = family), color = colors.ink, maxLines = 1)
            Text(note, style = TaktTheme.type.small, color = colors.mutedText, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (selected) Icon(Icons.Filled.Check, contentDescription = "Chosen", tint = colors.primary)
    }
}

/** A specimen of the three faces and the scale, drawn in the theme in force. */
@Composable
private fun TypePreview() {
    val colors = TaktTheme.colors
    val type = TaktTheme.type
    Column(
        Modifier.fillMaxWidth().padding(Metrics.md).semantics(mergeDescendants = true) {},
        verticalArrangement = Arrangement.spacedBy(Metrics.xs),
    ) {
        Text("Plan the week", style = type.title, color = colors.ink)
        Text("Draft the release notes and send them for review before Thursday.", style = type.body, color = colors.ink)
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Metrics.sm)) {
            val label = if (TaktTheme.microLabel.uppercase) "DUE TOMORROW" else "Due tomorrow"
            Text(label, style = type.label, color = colors.mutedText)
            Text("25:00", style = type.monoBody, color = colors.primary)
            Text("Caption text", style = type.small, color = colors.mutedText)
        }
    }
}

private fun describe(face: ThemeFontFace): String = face.families.firstOrNull() ?: "System ${face.design.raw}"

private fun preview(family: String?, themeFace: ThemeFontFace): FontFamily =
    Fonts.resolve(if (family == null) themeFace else ThemeFontFace(listOf(family) + themeFace.families, themeFace.design))

private fun points(value: Double): String =
    if (value == Math.rint(value)) "${value.toLong()}sp" else String.format(java.util.Locale.UK, "%.1fsp", value)
