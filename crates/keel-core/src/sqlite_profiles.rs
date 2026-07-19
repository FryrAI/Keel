//! Module-profile persistence for the SQLite graph store.
//!
//! Split from `sqlite.rs` to keep that file under the 400-line cap.

use rusqlite::params;

use crate::sqlite::SqliteGraphStore;
use crate::types::GraphError;

impl SqliteGraphStore {
    /// Insert or update module profiles in bulk.
    /// Uses INSERT ... ON CONFLICT DO UPDATE for upsert semantics.
    pub fn upsert_module_profiles(
        &self,
        profiles: Vec<crate::types::ModuleProfile>,
    ) -> Result<(), GraphError> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO module_profiles (
                    module_id, path, function_count, class_count, line_count,
                    function_name_prefixes, primary_types, import_sources,
                    export_targets, external_endpoint_count, responsibility_keywords
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ON CONFLICT(module_id) DO UPDATE SET
                    path = excluded.path,
                    function_count = excluded.function_count,
                    class_count = excluded.class_count,
                    line_count = excluded.line_count,
                    function_name_prefixes = excluded.function_name_prefixes,
                    primary_types = excluded.primary_types,
                    import_sources = excluded.import_sources,
                    export_targets = excluded.export_targets,
                    external_endpoint_count = excluded.external_endpoint_count,
                    responsibility_keywords = excluded.responsibility_keywords",
            )?;
            for p in &profiles {
                let prefixes_json = serde_json::to_string(&p.function_name_prefixes)
                    .unwrap_or_else(|_| "[]".to_string());
                let types_json =
                    serde_json::to_string(&p.primary_types).unwrap_or_else(|_| "[]".to_string());
                let imports_json =
                    serde_json::to_string(&p.import_sources).unwrap_or_else(|_| "[]".to_string());
                let exports_json =
                    serde_json::to_string(&p.export_targets).unwrap_or_else(|_| "[]".to_string());
                let keywords_json = serde_json::to_string(&p.responsibility_keywords)
                    .unwrap_or_else(|_| "[]".to_string());
                stmt.execute(params![
                    p.module_id,
                    p.path,
                    p.function_count,
                    p.class_count,
                    p.line_count,
                    prefixes_json,
                    types_json,
                    imports_json,
                    exports_json,
                    p.external_endpoint_count,
                    keywords_json,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
