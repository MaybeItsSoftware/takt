package uk.co.maybeitsadam.takt.core.theme

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Case for case with corelogic-tests/ThemeFileTests.swift, then the per-platform layer. */
class ThemeFileTest {
    private val android = ThemePlatform.ANDROID
    private val chalk get() = BuiltInThemeSpecifications.chalk(android)
    private val priority get() = BuiltInThemeSpecifications.priority(android)
    private val light = ThemeAppearance.LIGHT
    private val dark = ThemeAppearance.DARK

    private fun source(name: String, json: String) = ThemeFileSource(name, json)

    private fun load(vararg sources: ThemeFileSource, platform: ThemePlatform = android) =
        ThemeFileLoader.load(sources.toList(), platform)

    private fun messages(outcome: ThemeFileOutcome?): List<String> = outcome?.issues?.map { it.message } ?: emptyList()

    // Round trip

    @Test
    fun exportedBuiltInsLoadBackAsTheSameTheme() {
        for (platform in ThemePlatform.entries) {
            for (builtIn in BuiltInThemeSpecifications.all(platform)) {
                val file = ThemeFile.of(builtIn).copy(identifier = "user.copy")
                val text = file.encoded()

                val (decoded, decodeIssues) = ThemeFileLoader.decode(text, "copy.json")
                assertEquals(emptyList<ThemeFileIssue>(), decodeIssues)
                assertEquals(file, decoded)

                val library = ThemeFileLoader.load(listOf(ThemeFileSource("copy.json", text)), platform)
                val loaded = library.themes.first()
                assertEquals(builtIn.name, loaded.name)
                assertEquals(builtIn.lockedAppearance, loaded.lockedAppearance)
                assertEquals(builtIn.structure, loaded.structure)
                for (appearance in ThemeAppearance.entries) {
                    for (role in ThemeColorRole.entries) {
                        assertEquals(
                            "${builtIn.name} ${appearance.raw}.${role.raw}",
                            builtIn.color(role, appearance).hexString,
                            loaded.color(role, appearance).hexString,
                        )
                    }
                }
                assertFalse("${library.issues}", library.issues.any { it.severity == ThemeIssueSeverity.ERROR })
            }
        }
    }

    @Test
    fun theTriStateFieldsSurviveEncoding() {
        val file = ThemeFile(identifier = "x", lockedAppearance = ThemeFile.Lock.Unlocked, extends = ThemeFile.Base.Nothing)
        val json = file.encoded()
        assertTrue(json, json.contains("\"extends\" : null"))
        assertTrue(json, json.contains("\"lockedAppearance\" : null"))
        assertEquals(file, ThemeFileLoader.decode(json, "x.json").first)

        val bare = ThemeFileLoader.decode("{}", "bare.json").first
        assertEquals(ThemeFile.Base.DefaultTheme, bare?.extends)
        assertEquals(ThemeFile.Lock.Inherit, bare?.lockedAppearance)
    }

    // extends and merging

    @Test
    fun aFewOverridesInheritEverythingElseFromTheDefault() {
        val library = load(
            source(
                "dusk.json",
                """
                {
                  "name": "Dusk",
                  "palette": { "light": { "primary": "#7a4de8" }, "dark": { "paper": "#101014" } },
                  "structure": { "radius": { "panel": 10 }, "typography": { "bodySize": 15 } }
                }
                """,
            ),
        )
        val dusk = library.themes.first()

        assertEquals("no identifier: named after the file", "user.dusk", dusk.identifier)
        assertEquals("Dusk", dusk.name)
        assertEquals("#7A4DE8", dusk.color(ThemeColorRole.PRIMARY, light).hexString)
        assertEquals(priority.color(ThemeColorRole.PRIMARY, dark), dusk.color(ThemeColorRole.PRIMARY, dark))
        assertEquals("#101014", dusk.color(ThemeColorRole.PAPER, dark).hexString)
        assertEquals(priority.color(ThemeColorRole.INK, light), dusk.color(ThemeColorRole.INK, light))
        assertEquals(10.0, dusk.structure.radius.panel, 0.0)
        assertEquals(priority.structure.radius.control, dusk.structure.radius.control, 0.0)
        assertEquals(priority.structure.border, dusk.structure.border)
        assertEquals(15.0, dusk.structure.typography.bodySize, 0.0)
        assertEquals(
            "the body size Android already has keeps Android's scale",
            priority.structure.typography.scale, dusk.structure.typography.scale,
        )
    }

