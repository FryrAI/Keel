//! W009 / E006 unit tests, driven against a real in-memory SQLite graph so the
//! boundary SQL (directory ranges, name lookup, façade ranking) is under test
//! too — a mock store would only prove the Rust glue.

use super::*;
use keel_core::sqlite::SqliteGraphStore;
use keel_core::sqlite_meta::LAST_MAP_AT;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, NodeChange};
use keel_parsers::resolver::{Import, Reference};

const CORE_FILE: &str = "crates/core/src/ingest.rs";
const CORE_API_FILE: &str = "crates/core/src/api.rs";
const HARNESS_FILE: &str = "crates/harness/src/run.rs";
const HARNESS_SIBLING: &str = "crates/harness/src/setup.rs";

/// A stored function node, optionally inside a declared package.
fn node(id: u64, name: &str, file: &str, package: Option<&str>, is_public: bool) -> GraphNode {
    GraphNode {
        complexity: 0,
        is_trivial_wrapper: false,
        in_test_context: false,
        id,
        hash: format!("h{id}"),
        kind: NodeKind::Function,
        name: name.to_string(),
        signature: format!("fn {name}()"),
        file_path: file.to_string(),
        line_start: 1,
        line_end: 5,
        docstring: None,
        is_public,
        type_hints_present: true,
        has_docstring: false,
        is_associated: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: package.map(str::to_string),
    }
}

fn seed_nodes(store: &mut SqliteGraphStore, nodes: Vec<GraphNode>) {
    store
        .update_nodes(nodes.into_iter().map(NodeChange::Add).collect())
        .expect("seed nodes");
}

fn seed_call_edge(store: &mut SqliteGraphStore, id: u64, src: u64, tgt: u64, file: &str) {
    store
        .update_edges(vec![EdgeChange::Add(GraphEdge {
            id,
            source_id: src,
            target_id: tgt,
            kind: EdgeKind::Calls,
            file_path: file.to_string(),
            line: 2,
            confidence: 1.0,
        })])
        .expect("seed edge");
}

/// A graph that already knows two packages, where `crates/harness/src` has
/// stored call edges (so the module-level bootstrap guard is satisfied) but
/// none of them reach `core`.
///
/// Node ids: 1 `execute` (core façade), 2 `raster_ingest` (core internal —
/// public, because a symbol another crate can call has to be), 3 `run`
/// (harness), 4 `prepare` (harness sibling file), 5 `enqueue` (core, the
/// in-package caller that makes `execute` the most-called public symbol).
fn mapped_workspace() -> SqliteGraphStore {
    let store = unmapped_workspace();
    store
        .set_meta_value(LAST_MAP_AT, "1700000000")
        .expect("stamp last_map_at");
    store
}

/// The same graph, minus the `last_map_at` marker — what a repo looks like
/// between `keel init` and the first `keel map`.
fn unmapped_workspace() -> SqliteGraphStore {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    seed_nodes(
        &mut store,
        vec![
            node(1, "execute", CORE_FILE, Some("core"), true),
            node(2, "raster_ingest", CORE_FILE, Some("core"), true),
            node(3, "run", HARNESS_FILE, Some("harness"), true),
            node(4, "prepare", HARNESS_SIBLING, Some("harness"), true),
            node(5, "enqueue", CORE_API_FILE, Some("core"), true),
        ],
    );
    // An in-package call, so the harness module has SOME stored call edges —
    // and none of them cross into `core`.
    seed_call_edge(&mut store, 10, 3, 4, HARNESS_FILE);
    // A core-internal caller for `execute`, so it outranks every other public
    // core symbol as the package façade.
    seed_call_edge(&mut store, 11, 5, 1, CORE_API_FILE);
    store
}

fn ctx(store: &SqliteGraphStore) -> BoundaryContext {
    BoundaryContext::new(store, &ArchitectureConfig::default(), true)
}

fn call(name: &str, line: u32, file: &str) -> Reference {
    Reference {
        name: name.to_string(),
        file_path: file.to_string(),
        line,
        kind: ReferenceKind::Call,
        resolved_to: None,
        call_arity: None,
    }
}

fn import(source: &str, names: &[&str], file: &str) -> Import {
    Import {
        source: source.to_string(),
        imported_names: names.iter().map(|n| n.to_string()).collect(),
        file_path: file.to_string(),
        line: 1,
        is_relative: false,
    }
}

/// A parsed file with references and imports but no definitions of its own.
fn file_with(file: &str, references: Vec<Reference>, imports: Vec<Import>) -> FileIndex {
    FileIndex {
        file_path: file.to_string(),
        content_hash: 0,
        definitions: vec![],
        references,
        imports,
        external_endpoints: vec![],
        parse_duration_us: 0,
    }
}

