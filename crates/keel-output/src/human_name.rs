//! Human rendering for reuse-first `keel name` results.

use keel_enforce::types::NameResult;

/// Render existing reuse candidates before any create-new naming suggestion.
pub(crate) fn render_name(result: &NameResult) -> String {
    if result.suggestions.is_empty() && result.reuse_candidates.is_empty() {
        return format!("No naming suggestions for \"{}\".\n", result.description);
    }
    let mut out = format!("Naming suggestion for \"{}\"\n\n", result.description);
    for candidate in &result.reuse_candidates {
        out.push_str(&format!(
            "  Reuse? {} at {}:{} ({}, score: {:.0}%, callers: {}, callees: {})\n",
            candidate.name,
            candidate.file,
            candidate.line,
            candidate.source,
            candidate.score * 100.0,
            candidate.callers,
            candidate.callees,
        ));
        out.push_str(&format!("    Signature: {}\n", candidate.signature));
        if !candidate.evidence.is_empty() {
            out.push_str(&format!(
                "    Evidence: {}\n",
                candidate.evidence.join("; ")
            ));
        }
    }
    let Some(best) = result.suggestions.first() else {
        return out;
    };
    out.push_str(&format!(
        "  Location: {} (score: {:.0}%)\n",
        best.location,
        best.score * 100.0,
    ));
    out.push_str(&format!("  Suggested name: {}\n", best.suggested_name));
    out.push_str(&format!("  Convention: {}\n", best.convention));
    if let Some(after) = &best.insert_after {
        out.push_str(&format!("  Insert after: {}\n", after));
    }
    if !best.siblings.is_empty() {
        out.push_str(&format!("  Siblings: {}\n", best.siblings.join(", ")));
    }
    out
}
