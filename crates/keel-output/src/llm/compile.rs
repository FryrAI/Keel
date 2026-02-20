use super::violation::{format_violation_llm, violation_priority};
use crate::token_budget;
use keel_enforce::types::{CompileDelta, CompileResult, PressureLevel, Violation};
use std::collections::BTreeMap;

/// Format compile result at depth 0: counts only (1 line).
pub fn format_compile_depth0(result: &CompileResult) -> String {
    if result.errors.is_empty() && result.warnings.is_empty() {
        return String::new();
    }
    let pressure = PressureLevel::from_error_count(result.errors.len());
    format!(
        "COMPILE files={} errors={} warnings={} PRESSURE={} BUDGET={}\n",
        result.files_analyzed.len(),
        result.errors.len(),
        result.warnings.len(),
        pressure,
        pressure.budget_directive(),
    )
}

/// Format compile result at depth 1: grouped by file with truncation.
pub fn format_compile_depth1(result: &CompileResult, max_tokens: usize) -> String {
    if result.errors.is_empty() && result.warnings.is_empty() {
        return String::new();
    }

    let pressure = PressureLevel::from_error_count(result.errors.len());
    let mut out = format!(
        "COMPILE files={} errors={} warnings={} PRESSURE={} BUDGET={}\n",
        result.files_analyzed.len(),
        result.errors.len(),
        result.warnings.len(),
        pressure,
        pressure.budget_directive(),
    );

    // Group violations by file
    let mut by_file: BTreeMap<&str, Vec<&Violation>> = BTreeMap::new();
    for v in result.errors.iter().chain(result.warnings.iter()) {
        by_file.entry(&v.file).or_default().push(v);
    }

    // Format each file group
    let mut file_lines: Vec<String> = Vec::new();
    for (file, mut violations) in by_file {
        violations.sort_by_key(|v| violation_priority(&v.code));
        let error_count = violations.iter().filter(|v| v.severity == "ERROR").count();
        let warn_count = violations.len() - error_count;
        let mut file_block = format!(
            "\nFILE {} errors={} warnings={}",
            file, error_count, warn_count
        );
        for v in &violations {
            file_block.push_str(&format!("\n  {} {} hash={}", v.code, v.category, v.hash));
            if let Some(fix) = &v.fix_hint {
                file_block.push_str(&format!(" FIX: {}", fix));
            }
        }
        file_lines.push(file_block);
    }

    // Apply token budget (configurable via --max-tokens, default 500)
    let (kept, overflow) = token_budget::truncate_to_budget(&file_lines, max_tokens);
    for line in &kept {
        out.push_str(line);
        out.push('\n');
    }
    if overflow > 0 {
        out.push_str(&format!(
            "\n... +{} more file(s) (run with --depth 2 for full list)\n",
            overflow
        ));
    }

    out
}

/// Format compile result at depth 2: full detail with affected nodes.
pub fn format_compile_depth2(result: &CompileResult) -> String {
    if result.errors.is_empty() && result.warnings.is_empty() {
        return String::new();
    }

    let pressure = PressureLevel::from_error_count(result.errors.len());
    let mut out = format!(
        "COMPILE files={} errors={} warnings={} PRESSURE={} BUDGET={}\n",
        result.files_analyzed.len(),
        result.errors.len(),
        result.warnings.len(),
        pressure,
        pressure.budget_directive(),
    );

    for v in &result.errors {
        out.push_str(&format_violation_llm(v));
    }
    for v in &result.warnings {
        out.push_str(&format_violation_llm(v));
    }

    out
}

/// Format compile at the given depth (0, 1, 2). Default = 1.
pub fn format_compile(result: &CompileResult, depth: u32, max_tokens: usize) -> String {
    match depth {
        0 => format_compile_depth0(result),
        2.. => format_compile_depth2(result),
        _ => format_compile_depth1(result, max_tokens),
    }
}

