//! Architectural-boundary erosion checks.
//!
//! - W009 `new_cross_boundary_dep` — this module just started calling into a
//!   package it did not depend on before (WARNING, confidence 0.9).
//! - E006 `layer_violation` — that dependency matches an ordered pair in the
//!   opt-in `architecture.deny` list (ERROR, gates exit 1).
//!
//! An architecture decision is cheapest to reverse at the moment it is made,
//! and a cross-package edge is a design decision that otherwise gets zero
//! review because it looks like one added `use` line in a diff of 400.
//!
//! **Self-baselining.** Everything already in the graph is grandfathered: the
//! stored `calls` targets of the compiled file's MODULE define its allowed
//! boundary set, so only *new* erosion fires. That is keel's progressive-
//! adoption philosophy lifted from the function level to the architecture
//! level, and it is why W009 needs no configuration and no baseline file.
//!
//! **Symmetry over reach.** The baseline comes from edges keel's map resolved;
//! this check resolves names itself. Anything the check can see but the map
//! cannot would be reported on an unchanged tree, forever — so the detector is
//! deliberately narrower than what it *could* attribute: a reference counts
//! only when the file imported that exact name, and only when it resolves to
//! exactly one boundary's public, non-associated function. Measured over
//! keel's own 6-crate workspace, those three filters take an unchanged tree
//! from 8 findings to 0 while a genuine new cross-crate call still fires.
//!
//! **Silent unless the repo declares boundaries.** With no declared packages a
//! directory-derived boundary is a guess, and a guessed boundary produces
//! confident wrong warnings with no signal that it was guessed — so a flat repo
//! sees nothing at all.

use std::collections::{HashMap, HashSet};

use keel_core::config::ArchitectureConfig;
use keel_core::store::GraphStore;
use keel_core::types::{Boundary, GraphNode, NodeKind};
use keel_parsers::resolver::{FileIndex, ReferenceKind};

use crate::types::Violation;
use crate::violations_util::{is_bench_file, is_stub_file, is_test_file};

/// Confidence carried by both codes: the boundary sides are read straight out
/// of the graph, and only unambiguous name resolutions are counted, but the
/// name-to-node step is still a heuristic.
const BOUNDARY_CONFIDENCE: f64 = 0.9;

/// Per-compile state for the boundary checks, built once and shared by every
/// file in the batch.
pub struct BoundaryContext {
    enabled: bool,
    count_type_deps: bool,
    deny: HashSet<(String, String)>,
}

impl BoundaryContext {
    /// Build the context for one compile.
    ///
    /// `packages_declared` is the repo's own statement that it has boundaries
    /// (a detected monorepo layout in `keel.json`). Combined with the
    /// `last_map_at` marker, it forms the bootstrap guard: before a first
    /// `keel map` the graph holds no edges at all, so *every* dependency would
    /// read as new.
    pub fn new(store: &dyn GraphStore, arch: &ArchitectureConfig, packages_declared: bool) -> Self {
        let mapped = store
            .meta_value(keel_core::sqlite_boundary::LAST_MAP_AT)
            .is_some();
        Self {
            enabled: packages_declared && mapped,
            count_type_deps: arch.count_type_deps,
            deny: arch.deny.iter().cloned().collect(),
        }
    }

    /// A context that never fires — used by engines built without a config.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            count_type_deps: false,
            deny: HashSet::new(),
        }
    }
}

