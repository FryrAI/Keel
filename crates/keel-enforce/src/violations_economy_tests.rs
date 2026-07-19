use std::collections::{HashMap, HashSet};

use super::*;
use crate::test_fixtures::{
    call_ref, definition, file_index, file_index_with_refs, function_node, node_for_definition,
    test_context_definition, ECON_BODY,
};
use keel_core::sqlite::SqliteGraphStore;
use keel_core::types::{EdgeChange, GraphEdge};

fn calls_edge(id: u64, src: u64, tgt: u64) -> EdgeChange {
    EdgeChange::Add(GraphEdge {
        id,
        source_id: src,
        target_id: tgt,
        kind: EdgeKind::Calls,
        file_path: "src/b.rs".to_string(),
        line: 3,
        confidence: 1.0,
    })
}

// --- W005 dead_code ---

#[test]
fn w005_silent_for_unstored_function() {
    // A def with no stored node has no edge history — and in the one-file-
    // per-edit hook flow its caller often isn't written yet. Never flag it;
    // it's caught on the compile after the next `keel map`.
    let store = SqliteGraphStore::in_memory().unwrap();
    let file = file_index("src/a.rs", vec![definition("orphan", "src/a.rs", false)]);
    assert!(check_dead_code(&file, &store, &[], &HashSet::new()).is_empty());
}

#[test]
fn w005_fires_on_stored_function_with_zero_edges() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let def = definition("orphan", "src/a.rs", false);
    store.insert_node(&node_for_definition(1, &def)).unwrap();
    let file = file_index("src/a.rs", vec![def]);
    let stored = store.get_nodes_in_file("src/a.rs");

    let v = check_dead_code(&file, &store, &stored, &HashSet::new());
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "W005");
    assert_eq!(v[0].severity, "WARNING");
    assert!((v[0].confidence - 0.7).abs() < f64::EPSILON);
    assert_eq!(v[0].hash, stored[0].hash, "stored hash is reported");
}

#[test]
fn w005_silent_for_public_entrypoint_underscore_and_qualified() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let defs = vec![
        definition("exported", "src/a.rs", true),
        definition("main", "src/a.rs", false),
        definition("_intentional", "src/a.rs", false),
        definition("Widget.render", "src/a.rs", false),
    ];
    let mut stored = Vec::new();
    for (i, def) in defs.iter().enumerate() {
        let node = node_for_definition(i as u64 + 1, def);
        store.insert_node(&node).unwrap();
        stored.push(node);
    }
    let file = file_index("src/a.rs", defs);
    assert!(check_dead_code(&file, &store, &stored, &HashSet::new()).is_empty());
}

#[test]
fn w005_silent_when_referenced_in_batch() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let def = definition("helper", "src/a.rs", false);
    store.insert_node(&node_for_definition(1, &def)).unwrap();
    let stored = store.get_nodes_in_file("src/a.rs");
    let file = file_index("src/a.rs", vec![def]);

    let mut referenced = HashSet::new();
    referenced.insert("helper".to_string());
    assert!(check_dead_code(&file, &store, &stored, &referenced).is_empty());
}

#[test]
fn w005_respects_graph_call_edges() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let def = definition("used", "src/a.rs", false);
    store.insert_node(&node_for_definition(1, &def)).unwrap();
    store
        .insert_node(&function_node(2, "callerHash000", "caller", "src/b.rs"))
        .unwrap();
    store.update_edges(vec![calls_edge(1, 2, 1)]).unwrap();

    let stored = store.get_nodes_in_file("src/a.rs");
    let file = file_index("src/a.rs", vec![def]);
    assert!(check_dead_code(&file, &store, &stored, &HashSet::new()).is_empty());
}