/// The canonical erosion case: `crates/harness` reaches into a `crates/core`
/// internal it never called before.
fn harness_reaching_into_core() -> FileIndex {
    file_with(
        HARNESS_FILE,
        vec![call("raster_ingest", 12, HARNESS_FILE)],
        vec![import("core::ingest", &["raster_ingest"], HARNESS_FILE)],
    )
}

#[test]
fn w009_fires_once_with_facade_fix_hint() {
    let store = mapped_workspace();
    let stored = store.get_nodes_in_file(HARNESS_FILE);
    let file = harness_reaching_into_core();

    let v = check_cross_boundary_deps(&file, &store, &stored, &ctx(&store));
    assert_eq!(v.len(), 1, "one warning per newly depended-on boundary");
    assert_eq!(v[0].code, "W009");
    assert_eq!(v[0].severity, "WARNING");
    assert_eq!(v[0].category, "new_cross_boundary_dep");
    assert!((v[0].confidence - 0.9).abs() < f64::EPSILON);
    assert_eq!(v[0].line, 12);
    assert!(
        v[0].message.contains("harness") && v[0].message.contains("core"),
        "message names both sides: {}",
        v[0].message
    );
    let hint = v[0].fix_hint.as_deref().unwrap();
    assert!(
        hint.contains("`execute`"),
        "fix_hint names the most-called public symbol as the façade: {hint}"
    );
}

#[test]
fn w009_reports_one_violation_per_boundary_not_per_reference() {
    let store = mapped_workspace();
    let stored = store.get_nodes_in_file(HARNESS_FILE);
    let file = file_with(
        HARNESS_FILE,
        vec![
            call("raster_ingest", 12, HARNESS_FILE),
            call("execute", 20, HARNESS_FILE),
        ],
        vec![import(
            "core::ingest",
            &["raster_ingest", "execute"],
            HARNESS_FILE,
        )],
    );

    let v = check_cross_boundary_deps(&file, &store, &stored, &ctx(&store));
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].line, 12, "first reference reaching the boundary wins");
}

#[test]
fn w009_grandfathers_a_dependency_already_in_the_graph() {
    let mut store = mapped_workspace();
    // `run` already calls `raster_ingest` per the last map.
    seed_call_edge(&mut store, 12, 3, 2, HARNESS_FILE);
    let stored = store.get_nodes_in_file(HARNESS_FILE);

    let v = check_cross_boundary_deps(&harness_reaching_into_core(), &store, &stored, &ctx(&store));
    assert!(v.is_empty(), "self-baselining: stored deps never fire");
}

/// The baseline is the MODULE's, not the file's: a sibling file's stored
/// dependency grandfathers the boundary for the whole directory. Keeping it
/// per-file made keel's own unresolved cross-crate calls — which leave no
/// stored edge — read as architecture changes on an unchanged tree.
#[test]
fn w009_grandfathers_a_dependency_a_sibling_file_already_has() {
    let mut store = mapped_workspace();
    // `prepare` (harness/src/setup.rs) already calls into core.
    seed_call_edge(&mut store, 12, 4, 2, HARNESS_SIBLING);
    let stored = store.get_nodes_in_file(HARNESS_FILE);

    let v = check_cross_boundary_deps(&harness_reaching_into_core(), &store, &stored, &ctx(&store));
    assert!(v.is_empty());
}

#[test]
fn w009_fires_for_a_brand_new_file_in_a_mapped_module() {
    let store = mapped_workspace();
    let new_file = "crates/harness/src/fresh.rs";
    let file = file_with(
        new_file,
        vec![call("raster_ingest", 3, new_file)],
        vec![import("core::ingest", &["raster_ingest"], new_file)],
    );

    // No stored nodes at all for this path — the module-level guard is what
    // keeps the check alive here, and it must inherit the module's package
    // rather than reading as its own boundary.
    let v = check_cross_boundary_deps(&file, &store, &[], &ctx(&store));
    assert_eq!(v.len(), 1, "a new file is the likeliest way to erode");
    assert_eq!(v[0].code, "W009");
}

#[test]
fn w009_silent_when_the_module_has_no_stored_call_edges() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    seed_nodes(
        &mut store,
        vec![
            node(1, "execute", CORE_FILE, Some("core"), true),
            node(2, "raster_ingest", CORE_FILE, Some("core"), false),
            node(3, "run", HARNESS_FILE, Some("harness"), true),
        ],
    );
    store.set_meta_value(LAST_MAP_AT, "1700000000").unwrap();
    let stored = store.get_nodes_in_file(HARNESS_FILE);

    let v = check_cross_boundary_deps(&harness_reaching_into_core(), &store, &stored, &ctx(&store));
    assert!(
        v.is_empty(),
        "an unmapped module has no baseline, so everything would look new"
    );
}

