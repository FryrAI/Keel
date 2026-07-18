use keel_enforce::types::{AuditFinding, AuditResult};

use crate::token_budget;

/// Token-efficient audit output for LLM agents.
///
/// Findings are sorted worst-first (FAIL, then WARN, then TIP, then PASS) and
/// truncated to `max_tokens` so a large audit doesn't drown the summary line
/// in low-priority findings — mirrors how `compile` output is budgeted.
pub fn format_audit(result: &AuditResult, max_tokens: usize) -> String {
    let dim_scores: Vec<String> = result
        .dimensions
        .iter()
        .map(|d| format!("{}:{}", d.name, d.score))
        .collect();

    let header = format!(
        "audit:{}/{} {}\n",
        result.total_score,
        result.max_score,
        dim_scores.join(" "),
    );

    let mut findings: Vec<&AuditFinding> = result
        .dimensions
        .iter()
        .flat_map(|d| d.findings.iter())
        .collect();
    findings.sort_by_key(|f| std::cmp::Reverse(f.severity));

    let lines: Vec<String> = findings.iter().map(|f| format_finding(f)).collect();
    // The header spends part of the budget too — findings get what's left.
    let line_budget = max_tokens.saturating_sub(token_budget::estimate_tokens(&header));
    let (kept, overflow) = token_budget::truncate_to_budget(&lines, line_budget);

    let mut out = header;
    for line in &kept {
        out.push_str(line);
        out.push('\n');
    }
    if overflow > 0 {
        out.push_str(&format!(
            "... +{} more finding(s) (raise --max-tokens for full list)\n",
            overflow
        ));
    }

    out
}

/// Format a single finding as one line.
fn format_finding(f: &AuditFinding) -> String {
    let mut line = format!("{}:{} {}", f.severity, f.check, f.message);
    if let Some(ref file) = f.file {
        line.push_str(&format!(" {}", file));
    }
    if let Some(ref tip) = f.tip {
        line.push_str(&format!(" tip={}", tip));
    }
    if let Some(count) = f.count {
        line.push_str(&format!(" count={}", count));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_enforce::types::{AuditDimension, AuditSeverity};

    fn finding(severity: AuditSeverity, check: &str, message: &str) -> AuditFinding {
        AuditFinding {
            severity,
            check: check.to_string(),
            message: message.to_string(),
            tip: None,
            file: None,
            count: None,
        }
    }

    fn result_with(findings: Vec<AuditFinding>) -> AuditResult {
        AuditResult {
            version: "0.0.0".into(),
            command: "audit".into(),
            total_score: 10,
            max_score: 25,
            dimensions: vec![AuditDimension {
                name: "structure".into(),
                score: 2,
                max_score: 5,
                findings,
            }],
        }
    }

    #[test]
    fn test_sorts_worst_first() {
        let result = result_with(vec![
            finding(AuditSeverity::Pass, "p", "all good"),
            finding(AuditSeverity::Fail, "f", "broken"),
            finding(AuditSeverity::Tip, "t", "consider this"),
            finding(AuditSeverity::Warn, "w", "watch out"),
        ]);
        let out = format_audit(&result, 5000);
        let fail_pos = out.find("FAIL:f").unwrap();
        let warn_pos = out.find("WARN:w").unwrap();
        let tip_pos = out.find("TIP:t").unwrap();
        let pass_pos = out.find("PASS:p").unwrap();
        assert!(fail_pos < warn_pos);
        assert!(warn_pos < tip_pos);
        assert!(tip_pos < pass_pos);
    }

    #[test]
    fn test_respects_max_tokens_budget() {
        let findings: Vec<AuditFinding> = (0..50)
            .map(|i| {
                finding(
                    AuditSeverity::Warn,
                    "w",
                    &format!("finding number {i} with a fairly long descriptive message"),
                )
            })
            .collect();
        let result = result_with(findings);

        let small = format_audit(&result, 20);
        let large = format_audit(&result, 5000);

        assert!(
            small.lines().count() < large.lines().count(),
            "small budget should truncate more lines than a large one"
        );
        assert!(
            small.contains("more finding(s)"),
            "truncated output should say how much was dropped"
        );
    }

    #[test]
    fn test_no_findings_still_has_header() {
        let result = result_with(vec![]);
        let out = format_audit(&result, 500);
        assert!(out.contains("audit:10/25"));
        assert_eq!(out.lines().count(), 1);
    }
}
