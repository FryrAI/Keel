//! Compact LLM formatter for `keel review`.
//!
//! Leads with the one sentence a reviewer needs, then the counts, then the
//! ranked contract changes. Body-only and doc-only deltas never get a line —
//! they exist in the payload as counts so the report can say "12 functions
//! changed, only 3 changed their contract" and then stop.

use keel_enforce::review::{render, ChangeKind, ContractChange, ReviewResult};

/// Maximum contract changes listed before collapsing into a `+N more` line.
const MAX_CHANGES: usize = 25;
/// Maximum unanalyzed paths listed before collapsing into a `+N more` line.
const MAX_UNANALYZED: usize = 15;

/// One `CONTRACT` block: the delta line, both signatures, and the callers.
fn change_block(change: &ContractChange) -> String {
    let mut out = format!(
        "CONTRACT {} {} {} callers_outside={}\n",
        change.kind.label(),
        change.name,
        change.file,
        change.callers_outside_diff_count,
    );
    if let ChangeKind::Moved { from } = &change.kind {
        out.push_str(&format!("  from: {}\n", from));
    }
    if let Some(sig) = &change.sig_base {
        out.push_str(&format!("  base: {}\n", sig));
    }
    if let Some(sig) = &change.sig_head {
        out.push_str(&format!("  head: {}\n", sig));
    }
    if !change.callers_outside_diff.is_empty() {
        let refs: Vec<String> = change
            .callers_outside_diff
            .iter()
            .map(|c| format!("{}@{}:{}", c.name, c.file, c.line))
            .collect();
        let more = change
            .callers_outside_diff_count
            .saturating_sub(change.callers_outside_diff.len());
        let suffix = if more > 0 {
            format!(" +{more} more")
        } else {
            String::new()
        };
        out.push_str(&format!("  callers: {}{}\n", refs.join(" "), suffix));
    }
    out
}

/// Render a review in the compact LLM format.
///
/// Returns the empty string when the review has nothing to say, honoring
/// keel's clean-output contract (see `keel_enforce::review::render::is_silent`).
pub fn format_review(result: &ReviewResult) -> String {
    if render::is_silent(result) {
        return String::new();
    }

    let mut out = String::new();
    if let Some(headline) = render::headline(result) {
        out.push_str(&format!("REVIEW {}\n", headline));
    } else {
        out.push_str("REVIEW no contract changes\n");
    }
    out.push_str(&format!(
        "base={} tier={} files={} analyzed={} touched={} contracts={} body_only={} doc_only={}\n",
        result.base,
        result.resolution,
        result.files_changed,
        result.files_analyzed,
        result.functions_touched,
        result.contract_change_count,
        result.body_only_count,
        result.doc_only_count,
    ));

    for change in render::contract_changes(result).take(MAX_CHANGES) {
        out.push_str(&change_block(change));
    }
    let hidden = result.contract_change_count.saturating_sub(MAX_CHANGES);
    if hidden > 0 {
        out.push_str(&format!("+{hidden} more contract change(s)\n"));
    }

    for file in result.unanalyzed.iter().take(MAX_UNANALYZED) {
        out.push_str(&format!("UNANALYZED {} [{}]\n", file.path, file.class));
    }
    let hidden = result.unanalyzed.len().saturating_sub(MAX_UNANALYZED);
    if hidden > 0 {
        out.push_str(&format!("+{hidden} more unanalyzed file(s)\n"));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::human::HumanFormatter;
    use crate::json::JsonFormatter;
    use crate::OutputFormatter;
    use keel_enforce::checkpoint::CallerRef;

    fn sig_change() -> ContractChange {
        ContractChange {
            name: "execute".into(),
            symbol_kind: keel_enforce::review::NodeKind::Function,
            file: "crates/core/src/commands/mod.rs".into(),
            kind: ChangeKind::SignatureChanged,
            sig_base: Some("fn execute(cmd: &Command)".into()),
            sig_head: Some("fn execute(cmd: &Command, dry_run: bool)".into()),
            hash_base: Some("aaaaaaaaaaa".into()),
            hash_head: Some("bbbbbbbbbbb".into()),
            is_public: true,
            callers_outside_diff: vec![CallerRef {
                name: "main".into(),
                file: "src/main.rs".into(),
                line: 12,
            }],
            callers_outside_diff_count: 7,
        }
    }

    fn result(changes: Vec<ContractChange>) -> ReviewResult {
        let contract_change_count = changes
            .iter()
            .filter(|c| c.kind.is_contract_change())
            .count();
        ReviewResult {
            version: "0.0.0".into(),
            command: "review".into(),
            base: "main".into(),
            resolution: "tier1".into(),
            files_changed: 12,
            files_analyzed: 11,
            functions_touched: changes.len(),
            contract_change_count,
            body_only_count: 9,
            doc_only_count: 0,
            changes,
            unanalyzed: Vec::new(),
        }
    }

    #[test]
    fn llm_leads_with_the_headline_and_caps_the_caller_list() {
        let out = format_review(&result(vec![sig_change()]));
        let first = out.lines().next().unwrap();
        assert!(first.starts_with("REVIEW execute()"), "{first}");
        assert!(first.contains("7 caller(s) outside the diff"));
        assert!(out.contains("CONTRACT signature_changed execute"));
        // One caller shown, six accounted for.
        assert!(
            out.contains("callers: main@src/main.rs:12 +6 more"),
            "{out}"
        );
    }

    #[test]
    fn llm_and_human_are_both_silent_on_a_body_only_review() {
        let mut change = sig_change();
        change.kind = ChangeKind::BodyOnly;
        change.callers_outside_diff = Vec::new();
        change.callers_outside_diff_count = 0;
        let r = result(vec![change]);
        assert_eq!(format_review(&r), "");
        assert_eq!(HumanFormatter.format_review(&r), "");
    }

    #[test]
    fn json_is_never_silent_and_carries_the_moved_payload() {
        let mut change = sig_change();
        change.kind = ChangeKind::Moved {
            from: "src/old.rs".into(),
        };
        let out = JsonFormatter.format_review(&result(vec![change]));
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["command"], "review");
        assert_eq!(parsed["changes"][0]["kind"], "moved");
        assert_eq!(parsed["changes"][0]["from"], "src/old.rs");
    }

    #[test]
    fn human_names_both_signatures_and_the_unanalyzed_files() {
        let mut r = result(vec![sig_change()]);
        r.unanalyzed.push(keel_enforce::review::UnanalyzedFile {
            path: "migrations/001.sql".into(),
            class: "data".into(),
        });
        let out = HumanFormatter.format_review(&r);
        assert!(out.starts_with("execute() signature changed"), "{out}");
        assert!(out.contains("base: fn execute(cmd: &Command)"));
        assert!(out.contains("head: fn execute(cmd: &Command, dry_run: bool)"));
        assert!(out.contains("UNANALYZED"));
        assert!(out.contains("migrations/001.sql [data]"));
    }
}
