//! The stored-hash fixup must not rewrite a module-identity finding.
//!
//! W007/W009/E006 are *file*-level: they carry the file's MODULE hash and a
//! line that is a file position, not a definition's `line_start`. The fixup in
//! `EnforcementEngine::compile` rewrites a violation's hash to the hash of the
//! stored definition that begins on its line — correct for the def-level codes,
//! silently destructive for these, since `keel explain` would then resolve a
//! file-level finding to whichever function happened to start there.

use super::*;
use crate::test_fixtures::file_index;
use keel_core::sqlite_meta::LAST_MAP_AT;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, NodeChange};
use keel_parsers::resolver::{Import, Reference, ReferenceKind};

const MODULE_HASH: &str = "modulehash1";
const HARNESS_FILE: &str = "crates/harness/src/run.rs";
const CORE_FILE: &str = "crates/core/src/ingest.rs";

/// A stored module node for `file`, carrying [`MODULE_HASH`].
fn module_node(id: u64, file: &str, package: &str) -> GraphNode {
    GraphNode {
        complexity: 0,
        is_trivial_wrapper: false,
        in_test_context: false,
        id,
        hash: MODULE_HASH.to_string(),
        kind: NodeKind::Module,
        name: "run".to_string(),
        signature: String::new(),
        file_path: file.to_string(),
        line_start: 1,
        line_end: 400,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        has_docstring: false,
        is_associated: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: Some(package.to_string()),
    }
}

/// A stored function node at `line_start`, in a declared package.
fn fn_node(id: u64, name: &str, file: &str, package: &str, line_start: u32) -> GraphNode {
    let mut node = make_node(
        id,
        &format!("fnhash{id}"),
        name,
        &format!("fn {name}()"),
        file,
    );
    node.line_start = line_start;
    node.line_end = line_start;
    node.package = Some(package.to_string());
    node
}

/// A config that declares packages, so the boundary checks switch on.
fn monorepo_config() -> keel_core::config::KeelConfig {
    let mut config = keel_core::config::KeelConfig::default();
    config.monorepo.enabled = true;
    config.monorepo.packages = vec!["crates/core".into(), "crates/harness".into()];
    config
}

/// W009 reports at the offending CALL SITE. When that call sits on the first
/// line of a one-line function — the shape of a thin wrapper, and of most
/// generated code — the fixup used to swap the module hash for the wrapper's.
#[test]
fn w009_keeps_the_module_hash_when_a_definition_starts_on_its_line() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .update_nodes(
            vec![
                module_node(1, HARNESS_FILE, "harness"),
                // The one-line wrapper whose body IS the offending call.
                fn_node(2, "wrap", HARNESS_FILE, "harness", 12),
                fn_node(3, "raster_ingest", CORE_FILE, "core", 1),
                fn_node(4, "enqueue", CORE_FILE, "core", 20),
            ]
            .into_iter()
            .map(NodeChange::Add)
            .collect(),
        )
        .unwrap();
    // A stored in-package edge, so the harness module counts as mapped and its
    // grandfathered boundary set is non-empty but does not include `core`.
    store
        .update_edges(vec![EdgeChange::Add(GraphEdge {
            id: 1,
            source_id: 2,
            target_id: 1,
            kind: EdgeKind::Calls,
            file_path: HARNESS_FILE.into(),
            line: 12,
            confidence: 1.0,
        })])
        .unwrap();
    store.set_meta_value(LAST_MAP_AT, "1700000000").unwrap();

    let mut def = make_definition("wrap", "fn wrap()", "raster_ingest()", HARNESS_FILE);
    def.line_start = 12;
    def.line_end = 12;
    let mut file = file_index(HARNESS_FILE, vec![def]);
    file.references = vec![Reference {
        name: "raster_ingest".into(),
        file_path: HARNESS_FILE.into(),
        line: 12,
        kind: ReferenceKind::Call,
        resolved_to: None,
        call_arity: None,
    }];
    file.imports = vec![Import {
        source: "core::ingest".into(),
        imported_names: vec!["raster_ingest".into()],
        file_path: HARNESS_FILE.into(),
        line: 1,
        is_relative: false,
    }];

    let mut engine = EnforcementEngine::with_config(Box::new(store), &monorepo_config());
    let result = engine.compile(&[file]);

    let w009 = result
        .warnings
        .iter()
        .find(|v| v.code == "W009")
        .unwrap_or_else(|| panic!("W009 should fire: {:?}", result.warnings));
    assert_eq!(w009.line, 12, "W009 reports at the call site");
    assert_eq!(
        w009.hash, MODULE_HASH,
        "a module-identity finding must keep the module's hash, not the hash of \
         the definition that happens to start on the call's line"
    );
}

/// W007 always reports at line 1 — so any file whose first definition starts
/// there (a Go file after its package clause, a `.py` module, anything
/// generated) lost its module hash to that definition.
#[test]
fn w007_keeps_the_module_hash_when_a_definition_starts_on_line_one() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let mut stored_fn = fn_node(1, "big", "src/huge.rs", "app", 1);
    // Stored extent below the new one, so W007's grew-gate passes.
    stored_fn.line_end = 300;
    // The definition takes the LOWER id on purpose: a module node also starts
    // at line 1, so whichever row `get_nodes_in_file` yields first decided
    // whether the bug bit. That query has no ORDER BY — "the module happens to
    // come back first" is an artifact of insertion order, not a property.
    store
        .update_nodes(
            vec![stored_fn, module_node(2, "src/huge.rs", "app")]
                .into_iter()
                .map(NodeChange::Add)
                .collect(),
        )
        .unwrap();

    let mut def = make_definition("big", "fn big()", "1", "src/huge.rs");
    def.line_start = 1;
    def.line_end = 450;
    let mut engine = EnforcementEngine::new(Box::new(store));
    let result = engine.compile(&[file_index("src/huge.rs", vec![def])]);

    let w007 = result
        .warnings
        .iter()
        .find(|v| v.code == "W007")
        .unwrap_or_else(|| panic!("W007 should fire: {:?}", result.warnings));
    assert_eq!(w007.line, 1);
    assert_eq!(
        w007.hash, MODULE_HASH,
        "W007 is about the file; the definition at line 1 must not claim it"
    );
}

/// The other half of the contract: the def-level codes still get the graph's
/// hash, disambiguation ordinal and all, so `keel explain <hash>` resolves.
#[test]
fn def_level_codes_still_take_the_stored_hash() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let mut stored = make_node(1, "storedhash1", "legacy", "fn legacy()", "src/a.rs");
    stored.line_start = 10;
    stored.has_docstring = false;
    stored.docstring = None;
    store.update_nodes(vec![NodeChange::Add(stored)]).unwrap();

    // Public and undocumented at the stored node's line: E003 fires.
    let mut def = make_definition("legacy", "fn legacy()", "work()", "src/a.rs");
    def.docstring = None;

    let mut config = keel_core::config::KeelConfig::default();
    config.enforce.progressive = false;
    let mut engine = EnforcementEngine::with_config(Box::new(store), &config);
    let result = engine.compile(&[file_index("src/a.rs", vec![def])]);

    let e003 = result
        .errors
        .iter()
        .chain(result.warnings.iter())
        .find(|v| v.code == "E003")
        .unwrap_or_else(|| panic!("E003 should fire: {:?}", result.errors));
    assert_eq!(e003.hash, "storedhash1");
}