    @Test
    fun aThemeCanRestoreTheSlabFaceAndTheTrackedCapitals() {
        val library = load(
            source(
                "slab.json",
                """
                {
                  "structure": { "typography": {
                    "body": { "families": ["Arvo", "Rockwell"], "design": "serif" },
                    "microLabel": { "size": 10, "weight": "bold", "tracking": 0.15, "uppercase": true }
                  } }
                }
                """,
            ),
        )
        val type = library.themes.first().structure.typography
        assertEquals(ThemeFontFace(listOf("Arvo", "Rockwell"), ThemeFontDesign.SERIF), type.body)
        assertEquals(priority.structure.typography.mono, type.mono)
        assertEquals(
            ThemeMicroLabel(10.0, ThemeFontWeight.BOLD, 0.15, true, ThemeColorRole.MUTED_TEXT),
            type.microLabel,
        )
    }

    @Test
    fun extendingChalkDarkInheritsItsLockUnlessClearedWithNull() {
        val library = load(
            source("a.json", """{ "extends": "native.theme.chalk.dark" }"""),
            source("b.json", """{ "extends": "native.theme.chalk.dark", "lockedAppearance": null }"""),
            source("c.json", """{ "lockedAppearance": "light" }"""),
        )
        assertEquals(listOf(dark, null, light), library.themes.map { it.lockedAppearance })
    }

    @Test
    fun aThemeCanExtendAnotherUserThemeWhateverTheFileOrder() {
        val library = load(
            source("a-child.json", """{ "extends": "mine.base", "palette": { "light": { "ink": "#111111" } } }"""),
            source("z-base.json", """{ "identifier": "mine.base", "palette": { "light": { "paper": "#fefefe" } } }"""),
        )
        val child = library.themes.first { it.identifier == "user.a-child" }
        assertEquals("#FEFEFE", child.color(ThemeColorRole.PAPER, light).hexString)
        assertEquals("#111111", child.color(ThemeColorRole.INK, light).hexString)
    }

    @Test
    fun cyclesAndUnknownParentsAreSkippedWithAReason() {
        val library = load(
            source("a.json", """{ "identifier": "a", "extends": "b" }"""),
            source("b.json", """{ "identifier": "b", "extends": "a" }"""),
            source("c.json", """{ "extends": "nope" }"""),
        )
        assertEquals(emptyList<ThemeSpecification>(), library.themes)
        assertEquals(3, library.outcomes.size)
        assertTrue(library.outcomes.all { it.skippedReason != null })
        assertEquals("it extends \"nope\", which is not a theme", library.outcomes[2].skippedReason)
    }

    @Test
    fun identifiersCannotShadowABuiltInOrEachOther() {
        val library = load(
            source("a.json", """{ "identifier": "native.theme.chalk" }"""),
            source("b.json", """{ "identifier": "mine" }"""),
            source("c.json", """{ "identifier": "mine" }"""),
        )
        assertEquals(listOf("mine"), library.themes.map { it.identifier })
        assertNotNull(library.outcomes[0].skippedReason)
        assertEquals("identifier \"mine\" is already used by b.json", library.outcomes[2].skippedReason)
    }

    // Bad values are reported, not fatal

    @Test
    fun aBadHexIsAnErrorButTheThemeStillLoadsWithTheInheritedValue() {
        val library = load(source("bad.json", """{ "palette": { "light": { "paper": "#nothex", "ink": "#222" } } }"""))
        val outcome = library.outcomes.first()
        val theme = outcome.specification!!
        assertEquals(priority.color(ThemeColorRole.PAPER, light), theme.color(ThemeColorRole.PAPER, light))
        assertEquals("#222222", theme.color(ThemeColorRole.INK, light).hexString)
        assertEquals(ThemeIssueSeverity.ERROR, outcome.issues.first().severity)
        assertTrue(messages(outcome).any { it.contains("palette.light.paper \"#nothex\"") })
    }