/// W009/E006: cross-boundary dependencies this file did not have before.
///
/// Returns at most one violation per newly depended-on boundary, reported at
/// the first reference that reaches it.
pub fn check_cross_boundary_deps(
    file: &FileIndex,
    store: &dyn GraphStore,
    existing_nodes: &[GraphNode],
    ctx: &BoundaryContext,
) -> Vec<Violation> {
    if !ctx.enabled
        || is_test_file(&file.file_path)
        || is_stub_file(&file.file_path)
        || is_bench_file(&file.file_path)
    {
        return Vec::new();
    }
    let path = file.file_path.replace('\\', "/");
    let Some(dir) = path.rsplit_once('/').map(|(d, _)| d) else {
        // A repo-root file has no directory segment, so it has no boundary.
        return Vec::new();
    };

    // Both the bootstrap guard and the grandfathered set live at MODULE level,
    // not file level. Gating on the file would exempt every newly created file
    // by definition — the most likely way to introduce a boundary violation —
    // and baselining on the file would report keel's own resolution gaps as
    // architecture changes: a cross-package call the map cannot resolve leaves
    // no stored edge, so an unchanged tree would warn about it forever.
    let module = store.module_boundary_info(dir);
    if !module.is_mapped() {
        return Vec::new();
    }

    let Some(own) = own_boundary(existing_nodes, module.package.as_deref(), &path) else {
        return Vec::new();
    };

    let stored: HashSet<Boundary> = module
        .call_targets
        .iter()
        .filter_map(|t| t.boundary())
        .collect();

    let evidence = ImportEvidence::of(file);
    let counted: Vec<(&str, u32)> = file
        .references
        .iter()
        .filter(|r| match r.kind {
            ReferenceKind::Call => true,
            ReferenceKind::TypeRef => ctx.count_type_deps,
            _ => false,
        })
        .map(|r| (bare_name(&r.name), r.line))
        .filter(|(name, _)| !name.is_empty() && evidence.covers(name))
        .collect();
    let mut lookup: Vec<&str> = Vec::new();
    let mut seen_names: HashSet<&str> = HashSet::new();
    for (name, _) in &counted {
        if seen_names.insert(name) {
            lookup.push(name);
        }
    }
    if lookup.is_empty() {
        return Vec::new();
    }

    // One query for the whole name set; a round trip per reference would blow
    // the hot-path budget on its own.
    let mut resolved: HashMap<String, Resolution> = HashMap::new();
    for target in store.find_boundary_targets(&lookup, &file.file_path) {
        let Some(boundary) = target.boundary() else {
            continue;
        };
        match resolved.entry(target.name) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(Resolution::Unique {
                    boundary,
                    file_path: target.file_path,
                });
            }
            std::collections::hash_map::Entry::Occupied(mut slot) => {
                if matches!(slot.get(), Resolution::Unique { boundary: b, .. } if *b != boundary) {
                    slot.insert(Resolution::Ambiguous);
                }
            }
        }
    }

    // First reference reaching each foreign boundary wins the report line.
    let mut deps: Vec<Dep> = Vec::new();
    let mut reported: HashSet<Boundary> = HashSet::new();
    for (bare, line) in &counted {
        let Some(Resolution::Unique {
            boundary,
            file_path,
        }) = resolved.get(*bare)
        else {
            continue;
        };
        if *boundary == own || !reported.insert(boundary.clone()) {
            continue;
        }
        deps.push(Dep {
            boundary: boundary.clone(),
            target_file: file_path.clone(),
            symbol: (*bare).to_string(),
            line: *line,
        });
    }

    let module_hash = existing_nodes
        .iter()
        .find(|n| n.kind == NodeKind::Module)
        .map(|n| n.hash.clone())
        .unwrap_or_default();

    deps.into_iter()
        .filter_map(|dep| {
            let denied = ctx
                .deny
                .contains(&(own.label().to_string(), dep.boundary.label().to_string()));
            if !denied && stored.contains(&dep.boundary) {
                return None;
            }
            let facade = store.boundary_facade(&dep.boundary);
            Some(build_violation(
                &own,
                &dep,
                facade.as_deref(),
                denied,
                &file.file_path,
                &module_hash,
            ))
        })
        .collect()
}

/// One newly observed dependency on a foreign boundary.
struct Dep {
    boundary: Boundary,
    target_file: String,
    symbol: String,
    line: u32,
}

/// What a reference name resolved to in the graph.
enum Resolution {
    /// Every stored function with this name lives in one boundary.
    Unique {
        boundary: Boundary,
        file_path: String,
    },
    /// Same-named functions in several boundaries — which one is meant cannot
    /// be told from the name, so the reference is dropped rather than guessed.
    Ambiguous,
}

