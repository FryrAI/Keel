use super::*;
use keel_core::types::EdgeDirection;

fn def(name: &str, file: &str) -> Definition {
    Definition {
        name: name.to_string(),
        kind: NodeKind::Function,
        signature: format!("fn {}()", name),
        file_path: file.to_string(),
        line_start: 1,
        line_end: 3,
        docstring: None,
        is_public: true,
        type_hints_present: true,
        body_text: "{ return 42; }".to_string(),
        in_test_context: false,
        in_trait_context: false,
        is_associated: false,
        is_auto_invoked: false,
        is_decorated: false,
        has_keep_marker: false,
        is_macro: false,
    }
}

fn index(file: &str, defs: Vec<Definition>) -> FileIndex {
    FileIndex {
        file_path: file.to_string(),
        content_hash: 1,
        definitions: defs,
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    }
}

fn def_at(name: &str, file: &str, line_start: u32, line_end: u32) -> Definition {
    Definition {
        line_start,
        line_end,
        ..def(name, file)
    }
}

fn value_ref(name: &str, file: &str, line: u32) -> keel_parsers::resolver::Reference {
    keel_parsers::resolver::Reference {
        name: name.to_string(),
        file_path: file.to_string(),
        line,
        kind: keel_parsers::resolver::ReferenceKind::Value,
        resolved_to: None,
        call_arity: None,
    }
}

fn call_ref(name: &str, file: &str, line: u32, arity: u32) -> keel_parsers::resolver::Reference {
    keel_parsers::resolver::Reference {
        kind: ReferenceKind::Call,
        call_arity: Some(arity),
        ..value_ref(name, file, line)
    }
}

/// Seed the graph with a two-def `src/a.rs` (callee at 1-3, caller at 10-12)
/// and return its index — the shared fixture of the `resolve_call_targets`
/// tests below.
fn seeded_file(store: &mut SqliteGraphStore, cwd: &Path, resolvers: &ResolverSet) -> FileIndex {
    let seed = index(
        "src/a.rs",
        vec![
            def_at("callee", "src/a.rs", 1, 3),
            def_at("caller", "src/a.rs", 10, 12),
        ],
    );
    sync_compiled_files(store, cwd, std::slice::from_ref(&seed), resolvers, false);
    seed
}

fn empty_resolvers() -> ResolverSet<'static> {
    ResolverSet {
        ts: None,
        py: None,
        go: None,
        rs: None,
    }
}

fn incoming_of(store: &SqliteGraphStore, file: &str, name: &str) -> Vec<GraphEdge> {
    let node = store
        .get_nodes_in_file(file)
        .into_iter()
        .find(|n| n.name == name)
        .expect("node exists");
    store.get_edges(node.id, EdgeDirection::Incoming)
}

/// A same-file callback reference (`spawn(handler)`) is a usage, not a
/// call: it must land as a `uses` edge so W005 stays quiet, and must never
/// become a `calls` edge that feeds arity/broken-caller checks.
#[test]
fn same_file_value_reference_becomes_a_uses_edge() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let cwd = std::env::current_dir().unwrap();

    let mut file = index(
        "src/a.rs",
        vec![
            def_at("handler", "src/a.rs", 1, 3),
            def_at("wire", "src/a.rs", 10, 12),
        ],
    );
    file.references = vec![value_ref("handler", "src/a.rs", 11)];
    sync_compiled_files(&mut store, &cwd, &[file], &empty_resolvers(), false);

    let incoming = incoming_of(&store, "src/a.rs", "handler");
    let uses: Vec<_> = incoming
        .iter()
        .filter(|e| e.kind == EdgeKind::Uses)
        .collect();
    assert_eq!(uses.len(), 1, "value ref must produce one uses edge");
    assert!((uses[0].confidence - keel_core::confidence::SAME_FILE_VALUE_REF).abs() < f64::EPSILON);
    assert!(
        !incoming.iter().any(|e| e.kind == EdgeKind::Calls),
        "a value reference must never produce a calls edge"
    );
}

/// The cross-file case W005 was false-positiving on: `child.rs` imports a
/// callback from `mod.rs` and passes it as a value. The import-based
/// resolution ladder must link it as a `uses` edge.
#[test]
fn cross_file_value_reference_resolves_to_a_uses_edge() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let cwd = std::env::current_dir().unwrap();
    let resolvers = empty_resolvers();

    // The callback's defining file is already in the graph.
    let owner = index(
        "src/mod.rs",
        vec![def_at("cross_file_cb", "src/mod.rs", 1, 3)],
    );
    sync_compiled_files(&mut store, &cwd, &[owner], &resolvers, false);

    // The user's file references it as a value through an import.
    let mut caller = index("src/child.rs", vec![def_at("wire", "src/child.rs", 5, 9)]);
    caller.references = vec![value_ref("cross_file_cb", "src/child.rs", 7)];
    caller.imports = vec![keel_parsers::resolver::Import {
        source: "src/mod.rs".to_string(),
        imported_names: vec!["cross_file_cb".to_string()],
        file_path: "src/child.rs".to_string(),
        line: 1,
        is_relative: true,
    }];
    sync_compiled_files(&mut store, &cwd, &[caller], &resolvers, false);

    let incoming = incoming_of(&store, "src/mod.rs", "cross_file_cb");
    assert!(
        incoming.iter().any(|e| e.kind == EdgeKind::Uses),
        "cross-file value reference must resolve to a uses edge: {incoming:?}"
    );
    assert!(
        !incoming.iter().any(|e| e.kind == EdgeKind::Calls),
        "a value reference must never produce a calls edge"
    );
}