    @Test
    fun anUnknownRoleIsAWarningAndIgnored() {
        val library = load(source("x.json", """{ "palette": { "dark": { "backgroundd": "#000000" } } }"""))
        val outcome = library.outcomes.first()
        assertNotNull(outcome.specification)
        val issue = outcome.issues.first { it.message.contains("backgroundd") }
        assertEquals(ThemeIssueSeverity.WARNING, issue.severity)
        assertEquals("x.json", issue.source)
    }

    @Test
    fun unknownKeysAndBadEnumeratedValuesAreReported() {
        val library = load(
            source(
                "x.json",
                """
                {
                  "pallete": {},
                  "lockedAppearance": "dusk",
                  "structure": {
                    "radius": { "pannel": 3 },
                    "typography": { "body": { "design": "comic" }, "microLabel": { "weight": "heavy", "role": "nope" } }
                  }
                }
                """,
            ),
        )
        val outcome = library.outcomes.first()
        assertNotNull("every one of those is recoverable", outcome.specification)
        val text = messages(outcome).joinToString("\n")
        for (fragment in listOf("pallete", "structure.radius.pannel", "\"dusk\"", "\"comic\"", "\"heavy\"", "\"nope\"")) {
            assertTrue("missing $fragment in:\n$text", text.contains(fragment))
        }
    }

    @Test
    fun aFileThatIsNotJSONIsSkippedNotThrown() {
        val library = load(source("broken.json", "{ \"name\": "), source("ok.json", "{}"))
        assertEquals(listOf("user.ok"), library.themes.map { it.identifier })
        val broken = library.outcomes.first()
        assertEquals("could not be read", broken.skippedReason)
        assertTrue(messages(broken).contains("not valid JSON"))
    }

    @Test
    fun aWrongTypeNamesThePath() {
        val library = load(source("x.json", """{ "structure": { "radius": { "panel": "big" } } }"""))
        assertTrue(
            "${messages(library.outcomes.first())}",
            messages(library.outcomes.first()).contains("structure.radius.panel should be a number"),
        )
    }

    @Test
    fun aNegativeSizeKeepsTheInheritedValue() {
        val library = load(source("x.json", """{ "structure": { "border": { "hairline": -1 } } }"""))
        val theme = library.themes.first()
        assertEquals(1.0, theme.structure.border.hairline, 0.0)
        assertEquals(ThemeIssueSeverity.ERROR, library.issues.first().severity)
        assertEquals("structure.border.hairline -1 should be zero or more", library.issues.first().message)
    }

    // Missing roles

    @Test
    fun aStandaloneThemeMissingARoleEverywhereIsSkipped() {
        val lightTable = ThemeColorRole.entries.filter { it != ThemeColorRole.CATEGORICAL_PINK }.associate { it.raw to "#808080" }
        val file = ThemeFile(extends = ThemeFile.Base.Nothing, palette = ThemeFile.Palette(light = lightTable))
        val library = ThemeFileLoader.load(listOf(ThemeFileSource("partial.json", file.encoded())), android)
        val outcome = library.outcomes.first()
        assertNull(outcome.specification)
        assertEquals("no colour for categoricalPink", outcome.skippedReason)
    }

    @Test
    fun aStandaloneThemeWithOneTableLoadsWithMissingRoleErrors() {
        val lightTable = ThemeColorRole.entries.associate { it.raw to "#808080" }.toMutableMap()
        lightTable[ThemeColorRole.INK.raw] = "#000000"
        lightTable[ThemeColorRole.PAPER.raw] = "#ffffff"
        val file = ThemeFile(extends = ThemeFile.Base.Nothing, palette = ThemeFile.Palette(light = lightTable))
        val outcome = ThemeFileLoader.resolve(file, "light-only.json", null, android)
        val theme = outcome.specification
        assertNotNull("every role resolves, via the light table", theme)
        assertEquals("#000000", theme!!.color(ThemeColorRole.INK, dark).hexString)
        assertTrue(outcome.issues.any { it.severity == ThemeIssueSeverity.ERROR && it.message.contains("no dark value") })
    }