/// Assemble the W009 warning or its E006 escalation.
fn build_violation(
    own: &Boundary,
    dep: &Dep,
    facade: Option<&str>,
    denied: bool,
    file_path: &str,
    module_hash: &str,
) -> Violation {
    let from = own.label();
    let to = dep.boundary.label();
    let entry = match facade {
        Some(f) => format!(
            "`{to}` already exposes `{f}` — go through it instead of reaching into \
             `{}`, or move the shared code into a boundary both already depend on",
            dep.target_file
        ),
        None => format!(
            "Depend on `{to}`'s public surface instead of reaching into `{}`, or move \
             the shared code into a boundary both already depend on",
            dep.target_file
        ),
    };
    let (code, severity, category, message, fix_hint) = if denied {
        (
            "E006",
            "ERROR",
            "layer_violation",
            format!(
                "`{from}` must not depend on `{to}` (denied in architecture.deny) — \
                 calls `{}`",
                dep.symbol
            ),
            entry,
        )
    } else {
        (
            "W009",
            "WARNING",
            "new_cross_boundary_dep",
            format!("New dependency `{from}` -> `{to}` via `{}`", dep.symbol),
            entry,
        )
    };
    Violation {
        code: code.to_string(),
        severity: severity.to_string(),
        category: category.to_string(),
        message,
        file: file_path.to_string(),
        line: dep.line,
        hash: module_hash.to_string(),
        confidence: BOUNDARY_CONFIDENCE,
        resolution_tier: "heuristic".to_string(),
        fix_hint: Some(fix_hint),
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    }
}

/// The boundary the compiled file itself belongs to.
///
/// Its own stored nodes first, then its module siblings' declared package (so
/// a brand-new file inherits the package of the directory it was created in
/// rather than reading as cross-package against its own neighbours), then the
/// directory-segment fallback.
fn own_boundary(
    existing_nodes: &[GraphNode],
    module_package: Option<&str>,
    file_path: &str,
) -> Option<Boundary> {
    let declared = existing_nodes
        .iter()
        .find_map(|n| n.package.as_deref().filter(|p| !p.is_empty()))
        .or(module_package);
    Boundary::of(declared, file_path)
}

/// Final segment of a possibly qualified reference name (`fmt.Println` →
/// `Println`, `Vec::new` → `new`).
fn bare_name(name: &str) -> &str {
    let after_colons = name.rsplit("::").next().unwrap_or(name);
    after_colons.rsplit('.').next().unwrap_or(after_colons)
}

/// The names one file imported *by name* — the only references W009 will
/// count.
///
/// A bare name on its own manufactures dependencies: Rust's `format!`/`write!`
/// macro sites, `.collect()` and every other method whose name also belongs to
/// some free function in another package, and any common verb (`run`, `parse`,
/// `load`). Matching on the qualifier instead is no better — `Formatter::fmt`
/// passes on the *type*, not on a dependency the file declared.
///
/// A file that imported the symbol by name has declared that dependency in its
/// own text, which is exactly the decision W009 exists to surface. The cost is
/// coverage: a fully-qualified path call (`other_crate::helper()`), a
/// namespace import (`import * as core`), and Go's `pkg.Func()` — which
/// imports the package, not the function — are all invisible. That trade is
/// deliberate; a false architecture warning teaches the team to distrust the
/// tool wholesale.
struct ImportEvidence<'a> {
    names: HashSet<&'a str>,
}

impl<'a> ImportEvidence<'a> {
    fn of(file: &'a FileIndex) -> Self {
        Self {
            names: file
                .imports
                .iter()
                .flat_map(|i| i.imported_names.iter().map(String::as_str))
                .collect(),
        }
    }

    /// True when this exact name was imported into the file.
    fn covers(&self, bare_name: &str) -> bool {
        self.names.contains(bare_name)
    }
}

#[cfg(test)]
#[path = "violations_boundary_tests.rs"]
mod tests;