#[test]
fn w005_same_named_siblings_use_hash_to_pick_the_right_node() {
    // Two impl blocks in one file, both defining `parse`. Foo::parse is
    // called; Bar::parse is dead. Hash matching must consult the right
    // node's edges — first-name-wins would either miss the dead one or flag
    // the live one depending on declaration order.
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let live_def = definition("parse", "src/a.rs", false);
    let mut dead_def = definition("parse", "src/a.rs", false);
    dead_def.body_text = format!("{ECON_BODY}\nextra_dead_variant()");
    dead_def.line_start = 30;
    dead_def.line_end = 40;

    let live_node = node_for_definition(1, &live_def);
    let mut dead_node = node_for_definition(2, &dead_def);
    dead_node.line_start = 30;
    dead_node.line_end = 40;
    store.insert_node(&live_node).unwrap();
    store.insert_node(&dead_node).unwrap();
    store
        .insert_node(&function_node(3, "callerHash000", "caller", "src/b.rs"))
        .unwrap();
    store.update_edges(vec![calls_edge(1, 3, 1)]).unwrap();

    let stored = store.get_nodes_in_file("src/a.rs");
    let file = file_index("src/a.rs", vec![live_def, dead_def.clone()]);
    let v = check_dead_code(&file, &store, &stored, &HashSet::new());
    assert_eq!(v.len(), 1, "only the dead sibling fires: {v:?}");
    assert_eq!(v[0].line, dead_def.line_start);
    assert_eq!(
        v[0].hash, dead_node.hash,
        "the DEAD node's hash is reported"
    );
}

#[test]
fn w005_silent_in_test_and_bench_files() {
    let store = SqliteGraphStore::in_memory().unwrap();
    for path in ["tests/util.rs", "benches/perf.rs"] {
        let def = definition("fixture", path, false);
        let node = node_for_definition(1, &def);
        let file = file_index(path, vec![def]);
        assert!(
            check_dead_code(&file, &store, &[node], &HashSet::new()).is_empty(),
            "{path} must be exempt"
        );
    }
}

#[test]
fn w005_silent_for_test_context_definition_but_fires_on_dead_sibling() {
    // A helper inside a co-located `#[cfg(test)] mod tests` (marked at parse
    // time) is exempt even without a `test_` prefix, while a genuinely dead
    // non-test function in the SAME file still fires W005.
    let store = SqliteGraphStore::in_memory().unwrap();
    let test_helper = test_context_definition("helper_no_prefix", "src/a.rs");
    let dead = definition("dead_one", "src/a.rs", false);
    store
        .insert_node(&node_for_definition(1, &test_helper))
        .unwrap();
    store.insert_node(&node_for_definition(2, &dead)).unwrap();
    let stored = store.get_nodes_in_file("src/a.rs");

    let file = file_index("src/a.rs", vec![test_helper, dead.clone()]);
    let v = check_dead_code(&file, &store, &stored, &HashSet::new());
    assert_eq!(v.len(), 1, "only the non-test dead fn fires: {v:?}");
    assert_eq!(v[0].line, dead.line_start);
    assert!(v[0].message.contains("dead_one"));
}

// --- W006 duplicate_implementation ---

#[test]
fn w006_fires_on_batch_local_duplicate() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut seen = HashMap::new();
    let file_a = file_index("src/a.rs", vec![definition("calc_a", "src/a.rs", true)]);
    let file_b = file_index("src/b.rs", vec![definition("calc_b", "src/b.rs", true)]);

    assert!(check_duplicate_implementation(&file_a, &store, &mut seen).is_empty());
    let v = check_duplicate_implementation(&file_b, &store, &mut seen);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "W006");
    assert!(v[0].message.contains("calc_a"));
    assert!(v[0].fix_hint.as_deref().unwrap().contains("src/a.rs"));
}

#[test]
fn w006_ignores_whitespace_differences() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut seen = HashMap::new();
    let mut reformatted = definition("calc_b", "src/b.rs", true);
    reformatted.body_text = ECON_BODY.replace(' ', "  ").replace('\n', "\n\n");

    let file_a = file_index("src/a.rs", vec![definition("calc_a", "src/a.rs", true)]);
    let file_b = file_index("src/b.rs", vec![reformatted]);
    assert!(check_duplicate_implementation(&file_a, &store, &mut seen).is_empty());
    assert_eq!(
        check_duplicate_implementation(&file_b, &store, &mut seen).len(),
        1
    );
}

#[test]
fn w006_silent_for_trivial_bodies() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut seen = HashMap::new();
    let mut a = definition("get_a", "src/a.rs", true);
    a.body_text = "self.a".to_string();
    let mut b = definition("get_b", "src/b.rs", true);
    b.body_text = "self.a".to_string();

    assert!(
        check_duplicate_implementation(&file_index("src/a.rs", vec![a]), &store, &mut seen)
            .is_empty()
    );
    assert!(
        check_duplicate_implementation(&file_index("src/b.rs", vec![b]), &store, &mut seen)
            .is_empty()
    );
}

