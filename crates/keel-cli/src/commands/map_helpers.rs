//! CLI-local map utilities.
//!
//! The map-graph assembly (`build_map_result`, `populate_hotspots`,
//! `populate_functions`, `build_module_profiles`) lives in
//! [`keel_enforce::map`] so the CLI and the MCP server share one source of
//! truth.

use std::path::Path;

/// Make a path relative to the project root.
///
/// Thin re-export of the canonical [`keel_core::paths::make_relative`] so the
/// many `use super::map_helpers::make_relative;` call sites keep compiling
/// against one implementation.
pub fn make_relative(root: &Path, path: &Path) -> String {
    keel_core::paths::make_relative(root, path)
}
