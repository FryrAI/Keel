//! Raw-text expression matching and multiset subtraction for W011/E007.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use keel_core::config::{HomeRule, HomeSeverity};
use keel_core::config_homes::path_globs;
use keel_parsers::treesitter::detect_language;

use crate::types::Violation;

struct PreparedRule {
    rule: HomeRule,
    home: globset::GlobSet,
    scope: globset::GlobSet,
}

impl PreparedRule {
    fn eligible(&self, path: &str) -> bool {
        detect_language(Path::new(path)).is_some()
            && (self.rule.scope.is_empty() || matches_ancestor(&self.scope, path))
            && !matches_ancestor(&self.home, path)
    }
}

fn matches_ancestor(globs: &globset::GlobSet, path: &str) -> bool {
    Path::new(path).ancestors().any(|p| globs.is_match(p))
}

/// A single rule/pattern match on a line, including its formatting-neutral identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeOccurrence {
    pub rule: usize,
    pub pattern: usize,
    pub line: u32,
    pub text: String,
}

/// Prepared home rules shared by both sides and every file of one invocation.
pub struct HomeScanner {
    rules: Vec<PreparedRule>,
    severity: HomeSeverity,
}

impl HomeScanner {
    /// Prepare validated configuration once; malformed programmatic rules are skipped.
    pub fn new(rules: &[HomeRule], severity: HomeSeverity) -> Self {
        Self {
            rules: rules
                .iter()
                .filter_map(|rule| {
                    let mut rule = rule.clone();
                    let mut patterns = std::collections::HashSet::new();
                    rule.patterns.retain(|p| patterns.insert(p.clone()));
                    Some(PreparedRule {
                        home: path_globs(&rule.home).ok()?,
                        scope: path_globs(&rule.scope).ok()?,
                        rule,
                    })
                })
                .collect(),
            severity,
        }
    }

    /// Match case-sensitive substrings in code, strings, and comments, once per pattern/line.
    pub fn scan(&self, path: &str, text: &str) -> Vec<HomeOccurrence> {
        let mut out = Vec::new();
        for (rule_index, prepared) in self.rules.iter().enumerate() {
            if !prepared.eligible(path) {
                continue;
            }
            for (line, text) in text.lines().enumerate() {
                for (pattern_index, pattern) in prepared.rule.patterns.iter().enumerate() {
                    if text.contains(pattern) {
                        out.push(HomeOccurrence {
                            rule: rule_index,
                            pattern: pattern_index,
                            line: line as u32 + 1,
                            text: text.split_whitespace().collect::<Vec<_>>().join(" "),
                        });
                    }
                }
            }
        }
        out
    }

    /// Subtract the base multiset, grouping all surplus rule/pattern matches by head line.
    pub fn introduced(
        &self,
        path: &str,
        head: &[HomeOccurrence],
        base: &[HomeOccurrence],
    ) -> Vec<Violation> {
        self.introduced_many(&[(path.to_string(), head.to_vec(), base.to_vec())])
    }

    /// Subtract per-file baselines, then cancel moves across the checked file set.
    /// Surplus matches consume removed occurrences in deterministic path/line order.
    pub fn introduced_many(
        &self,
        files: &[(String, Vec<HomeOccurrence>, Vec<HomeOccurrence>)],
    ) -> Vec<Violation> {
        let mut pool = HashMap::new();
        let mut surpluses = BTreeMap::new();
        for (path, head, base) in files {
            let mut counts = HashMap::new();
            for occurrence in base {
                *counts
                    .entry((occurrence.rule, occurrence.pattern, occurrence.text.clone()))
                    .or_insert(0usize) += 1;
            }
            let mut surplus = Vec::new();
            for occurrence in head {
                let count = counts
                    .entry((occurrence.rule, occurrence.pattern, occurrence.text.clone()))
                    .or_default();
                if *count > 0 {
                    *count -= 1;
                } else {
                    surplus.push(occurrence.clone());
                }
            }
            for (key, count) in counts {
                *pool.entry(key).or_insert(0usize) += count;
            }
            surplus.sort_by_key(|o| (o.line, o.rule, o.pattern));
            surpluses.insert(path, surplus);
        }
        let mut out = Vec::new();
        for (path, surplus) in surpluses {
            let head = surplus
                .into_iter()
                .filter(|o| {
                    let count = pool.entry((o.rule, o.pattern, o.text.clone())).or_default();
                    if *count == 0 {
                        true
                    } else {
                        *count -= 1;
                        false
                    }
                })
                .collect::<Vec<_>>();
            out.extend(self.report(path, &head));
        }
        out
    }

    fn report(&self, path: &str, head: &[HomeOccurrence]) -> Vec<Violation> {
        let mut lines = BTreeMap::<u32, Vec<String>>::new();
        for occurrence in head {
            let rule = &self.rules[occurrence.rule].rule;
            let home = if rule.home.is_empty() {
                "no permitted home".into()
            } else {
                rule.home.join(", ")
            };
            lines.entry(occurrence.line).or_default().push(format!(
                "{}: {:?} (home: {})",
                rule.name, rule.patterns[occurrence.pattern], home
            ));
        }
        lines.into_iter().map(|(line, matches)| Violation {
            code: if self.severity == HomeSeverity::Error { "E007" } else { "W011" }.into(),
            severity: if self.severity == HomeSeverity::Error { "ERROR" } else { "WARNING" }.into(),
            category: "home_violation".into(),
            message: format!("Expression outside its home: {}", matches.join("; ")),
            file: path.into(), line, hash: String::new(), confidence: 1.0,
            resolution_tier: "tier1".into(),
            fix_hint: Some(format!("At {path}:{line}, route through the home instead of re-spelling {}; remove the expression when no home is permitted.", matches.join("; "))),
            suppressed: false, suppress_hint: None, affected: vec![], suggested_module: None, existing: None,
        }).collect()
    }
}

#[cfg(test)]
#[path = "violations_homes_tests.rs"]
mod tests;