    // Platforms (docs/themes.md, "Across the Mac, iPhone and Android")

    @Test
    fun prioritysPerPlatformStructureIsTheDocumentedTable() {
        val table = mapOf(
            ThemePlatform.MACOS to Triple(13.0, listOf(0.0, 0.0, 4.0), 0.0),
            ThemePlatform.IOS to Triple(15.0, listOf(0.0, 0.0, 4.0), 44.0),
            ThemePlatform.ANDROID to Triple(15.0, listOf(0.0, 0.0, 4.0), 48.0),
        )
        for ((platform, row) in table) {
            val structure = BuiltInThemeSpecifications.priority(platform).structure
            assertEquals(platform.raw, row.first, structure.typography.bodySize, 0.0)
            assertEquals(platform.raw, row.second, structure.radius.let { listOf(it.panel, it.row, it.control) })
            assertEquals(platform.raw, row.third, structure.touchTarget, 0.0)
            assertEquals(
                BuiltInThemeSpecifications.priority(ThemePlatform.MACOS).palette,
                BuiltInThemeSpecifications.priority(platform).palette,
            )
            val issues = BuiltInThemeSpecifications.priority(platform).validate()
            assertFalse("$platform: $issues", issues.any { it.severity == ThemeIssueSeverity.ERROR })
        }
        val droid = priority.structure.typography
        assertEquals(ThemeTypeScale(13.0, 15.0, 17.0, 32.0, 72.0), droid.scale)
        assertEquals(13.0, droid.microLabel.size, 0.0)
    }

    @Test
    fun theBuiltInsComeInOrderAndTheDefaultIsZedOne() {
        for (platform in ThemePlatform.entries) {
            val all = BuiltInThemeSpecifications.all(platform)
            assertEquals(
                listOf("native.theme.priority", "native.theme.chalk", "native.theme.chalk.dark", "native.theme.grape", "native.theme.one"),
                all.map { it.identifier },
            )
            assertEquals(listOf("Takt", "Zed", "Zed Dark", "Grape", "Zed One"), all.map { it.name })
            assertEquals("native.theme.one", BuiltInThemeSpecifications.defaultTheme(platform).identifier)
        }
        val type = priority.structure.typography
        assertEquals(ThemeFontFace(listOf("IBM Plex Sans"), ThemeFontDesign.SANS), type.body)
        assertEquals(ThemeFontFace(listOf("IBM Plex Sans"), ThemeFontDesign.SANS), type.display)
        assertEquals(ThemeFontFace(listOf("Lilex"), ThemeFontDesign.MONOSPACED), type.mono)
        // A file that names no base extends Priority.
        val bare = load(source("bare.json", "{}")).themes.first()
        assertEquals(priority.palette, bare.palette)
        assertEquals(priority.structure, bare.structure)
    }

    @Test
    fun prioritysPaletteIsTheHouseColoursExactly() {
        // The default is Zed's colours on its own roomier structure: every
        // role Zed names, with the same hex, in both appearances.
        for (appearance in ThemeAppearance.entries) {
            for ((role, value) in chalk.palette.table(appearance)) {
                assertEquals("${appearance.raw} ${role.raw}", value, priority.color(role, appearance))
            }
        }
    }

    // Seeds

    @Test
    fun mixRoundsOnWholeChannelStepsHalfAwayFromZero() {
        val black = ThemeColorValue.hex("#000000")!!
        val white = ThemeColorValue.hex("#ffffff")!!
        assertEquals("#808080", ThemeSeeds.mix(black, white, 0.5).hexString) // 127.5 rounds up
        assertEquals("#000000", ThemeSeeds.mix(black, white, 0.0).hexString)
        assertEquals("#FFFFFF", ThemeSeeds.mix(black, white, 1.0).hexString)
        assertEquals("255 - 127.5 rounds up too", "#808080", ThemeSeeds.mix(white, black, 0.5).hexString)
    }

