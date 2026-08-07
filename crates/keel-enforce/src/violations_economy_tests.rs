use std::collections::{HashMap, HashSet};

use super::*;
use crate::test_fixtures::{
    auto_invoked_definition, call_ref, decorated_definition, definition, file_index,
    file_index_with_refs, function_node, keep_marker_definition, node_for_definition,
    test_context_definition, trait_context_definition, ECON_BODY,
};
use keel_core::sqlite::SqliteGraphStore;
use keel_core::types::{EdgeChange, GraphEdge};

fn calls_edge(id: u64, src: u64, tgt: u64) -> EdgeChange {
    edge(id, src, tgt, EdgeKind::Calls)
}

/// A function named as a value (callback) rather than invoked.
fn uses_edge(id: u64, src: u64, tgt: u64) -> EdgeChange {
    edge(id, src, tgt, EdgeKind::Uses)
}

fn edge(id: u64, src: u64, tgt: u64, kind: EdgeKind) -> EdgeChange {
    EdgeChange::Add(GraphEdge {
        id,
        source_id: src,
        target_id: tgt,
        kind,
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
fn w005_go_auto_invoked_entrypoints_not_dead() {
    // Go's Tier-2 pass marks `init`/`main`/`TestMain` as auto-invoked (run by
    // the runtime or test harness) — never dead, even with zero call edges.
    // W005 keys off that parser flag, not a language-from-path special case.
    for name in ["init", "main", "TestMain"] {
        let store = SqliteGraphStore::in_memory().unwrap();
        let def = auto_invoked_definition(name, "src/app.go");
        store.insert_node(&node_for_definition(1, &def)).unwrap();
        let file = file_index("src/app.go", vec![def]);
        let stored = store.get_nodes_in_file("src/app.go");
        assert!(
            check_dead_code(&file, &store, &stored, &HashSet::new()).is_empty(),
            "Go `{name}` (is_auto_invoked) must be exempt from W005"
        );
    }
}

#[test]
fn w005_rust_init_is_not_exempt() {
    // The Go exemption must NOT leak into other languages: a private, uncalled
    // `init` in Rust is ordinary dead code.
    let store = SqliteGraphStore::in_memory().unwrap();
    let def = definition("init", "src/a.rs", false);
    store.insert_node(&node_for_definition(1, &def)).unwrap();
    let file = file_index("src/a.rs", vec![def]);
    let stored = store.get_nodes_in_file("src/a.rs");
    let v = check_dead_code(&file, &store, &stored, &HashSet::new());
    assert_eq!(v.len(), 1, "Rust `init` is not an entrypoint: {v:?}");
    assert_eq!(v[0].code, "W005");
}

#[test]
fn w005_test_prefixed_name_exempt_via_context_flag_not_name() {
    // The old blind `name.starts_with("test_")` skip is gone. A Python
    // `def test_helper()` is now exempt because its parser-set in_test_context
    // is true — proving the deleted string branch's behavior is fully covered
    // by precise AST marking.
    let store = SqliteGraphStore::in_memory().unwrap();
    let def = test_context_definition("test_helper", "src/mod_a.py");
    store.insert_node(&node_for_definition(1, &def)).unwrap();
    let file = file_index("src/mod_a.py", vec![def]);
    let stored = store.get_nodes_in_file("src/mod_a.py");
    assert!(
        check_dead_code(&file, &store, &stored, &HashSet::new()).is_empty(),
        "`test_helper` with in_test_context must be exempt from W005"
    );

    // Same name WITHOUT the context flag is no longer given a free pass by the
    // name alone — the blind prefix check is truly removed.
    let bare = definition("test_helper", "src/mod_a.py", false);
    store.insert_node(&node_for_definition(2, &bare)).unwrap();
    let file = file_index("src/mod_a.py", vec![bare]);
    let stored = store.get_nodes_in_file("src/mod_a.py");
    let v = check_dead_code(&file, &store, &stored, &HashSet::new());
    assert_eq!(v.len(), 1, "test_-prefix alone no longer exempts: {v:?}");
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
fn w005_respects_graph_uses_edges() {
    // A function whose only usage is a value reference (passed as a callback
    // from another file) is used, not dead — the `uses` edge is the only
    // evidence, since nothing calls it by name.
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let def = definition("callback", "src/a.rs", false);
    store.insert_node(&node_for_definition(1, &def)).unwrap();
    store
        .insert_node(&function_node(2, "callerHash000", "wiring", "src/b.rs"))
        .unwrap();
    store.update_edges(vec![uses_edge(1, 2, 1)]).unwrap();

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
fn w005_silent_for_decorated_function() {
    // A Python `@register("evt")`-decorated function is handed to the
    // decorator, not called by name — the parser's `is_decorated` flag must
    // exempt it from W005 even with zero call edges.
    let store = SqliteGraphStore::in_memory().unwrap();
    let def = decorated_definition("handler", "src/handlers.py");
    store.insert_node(&node_for_definition(1, &def)).unwrap();
    let file = file_index("src/handlers.py", vec![def]);
    let stored = store.get_nodes_in_file("src/handlers.py");
    assert!(
        check_dead_code(&file, &store, &stored, &HashSet::new()).is_empty(),
        "a decorated function must be exempt from W005"
    );
}

#[test]
fn w005_silent_for_keep_marker_definition() {
    // `keel:keep` is the language-agnostic escape hatch for dynamic dispatch
    // (`globals()[name]()`) that no exemption rule can see through.
    let store = SqliteGraphStore::in_memory().unwrap();
    let def = keep_marker_definition("dynamic_handler", "src/dispatch.py");
    store.insert_node(&node_for_definition(1, &def)).unwrap();
    let file = file_index("src/dispatch.py", vec![def]);
    let stored = store.get_nodes_in_file("src/dispatch.py");
    assert!(
        check_dead_code(&file, &store, &stored, &HashSet::new()).is_empty(),
        "a keel:keep-marked function must be exempt from W005"
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

/// The batch-wide state `check_duplicate_implementation` threads through one
/// compile: the two seen-fingerprint maps (Type-1, Type-2) and the two
/// trait-context maps.
#[derive(Default)]
struct DupState {
    seen: HashMap<String, (String, String, u32)>,
    seen_t2: HashMap<String, (String, String, u32)>,
    trait_bodies: HashMap<String, HashSet<String>>,
    trait_bodies_t2: HashMap<String, HashSet<String>>,
}

impl DupState {
    /// Batch state carrying the trait-context maps the real batch would build
    /// for `files` — the only way to exercise the trait exemption honestly.
    fn for_files(files: &[FileIndex]) -> Self {
        let (trait_bodies, trait_bodies_t2) = batch_trait_context_bodies(files);
        Self {
            trait_bodies,
            trait_bodies_t2,
            ..Default::default()
        }
    }

    fn check(&mut self, file: &FileIndex, store: &dyn GraphStore) -> Vec<Violation> {
        check_duplicate_implementation(
            file,
            store,
            &mut self.seen,
            &self.trait_bodies,
            &mut self.seen_t2,
            &self.trait_bodies_t2,
        )
    }
}

/// A body over the Type-2 floor, and a copy with every local renamed and one
/// literal changed — Type-1 sees two different functions, Type-2 sees one.
const T2_BODY: &str = "let mut selected = Vec::new();\n\
                       for item in items.iter() {\n\
                           if item.active && item.count > 0 {\n\
                               selected.push(item.name.clone());\n\
                           }\n\
                       }\n\
                       selected.sort();\n\
                       selected";
const T2_BODY_RENAMED: &str = "let mut chosen = Vec::new();\n\
                               for row in rows.iter() {\n\
                                   if row.enabled && row.total > 3 {\n\
                                       chosen.push(row.label.clone());\n\
                                   }\n\
                               }\n\
                               chosen.sort();\n\
                               chosen";

fn definition_with_body(name: &str, file: &str, body: &str) -> Definition {
    Definition {
        body_text: body.to_string(),
        ..definition(name, file, true)
    }
}

fn t2_hash_of(body: &str) -> String {
    keel_core::hash_t2::compute_t2_hash(body, "rust")
}

#[test]
fn w006_fires_on_batch_local_duplicate() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut state = DupState::default();
    let file_a = file_index("src/a.rs", vec![definition("calc_a", "src/a.rs", true)]);
    let file_b = file_index("src/b.rs", vec![definition("calc_b", "src/b.rs", true)]);

    assert!(state.check(&file_a, &store).is_empty());
    let v = state.check(&file_b, &store);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].code, "W006");
    assert!(v[0].message.contains("calc_a"));
    assert!(v[0].fix_hint.as_deref().unwrap().contains("src/a.rs"));
}

#[test]
fn w006_ignores_whitespace_differences() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut state = DupState::default();
    let mut reformatted = definition("calc_b", "src/b.rs", true);
    reformatted.body_text = ECON_BODY.replace(' ', "  ").replace('\n', "\n\n");

    let file_a = file_index("src/a.rs", vec![definition("calc_a", "src/a.rs", true)]);
    let file_b = file_index("src/b.rs", vec![reformatted]);
    assert!(state.check(&file_a, &store).is_empty());
    assert_eq!(state.check(&file_b, &store).len(), 1);
}

#[test]
fn w006_silent_for_trivial_bodies() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut state = DupState::default();
    let mut a = definition("get_a", "src/a.rs", true);
    a.body_text = "self.a".to_string();
    let mut b = definition("get_b", "src/b.rs", true);
    b.body_text = "self.a".to_string();

    assert!(state
        .check(&file_index("src/a.rs", vec![a]), &store)
        .is_empty());
    assert!(state
        .check(&file_index("src/b.rs", vec![b]), &store)
        .is_empty());
}

#[test]
fn w006_silent_for_same_file_siblings() {
    // Identical siblings within ONE file are usually a deliberate dispatch
    // pattern (trait impls, format_* fan-out) — only cross-file copies warn.
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut state = DupState::default();
    let mut second = definition("calc_b", "src/a.rs", true);
    second.line_start = 30;
    let file = file_index(
        "src/a.rs",
        vec![definition("calc_a", "src/a.rs", true), second],
    );
    assert!(state.check(&file, &store).is_empty());
}

#[test]
fn w006_fires_on_graph_index_match() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![keel_core::types::BodyIndexEntry {
            body_hash: keel_core::hash::compute_body_hash(ECON_BODY),
            t2_hash: String::new(),
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
    let v = DupState::default().check(&file, &store);
    assert_eq!(v.len(), 1);
    assert!(v[0].message.contains("original"));
    assert!(v[0].message.contains("src/orig.rs"));
    assert_eq!(v[0].confidence, 0.85);
}

#[test]
fn w006_ignores_graph_matches_from_own_file() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![keel_core::types::BodyIndexEntry {
            body_hash: keel_core::hash::compute_body_hash(ECON_BODY),
            t2_hash: String::new(),
            node_hash: "selfHash0000".to_string(),
            name: "calc".to_string(),
            file_path: "src/a.rs".to_string(),
            line: 10,
        }])
        .unwrap();

    let file = file_index("src/a.rs", vec![definition("calc", "src/a.rs", true)]);
    assert!(DupState::default().check(&file, &store).is_empty());
}

// --- W006 Type-2 (renamed-clone) detection ---

#[test]
fn w006_t2_fires_on_renamed_clone_cross_file() {
    assert_ne!(
        keel_core::hash::compute_body_hash(T2_BODY),
        keel_core::hash::compute_body_hash(T2_BODY_RENAMED),
        "Type-1 must NOT see this pair, or the test proves nothing about T2"
    );

    let store = SqliteGraphStore::in_memory().unwrap();
    let mut state = DupState::default();
    let file_a = file_index(
        "src/a.rs",
        vec![definition_with_body("collect_active", "src/a.rs", T2_BODY)],
    );
    let file_b = file_index(
        "src/b.rs",
        vec![definition_with_body(
            "gather_enabled",
            "src/b.rs",
            T2_BODY_RENAMED,
        )],
    );

    assert!(state.check(&file_a, &store).is_empty());
    let v = state.check(&file_b, &store);
    assert_eq!(v.len(), 1, "{v:?}");
    assert_eq!(v[0].code, "W006");
    assert_eq!(v[0].confidence, 0.6);
    assert!(v[0].message.contains("collect_active"), "{}", v[0].message);
    assert!(v[0].message.contains("near-duplicate"), "{}", v[0].message);
}

#[test]
fn w006_t2_silent_when_t1_already_fires() {
    // An exact copy matches on both tiers; only the stronger finding is
    // reported, and at Type-1 confidence.
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut state = DupState::default();
    let file_a = file_index(
        "src/a.rs",
        vec![definition_with_body("collect_active", "src/a.rs", T2_BODY)],
    );
    let file_b = file_index(
        "src/b.rs",
        vec![definition_with_body("collect_copy", "src/b.rs", T2_BODY)],
    );

    assert!(state.check(&file_a, &store).is_empty());
    let v = state.check(&file_b, &store);
    assert_eq!(v.len(), 1, "one body, one violation: {v:?}");
    assert_eq!(v[0].confidence, 0.85);
}

#[test]
fn w006_t2_silent_for_trivial_renamed_bodies() {
    // Over the Type-1 floor but under the (higher) Type-2 one: renaming
    // shrinks the fingerprinted string, so this shape is near-universal.
    let a = "return self.configuration_manager.resolve_current_profile_name();";
    let b = "return self.settings_provider.lookup_active_profile_label_now();";
    assert!(keel_core::hash::normalize_body(a).len() >= 60, "T1 floor");

    let store = SqliteGraphStore::in_memory().unwrap();
    let mut state = DupState::default();
    assert!(state
        .check(
            &file_index(
                "src/a.rs",
                vec![definition_with_body("name_of", "src/a.rs", a)]
            ),
            &store
        )
        .is_empty());
    assert!(state
        .check(
            &file_index(
                "src/b.rs",
                vec![definition_with_body("label_of", "src/b.rs", b)]
            ),
            &store
        )
        .is_empty());
}

#[test]
fn w006_t2_respects_same_file_exemption() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut second = definition_with_body("gather_enabled", "src/a.rs", T2_BODY_RENAMED);
    second.line_start = 30;
    let file = file_index(
        "src/a.rs",
        vec![
            definition_with_body("collect_active", "src/a.rs", T2_BODY),
            second,
        ],
    );
    assert!(DupState::default().check(&file, &store).is_empty());
}

