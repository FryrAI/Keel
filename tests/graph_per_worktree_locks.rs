//! Each checkout's graph lock is its own (issue #107, R3): a lock held on one
//! checkout's graph directory never stops a writer in the other. The lock is taken
//! in-process through `keel_core::graph_lock`, so every writer is covered.

#[path = "common/mod.rs"]
mod common;
#[path = "common/worktree_graph.rs"]
mod wg;

use wg::*;

/// A compile in the worktree checks while main's graph lock is held.
#[test]
fn compile_in_worktree_does_not_wait_for_a_held_main_lock() {
    let p = pair_mapped();
    let _held = keel_core::graph_lock::try_acquire(&p.main.join(".keel")).unwrap();
    write(&p.wt, "alpha.py", ALPHA_BROKEN);
    let o = keel(&p.wt, &["compile", "alpha.py"]);
    assert_eq!(o.status.code(), Some(1), "{}", describe(&o));
    assert!(out(&o).contains("E001"), "{}", describe(&o));
}

/// A compile in main checks while the worktree's graph lock is held.
#[test]
fn compile_in_main_does_not_wait_for_a_held_worktree_lock() {
    let p = pair_mapped();
    let _held = keel_core::graph_lock::try_acquire(&abs_git_dir(&p.wt).join("keel")).unwrap();
    write(&p.main, "alpha.py", ALPHA_BROKEN);
    let o = keel(&p.main, &["compile", "alpha.py"]);
    assert_eq!(o.status.code(), Some(1), "{}", describe(&o));
    assert!(out(&o).contains("E001"), "{}", describe(&o));
}

/// A map in the worktree completes while main's graph lock is held.
#[test]
fn map_in_worktree_does_not_wait_for_a_held_main_lock() {
    let p = pair_mapped();
    let _held = keel_core::graph_lock::try_acquire(&p.main.join(".keel")).unwrap();
    let o = keel(&p.wt, &["map"]);
    assert_eq!(o.status.code(), Some(0), "{}", describe(&o));
}
