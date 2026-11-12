package uk.co.maybeitsadam.takt.core.theme

import uniffi.takt_core.CoreThemeAppearance
import uniffi.takt_core.CoreThemeBorder
import uniffi.takt_core.CoreThemeColor
import uniffi.takt_core.CoreThemeFile
import uniffi.takt_core.CoreThemeFileBase
import uniffi.takt_core.CoreThemeFileBorder
import uniffi.takt_core.CoreThemeFileFace
import uniffi.takt_core.CoreThemeFileIssue
import uniffi.takt_core.CoreThemeFileLock
import uniffi.takt_core.CoreThemeFileMicroLabel
import uniffi.takt_core.CoreThemeFileOutcome
import uniffi.takt_core.CoreThemeFilePalette
import uniffi.takt_core.CoreThemeFilePlatformOverride
import uniffi.takt_core.CoreThemeFilePlatforms
import uniffi.takt_core.CoreThemeFileRadius
import uniffi.takt_core.CoreThemeFileSpacing
import uniffi.takt_core.CoreThemeFileStructure
import uniffi.takt_core.CoreThemeFileTypeScale
import uniffi.takt_core.CoreThemeFileTypography
import uniffi.takt_core.CoreThemeFontFace
import uniffi.takt_core.CoreThemeIssue
import uniffi.takt_core.CoreThemeIssueSeverity
import uniffi.takt_core.CoreThemeMicroLabel
import uniffi.takt_core.CoreThemePalette
import uniffi.takt_core.CoreThemePlatform
import uniffi.takt_core.CoreThemeRadius
import uniffi.takt_core.CoreThemeSpacing
import uniffi.takt_core.CoreThemeSpecification
import uniffi.takt_core.CoreThemeStructure
import uniffi.takt_core.CoreThemeTypeScale
import uniffi.takt_core.CoreThemeTypography

// The theme format is the Rust core's (core/src/theme), which the Mac and the
// iPhone call too. These are the conversions between this module's theme
// types, which Compose renders through, and the core's records. They run once
// per theme load, never per render.

internal val ThemeColorValue.core: CoreThemeColor get() = CoreThemeColor(red, green, blue, alpha)

internal fun ThemeColorValue.Companion.of(core: CoreThemeColor) = ThemeColorValue(core.red, core.green, core.blue, core.alpha)

internal val ThemeAppearance.core: CoreThemeAppearance
    get() = if (this == ThemeAppearance.LIGHT) CoreThemeAppearance.LIGHT else CoreThemeAppearance.DARK

internal fun CoreThemeAppearance.local(): ThemeAppearance =
    if (this == CoreThemeAppearance.LIGHT) ThemeAppearance.LIGHT else ThemeAppearance.DARK

internal val ThemePlatform.core: CoreThemePlatform
    get() = when (this) {
        ThemePlatform.MACOS -> CoreThemePlatform.MACOS
        ThemePlatform.IOS -> CoreThemePlatform.IOS
        ThemePlatform.ANDROID -> CoreThemePlatform.ANDROID
    }

internal fun CoreThemePlatform.local(): ThemePlatform = when (this) {
    CoreThemePlatform.MACOS -> ThemePlatform.MACOS
    CoreThemePlatform.IOS -> ThemePlatform.IOS
    CoreThemePlatform.ANDROID -> ThemePlatform.ANDROID
}

internal fun CoreThemeIssueSeverity.local(): ThemeIssueSeverity = when (this) {
    CoreThemeIssueSeverity.ERROR -> ThemeIssueSeverity.ERROR
    CoreThemeIssueSeverity.WARNING -> ThemeIssueSeverity.WARNING
    CoreThemeIssueSeverity.NOTE -> ThemeIssueSeverity.NOTE
}

// MARK: - Resolved

private fun coreTable(values: Map<ThemeColorRole, ThemeColorValue>): Map<String, CoreThemeColor> =
    values.entries.associate { it.key.raw to it.value.core }

private fun localTable(values: Map<String, CoreThemeColor>): Map<ThemeColorRole, ThemeColorValue> =
    values.entries.mapNotNull { (key, value) -> ThemeColorRole.of(key)?.let { it to ThemeColorValue.of(value) } }
        // Role order, so a table reads the same however the map came across.
        .sortedBy { it.first.ordinal }
        .toMap(LinkedHashMap())

