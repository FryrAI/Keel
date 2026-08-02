//! Purpose: turn a bag of audit findings into a list ordered by what a human
//! should fix first, and drop the duplicates that scan order used to hide.
//!
//! Related: audit/mod.rs (calls both on the way out), types/audit.rs
//! (`score_from_counts`), keel-output llm/audit.rs + radar.rs (renderers).
//!
//! Scan order is not review order. An audit that opens with the third
//! `function_size` warning on a test helper and buries the one FAIL that
//! actually moves a dimension score teaches the reader to stop reading.

use std::cmp::Reverse;
use std::collections::HashSet;

use crate::types::{score_from_counts, AuditDimension, AuditFinding, AuditResult, AuditSeverity};

/// Dimension whose findings float above per-file smells at equal severity.
///
/// A stale 1,500-line CLAUDE.md silently degrades every agent session in the
/// repo; a long function degrades one file. Both are FAILs, and only one of
/// them is worth reading first.
const FLOATED_DIMENSION: &str = "config";

/// Drop findings that repeat an earlier one, keyed on check + file + message.
///
/// The same module can be visited more than once in a dimension's walk (a file
/// with several module nodes, an overlapping `--changed` set), and the audit
/// used to emit the finding once per visit. The message carries the symbol, so
/// check+file+message is the rule+file+symbol identity.
///
/// Must run *before* scoring: duplicate FAILs inflate the FAIL count and
/// therefore depress the dimension score.
pub fn dedup_findings(findings: &mut Vec<AuditFinding>) {
    let mut seen: HashSet<(String, String, String)> = HashSet::new();
    findings.retain(|f| {
        seen.insert((
            f.check.clone(),
            f.file.clone().unwrap_or_default(),
            f.message.clone(),
        ))
    });
}

/// Points this dimension would regain if this one finding were fixed.
///
/// The scoring curve is a step function of (FAIL, WARN) counts, so in a
/// dimension already holding four FAILs a fifth is worth zero points, while in
/// a dimension holding exactly one it is worth two. That difference is what
/// separates "fix this and the score moves" from "fix this and nothing
/// visible happens".
fn score_impact(fails: usize, warns: usize, severity: AuditSeverity) -> u32 {
    let current = score_from_counts(fails, warns);
    let without = match severity {
        AuditSeverity::Fail => score_from_counts(fails.saturating_sub(1), warns),
        AuditSeverity::Warn => score_from_counts(fails, warns.saturating_sub(1)),
        // Tips and passes are not scored, so removing one changes nothing.
        AuditSeverity::Tip | AuditSeverity::Pass => current,
    };
    without.saturating_sub(current)
}

/// FAIL and WARN counts for a dimension, computed once per dimension.
fn severity_counts(dim: &AuditDimension) -> (usize, usize) {
    let fails = dim
        .findings
        .iter()
        .filter(|f| f.severity == AuditSeverity::Fail)
        .count();
    let warns = dim
        .findings
        .iter()
        .filter(|f| f.severity == AuditSeverity::Warn)
        .count();
    (fails, warns)
}

/// Sort key: worst severity first, then the floated dimension, then
/// score impact, then aggregate count, then a stable name/message tiebreak.
type RankKey<'a> = (
    Reverse<AuditSeverity>,
    Reverse<u8>,
    Reverse<u32>,
    Reverse<u32>,
    &'a str,
    &'a str,
);

fn rank_key<'a>(
    dim_name: &str,
    finding: &'a AuditFinding,
    fails: usize,
    warns: usize,
) -> RankKey<'a> {
    let floated = u8::from(dim_name.eq_ignore_ascii_case(FLOATED_DIMENSION));
    (
        Reverse(finding.severity),
        Reverse(floated),
        Reverse(score_impact(fails, warns, finding.severity)),
        Reverse(finding.count.unwrap_or(0)),
        finding.check.as_str(),
        finding.message.as_str(),
    )
}

/// Sort one dimension's findings in place, worst-first.
///
/// Applied by `audit_repo` so every renderer — human, JSON, LLM — sees ranked
/// findings rather than file-walk order.
pub fn sort_dimension(dim: &mut AuditDimension) {
    let (fails, warns) = severity_counts(dim);
    // Cloned so the name is not borrowed from `dim` while its findings are
    // mutably borrowed; five dimensions, so the cost is noise.
    let name = dim.name.clone();
    dim.findings
        .sort_by(|a, b| rank_key(&name, a, fails, warns).cmp(&rank_key(&name, b, fails, warns)));
}

