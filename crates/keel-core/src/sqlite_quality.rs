//! `quality_snapshots` (schema v7) — the one table in keel's graph that a full
//! re-map does not delete.
//!
//! Every other table here is derived: `keel map` calls
//! [`SqliteGraphStore::clear_all`](crate::sqlite::SqliteGraphStore::clear_all)
//! and rebuilds them from source, which is why two consecutive audits of a
//! decaying codebase produce no diff and nobody can say whether it is improving
//! or rotting. This table is the exception, and its absence from that DELETE
//! batch is the entire mechanism that gives keel a memory across maps.
//!
//! The `metrics` column holds a versioned JSON blob rather than one column per
//! metric, so adding a metric never needs a migration — and a *changed*
//! definition bumps the blob version, which makes `keel quality --trend` refuse
//! the comparison instead of silently mixing two definitions into one line.

use rusqlite::params;

use crate::sqlite::SqliteGraphStore;
use crate::types::{GraphError, QualityInputs};

/// Edge kinds that make one MODULE depend on another, as a SQL `IN` list.
///
/// Mirrors `keel_enforce::audit::navigation::is_module_dep_edge`; a test in
/// that crate asserts the two agree, because a literal in a SQL string cannot
/// be type-checked against a Rust `match`.
pub const MODULE_DEP_KINDS_SQL: &str = "'calls','imports'";

/// Edge kinds that make one SYMBOL depend on another, as a SQL `IN` list.
///
/// Mirrors `keel_enforce::queries::is_dependency_edge`, guarded by the same
/// test.
pub const SYMBOL_DEP_KINDS_SQL: &str = "'calls','uses'";

/// DDL for the schema-v7 `quality_snapshots` table and its lookup index.
///
/// Shared by `initialize_schema` (fresh databases) and `migrate_v6_to_v7`
/// (existing ones) so the two can never drift apart. Purely additive: no table
/// is rebuilt and no existing row is touched.
///
/// `commit_sha` is UNIQUE so one merge commit can only ever own one row — a
/// re-run of the CI snapshot step updates the measurement in place instead of
/// stacking duplicate points onto the series. It is nullable: a snapshot taken
/// outside a git checkout still records, and SQLite treats NULLs as distinct,
/// so those never collide with each other.
pub const QUALITY_SNAPSHOTS_DDL: &str = "
    CREATE TABLE IF NOT EXISTS quality_snapshots (
        id INTEGER PRIMARY KEY,
        captured_at TEXT DEFAULT (datetime('now')),
        commit_sha TEXT UNIQUE,
        metrics TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_quality_snapshots_commit
        ON quality_snapshots(commit_sha);
";

/// One persisted quality snapshot, exactly as stored.
///
/// `metrics` is left as the raw JSON blob: this crate stores the series and
/// knows nothing about what a metric means — `keel-enforce` owns the schema of
/// the blob and the refusal rule for comparing across versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualitySnapshotRow {
    /// Monotonic insertion order. The series is ordered by this, not by
    /// `captured_at`: re-measuring an existing commit refreshes the timestamp
    /// but must not move the point in history.
    pub id: i64,
    /// SQLite `datetime('now')` at capture (UTC, `YYYY-MM-DD HH:MM:SS`).
    pub captured_at: String,
    /// The commit the measured graph describes, when there was one.
    pub commit_sha: Option<String>,
    /// The versioned metrics JSON blob.
    pub metrics: String,
}

/// Outcome of persisting a snapshot — which of the two idempotent paths ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotWrite {
    /// A new point was appended to the series.
    Inserted,
    /// A point for this commit already existed and its measurement was
    /// refreshed in place.
    Updated,
}

impl SqliteGraphStore {
    /// Persist one quality snapshot, keyed on `commit_sha`.
    ///
    /// Idempotent per commit: a second capture at the same commit **updates**
    /// the existing row rather than skipping or duplicating. Updating is the
    /// safe direction — a re-run happens after the graph was rebuilt, so the
    /// newer measurement is the more accurate one, and keeping the original
    /// `id` means refreshing a point never reorders history.
    pub fn insert_quality_snapshot(
        &self,
        commit_sha: Option<&str>,
        metrics: &str,
    ) -> Result<SnapshotWrite, GraphError> {
        let existing: Option<i64> = commit_sha.and_then(|sha| {
            self.conn
                .query_row(
                    "SELECT id FROM quality_snapshots WHERE commit_sha = ?1",
                    params![sha],
                    |row| row.get(0),
                )
                .ok()
        });

        match existing {
            Some(id) => {
                self.conn.execute(
                    "UPDATE quality_snapshots
                     SET metrics = ?1, captured_at = datetime('now')
                     WHERE id = ?2",
                    params![metrics, id],
                )?;
                Ok(SnapshotWrite::Updated)
            }
            None => {
                self.conn.execute(
                    "INSERT INTO quality_snapshots (commit_sha, metrics)
                     VALUES (?1, ?2)",
                    params![commit_sha, metrics],
                )?;
                Ok(SnapshotWrite::Inserted)
            }
        }
    }

