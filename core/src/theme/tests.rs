//! `shared/themes/` against the core: every built-in file and every
//! conformance case is rebuilt from its own inputs and must come out byte for
//! byte as committed. `TAKT_REGENERATE_THEMES=1` rewrites them instead.

use std::path::{Path, PathBuf};

use super::*;
use crate::theme::json::Json;

fn shared_themes() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../shared/themes")
}

fn regenerating() -> bool {
    std::env::var("TAKT_REGENERATE_THEMES").as_deref() == Ok("1")
}

fn check(path: &Path, expected: &str) {
    if regenerating() {
        if std::fs::read_to_string(path).ok().as_deref() != Some(expected) {
            std::fs::write(path, expected).unwrap();
        }
        return;
    }
    let current = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        current == expected,
        "{} differs from the core; run TAKT_REGENERATE_THEMES=1 cargo test --manifest-path core/Cargo.toml theme",
        path.display()
    );
}

#[test]
fn the_shared_built_in_files_match_the_core() {
    let files = theme_shared_files();
    assert_eq!(files.len(), 5);
    for file in files {
        check(&shared_themes().join(&file.name), &file.json);
    }
}

fn string(value: Option<&Json>) -> String {
    match value {
        Some(Json::String(s)) => s.clone(),
        other => panic!("expected a string, found {other:?}"),
    }
}

#[test]
fn every_conformance_case_resolves_exactly_as_committed() {
    let folder = shared_themes().join("conformance");
    let mut entries: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    entries.sort();
    assert!(entries.len() >= 20, "{} cases", entries.len());
    for path in entries {
        let committed = std::fs::read(&path).unwrap();
        let case = json::parse(&committed).unwrap();
        let Some(Json::Array(inputs)) = case.get("files") else {
            panic!("{} has no files", path.display())
        };
        let files: Vec<CoreThemeNamedText> = inputs
            .iter()
            .map(|f| CoreThemeNamedText {
                name: string(f.get("name")),
                json: string(f.get("json")),
            })
            .collect();
        let rebuilt = theme_conformance_case(files, string(case.get("selected")));
        check(&path, &rebuilt);
    }
}

#[test]
fn the_built_ins_are_fit_to_ship_on_every_platform() {
    for platform in CoreThemePlatform::ALL {
        for builtin in theme_builtins(platform) {
            let blocking: Vec<_> = audit::validate(&builtin)
                .into_iter()
                .filter(|i| i.severity() == CoreThemeIssueSeverity::Error)
                .collect();
            assert!(
                blocking.is_empty(),
                "{} on {platform:?}: {blocking:?}",
                builtin.identifier
            );
            assert!(
                audit::structure_findings(&builtin.structure).is_empty(),
                "{} on {platform:?}",
                builtin.identifier
            );
        }
    }
    let android =
        builtins::specification(builtins::CHALK_IDENTIFIER, CoreThemePlatform::Android).unwrap();
    assert_eq!(android.structure.touch_target, 48.0);
    assert_eq!(android.structure.typography.body_size, 16.0);
}

#[test]
fn a_shared_file_resolves_back_to_its_built_in_everywhere() {
    for identifier in builtins::IDENTIFIERS {
        let text = file::encode(&theme_shared_file(identifier.into()).unwrap()).unwrap();
        let (_, issues) = file::decode(text.as_bytes(), "shared.json");
        assert!(issues.is_empty(), "{issues:?}");
        for platform in CoreThemePlatform::ALL {
            let renamed = text.replace(identifier, "user.copy");
            let outcomes = loader::load(
                &[CoreThemeFileSource {
                    name: "copy.json".into(),
                    data: renamed.into_bytes(),
                }],
                platform,
                None,
                None,
            );
            let loaded = outcomes[0].specification.clone().unwrap();
            let expected = builtins::specification(identifier, platform).unwrap();
            assert_eq!(
                loaded.structure, expected.structure,
                "{identifier} on {platform:?}"
            );
            for appearance in CoreThemeAppearance::ALL {
                // Compared as written: hex is 255 steps, so 0.7 alpha reads back as b3.
                assert_eq!(
                    conformance::resolved(&loaded, appearance).colors,
                    conformance::resolved(expected, appearance).colors,
                    "{identifier} on {platform:?}"
                );
            }
            assert_eq!(loaded.locked_appearance, expected.locked_appearance);
        }
    }
}

#[test]
fn decoding_names_the_first_bad_value_the_way_swift_did() {
    let message = |text: &str| file::decode(text.as_bytes(), "x.json").1[0].message.clone();
    assert_eq!(message("not json"), "not valid JSON");
    assert_eq!(message("[1]"), "the file should be an object");
    assert_eq!(message("null"), "the file should be an object, not null");
    assert_eq!(
        message(r#"{ "structure": { "radius": { "panel": "4" } } }"#),
        "structure.radius.panel should be a number"
    );
    assert_eq!(
        message(r#"{ "structure": { "typography": { "body": { "families": "Inter" } } } }"#),
        "structure.typography.body.families should be an object"
    );
    assert_eq!(
        message(r#"{ "structure": { "typography": { "body": { "families": [null] } } } }"#),
        "structure.typography.body.families.0 should be a string, not null"
    );
    assert_eq!(
        message(r#"{ "palette": { "light": { "paper": 1 } }, "extends": 3 }"#),
        "palette.light.paper should be a string"
    );
    assert_eq!(message(r#"{ "extends": 3 }"#), "extends should be a string");
    assert_eq!(
        message(r#"{ "structure": { "usesShadows": 1 } }"#),
        "structure.usesShadows should be true or false"
    );
    assert_eq!(
        message(r#"{ "platforms": [] }"#),
        "platforms should be an object"
    );
}

#[test]
fn a_bad_size_keeps_the_inherited_value_and_says_so() {
    let merged = theme_merge_structure(
        Some(CoreThemeFileStructure {
            radius: Some(file::CoreThemeFileRadius {
                panel: Some(-2.0),
                control: Some(f64::NAN),
                ..Default::default()
            }),
            typography: Some(file::CoreThemeFileTypography {
                body_size: Some(0.0),
                ..Default::default()
            }),
            ..Default::default()
        }),
        builtins::default_theme(CoreThemePlatform::Macos)
            .structure
            .clone(),
        "structure".into(),
    );
    let messages: Vec<_> = merged.reports.iter().map(|r| r.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "structure.radius.panel -2 should be zero or more",
            "structure.radius.control nan should be zero or more",
            "structure.typography.bodySize 0 should be above zero",
        ]
    );
    assert_eq!(merged.structure.radius.panel, 12.0);
}
