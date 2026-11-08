//! The theme format: reading a theme file, following `extends`, merging it
//! over its base, auditing the result, and the built-in themes. Every client
//! resolves themes here; each keeps only its own view types and the
//! conversion to its UI toolkit's colour. The format is `docs/themes.md`, and
//! `shared/themes/` holds the cases every client is checked against.

pub mod audit;
pub mod builtins;
pub mod color;
pub mod conformance;
pub mod export;
pub mod file;
pub mod json;
pub mod loader;
pub mod merge;
pub mod model;
pub mod seeds;
pub mod zed;

use std::collections::HashMap;

use audit::CoreThemeIssue;
use conformance::{CoreThemeNamedText, CoreThemeResolved};
use file::{CoreThemeFile, CoreThemeFileIssue, CoreThemeFileStructure};
use loader::CoreThemeFileSource;
use merge::CoreThemeFileOutcome;
use model::{
    CoreThemeAppearance, CoreThemeColor, CoreThemeIssueSeverity, CoreThemePlatform,
    CoreThemeSpecification, CoreThemeStructure, CoreThemeTypeScale,
};
use seeds::CoreThemeSeeds;

/// One file decoded: the file, or `None` and why not.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreThemeDecoded {
    pub file: Option<CoreThemeFile>,
    pub issues: Vec<CoreThemeFileIssue>,
}

/// A structure merged, and what was wrong with the overrides.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreThemeMergedStructure {
    pub structure: CoreThemeStructure,
    pub reports: Vec<CoreThemeReport>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreThemeReport {
    pub severity: CoreThemeIssueSeverity,
    pub message: String,
}

/// A theme as one platform resolved it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreThemePlatformSpecification {
    pub platform: CoreThemePlatform,
    pub specification: CoreThemeSpecification,
}

/// A built-in's partial structure for one platform.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreThemePlatformStructure {
    pub platform: CoreThemePlatform,
    pub structure: CoreThemeFileStructure,
}

/// The built-ins as `platform` resolves them, the default first.
#[uniffi::export]
pub fn theme_builtins(platform: CoreThemePlatform) -> Vec<CoreThemeSpecification> {
    builtins::all(platform).to_vec()
}

/// What the built-in `identifier` lays over its own structure on each phone.
#[uniffi::export]
pub fn theme_builtin_platform_structures(identifier: String) -> Vec<CoreThemePlatformStructure> {
    CoreThemePlatform::ALL
        .iter()
        .filter_map(|platform| {
            builtins::platform_structure(&identifier, *platform).map(|structure| {
                CoreThemePlatformStructure {
                    platform: *platform,
                    structure,
                }
            })
        })
        .collect()
}

#[uniffi::export]
pub fn theme_decode(data: Vec<u8>, source: String) -> CoreThemeDecoded {
    let (file, issues) = file::decode(&data, &source);
    CoreThemeDecoded { file, issues }
}

/// Loads a folder of files for `platform`. `built_ins` and `default_base`
/// default to the built-ins as resolved for it.
#[uniffi::export]
pub fn theme_load(
    sources: Vec<CoreThemeFileSource>,
    platform: CoreThemePlatform,
    built_ins: Option<Vec<CoreThemeSpecification>>,
    default_base: Option<CoreThemeSpecification>,
) -> Vec<CoreThemeFileOutcome> {
    loader::load(
        &sources,
        platform,
        built_ins.as_deref(),
        default_base.as_ref(),
    )
}

#[uniffi::export]
pub fn theme_resolve(
    file: CoreThemeFile,
    source: String,
    base: Option<CoreThemeSpecification>,
    platform: CoreThemePlatform,
) -> CoreThemeFileOutcome {
    loader::resolve(&file, &source, base.as_ref(), platform)
}

/// Lays a partial structure over a whole one, reporting under `path`.
#[uniffi::export]
pub fn theme_merge_structure(
    overrides: Option<CoreThemeFileStructure>,
    base: CoreThemeStructure,
    path: String,
) -> CoreThemeMergedStructure {
    let mut reports = Vec::new();
    let structure = merge::merge_structure(
        overrides.as_ref(),
        &base,
        &path,
        &mut |severity, message| reports.push(CoreThemeReport { severity, message }),
    );
    CoreThemeMergedStructure { structure, reports }
}