#[test]
fn w009_silent_before_the_first_map() {
    let store = unmapped_workspace();
    let stored = store.get_nodes_in_file(HARNESS_FILE);

    let v = check_cross_boundary_deps(&harness_reaching_into_core(), &store, &stored, &ctx(&store));
    assert!(v.is_empty(), "no last_map_at marker means no baseline");
}

#[test]
fn w009_silent_in_a_flat_repo() {
    let store = mapped_workspace();
    let stored = store.get_nodes_in_file(HARNESS_FILE);
    // `packages_declared = false` — a repo that declares no boundaries would
    // only ever get guessed ones.
    let flat = BoundaryContext::new(&store, &ArchitectureConfig::default(), false);

    let v = check_cross_boundary_deps(&harness_reaching_into_core(), &store, &stored, &flat);
    assert!(v.is_empty());
}

#[test]
fn w009_silent_without_import_evidence() {
    let store = mapped_workspace();
    let stored = store.get_nodes_in_file(HARNESS_FILE);
    // A bare name that happens to match a function in another package, with
    // nothing importing it — the `format!`-macro false-positive class.
    let file = file_with(
        HARNESS_FILE,
        vec![call("raster_ingest", 12, HARNESS_FILE)],
        vec![],
    );

    let v = check_cross_boundary_deps(&file, &store, &stored, &ctx(&store));
    assert!(v.is_empty(), "a dependency you never imported is not a dep");
}

/// The dominant false-positive class found by running this check over keel's
/// own 6-crate workspace: `store.get_edges(..)`, `Duration::from(..)`,
/// `iter.collect()` are dispatch, not named dependencies, and the map does not
/// resolve them — so an unchanged tree warned forever.
#[test]
fn w009_ignores_associated_and_private_targets() {
    let mut store = mapped_workspace();
    seed_nodes(
        &mut store,
        vec![
            GraphNode {
                is_associated: true,
                ..node(21, "get_edges", CORE_FILE, Some("core"), true)
            },
            node(22, "internal_helper", CORE_FILE, Some("core"), false),
        ],
    );
    let stored = store.get_nodes_in_file(HARNESS_FILE);
    let file = file_with(
        HARNESS_FILE,
        vec![
            call("get_edges", 12, HARNESS_FILE),
            call("internal_helper", 13, HARNESS_FILE),
        ],
        vec![import(
            "core",
            &["get_edges", "internal_helper"],
            HARNESS_FILE,
        )],
    );

    assert!(check_cross_boundary_deps(&file, &store, &stored, &ctx(&store)).is_empty());
}

#[test]
fn w009_silent_when_the_name_is_ambiguous_across_boundaries() {
    let mut store = mapped_workspace();
    // A same-named `raster_ingest` in a third package: the reference cannot be
    // attributed to either without guessing.
    seed_nodes(
        &mut store,
        vec![node(
            20,
            "raster_ingest",
            "crates/eval/src/lib.rs",
            Some("eval"),
            true,
        )],
    );
    let stored = store.get_nodes_in_file(HARNESS_FILE);

    let v = check_cross_boundary_deps(&harness_reaching_into_core(), &store, &stored, &ctx(&store));
    assert!(v.is_empty());
}

#[test]
fn w009_silent_for_same_package_calls() {
    let store = mapped_workspace();
    let stored = store.get_nodes_in_file(HARNESS_FILE);
    let file = file_with(
        HARNESS_FILE,
        vec![call("prepare", 4, HARNESS_FILE)],
        vec![import("crate::setup", &["prepare"], HARNESS_FILE)],
    );

    let v = check_cross_boundary_deps(&file, &store, &stored, &ctx(&store));
    assert!(v.is_empty(), "same boundary is not a cross-boundary dep");
}

#[test]
fn w009_counts_type_refs_only_when_configured() {
    let store = mapped_workspace();
    let stored = store.get_nodes_in_file(HARNESS_FILE);
    let type_ref = Reference {
        kind: ReferenceKind::TypeRef,
        ..call("raster_ingest", 12, HARNESS_FILE)
    };
    let file = file_with(
        HARNESS_FILE,
        vec![type_ref],
        vec![import("core::ingest", &["raster_ingest"], HARNESS_FILE)],
    );

    assert!(
        check_cross_boundary_deps(&file, &store, &stored, &ctx(&store)).is_empty(),
        "type-only deps are the behaviour you want, off by default"
    );

    let counting = BoundaryContext::new(
        &store,
        &ArchitectureConfig {
            count_type_deps: true,
            deny: vec![],
        },
        true,
    );
    assert_eq!(
        check_cross_boundary_deps(&file, &store, &stored, &counting).len(),
        1
    );
}

