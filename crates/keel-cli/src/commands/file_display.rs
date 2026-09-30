//! Preserve established root-invocation display paths independently of graph keys.

use std::path::Path;

/// Render a graph-backed file argument as base commands did at the project root.
/// Subdirectory invocations display the resolved graph key for root/package parity.
pub(crate) fn graph_argument(cwd: &Path, root: &Path, input: &str, key: &str) -> String {
    if cwd == root {
        let path = Path::new(input);
        path.strip_prefix(cwd)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    } else {
        key.to_string()
    }
}