/// All findings across all dimensions, ranked worst-first.
///
/// The flat list is what a reader actually consumes; dimension boundaries are
/// an implementation detail of scoring. Findings from the agent-config
/// dimension float above per-file smells of the same severity.
pub fn ranked_findings(result: &AuditResult) -> Vec<&AuditFinding> {
    let mut all: Vec<(RankKey<'_>, &AuditFinding)> = Vec::new();
    for dim in &result.dimensions {
        let (fails, warns) = severity_counts(dim);
        for finding in &dim.findings {
            all.push((rank_key(&dim.name, finding, fails, warns), finding));
        }
    }
    all.sort_by(|a, b| a.0.cmp(&b.0));
    all.into_iter().map(|(_, f)| f).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AuditResult;

    fn finding(severity: AuditSeverity, check: &str, message: &str) -> AuditFinding {
        AuditFinding {
            severity,
            check: check.into(),
            message: message.into(),
            tip: None,
            file: None,
            count: None,
        }
    }

    fn dim(name: &str, findings: Vec<AuditFinding>) -> AuditDimension {
        AuditDimension {
            name: name.into(),
            score: 0,
            max_score: 5,
            findings,
        }
    }

    #[test]
    fn dedup_drops_repeats_of_rule_file_symbol() {
        let mut findings = vec![
            AuditFinding {
                file: Some("src/a.rs".into()),
                ..finding(AuditSeverity::Warn, "god_file", "src/a.rs — 21 symbols")
            },
            AuditFinding {
                file: Some("src/a.rs".into()),
                ..finding(AuditSeverity::Warn, "god_file", "src/a.rs — 21 symbols")
            },
            AuditFinding {
                file: Some("src/b.rs".into()),
                ..finding(AuditSeverity::Warn, "god_file", "src/b.rs — 21 symbols")
            },
        ];
        dedup_findings(&mut findings);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[1].file.as_deref(), Some("src/b.rs"));
    }

    #[test]
    fn severity_beats_everything() {
        let result = AuditResult {
            version: "0".into(),
            command: "audit".into(),
            total_score: 0,
            max_score: 25,
            dimensions: vec![dim(
                "structure",
                vec![
                    finding(AuditSeverity::Tip, "t", "tip"),
                    finding(AuditSeverity::Warn, "w", "warn"),
                    finding(AuditSeverity::Fail, "f", "fail"),
                ],
            )],
        };
        let ranked = ranked_findings(&result);
        assert_eq!(ranked[0].check, "f");
        assert_eq!(ranked[1].check, "w");
        assert_eq!(ranked[2].check, "t");
    }

    #[test]
    fn config_findings_float_above_per_file_smells() {
        let result = AuditResult {
            version: "0".into(),
            command: "audit".into(),
            total_score: 0,
            max_score: 25,
            dimensions: vec![
                dim(
                    "structure",
                    (0..30)
                        .map(|i| finding(AuditSeverity::Fail, "god_file", &format!("file{i}")))
                        .collect(),
                ),
                dim(
                    "config",
                    vec![finding(
                        AuditSeverity::Fail,
                        "agent_instructions_size",
                        "CLAUDE.md — 1548 lines",
                    )],
                ),
            ],
        };
        let ranked = ranked_findings(&result);
        assert_eq!(ranked[0].check, "agent_instructions_size");
    }

    #[test]
    fn score_impact_orders_within_a_severity() {
        // "structure" holds many FAILs (fixing one moves nothing);
        // "navigation" holds exactly one (fixing it is worth two points).
        let result = AuditResult {
            version: "0".into(),
            command: "audit".into(),
            total_score: 0,
            max_score: 25,
            dimensions: vec![
                dim(
                    "structure",
                    (0..6)
                        .map(|i| finding(AuditSeverity::Fail, "file_size", &format!("file{i}")))
                        .collect(),
                ),
                dim(
                    "navigation",
                    vec![finding(AuditSeverity::Fail, "unstable_api", "one fail")],
                ),
            ],
        };
        let ranked = ranked_findings(&result);
        assert_eq!(ranked[0].check, "unstable_api");
    }

    #[test]
    fn aggregate_count_breaks_remaining_ties() {
        let mut a = finding(AuditSeverity::Warn, "missing_file_header", "headers");
        a.count = Some(3);
        let mut b = finding(AuditSeverity::Warn, "missing_docstrings", "docstrings");
        b.count = Some(40);
        let result = AuditResult {
            version: "0".into(),
            command: "audit".into(),
            total_score: 0,
            max_score: 25,
            dimensions: vec![dim("discoverability", vec![a, b])],
        };
        let ranked = ranked_findings(&result);
        assert_eq!(ranked[0].check, "missing_docstrings");
    }

    #[test]
    fn sort_dimension_matches_flat_ranking_within_a_dimension() {
        let mut d = dim(
            "structure",
            vec![
                finding(AuditSeverity::Warn, "god_file", "b"),
                finding(AuditSeverity::Fail, "file_size", "a"),
            ],
        );
        sort_dimension(&mut d);
        assert_eq!(d.findings[0].check, "file_size");
    }
}