    @Test
    fun seedsGrowEveryNeutralAndTheirStatedRoles() {
        val seeds = ThemeSeeds(
            background = ThemeColorValue.hex("#f7f7f8"),
            foreground = ThemeColorValue.hex("#1f2026"),
            accent = ThemeColorValue.hex("#3d63dd"),
        )
        val light = seeds.roles(ThemeAppearance.LIGHT)!!
        assertEquals("#F7F7F8", light[ThemeColorRole.PAPER]!!.hexString)
        assertEquals("#1F2026", light[ThemeColorRole.INK]!!.hexString)
        assertEquals("#3D63DD", light[ThemeColorRole.PRIMARY]!!.hexString)
        assertEquals("#DDDDDF", light[ThemeColorRole.BORDER]!!.hexString)
        assertEquals("#FCFCFC", light[ThemeColorRole.RAISED]!!.hexString)
        assertNull("no success seed, no success role", light[ThemeColorRole.SUCCESS])
        assertNull("nothing to mix between", ThemeSeeds(background = seeds.background).roles(ThemeAppearance.LIGHT))
        val over = seeds.overlaid(ThemeSeeds(accent = ThemeColorValue.hex("#ff0000")))
        assertEquals(seeds.background, over.background)
        assertEquals("#FF0000", over.accent!!.hexString)
    }

    @Test
    fun aFileCanGiveSeedsAndPaletteStillWins() {
        val library = load(
            source(
                "sea.json",
                """
                { "seeds": { "light": { "background": "#f4f8f9", "foreground": "#14303a", "accent": "#0b7a8c" } },
                  "palette": { "light": { "border": "#c9d9dd" } } }
                """,
            ),
        )
        val sea = library.themes.first()
        assertEquals("#F4F8F9", sea.color(ThemeColorRole.PAPER, light).hexString)
        assertEquals("#14303A", sea.color(ThemeColorRole.INK, light).hexString)
        assertEquals("#0B7A8C", sea.color(ThemeColorRole.PRIMARY, light).hexString)
        assertEquals("palette beats seeds", "#C9D9DD", sea.color(ThemeColorRole.BORDER, light).hexString)
        assertEquals(
            ThemeSeeds.mix(ThemeColorValue.hex("#f4f8f9")!!, ThemeColorValue.hex("#14303a")!!, 0.07),
            sea.color(ThemeColorRole.WELL, light),
        )
        assertEquals("status seeds come from the base", priority.color(ThemeColorRole.DANGER, light), sea.color(ThemeColorRole.DANGER, light))
        assertEquals("no dark seeds: the base's dark table", priority.palette.dark, sea.palette.dark)
        assertTrue(library.issues.none { it.message.startsWith("seeds") })
    }

    @Test
    fun aBadSeedIsReportedAndAMissingOneComesFromTheBase() {
        val library = load(
            source("half.json", """{ "seeds": { "light": { "background": "#ffffff", "glow": "#00ff00", "accent": "#zz" } } }"""),
        )
        val outcome = library.outcomes.first()
        val half = outcome.specification!!
        assertEquals(priority.color(ThemeColorRole.INK, light), half.color(ThemeColorRole.INK, light))
        assertEquals(priority.color(ThemeColorRole.PRIMARY, light), half.color(ThemeColorRole.PRIMARY, light))
        assertEquals("#FFFFFF", half.color(ThemeColorRole.PAPER, light).hexString)
        assertTrue(
            messages(outcome).contains(
                "seeds.light.glow is not a seed (background, foreground, accent, success, danger or warning); ignored",
            ),
        )
        assertTrue(messages(outcome).contains("seeds.light.accent \"#zz\" is not a hex colour (#rgb, #rrggbb or #rrggbbaa)"))
        assertEquals(ThemeIssueSeverity.ERROR, outcome.issues.first { it.message.startsWith("seeds.light.accent") }.severity)
        assertEquals(ThemeIssueSeverity.WARNING, outcome.issues.first { it.message.startsWith("seeds.light.glow") }.severity)
    }

