use keel_enforce::types::Violation;

/// Format a single violation in LLM-compact format.
pub fn format_violation_llm(v: &Violation) -> String {
    let mut out = format!(
        "{} {} hash={} conf={:.2} tier={}",
        v.code, v.category, v.hash, v.confidence, v.resolution_tier,
    );

    if !v.affected.is_empty() {
        out.push_str(&format!(" callers={}", v.affected.len()));
    }

    out.push('\n');

    if let Some(fix) = &v.fix_hint {
        out.push_str(&format!("  FIX: {}\n", fix));
    }

    if !v.affected.is_empty() {
        let affected_strs: Vec<String> = v
            .affected
            .iter()
            .map(|a| format!("{}@{}:{}", a.hash, a.file, a.line))
            .collect();
        out.push_str(&format!("  AFFECTED: {}\n", affected_strs.join(" ")));
    }

    out
}

/// Violation priority for sorting (lower = higher priority).
pub fn violation_priority(code: &str) -> u32 {
    match code {
        "E004" => 0, // function_removed — most critical
        "E001" => 1, // broken_caller
        "E005" => 2, // arity_mismatch
        "E002" => 3, // missing_type_hints
        "E003" => 4, // missing_docstring
        "W001" => 5, // placement
        "W002" => 6, // duplicate_name
        _ => 7,
    }
}

/// Sort violations by priority (structural breaks first, cosmetic last).
pub fn sort_by_priority(violations: &mut [&Violation]) {
    violations.sort_by_key(|v| violation_priority(&v.code));
}
