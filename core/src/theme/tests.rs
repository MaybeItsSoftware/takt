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
    assert_eq!(android.structure.typography.body_size, 15.0);
}

#[test]
fn the_phones_are_set_as_the_mac_is_a_step_larger() {
    let mac = builtins::specification(builtins::ONE_IDENTIFIER, CoreThemePlatform::Macos).unwrap();
    for platform in [CoreThemePlatform::Ios, CoreThemePlatform::Android] {
        let phone = builtins::specification(builtins::ONE_IDENTIFIER, platform).unwrap();
        assert_eq!(phone.structure.radius, mac.structure.radius, "{platform:?}");
        assert_eq!(
            phone.structure.spacing, mac.structure.spacing,
            "{platform:?}"
        );
        let scale = &phone.structure.typography.scale;
        assert_eq!(
            (scale.caption, scale.body, scale.title),
            (13.0, 15.0, 17.0),
            "{platform:?}"
        );
    }
    assert_eq!(
        builtins::default_theme(CoreThemePlatform::Ios).identifier,
        builtins::ONE_IDENTIFIER
    );
    assert_eq!(
        builtins::extends_base(CoreThemePlatform::Ios).identifier,
        builtins::PRIORITY_IDENTIFIER
    );
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
    assert_eq!(merged.structure.radius.panel, 0.0);
}

// MARK: - Zed themes

fn meadow() -> Vec<u8> {
    std::fs::read(shared_themes().join("zed/meadow.json")).unwrap()
}

fn load_one(name: &str, data: Vec<u8>) -> Vec<CoreThemeFileOutcome> {
    loader::load(
        &[CoreThemeFileSource {
            name: name.into(),
            data,
        }],
        CoreThemePlatform::Macos,
        None,
        None,
    )
}

fn hex(spec: &CoreThemeSpecification, role: &str, appearance: CoreThemeAppearance) -> String {
    color::hex_string(&spec.palette.color(role, appearance))
}

#[test]
fn a_zed_family_loads_as_one_locked_theme_per_variant() {
    let outcomes = load_one("meadow.json", meadow());
    assert_eq!(outcomes.len(), 2);
    for outcome in &outcomes {
        assert_eq!(outcome.source, "meadow.json");
        let problems: Vec<_> = outcome
            .issues
            .iter()
            .filter(|i| !i.is_audit && i.severity != CoreThemeIssueSeverity::Note)
            .collect();
        assert!(problems.is_empty(), "{problems:?}");
    }
    let dusk = outcomes[0].specification.clone().unwrap();
    let morning = outcomes[1].specification.clone().unwrap();
    assert_eq!(dusk.identifier, "zed.meadow-dusk");
    assert_eq!(dusk.name, "Meadow Dusk");
    assert_eq!(dusk.summary, "From the Zed theme Meadow, by Takt.");
    assert_eq!(dusk.locked_appearance, Some(CoreThemeAppearance::Dark));
    assert_eq!(morning.identifier, "zed.meadow-morning");
    assert_eq!(morning.locked_appearance, Some(CoreThemeAppearance::Light));
    // Zed's look: the structure is Zed One's.
    let one = builtins::specification(builtins::ONE_IDENTIFIER, CoreThemePlatform::Macos).unwrap();
    assert_eq!(dusk.structure, one.structure);
}