/// Format a compile delta (new vs resolved violations).
pub fn format_compile_delta(delta: &CompileDelta) -> String {
    let net_e = if delta.net_errors >= 0 {
        format!("+{}", delta.net_errors)
    } else {
        delta.net_errors.to_string()
    };
    let net_w = if delta.net_warnings >= 0 {
        format!("+{}", delta.net_warnings)
    } else {
        delta.net_warnings.to_string()
    };

    let mut out =
        format!(
        "COMPILE DELTA errors={} warnings={} NET: {} errors, {} warnings PRESSURE={} BUDGET={}\n",
        delta.total_errors, delta.total_warnings,
        net_e, net_w,
        delta.pressure, delta.pressure.budget_directive(),
    );

    for k in &delta.new_errors {
        out.push_str(&format!(
            "  +ERROR [{}] hash={} {}:{}\n",
            k.code, k.hash, k.file, k.line
        ));
    }
    for k in &delta.resolved_errors {
        out.push_str(&format!(
            "  -ERROR [{}] hash={} {}:{}\n",
            k.code, k.hash, k.file, k.line
        ));
    }
    for k in &delta.new_warnings {
        out.push_str(&format!(
            "  +WARN [{}] hash={} {}:{}\n",
            k.code, k.hash, k.file, k.line
        ));
    }
    for k in &delta.resolved_warnings {
        out.push_str(&format!(
            "  -WARN [{}] hash={} {}:{}\n",
            k.code, k.hash, k.file, k.line
        ));
    }

    if delta.new_errors.is_empty()
        && delta.resolved_errors.is_empty()
        && delta.new_warnings.is_empty()
        && delta.resolved_warnings.is_empty()
    {
        out.push_str("  (no changes)\n");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_enforce::types::*;

    fn clean_compile() -> CompileResult {
        CompileResult {
            version: env!("CARGO_PKG_VERSION").into(),
            command: "compile".into(),
            status: "ok".into(),
            files_analyzed: vec!["src/main.rs".into()],
            errors: vec![],
            warnings: vec![],
            info: CompileInfo {
                nodes_updated: 0,
                edges_updated: 0,
                hashes_changed: vec![],
            },
        }
    }

    fn make_violation(code: &str, file: &str, hash: &str) -> Violation {
        Violation {
            code: code.into(),
            severity: if code.starts_with('E') {
                "ERROR"
            } else {
                "WARNING"
            }
            .into(),
            category: "test_cat".into(),
            message: format!("Test violation {}", code),
            file: file.into(),
            line: 10,
            hash: hash.into(),
            confidence: 0.9,
            resolution_tier: "tree-sitter".into(),
            fix_hint: Some(format!("Fix {}", code)),
            suppressed: false,
            suppress_hint: None,
            affected: vec![],
            suggested_module: None,
            existing: None,
        }
    }

    #[test]
    fn test_clean_compile_all_depths() {
        let c = clean_compile();
        assert!(format_compile(&c, 0, 500).is_empty());
        assert!(format_compile(&c, 1, 500).is_empty());
        assert!(format_compile(&c, 2, 500).is_empty());
    }

    #[test]
    fn test_depth0_is_one_line() {
        let mut c = clean_compile();
        c.errors.push(make_violation("E001", "src/a.rs", "h1"));
        let out = format_compile(&c, 0, 500);
        assert_eq!(out.lines().count(), 1);
        assert!(out.contains("PRESSURE=LOW"));
        assert!(out.contains("BUDGET=keep_going"));
    }

    #[test]
    fn test_pressure_levels() {
        let mut c = clean_compile();
        // 1 error = LOW
        c.errors.push(make_violation("E001", "a.rs", "h1"));
        assert!(format_compile(&c, 0, 500).contains("PRESSURE=LOW"));

        // 4 errors = MED
        c.errors.push(make_violation("E002", "a.rs", "h2"));
        c.errors.push(make_violation("E003", "a.rs", "h3"));
        c.errors.push(make_violation("E004", "a.rs", "h4"));
        assert!(format_compile(&c, 0, 500).contains("PRESSURE=MED"));

        // 7 errors = HIGH
        c.errors.push(make_violation("E005", "a.rs", "h5"));
        c.errors.push(make_violation("E001", "b.rs", "h6"));
        c.errors.push(make_violation("E001", "c.rs", "h7"));
        assert!(format_compile(&c, 0, 500).contains("PRESSURE=HIGH"));
        assert!(format_compile(&c, 0, 500).contains("BUDGET=stop_generating"));
    }

    #[test]
    fn test_depth1_groups_by_file() {
        let mut c = clean_compile();
        c.errors.push(make_violation("E001", "src/a.rs", "h1"));
        c.errors.push(make_violation("E005", "src/a.rs", "h2"));
        c.errors.push(make_violation("E001", "src/b.rs", "h3"));
        let out = format_compile(&c, 1, 500);
        assert!(out.contains("FILE src/a.rs errors=2"));
        assert!(out.contains("FILE src/b.rs errors=1"));
    }

    #[test]
    fn test_depth2_shows_full_detail() {
        let mut c = clean_compile();
        let mut v = make_violation("E001", "src/a.rs", "h1");
        v.affected.push(AffectedNode {
            hash: "a1".into(),
            name: "caller".into(),
            file: "src/b.rs".into(),
            line: 5,
        });
        c.errors.push(v);
        let out = format_compile(&c, 2, 500);
        assert!(out.contains("AFFECTED:"));
        assert!(out.contains("a1@src/b.rs:5"));
    }
}
