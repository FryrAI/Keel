//! One stderr line when a full `keel map` baselined uncommitted source files.

use std::path::Path;

use keel_enforce::gitdiff::{
    changed_files_checked, head_commit, untracked_files_checked, DiffMode,
};

/// Rule N1: in a repository with a commit, say how many modified and untracked
/// source files were mapped as they are; on a git failure, say it could not count.
pub(super) fn note_uncommitted(root: &Path) {
    if head_commit(root).is_none() {
        return;
    }
    let counts = changed_files_checked(root, &DiffMode::Since(None), true)
        .and_then(|m| untracked_files_checked(root).map(|u| (m.len(), u.len())));
    match counts {
        Ok((0, 0)) => {}
        Ok((m, u)) => eprintln!(
            "keel map: {m} modified and {u} untracked source file(s) were mapped as they are — uncommitted changes are now baseline and were not checked against their callers"
        ),
        Err(e) => eprintln!("keel map: could not list uncommitted files: {e}"),
    }
}
