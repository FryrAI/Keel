use keel_enforce::types::AuditResult;

/// Token-efficient audit output for LLM agents.
pub fn format_audit(result: &AuditResult) -> String {
    let dim_scores: Vec<String> = result
        .dimensions
        .iter()
        .map(|d| format!("{}:{}", d.name, d.score))
        .collect();

    let mut out = format!(
        "audit:{}/{} {}\n",
        result.total_score,
        result.max_score,
        dim_scores.join(" "),
    );

    for dim in &result.dimensions {
        for f in &dim.findings {
            let severity = f.severity.to_string();
            let mut line = format!("{}:{} {}", severity, f.check, f.message);
            if let Some(ref file) = f.file {
                line.push_str(&format!(" {}", file));
            }
            if let Some(count) = f.count {
                line.push_str(&format!(" count={}", count));
            }
            out.push_str(&line);
            out.push('\n');
        }
    }

    out
}