#[test]
fn a_zed_variant_maps_its_style_keys_onto_the_roles() {
    let outcomes = load_one("meadow.json", meadow());
    let dusk = outcomes[0].specification.clone().unwrap();
    let dark = CoreThemeAppearance::Dark;
    for (role, expected) in [
        ("paper", "#161C1A"),       // editor.background
        ("raised", "#222B28"),      // elevated_surface.background
        ("altRow", "#1A201E"),      // panel.background
        ("hover", "#282E2C"),       // element.hover, #ffffff14 over the page
        ("well", "#1D2421"),        // background
        ("border", "#33403A"),      // border
        ("borderMuted", "#28322E"), // border.variant
        ("inputBorder", "#33403A"), // border
        ("ink", "#DFE8E2"),         // text
        ("mutedText", "#A3B3AA"),   // text.muted
        ("dimText", "#6F7F77"),     // text.placeholder
        ("primary", "#5FB3A1"),     // text.accent
        ("success", "#8CC06B"),
        ("danger", "#E0706B"), // error
        ("warning", "#D9B25C"),
        ("categoricalPurple", "#B48CE6"), // nearest accent to violet
        ("categoricalPink", "#E48AC0"),
        ("categoricalOrange", "#E8955A"),
    ] {
        assert_eq!(hex(&dusk, role, dark), expected, "{role}");
    }

    // A sparser variant: what it leaves out is grown from its own page and
    // text, not taken from Zed One.
    let morning = outcomes[1].specification.clone().unwrap();
    let light = CoreThemeAppearance::Light;
    assert_eq!(hex(&morning, "paper", light), "#FBFCF9");
    assert_eq!(hex(&morning, "raised", light), "#F2F5F1"); // surface.background
    assert_eq!(hex(&morning, "well", light), "#EEF2EE");
    assert_eq!(hex(&morning, "primary", light), "#2F7F6F");
    let grown = seeds::CoreThemeSeeds {
        background: color::from_hex("#fbfcf9"),
        foreground: color::from_hex("#1f2a25"),
        ..Default::default()
    }
    .roles(light)
    .unwrap();
    assert_eq!(
        hex(&morning, "inputBorder", light),
        hex(&morning, "border", light),
        "inputBorder is Zed's border"
    );
    assert_eq!(
        morning.palette.color("altRow", light),
        // panel.background is stated, so not the grown one
        color::from_hex("#f2f5f1").unwrap()
    );
    assert_ne!(grown["altRow"], morning.palette.color("altRow", light));
}

#[test]
fn a_zed_family_decodes_as_its_first_variant_through_the_single_file_path() {
    let decoded = theme_decode(meadow(), "meadow.json".into());
    let file = decoded.file.unwrap();
    assert_eq!(
        theme_identifier(file, "meadow.json".into()),
        "zed.meadow-dusk"
    );
}

#[test]
fn a_zed_family_is_told_apart_from_a_takt_theme_by_its_shape() {
    // No $schema: the themes list is enough.
    let bare = br##"{ "name": "Bare", "themes": [ { "name": "Bare", "appearance": "light", "style": { "background": "#ffffff", "text": "#000000" } } ] }"##;
    let outcomes = load_one("bare.json", bare.to_vec());
    assert_eq!(
        outcomes[0].specification.as_ref().unwrap().identifier,
        "zed.bare"
    );
    // A $schema on zed.dev without a list is a Zed file that says nothing.
    let empty = br#"{ "$schema": "https://zed.dev/schema/themes/v0.2.0.json", "name": "Empty" }"#;
    let outcomes = load_one("empty.json", empty.to_vec());
    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].specification.is_none());
    assert!(
        outcomes[0]
            .issues
            .iter()
            .any(|i| i.message == "a Zed theme should have a themes list")
    );
    // A Takt theme is still a Takt theme.
    let takt = br##"{ "name": "Sea", "seeds": { "light": { "background": "#f4f8f9", "foreground": "#14303a" } } }"##;
    let outcomes = load_one("sea.json", takt.to_vec());
    assert_eq!(
        outcomes[0].specification.as_ref().unwrap().identifier,
        "user.sea"
    );
}

#[test]
fn a_zed_variant_reports_bad_values_and_guesses_a_missing_appearance() {
    let text = br##"{ "name": "Odd", "themes": [
        { "name": "Odd", "style": { "editor.background": "#f8f8f8", "text": "#111111", "border": "teal", "text.muted": 4 } },
        { "name": "Odd", "appearance": "dim", "style": {} }
    ] }"##;
    let outcomes = load_one("odd.json", text.to_vec());
    assert_eq!(outcomes.len(), 2);
    let first = &outcomes[0];
    let spec = first.specification.as_ref().unwrap();
    assert_eq!(spec.locked_appearance, Some(CoreThemeAppearance::Light));
    let messages: Vec<&str> = first.issues.iter().map(|i| i.message.as_str()).collect();
    assert!(messages.contains(&"themes.0.appearance is missing; read as light"));
    assert!(messages.contains(&"themes.0.style.border \"teal\" is not a hex colour; ignored"));
    assert!(messages.contains(&"themes.0.style.text.muted should be a hex colour; ignored"));
    // The second variant has the same name, so the same identifier: skipped.
    assert!(outcomes[1].specification.is_none());
    assert!(
        outcomes[1]
            .skipped_reason
            .as_deref()
            .unwrap()
            .contains("already used by odd.json")
    );
}