private fun coreChrome(values: Map<ThemeChromeRole, ThemeColorValue>): Map<String, CoreThemeColor> =
    values.entries.associate { it.key.raw to it.value.core }

private fun localChrome(values: Map<String, CoreThemeColor>): Map<ThemeChromeRole, ThemeColorValue> =
    values.entries.mapNotNull { (key, value) -> ThemeChromeRole.of(key)?.let { it to ThemeColorValue.of(value) } }
        .sortedBy { it.first.ordinal }
        .toMap(LinkedHashMap())

internal val ThemePalette.core: CoreThemePalette
    get() = CoreThemePalette(coreTable(light) + coreChrome(chromeLight), coreTable(dark) + coreChrome(chromeDark))

internal fun CoreThemePalette.local() =
    ThemePalette(localTable(light), localTable(dark), localChrome(light), localChrome(dark))

internal val ThemeFontFace.core: CoreThemeFontFace get() = CoreThemeFontFace(families, design.raw)

internal fun CoreThemeFontFace.local() = ThemeFontFace(families, ThemeFontDesign.of(design) ?: ThemeFontDesign.SANS)

internal val ThemeTypeScale.core: CoreThemeTypeScale get() = CoreThemeTypeScale(caption, body, title, display, hero)

internal fun CoreThemeTypeScale.local() = ThemeTypeScale(caption, body, title, display, hero)

internal val ThemeStructure.core: CoreThemeStructure
    get() = CoreThemeStructure(
        radius = radius.let { CoreThemeRadius(it.panel, it.row, it.control, it.pill, it.shell) },
        border = border.let { CoreThemeBorder(it.hairline, it.emphasis, it.focusRing) },
        spacing = spacing.let { CoreThemeSpacing(it.xxs, it.xs, it.sm, it.md, it.lg, it.xl) },
        typography = typography.let { type ->
            CoreThemeTypography(
                display = type.display.core,
                body = type.body.core,
                mono = type.mono.core,
                bodySize = type.bodySize,
                scale = type.scale.core,
                microLabel = type.microLabel.let {
                    CoreThemeMicroLabel(it.size, it.weight.raw, it.tracking, it.isUppercased, it.role.raw)
                },
            )
        },
        touchTarget = touchTarget,
        usesShadows = usesShadows,
        usesGradientsOnChrome = usesGradientsOnChrome,
    )

internal fun CoreThemeStructure.local() = ThemeStructure(
    radius = radius.let { ThemeRadiusScale(it.panel, it.row, it.control, it.pill, it.shell) },
    border = border.let { ThemeBorderScale(it.hairline, it.emphasis, it.focusRing) },
    spacing = spacing.let { ThemeSpacingScale(it.xxs, it.xs, it.sm, it.md, it.lg, it.xl) },
    typography = typography.let { type ->
        ThemeTypography(
            display = type.display.local(),
            body = type.body.local(),
            mono = type.mono.local(),
            bodySize = type.bodySize,
            scale = type.scale.local(),
            microLabel = type.microLabel.let {
                ThemeMicroLabel(
                    size = it.size,
                    weight = ThemeFontWeight.of(it.weight) ?: ThemeFontWeight.REGULAR,
                    tracking = it.tracking,
                    isUppercased = it.isUppercased,
                    role = ThemeColorRole.of(it.role) ?: ThemeColorRole.MUTED_TEXT,
                )
            },
        )
    },
    touchTarget = touchTarget,
    usesShadows = usesShadows,
    usesGradientsOnChrome = usesGradientsOnChrome,
)

internal val ThemeSpecification.core: CoreThemeSpecification
    get() = CoreThemeSpecification(identifier, name, summary, lockedAppearance?.core, palette.core, structure.core)

internal fun CoreThemeSpecification.local() = ThemeSpecification(
    identifier = identifier,
    name = name,
    summary = summary,
    lockedAppearance = lockedAppearance?.local(),
    palette = palette.local(),
    structure = structure.local(),
)

// MARK: - Issues

