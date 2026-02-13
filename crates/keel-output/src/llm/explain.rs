use keel_enforce::types::ExplainResult;

pub fn format_explain(result: &ExplainResult) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "EXPLAIN {} hash={} conf={:.2} tier={}\n",
        result.error_code, result.hash, result.confidence, result.resolution_tier,
    ));

    for step in &result.resolution_chain {
        out.push_str(&format!(
            "  {} {}:{} {}\n",
            step.kind, step.file, step.line, step.text,
        ));
    }

    out.push_str(&format!("SUMMARY {}\n", result.summary));
    out
}
