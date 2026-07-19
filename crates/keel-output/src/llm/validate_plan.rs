//! Compact LLM formatter for `keel validate-plan`.

use keel_enforce::validate_plan::PlanValidationResult;

const MAX_CALLERS: usize = 8;

/// Render a plan-validation report in the compact LLM format.
pub fn format_validate_plan(result: &PlanValidationResult) -> String {
    if result.unrecognized {
        return "VALIDATE-PLAN no graph-relevant actions detected\n".to_string();
    }

    let mut out = format!(
        "VALIDATE-PLAN actions={} symbols={}\n",
        result.actions.len(),
        result.symbols_detected,
    );

    for a in &result.actions {
        out.push_str(&format!(
            "ACTION {} {} hash={} risk={} callers={}\n",
            a.action, a.symbol, a.hash, a.risk, a.caller_count,
        ));
        if !a.callers.is_empty() {
            let refs: Vec<String> = a
                .callers
                .iter()
                .take(MAX_CALLERS)
                .map(|c| format!("{}@{}:{}", c.name, c.file, c.line))
                .collect();
            let more = a.callers.len().saturating_sub(MAX_CALLERS);
            let suffix = if more > 0 {
                format!(" +{more} more")
            } else {
                String::new()
            };
            out.push_str(&format!("  callers: {}{}\n", refs.join(" "), suffix));
        }
        out.push_str(&format!("  order: {}\n", a.suggested_order));
    }

    if !result.files_detected.is_empty() {
        out.push_str(&format!("FILES: {}\n", result.files_detected.join(" ")));
    }

    out
}
