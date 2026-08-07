//! Export/import of the `quality_snapshots` series as JSON Lines.
//!
//! CI's default-branch graph cache is keyed per commit, so the fresh or
//! restored `graph.db` a push run starts from never carries a prior commit's
//! snapshot row on its own — a full re-map's `clear_all` doesn't touch the
//! table (see [`keel_core::sqlite_quality`]), but a *new* per-commit db has no
//! rows to begin with. A JSON Lines history file, restored/merged/saved around
//! the snapshot step, is what lets the series survive across runners.

use keel_core::sqlite::SqliteGraphStore;
use keel_core::sqlite_quality::{QualitySnapshotRow, SnapshotWrite};

/// Outcome of importing a history file: how many rows were new vs. refreshed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportSummary {
    /// Rows appended to the series.
    pub inserted: usize,
    /// Rows that already existed at that `commit_sha` and were refreshed.
    pub updated: usize,
}

impl ImportSummary {
    /// Total rows the import touched, inserted or updated.
    pub fn total(&self) -> usize {
        self.inserted + self.updated
    }
}

/// One JSON line per stored snapshot, oldest first — `metrics` kept as the raw
/// stored TEXT blob (so it appears double-encoded within the line) rather than
/// parsed and re-nested, so a re-imported row is byte-identical to what
/// `--snapshot` wrote directly; no reserialization step can perturb a
/// version-sensitive blob.
pub fn export_jsonl(store: &SqliteGraphStore) -> String {
    store
        .quality_snapshots(0)
        .iter()
        .filter_map(|row| serde_json::to_string(row).ok())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Upsert every row in a JSON Lines history file, in file order, by
/// `commit_sha`. A blank line is skipped. A line that fails to parse is a hard
/// error naming its 1-based line number — a corrupt history file must not
/// silently lose rows from the one series that exists to catch decay a human
/// wouldn't otherwise see.
pub fn import_jsonl(store: &SqliteGraphStore, content: &str) -> Result<ImportSummary, String> {
    let mut summary = ImportSummary::default();
    for (i, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row: QualitySnapshotRow =
            serde_json::from_str(line).map_err(|e| format!("line {}: {e}", i + 1))?;
        match store
            .insert_quality_snapshot(row.commit_sha.as_deref(), &row.metrics)
            .map_err(|e| format!("line {}: {e}", i + 1))?
        {
            SnapshotWrite::Inserted => summary.inserted += 1,
            SnapshotWrite::Updated => summary.updated += 1,
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_store() -> SqliteGraphStore {
        let store = SqliteGraphStore::in_memory().unwrap();
        store
            .insert_quality_snapshot(Some("aaa"), r#"{"version":1,"files_over_budget":1}"#)
            .unwrap();
        store
            .insert_quality_snapshot(Some("bbb"), r#"{"version":1,"files_over_budget":2}"#)
            .unwrap();
        store
    }

    #[test]
    fn export_then_import_into_a_fresh_store_reproduces_the_series() {
        let source = seeded_store();
        let jsonl = export_jsonl(&source);
        assert_eq!(jsonl.lines().count(), 2, "oldest to newest, one line each");

        let target = SqliteGraphStore::in_memory().unwrap();
        let summary = import_jsonl(&target, &jsonl).unwrap();
        assert_eq!(
            summary,
            ImportSummary {
                inserted: 2,
                updated: 0
            }
        );
        assert_eq!(summary.total(), 2);

        let rows = target.quality_snapshots(0);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].commit_sha.as_deref(), Some("aaa"));
        assert_eq!(rows[1].commit_sha.as_deref(), Some("bbb"));
    }

    #[test]
    fn reimporting_the_same_history_is_idempotent() {
        let source = seeded_store();
        let jsonl = export_jsonl(&source);

        let target = SqliteGraphStore::in_memory().unwrap();
        import_jsonl(&target, &jsonl).unwrap();
        let summary = import_jsonl(&target, &jsonl).unwrap();
        assert_eq!(
            summary,
            ImportSummary {
                inserted: 0,
                updated: 2
            }
        );
        assert_eq!(target.quality_snapshots(0).len(), 2, "no duplicate rows");
    }

    #[test]
    fn blank_lines_are_skipped() {
        let target = SqliteGraphStore::in_memory().unwrap();
        let summary = import_jsonl(&target, "\n\n  \n").unwrap();
        assert_eq!(summary.total(), 0);
    }

    #[test]
    fn a_corrupt_line_is_a_hard_error_naming_its_line_number() {
        let target = SqliteGraphStore::in_memory().unwrap();
        let content = format!(
            "{}\nnot json\n",
            serde_json::to_string(&QualitySnapshotRow {
                id: 1,
                captured_at: "2026-01-01 00:00:00".to_string(),
                commit_sha: Some("aaa".to_string()),
                metrics: "{}".to_string(),
            })
            .unwrap()
        );
        let err = import_jsonl(&target, &content).expect_err("line 2 is not valid JSON");
        assert!(err.starts_with("line 2:"), "got: {err}");
    }
}
