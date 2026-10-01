//! Full-map template home indexing from the first pass's existing parses.

use super::map_passes::FileParseData;
use keel_core::store::GraphStore;
use keel_core::types::GraphError;

/// Rebuild the invocation-wide home cache after every full map.
pub(super) fn refresh(
    store: &mut dyn GraphStore,
    files: &[FileParseData],
) -> Result<(), GraphError> {
    let mut homes: Vec<_> = files
        .iter()
        .flat_map(|file| file.template_homes.iter().cloned())
        .collect();
    homes.sort_by(|a, b| (&a.file, a.line, &a.name).cmp(&(&b.file, b.line, &b.name)));
    store.replace_template_homes(homes)
}
