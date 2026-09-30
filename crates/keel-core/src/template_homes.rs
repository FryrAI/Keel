//! Map-derived template homes. Like fragment clones, these survive only until
//! the next map; idempotent additive DDL needs no schema-version migration.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::sqlite::SqliteGraphStore;
use crate::types::GraphError;

/// A pure template function and its fixed, decoded segments (minimum 16 chars).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateHome {
    /// Diagnostic hash, never part of baseline identity.
    pub hash: String,
    /// Function or method name.
    pub name: String,
    /// Project-relative source path.
    pub file: String,
    /// Function's first source line.
    pub line: u32,
    /// Unique eligible fixed segments; empty homes still count in the study.
    pub segments: Vec<String>,
}

impl SqliteGraphStore {
    /// Create the additive map cache on fresh and existing databases.
    pub(crate) fn initialize_template_homes(&self) -> Result<(), GraphError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS template_homes (
                id INTEGER PRIMARY KEY,
                file TEXT NOT NULL, line INTEGER NOT NULL, payload TEXT NOT NULL,
                CHECK (line > 0)
            );",
        )?;
        Ok(())
    }

    /// Atomically replace every mapped home.
    pub(crate) fn template_homes_replace(
        &mut self,
        homes: Vec<TemplateHome>,
    ) -> Result<(), GraphError> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM template_homes", [])?;
        for home in homes {
            let payload =
                serde_json::to_string(&home).map_err(|e| GraphError::Internal(e.to_string()))?;
            tx.execute(
                "INSERT INTO template_homes (file, line, payload) VALUES (?1, ?2, ?3)",
                params![home.file, home.line, payload],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Read mapped homes in stable source order.
    pub(crate) fn template_homes_read(&self) -> Vec<TemplateHome> {
        let mut stmt = match self
            .conn
            .prepare("SELECT payload FROM template_homes ORDER BY file, line, rowid")
        {
            Ok(stmt) => stmt,
            Err(e) => {
                eprintln!("keel: cannot read template homes: {e}");
                return Vec::new();
            }
        };
        let rows = stmt.query_map([], |row| row.get::<_, String>(0));
        match rows {
            Ok(rows) => rows
                .filter_map(|row| match row {
                    Ok(json) => match serde_json::from_str(&json) {
                        Ok(home) => Some(home),
                        Err(e) => {
                            eprintln!("keel: invalid template home: {e}");
                            None
                        }
                    },
                    Err(e) => {
                        eprintln!("keel: cannot read template home: {e}");
                        None
                    }
                })
                .collect(),
            Err(e) => {
                eprintln!("keel: cannot query template homes: {e}");
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::GraphStore;

    fn home() -> TemplateHome {
        TemplateHome {
            hash: "h".into(),
            name: "home".into(),
            file: "a.rs".into(),
            line: 1,
            segments: vec!["AT TIME ZONE 'Europe/Berlin')::date".into()],
        }
    }

    #[test]
    fn template_homes_roundtrip_replace_and_clear() {
        let mut store = SqliteGraphStore::in_memory().unwrap();
        store.replace_template_homes(vec![home()]).unwrap();
        assert_eq!(store.template_homes(), vec![home()]);
        store.replace_template_homes(Vec::new()).unwrap();
        assert!(store.template_homes().is_empty());
        store.replace_template_homes(vec![home()]).unwrap();
        store.clear_all().unwrap();
        assert!(store.template_homes().is_empty());
    }

    #[test]
    fn template_homes_same_line_preserve_every_owner() {
        let mut store = SqliteGraphStore::in_memory().unwrap();
        let mut other = home();
        other.name = "other".into();
        other.hash = "other-hash".into();
        let homes = vec![home(), other, home()];
        store.replace_template_homes(homes.clone()).unwrap();
        assert_eq!(store.template_homes(), homes);
    }

    #[test]
    fn template_homes_persist_and_old_database_opens_additively() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        {
            let mut store = SqliteGraphStore::open(path).unwrap();
            store
                .conn
                .execute_batch("DROP TABLE template_homes")
                .unwrap();
            store.initialize_template_homes().unwrap();
            store.replace_template_homes(vec![home()]).unwrap();
        }
        assert_eq!(
            SqliteGraphStore::open(path).unwrap().template_homes(),
            vec![home()]
        );
    }
}
