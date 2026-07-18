//! E001 (broken_caller) engine tests: signature changes and batch-skip behavior.
use super::*;

#[test]
fn test_e001_broken_caller_fires() {
    let store = SqliteGraphStore::in_memory().unwrap();
    // Store a function with old hash
    let old_hash = keel_core::hash::compute_hash("fn foo(x: i32)", "{ x + 1 }", "Doc for foo");
    let mut node = make_node(1, &old_hash, "foo", "fn foo(x: i32)", "src/lib.rs");
    node.docstring = Some("Doc for foo".to_string());
    store.insert_node(&node).unwrap();

    // Store a caller
    let caller = make_node(2, "cal11111111", "bar", "fn bar()", "src/bar.rs");
    store.insert_node(&caller).unwrap();

    // Edge: caller -> foo
    let mut store_mut = store;
    store_mut
        .update_edges(vec![EdgeChange::Add(make_call_edge(1, 2, 1, "src/bar.rs"))])
        .unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store_mut));

    // Compile with a changed signature for foo
    let file = FileIndex {
        file_path: "src/lib.rs".to_string(),
        content_hash: 0,
        definitions: vec![make_definition(
            "foo",
            "fn foo(x: i32, y: i32)",
            "{ x + y }",
            "src/lib.rs",
        )],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let result = engine.compile(&[file]);
    assert_eq!(result.status, "error");
    assert!(!result.errors.is_empty());
    let e001 = result.errors.iter().find(|v| v.code == "E001");
    assert!(e001.is_some(), "E001 broken_caller should fire");
    let v = e001.unwrap();
    assert_eq!(v.category, "broken_caller");
    assert_eq!(v.affected.len(), 1);
    assert_eq!(v.affected[0].name, "bar");
}

#[test]
fn test_e001_skipped_when_caller_in_batch() {
    let store = SqliteGraphStore::in_memory().unwrap();
    // Store function foo with old hash
    let old_hash = keel_core::hash::compute_hash("fn foo(x: i32)", "{ x + 1 }", "Doc for foo");
    let mut node = make_node(1, &old_hash, "foo", "fn foo(x: i32)", "src/lib.rs");
    node.docstring = Some("Doc for foo".to_string());
    store.insert_node(&node).unwrap();

    // Store caller bar in src/bar.rs
    let caller = make_node(2, "cal11111111", "bar", "fn bar()", "src/bar.rs");
    store.insert_node(&caller).unwrap();

    // Edge: bar -> foo
    let mut store_mut = store;
    store_mut
        .update_edges(vec![EdgeChange::Add(make_call_edge(1, 2, 1, "src/bar.rs"))])
        .unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store_mut));

    // Compile BOTH files — caller is in the batch, so E001 should be skipped
    let file_foo = FileIndex {
        file_path: "src/lib.rs".to_string(),
        content_hash: 0,
        definitions: vec![make_definition(
            "foo",
            "fn foo(x: i32, y: i32)",
            "{ x + y }",
            "src/lib.rs",
        )],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };
    let file_bar = FileIndex {
        file_path: "src/bar.rs".to_string(),
        content_hash: 0,
        definitions: vec![make_definition(
            "bar",
            "fn bar()",
            "{ foo(1, 2) }",
            "src/bar.rs",
        )],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let result = engine.compile(&[file_foo, file_bar]);
    let e001 = result.errors.iter().find(|v| v.code == "E001");
    assert!(
        e001.is_none(),
        "E001 should NOT fire when caller is in the same compile batch"
    );
}

#[test]
fn test_e001_fires_when_caller_not_in_batch() {
    // Same as test_e001_broken_caller_fires — compile only the changed file,
    // caller is NOT in the batch, so E001 should fire.
    let store = SqliteGraphStore::in_memory().unwrap();
    let old_hash = keel_core::hash::compute_hash("fn foo(x: i32)", "{ x + 1 }", "Doc for foo");
    let mut node = make_node(1, &old_hash, "foo", "fn foo(x: i32)", "src/lib.rs");
    node.docstring = Some("Doc for foo".to_string());
    store.insert_node(&node).unwrap();

    let caller = make_node(2, "cal11111111", "bar", "fn bar()", "src/bar.rs");
    store.insert_node(&caller).unwrap();

    let mut store_mut = store;
    store_mut
        .update_edges(vec![EdgeChange::Add(make_call_edge(1, 2, 1, "src/bar.rs"))])
        .unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store_mut));

    // Compile only src/lib.rs — caller src/bar.rs is NOT in batch
    let file = FileIndex {
        file_path: "src/lib.rs".to_string(),
        content_hash: 0,
        definitions: vec![make_definition(
            "foo",
            "fn foo(x: i32, y: i32)",
            "{ x + y }",
            "src/lib.rs",
        )],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let result = engine.compile(&[file]);
    let e001 = result.errors.iter().find(|v| v.code == "E001");
    assert!(
        e001.is_some(),
        "E001 should fire when caller is NOT in the compile batch"
    );
    assert_eq!(e001.unwrap().affected.len(), 1);
    assert_eq!(e001.unwrap().affected[0].name, "bar");
}

#[test]
fn test_e001_partial_batch_skips_only_batch_callers() {
    let store = SqliteGraphStore::in_memory().unwrap();
    let old_hash = keel_core::hash::compute_hash("fn foo(x: i32)", "{ x + 1 }", "Doc for foo");
    let mut node = make_node(1, &old_hash, "foo", "fn foo(x: i32)", "src/lib.rs");
    node.docstring = Some("Doc for foo".to_string());
    store.insert_node(&node).unwrap();

    // Two callers: bar in src/bar.rs (will be in batch) and baz in src/baz.rs (not in batch)
    let caller_bar = make_node(2, "cal11111111", "bar", "fn bar()", "src/bar.rs");
    store.insert_node(&caller_bar).unwrap();
    let caller_baz = make_node(3, "cal22222222", "baz", "fn baz()", "src/baz.rs");
    store.insert_node(&caller_baz).unwrap();

    let mut store_mut = store;
    store_mut
        .update_edges(vec![
            EdgeChange::Add(make_call_edge(1, 2, 1, "src/bar.rs")),
            EdgeChange::Add(make_call_edge(2, 3, 1, "src/baz.rs")),
        ])
        .unwrap();

    let mut engine = EnforcementEngine::new(Box::new(store_mut));

    // Compile src/lib.rs AND src/bar.rs, but NOT src/baz.rs
    let file_foo = FileIndex {
        file_path: "src/lib.rs".to_string(),
        content_hash: 0,
        definitions: vec![make_definition(
            "foo",
            "fn foo(x: i32, y: i32)",
            "{ x + y }",
            "src/lib.rs",
        )],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };
    let file_bar = FileIndex {
        file_path: "src/bar.rs".to_string(),
        content_hash: 0,
        definitions: vec![make_definition(
            "bar",
            "fn bar()",
            "{ foo(1, 2) }",
            "src/bar.rs",
        )],
        references: vec![],
        imports: vec![],
        external_endpoints: vec![],
        parse_duration_us: 0,
    };

    let result = engine.compile(&[file_foo, file_bar]);
    let e001 = result.errors.iter().find(|v| v.code == "E001");
    assert!(
        e001.is_some(),
        "E001 should fire for the non-batch caller (baz)"
    );
    let v = e001.unwrap();
    assert_eq!(
        v.affected.len(),
        1,
        "Only the non-batch caller should remain"
    );
    assert_eq!(v.affected[0].name, "baz");
}
