use keel_enforce::audit::ranked_findings;
use keel_enforce::types::{AuditFinding, AuditResult};

use crate::token_budget;

/// How many findings `keel audit --llm` shows before it stops.
///
/// A 1,799-line wall in scan order is not review leverage, it is review
/// defeat. Twenty ranked findings is a list a human (or an agent with a
/// context budget) will actually read to the end; `--top 0` lifts the cap.
pub const DEFAULT_TOP: usize = 20;

/// Token-efficient audit output for LLM agents.
///
/// Findings are ranked worst-first by `keel_enforce::audit::ranked_findings`
/// (severity, then agent-config over per-file smells, then dimension-score
/// impact, then count), capped at `top`, and finally truncated to `max_tokens`.
/// The trailing note always states how many findings were left out, so a capped
/// list is never mistaken for the whole story.
pub fn format_audit(result: &AuditResult, max_tokens: usize, top: usize) -> String {
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

    let findings: Vec<&AuditFinding> = ranked_findings(result);
    let total = findings.len();
    let capped = if top == 0 { total } else { top.min(total) };

    let lines: Vec<String> = findings[..capped]
        .iter()
        .map(|f| format_finding(f))
        .collect();
    // The header spends part of the budget too — findings get what's left.
    let line_budget = max_tokens.saturating_sub(token_budget::estimate_tokens(&header));
    let (kept, _) = token_budget::truncate_to_budget(&lines, line_budget);

    let mut out = header;
    for line in &kept {
        out.push_str(line);
        out.push('\n');
    }
    let omitted = total.saturating_sub(kept.len());
    if omitted > 0 {
        out.push_str(&format!(
            "... +{} more finding(s) omitted (showing top {} of {}; \
             --top 0 or --json for the full list)\n",
            omitted,
            kept.len(),
            total,
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
        let out = format_audit(&result, 5000, DEFAULT_TOP);
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

        let small = format_audit(&result, 20, 0);
        let large = format_audit(&result, 5000, 0);

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
        let out = format_audit(&result, 500, DEFAULT_TOP);
        assert!(out.contains("audit:10/25"));
        assert_eq!(out.lines().count(), 1);
    }

    #[test]
    fn test_caps_at_top_and_reports_omissions() {
        let findings: Vec<AuditFinding> = (0..50)
            .map(|i| finding(AuditSeverity::Warn, "w", &format!("finding {i}")))
            .collect();
        let result = result_with(findings);

        // Generous token budget: the cap, not the budget, is what bites.
        let out = format_audit(&result, 100_000, DEFAULT_TOP);
        let finding_lines = out.lines().filter(|l| l.starts_with("WARN:")).count();
        assert_eq!(finding_lines, DEFAULT_TOP);
        assert!(
            out.contains("+30 more finding(s) omitted (showing top 20 of 50"),
            "expected an omission note, got:\n{out}"
        );
    }

    #[test]
    fn test_top_zero_lifts_the_cap() {
        let findings: Vec<AuditFinding> = (0..50)
            .map(|i| finding(AuditSeverity::Warn, "w", &format!("finding {i}")))
            .collect();
        let result = result_with(findings);

        let out = format_audit(&result, 100_000, 0);
        assert_eq!(out.lines().filter(|l| l.starts_with("WARN:")).count(), 50);
        assert!(!out.contains("omitted"));
    }
}
