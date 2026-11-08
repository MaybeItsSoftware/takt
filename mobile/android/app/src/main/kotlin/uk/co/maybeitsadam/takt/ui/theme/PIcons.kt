package uk.co.maybeitsadam.takt.ui.theme

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.unit.dp

/**
 * The icons the app needs beyond `material-icons-core`, as 24dp path data.
 * (The extended icon artefact is avoided: dexing it costs minutes per build.)
 * Add more with [icon]; tint them like any other `ImageVector`.
 */
object PIcons {
    fun icon(name: String, vararg paths: String): ImageVector {
        val builder = ImageVector.Builder(name = name, defaultWidth = 24.dp, defaultHeight = 24.dp, viewportWidth = 24f, viewportHeight = 24f)
        for (path in paths) {
            builder.addPath(pathData = PathParser().parsePathString(path).toNodes(), fill = SolidColor(Color.Black))
        }
        return builder.build()
    }

    val Undo by lazy { icon("Undo", "M12.5,8c-2.65,0 -5.05,0.99 -6.9,2.6L2,7v9h9l-3.62,-3.62c1.39,-1.16 3.16,-1.88 5.12,-1.88 3.54,0 6.55,2.31 7.6,5.5l2.37,-0.78C21.08,11.03 17.15,8 12.5,8z") }
    val Redo by lazy { icon("Redo", "M18.4,10.6C16.55,8.99 14.15,8 11.5,8c-4.65,0 -8.58,3.03 -9.96,7.22L3.9,16c1.05,-3.19 4.05,-5.5 7.6,-5.5 1.95,0 3.73,0.72 5.12,1.88L13,16h9V7l-3.6,3.6z") }
    val History by lazy { icon("History", "M13,3c-4.97,0 -9,4.03 -9,9L1,12l3.89,3.89 0.07,0.14L9,12L6,12c0,-3.87 3.13,-7 7,-7s7,3.13 7,7 -3.13,7 -7,7c-1.93,0 -3.68,-0.79 -4.94,-2.06l-1.42,1.42C8.27,19.99 10.51,21 13,21c4.97,0 9,-4.03 9,-9s-4.03,-9 -9,-9zM12,8v5l4.28,2.54 0.72,-1.21 -3.5,-2.08L13.5,8L12,8z") }
    val Pause by lazy { icon("Pause", "M6,19h4L10,5L6,5v14zM14,5v14h4L18,5h-4z") }
    val Stop by lazy { icon("Stop", "M6,6h12v12H6z") }
    val DragHandle by lazy { icon("DragHandle", "M11,18c0,1.1 -0.9,2 -2,2s-2,-0.9 -2,-2 0.9,-2 2,-2 2,0.9 2,2zM9,10c-1.1,0 -2,0.9 -2,2s0.9,2 2,2 2,-0.9 2,-2 -0.9,-2 -2,-2zM9,4c-1.1,0 -2,0.9 -2,2s0.9,2 2,2 2,-0.9 2,-2 -0.9,-2 -2,-2zM15,8c1.1,0 2,-0.9 2,-2s-0.9,-2 -2,-2 -2,0.9 -2,2 0.9,2 2,2zM15,10c-1.1,0 -2,0.9 -2,2s0.9,2 2,2 2,-0.9 2,-2 -0.9,-2 -2,-2zM15,16c-1.1,0 -2,0.9 -2,2s0.9,2 2,2 2,-0.9 2,-2 -0.9,-2 -2,-2z") }
    val Today by lazy { icon("Today", "M12,7a5,5 0,1 0,0.001 0zM12,9a3,3 0,1 1,-0.001 0z", "M11,1h2v3h-2zM11,20h2v3h-2zM1,11h3v2H1zM20,11h3v2h-3zM4.22,5.64l1.42,-1.42 2.12,2.12 -1.42,1.42zM16.24,17.66l1.42,-1.42 2.12,2.12 -1.42,1.42zM4.22,18.36l2.12,-2.12 1.42,1.42 -2.12,2.12zM16.24,6.34l2.12,-2.12 1.42,1.42 -2.12,2.12z") }
    val Focus by lazy { icon("Focus", "M12,2a10,10 0,1 0,0.001 0zM12,4a8,8 0,1 1,-0.001 0z", "M12,7a5,5 0,1 0,0.001 0zM12,9a3,3 0,1 1,-0.001 0z", "M12,11a1,1 0,1 0,0.001 0z") }
    val Review by lazy { icon("Review", "M4,13h3v7H4zM10.5,9h3v11h-3zM17,4h3v16h-3z", "M2,21h20v1.5H2z") }
    val ListIcon by lazy { icon("Lists", "M3,5h2v2H3zM8,5h13v2H8zM3,11h2v2H3zM8,11h13v2H8zM3,17h2v2H3zM8,17h13v2H8z") }
    /** Material's "group": a list shared with other people. */
    val Shared by lazy { icon("Shared", "M16,11c1.66,0 2.99,-1.34 2.99,-3S17.66,5 16,5c-1.66,0 -3,1.34 -3,3s1.34,3 3,3zM8,11c1.66,0 2.99,-1.34 2.99,-3S9.66,5 8,5C6.34,5 5,6.34 5,8s1.34,3 3,3zM8,13c-2.33,0 -7,1.17 -7,3.5V19h14v-2.5c0,-2.33 -4.67,-3.5 -7,-3.5zM16,13c-0.29,0 -0.62,0.02 -0.97,0.05 1.16,0.84 1.97,1.97 1.97,3.45V19h6v-2.5c0,-2.33 -4.67,-3.5 -7,-3.5z") }
    val Folder by lazy { icon("Folder", "M10,4H4c-1.1,0 -1.99,0.9 -1.99,2L2,18c0,1.1 0.9,2 2,2h16c1.1,0 2,-0.9 2,-2V8c0,-1.1 -0.9,-2 -2,-2h-8l-2,-2z") }
    val Flag by lazy { icon("Flag", "M14.4,6L14,4H5v17h2v-7h5.6l0.4,2h7V6z") }
    val Timer by lazy { icon("Timer", "M11.99,2C6.47,2 2,6.48 2,12s4.47,10 9.99,10C17.52,22 22,17.52 22,12S17.52,2 11.99,2zM12,20c-4.42,0 -8,-3.58 -8,-8s3.58,-8 8,-8 8,3.58 8,8 -3.58,8 -8,8zM12.5,7H11v6l5.25,3.15 0.75,-1.23 -4.5,-2.67z") }
    val Indent by lazy { icon("Indent", "M3,21h18v-2H3v2zM3,8v8l4,-4 -4,-4zM11,17h10v-2H11v2zM3,3v2h18V3H3zM11,9h10V7H11v2zM11,13h10v-2H11v2z") }
    val Outdent by lazy { icon("Outdent", "M11,17h10v-2H11v2zM3,12l4,4L7,8l-4,4zM3,21h18v-2L3,19v2zM3,3v2h18L21,3L3,3zM11,9h10L21,7L11,7v2zM11,13h10v-2L11,11v2z") }
    val Repeat by lazy { icon("Repeat", "M7,7h10v3l4,-4 -4,-4v3H5v6h2V7zM17,17H7v-3l-4,4 4,4v-3h12v-6h-2v4z") }
    val Tag by lazy { icon("Tag", "M17.63,5.84C17.27,5.33 16.67,5 16,5L5,5.01C3.9,5.01 3,5.9 3,7v10c0,1.1 0.9,1.99 2,1.99L16,19c0.67,0 1.27,-0.33 1.63,-0.84L22,12l-4.37,-6.16z") }
    val Link by lazy { icon("Link", "M3.9,12c0,-1.71 1.39,-3.1 3.1,-3.1h4V7H7c-2.76,0 -5,2.24 -5,5s2.24,5 5,5h4v-1.9H7c-1.71,0 -3.1,-1.39 -3.1,-3.1zM8,13h8v-2H8v2zM17,7h-4v1.9h4c1.71,0 3.1,1.39 3.1,3.1s-1.39,3.1 -3.1,3.1h-4V17h4c2.76,0 5,-2.24 5,-5s-2.24,-5 -5,-5z") }
    val Grid by lazy { icon("Grid", "M3,3v8h8V3H3zM9,9H5V5h4V9zM3,13v8h8v-8H3zM9,19H5v-4h4V19zM13,3v8h8V3H13zM19,9h-4V5h4V9zM13,13v8h8v-8H13zM19,19h-4v-4h4V19z") }
    val Board by lazy { icon("Board", "M3,4h5v16H3zM9.5,4h5v10h-5zM16,4h5v13h-5z") }
    val Outline by lazy { icon("Outline", "M3,4h18v2H3zM7,9h14v2H7zM7,14h14v2H7zM3,19h18v2H3z") }
    val Sync by lazy { icon("Sync", "M12,4V1L8,5l4,4V6c3.31,0 6,2.69 6,6 0,1.01 -0.25,1.97 -0.7,2.8l1.46,1.46C19.54,15.03 20,13.57 20,12c0,-4.42 -3.58,-8 -8,-8zM12,18c-3.31,0 -6,-2.69 -6,-6 0,-1.01 0.25,-1.97 0.7,-2.8L5.24,7.74C4.46,8.97 4,10.43 4,12c0,4.42 3.58,8 8,8v3l4,-4 -4,-4v3z") }
    val Inbox by lazy { icon("Inbox", "M19,3H4.99C3.88,3 3,3.9 3,5v14c0,1.1 0.88,2 1.99,2H19c1.1,0 2,-0.9 2,-2V5c0,-1.1 -0.9,-2 -2,-2zM19,15h-4c0,1.66 -1.35,3 -3,3s-3,-1.34 -3,-3H4.99V5H19v10z") }
    val Archive by lazy { icon("Archive", "M20.54,5.23l-1.39,-1.68C18.88,3.21 18.47,3 18,3H6c-0.47,0 -0.88,0.21 -1.16,0.55L3.46,5.23C3.17,5.57 3,6.02 3,6.5V19c0,1.1 0.9,2 2,2h14c1.1,0 2,-0.9 2,-2V6.5c0,-0.48 -0.17,-0.93 -0.46,-1.27zM12,17.5L6.5,12H10v-2h4v2h3.5L12,17.5zM5.12,5l0.81,-1h12l0.94,1H5.12z") }
    val Copy by lazy { icon("Copy", "M16,1H4c-1.1,0 -2,0.9 -2,2v14h2V3h12V1zM19,5H8c-1.1,0 -2,0.9 -2,2v14c0,1.1 0.9,2 2,2h11c1.1,0 2,-0.9 2,-2V7c0,-1.1 -0.9,-2 -2,-2zM19,21H8V7h11v14z") }
    val Notes by lazy { icon("Notes", "M3,18h12v-2H3v2zM3,6v2h18V6H3zM3,13h18v-2H3v2z") }
    val Calendar by lazy { icon("Calendar", "M19,4h-1V2h-2v2H8V2H6v2H5c-1.11,0 -1.99,0.9 -1.99,2L3,20c0,1.1 0.89,2 2,2h14c1.1,0 2,-0.9 2,-2V6c0,-1.1 -0.9,-2 -2,-2zM19,20H5V9h14v11z") }
    val Expand by lazy { icon("Expand", "M5,5h5v2H7v3H5V5zm14,0h-5v2h3v3h2V5zm0,14h-5v-2h3v-3h2v5zm-14,0h5v-2H7v-3H5v5z") }
    /** Zed's panel-left: a frame with a sidebar ruled off. */
    val Sidebar by lazy { icon("Sidebar", "M3,4h18v2H3z", "M3,18h18v2H3z", "M3,4h2v16H3z", "M19,4h2v16h-2z", "M9,4h2v16H9z") }
    val Keyboard by lazy { icon("Keyboard", "M20,5H4c-1.1,0 -1.99,0.9 -1.99,2L2,17c0,1.1 0.9,2 2,2h16c1.1,0 2,-0.9 2,-2V7c0,-1.1 -0.9,-2 -2,-2zM11,8h2v2h-2V8zM11,11h2v2h-2v-2zM8,8h2v2H8V8zM8,11h2v2H8v-2zM7,13H5v-2h2v2zM7,10H5V8h2v2zM16,17H8v-2h8v2zM16,13h-2v-2h2v2zM16,10h-2V8h2v2zM19,13h-2v-2h2v2zM19,10h-2V8h2v2z") }
}