#[test]
fn w006_silent_for_same_file_siblings() {
    // Identical siblings within ONE file are usually a deliberate dispatch
    // pattern (trait impls, format_* fan-out) — only cross-file copies warn.
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut seen = HashMap::new();
    let mut second = definition("calc_b", "src/a.rs", true);
    second.line_start = 30;
    let file = file_index(
        "src/a.rs",
        vec![definition("calc_a", "src/a.rs", true), second],
    );
    assert!(check_duplicate_implementation(&file, &store, &mut seen).is_empty());
}

#[test]
fn w006_fires_on_graph_index_match() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![keel_core::types::BodyIndexEntry {
            body_hash: keel_core::hash::compute_body_hash(ECON_BODY),
            node_hash: "origHash0000".to_string(),
            name: "original".to_string(),
            file_path: "src/orig.rs".to_string(),
            line: 42,
        }])
        .unwrap();

    let file = file_index(
        "src/copy.rs",
        vec![definition("copied", "src/copy.rs", true)],
    );
    let mut seen = HashMap::new();
    let v = check_duplicate_implementation(&file, &store, &mut seen);
    assert_eq!(v.len(), 1);
    assert!(v[0].message.contains("original"));
    assert!(v[0].message.contains("src/orig.rs"));
}

#[test]
fn w006_ignores_graph_matches_from_own_file() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![keel_core::types::BodyIndexEntry {
            body_hash: keel_core::hash::compute_body_hash(ECON_BODY),
            node_hash: "selfHash0000".to_string(),
            name: "calc".to_string(),
            file_path: "src/a.rs".to_string(),
            line: 10,
        }])
        .unwrap();

    let file = file_index("src/a.rs", vec![definition("calc", "src/a.rs", true)]);
    let mut seen = HashMap::new();
    assert!(check_duplicate_implementation(&file, &store, &mut seen).is_empty());
}

// --- W007 oversized_file ---

#[test]
fn w007_fires_when_over_budget_and_growing() {
    let mut def = definition("big", "src/huge.rs", true);
    def.line_end = 450;
    let file = file_index("src/huge.rs", vec![def]);
    let v = check_oversized_file(&file, &[], 400);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "W007");
    assert!(v[0].message.contains("450"));
}

#[test]
fn w007_silent_when_shrinking_even_if_still_over() {
    let mut def = definition("big", "src/huge.rs", true);
    def.line_end = 450;
    let file = file_index("src/huge.rs", vec![def]);
    let mut stored = function_node(1, "bigHash00000", "big", "src/huge.rs");
    stored.line_end = 500; // was bigger before this change
    assert!(check_oversized_file(&file, &[stored], 400).is_empty());
}

#[test]
fn w007_module_node_line_count_does_not_mask_growth() {
    // The stored Module node records the WHOLE file's line count (defs +
    // trailing footer). Comparing the def-derived extent against it would
    // hide genuine growth whenever the footer is longer than the increment.
    let mut def = definition("appended", "src/huge.rs", true);
    def.line_end = 597; // last def after the agent appended a function

    let mut module = function_node(1, "modHash00000", "src/huge.rs", "src/huge.rs");
    module.kind = NodeKind::Module;
    module.line_end = 600; // true file length at map time (footer included)
    let mut old_def = function_node(2, "defHash00000", "appended", "src/huge.rs");
    old_def.line_end = 585; // last def at map time

    let file = file_index("src/huge.rs", vec![def]);
    let v = check_oversized_file(&file, &[module, old_def], 400);
    assert_eq!(v.len(), 1, "growth 585 -> 597 must fire despite module=600");
}

#[test]
fn w007_silent_under_budget() {
    let file = file_index("src/small.rs", vec![definition("f", "src/small.rs", true)]);
    assert!(check_oversized_file(&file, &[], 400).is_empty());
}

// --- batch_reference_names ---

#[test]
fn batch_reference_names_include_qualified_suffix() {
    let file = file_index_with_refs(
        "src/a.rs",
        vec![],
        vec![call_ref("widget.render", "src/a.rs")],
    );
    let names = batch_reference_names(&[file]);
    assert!(names.contains("widget.render"));
    assert!(names.contains("render"));
}
