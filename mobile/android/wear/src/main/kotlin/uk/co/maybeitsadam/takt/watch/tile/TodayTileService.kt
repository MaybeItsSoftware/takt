package uk.co.maybeitsadam.takt.watch.tile

import androidx.annotation.OptIn
import androidx.concurrent.futures.CallbackToFutureAdapter
import androidx.wear.protolayout.ActionBuilders
import androidx.wear.protolayout.ColorBuilders.argb
import androidx.wear.protolayout.DimensionBuilders.dp
import androidx.wear.protolayout.DimensionBuilders.expand
import androidx.wear.protolayout.DimensionBuilders.sp
import androidx.wear.protolayout.LayoutElementBuilders
import androidx.wear.protolayout.ModifiersBuilders
import androidx.wear.protolayout.ResourceBuilders
import androidx.wear.protolayout.TimelineBuilders
import androidx.wear.protolayout.expression.ProtoLayoutExperimental
import androidx.wear.tiles.RequestBuilders
import androidx.wear.tiles.TileBuilders
import androidx.wear.tiles.TileService
import com.google.common.util.concurrent.ListenableFuture
import uk.co.maybeitsadam.takt.watch.MainActivity
import uk.co.maybeitsadam.takt.watch.WatchGlance
import uk.co.maybeitsadam.takt.watch.watch
import uk.co.maybeitsadam.takt.wear.contract.withPending

/**
 * The Today tile: the running block or the next task, and how many are left.
 * Redrawn whenever the day changes ([uk.co.maybeitsadam.takt.watch.WatchApplication.refreshGlanceables]),
 * and each minute while a block runs so its minutes move on.
 */
class TodayTileService : TileService() {
    override fun onTileRequest(requestParams: RequestBuilders.TileRequest): ListenableFuture<TileBuilders.Tile> =
        CallbackToFutureAdapter.getFuture { completer ->
            val store = watch.store
            val snapshot = store.snapshot.value?.withPending(store.awaiting.value + store.queued.value)
            val glance = WatchGlance.of(snapshot, System.currentTimeMillis())
            val tile = TileBuilders.Tile.Builder()
                .setResourcesVersion(RESOURCES_VERSION)
                .setFreshnessIntervalMillis(if (glance.isFocusing) 60_000L else 15 * 60_000L)
                .setTileTimeline(TimelineBuilders.Timeline.fromLayoutElement(layout(glance)))
                .build()
            completer.set(tile)
            "today tile"
        }

    override fun onTileResourcesRequest(requestParams: RequestBuilders.ResourcesRequest): ListenableFuture<ResourceBuilders.Resources> =
        CallbackToFutureAdapter.getFuture { completer ->
            completer.set(ResourceBuilders.Resources.Builder().setVersion(RESOURCES_VERSION).build())
            "today tile resources"
        }

    private fun layout(glance: WatchGlance): LayoutElementBuilders.LayoutElement {
        val open = ModifiersBuilders.Clickable.Builder()
            .setId("open")
            .setOnClick(
                ActionBuilders.LaunchAction.Builder()
                    .setAndroidActivity(
                        ActionBuilders.AndroidActivity.Builder()
                            .setPackageName(packageName)
                            .setClassName(MainActivity::class.java.name)
                            .build(),
                    )
                    .build(),
            )
            .build()
        val eyebrowColor = if (glance.isFocusing) (if (glance.countingFrom == null) AMBER else AZURE) else MUTED
        val column = LayoutElementBuilders.Column.Builder()
            .setHorizontalAlignment(LayoutElementBuilders.HORIZONTAL_ALIGN_CENTER)
            .addContent(text(eyebrowLine(glance), 13f, eyebrowColor, bold = true))
            .addContent(spacer(6f))
            .addContent(text(glance.title ?: "Nothing to do", 17f, if (glance.title == null) MUTED else INK, bold = glance.title != null, maxLines = 3))
            .addContent(spacer(8f))
            .addContent(text(glance.footer, 12f, MUTED))
            .build()
        return LayoutElementBuilders.Box.Builder()
            .setWidth(expand())
            .setHeight(expand())
            .setHorizontalAlignment(LayoutElementBuilders.HORIZONTAL_ALIGN_CENTER)
            .setVerticalAlignment(LayoutElementBuilders.VERTICAL_ALIGN_CENTER)
            .setModifiers(
                ModifiersBuilders.Modifiers.Builder()
                    .setClickable(open)
                    .setPadding(ModifiersBuilders.Padding.Builder().setAll(dp(28f)).build())
                    .build(),
            )
            .addContent(column)
            .build()
    }

    /** `Focusing · 12m in`, `Paused · 12m in`, `Next`. */
    private fun eyebrowLine(glance: WatchGlance): String =
        if (glance.isFocusing) "${glance.eyebrow} · ${glance.elapsedSeconds / 60}m in" else glance.eyebrow

    @OptIn(markerClass = [ProtoLayoutExperimental::class])
    private fun text(value: String, size: Float, color: Int, bold: Boolean = false, maxLines: Int = 1): LayoutElementBuilders.Text =
        LayoutElementBuilders.Text.Builder()
            .setText(value)
            .setMaxLines(maxLines)
            .setMultilineAlignment(LayoutElementBuilders.TEXT_ALIGN_CENTER)
            .setOverflow(LayoutElementBuilders.TEXT_OVERFLOW_ELLIPSIZE)
            .setFontStyle(
                LayoutElementBuilders.FontStyle.Builder()
                    .setSize(sp(size))
                    .setColor(argb(color))
                    .setWeight(if (bold) LayoutElementBuilders.FONT_WEIGHT_MEDIUM else LayoutElementBuilders.FONT_WEIGHT_NORMAL)
                    .build(),
            )
            .build()

    private fun spacer(height: Float) = LayoutElementBuilders.Spacer.Builder().setHeight(dp(height)).build()

    private companion object {
        const val RESOURCES_VERSION = "1"
        const val INK = 0xFFDCE0E5.toInt()
        const val MUTED = 0xFFA9AFBC.toInt()
        const val AZURE = 0xFF74ADE8.toInt()
        const val AMBER = 0xFFDEC184.toInt()
    }
}