    @Test
    fun seedsWithNothingToMixBetweenAreAnError() {
        val library = ThemeFileLoader.load(
            listOf(source("x.json", """{ "extends": null, "seeds": { "light": { "accent": "#ff0000" } } }""")),
            android,
        )
        assertTrue(library.issues.any { it.message == "seeds.light needs a background and a foreground; ignored" })
    }

    @Test
    fun seedsSurviveEncoding() {
        val text = """{ "seeds": { "dark": { "background": "#000000", "foreground": "#ffffff" } } }"""
        val (file, issues) = ThemeFileLoader.decode(text, "x.json")
        assertEquals(emptyList<ThemeFileIssue>(), issues)
        assertEquals(mapOf("background" to "#000000", "foreground" to "#ffffff"), file!!.seeds?.dark)
        assertEquals(file, ThemeFileLoader.decode(file.encoded(), "x.json").first)
    }

    @Test
    fun zedCarriesEachPlatformsDefaults() {
        val mac = BuiltInThemeSpecifications.chalk(ThemePlatform.MACOS).structure
        val ios = BuiltInThemeSpecifications.chalk(ThemePlatform.IOS).structure
        val droid = chalk.structure
        assertEquals(listOf(13.0, 15.0, 15.0), listOf(mac, ios, droid).map { it.typography.bodySize })
        assertEquals(ThemeTypeScale(13.0, 15.0, 17.0, 32.0, 72.0), droid.typography.scale)
        assertEquals(listOf(0.0, 44.0, 48.0), listOf(mac, ios, droid).map { it.touchTarget })
        assertEquals(listOf(0.0, 0.0, 4.0), droid.radius.let { listOf(it.panel, it.row, it.control) })
        assertEquals(13.0, ios.typography.microLabel.size, 0.0)
        assertEquals(
            "Chalk Dark extends Chalk and takes the same per-platform structure",
            droid, BuiltInThemeSpecifications.chalkDark(android).structure,
        )
        for (platform in ThemePlatform.entries) {
            val issues = BuiltInThemeSpecifications.chalk(platform).validate()
            assertFalse("$platform: $issues", issues.any { it.severity == ThemeIssueSeverity.ERROR })
        }
    }

    @Test
    fun thePlatformBlockIsLaidOverTheSharedStructureOnItsPlatformOnly() {
        val dusk = """
            {
              "name": "Dusk",
              "palette": { "dark": { "paper": "#15131c" } },
              "structure": { "radius": { "control": 2 } },
              "platforms": {
                "ios": { "structure": { "typography": { "bodySize": 18 } } },
                "android": { "structure": { "spacing": { "md": 18 } } }
              }
            }
        """
        val onAndroid = load(source("dusk.json", dusk)).themes.first()
        val onIOS = load(source("dusk.json", dusk), platform = ThemePlatform.IOS).themes.first()
        val onMac = load(source("dusk.json", dusk), platform = ThemePlatform.MACOS).themes.first()

        assertEquals(18.0, onAndroid.structure.spacing.md, 0.0)
        assertEquals(12.0, onIOS.structure.spacing.md, 0.0)
        assertEquals(2.0, onAndroid.structure.radius.control, 0.0)
        assertEquals(2.0, onMac.structure.radius.control, 0.0)
        assertEquals("Android keeps Chalk's Android body size", 15.0, onAndroid.structure.typography.bodySize, 0.0)
        assertEquals(18.0, onIOS.structure.typography.bodySize, 0.0)
        assertEquals(ThemeTypeScale.proportioned(18.0), onIOS.structure.typography.scale)
        assertEquals(13.0, onMac.structure.typography.bodySize, 0.0)
        assertEquals(48.0, onAndroid.structure.touchTarget, 0.0)
        assertEquals("the palette is the same everywhere", onMac.palette, onAndroid.palette)
    }

