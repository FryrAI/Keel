//! Unit tests for the quality metrics and the trend, driven against a real
//! in-memory SQLite graph so the traversal is under test too.

use super::trend::build_trend;
use super::*;

use keel_core::sqlite::SqliteGraphStore;
use keel_core::sqlite_quality::QualitySnapshotRow;
use keel_core::types::{
    EdgeChange, EdgeKind, FragmentCloneEntry, GraphEdge, GraphNode, NodeChange, NodeKind,
};

const BUDGET: u32 = 400;

/// A stored node. `lines` sets the span, which is what the size metric reads.
fn node(id: u64, name: &str, file: &str, kind: NodeKind, is_public: bool, lines: u32) -> GraphNode {
    GraphNode {
        complexity: 0,
        is_trivial_wrapper: false,
        in_test_context: false,
        id,
        hash: format!("h{id}"),
        kind,
        name: name.to_string(),
        signature: format!("fn {name}()"),
        file_path: file.to_string(),
        line_start: 1,
        line_end: lines,
        docstring: None,
        is_public,
        type_hints_present: true,
        has_docstring: false,
        is_associated: false,
        external_endpoints: vec![],
        previous_hashes: vec![],
        module_id: 0,
        package: None,
    }
}

/// A stored public function node carrying a real cyclomatic complexity.
fn fn_node(id: u64, name: &str, file: &str, cc: u32, lines: u32) -> GraphNode {
    GraphNode {
        complexity: cc,
        ..node(id, name, file, NodeKind::Function, true, lines)
    }
}

/// One stored fragment-clone measurement.
fn fragment(file: &str, cloned: u32, code: u32) -> FragmentCloneEntry {
    FragmentCloneEntry {
        node_hash: format!("h_{file}"),
        name: "f".to_string(),
        file_path: file.to_string(),
        line: 1,
        cloned_lines: cloned,
        code_lines: code,
    }
}

fn seed(store: &mut SqliteGraphStore, nodes: Vec<GraphNode>) {
    store
        .update_nodes(nodes.into_iter().map(NodeChange::Add).collect())
        .expect("seed nodes");
}

fn edge(store: &mut SqliteGraphStore, id: u64, src: u64, tgt: u64, kind: EdgeKind, file: &str) {
    store
        .update_edges(vec![EdgeChange::Add(GraphEdge {
            id,
            source_id: src,
            target_id: tgt,
            kind,
            file_path: file.to_string(),
            line: 2,
            confidence: 1.0,
        })])
        .expect("seed edge");
}

#[test]
fn counts_only_graded_files_over_budget() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    seed(
        &mut store,
        vec![
            // Over budget, hand-written source → counted.
            node(1, "big", "src/big.ts", NodeKind::Module, true, 900),
            // Over budget, but a test file → excluded, or the series tracks
            // fixture growth.
            node(2, "spec", "src/big.spec.ts", NodeKind::Module, true, 900),
            // Over budget, but generated → excluded.
            node(3, "gen", "baml_client/x.py", NodeKind::Module, true, 900),
            // Under budget.
            node(4, "small", "src/small.ts", NodeKind::Module, true, 40),
        ],
    );

    let m = compute_metrics(&store, BUDGET);
    assert_eq!(m.files_over_budget, 1);
    assert_eq!(m.version, METRICS_VERSION);
}