#[test]
fn w006_t2_respects_trait_context_exemption() {
    // Two implementors of one trait may legitimately share a body shape.
    let store = SqliteGraphStore::in_memory().unwrap();
    let file_a = file_index(
        "src/a.rs",
        vec![Definition {
            body_text: T2_BODY.to_string(),
            ..trait_context_definition("render", "src/a.rs")
        }],
    );
    let file_b = file_index(
        "src/b.rs",
        vec![Definition {
            body_text: T2_BODY_RENAMED.to_string(),
            ..trait_context_definition("render", "src/b.rs")
        }],
    );

    let mut state = DupState::for_files(&[file_a.clone(), file_b.clone()]);
    assert!(state.check(&file_a, &store).is_empty());
    assert!(
        state.check(&file_b, &store).is_empty(),
        "both sides on a trait surface — nothing to extract"
    );
}

#[test]
fn w006_t2_fires_when_trait_impl_duplicates_free_function() {
    // The exemption is symmetric only: a trait method whose near-twin is an
    // ordinary function is still a real "extract a helper" finding.
    let store = SqliteGraphStore::in_memory().unwrap();
    let free = file_index(
        "src/a.rs",
        vec![definition_with_body("collect_active", "src/a.rs", T2_BODY)],
    );
    let impl_file = file_index(
        "src/b.rs",
        vec![Definition {
            body_text: T2_BODY_RENAMED.to_string(),
            ..trait_context_definition("render", "src/b.rs")
        }],
    );

    let mut state = DupState::for_files(&[free.clone(), impl_file.clone()]);
    assert!(state.check(&free, &store).is_empty());
    let v = state.check(&impl_file, &store);
    assert_eq!(v.len(), 1, "{v:?}");
    assert_eq!(v[0].confidence, 0.6);
}