#[uniffi::export]
pub fn theme_identifier(file: CoreThemeFile, source: String) -> String {
    loader::identifier(&file, &source)
}

#[uniffi::export]
pub fn theme_validate(specification: CoreThemeSpecification) -> Vec<CoreThemeIssue> {
    audit::validate(&specification)
}

#[uniffi::export]
pub fn theme_contrast_findings(specification: CoreThemeSpecification) -> Vec<CoreThemeIssue> {
    audit::contrast_findings(&specification)
}

#[uniffi::export]
pub fn theme_structure_findings(structure: CoreThemeStructure) -> Vec<CoreThemeIssue> {
    audit::structure_findings(&structure)
}

#[uniffi::export]
pub fn theme_issue_message(issue: CoreThemeIssue) -> String {
    issue.message()
}

#[uniffi::export]
pub fn theme_issue_severity(issue: CoreThemeIssue) -> CoreThemeIssueSeverity {
    issue.severity()
}

#[uniffi::export]
pub fn theme_contrast_ratio(a: CoreThemeColor, b: CoreThemeColor) -> f64 {
    color::contrast_ratio(&a, &b)
}

#[uniffi::export]
pub fn theme_relative_luminance(color: CoreThemeColor) -> f64 {
    color::relative_luminance(&color)
}

/// The roles `seeds` paint in `appearance`, or `None` without both a
/// background and a foreground.
#[uniffi::export]
pub fn theme_seed_roles(
    seeds: CoreThemeSeeds,
    appearance: CoreThemeAppearance,
) -> Option<HashMap<String, CoreThemeColor>> {
    seeds.roles(appearance)
}

#[uniffi::export]
pub fn theme_mix(a: CoreThemeColor, b: CoreThemeColor, t: f64) -> CoreThemeColor {
    color::mix(&a, &b, t)
}

#[uniffi::export]
pub fn theme_proportioned_scale(body: f64) -> CoreThemeTypeScale {
    CoreThemeTypeScale::proportioned(body)
}

/// Every value of `specification` as a file that extends the default, with
/// each variant's differences under `platforms`.
#[uniffi::export]
pub fn theme_file_from_specification(
    specification: CoreThemeSpecification,
    variants: Vec<CoreThemePlatformSpecification>,
) -> CoreThemeFile {
    let variants: Vec<(CoreThemePlatform, &CoreThemeSpecification)> = variants
        .iter()
        .map(|v| (v.platform, &v.specification))
        .collect();
    export::file_from_specification(&specification, &variants)
}

#[uniffi::export]
pub fn theme_structure_stated(structure: CoreThemeStructure) -> CoreThemeFileStructure {
    export::stated(&structure)
}

#[uniffi::export]
pub fn theme_structure_difference(
    base: CoreThemeStructure,
    target: CoreThemeStructure,
) -> Option<CoreThemeFileStructure> {
    export::difference(&base, &target)
}

/// The file pretty-printed with sorted keys; `None` if a number in it is not finite.
#[uniffi::export]
pub fn theme_file_encode(file: CoreThemeFile) -> Option<String> {
    file::encode(&file)
}

#[uniffi::export]
pub fn theme_resolved(
    specification: CoreThemeSpecification,
    requested: CoreThemeAppearance,
) -> CoreThemeResolved {
    conformance::resolved(&specification, requested)
}

/// A conformance case, as the bytes `shared/themes/conformance/` holds.
#[uniffi::export]
pub fn theme_conformance_case(files: Vec<CoreThemeNamedText>, selected: String) -> String {
    conformance::case(&files, &selected)
}

/// The built-ins as the complete files `shared/themes/` holds.
#[uniffi::export]
pub fn theme_shared_files() -> Vec<CoreThemeNamedText> {
    conformance::shared_files()
}

#[uniffi::export]
pub fn theme_shared_file(identifier: String) -> Option<CoreThemeFile> {
    conformance::shared_file(&identifier)
}

#[cfg(test)]
mod tests;
