//! Node deletion clears dependent rows without disturbing surviving nodes.

use super::*;

#[test]
fn test_remove_nodes_cleans_every_node_foreign_key() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let mut module = test_node(1, "module", "module");
    module.kind = NodeKind::Module;
    store.insert_node(&module).unwrap();
    let mut child = test_node(2, "child", "child");
    child.module_id = module.id;
    store.insert_node(&child).unwrap();
    store
        .insert_node(&test_node(3, "keeper", "keeper"))
        .unwrap();
    store
        .conn
        .execute_batch(
            "INSERT INTO previous_hashes (node_id, hash) VALUES (2, 'old'), (3, 'keep_old');
         INSERT INTO external_endpoints (node_id, kind, path, direction)
             VALUES (2, 'http', '/gone', 'serves'), (3, 'http', '/keep', 'serves');
         INSERT INTO module_profiles (module_id, path) VALUES (1, 'src/test.rs');
         INSERT INTO resolution_cache
             (call_site_hash, resolved_node_id, confidence, resolution_tier)
             VALUES ('module_ref', 1, 1.0, 'tier1'), ('child_ref', 2, 1.0, 'tier2'),
                    ('keep_ref', 3, 1.0, 'tier1'), ('unresolved', NULL, 0.0, 'tier1');
         INSERT INTO edges (id, source_id, target_id, kind, file_path, line)
             VALUES (1, 1, 2, 'contains', 'src/test.rs', 1),
                    (2, 3, 2, 'calls', 'src/keep.rs', 2);",
        )
        .unwrap();

    store
        .update_nodes(vec![NodeChange::Remove(2), NodeChange::Remove(1)])
        .unwrap();
    assert!(store.get_node_by_id(1).is_none());
    assert!(store.get_node_by_id(2).is_none());
    assert!(store.get_node_by_id(3).is_some());
    assert_eq!(store.get_previous_hashes(3), vec!["keep_old"]);
    for (table, expected) in [
        ("previous_hashes", 1),
        ("external_endpoints", 1),
        ("module_profiles", 0),
        ("edges", 0),
        ("resolution_cache", 2),
    ] {
        let count: i64 = store
            .conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, expected, "{table}");
    }
    let violations: i64 = store
        .conn
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0);
}
