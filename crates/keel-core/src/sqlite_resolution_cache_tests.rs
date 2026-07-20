//! Tests for the persisted Tier 3 resolution cache (issue #44).
//!
//! Split from `sqlite_tests.rs` to keep both files under the line cap. The
//! store layer scopes every read/write to `resolution_tier = 'tier3'`, so these
//! never disturb rows other tiers own (covered separately by the raw-SQL test
//! in `tests/graph/test_sqlite_advanced.rs`).

use super::*;
use crate::store::GraphStore;
use crate::types::ResolutionCacheEntry;

fn resolved(hash: &str, file: &str, name: &str) -> ResolutionCacheEntry {
    ResolutionCacheEntry {
        call_site_hash: hash.to_string(),
        target_file: Some(file.to_string()),
        target_name: Some(name.to_string()),
        confidence: 0.95,
        provider: Some("scip".to_string()),
    }
}

#[test]
fn test_resolution_cache_empty_on_fresh_db() {
    let store = SqliteGraphStore::in_memory().unwrap();
    assert!(store.load_resolution_cache().is_empty());
}

#[test]
fn test_resolution_cache_resolved_roundtrip() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_resolution_cache(vec![resolved("h1", "def.ts", "foo")])
        .expect("replace should succeed");

    let loaded = store.load_resolution_cache();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0], resolved("h1", "def.ts", "foo"));
}

#[test]
fn test_resolution_cache_unresolved_roundtrip() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    let unresolved = ResolutionCacheEntry {
        call_site_hash: "h2".into(),
        target_file: None,
        target_name: None,
        confidence: 0.0,
        provider: None,
    };
    store
        .replace_resolution_cache(vec![unresolved.clone()])
        .expect("replace should succeed");

    let loaded = store.load_resolution_cache();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0], unresolved);
}

/// `replace_resolution_cache` is a wholesale rebuild, not an upsert: the second
/// write's rows fully supersede the first's.
#[test]
fn test_resolution_cache_replace_is_wholesale() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_resolution_cache(vec![
            resolved("old1", "a.ts", "a"),
            resolved("old2", "b.ts", "b"),
        ])
        .unwrap();
    store
        .replace_resolution_cache(vec![resolved("new1", "c.ts", "c")])
        .unwrap();

    let loaded = store.load_resolution_cache();
    assert_eq!(loaded.len(), 1, "prior rows must not survive a replace");
    assert_eq!(loaded[0].call_site_hash, "new1");
}

/// Writes scoped to `tier3` must not delete rows another tier owns.
#[test]
fn test_resolution_cache_replace_preserves_other_tiers() {
    let store = SqliteGraphStore::in_memory().unwrap();
    store
        .conn
        .execute(
            "INSERT INTO resolution_cache (call_site_hash, confidence, resolution_tier)
             VALUES ('t1row', 0.9, 'tier1')",
            [],
        )
        .unwrap();

    let mut store = store;
    store
        .replace_resolution_cache(vec![resolved("h1", "def.ts", "foo")])
        .unwrap();

    // The tier3 write left the tier1 row alone.
    let tier1_count: i64 = store
        .conn
        .query_row(
            "SELECT COUNT(*) FROM resolution_cache WHERE resolution_tier = 'tier1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tier1_count, 1);
    // And load only returns the tier3 row.
    assert_eq!(store.load_resolution_cache().len(), 1);
}

#[test]
fn test_clear_all_empties_resolution_cache() {
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .replace_resolution_cache(vec![resolved("h1", "def.ts", "foo")])
        .unwrap();
    store.clear_all().unwrap();
    assert!(store.load_resolution_cache().is_empty());
}
