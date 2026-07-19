//! Path confinement for HTTP compile requests.
//!
//! The HTTP server is unauthenticated localhost tooling, so a compile request
//! must never be able to reach files outside the project root. Every candidate
//! path is resolved against the root and lexically normalized (resolving `.`
//! and `..` without touching the filesystem, so non-existent targets still
//! validate); when the target actually exists it is additionally canonicalized
//! so a symlink inside the root pointing outside it cannot smuggle external
//! files past the lexical check.

use std::path::{Path, PathBuf};

/// Resolve `candidate` against `root` and confine it to the project tree.
///
/// Returns the normalized absolute path when it stays under `root`, or `None`
/// when `candidate` is absolute-outside-root, escapes via `..`, or is a
/// symlink whose real target lies outside `root`. `root` is assumed already
/// canonicalized (an existing directory). Non-existent targets pass on the
/// lexical check alone (they cannot be read anyway).
pub(crate) fn confine(root: &Path, candidate: &str) -> Option<PathBuf> {
    keel_core::paths::confine(root, candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("/home/user/project")
    }

    #[test]
    fn accepts_relative_inside_root() {
        let got = confine(&root(), "src/main.rs").unwrap();
        assert_eq!(got, PathBuf::from("/home/user/project/src/main.rs"));
    }

    #[test]
    fn accepts_nested_relative_inside_root() {
        let got = confine(&root(), "a/b/../c.rs").unwrap();
        assert_eq!(got, PathBuf::from("/home/user/project/a/c.rs"));
    }

    #[test]
    fn rejects_parent_escape() {
        assert!(confine(&root(), "../../etc/passwd").is_none());
    }

    #[test]
    fn rejects_absolute_outside_root() {
        assert!(confine(&root(), "/etc/passwd").is_none());
    }

    #[test]
    fn accepts_absolute_inside_root() {
        let got = confine(&root(), "/home/user/project/src/lib.rs").unwrap();
        assert_eq!(got, PathBuf::from("/home/user/project/src/lib.rs"));
    }

    #[test]
    fn rejects_sneaky_prefix_sibling() {
        // A sibling dir that merely shares a name prefix must not pass.
        assert!(confine(&root(), "/home/user/project-evil/x.rs").is_none());
    }

    /// A symlink inside the root pointing outside it must be rejected even
    /// though it passes the lexical check (the walker/read would follow it).
    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escaping_root() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "s").unwrap();
        let project = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(project.path()).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("evil")).unwrap();
        std::fs::write(root.join("ok.rs"), "fn a() {}").unwrap();

        assert!(confine(&root, "evil").is_none(), "symlinked dir must fail");
        assert!(
            confine(&root, "evil/secret.txt").is_none(),
            "file through symlinked dir must fail"
        );
        assert!(confine(&root, "ok.rs").is_some(), "real file still passes");
    }
}
