//! `keel_meta` — the graph's key/value side table, and every key kept in it.
//!
//! Three unrelated features (the map markers, batch mode, the schema version)
//! all persist one small string each. They share one table so those markers
//! live in the database itself rather than in a stray `.keel/*.json` beside it,
//! which would be a second source of truth to keep in sync.
//!
//! `keel_meta` is deliberately NOT cleared by
//! [`SqliteGraphStore::clear_all`](crate::sqlite::SqliteGraphStore::clear_all):
//! the schema version lives here and must outlive a re-map.
//!
//! The map markers are a different matter. `keel map` wipes the graph and
//! re-stamps [`LAST_MAP_AT`]/[`LAST_MAP_COMMIT`] only when it finishes, so a
//! crash in between would leave markers describing HEAD over an empty graph —
//! and `keel compile`'s staleness guard, seeing a `last_map_commit` that IS an
//! ancestor of HEAD, would wave the compile through to enforce against
//! nothing. So `keel map` calls [`SqliteGraphStore::clear_map_markers`] as part
//! of its clearing step: a crashed map then reads as never-mapped, which is
//! both true and a state every consumer already handles (the staleness guard
//! and the W009 bootstrap guard are each documented to stay silent without a
//! marker).
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

    /// Drop both map markers, declaring the graph never-mapped.
    ///
    /// `keel map` calls this immediately after `clear_all`, so the window in
    /// which the graph is empty is also a window in which no marker claims
    /// otherwise. See the module documentation for why a crash inside that
    /// window has to read as "never mapped" rather than "mapped at HEAD".
    pub fn clear_map_markers(&self) -> Result<(), GraphError> {
        self.clear_meta_value(LAST_MAP_AT)?;
        self.clear_meta_value(LAST_MAP_COMMIT)
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

    #[test]
    fn map_clear_phase_erases_only_the_map_markers() {
        // `keel map`'s clearing step, in miniature: wipe the graph, then drop
        // the markers. A crash from here on must read as never-mapped, or the
        // staleness guard passes an empty graph off as current.
        let mut store = SqliteGraphStore::in_memory().unwrap();
        let version = store.schema_version().unwrap();
        store.set_meta_value(LAST_MAP_AT, "1700000000").unwrap();
        store.set_meta_value(LAST_MAP_COMMIT, "deadbeef").unwrap();
        store.set_meta_value(BATCH_STATE, "{}").unwrap();

        store.clear_all().unwrap();
        store.clear_map_markers().unwrap();

        assert!(store.query_meta_value(LAST_MAP_AT).is_none());
        assert!(store.query_meta_value(LAST_MAP_COMMIT).is_none());
        // Everything else in the table is untouched.
        assert_eq!(store.schema_version().unwrap(), version);
        assert_eq!(store.query_meta_value(BATCH_STATE).as_deref(), Some("{}"));

        // And a completed map puts them back.
        store.set_meta_value(LAST_MAP_AT, "1700000001").unwrap();
        store.set_meta_value(LAST_MAP_COMMIT, "cafebabe").unwrap();
        assert_eq!(
            store.query_meta_value(LAST_MAP_COMMIT).as_deref(),
            Some("cafebabe")
        );
    }
}
