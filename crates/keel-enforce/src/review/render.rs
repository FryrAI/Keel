//! The sentences every renderer shares.
//!
//! The cover letter's value is that its *first line* is the one fact a reviewer
//! needs. Keeping that line here — rather than re-deriving it in the human and
//! LLM formatters — is what guarantees the two interfaces cannot disagree about
//! which change leads.

use keel_core::types::NodeKind;

use super::{ChangeKind, ContractChange, ReviewResult};

/// Whether the review has nothing worth printing.
///
/// Honors keel's clean-output contract: a PR that moved no contract, touched
/// no file keel cannot read, and introduced no violation prints **nothing** and
/// exits 0. Body-only and doc-only changes are, by construction, not worth a
/// reviewer's attention budget — that is the whole claim of "12 functions
/// changed, only 3 changed their contract". Violations the PR *inherited* are
/// equally not worth it: only the ones it introduced break the silence.
pub fn is_silent(result: &ReviewResult) -> bool {
    result.contract_change_count == 0
        && result.unanalyzed.is_empty()
        && result.new_violations.is_empty()
}

/// `"3 new violation(s) (41 pre-existing)"`, or `None` when the diff
/// introduced none.
pub fn new_violations_line(result: &ReviewResult) -> Option<String> {
    if result.new_violations.is_empty() {
        return None;
    }
    Some(format!(
        "{} new violation(s) ({} pre-existing)",
        result.new_violations.len(),
        result.pre_existing_violations,
    ))
}

/// The changes worth listing, already ranked.
pub fn contract_changes(result: &ReviewResult) -> impl Iterator<Item = &ContractChange> {
    result
        .changes
        .iter()
        .filter(|c| c.kind.is_contract_change())
}

/// `" — 7 caller(s) outside the diff"`, or empty when nothing was left behind.
fn callers_clause(change: &ContractChange) -> String {
    match change.callers_outside_diff_count {
        0 => String::new(),
        n => format!(" — {} caller(s) outside the diff", n),
    }
}

/// One English sentence for a single contract change.
pub fn change_sentence(change: &ContractChange) -> String {
    let what = match &change.kind {
        ChangeKind::SignatureChanged => format!("signature changed in {}", change.file),
        ChangeKind::Added => format!("added in {}", change.file),
        ChangeKind::Removed => format!("removed from {}", change.file),
        ChangeKind::Moved { from } => format!("moved to {} from {}", change.file, from),
        ChangeKind::BodyOnly => format!("body changed in {}", change.file),
        ChangeKind::DocOnly => format!("docstring changed in {}", change.file),
    };
    let subject = match change.symbol_kind {
        NodeKind::Function => format!("{}()", change.name),
        _ => change.name.clone(),
    };
    format!("{} {}{}", subject, what, callers_clause(change))
}

/// The lead line: the top-ranked contract change, or `None` when there is none.
pub fn headline(result: &ReviewResult) -> Option<String> {
    contract_changes(result).next().map(change_sentence)
}

/// The counts line — the "only 3 of 12 changed their contract" claim.
pub fn counts_line(result: &ReviewResult) -> String {
    format!(
        "{} file(s) changed, {} function(s) touched, {} changed their contract ({} body-only, {} doc-only)",
        result.files_changed,
        result.functions_touched,
        result.contract_change_count,
        result.body_only_count,
        result.doc_only_count,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::CallerRef;

    fn result(changes: Vec<ContractChange>) -> ReviewResult {
        let contract_change_count = changes
            .iter()
            .filter(|c| c.kind.is_contract_change())
            .count();
        ReviewResult {
            version: "0".into(),
            command: "review".into(),
            base: "main".into(),
            resolution: "tier1".into(),
            files_changed: 1,
            files_analyzed: 1,
            functions_touched: changes.len(),
            contract_change_count,
            body_only_count: 0,
            doc_only_count: 0,
            changes,
            unanalyzed: Vec::new(),
            new_violations: Vec::new(),
            pre_existing_violations: 0,
        }
    }

    fn sig_change(name: &str, callers: usize) -> ContractChange {
        ContractChange {
            name: name.into(),
            symbol_kind: NodeKind::Function,
            file: "crates/core/src/commands/mod.rs".into(),
            kind: ChangeKind::SignatureChanged,
            sig_base: Some("fn execute(cmd: &Command)".into()),
            sig_head: Some("fn execute(cmd: &Command, dry_run: bool)".into()),
            hash_base: Some("aaa".into()),
            hash_head: Some("bbb".into()),
            is_public: true,
            callers_outside_diff: vec![CallerRef {
                name: "main".into(),
                file: "src/main.rs".into(),
                line: 3,
            }],
            callers_outside_diff_count: callers,
        }
    }

    #[test]
    fn headline_names_the_symbol_file_and_stranded_callers() {
        let r = result(vec![sig_change("execute", 7)]);
        assert_eq!(
            headline(&r).unwrap(),
            "execute() signature changed in crates/core/src/commands/mod.rs — 7 caller(s) outside the diff"
        );
    }

    #[test]
    fn a_body_only_review_is_silent() {
        let mut c = sig_change("helper", 0);
        c.kind = ChangeKind::BodyOnly;
        c.callers_outside_diff = Vec::new();
        let r = result(vec![c]);
        assert!(is_silent(&r));
        assert!(headline(&r).is_none());
    }

    #[test]
    fn unanalyzed_files_alone_break_the_silence() {
        let mut r = result(Vec::new());
        r.unanalyzed.push(super::super::UnanalyzedFile {
            path: "migrations/001.sql".into(),
            class: "data".into(),
        });
        assert!(!is_silent(&r));
    }
}