    /// The most recent `limit` snapshots, returned oldest → newest.
    ///
    /// `limit == 0` means no cap. The window is taken from the *newest* end and
    /// then reversed, so `--last 20` is the last 20 merges, read left to right.
    pub fn quality_snapshots(&self, limit: usize) -> Vec<QualitySnapshotRow> {
        let sql = if limit == 0 {
            "SELECT id, captured_at, commit_sha, metrics FROM quality_snapshots ORDER BY id DESC"
                .to_string()
        } else {
            format!(
                "SELECT id, captured_at, commit_sha, metrics FROM quality_snapshots \
                 ORDER BY id DESC LIMIT {limit}"
            )
        };
        let mut rows = self.query_snapshots(&sql, params![]);
        rows.reverse();
        rows
    }

    /// Every snapshot from the one at `commit` onward, oldest → newest.
    ///
    /// `commit` may be a short prefix, because that is what a human types and
    /// what `git log --oneline` prints, while CI stores the full 40-character
    /// SHA. An empty vector means no snapshot matches — the caller reports that
    /// as an unknown ref rather than as an empty trend.
    pub fn quality_snapshots_since(&self, commit: &str) -> Vec<QualitySnapshotRow> {
        if commit.is_empty() {
            return Vec::new();
        }
        let start: Option<i64> = self
            .conn
            .query_row(
                "SELECT MIN(id) FROM quality_snapshots
                 WHERE commit_sha = ?1 OR commit_sha LIKE ?1 || '%'",
                params![commit],
                |row| row.get::<_, Option<i64>>(0),
            )
            .ok()
            .flatten();
        let Some(start) = start else {
            return Vec::new();
        };
        self.query_snapshots(
            "SELECT id, captured_at, commit_sha, metrics FROM quality_snapshots
             WHERE id >= ?1 ORDER BY id ASC",
            params![start],
        )
    }