#[test]
fn dead_private_fns_excludes_public_entrypoints_and_used_functions() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    seed(
        &mut store,
        vec![
            node(1, "mod_a", "src/a.ts", NodeKind::Module, true, 10),
            // Private, no callers → dead.
            node(2, "orphan", "src/a.ts", NodeKind::Function, false, 5),
            // Private, but reached through a `uses` edge (callback) → alive.
            node(3, "callback", "src/a.ts", NodeKind::Function, false, 5),
            // Public → not this metric's business.
            node(4, "exported", "src/a.ts", NodeKind::Function, true, 5),
            // Entrypoint name → never dead.
            node(5, "main", "src/a.ts", NodeKind::Function, false, 5),
            // Underscore-prefixed → deliberately unused.
            node(6, "_scratch", "src/a.ts", NodeKind::Function, false, 5),
            // The caller.
            node(7, "run", "src/a.ts", NodeKind::Function, true, 5),
        ],
    );
    edge(&mut store, 1, 7, 3, EdgeKind::Uses, "src/a.ts");

    let m = compute_metrics(&store, BUDGET);
    assert_eq!(m.dead_private_fns, 1);
}

#[test]
fn dead_private_fns_ignores_test_files() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    seed(
        &mut store,
        vec![
            node(1, "spec", "src/a.spec.ts", NodeKind::Module, true, 10),
            node(2, "helper", "src/a.spec.ts", NodeKind::Function, false, 5),
        ],
    );
    assert_eq!(compute_metrics(&store, BUDGET).dead_private_fns, 0);
}

#[test]
fn surface_metrics_track_exports_files_and_single_consumer_helpers() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    seed(
        &mut store,
        vec![
            node(1, "mod_a", "src/a.ts", NodeKind::Module, true, 100),
            node(2, "mod_b", "src/b.ts", NodeKind::Module, true, 100),
            node(3, "spec", "src/a.spec.ts", NodeKind::Module, true, 900),
            node(4, "api", "src/a.ts", NodeKind::Function, true, 5),
            node(5, "helper", "src/a.ts", NodeKind::Function, false, 5),
            node(6, "caller", "src/b.ts", NodeKind::Function, true, 5),
            node(7, "other", "src/b.ts", NodeKind::Function, true, 5),
            node(8, "shared", "src/a.ts", NodeKind::Function, false, 5),
            node(9, "fixture", "src/a.spec.ts", NodeKind::Function, true, 5),
        ],
    );
    edge(&mut store, 1, 6, 5, EdgeKind::Calls, "src/b.ts");
    edge(&mut store, 2, 6, 8, EdgeKind::Calls, "src/b.ts");
    edge(&mut store, 3, 7, 8, EdgeKind::Uses, "src/b.ts");

    let metrics = compute_metrics(&store, BUDGET);

    assert_eq!(metrics.source_file_count, 2);
    assert_eq!(metrics.exported_symbol_count, 3);
    assert_eq!(metrics.single_consumer_helper_count, 1);
    assert_eq!(metrics.exported_symbols_per_kloc, 15.0);
}

#[test]
fn cross_module_edge_ratio_counts_edges_that_leave_their_file() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    seed(
        &mut store,
        vec![
            node(1, "mod_a", "src/a.ts", NodeKind::Module, true, 10),
            node(2, "mod_b", "src/b.ts", NodeKind::Module, true, 10),
            node(3, "a1", "src/a.ts", NodeKind::Function, true, 5),
            node(4, "a2", "src/a.ts", NodeKind::Function, true, 5),
            node(5, "b1", "src/b.ts", NodeKind::Function, true, 5),
        ],
    );
    // One in-file call, one cross-file call → 0.5.
    edge(&mut store, 1, 3, 4, EdgeKind::Calls, "src/a.ts");
    edge(&mut store, 2, 3, 5, EdgeKind::Calls, "src/a.ts");

    let m = compute_metrics(&store, BUDGET);
    assert!((m.cross_module_edge_ratio - 0.5).abs() < 1e-9);
}