    @Test
    fun aSharedBodySizeAppliesEverywhereUnlessAPlatformSaysOtherwise() {
        val file = """
            { "structure": { "typography": { "bodySize": 15 } },
              "platforms": { "android": { "structure": { "typography": { "bodySize": 17 } } } } }
        """
        assertEquals(15.0, load(source("x.json", file), platform = ThemePlatform.IOS).themes.first().structure.typography.bodySize, 0.0)
        val droid = load(source("x.json", file)).themes.first().structure.typography
        assertEquals(17.0, droid.bodySize, 0.0)
        assertEquals(ThemeTypeScale.proportioned(17.0), droid.scale)
    }

    @Test
    fun anExtendedThemeIsResolvedForThisPlatformIncludingItsOwnPlatformsBlock() {
        val library = load(
            source("base.json", """{ "identifier": "b", "platforms": { "android": { "structure": { "touchTarget": 56 } } } }"""),
            source("child.json", """{ "extends": "b", "structure": { "radius": { "panel": 12 } } }"""),
        )
        val child = library.themes.first { it.identifier == "user.child" }
        assertEquals(56.0, child.structure.touchTarget, 0.0)
        assertEquals(12.0, child.structure.radius.panel, 0.0)
        assertEquals(priority.structure.radius.control, child.structure.radius.control, 0.0)
    }

    @Test
    fun aPaletteUnderPlatformsIsAWarningAndIgnored() {
        val library = load(
            source("x.json", """{ "platforms": { "android": { "palette": { "light": { "paper": "#000000" } } } } }"""),
        )
        val theme = library.themes.first()
        assertEquals(priority.color(ThemeColorRole.PAPER, light), theme.color(ThemeColorRole.PAPER, light))
        val issue = library.issues.first { it.message.contains("platforms.android.palette") }
        assertEquals(ThemeIssueSeverity.WARNING, issue.severity)
        assertEquals("platforms.android.palette is not allowed: colour is the same on every platform; ignored", issue.message)
    }

    @Test
    fun aBadValueForAnotherPlatformIsReportedHereTooButNotUsed() {
        val text = """{ "platforms": { "ios": { "structure": { "spacing": { "md": -2 }, "radius": { "row": 3 } } } } }"""
        for (platform in ThemePlatform.entries) {
            val library = ThemeFileLoader.load(listOf(source("x.json", text)), platform)
            assertTrue(platform.raw, library.issues.any { it.message == "platforms.ios.structure.spacing.md -2 should be zero or more" })
            val expectedRow = if (platform == ThemePlatform.IOS) 3.0 else BuiltInThemeSpecifications.priority(platform).structure.radius.row
            assertEquals(platform.raw, expectedRow, library.themes.first().structure.radius.row, 0.0)
        }
    }

    @Test
    fun auditFindingsAreMarkedAndMissingRolesAreNot() {
        val library = load(source("x.json", """{ "extends": "native.theme.chalk", "structure": { "touchTarget": 30, "border": { "hairline": -1 } } }"""))
        val issues = library.issues
        assertTrue(issues.first { it.message == "a 30.00pt touch target is under the 44.00pt a finger needs" }.isAudit)
        assertFalse(issues.first { it.message.startsWith("structure.border.hairline") }.isAudit)
        assertTrue(issues.first { it.message.startsWith("primary is 3.61:1") }.isAudit)
    }

