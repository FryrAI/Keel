//! SQL behind the architectural-boundary queries (W009 / E006).
//!
//! Every statement here runs in the `keel compile` hot path except the façade
//! lookup, which only runs when a violation is already firing. They are therefore all driven off existing
//! indexes — `idx_nodes_file` for the directory-prefix ranges,
//! `idx_nodes_name_kind` for the name lookup, and `idx_edges_source_kind` for
//! the edge joins — and never scan `edges.file_path`, which has no index.

use rusqlite::params;

use crate::sqlite::SqliteGraphStore;
use crate::types::{Boundary, BoundaryTarget, ModuleBoundaryInfo};

/// `keel_meta` key stamped by `keel map` on completion.
///
/// Its presence is the "this graph has been mapped at least once" signal the
/// W009 bootstrap guard needs: before a first map, the graph holds no edges at
/// all and every dependency would read as new.
pub const LAST_MAP_AT: &str = "last_map_at";

/// Cap on how many distinct reference names one file contributes to the
/// boundary lookup. Bounds the generated `IN (...)` list on pathological
/// machine-generated files; ordinary files sit far below it.
const MAX_LOOKUP_NAMES: usize = 256;

/// Half-open key range covering every path under `dir`, as
/// `("dir/", "dir0")` — `'0'` is the byte after `'/'`, so the pair brackets
/// exactly the `dir/` prefix and SQLite can answer it from `idx_nodes_file`
/// with a range scan instead of a `LIKE` table scan.
fn prefix_range(dir: &str) -> (String, String) {
    (format!("{dir}/"), format!("{dir}0"))
}

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
    /// Used by `keel map` to stamp `last_map_at`, the marker W009's bootstrap
    /// guard reads to tell "no cross-boundary edges because none exist" apart
    /// from "no cross-boundary edges because nothing was ever mapped".
    pub fn set_meta_value(&self, key: &str, value: &str) -> Result<(), crate::types::GraphError> {
        self.conn.execute(
            "INSERT INTO keel_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Everything boundary analysis needs to know about the module (directory)
    /// holding a compiled file: its declared package and every boundary its
    /// stored `calls` edges already reach.
    ///
    /// The baseline is deliberately the MODULE's, not the file's. keel's own
    /// resolution is imperfect and asymmetric — a cross-crate call the map
    /// leaves unresolved has no stored edge, so a file-scoped baseline would
    /// report keel's resolution gaps as architecture changes on an unchanged
    /// tree, forever. The module is also the unit the bootstrap guard uses, so
    /// one query answers both.
    pub(crate) fn query_module_boundary_info(&self, dir: &str) -> ModuleBoundaryInfo {
        let (lo, hi) = prefix_range(dir);
        let package = self
            .conn
            .query_row(
                "SELECT package FROM nodes
                 WHERE file_path >= ?1 AND file_path < ?2
                   AND package IS NOT NULL AND package != ''
                 LIMIT 1",
                params![lo, hi],
                |row| row.get::<_, String>(0),
            )
            .ok();
        let mut stmt = match self.conn.prepare(
            "SELECT DISTINCT tn.package, tn.file_path
             FROM nodes sn
             JOIN edges e ON e.source_id = sn.id AND e.kind = 'calls'
             JOIN nodes tn ON tn.id = e.target_id
             WHERE sn.file_path >= ?1 AND sn.file_path < ?2",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[keel] module_boundary_info: prepare failed: {e}");
                return ModuleBoundaryInfo {
                    package,
                    call_targets: Vec::new(),
                };
            }
        };
        let rows = stmt.query_map(params![lo, hi], |row| {
            Ok(BoundaryTarget {
                name: String::new(),
                package: row.get::<_, Option<String>>(0)?,
                file_path: row.get::<_, String>(1)?,
            })
        });
        let call_targets = match rows {
            Ok(r) => r.filter_map(|x| x.ok()).collect(),
            Err(e) => {
                eprintln!("[keel] module_boundary_info: query failed: {e}");
                Vec::new()
            }
        };
        ModuleBoundaryInfo {
            package,
            call_targets,
        }
    }

    /// Stored PUBLIC, non-associated function nodes matching any of `names`,
    /// outside `exclude_file`.
    ///
    /// Both filters exist because a bare name is weak evidence and the graph
    /// says so: an associated item is addressed as `Type::name`/`obj.name`, so
    /// `from`, `collect` and `get_edges` collide across every package that
    /// implements a trait, and a private function cannot be the target of a
    /// cross-boundary call in any language keel parses. Dropping both classes
    /// keeps the check on names that can genuinely name another package's
    /// surface.
    pub(crate) fn query_boundary_targets(
        &self,
        names: &[&str],
        exclude_file: &str,
    ) -> Vec<BoundaryTarget> {
        if names.is_empty() {
            return Vec::new();
        }
        let names = &names[..names.len().min(MAX_LOOKUP_NAMES)];
        let placeholders = std::iter::repeat_n("?", names.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT name, package, file_path FROM nodes
             WHERE kind = 'function' AND name IN ({placeholders}) AND file_path != ?
               AND is_associated = 0 AND is_public = 1"
        );
        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[keel] find_boundary_targets: prepare failed: {e}");
                return Vec::new();
            }
        };
        let mut bound: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(names.len() + 1);
        for n in names {
            bound.push(n);
        }
        bound.push(&exclude_file);
        let rows = stmt.query_map(bound.as_slice(), |row| {
            Ok(BoundaryTarget {
                name: row.get::<_, String>(0)?,
                package: row.get::<_, Option<String>>(1)?,
                file_path: row.get::<_, String>(2)?,
            })
        });
        match rows {
            Ok(r) => r.filter_map(|x| x.ok()).collect(),
            Err(e) => {
                eprintln!("[keel] find_boundary_targets: query failed: {e}");
                Vec::new()
            }
        }
    }

    /// Most-called public function inside `boundary`, ties broken by name so
    /// the suggestion is stable across runs.
    pub(crate) fn query_boundary_facade(&self, boundary: &Boundary) -> Option<String> {
        match boundary {
            Boundary::Package(pkg) => self
                .conn
                .query_row(
                    "SELECT n.name FROM nodes n
                     LEFT JOIN edges e ON e.target_id = n.id AND e.kind = 'calls'
                     WHERE n.kind = 'function' AND n.is_public = 1 AND n.package = ?1
                     GROUP BY n.id
                     ORDER BY COUNT(e.id) DESC, n.name ASC
                     LIMIT 1",
                    params![pkg],
                    |row| row.get::<_, String>(0),
                )
                .ok(),
            Boundary::Directory(dir) => {
                let (lo, hi) = prefix_range(dir);
                self.conn
                    .query_row(
                        "SELECT n.name FROM nodes n
                         LEFT JOIN edges e ON e.target_id = n.id AND e.kind = 'calls'
                         WHERE n.kind = 'function' AND n.is_public = 1
                           AND (n.package IS NULL OR n.package = '')
                           AND n.file_path >= ?1 AND n.file_path < ?2
                         GROUP BY n.id
                         ORDER BY COUNT(e.id) DESC, n.name ASC
                         LIMIT 1",
                        params![lo, hi],
                        |row| row.get::<_, String>(0),
                    )
                    .ok()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::prefix_range;

    #[test]
    fn test_prefix_range_brackets_the_directory() {
        let (lo, hi) = prefix_range("crates/core/src");
        assert_eq!(lo, "crates/core/src/");
        assert_eq!(hi, "crates/core/src0");
        // Every path under the directory sorts inside the half-open range...
        assert!("crates/core/src/lib.rs" >= lo.as_str());
        assert!("crates/core/src/lib.rs" < hi.as_str());
        // ...and a sibling directory sharing the prefix does not.
        assert!("crates/core/srcs/lib.rs" >= hi.as_str());
    }
}