#[test]
fn cycle_count_matches_the_audit_population() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    seed(
        &mut store,
        vec![
            node(1, "mod_a", "src/a.ts", NodeKind::Module, true, 10),
            node(2, "mod_b", "src/b.ts", NodeKind::Module, true, 10),
            node(3, "a1", "src/a.ts", NodeKind::Function, true, 5),
            node(4, "b1", "src/b.ts", NodeKind::Function, true, 5),
        ],
    );
    edge(&mut store, 1, 3, 4, EdgeKind::Calls, "src/a.ts");
    edge(&mut store, 2, 4, 3, EdgeKind::Calls, "src/b.ts");

    assert_eq!(compute_metrics(&store, BUDGET).cycle_count, 1);

    // The same cycle between two Rust files is legal, idiomatic Rust and the
    // audit does not report it — the metric must not either.
    let mut rust = SqliteGraphStore::in_memory().unwrap();
    seed(
        &mut rust,
        vec![
            node(1, "mod_a", "src/a.rs", NodeKind::Module, true, 10),
            node(2, "mod_b", "src/b.rs", NodeKind::Module, true, 10),
            node(3, "a1", "src/a.rs", NodeKind::Function, true, 5),
            node(4, "b1", "src/b.rs", NodeKind::Function, true, 5),
        ],
    );
    edge(&mut rust, 1, 3, 4, EdgeKind::Calls, "src/a.rs");
    edge(&mut rust, 2, 4, 3, EdgeKind::Calls, "src/b.rs");
    assert_eq!(compute_metrics(&rust, BUDGET).cycle_count, 0);
}

#[test]
fn high_cc_mass_share_weights_hot_functions_and_skips_ungraded_files() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    seed(
        &mut store,
        vec![
            node(1, "mod_a", "src/a.ts", NodeKind::Module, true, 10),
            // Above the threshold: 20 · √100 = 200 of hot mass.
            fn_node(2, "hot", "src/a.ts", 20, 100),
            // At/below it: 5 · √100 = 50 of ordinary mass.
            fn_node(3, "cool", "src/a.ts", 5, 100),
            // Hot, but generated and test code respectively — neither may move
            // a metric about hand-written maintainability.
            fn_node(4, "gen", "baml_client/x.py", 40, 100),
            fn_node(5, "spec_hot", "src/a.spec.ts", 40, 100),
        ],
    );
    assert!((compute_metrics(&store, BUDGET).high_cc_mass_share - 0.8).abs() < 1e-9);
}

#[test]
fn propagation_cost_counts_transitively_reachable_module_pairs() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    seed(
        &mut store,
        vec![
            node(1, "mod_a", "src/a.ts", NodeKind::Module, true, 10),
            node(2, "mod_b", "src/b.ts", NodeKind::Module, true, 10),
            node(3, "mod_c", "src/c.ts", NodeKind::Module, true, 10),
            fn_node(4, "a1", "src/a.ts", 1, 5),
            fn_node(5, "b1", "src/b.ts", 1, 5),
            fn_node(6, "c1", "src/c.ts", 1, 5),
        ],
    );
    // A chain a → b → c: a reaches 2, b reaches 1, c reaches 0 → 3/3² = 0.33.
    edge(&mut store, 1, 4, 5, EdgeKind::Calls, "src/a.ts");
    edge(&mut store, 2, 5, 6, EdgeKind::Calls, "src/b.ts");

    assert!((compute_metrics(&store, BUDGET).propagation_cost - 0.33).abs() < 1e-9);
}

#[test]
fn clone_loc_ratio_divides_cloned_lines_by_measured_code_lines() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_fragment_clones(vec![
            fragment("src/a.ts", 10, 40),
            fragment("src/b.ts", 0, 60),
            // Generated and test code are excluded by the scan, and excluded
            // again here — a stale row written by an older map must not move a
            // metric about hand-written maintainability.
            fragment("baml_client/x.py", 50, 50),
            fragment("src/a.spec.ts", 50, 50),
        ])
        .expect("store measurements");

    assert!((compute_metrics(&store, BUDGET).clone_loc_ratio - 0.1).abs() < 1e-9);
}

