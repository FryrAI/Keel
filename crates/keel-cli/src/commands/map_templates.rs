//! Full-map template home indexing. Uses the same walker population as nodes;
//! excluded/ignored files cannot become invisible owners in review exports.

use keel_core::store::GraphStore;
use keel_core::types::GraphError;
use keel_parsers::walker::WalkEntry;
use std::path::Path;

/// Rebuild the invocation-wide home cache after every full map.
pub(super) fn refresh(
    store: &mut dyn GraphStore,
    entries: &[WalkEntry],
    root: &Path,
) -> Result<(), GraphError> {
    let mut homes = Vec::new();
    for entry in entries {
        let file = entry
            .path
            .strip_prefix(root)
            .unwrap_or(&entry.path)
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(source) = std::fs::read_to_string(&entry.path) else {
            continue;
        };
        homes.extend(keel_enforce::template_respelled::extract_homes(
            &file, &source,
        ));
    }
    homes.sort_by(|a, b| (&a.file, a.line, &a.name).cmp(&(&b.file, b.line, &b.name)));
    store.replace_template_homes(homes)
}
