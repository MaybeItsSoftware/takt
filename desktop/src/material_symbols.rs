//! Google's Material Symbols (Rounded, regular weight, unfilled) as list
//! icons. The font is bundled with ligatures stripped, so a symbol is drawn
//! by its codepoint, looked up by name in Google's codepoints file.
//!
//! A chosen symbol is stored among the icon prefs as `material:<name>`,
//! beside the emoji a row can wear instead.

use std::collections::HashMap;
use std::sync::OnceLock;

/// The family the bundled font registers.
pub const FONT_FAMILY: &str = "Material Symbols Rounded";

pub const FONT: &[u8] = include_bytes!("../../shared/fonts/MaterialSymbolsRounded.ttf");

const CODEPOINTS: &str = include_str!("../../shared/fonts/MaterialSymbolsRounded.codepoints");

const PREFIX: &str = "material:";

/// Offered before anything is typed, after the emoji.
pub const SUGGESTED: &[&str] = &[
    "school",
    "work",
    "home",
    "favorite",
    "fitness_center",
    "hiking",
    "menu_book",
    "music_note",
    "radio",
    "sailing",
    "star",
    "groups",
    "volunteer_activism",
    "campaign",
    "event",
    "lightbulb",
    "code",
    "science",
    "rocket_launch",
    "eco",
    "flight",
    "sports_esports",
    "person",
    "build",
];

/// Every symbol's name, in the file's (alphabetical) order, with its glyph.
fn table() -> &'static (Vec<&'static str>, HashMap<&'static str, char>) {
    static TABLE: OnceLock<(Vec<&'static str>, HashMap<&'static str, char>)> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut names = Vec::new();
        let mut glyphs = HashMap::new();
        for line in CODEPOINTS.lines() {
            let Some((name, hex)) = line.split_once(' ') else {
                continue;
            };
            let Some(glyph) = u32::from_str_radix(hex.trim(), 16)
                .ok()
                .and_then(char::from_u32)
            else {
                continue;
            };
            if glyphs.insert(name, glyph).is_none() {
                names.push(name);
            }
        }
        (names, glyphs)
    })
}

/// The pref value that stands for the symbol `name`.
pub fn pref_value(name: &str) -> String {
    format!("{PREFIX}{name}")
}

/// The symbol a pref value names, if it names one that exists.
pub fn name_of(value: &str) -> Option<&'static str> {
    let name = value.strip_prefix(PREFIX)?;
    table().1.get_key_value(name).map(|(name, _)| *name)
}

/// The glyph to draw, in [`FONT_FAMILY`], for a pref value naming a symbol.
pub fn glyph(value: &str) -> Option<char> {
    name_of(value).and_then(|name| table().1.get(name).copied())
}

/// How a symbol's name reads: `fitness_center` as "fitness center".
pub fn label(name: &str) -> String {
    name.replace('_', " ")
}

/// How an icon pref reads where only text can go, as the inspector shows
/// it: a symbol by its name, an emoji as itself.
pub fn describe(value: &str) -> String {
    match name_of(value) {
        Some(name) => format!("{} (Google icon)", label(name)),
        None => value.to_string(),
    }
}

/// Symbols whose names hold every word of `query`, names starting with
/// the query first, then shortest, up to `limit` of them.
pub fn search(query: &str, limit: usize) -> Vec<&'static str> {
    let query = query.trim().to_lowercase().replace([' ', '-'], "_");
    if query.is_empty() {
        return Vec::new();
    }
    let words: Vec<&str> = query.split('_').filter(|w| !w.is_empty()).collect();
    let mut found: Vec<&'static str> = table()
        .0
        .iter()
        .copied()
        .filter(|name| words.iter().all(|word| name.contains(word)))
        .collect();
    found.sort_by_key(|name| (!name.starts_with(&query), *name != query, name.len()));
    found.truncate(limit);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_resolve_to_glyphs() {
        assert!(table().0.len() > 3000);
        assert!(glyph(&pref_value("school")).is_some());
        assert!(glyph(&pref_value("no_such_symbol")).is_none());
        assert!(glyph("🎓").is_none());
        for name in SUGGESTED {
            assert!(glyph(&pref_value(name)).is_some(), "{name}");
        }
    }

    #[test]
    fn search_puts_exact_and_prefix_matches_first() {
        let found = search("school", 10);
        assert_eq!(found[0], "school");
        assert!(found.iter().all(|n| n.contains("school")));
        assert_eq!(search("fitness center", 5)[0], "fitness_center");
        assert!(search("", 5).is_empty());
    }

    #[test]
    fn describe_reads_names_and_passes_emoji_through() {
        assert_eq!(describe("material:menu_book"), "menu book (Google icon)");
        assert_eq!(describe("🎓"), "🎓");
    }
}
