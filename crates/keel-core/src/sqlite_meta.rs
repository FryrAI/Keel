//! `keel_meta` — the graph's key/value side table, and every key kept in it.
//!
//! Three unrelated features (the map markers, batch mode, the schema version)
//! all persist one small string each. They share this table because a stray
//! `.keel/*.json` next to the database is a second source of truth that can
//! survive a `clear_all` the database does not.
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
