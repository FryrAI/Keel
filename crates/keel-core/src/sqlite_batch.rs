use std::collections::HashMap;

use crate::sqlite::SqliteGraphStore;
use crate::types::{ExternalEndpoint, GraphNode};

impl SqliteGraphStore {
    /// Batch-load endpoints for multiple nodes in a single query.
    /// Replaces N individual load_endpoints() calls with 1 query.
    pub(crate) fn batch_load_endpoints(
        &self,
        node_ids: &[u64],
    ) -> HashMap<u64, Vec<ExternalEndpoint>> {
        if node_ids.is_empty() {
            return HashMap::new();
        }
        let placeholders: Vec<String> = (1..=node_ids.len()).map(|i| format!("?{}", i)).collect();
        let sql = format!(
            "SELECT node_id, kind, method, path, direction FROM external_endpoints WHERE node_id IN ({})",
            placeholders.join(", ")
        );
        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[keel] batch_load_endpoints: prepare failed: {e}");
                return HashMap::new();
            }
        };
        let params: Vec<&dyn rusqlite::ToSql> =
            node_ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        let rows = match stmt.query_map(params.as_slice(), |row| {
            Ok((
                row.get::<_, u64>(0)?,
                ExternalEndpoint {
                    kind: row.get(1)?,
                    method: row.get(2)?,
                    path: row.get(3)?,
                    direction: row.get(4)?,
                },
            ))
        }) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[keel] batch_load_endpoints: query failed: {e}");
                return HashMap::new();
            }
        };
        let mut map: HashMap<u64, Vec<ExternalEndpoint>> = HashMap::new();
        for row in rows.filter_map(|r| r.ok()) {
            map.entry(row.0).or_default().push(row.1);
        }
        map
    }

    /// Batch-load previous hashes for multiple nodes in a single query.
    /// Replaces N individual load_previous_hashes() calls with 1 query.
    pub(crate) fn batch_load_previous_hashes(
        &self,
        node_ids: &[u64],
    ) -> HashMap<u64, Vec<String>> {
        if node_ids.is_empty() {
            return HashMap::new();
        }
        let placeholders: Vec<String> = (1..=node_ids.len()).map(|i| format!("?{}", i)).collect();
        let sql = format!(
            "SELECT node_id, hash FROM previous_hashes WHERE node_id IN ({}) ORDER BY created_at DESC",
            placeholders.join(", ")
        );
        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[keel] batch_load_previous_hashes: prepare failed: {e}");
                return HashMap::new();
            }
        };
        let params: Vec<&dyn rusqlite::ToSql> =
            node_ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        let rows = match stmt.query_map(params.as_slice(), |row| {
            Ok((row.get::<_, u64>(0)?, row.get::<_, String>(1)?))
        }) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[keel] batch_load_previous_hashes: query failed: {e}");
                return HashMap::new();
            }
        };
        let mut map: HashMap<u64, Vec<String>> = HashMap::new();
        for row in rows.filter_map(|r| r.ok()) {
            let hashes = map.entry(row.0).or_default();
            // Limit to 3 per node (matching single-load behavior)
            if hashes.len() < 3 {
                hashes.push(row.1);
            }
        }
        map
    }

    /// Attach relations (endpoints + previous_hashes) to a batch of nodes
    /// using only 2 queries total instead of 2*N.
    pub(crate) fn nodes_with_relations_batch(&self, nodes: Vec<GraphNode>) -> Vec<GraphNode> {
        if nodes.is_empty() {
            return nodes;
        }
        let ids: Vec<u64> = nodes.iter().map(|n| n.id).collect();
        let mut endpoints_map = self.batch_load_endpoints(&ids);
        let mut hashes_map = self.batch_load_previous_hashes(&ids);
        nodes
            .into_iter()
            .map(|mut n| {
                n.external_endpoints = endpoints_map.remove(&n.id).unwrap_or_default();
                n.previous_hashes = hashes_map.remove(&n.id).unwrap_or_default();
                n
            })
            .collect()
    }
}