    @Test
    fun theSharedBuiltInsStateEveryValueAndLoadCleanly() {
        val folder = java.io.File(System.getProperty("priority.sharedThemesDir") ?: "../../shared/themes")
        val sources = listOf("priority.json", "chalk.json", "chalk-dark.json", "grape.json", "one.json").map {
            ThemeFileSource(it, folder.resolve(it).readText())
        }
        for (platform in ThemePlatform.entries) {
            // Laid over a base nothing like any of them, so every value has to come from the file.
            val grape = BuiltInThemeSpecifications.grape(platform)
            val other = grape.copy(
                structure = grape.structure.copy(
                    radius = ThemeRadiusScale(7.0, 7.0, 7.0, 7.0, 7.0),
                    border = ThemeBorderScale(7.0, 7.0, 7.0),
                    spacing = ThemeSpacingScale(7.0, 7.0, 7.0, 7.0, 7.0, 7.0),
                    touchTarget = 7.0,
                    usesShadows = true,
                    usesGradientsOnChrome = true,
                ),
            )
            val library = ThemeFileLoader.load(sources, platform, builtIns = emptyList(), defaultBase = other)
            assertTrue(library.issues.filterNot { it.isAudit }.toString(), library.issues.none { !it.isAudit })
            // A file states colours in whole 255ths; the built-in scrim is 0.7 exactly.
            fun ThemeSpecification.inHex() = copy(
                palette = ThemePalette(
                    palette.light.mapValues { ThemeColorValue.hex(it.value.hexString)!! },
                    palette.dark.mapValues { ThemeColorValue.hex(it.value.hexString)!! },
                ),
            )
            // The folder loads in file-name order; the built-ins come default first.
            assertEquals(
                BuiltInThemeSpecifications.all(platform).map { it.inHex() }.sortedBy { it.identifier },
                library.themes.map { it.inHex() }.sortedBy { it.identifier },
            )
        }
    }

    @Test
    fun platformKeysAreCheckedAndPlatformErrorsNameTheirPath() {
        val library = load(
            source(
                "x.json",
                """{ "platforms": { "windows": {}, "android": { "structure": { "touchTarget": -4, "radius": { "pannel": 1 } } } } }""",
            ),
        )
        val text = messages(library.outcomes.first()).joinToString("\n")
        for (fragment in listOf("platforms.windows", "platforms.android.structure.radius.pannel",
            "platforms.android.structure.touchTarget -4 should be zero or more")) {
            assertTrue("missing $fragment in:\n$text", text.contains(fragment))
        }
        assertEquals(48.0, library.themes.first().structure.touchTarget, 0.0)
    }

    @Test
    fun typeErrorsReadLikeSwiftsDecoder() {
        fun reason(json: String) = messages(load(source("x.json", json)).outcomes.first())
        assertTrue(reason("[]").contains("the file should be an object"))
        assertTrue(reason("""{ "name": 3 }""").contains("name should be a string"))
        assertTrue(reason("""{ "palette": { "light": { "paper": null } } }""").contains("palette.light.paper should be a string, not null"))
        assertTrue(reason("""{ "structure": { "usesShadows": "yes" } }""").contains("structure.usesShadows should be true or false"))
        assertTrue(
            reason("""{ "structure": { "typography": { "body": { "families": ["Arvo", 3] } } } }""")
                .contains("structure.typography.body.families.1 should be a string"),
        )
        assertTrue("null is absent", load(source("x.json", """{ "structure": { "radius": { "panel": null } } }""")).themes.size == 1)
    }

    @Test
    fun hexParsingMatchesSwift() {
        assertEquals("#AABBCC", ThemeColorValue.hex("abc")!!.hexString)
        assertEquals("#000000B3", ThemeColorValue.hex("#000000b3")!!.hexString)
        assertNull(ThemeColorValue.hex("#12345"))
        assertNull(ThemeColorValue.hex(""))
        assertEquals(0xFF007FFF.toInt(), ThemeColorValue.hex("#007fff")!!.argb)
    }

    @Test
    fun zedsAuditHasTheAzureNoteAndNothingWorse() {
        val issues = chalk.validate()
        assertTrue(issues.any { it is ThemeIssue.LargeTextOnly && it.role == ThemeColorRole.PRIMARY })
        assertTrue(issues.none { it.severity == ThemeIssueSeverity.ERROR })
        assertEquals(
            "primary is 3.61:1 on light paper — not for body copy",
            issues.first { it is ThemeIssue.LargeTextOnly && it.role == ThemeColorRole.PRIMARY && it.appearance == light }.message,
        )
    }
}
