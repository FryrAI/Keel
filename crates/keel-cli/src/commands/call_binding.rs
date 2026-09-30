//! Candidate eligibility shared by every graph call-resolution pass.

use std::collections::{HashMap, HashSet};

use keel_core::types::NodeKind;
use keel_parsers::resolver::{Definition, Reference, ReferenceKind};

use super::map_resolve::CallIndex;

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
    // SQL also uses is_associated as an enforcement exemption, not membership.
    let language = keel_parsers::treesitter::detect_language(std::path::Path::new(target_file));
    if !language.is_some_and(|l| {
        matches!(l, "rust" | "go" | "python" | "astro")
            || keel_parsers::treesitter::is_typescript_family(l)
    }) {
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

/// Whether the callee text is an unqualified call name.
pub(crate) fn is_bare_call(reference: &Reference) -> bool {
    reference.kind == ReferenceKind::Call
        && !reference.name.contains('.')
        && !reference.name.contains("::")
}

/// Eligibility for a selected target; candidate lists retain base ambiguity.
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

/// Preserve the base local pick, except for one eligible local beside members.
/// The boolean reports that the unique-free replacement branch actually fired.
pub(crate) fn select_local_target(
    reference: &Reference,
    selected: u64,
    candidates: impl Iterator<Item = (u64, bool, bool)>,
) -> Option<(u64, bool)> {
    let mut selected_allowed = false;
    let mut eligible = None;
    let mut eligible_count = 0;
    let mut eligible_is_free = false;
    let mut rejected = false;
    for (id, allowed, associated) in candidates {
        if id == selected {
            selected_allowed = allowed;
        }
        if allowed {
            eligible = Some(id);
            eligible_is_free = !associated;
            eligible_count += 1;
        } else {
            rejected = true;
        }
    }
    if is_bare_call(reference) && rejected && eligible_count == 1 && eligible_is_free {
        eligible.map(|id| (id, true))
    } else {
        selected_allowed.then_some((selected, false))
    }
}

/// Apply local selection using association facts from the first-pass definitions.
pub(crate) fn select_parsed_local_target(
    reference: &Reference,
    selected: u64,
    file: &str,
    definitions: &[Definition],
    locals: &[(&Definition, u64)],
) -> Option<u64> {
    select_local_target(
        reference,
        selected,
        locals.iter().map(|(d, id)| {
            (
                *id,
                !d.is_associated
                    || allows_associated(reference, file, file, d.line_start, definitions),
                d.is_associated,
            )
        }),
    )
    .map(|(id, _)| id)
}

/// Association facts keyed by graph id; `None` records a fresh free definition.
pub(crate) type AssociationFacts = HashMap<u64, Option<(String, u32)>>;

/// Fresh facts keyed by local id, plus names whose facts need stored-row selection.
pub(crate) fn local_association_facts(
    local: &HashMap<String, u64>,
    file_path: &str,
    definitions: &[Definition],
) -> (AssociationFacts, HashSet<String>) {
    let mut local_associated = HashMap::new();
    let mut conflicting_names = HashSet::new();
    for def in definitions {
        if let Some(&id) = local.get(&def.name) {
            let fact = def
                .is_associated
                .then(|| (file_path.to_string(), def.line_start));
            match local_associated.entry(id) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(fact);
                }
                std::collections::hash_map::Entry::Occupied(entry) => {
                    if entry.get() != &fact {
                        conflicting_names.insert(def.name.clone());
                    }
                }
            }
        }
    }
    (local_associated, conflicting_names)
}

/// Whether a bare same-file collision permits the one free-definition replacement.
pub(crate) fn has_local_replacement(
    reference: &Reference,
    file: &str,
    name: &str,
    definitions: &[Definition],
) -> bool {
    if !is_bare_call(reference) {
        return false;
    }
    let mut eligible = 0;
    let mut rejected = false;
    let mut eligible_is_free = false;
    for def in definitions.iter().filter(|d| d.name == name) {
        if !def.is_associated
            || allows_associated(reference, file, file, def.line_start, definitions)
        {
            eligible += 1;
            eligible_is_free = !def.is_associated;
        } else {
            rejected = true;
        }
    }
    rejected && eligible == 1 && eligible_is_free
}

#[cfg(test)]
#[path = "call_binding_tests.rs"]
mod tests;
