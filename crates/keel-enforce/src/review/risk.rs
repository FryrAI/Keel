//! Callers left behind, and the order the reviewer should meet them in.
//!
//! Deliberately **not** a score. The v0.5 plan cuts the integer risk score,
//! `role_rank` banding, and union-find clustering from stage 1; what survives is
//! one fact per change — how many stored callers live in files this PR did not
//! touch — and a total order over that fact.

use std::collections::{HashMap, HashSet};

use keel_core::store::GraphStore;
use keel_core::types::{GraphNode, NodeKind};

use crate::checkpoint::callers_of;

use super::{ChangeKind, ContractChange};

/// How many caller references a single change lists before collapsing into a
/// `+N more` suffix. The count itself is never capped.
pub const MAX_DISPLAYED_CALLERS: usize = 5;

/// Find the stored node for a change, trying the head path then the base path.
///
/// A renamed file may sit in the graph under either name depending on whether
/// `keel map` has run since the rename, and a deletion only ever exists under
/// the base path. Lookup stays file-scoped on purpose: a repo-wide name search
/// would attach some other module's `execute` to this one.
///
/// `cache` holds one `get_nodes_in_file` result per path for the life of the
/// pass, so a PR changing forty symbols in one file reads that file once.
fn stored_node(
    store: &dyn GraphStore,
    cache: &mut HashMap<String, Vec<GraphNode>>,
    change: &ContractChange,
) -> Option<GraphNode> {
    let mut candidates = vec![change.file.as_str()];
    if let ChangeKind::Moved { from } = &change.kind {
        candidates.push(from.as_str());
    }
    candidates.into_iter().find_map(|path| {
        cache
            .entry(path.to_string())
            .or_insert_with(|| store.get_nodes_in_file(path))
            .iter()
            .find(|n| n.kind != NodeKind::Module && n.name == change.name)
            .cloned()
    })
}

/// Attach each contract change's callers that live outside `diff_files`.
///
/// **Counts `EdgeKind::Calls` only** — via `checkpoint::callers_of` — not the wider
/// `queries::is_dependency_edge` (`Calls | Uses`) set that discover, focus, and
/// search fan-in use. The headline number here answers "whose code breaks if
/// this contract moved", and that is precisely the severity question E001/E004/
/// E005 and the fix planner answer, all of which filter `Calls`. A `Uses` edge
/// — a function named as a value, a callback registration — survives a
/// signature change far more often than it breaks on one, so folding it in
/// would inflate the one number the cover letter leads with.
///
/// `Added` symbols are skipped: a symbol that did not exist on the base side
/// has no callers this PR failed to update.
pub fn attach_callers(
    store: &dyn GraphStore,
    changes: &mut [ContractChange],
    diff_files: &HashSet<String>,
) {
    let mut by_file: HashMap<String, Vec<GraphNode>> = HashMap::new();
    for change in changes.iter_mut() {
        if !change.kind.is_contract_change() || change.kind == ChangeKind::Added {
            continue;
        }
        let Some(node) = stored_node(store, &mut by_file, change) else {
            continue;
        };
        let outside: Vec<_> = callers_of(store, &node)
            .into_iter()
            .filter(|c| !diff_files.contains(&c.file))
            .collect();
        change.callers_outside_diff_count = outside.len();
        change.callers_outside_diff = outside.into_iter().take(MAX_DISPLAYED_CALLERS).collect();
    }
}

/// Tie-break weight for a kind: a deletion outranks a signature change
/// outranks a move outranks an addition, and non-contract deltas sink.
fn kind_rank(kind: &ChangeKind) -> u8 {
    match kind {
        ChangeKind::Removed => 0,
        ChangeKind::SignatureChanged => 1,
        ChangeKind::Moved { .. } => 2,
        ChangeKind::Added => 3,
        ChangeKind::BodyOnly | ChangeKind::DocOnly => 4,
    }
}

/// Order changes so the first one is the one a reviewer must read first.
///
/// Callers-outside-the-diff dominates — a removal nobody calls is harmless,
/// a signature change with seven stranded call sites is the PR. Kind, public
/// visibility, then name break ties, the last purely so the output is stable.
pub fn rank(changes: &mut [ContractChange]) {
    changes.sort_by(|a, b| {
        b.callers_outside_diff_count
            .cmp(&a.callers_outside_diff_count)
            .then_with(|| kind_rank(&a.kind).cmp(&kind_rank(&b.kind)))
            .then_with(|| b.is_public.cmp(&a.is_public))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.file.cmp(&b.file))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(name: &str, kind: ChangeKind, callers: usize, public: bool) -> ContractChange {
        ContractChange {
            name: name.to_string(),
            symbol_kind: NodeKind::Function,
            file: "src/lib.rs".to_string(),
            kind,
            sig_base: None,
            sig_head: None,
            hash_base: None,
            hash_head: None,
            is_public: public,
            callers_outside_diff: Vec::new(),
            callers_outside_diff_count: callers,
        }
    }

    #[test]
    fn callers_outside_the_diff_dominate_the_order() {
        let mut changes = vec![
            change("harmless_removal", ChangeKind::Removed, 0, true),
            change("execute", ChangeKind::SignatureChanged, 7, true),
            change("noticed", ChangeKind::BodyOnly, 0, true),
        ];
        rank(&mut changes);
        assert_eq!(changes[0].name, "execute");
        assert_eq!(changes[1].name, "harmless_removal");
        assert_eq!(changes[2].name, "noticed");
    }

    #[test]
    fn ties_break_deterministically() {
        let mut changes = vec![
            change("zeta", ChangeKind::SignatureChanged, 2, true),
            change("alpha", ChangeKind::SignatureChanged, 2, true),
            change("private_one", ChangeKind::SignatureChanged, 2, false),
        ];
        rank(&mut changes);
        assert_eq!(
            changes.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            vec!["alpha", "zeta", "private_one"]
        );
    }
}
