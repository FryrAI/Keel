//! Human-readable rendering for `keel review`.
//!
//! Split out of `human.rs` (already over its size budget) and mirroring
//! `llm/review.rs`, so the two review renderings sit side by side. Both lead
//! with the shared `keel_enforce::review::render` headline — that is what keeps
//! the interfaces from disagreeing about which change a reviewer meets first.

use keel_enforce::review::{render, ReviewResult};

/// Render a review for a terminal reader.
///
/// Returns the empty string when the review has nothing to say, honoring
/// keel's clean-output contract.
pub fn format_review_human(result: &ReviewResult) -> String {
    if render::is_silent(result) {
        return String::new();
    }

    let mut out = match render::headline(result) {
        Some(headline) => format!("{}\n", headline),
        None => "No contract changes.\n".to_string(),
    };
    out.push_str(&format!(
        "{} (vs {}, {} resolution)\n",
        render::counts_line(result),
        result.base,
        result.resolution,
    ));

    let mut listed = false;
    for change in render::contract_changes(result) {
        if !listed {
            out.push_str("\nContract changes:\n");
            listed = true;
        }
        out.push_str(&format!(
            "  [{}] {} in {}\n",
            change.kind.label(),
            change.name,
            change.file,
        ));
        if let (Some(base), Some(head)) = (&change.sig_base, &change.sig_head) {
            if base != head {
                out.push_str(&format!("    base: {}\n    head: {}\n", base, head));
            }
        }
        if change.callers_outside_diff_count > 0 {
            out.push_str(&format!(
                "    {} caller(s) outside the diff:\n",
                change.callers_outside_diff_count,
            ));
            for caller in &change.callers_outside_diff {
                out.push_str(&format!(
                    "      {} at {}:{}\n",
                    caller.name, caller.file, caller.line
                ));
            }
        }
    }

    if !result.unanalyzed.is_empty() {
        out.push_str("\nUNANALYZED (keel has no grammar for these):\n");
        for file in &result.unanalyzed {
            out.push_str(&format!("  {} [{}]\n", file.path, file.class));
        }
    }

    out
}