internal val ThemeIssue.core: CoreThemeIssue
    get() = when (this) {
        is ThemeIssue.MissingRole -> CoreThemeIssue.MissingRole(role.raw, appearance.core)
        is ThemeIssue.BodyTextBelowAA -> CoreThemeIssue.BodyTextBelowAa(role.raw, appearance.core, ratio)
        is ThemeIssue.LargeTextOnly -> CoreThemeIssue.LargeTextOnly(role.raw, appearance.core, ratio)
        is ThemeIssue.AccentBelowUIMinimum -> CoreThemeIssue.AccentBelowUiMinimum(role.raw, appearance.core, ratio)
        is ThemeIssue.RaisedIndistinctFromPaper -> CoreThemeIssue.RaisedIndistinctFromPaper(appearance.core, ratio)
        ThemeIssue.ShadowsUsed -> CoreThemeIssue.ShadowsUsed
        ThemeIssue.GradientsOnChrome -> CoreThemeIssue.GradientsOnChrome
        ThemeIssue.RadiusScaleOutOfOrder -> CoreThemeIssue.RadiusScaleOutOfOrder
        is ThemeIssue.ShellRadiusOffScale -> CoreThemeIssue.ShellRadiusOffScale(value)
        is ThemeIssue.HairlineTooHeavy -> CoreThemeIssue.HairlineTooHeavy(value)
        is ThemeIssue.TouchTargetTooSmall -> CoreThemeIssue.TouchTargetTooSmall(value)
    }

private fun role(raw: String) = ThemeColorRole.of(raw) ?: ThemeColorRole.PAPER

internal fun CoreThemeIssue.local(): ThemeIssue = when (this) {
    is CoreThemeIssue.MissingRole -> ThemeIssue.MissingRole(role(role), appearance.local())
    is CoreThemeIssue.BodyTextBelowAa -> ThemeIssue.BodyTextBelowAA(role(role), appearance.local(), ratio)
    is CoreThemeIssue.LargeTextOnly -> ThemeIssue.LargeTextOnly(role(role), appearance.local(), ratio)
    is CoreThemeIssue.AccentBelowUiMinimum -> ThemeIssue.AccentBelowUIMinimum(role(role), appearance.local(), ratio)
    is CoreThemeIssue.RaisedIndistinctFromPaper -> ThemeIssue.RaisedIndistinctFromPaper(appearance.local(), ratio)
    CoreThemeIssue.ShadowsUsed -> ThemeIssue.ShadowsUsed
    CoreThemeIssue.GradientsOnChrome -> ThemeIssue.GradientsOnChrome
    CoreThemeIssue.RadiusScaleOutOfOrder -> ThemeIssue.RadiusScaleOutOfOrder
    is CoreThemeIssue.ShellRadiusOffScale -> ThemeIssue.ShellRadiusOffScale(value)
    is CoreThemeIssue.HairlineTooHeavy -> ThemeIssue.HairlineTooHeavy(value)
    is CoreThemeIssue.TouchTargetTooSmall -> ThemeIssue.TouchTargetTooSmall(value)
}

internal fun CoreThemeFileIssue.local() = ThemeFileIssue(source, severity.local(), message, isAudit)

internal fun CoreThemeFileOutcome.local() =
    ThemeFileOutcome(source, specification?.local(), skippedReason, issues.map { it.local() })

// MARK: - Files

private fun ThemeFile.Face.core() = CoreThemeFileFace(families, design)

private fun CoreThemeFileFace.local() = ThemeFile.Face(families, design)

internal val ThemeFile.Structure.core: CoreThemeFileStructure
    get() = CoreThemeFileStructure(
        radius = radius?.let { CoreThemeFileRadius(it.panel, it.row, it.control, it.pill, it.shell) },
        border = border?.let { CoreThemeFileBorder(it.hairline, it.emphasis, it.focusRing) },
        spacing = spacing?.let { CoreThemeFileSpacing(it.xxs, it.xs, it.sm, it.md, it.lg, it.xl) },
        typography = typography?.let { type ->
            CoreThemeFileTypography(
                display = type.display?.core(),
                body = type.body?.core(),
                mono = type.mono?.core(),
                bodySize = type.bodySize,
                scale = type.scale?.let { CoreThemeFileTypeScale(it.caption, it.body, it.title, it.display, it.hero) },
                microLabel = type.microLabel?.let {
                    CoreThemeFileMicroLabel(it.size, it.weight, it.tracking, it.uppercase, it.role)
                },
            )
        },
        touchTarget = touchTarget,
        usesShadows = usesShadows,
        usesGradientsOnChrome = usesGradientsOnChrome,
    )

