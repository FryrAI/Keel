//! Compact LLM formatter for `keel checkpoint`.
//!
//! Kept small (typical diffs render well under ~1-2KB) by capping the number
//! of symbols, callers, violations, and commits printed.

use keel_enforce::checkpoint::CheckpointResult;

const MAX_SYMBOLS: usize = 12;
const MAX_CALLERS: usize = 6;
const MAX_VIOLATIONS: usize = 12;
const MAX_COMMITS: usize = 10;

/// Render a checkpoint in the compact LLM format.
pub fn format_checkpoint(result: &CheckpointResult) -> String {
    let mut out = format!(
        "CHECKPOINT range={} files={} callers_at_risk={} errors={} warnings={}\n",
        result.range,
        result.files.len(),
        result.affected_callers.len(),
        result.error_count,
        result.warning_count,
    );

    for fd in &result.files {
        out.push_str(&format!(
            "FILE {} +{} ~{} -{}\n",
            fd.file,
            fd.added.len(),
            fd.changed.len(),
            fd.removed.len(),
        ));
        push_symbols(&mut out, "+", &fd.added);
        push_symbols(&mut out, "~", &fd.changed);
        push_symbols(&mut out, "-", &fd.removed);
    }

    for ac in &result.affected_callers {
        let refs: Vec<String> = ac
            .callers
            .iter()
            .take(MAX_CALLERS)
            .map(|c| format!("{}@{}:{}", c.name, c.file, c.line))
            .collect();
        let more = ac.callers.len().saturating_sub(MAX_CALLERS);
        let suffix = if more > 0 {
            format!(" +{more} more")
        } else {
            String::new()
        };
        out.push_str(&format!(
            "RISK {}: {}{}\n",
            ac.symbol,
            refs.join(" "),
            suffix
        ));
    }

    if !result.violations.is_empty() {
        out.push_str("VIOLATIONS:\n");
        for v in result.violations.iter().take(MAX_VIOLATIONS) {
            out.push_str(&format!(
                "  {} {} {}:{} {}\n",
                v.code, v.severity, v.file, v.line, v.message
            ));
        }
        let more = result.violations.len().saturating_sub(MAX_VIOLATIONS);
        if more > 0 {
            out.push_str(&format!("  ... +{more} more\n"));
        }
    }

    if !result.commits.is_empty() {
        out.push_str("COMMITS:\n");
        for c in result.commits.iter().take(MAX_COMMITS) {
            out.push_str(&format!("  {c}\n"));
        }
    }

    out
}

fn push_symbols(out: &mut String, marker: &str, syms: &[keel_enforce::checkpoint::SymbolRef]) {
    for s in syms.iter().take(MAX_SYMBOLS) {
        out.push_str(&format!("  {} {} {}\n", marker, s.name, s.hash));
    }
    let more = syms.len().saturating_sub(MAX_SYMBOLS);
    if more > 0 {
        out.push_str(&format!("  {marker} ... +{more} more\n"));
    }
}
