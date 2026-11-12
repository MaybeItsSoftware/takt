package uk.co.maybeitsadam.takt.ui.theme

import androidx.compose.foundation.IndicationNodeFactory
import androidx.compose.foundation.interaction.InteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.ContentDrawScope
import androidx.compose.ui.node.DelegatableNode
import androidx.compose.ui.node.DrawModifierNode
import androidx.compose.ui.node.invalidateDraw
import kotlinx.coroutines.launch

/**
 * The press as the Mac shows a hover: the `hover` fill behind the content,
 * edge to edge of whatever the clickable is clipped to, and no ripple.
 */
internal class PressFill(private val color: Color) : IndicationNodeFactory {
    override fun create(interactionSource: InteractionSource): DelegatableNode = PressFillNode(interactionSource, color)

    override fun equals(other: Any?): Boolean = other is PressFill && other.color == color

    override fun hashCode(): Int = color.hashCode()
}

private class PressFillNode(private val source: InteractionSource, private val color: Color) : Modifier.Node(), DrawModifierNode {
    private var pressed = 0

    override fun onAttach() {
        coroutineScope.launch {
            source.interactions.collect { interaction ->
                when (interaction) {
                    is PressInteraction.Press -> pressed++
                    is PressInteraction.Release, is PressInteraction.Cancel -> pressed = (pressed - 1).coerceAtLeast(0)
                    else -> return@collect
                }
                invalidateDraw()
            }
        }
    }

    override fun ContentDrawScope.draw() {
        if (pressed > 0) drawRect(color)
        drawContent()
    }
}
