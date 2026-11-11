package uk.co.maybeitsadam.takt.ui.theme

import java.io.File
import androidx.compose.ui.text.font.FontFamily
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import uk.co.maybeitsadam.takt.core.theme.BuiltInThemeSpecifications
import uk.co.maybeitsadam.takt.core.theme.BundledFontFamily
import uk.co.maybeitsadam.takt.core.theme.ThemeFontFace
import uk.co.maybeitsadam.takt.core.theme.ThemePlatform

/**
 * The default theme ("Takt", `priority.json`) names IBM Plex Sans and Lilex, as the Mac draws.
 * They must resolve to the bundled faces, not the system fallback, and the
 * files they come from must be in `res/font`, or the default theme quietly
 * renders in Roboto.
 */
class BundledFontsTest {
    private val typography = BuiltInThemeSpecifications.defaultTheme(ThemePlatform.ANDROID).structure.typography

    @Test fun theDefaultThemeResolvesToTheBundledPlexAndLilex() {
        assertSame(Fonts.sans, Fonts.resolve(typography.body))
        assertSame(Fonts.sans, Fonts.resolve(typography.display))
        assertSame(Fonts.mono, Fonts.resolve(typography.mono))
    }

    @Test fun theFaceFilesAreBundled() {
        val fonts = File("src/main/res/font")
        for (name in listOf("ibmplexsans_regular", "ibmplexsans_medium", "ibmplexsans_semibold", "ibmplexsans_bold",
            "lilex_regular", "lilex_medium", "lilex_bold", "inter_regular", "inter_medium", "inter_semibold", "inter_bold", "inter_italic",
            "geistmono_regular", "geistmono_medium", "geistmono_bold")) {
            val file = File(fonts, "$name.ttf")
            assertTrue("$file is missing", file.isFile && file.length() > 10_000)
        }
    }

    /** Every family the type settings offer is a bundled face, never the system fallback. */
    @Test fun everyFamilyTheTypeSettingsOfferIsBundled() {
        val system = setOf(FontFamily.SansSerif, FontFamily.Serif, FontFamily.Monospace, FontFamily.Default)
        for (family in BundledFontFamily.all) {
            val resolved = Fonts.resolve(ThemeFontFace(listOf(family.name), family.design))
            assertFalse("${family.name} falls back to the system", resolved in system)
        }
    }
}
