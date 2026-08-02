//! `keel_meta` — the graph's key/value side table, and every key kept in it.
//!
//! Three unrelated features (the map markers, batch mode, the schema version)
//! all persist one small string each. They share one table so those markers
//! live in the database itself rather than in a stray `.keel/*.json` beside it,
//! which would be a second source of truth to keep in sync.
//!
//! `keel_meta` is deliberately NOT cleared by
//! [`SqliteGraphStore::clear_all`](crate::sqlite::SqliteGraphStore::clear_all):
//! the schema version lives here and must outlive a re-map, and `keel map`
//! re-stamps [`LAST_MAP_AT`]/[`LAST_MAP_COMMIT`] at the end of every run. A
//! crash between `clear_all` and that final stamp therefore leaves a stale
//! marker behind. That is tolerated rather than fixed, because the only
//! consumer that could be misled — the W009 bootstrap guard — also requires
//! stored module edges, and the crashed run left none.
//!
//! Keys live here rather than next to their readers so the set is enumerable:
//! a key defined in the module that happens to read it is invisible to everyone
//! else, and two features silently sharing one name is a corruption bug.

use rusqlite::params;

use crate::sqlite::SqliteGraphStore;
use crate::types::GraphError;

/// `keel_meta` key stamped by `keel map` on completion.
///
/// Its presence is the "this graph has been mapped at least once" signal the
/// W009 bootstrap guard needs: before a first map, the graph holds no edges at
/// all and every dependency would read as new.
pub const LAST_MAP_AT: &str = "last_map_at";

/// `keel_meta` key holding the git commit `keel map` last built the graph from.
///
/// `keel compile` refuses to run (exit 2) when this commit is not an ancestor
/// of `HEAD`: the graph then describes code the checkout does not contain, and
/// `compile --changed` against it manufactures phantom `E001`/`E004` —
/// renamed functions read as removed, live callers read as broken. A graph
/// with no marker (mapped by an older keel, or mapped outside a git repo) is
/// never treated as stale, so pre-existing graphs keep working.
pub const LAST_MAP_COMMIT: &str = "last_map_commit";

/// `keel_meta` key holding the batch-mode state blob (opaque JSON owned by
/// `keel-enforce`).
pub const BATCH_STATE: &str = "batch";

impl SqliteGraphStore {
    /// Read a single `keel_meta` value.
    pub(crate) fn query_meta_value(&self, key: &str) -> Option<String> {
        self.conn
            .query_row(
                "SELECT value FROM keel_meta WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .ok()
    }

    /// Write (or replace) a single `keel_meta` value.
    ///
    /// Used by `keel map` to stamp [`LAST_MAP_AT`], the marker W009's bootstrap
    /// guard reads to tell "no cross-boundary edges because none exist" apart
    /// from "no cross-boundary edges because nothing was ever mapped".
    pub fn set_meta_value(&self, key: &str, value: &str) -> Result<(), GraphError> {
        self.conn.execute(
            "INSERT INTO keel_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Delete a single `keel_meta` value, if it is set.
    pub(crate) fn clear_meta_value(&self, key: &str) -> Result<(), GraphError> {
        self.conn
            .execute("DELETE FROM keel_meta WHERE key = ?1", params![key])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_keys_survive_clear_all() {
        // Pins the module doc: `clear_all` re-baselines every derived table but
        // must not touch `keel_meta`. A wiped `schema_version` would make an
        // up-to-date database look unmigrated.
        let mut store = SqliteGraphStore::in_memory().unwrap();
        let version = store.schema_version().unwrap();
        store
            .set_meta_value(LAST_MAP_AT, "2026-08-02T12:00:00Z")
            .unwrap();
        store.set_meta_value(LAST_MAP_COMMIT, "deadbeef").unwrap();
        store.set_meta_value(BATCH_STATE, "{}").unwrap();

        store.clear_all().unwrap();

        assert_eq!(store.schema_version().unwrap(), version);
        assert_eq!(
            store.query_meta_value(LAST_MAP_AT).as_deref(),
            Some("2026-08-02T12:00:00Z")
        );
        assert_eq!(
            store.query_meta_value(LAST_MAP_COMMIT).as_deref(),
            Some("deadbeef")
        );
        assert_eq!(store.query_meta_value(BATCH_STATE).as_deref(), Some("{}"));
    }
}
