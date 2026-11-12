package uk.co.maybeitsadam.takt.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import uk.co.maybeitsadam.takt.ui.theme.Metrics
import uk.co.maybeitsadam.takt.ui.theme.TaktTheme

/**
 * How rows are framed on Today, in a list's outline and in the inspector, so
 * the phone has the Mac's bordered panels rather than rows loose on the page.
 * Chosen per device in Settings › Appearance.
 */
enum class CardStyle(val raw: String, val title: String, val detail: String) {
    /** Each task its own bordered card, a gap between them, as the board's cards are. */
    TASK_CARDS("tasks", "Every task a card", "Each row its own bordered card"),

    /** A section's rows in one bordered panel, split by hairlines, its heading inside. */
    SECTION_CARDS("sections", "Sections as cards", "Each section one panel, rows split by hairlines"),

    /** Flat rows on the page, a hairline between each. */
    HAIRLINES("hairlines", "Hairlines", "Flat rows with a line between each"),
    ;

    companion object {
        val DEFAULT = TASK_CARDS
        fun of(raw: String?): CardStyle = entries.firstOrNull { it.raw == raw } ?: DEFAULT
    }
}

/** The device's [CardStyle], provided by the app shell. */
val LocalCardStyle = compositionLocalOf { CardStyle.DEFAULT }

/** Where a row stands in its section, which decides the corners and edges it draws. */
data class RowPlace(val first: Boolean, val last: Boolean) {
    companion object {
        fun of(index: Int, count: Int) = RowPlace(first = index == 0, last = index == count - 1)
        val ALONE = RowPlace(first = true, last = true)
    }
}

/**
 * Frames one row as the [CardStyle] has it:
 *
 * - [CardStyle.TASK_CARDS]: inset from the page's edge, on the raised fill
 *   inside a muted hairline at the item radius, a small gap above and below.
 *   [selected] edges it in the focus colour, as the Mac's selection is.
 * - [CardStyle.SECTION_CARDS]: one slice of the section's panel — side
 *   edges always, the top edge and corners when [place] is first, the bottom
 *   when last, and an inset muted hairline between rows. A section heading
 *   drawn with [sectionHeaderFrame] is the panel's first slice.
 * - [CardStyle.HAIRLINES]: the row flat on the page with a muted hairline
 *   under it unless it is last.
 *
 * The clip means a swipe's reveal and a press stay inside the card's shape.
 */
@Composable
fun Modifier.rowFrame(place: RowPlace, selected: Boolean = false, inset: Dp = Metrics.md): Modifier {
    val colors = TaktTheme.colors
    val hairline = Metrics.hairline
    return when (LocalCardStyle.current) {
        CardStyle.TASK_CARDS -> {
            val shape = Metrics.card
            this
                .padding(horizontal = inset, vertical = 3.dp)
                .clip(shape)
                .background(colors.raised, shape)
                .border(hairline, if (selected) colors.focus else colors.borderMuted, shape)
        }
        CardStyle.SECTION_CARDS -> this
            .padding(horizontal = inset)
            .panelSlice(place, colors.altRow, if (selected) colors.focus else colors.border, colors.borderMuted, hairline, TaktTheme.radii.popover)
        CardStyle.HAIRLINES -> if (place.last) this else this.underline(colors.borderMuted, hairline, inset)
    }
}

/**
 * A section heading's frame: in [CardStyle.SECTION_CARDS] the top slice of
 * the panel its rows continue (closed at the bottom too when the section is
 * empty); otherwise unchanged, with a stronger rule under it in
 * [CardStyle.HAIRLINES].
 */
@Composable
fun Modifier.sectionHeaderFrame(hasRows: Boolean = true, inset: Dp = Metrics.md): Modifier {
    val colors = TaktTheme.colors
    val hairline = Metrics.hairline
    return when (LocalCardStyle.current) {
        CardStyle.SECTION_CARDS -> this
            .padding(start = inset, end = inset, top = Metrics.sm)
            .panelSlice(RowPlace(first = true, last = !hasRows), colors.altRow, colors.border, colors.borderMuted, hairline, TaktTheme.radii.popover)
        CardStyle.HAIRLINES -> this.underline(colors.border, hairline, 0.dp)
        CardStyle.TASK_CARDS -> this
    }
}

/** The fill a framed row paints under its content, so a swipe's reveal stays hidden until it moves. */
val rowFill: Color
    @Composable @ReadOnlyComposable get() = when (LocalCardStyle.current) {
        CardStyle.TASK_CARDS -> TaktTheme.colors.raised
        CardStyle.SECTION_CARDS -> TaktTheme.colors.altRow
        CardStyle.HAIRLINES -> TaktTheme.colors.paper
    }

/** Whether a panel's rows are drawn inside one bordered panel, so callers can drop their own padding or rules. */
val isSectionPanel: Boolean
    @Composable @ReadOnlyComposable get() = LocalCardStyle.current == CardStyle.SECTION_CARDS

private fun Modifier.underline(color: Color, width: Dp, inset: Dp): Modifier = drawWithContent {
    drawContent()
    val stroke = width.toPx()
    val y = size.height - stroke / 2
    drawLine(color, Offset(inset.toPx(), y), Offset(size.width - inset.toPx(), y), stroke)
}

/**
 * One horizontal slice of a bordered, rounded panel. The panel's outline is
 * drawn whole and clipped, so a middle slice shows only its side edges; the
 * rule between slices is drawn once, inset, in [divider].
 */
private fun Modifier.panelSlice(
    place: RowPlace,
    fill: Color,
    edge: Color,
    divider: Color,
    width: Dp,
    radius: Dp,
): Modifier {
    val shape: Shape = when {
        place.first && place.last -> RoundedCornerShape(radius)
        place.first -> RoundedCornerShape(topStart = radius, topEnd = radius)
        place.last -> RoundedCornerShape(bottomStart = radius, bottomEnd = radius)
        else -> RectangleShape
    }
    return this
        .clip(shape)
        .background(fill, shape)
        .drawWithContent {
            drawContent()
            val stroke = width.toPx()
            // Draw the outline a stroke taller at each open end, so its top or
            // bottom edge falls outside the slice and is clipped away.
            val top = if (place.first) 0f else -stroke * 2
            val bottom = if (place.last) size.height else size.height + stroke * 2
            clipRect(0f, 0f, size.width, size.height) {
                val outline = shape.createOutline(
                    androidx.compose.ui.geometry.Size(size.width - stroke, bottom - top - stroke),
                    layoutDirection,
                    this,
                )
                drawContext.transform.translate(stroke / 2, top + stroke / 2)
                drawOutline(outline, edge, style = Stroke(stroke))
                drawContext.transform.translate(-stroke / 2, -(top + stroke / 2))
            }
            if (!place.last) {
                val y = size.height - stroke / 2
                val inset = 12.dp.toPx()
                drawLine(divider, Offset(inset, y), Offset(size.width - inset, y), stroke)
            }
        }
}