#[test]
fn w009_skips_test_files() {
    let store = mapped_workspace();
    let path = "crates/harness/tests/it.rs";
    let file = file_with(
        path,
        vec![call("raster_ingest", 12, path)],
        vec![import("core::ingest", &["raster_ingest"], path)],
    );

    assert!(check_cross_boundary_deps(&file, &store, &[], &ctx(&store)).is_empty());
}

#[test]
fn e006_escalates_a_denied_pair_to_an_error() {
    let store = mapped_workspace();
    let stored = store.get_nodes_in_file(HARNESS_FILE);
    let denying = BoundaryContext::new(
        &store,
        &ArchitectureConfig {
            count_type_deps: false,
            deny: vec![("harness".to_string(), "core".to_string())],
        },
        true,
    );

    let v = check_cross_boundary_deps(&harness_reaching_into_core(), &store, &stored, &denying);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "E006");
    assert_eq!(v[0].severity, "ERROR");
    assert_eq!(v[0].category, "layer_violation");
    assert!(v[0].fix_hint.is_some(), "every ERROR carries a fix_hint");
}

#[test]
fn e006_ignores_the_reverse_direction() {
    let store = mapped_workspace();
    let stored = store.get_nodes_in_file(HARNESS_FILE);
    // `core -> harness` is denied; this file is the `harness -> core` direction.
    let denying = BoundaryContext::new(
        &store,
        &ArchitectureConfig {
            count_type_deps: false,
            deny: vec![("core".to_string(), "harness".to_string())],
        },
        true,
    );

    let v = check_cross_boundary_deps(&harness_reaching_into_core(), &store, &stored, &denying);
    assert_eq!(v[0].code, "W009", "deny pairs are ordered");
}

#[test]
fn w009_derives_a_directory_boundary_for_unpackaged_files() {
    let mut store = mapped_workspace();
    // A frontend tree with no declared package: its boundary is `frontend`,
    // and calling into `crates/core` still crosses one.
    seed_nodes(
        &mut store,
        vec![
            node(6, "renderPanel", "frontend/src/panel.ts", None, true),
            node(7, "loadState", "frontend/src/state.ts", None, false),
        ],
    );
    seed_call_edge(&mut store, 13, 6, 7, "frontend/src/panel.ts");
    let path = "frontend/src/panel.ts";
    let stored = store.get_nodes_in_file(path);
    let file = file_with(
        path,
        vec![call("raster_ingest", 8, path)],
        vec![import("@app/core", &["raster_ingest"], path)],
    );

    let v = check_cross_boundary_deps(&file, &store, &stored, &ctx(&store));
    assert_eq!(v.len(), 1);
    assert!(
        v[0].message.contains("frontend"),
        "directory boundary is named: {}",
        v[0].message
    );
}

/// The check runs on every compiled file, so its cost is a per-edit tax. The
/// budget is < 2ms per file; the ceiling asserted here is deliberately looser
/// because unit tests run unoptimized, and the measured value is printed so a
/// regression shows up in the test log.
#[test]
fn w009_stays_inside_the_per_file_hot_path_budget() {
    let mut store = mapped_workspace();
    // A graph with some bulk: 400 extra nodes across two packages, each with a
    // call edge, so every query has an index to actually work through.
    let mut bulk = Vec::new();
    for i in 0..400u64 {
        let pkg = if i % 2 == 0 { "core" } else { "harness" };
        let file = format!("crates/{pkg}/src/gen{}.rs", i / 8);
        bulk.push(node(
            100 + i,
            &format!("sym_{i}"),
            &file,
            Some(pkg),
            i % 3 == 0,
        ));
    }
    seed_nodes(&mut store, bulk);
    for i in 0..399u64 {
        seed_call_edge(
            &mut store,
            1000 + i,
            100 + i,
            101 + i,
            &format!("crates/core/src/gen{}.rs", i / 8),
        );
    }
    let stored = store.get_nodes_in_file(HARNESS_FILE);

    // A file with a realistic reference load: 60 imported names, 120 calls.
    let names: Vec<String> = (0..60).map(|i| format!("sym_{i}")).collect();
    let refs: Vec<Reference> = (0..120)
        .map(|i| call(&names[i % names.len()], i as u32 + 1, HARNESS_FILE))
        .collect();
    let imports = vec![import(
        "core::gen",
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
        HARNESS_FILE,
    )];
    let file = file_with(HARNESS_FILE, refs, imports);
    let context = ctx(&store);

    // Warm the page cache, then measure.
    check_cross_boundary_deps(&file, &store, &stored, &context);
    let runs = 20;
    let start = std::time::Instant::now();
    for _ in 0..runs {
        check_cross_boundary_deps(&file, &store, &stored, &context);
    }
    let per_file = start.elapsed() / runs;
    println!("W009 per-file cost: {per_file:?}");
    assert!(
        per_file < std::time::Duration::from_millis(10),
        "W009 must stay off the critical path, measured {per_file:?}"
    );
}
