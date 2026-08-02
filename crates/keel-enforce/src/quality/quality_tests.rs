//! Unit tests for the quality metrics and the trend, driven against a real
//! in-memory SQLite graph so the traversal is under test too.

use super::trend::build_trend;
use super::*;

use keel_core::sqlite::SqliteGraphStore;
use keel_core::sqlite_quality::QualitySnapshotRow;
use keel_core::types::{EdgeChange, EdgeKind, GraphEdge, GraphNode, NodeChange, NodeKind};

const BUDGET: u32 = 400;

/// A stored node. `lines` sets the span, which is what the size metric reads.
fn node(id: u64, name: &str, file: &str, kind: NodeKind, is_public: bool, lines: u32) -> GraphNode {
    GraphNode {
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
fn empty_graph_measures_zero_rather_than_dividing_by_zero() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let m = compute_metrics(&store, BUDGET);
    assert_eq!(m.files_over_budget, 0);
    assert_eq!(m.cycle_count, 0);
    assert_eq!(m.dead_private_fns, 0);
    assert_eq!(m.cross_module_edge_ratio, 0.0);
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
    assert_eq!(trend.metrics.len(), 4);

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
