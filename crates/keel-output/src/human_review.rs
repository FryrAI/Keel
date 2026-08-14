//! Human-readable rendering for `keel review`.
//!
//! Split out of `human.rs` (already over its size budget) and mirroring
//! `llm/review.rs`, so the two review renderings sit side by side. Both lead
//! with the shared `keel_enforce::review::render` headline — that is what keeps
//! the interfaces from disagreeing about which change a reviewer meets first.

use keel_enforce::review::{render, reuse::ReuseEvidenceKind, ReviewResult};

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
    if let Some(line) = render::sprawl_line(result) {
        out.push_str(&format!("Surface growth: {line}\n"));
    }

    if !result.reuse_advisories.is_empty() {
        out.push_str("\nReuse advisories (never gating):\n");
        for advisory in &result.reuse_advisories {
            let kind = match advisory.kind {
                ReuseEvidenceKind::Replacement => "replacement",
                ReuseEvidenceKind::RoleOverlap => "role overlap",
            };
            out.push_str(&format!(
                "  [{}] {} at {}:{} may overlap {} at {}:{} ({kind}, {:.2})\n",
                advisory.code,
                advisory.new_symbol,
                advisory.new_file,
                advisory.new_line,
                advisory.existing_symbol,
                advisory.existing_file,
                advisory.existing_line,
                advisory.confidence,
            ));
            for evidence in &advisory.evidence {
                out.push_str(&format!("    evidence: {evidence}\n"));
            }
            out.push_str(&format!("    fix: {}\n", advisory.fix_hint));
        }
    }

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

    if let Some(line) = render::new_violations_line(result) {
        out.push_str(&format!("\nNEW VIOLATIONS — {}:\n", line));
        for v in &result.new_violations {
            out.push_str(&format!(
                "  [{}] {}:{} {}\n",
                v.code, v.file, v.line, v.message
            ));
            if let Some(hint) = &v.fix_hint {
                out.push_str(&format!("    fix: {}\n", hint));
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
