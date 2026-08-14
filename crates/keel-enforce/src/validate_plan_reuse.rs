//! Advisory reuse candidates for symbols a plan proposes to create.

use std::collections::HashSet;

use keel_core::store::GraphStore;

use crate::naming;
use crate::validate_plan_findings::PlanFinding;

const MAX_REUSE_FINDINGS: usize = 5;
const MIN_REUSE_SCORE: f64 = 0.55;
const CREATION_WORDS: &[&str] = &[
    "add",
    "create",
    "introduce",
    "implement",
    "define",
    "extract",
    "write",
    "build",
];
const DECLARATION_WORDS: &[&str] = &["def", "fn", "func", "function"];
const SKIPPED_WORDS: &[&str] = &["a", "an", "the", "new", "function", "helper", "method"];

/// Find high-confidence existing candidates for explicitly proposed functions.
pub(crate) fn detect_reuse_findings(store: &dyn GraphStore, plan: &str) -> Vec<PlanFinding> {
    let mut findings = Vec::new();
    for proposed in creation_requests(plan) {
        let normalized = proposed.replace('_', " ");
        let Some(candidate) = naming::reuse_candidates(store, &normalized)
            .into_iter()
            .find(|candidate| candidate.score >= MIN_REUSE_SCORE)
        else {
            continue;
        };
        findings.push(PlanFinding {
            code: "P003".to_string(),
            severity: "WARNING".to_string(),
            category: "reuse_candidate".to_string(),
            symbol: proposed.clone(),
            message: format!(
                "Plan proposes `{proposed}`, but existing `{}` may already satisfy that intent",
                candidate.name
            ),
            hash: candidate.hash.clone(),
            file: candidate.file.clone(),
            line: candidate.line,
            claimed: format!("create {proposed}"),
            actual: Some(candidate.signature.clone()),
            fix_hint: format!(
                "Run `keel discover {}` and reuse `{}` if its behavior fits; otherwise keep `{proposed}` and state the semantic difference. P003 is advisory and never fails --strict.",
                candidate.hash, candidate.name
            ),
            confidence: candidate.score as f32,
            downgraded: false,
        });
        if findings.len() >= MAX_REUSE_FINDINGS {
            break;
        }
    }
    findings
}

/// High-precision proposed function names.
///
/// The creation verb must directly introduce the identifier (allowing only
/// articles and generic words such as `helper`). This avoids reading "add a
/// wrapper using existing_call()" as a proposal to create `existing_call`.
fn creation_requests(plan: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for line in plan.lines() {
        let words: Vec<&str> = line
            .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .filter(|word| !word.is_empty())
            .collect();
        for (index, word) in words.iter().enumerate() {
            let mut next = index + 1;
            let word_lower = word.to_ascii_lowercase();
            let declares = DECLARATION_WORDS.contains(&word_lower.as_str());
            if !declares && !CREATION_WORDS.contains(&word_lower.as_str()) {
                continue;
            }
            while next < words.len()
                && SKIPPED_WORDS.contains(&words[next].to_ascii_lowercase().as_str())
            {
                next += 1;
            }
            let Some(name) = words.get(next) else {
                continue;
            };
            let explicit_call = line.contains(&format!("{name}("));
            if (declares || explicit_call) && name.len() >= 3 && seen.insert(*name) {
                out.push((*name).to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::creation_requests;

    #[test]
    fn creation_request_requires_the_verb_to_introduce_the_name() {
        assert_eq!(
            creation_requests("Add a new parse_time(value) helper."),
            vec!["parse_time".to_string()]
        );
        assert!(creation_requests("Add a wrapper using parse_time(value).").is_empty());
        assert_eq!(
            creation_requests("Implement parseTime(value)."),
            vec!["parseTime".to_string()]
        );
    }
}
