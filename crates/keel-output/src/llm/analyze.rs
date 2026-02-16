use keel_enforce::types::AnalyzeResult;

pub fn format_analyze(result: &AnalyzeResult) -> String {
    let s = &result.structure;
    let mut out = format!(
        "ANALYZE file={} lines={} fns={} classes={}\n",
        result.file, s.line_count, s.function_count, s.class_count,
    );

    out.push_str("STRUCTURE\n");
    for f in &s.functions {
        out.push_str(&format!(
            "  fn {} hash={} lines={}-{} ({}) callers={} callees={}{}\n",
            f.name, f.hash, f.line_start, f.line_end, f.lines,
            f.callers, f.callees,
            if f.is_public { " PUB" } else { "" },
        ));
    }
    for c in &s.classes {
        out.push_str(&format!(
            "  class {} hash={} lines={}-{} ({}) callers={} callees={}{}\n",
            c.name, c.hash, c.line_start, c.line_end, c.lines,
            c.callers, c.callees,
            if c.is_public { " PUB" } else { "" },
        ));
    }

    if !result.smells.is_empty() {
        out.push_str(&format!("SMELLS count={}\n", result.smells.len()));
        for smell in &result.smells {
            out.push_str(&format!("  [{}] {}\n", smell.severity, smell.message));
        }
    }

    if !result.refactor_opportunities.is_empty() {
        out.push_str(&format!("REFACTOR count={}\n", result.refactor_opportunities.len()));
        for r in &result.refactor_opportunities {
            out.push_str(&format!("  {:?}: {}\n", r.kind, r.message));
        }
    }

    out
}
