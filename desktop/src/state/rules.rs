//! Rules typed as text: a filter list's or a rule column's definition, one
//! line each, the way the CLI's `--rule field:op:value` reads them.
//!
//! ```text
//! name Errands this week      (a filter list's name)
//! show open                   (open, done or all)
//! match all                   (all or any)
//! tag has errand
//! due within 7
//! ```
//!
//! The core checks the result (`validate_filter_definition`) and says it in
//! words (`describe_filter_definition`); this only reads and writes lines.

use takt_core::filter_lists::{
    FilterDefinition, FilterRule, describe_filter_definition, filter_rule_catalog,
    validate_filter_definition,
};

/// What the text read as: the name line, if any, and the definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub name: Option<String>,
    pub definition: FilterDefinition,
}

/// The definition as lines, with `name` first when given.
pub fn write(name: Option<&str>, definition: &FilterDefinition) -> String {
    let mut lines = Vec::new();
    if let Some(name) = name {
        lines.push(format!("name {name}"));
    }
    lines.push(format!("show {}", definition.show));
    lines.push(format!("match {}", definition.match_mode));
    for rule in &definition.rules {
        let line = format!("{} {} {}", rule.field, rule.op, rule.value);
        lines.push(line.trim().to_string());
    }
    lines.join("\n")
}

/// Reads the lines, checking each rule against the catalogue.
pub fn read(text: &str) -> Result<Reading, String> {
    let mut reading = Reading {
        name: None,
        definition: FilterDefinition::default(),
    };
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        // `field:op:value`, the CLI's spelling, reads the same.
        let line = if !line.contains(' ') && line.contains(':') {
            line.replacen(':', " ", 2)
        } else {
            line.to_string()
        };
        let (head, rest) = line.split_once(' ').unwrap_or((line.as_str(), ""));
        let rest = rest.trim();
        match head.to_lowercase().as_str() {
            "name" => reading.name = Some(rest.to_string()),
            "show" => reading.definition.show = rest.to_lowercase(),
            "match" => reading.definition.match_mode = rest.to_lowercase(),
            field => {
                let (op, value) = rest.split_once(' ').unwrap_or((rest, ""));
                reading.definition.rules.push(FilterRule {
                    field: field.to_string(),
                    op: op.to_string(),
                    value: value.trim().to_string(),
                });
            }
        }
    }
    reading.definition =
        validate_filter_definition(reading.definition).map_err(|e| e.to_string())?;
    Ok(reading)
}

/// The text in words, or what is wrong with it.
pub fn describe(text: &str) -> String {
    match read(text) {
        Ok(reading) => describe_filter_definition(reading.definition),
        Err(error) => error,
    }
}

/// Every field and its operators, a line each, for the editor's help.
pub fn catalogue_help() -> Vec<String> {
    filter_rule_catalog()
        .into_iter()
        .map(|field| {
            let ops: Vec<String> = field
                .ops
                .iter()
                .map(|op| {
                    if op.value_kind == "none" {
                        op.op.clone()
                    } else {
                        format!("{} ‹{}›", op.op, op.value_kind)
                    }
                })
                .collect();
            format!("{}  {}", field.field, ops.join(" · "))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_read_back_as_written() {
        let text = "name Errands\nshow open\nmatch any\ntag has errand\ndue within 7";
        let reading = read(text).unwrap();
        assert_eq!(reading.name.as_deref(), Some("Errands"));
        assert_eq!(reading.definition.match_mode, "any");
        assert_eq!(reading.definition.rules.len(), 2);
        assert_eq!(write(Some("Errands"), &reading.definition), text);
    }

    #[test]
    fn the_clis_spelling_reads_too_and_nonsense_is_refused() {
        let reading = read("tag:has:work").unwrap();
        assert_eq!(reading.definition.rules[0].value, "work");
        assert!(read("colour is blue").is_err());
    }
}
