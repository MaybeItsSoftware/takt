//! A folder of theme files, loaded: decoded, `extends` followed, merged and
//! audited.

use std::collections::{HashMap, HashSet};

use super::builtins;
use super::file::{CoreThemeFile, CoreThemeFileBase, CoreThemeFileIssue, decode_all};
use super::merge::{CoreThemeFileOutcome, merge_file, sorted};
use super::model::{CoreThemeIssueSeverity, CoreThemePlatform, CoreThemeSpecification};

/// The extension a theme file has to carry to be read.
pub const FILE_EXTENSION: &str = "json";
/// A theme with no `identifier` is named after its file under this prefix.
pub const DERIVED_IDENTIFIER_PREFIX: &str = "user.";

/// One file's bytes, named.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreThemeFileSource {
    pub name: String,
    pub data: Vec<u8>,
}

/// `dusk.json` → `dusk`.
pub fn stem(source: &str) -> String {
    source
        .strip_suffix(&format!(".{FILE_EXTENSION}"))
        .unwrap_or(source)
        .to_string()
}

/// The file's own identifier, or one derived from its name.
pub fn identifier(file: &CoreThemeFile, source: &str) -> String {
    match file.identifier.as_deref().map(str::trim) {
        Some(stated) if !stated.is_empty() => stated.to_string(),
        _ => format!("{DERIVED_IDENTIFIER_PREFIX}{}", stem(source)),
    }
}

fn skipped(source: &str, reason: &str, issues: &[CoreThemeFileIssue]) -> CoreThemeFileOutcome {
    let mut all = vec![CoreThemeFileIssue::new(
        source,
        CoreThemeIssueSeverity::Error,
        format!("not loaded: {reason}"),
    )];
    all.extend(issues.iter().cloned());
    CoreThemeFileOutcome {
        source: source.to_string(),
        specification: None,
        skipped_reason: Some(reason.to_string()),
        issues: sorted(all),
    }
}

/// Where an outcome came from: the file, and which of its themes (a Zed
/// family holds several; any other file one, at 0).
type OutcomeKey = (String, usize);

struct Candidate {
    source: String,
    index: usize,
    file: CoreThemeFile,
    issues: Vec<CoreThemeFileIssue>,
}

struct Resolver<'a> {
    platform: CoreThemePlatform,
    built_ins: HashMap<String, &'a CoreThemeSpecification>,
    default_base: &'a CoreThemeSpecification,
    candidates: HashMap<String, Candidate>,
    outcomes: HashMap<OutcomeKey, CoreThemeFileOutcome>,
    /// Memoised by identifier; `Some(None)` is settled as not loading.
    resolved: HashMap<String, Option<CoreThemeSpecification>>,
    in_progress: HashSet<String>,
}

impl Resolver<'_> {
    fn settle_skipped(&mut self, identifier: &str, reason: &str) {
        let candidate = &self.candidates[identifier];
        let outcome = skipped(&candidate.source, reason, &candidate.issues);
        self.outcomes
            .insert((candidate.source.clone(), candidate.index), outcome);
        self.resolved.insert(identifier.to_string(), None);
    }

    /// Depth first, so a theme can extend another user theme whatever the
    /// file order; `in_progress` catches a cycle.
    fn resolve(&mut self, identifier: &str) -> Option<CoreThemeSpecification> {
        if let Some(done) = self.resolved.get(identifier) {
            return done.clone();
        }
        let extends = self.candidates.get(identifier)?.file.extends.clone();
        if self.in_progress.contains(identifier) {
            self.settle_skipped(identifier, "its extends chain comes back round to itself");
            return None;
        }
        self.in_progress.insert(identifier.to_string());
        let result = self.resolve_inner(identifier, extends);
        self.in_progress.remove(identifier);
        result
    }

    fn resolve_inner(
        &mut self,
        identifier: &str,
        extends: CoreThemeFileBase,
    ) -> Option<CoreThemeSpecification> {
        let base: Option<CoreThemeSpecification> = match extends {
            CoreThemeFileBase::DefaultTheme => Some(self.default_base.clone()),
            CoreThemeFileBase::Nothing => None,
            CoreThemeFileBase::Theme { identifier: parent } => {
                if let Some(built_in) = self.built_ins.get(&parent) {
                    Some((*built_in).clone())
                } else if self.candidates.contains_key(&parent) {
                    match self.resolve(&parent) {
                        Some(parent_specification) => Some(parent_specification),
                        None => {
                            // A cycle may already have recorded this file's own outcome.
                            if !self.resolved.contains_key(identifier) {
                                self.settle_skipped(
                                    identifier,
                                    &format!("it extends \"{parent}\", which did not load"),
                                );
                            }
                            return None;
                        }
                    }
                } else {
                    self.settle_skipped(
                        identifier,
                        &format!("it extends \"{parent}\", which is not a theme"),
                    );
                    return None;
                }
            }
        };
        if let Some(done) = self.resolved.get(identifier) {
            return done.clone(); // Settled by a cycle below us.
        }
        let candidate = &self.candidates[identifier];
        let outcome = merge_file(
            &candidate.file,
            identifier,
            &candidate.source,
            base.as_ref(),
            &self.default_base.structure,
            self.platform,
        );
        let mut issues = candidate.issues.clone();
        issues.extend(outcome.issues);
        let specification = outcome.specification;
        self.outcomes.insert(
            (candidate.source.clone(), candidate.index),
            CoreThemeFileOutcome {
                source: outcome.source,
                specification: specification.clone(),
                skipped_reason: outcome.skipped_reason,
                issues: sorted(issues),
            },
        );
        self.resolved
            .insert(identifier.to_string(), specification.clone());
        specification
    }
}

