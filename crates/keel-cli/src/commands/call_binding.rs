//! Candidate eligibility shared by every graph call-resolution pass.

use std::borrow::Cow;
use std::collections::HashMap;

use keel_core::types::NodeKind;
use keel_parsers::resolver::{Definition, Reference, ReferenceKind};

use super::map_resolve::CallIndex;

/// Associated-item facts for a graph target, preferring unambiguous fresh locals.
pub(crate) fn graph_associated_target(
    store: &keel_core::sqlite::SqliteGraphStore,
    local: &HashMap<String, u64>,
    file_path: &str,
    definitions: &[Definition],
    id: u64,
) -> Option<(String, u32)> {
    use keel_core::store::GraphStore;
    if let Some((name, _)) = local.iter().find(|(_, local_id)| **local_id == id) {
        let mut matches = definitions.iter().filter(|d| &d.name == name);
        if let (Some(def), None) = (matches.next(), matches.next()) {
            return def
                .is_associated
                .then(|| (file_path.to_string(), def.line_start));
        }
    }
    store.get_node_by_id(id).and_then(|node| {
        node.is_associated
            .then_some((node.file_path, node.line_start))
    })
}

/// Whether a reference can name an associated definition at this location.
///
/// Callee text already retains receivers and paths in all four grammars. A
/// bare call cannot name a member, except while executing a Python class body:
/// that block has its own local namespace, which is not visible in methods.
pub(crate) fn allows_associated(
    reference: &Reference,
    caller_file: &str,
    target_file: &str,
    target_line: u32,
    definitions: &[Definition],
) -> bool {
    if !is_bare_call(reference) {
        return true;
    }
    if keel_parsers::treesitter::detect_language(std::path::Path::new(caller_file))
        != Some("python")
        || caller_file != target_file
    {
        return false;
    }
    let enclosing = |line| {
        definitions
            .iter()
            .filter(|d| d.line_start <= line && line <= d.line_end)
            .filter(|d| matches!(d.kind, NodeKind::Class | NodeKind::Function))
            .min_by_key(|d| d.line_end - d.line_start)
    };
    let Some(class) = enclosing(reference.line).filter(|d| d.kind == NodeKind::Class) else {
        return false;
    };
    // The target must belong to this class, not a sibling/nested class, and
    // must already have been defined when the class body executes the call.
    target_line < reference.line
        && definitions
            .iter()
            .filter(|d| d.kind == NodeKind::Class)
            .filter(|d| d.line_start <= target_line && target_line <= d.line_end)
            .min_by_key(|d| d.line_end - d.line_start)
            .is_some_and(|owner| std::ptr::eq(owner, class))
}

fn is_bare_call(reference: &Reference) -> bool {
    reference.kind == ReferenceKind::Call
        && !reference.name.contains('.')
        && !reference.name.contains("::")
}

/// A call index with impossible member candidates removed before selection.
pub(crate) struct BindingIndex<'a> {
    pub(crate) inner: &'a dyn CallIndex,
    pub(crate) reference: &'a Reference,
    pub(crate) caller_file: &'a str,
    pub(crate) definitions: &'a [Definition],
}

impl BindingIndex<'_> {
    /// Whether the selected target belongs to this reference's candidate set.
    pub(crate) fn allows(&self, id: u64) -> bool {
        if !is_bare_call(self.reference) {
            return true;
        }
        match self.inner.associated_target(id) {
            Some((file, line)) => allows_associated(
                self.reference,
                self.caller_file,
                &file,
                line,
                self.definitions,
            ),
            None => true,
        }
    }
}

impl CallIndex for BindingIndex<'_> {
    fn candidates(&self, name: &str) -> Cow<'_, [(String, u64)]> {
        let candidates = self.inner.candidates(name);
        if candidates.iter().all(|(_, id)| self.allows(*id)) {
            return candidates;
        }
        Cow::Owned(
            candidates
                .iter()
                .filter(|(_, id)| self.allows(*id))
                .cloned()
                .collect(),
        )
    }
    fn associated_target(&self, id: u64) -> Option<(String, u32)> {
        self.inner.associated_target(id)
    }
    fn module_files(&self) -> &HashMap<String, u64> {
        self.inner.module_files()
    }
    fn name_to_id(&self) -> &HashMap<(String, String), u64> {
        self.inner.name_to_id()
    }
    fn package_index(&self) -> &HashMap<String, HashMap<String, u64>> {
        self.inner.package_index()
    }
    fn boundary_index(&self) -> &HashMap<String, (u64, f64)> {
        self.inner.boundary_index()
    }
}

#[cfg(test)]
#[path = "call_binding_tests.rs"]
mod tests;