#[test]
fn a_zed_variant_takes_a_primary_that_can_carry_the_focus_ring() {
    // text.accent is the page's own colour; the cursor stands off it.
    let text = br##"{ "themes": [ { "name": "Flat", "appearance": "dark", "style": {
        "editor.background": "#202020", "text": "#e0e0e0", "text.accent": "#2020207f",
        "border.focused": "#202020", "players": [ { "cursor": "#4c9aff" } ],
        "element.hover": "#fff1"
    } } ] }"##;
    let outcomes = load_one("flat.json", text.to_vec());
    let spec = outcomes[0].specification.clone().unwrap();
    let dark = CoreThemeAppearance::Dark;
    assert_eq!(hex(&spec, "primary", dark), "#4C9AFF");
    // #rgba is Zed's too: #fff1 is white at 1/15.
    assert_eq!(hex(&spec, "hover", dark), "#2F2F2F");
    // Both tables say the same, though only the dark one is in force.
    assert_eq!(
        spec.palette.color("paper", CoreThemeAppearance::Light),
        spec.palette.color("paper", dark)
    );
}

#[test]
fn a_zed_variant_carries_zeds_chrome() {
    let style = r##"{
      "$schema": "https://zed.dev/schema/themes/v0.2.0.json",
      "name": "Chrome",
      "themes": [{
        "name": "Chrome Dark",
        "appearance": "dark",
        "style": {
          "editor.background": "#101010",
          "panel.background": "#202020",
          "text": "#eeeeee",
          "text.accent": "#4080ff",
          "ghost_element.selected": "#303030",
          "panel.indent_guide": "#ffffff20",
          "status_bar.background": "#123456",
          "panel.focused_border": "#ff8000",
          "icon": "#aaaaaa"
        }
      }]
    }"##;
    let outcomes = load_one("chrome.json", style.as_bytes().to_vec());
    let spec = outcomes[0].specification.clone().unwrap();
    let dark = CoreThemeAppearance::Dark;
    let chrome = |role: &str| color::hex_string(&spec.palette.chrome(role, dark));
    assert_eq!(chrome("selected"), "#303030");
    // #ffffff20 over the panel.
    assert_eq!(chrome("indentGuide"), "#3C3C3C");
    assert_eq!(chrome("statusBar"), "#123456");
    assert_eq!(chrome("focus"), "#FF8000");
    assert_eq!(chrome("icon"), "#AAAAAA");

    // Meadow names none of the five: its selection and guide are derived from
    // its own colours, not Zed One's, and its status bar is left unstated.
    let dusk = load_one("meadow.json", meadow())[0]
        .specification
        .clone()
        .unwrap();
    let one = builtins::specification(builtins::ONE_IDENTIFIER, CoreThemePlatform::Macos).unwrap();
    assert_ne!(
        dusk.palette.chrome("selected", dark),
        one.palette.chrome("selected", dark)
    );
    assert!(dusk.palette.stated("statusBar", dark).is_none());
    assert_eq!(
        dusk.palette.chrome("statusBar", dark),
        dusk.palette.color("well", dark)
    );
}

#[test]
fn zed_one_draws_the_macs_selection_and_guides() {
    let one = builtins::specification(builtins::ONE_IDENTIFIER, CoreThemePlatform::Ios).unwrap();
    let chrome = |role: &str, a| color::hex_string(&one.palette.chrome(role, a));
    assert_eq!(chrome("selected", CoreThemeAppearance::Dark), "#454A56");
    assert_eq!(chrome("indentGuide", CoreThemeAppearance::Dark), "#363C46");
    assert_eq!(chrome("selected", CoreThemeAppearance::Light), "#CACACA");
    assert_eq!(chrome("indentGuide", CoreThemeAppearance::Light), "#DFDFE0");
    assert_eq!(chrome("statusBar", CoreThemeAppearance::Dark), "#3B414D");
}

#[test]
fn a_focus_fainter_than_the_borders_takes_the_accent() {
    let style = r##"{
      "name": "Paper",
      "themes": [{
        "name": "Paper Light",
        "appearance": "light",
        "style": {
          "editor.background": "#faf8f0",
          "text": "#222222",
          "text.accent": "#6547b8",
          "border": "#6547b8",
          "border.variant": "#6547b8",
          "panel.focused_border": "#cecbbf"
        }
      }]
    }"##;
    let spec = load_one("paper.json", style.as_bytes().to_vec())[0]
        .specification
        .clone()
        .unwrap();
    let light = CoreThemeAppearance::Light;
    assert_eq!(
        spec.palette.chrome("focus", light),
        spec.palette.color("primary", light)
    );
}
