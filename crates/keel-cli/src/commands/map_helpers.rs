//! CLI-local map utilities.
//!
//! The map-graph assembly (`build_map_result`, `populate_hotspots`,
//! `populate_functions`, `build_module_profiles`) lives in
//! [`keel_enforce::map`] so the CLI and the MCP server share one source of
//! truth. Only `make_relative`, which is a path helper specific to the CLI's
//! project-root-relative bookkeeping, remains here.

use std::path::Path;

/// Make a path relative to the project root.
pub fn make_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}
