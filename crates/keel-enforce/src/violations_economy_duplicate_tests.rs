use super::*;
use keel_core::types::NodeChange;

// --- W006 duplicate_implementation ---

/// The batch-wide state `check_duplicate_implementation` threads through one
/// compile: the precomputed fingerprints and the per-tier seen maps.
#[derive(Default)]
struct DupState {
    index: DuplicateIndex,
    seen: SeenBodies,
}

impl DupState {
    /// Batch state carrying the fingerprints and trait-context maps the real
    /// batch would build for `files` — the only way to exercise the
    /// cross-file trait exemption honestly.
    fn for_files(files: &[FileIndex]) -> Self {
        Self {
            index: DuplicateIndex::new(files),
            seen: SeenBodies::default(),
        }
    }

    fn check(&mut self, file: &FileIndex, store: &dyn GraphStore) -> Vec<Violation> {
        // `DupState::default()` then `check(f)` is the single-file batch: the
        // index has to hold `f`'s fingerprints, and adding them is idempotent
        // for a state that was already seeded by `for_files`.
        self.index.add_file(file);
        check_duplicate_implementation(file, store, &self.index, &mut self.seen)
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
fn w006_judges_each_def_on_a_shared_line_by_its_own_body() {
    // Minified sources put several functions on one physical line. A
    // line-only per_def key kept just the last one, so the first def was
    // judged with the last def's fingerprints — here calc_b's real duplicate
    // would vanish behind tiny's under-floor body.
    let store = SqliteGraphStore::in_memory().unwrap();
    let mut state = DupState::default();
    let file_a = file_index("src/a.rs", vec![definition("calc_a", "src/a.rs", true)]);

    let mut dup = definition("calc_b", "src/b.rs", true);
    dup.line_start = 1;
    let mut tiny = definition("tiny", "src/b.rs", true);
    tiny.line_start = 1;
    tiny.body_text = "{ 1 }".to_string();
    let file_b = file_index("src/b.rs", vec![dup, tiny]);

    assert!(state.check(&file_a, &store).is_empty());
    let v = state.check(&file_b, &store);
    assert_eq!(v.len(), 1, "calc_b must be judged by its own body");
    assert!(v[0].message.contains("calc_a"));
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
        .update_nodes(vec![NodeChange::Add(function_node(
            1,
            "origHash0000",
            "original",
            "src/orig.rs",
        ))])
        .unwrap();
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
        .update_nodes(vec![NodeChange::Add(function_node(
            1,
            "selfHash0000",
            "calc",
            "src/a.rs",
        ))])
        .unwrap();
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
        .update_nodes(vec![NodeChange::Add(function_node(
            1,
            "origHash0000",
            "original",
            "src/orig.rs",
        ))])
        .unwrap();
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
        .update_nodes(vec![NodeChange::Add(function_node(
            1,
            "selfHash0000",
            "collect_active",
            "src/a.rs",
        ))])
        .unwrap();
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
        .update_nodes(vec![NodeChange::Add(function_node(
            1,
            "preUpgrade0",
            "legacy",
            "src/legacy.rs",
        ))])
        .unwrap();
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