    /// Everything the `keel quality` metrics need, in four aggregate queries.
    ///
    /// All four are answered from existing indexes (`nodes.id` and
    /// `edges.target_id` primary/secondary keys drive the joins), so a full
    /// measurement costs a handful of milliseconds instead of one query per
    /// node. Files are deduplicated here: hash-salt collisions can leave a file
    /// with two stored `module` rows, and iterating those in Rust silently
    /// double-counts every metric derived from them.
    pub(crate) fn query_quality_inputs(&self) -> QualityInputs {
        let file_lines = self
            .collect(
                "SELECT file_path, MAX(line_end - line_start + 1)
                 FROM nodes WHERE kind = 'module' GROUP BY file_path",
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?.max(0) as u32,
                    ))
                },
            )
            .unwrap_or_default();

        let uncalled_private_fns = self
            .collect(
                &format!(
                    "SELECT n.file_path, n.name FROM nodes n
                     WHERE n.kind = 'function' AND n.is_public = 0 AND n.is_associated = 0
                       AND NOT EXISTS (
                         SELECT 1 FROM edges e
                         WHERE e.target_id = n.id AND e.kind IN ({SYMBOL_DEP_KINDS_SQL})
                       )"
                ),
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap_or_default();

        let module_deps = self
            .collect(
                &format!(
                    "SELECT DISTINCT sn.file_path, tn.file_path FROM edges e
                     JOIN nodes sn ON sn.id = e.source_id
                     JOIN nodes tn ON tn.id = e.target_id
                     WHERE e.kind IN ({MODULE_DEP_KINDS_SQL}) AND sn.file_path <> tn.file_path"
                ),
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap_or_default();

        let (dependency_edges, cross_file_dependency_edges) = self
            .conn
            .query_row(
                &format!(
                    "SELECT COUNT(*),
                            SUM(CASE WHEN sn.file_path <> tn.file_path THEN 1 ELSE 0 END)
                     FROM edges e
                     JOIN nodes sn ON sn.id = e.source_id
                     JOIN nodes tn ON tn.id = e.target_id
                     WHERE e.kind IN ({SYMBOL_DEP_KINDS_SQL})"
                ),
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)? as u64,
                        row.get::<_, Option<i64>>(1)?.unwrap_or(0) as u64,
                    ))
                },
            )
            .unwrap_or((0, 0));

        QualityInputs {
            file_lines,
            uncalled_private_fns,
            module_deps,
            dependency_edges,
            cross_file_dependency_edges,
        }
    }

    /// Run a parameterless SELECT and collect its mapped rows.
    fn collect<T>(
        &self,
        sql: &str,
        map: impl Fn(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Vec<T>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], map)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Run a snapshot SELECT and collect its rows, reporting query failures on
    /// stderr like the rest of the store's read path.
    fn query_snapshots(&self, sql: &str, args: impl rusqlite::Params) -> Vec<QualitySnapshotRow> {
        let mut stmt = match self.conn.prepare(sql) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[keel] quality_snapshots: prepare failed: {e}");
                return Vec::new();
            }
        };
        let rows = stmt.query_map(args, |row| {
            Ok(QualitySnapshotRow {
                id: row.get(0)?,
                captured_at: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                commit_sha: row.get(2)?,
                metrics: row.get(3)?,
            })
        });
        match rows {
            Ok(r) => r.filter_map(|x| x.ok()).collect(),
            Err(e) => {
                eprintln!("[keel] quality_snapshots: query failed: {e}");
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> SqliteGraphStore {
        SqliteGraphStore::in_memory().unwrap()
    }

    #[test]
    fn insert_then_read_back_oldest_first() {
        let s = store();
        s.insert_quality_snapshot(Some("aaa"), "{\"version\":1}")
            .unwrap();
        s.insert_quality_snapshot(Some("bbb"), "{\"version\":2}")
            .unwrap();
        let rows = s.quality_snapshots(0);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].commit_sha.as_deref(), Some("aaa"));
        assert_eq!(rows[1].commit_sha.as_deref(), Some("bbb"));
        assert!(!rows[0].captured_at.is_empty());
    }

    #[test]
    fn second_capture_of_a_commit_updates_in_place() {
        let s = store();
        assert_eq!(
            s.insert_quality_snapshot(Some("aaa"), "{\"v\":1}").unwrap(),
            SnapshotWrite::Inserted
        );
        assert_eq!(
            s.insert_quality_snapshot(Some("aaa"), "{\"v\":2}").unwrap(),
            SnapshotWrite::Updated
        );
        let rows = s.quality_snapshots(0);
        assert_eq!(rows.len(), 1, "one commit owns exactly one point");
        assert_eq!(rows[0].metrics, "{\"v\":2}");
    }

    #[test]
    fn snapshots_without_a_commit_never_collide() {
        let s = store();
        s.insert_quality_snapshot(None, "{}").unwrap();
        s.insert_quality_snapshot(None, "{}").unwrap();
        assert_eq!(s.quality_snapshots(0).len(), 2);
    }

    #[test]
    fn limit_takes_the_newest_window() {
        let s = store();
        for sha in ["a", "b", "c", "d"] {
            s.insert_quality_snapshot(Some(sha), "{}").unwrap();
        }
        let rows = s.quality_snapshots(2);
        let shas: Vec<&str> = rows
            .iter()
            .filter_map(|r| r.commit_sha.as_deref())
            .collect();
        assert_eq!(shas, vec!["c", "d"]);
    }

    #[test]
    fn since_accepts_a_short_prefix_and_is_inclusive() {
        let s = store();
        s.insert_quality_snapshot(Some("1111111111"), "{}").unwrap();
        s.insert_quality_snapshot(Some("2222222222"), "{}").unwrap();
        s.insert_quality_snapshot(Some("3333333333"), "{}").unwrap();
        let rows = s.quality_snapshots_since("2222");
        let shas: Vec<&str> = rows
            .iter()
            .filter_map(|r| r.commit_sha.as_deref())
            .collect();
        assert_eq!(shas, vec!["2222222222", "3333333333"]);
    }

    #[test]
    fn since_an_unknown_commit_is_empty() {
        let s = store();
        s.insert_quality_snapshot(Some("aaa"), "{}").unwrap();
        assert!(s.quality_snapshots_since("zzz").is_empty());
        assert!(s.quality_snapshots_since("").is_empty());
    }
}
