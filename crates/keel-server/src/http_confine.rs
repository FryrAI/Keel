//! Path confinement for HTTP compile requests.
//!
//! The HTTP server is unauthenticated localhost tooling, so a compile request
//! must never be able to reach files outside the project root. Every candidate
//! path is resolved against the root and lexically normalized (resolving `.`
//! and `..` without touching the filesystem, so non-existent targets still
//! validate) and rejected unless the result stays under the root.

use std::path::{Component, Path, PathBuf};

/// Resolve `candidate` against `root` and confine it to the project tree.
///
/// Returns the normalized absolute path when it stays under `root`, or `None`
/// when `candidate` is absolute-outside-root or escapes via `..`. `root` is
/// assumed already canonicalized (an existing directory).
pub(crate) fn confine(root: &Path, candidate: &str) -> Option<PathBuf> {
    let raw = Path::new(candidate);
    let joined = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        root.join(raw)
    };
    let normalized = normalize_lexically(&joined);
    normalized.starts_with(root).then_some(normalized)
}

/// Resolve `.` and `..` components without consulting the filesystem.
///
/// A leading `..` that would climb above the path root is simply dropped, so an
/// escaping candidate normalizes to something outside `root` and fails the
/// `starts_with` check in [`confine`] rather than silently resolving.
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
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
}