/// A graph mapped before fragment measurements existed carries no rows at all.
/// The metric must read 0 rather than divide by zero — and the trend omits it
/// entirely, see `trend_omits_metrics_that_a_point_in_the_window_predates`.
#[test]
fn clone_loc_ratio_is_zero_when_nothing_was_measured() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    seed(
        &mut store,
        vec![node(1, "mod_a", "src/a.ts", NodeKind::Module, true, 10)],
    );
    assert_eq!(compute_metrics(&store, BUDGET).clone_loc_ratio, 0.0);
}

#[test]
fn empty_graph_measures_zero_rather_than_dividing_by_zero() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let m = compute_metrics(&store, BUDGET);
    assert_eq!(m.files_over_budget, 0);
    assert_eq!(m.cycle_count, 0);
    assert_eq!(m.dead_private_fns, 0);
    assert_eq!(m.cross_module_edge_ratio, 0.0);
    assert_eq!(m.high_cc_mass_share, 0.0);
    assert_eq!(m.propagation_cost, 0.0);
    assert_eq!(m.clone_loc_ratio, 0.0);
    assert_eq!(m.source_file_count, 0);
    assert_eq!(m.exported_symbol_count, 0);
    assert_eq!(m.single_consumer_helper_count, 0);
    assert_eq!(m.exported_symbols_per_kloc, 0.0);
}

/// A file with two stored `module` rows (hash-salt collisions do happen) is one
/// file, not two — the store deduplicates before the metric ever sees it.
#[test]
fn duplicate_module_rows_count_once() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    seed(
        &mut store,
        vec![
            node(1, "big", "src/big.ts", NodeKind::Module, true, 900),
            node(2, "big", "src/big.ts", NodeKind::Module, true, 900),
        ],
    );
    assert_eq!(compute_metrics(&store, BUDGET).files_over_budget, 1);
}

// --- trend ---------------------------------------------------------------

fn row(id: i64, sha: &str, blob: &str) -> QualitySnapshotRow {
    QualitySnapshotRow {
        id,
        captured_at: format!("2026-08-0{id} 10:00:00"),
        commit_sha: Some(sha.to_string()),
        metrics: blob.to_string(),
    }
}

fn metrics_blob(over: u32, cycles: u32, dead: u32, ratio: f64) -> String {
    QualityMetrics {
        version: METRICS_VERSION,
        files_over_budget: over,
        cycle_count: cycles,
        dead_private_fns: dead,
        cross_module_edge_ratio: ratio,
        high_cc_mass_share: 0.4,
        propagation_cost: 0.2,
        clone_loc_ratio: 0.1,
        source_file_count: 10,
        exported_symbol_count: 20,
        single_consumer_helper_count: 3,
        exported_symbols_per_kloc: 2.5,
    }
    .to_json()
}

#[test]
fn trend_reports_direction_and_per_commit_attribution() {
    let rows = vec![
        row(1, "aaaaaaa1", &metrics_blob(10, 2, 5, 0.30)),
        row(2, "bbbbbbb2", &metrics_blob(16, 2, 4, 0.31)),
        row(3, "ccccccc3", &metrics_blob(18, 1, 4, 0.34)),
    ];
    let trend = build_trend(&rows);
    assert!(trend.refused.is_none());
    assert_eq!(trend.points.len(), 3);
    assert_eq!(trend.metrics.len(), 11);
    assert!(trend.omitted.is_empty());

    let over = &trend.metrics[0];
    assert_eq!(over.name, "files_over_budget");
    assert_eq!(over.first, 10.0);
    assert_eq!(over.last, 18.0);
    assert_eq!(over.direction, "worsening");
    let step = over.largest_step.as_ref().expect("attribution");
    assert_eq!(step.commit.as_deref(), Some("bbbbbbb2"));
    assert_eq!(step.delta, 6.0);

    assert_eq!(trend.metrics[1].direction, "improving"); // cycle_count 2 → 1
    assert_eq!(trend.metrics[2].direction, "improving"); // dead 5 → 4

    // The ratio is trend-only: a direction, never a judgement.
    let ratio = &trend.metrics[3];
    assert_eq!(ratio.name, "cross_module_edge_ratio");
    assert!(!ratio.judged);
    assert_eq!(ratio.direction, "up");
}

