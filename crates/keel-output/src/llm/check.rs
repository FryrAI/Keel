use keel_enforce::types::CheckResult;

pub fn format_check(result: &CheckResult) -> String {
    let r = &result.risk;
    let mut out = format!(
        "CHECK hash={} name={} file={}:{}-{} RISK={} HEALTH={}\n",
        result.target.hash, result.target.name, result.target.file,
        result.target.line_start, result.target.line_end,
        r.level.to_uppercase(), r.health.to_uppercase(),
    );

    out.push_str(&format!(
        "CALLERS total={} cross_file={} cross_module={}\n",
        r.caller_count, r.cross_file_callers, r.cross_module_callers,
    ));
    if let Some(ref summary) = r.caller_summary {
        out.push_str(&format!("  SUMMARY: {}\n", summary));
        for c in r.callers.iter().take(5) {
            out.push_str(&format!("  {}@{}:{}\n", c.hash, c.file, c.line));
        }
        if r.callers.len() > 5 {
            out.push_str(&format!("  ... and {} more (use --verbose for full list)\n", r.callers.len() - 5));
        }
    } else {
        for c in &r.callers {
            out.push_str(&format!("  {}@{}:{}\n", c.hash, c.file, c.line));
        }
    }

    out.push_str(&format!(
        "CALLEES total={} local={}\n",
        r.callee_count, r.local_callees,
    ));
    for c in &r.callees {
        out.push_str(&format!("  {}@{}:{}\n", c.hash, c.file, c.line));
    }

    if r.is_public_api {
        out.push_str("PUBLIC_API=true\n");
    }

    if !result.violations.is_empty() {
        out.push_str(&format!("VIOLATIONS count={}\n", result.violations.len()));
        for v in &result.violations {
            out.push_str(&format!("  {} {} hash={}", v.code, v.category, v.hash));
            if let Some(fix) = &v.fix_hint {
                out.push_str(&format!(" FIX: {}", fix));
            }
            out.push('\n');
        }
    }

    if !result.suggestions.is_empty() {
        out.push_str("SUGGESTIONS\n");
        for s in &result.suggestions {
            out.push_str(&format!("  [{}] {}", s.kind, s.message));
            if let Some(ref h) = s.related_hash {
                out.push_str(&format!(" ref={}", h));
            }
            out.push('\n');
        }
    }

    if !result.module_context.module.is_empty() {
        out.push_str(&format!(
            "MODULE {} fns={}\n",
            result.module_context.module, result.module_context.function_count,
        ));
    }

    out
}