internal fun CoreThemeFileStructure.local() = ThemeFile.Structure(
    radius = radius?.let { ThemeFile.Radius(it.panel, it.row, it.control, it.pill, it.shell) },
    border = border?.let { ThemeFile.Border(it.hairline, it.emphasis, it.focusRing) },
    spacing = spacing?.let { ThemeFile.Spacing(it.xxs, it.xs, it.sm, it.md, it.lg, it.xl) },
    typography = typography?.let { type ->
        ThemeFile.Typography(
            display = type.display?.local(),
            body = type.body?.local(),
            mono = type.mono?.local(),
            bodySize = type.bodySize,
            scale = type.scale?.let { ThemeFile.TypeScale(it.caption, it.body, it.title, it.display, it.hero) },
            microLabel = type.microLabel?.let {
                ThemeFile.MicroLabel(it.size, it.weight, it.tracking, it.uppercase, it.role)
            },
        )
    },
    touchTarget = touchTarget,
    usesShadows = usesShadows,
    usesGradientsOnChrome = usesGradientsOnChrome,
)

internal val ThemeFile.core: CoreThemeFile
    get() {
        fun palette(p: ThemeFile.Palette?) = p?.let { CoreThemeFilePalette(it.light, it.dark) }
        fun entry(o: ThemeFile.PlatformOverride?) = o?.let { CoreThemeFilePlatformOverride(it.structure?.core) }
        return CoreThemeFile(
            identifier = identifier,
            name = name,
            summary = summary,
            lockedAppearance = when (val lock = lockedAppearance) {
                ThemeFile.Lock.Inherit -> CoreThemeFileLock.Inherit
                ThemeFile.Lock.Unlocked -> CoreThemeFileLock.Unlocked
                is ThemeFile.Lock.Locked -> CoreThemeFileLock.Locked(lock.raw)
            },
            extends = when (val base = extends) {
                ThemeFile.Base.DefaultTheme -> CoreThemeFileBase.DefaultTheme
                ThemeFile.Base.Nothing -> CoreThemeFileBase.Nothing
                is ThemeFile.Base.Theme -> CoreThemeFileBase.Theme(base.identifier)
            },
            seeds = palette(seeds),
            palette = palette(palette),
            structure = structure?.core,
            platforms = platforms?.let { CoreThemeFilePlatforms(entry(it.macos), entry(it.ios), entry(it.android)) },
        )
    }

internal fun CoreThemeFile.local(): ThemeFile {
    fun palette(p: CoreThemeFilePalette?) = p?.let { ThemeFile.Palette(it.light, it.dark) }
    fun entry(o: CoreThemeFilePlatformOverride?) = o?.let { ThemeFile.PlatformOverride(it.structure?.local()) }
    return ThemeFile(
        identifier = identifier,
        name = name,
        summary = summary,
        lockedAppearance = when (val lock = lockedAppearance) {
            CoreThemeFileLock.Inherit -> ThemeFile.Lock.Inherit
            CoreThemeFileLock.Unlocked -> ThemeFile.Lock.Unlocked
            is CoreThemeFileLock.Locked -> ThemeFile.Lock.Locked(lock.raw)
        },
        extends = when (val base = extends) {
            CoreThemeFileBase.DefaultTheme -> ThemeFile.Base.DefaultTheme
            CoreThemeFileBase.Nothing -> ThemeFile.Base.Nothing
            is CoreThemeFileBase.Theme -> ThemeFile.Base.Theme(base.identifier)
        },
        seeds = palette(seeds),
        palette = palette(palette),
        structure = structure?.local(),
        platforms = platforms?.let { ThemeFile.Platforms(entry(it.macos), entry(it.ios), entry(it.android)) },
    )
}