#[test]
fn w006_t2_fires_on_graph_index_match() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![keel_core::types::BodyIndexEntry {
            // Deliberately unmatchable on Type-1, so only the T2 path can fire.
            body_hash: "noT1Match00".to_string(),
            t2_hash: t2_hash_of(T2_BODY),
            node_hash: "origHash0000".to_string(),
            name: "original".to_string(),
            file_path: "src/orig.rs".to_string(),
            line: 42,
        }])
        .unwrap();

    let file = file_index(
        "src/copy.rs",
        vec![definition_with_body(
            "copied",
            "src/copy.rs",
            T2_BODY_RENAMED,
        )],
    );
    let v = DupState::default().check(&file, &store);
    assert_eq!(v.len(), 1, "{v:?}");
    assert_eq!(v[0].confidence, 0.6);
    assert!(v[0].message.contains("original"));
    assert!(v[0].message.contains("src/orig.rs"));
}

#[test]
fn w006_t2_ignores_graph_matches_from_own_file() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![keel_core::types::BodyIndexEntry {
            body_hash: "noT1Match00".to_string(),
            t2_hash: t2_hash_of(T2_BODY),
            node_hash: "selfHash0000".to_string(),
            name: "collect_active".to_string(),
            file_path: "src/a.rs".to_string(),
            line: 10,
        }])
        .unwrap();

    let file = file_index(
        "src/a.rs",
        vec![definition_with_body("collect_active", "src/a.rs", T2_BODY)],
    );
    assert!(DupState::default().check(&file, &store).is_empty());
}

#[test]
fn w006_t2_never_matches_unindexed_empty_fingerprint() {
    // Rows written before the t2_hash column existed store '' — the same value
    // every not-indexed-for-T2 row carries. Matching them against each other
    // would make every short function a near-duplicate of every other one.
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_body_index(vec![keel_core::types::BodyIndexEntry {
            body_hash: "noT1Match00".to_string(),
            t2_hash: String::new(),
            node_hash: "preUpgrade0".to_string(),
            name: "legacy".to_string(),
            file_path: "src/legacy.rs".to_string(),
            line: 7,
        }])
        .unwrap();

    let short = "return self.configuration_manager.resolve_current_profile_name();";
    let file = file_index(
        "src/a.rs",
        vec![definition_with_body("name_of", "src/a.rs", short)],
    );
    assert!(DupState::default().check(&file, &store).is_empty());
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