/// Loads every source for `platform`, in name order — a Zed theme family
/// gives one outcome per variant, each under the family's file name — resolving `extends`
/// against `built_ins` and each other. An identifier that is a built-in's,
/// or an earlier file's, is skipped. Both default to the built-ins as
/// resolved for `platform`.
pub fn load(
    sources: &[CoreThemeFileSource],
    platform: CoreThemePlatform,
    built_ins: Option<&[CoreThemeSpecification]>,
    default_base: Option<&CoreThemeSpecification>,
) -> Vec<CoreThemeFileOutcome> {
    let built_ins = built_ins.unwrap_or_else(|| builtins::all(platform));
    let default_base = default_base.unwrap_or_else(|| builtins::default_theme(platform));
    let mut ordered: Vec<&CoreThemeFileSource> = sources.iter().collect();
    ordered.sort_by(|a, b| a.name.cmp(&b.name));
    let mut built_ins_by_identifier = HashMap::new();
    for built_in in built_ins {
        built_ins_by_identifier
            .entry(built_in.identifier.clone())
            .or_insert(built_in);
    }

    let mut resolver = Resolver {
        platform,
        built_ins: built_ins_by_identifier,
        default_base,
        candidates: HashMap::new(),
        outcomes: HashMap::new(),
        resolved: HashMap::new(),
        in_progress: HashSet::new(),
    };

    let mut counts: HashMap<String, usize> = HashMap::new();
    for source in &ordered {
        let themes = match decode_all(&source.data, &source.name) {
            Ok(themes) => themes,
            Err(issues) => {
                resolver.outcomes.insert(
                    (source.name.clone(), 0),
                    skipped(&source.name, "could not be read", &issues),
                );
                counts.insert(source.name.clone(), 1);
                continue;
            }
        };
        counts.insert(source.name.clone(), themes.len());
        for (index, (file, issues)) in themes.into_iter().enumerate() {
            let key = (source.name.clone(), index);
            let identifier = identifier(&file, &source.name);
            if resolver.built_ins.contains_key(&identifier) {
                let reason = format!(
                    "identifier \"{identifier}\" is a built-in theme's; give it one of its own"
                );
                resolver
                    .outcomes
                    .insert(key, skipped(&source.name, &reason, &issues));
            } else if let Some(earlier) = resolver.candidates.get(&identifier) {
                let reason = format!(
                    "identifier \"{identifier}\" is already used by {}",
                    earlier.source
                );
                resolver
                    .outcomes
                    .insert(key, skipped(&source.name, &reason, &issues));
            } else {
                resolver.candidates.insert(
                    identifier,
                    Candidate {
                        source: source.name.clone(),
                        index,
                        file,
                        issues,
                    },
                );
            }
        }
    }

    let mut identifiers: Vec<String> = resolver.candidates.keys().cloned().collect();
    identifiers.sort();
    for identifier in identifiers {
        resolver.resolve(&identifier);
    }

    ordered
        .iter()
        .flat_map(|source| {
            let count = counts.get(&source.name).copied().unwrap_or(0);
            (0..count).map(move |index| (source.name.clone(), index))
        })
        .filter_map(|key| resolver.outcomes.remove(&key))
        .collect()
}

/// One already-decoded file against a base (`None` for a file that inherits
/// no colours): the single-file path, for tests and tooling.
pub fn resolve(
    file: &CoreThemeFile,
    source: &str,
    base: Option<&CoreThemeSpecification>,
    platform: CoreThemePlatform,
) -> CoreThemeFileOutcome {
    merge_file(
        file,
        &identifier(file, source),
        source,
        base,
        &builtins::default_theme(platform).structure,
        platform,
    )
}