/// Two files adding IDENTICAL new functions in one multi-file compile must
/// not share a hash: without the in-batch assigned-hash guard, the second
/// node silently overwrote the first's row and its edges dangled.
#[test]
fn same_batch_identical_defs_get_distinct_hashes() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let cwd = std::env::current_dir().unwrap();
    let resolvers = empty_resolvers();

    let files = vec![
        index("src/a.rs", vec![def("twin", "src/a.rs")]),
        index("src/b.rs", vec![def("twin", "src/b.rs")]),
    ];
    sync_compiled_files(&mut store, &cwd, &files, &resolvers, false);

    let a_nodes: Vec<_> = store
        .get_nodes_in_file("src/a.rs")
        .into_iter()
        .filter(|n| n.kind == NodeKind::Function)
        .collect();
    let b_nodes: Vec<_> = store
        .get_nodes_in_file("src/b.rs")
        .into_iter()
        .filter(|n| n.kind == NodeKind::Function)
        .collect();
    assert_eq!(a_nodes.len(), 1, "first file's node must survive");
    assert_eq!(b_nodes.len(), 1, "second file's node must be inserted");
    assert_ne!(
        a_nodes[0].hash, b_nodes[0].hash,
        "identical same-batch defs must get disambiguated hashes"
    );
    // Both files' Contains edges resolve to live nodes (nothing dangles).
    for n in a_nodes.iter().chain(b_nodes.iter()) {
        assert!(
            !store.get_edges(n.id, EdgeDirection::Incoming).is_empty(),
            "each node keeps its module Contains edge"
        );
    }
}

/// The issue-#54 seam: `resolve_call_targets` populates `resolved_to` with
/// the target node's hash for a same-file call, so E005 finally has a
/// production site feeding it. A value reference must stay unresolved —
/// it carries no argument list and must never reach arity checking.
#[test]
fn resolve_call_targets_sets_resolved_to_for_same_file_calls() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let cwd = std::env::current_dir().unwrap();
    let resolvers = empty_resolvers();
    let seed = seeded_file(&mut store, &cwd, &resolvers);
    let callee_hash = store
        .get_nodes_in_file("src/a.rs")
        .into_iter()
        .find(|n| n.name == "callee")
        .expect("callee node")
        .hash;

    let mut file = seed;
    file.references = vec![
        call_ref("callee", "src/a.rs", 11, 2),
        value_ref("callee", "src/a.rs", 11),
    ];
    let mut files = vec![file];
    resolve_call_targets(&store, &cwd, &mut files, &resolvers);

    assert_eq!(
        files[0].references[0].resolved_to.as_deref(),
        Some(callee_hash.as_str()),
        "a same-file call resolves to the stored node's hash"
    );
    assert_eq!(
        files[0].references[1].resolved_to, None,
        "value references stay unresolved — no argument list, no E005"
    );
}

/// A call without a syntactic argument count (attribute macros, markup
/// hits) is skipped outright, and a warning-tier resolution must not
/// stick: E005 is an ERROR and may only fire on error-tier targets.
#[test]
fn resolve_call_targets_skips_unknown_arity_and_warning_tier() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let cwd = std::env::current_dir().unwrap();
    let resolvers = empty_resolvers();
    let mut file = seeded_file(&mut store, &cwd, &resolvers);
    let mut no_arity = call_ref("callee", "src/a.rs", 11, 0);
    no_arity.call_arity = None;
    // An unfamiliar-receiver method call resolves through the ladder at
    // UNFAMILIAR_RECEIVER_METHOD (0.7) — below the error tier.
    let unfamiliar = call_ref("obj.callee", "src/a.rs", 11, 1);
    file.references = vec![no_arity, unfamiliar];
    let mut files = vec![file];
    resolve_call_targets(&store, &cwd, &mut files, &resolvers);
    assert_eq!(
        files[0].references[0].resolved_to, None,
        "a call with no countable argument list must not resolve"
    );
    assert_eq!(
        files[0].references[1].resolved_to, None,
        "a warning-tier resolution must not feed E005"
    );
}

/// A file that legally defines the same name twice (a free `search_graph`
/// next to a `search_graph` method) collapses to one entry in the name-keyed
/// local map — binding a same-file call against it is a coin flip, so the
/// pre-enforcement pass must refuse to resolve it (dogfooding phantom from
/// keel's own `queries.rs`).
#[test]
fn resolve_call_targets_refuses_same_file_ambiguous_names() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let cwd = std::env::current_dir().unwrap();
    let resolvers = empty_resolvers();

    let seed = index(
        "src/q.rs",
        vec![
            def_at("search_graph", "src/q.rs", 1, 3),
            def_at("search_graph", "src/q.rs", 10, 12),
            def_at("wire", "src/q.rs", 20, 24),
        ],
    );
    sync_compiled_files(
        &mut store,
        &cwd,
        std::slice::from_ref(&seed),
        &resolvers,
        false,
    );

    let mut file = seed;
    file.references = vec![
        call_ref("search_graph", "src/q.rs", 21, 4),
        call_ref("self.search_graph", "src/q.rs", 22, 3),
    ];
    let mut files = vec![file];
    resolve_call_targets(&store, &cwd, &mut files, &resolvers);

    for r in &files[0].references {
        assert_eq!(
            r.resolved_to, None,
            "`{}` must not resolve: the file defines the name twice",
            r.name
        );
    }
}