#[test]
fn trend_refuses_to_compare_across_metrics_versions() {
    let future = format!(
        "{{\"version\":{},\"files_over_budget\":1}}",
        METRICS_VERSION + 1
    );
    let rows = vec![
        row(1, "aaaaaaa1", &metrics_blob(10, 0, 0, 0.1)),
        row(2, "bbbbbbb2", &future),
    ];
    let trend = build_trend(&rows);
    let refused = trend.refused.expect("comparison must be refused");
    assert!(refused.contains(&format!("v{}", METRICS_VERSION + 1)));
    assert!(
        trend.metrics.is_empty(),
        "a refused comparison reports no direction"
    );
    // The readable point is still shown, so the reader can narrow the window.
    assert_eq!(trend.points.len(), 1);
}

/// A metric added without a version bump reads back as a defaulted `0.0` from
/// blobs written before it existed. Trending that would draw a step out of
/// "not measured" — the exact silent re-baselining `refused` exists to
/// prevent — so the metric is omitted and named instead, while every metric
/// both points did measure trends as usual.
#[test]
fn trend_omits_metrics_that_a_point_in_the_window_predates() {
    let legacy = format!(
        "{{\"version\":{METRICS_VERSION},\"files_over_budget\":10,\"cycle_count\":2,\
          \"dead_private_fns\":5,\"cross_module_edge_ratio\":0.3}}"
    );
    let rows = vec![
        row(1, "aaaaaaa1", &legacy),
        row(2, "bbbbbbb2", &metrics_blob(18, 1, 4, 0.34)),
    ];
    let trend = build_trend(&rows);

    assert!(trend.refused.is_none());
    assert_eq!(trend.points.len(), 2);
    let names: Vec<&str> = trend.metrics.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "files_over_budget",
            "cycle_count",
            "dead_private_fns",
            "cross_module_edge_ratio"
        ]
    );
    assert_eq!(
        trend.omitted,
        vec![
            "high_cc_mass_share",
            "propagation_cost",
            "clone_loc_ratio",
            "source_file_count",
            "exported_symbol_count",
            "single_consumer_helper_count",
            "exported_symbols_per_kloc",
        ]
    );
    assert_eq!(trend.metrics[0].first, 10.0);
    assert_eq!(trend.metrics[0].last, 18.0);
}

#[test]
fn trend_of_a_single_point_reports_no_direction() {
    let trend = build_trend(&[row(1, "aaaaaaa1", &metrics_blob(3, 0, 1, 0.2))]);
    assert!(trend.refused.is_none());
    assert_eq!(trend.points.len(), 1);
    assert!(trend.metrics.is_empty());
}

#[test]
fn unreadable_rows_are_counted_not_guessed() {
    let rows = vec![
        row(1, "aaaaaaa1", "not json at all"),
        row(2, "bbbbbbb2", &metrics_blob(1, 0, 0, 0.1)),
        row(3, "ccccccc3", &metrics_blob(2, 0, 0, 0.1)),
    ];
    let trend = build_trend(&rows);
    assert_eq!(trend.unreadable, 1);
    assert_eq!(trend.points.len(), 2);
    assert!(trend.refused.is_none());
}

#[test]
fn flat_series_reads_as_flat() {
    let rows = vec![
        row(1, "aaaaaaa1", &metrics_blob(4, 1, 2, 0.25)),
        row(2, "bbbbbbb2", &metrics_blob(4, 1, 2, 0.25)),
    ];
    let trend = build_trend(&rows);
    for m in &trend.metrics {
        assert_eq!(m.direction, "flat", "{} should be flat", m.name);
        assert!(m.largest_step.is_none());
    }
}
