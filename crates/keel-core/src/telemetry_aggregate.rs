//! Aggregation logic for telemetry events.

use std::collections::HashMap;

use super::telemetry::{AgentStats, TelemetryAggregate, TelemetryStore};
use crate::types::GraphError;

impl TelemetryStore {
    /// Aggregate telemetry over the last N days.
    pub fn aggregate(&self, days: u32) -> Result<TelemetryAggregate, GraphError> {
        let cutoff = format!("datetime('now', '-{} days')", days);

        let total: u64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM events WHERE timestamp >= {cutoff}"),
            [],
            |r| r.get(0),
        )?;

        let total_errors: u64 = self.conn.query_row(
            &format!(
                "SELECT COALESCE(SUM(error_count), 0) FROM events WHERE timestamp >= {cutoff}"
            ),
            [],
            |r| r.get(0),
        )?;

        let total_warnings: u64 = self.conn.query_row(
            &format!(
                "SELECT COALESCE(SUM(warning_count), 0) FROM events WHERE timestamp >= {cutoff}"
            ),
            [],
            |r| r.get(0),
        )?;

        let avg_compile: Option<f64> = self.conn.query_row(
            &format!(
                "SELECT AVG(duration_ms) FROM events WHERE command = 'compile' AND timestamp >= {cutoff}"
            ),
            [],
            |r| r.get(0),
        )?;

        let avg_map: Option<f64> = self.conn.query_row(
            &format!(
                "SELECT AVG(duration_ms) FROM events WHERE command = 'map' AND timestamp >= {cutoff}"
            ),
            [],
            |r| r.get(0),
        )?;

        // Command counts
        let mut cmd_stmt = self.conn.prepare(
            &format!(
                "SELECT command, COUNT(*) FROM events WHERE timestamp >= {cutoff} GROUP BY command ORDER BY COUNT(*) DESC"
            ),
        )?;
        let mut command_counts = HashMap::new();
        let cmd_rows = cmd_stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })?;
        for row in cmd_rows {
            let (cmd, count) = row?;
            command_counts.insert(cmd, count);
        }

        // Language percentages — use the latest `map` event's language_mix
        // (most accurate snapshot since map scans all files).
        // Falls back to latest event with any language_mix if no map events exist.
        let language_percentages: HashMap<String, f64> = {
            let mut lang_stmt = self.conn.prepare(&format!(
                "SELECT language_mix FROM events \
                 WHERE timestamp >= {cutoff} AND command = 'map' AND language_mix != '{{}}' \
                 ORDER BY id DESC LIMIT 1"
            ))?;
            let result: Option<String> =
                lang_stmt.query_row([], |row| row.get::<_, String>(0)).ok();

            // Fallback: any event with a non-empty language_mix
            let json_str = match result {
                Some(s) => s,
                None => {
                    let mut fallback = self.conn.prepare(&format!(
                        "SELECT language_mix FROM events \
                         WHERE timestamp >= {cutoff} AND language_mix != '{{}}' \
                         ORDER BY id DESC LIMIT 1"
                    ))?;
                    fallback
                        .query_row([], |row| row.get::<_, String>(0))
                        .unwrap_or_default()
                }
            };

            if let Ok(map) = serde_json::from_str::<HashMap<String, u32>>(&json_str) {
                map.into_iter().map(|(k, v)| (k, v as f64)).collect()
            } else {
                HashMap::new()
            }
        };

        // Error code aggregation
        let mut codes_stmt = self.conn.prepare(&format!(
            "SELECT error_codes FROM events WHERE timestamp >= {cutoff}"
        ))?;
        let mut top_error_codes: HashMap<String, u64> = HashMap::new();
        let codes_rows = codes_stmt.query_map([], |row| row.get::<_, Option<String>>(0))?;
        for row in codes_rows {
            if let Some(json_str) = row? {
                if let Ok(map) = serde_json::from_str::<HashMap<String, u32>>(&json_str) {
                    for (code, count) in map {
                        *top_error_codes.entry(code).or_default() += count as u64;
                    }
                }
            }
        }

        // Agent stats aggregation
        let mut agent_stats: HashMap<String, AgentStats> = HashMap::new();
        let mut agent_stmt = self.conn.prepare(&format!(
            "SELECT command, client_name, node_count FROM events WHERE client_name IS NOT NULL AND timestamp >= {cutoff}"
        ))?;
        let agent_rows = agent_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
            ))
        })?;
        for row in agent_rows {
            let (command, client, node_count) = row?;
            let stats = agent_stats.entry(client).or_default();
            if command == "mcp:session" {
                stats.sessions += 1;
                stats.total_tool_calls += node_count as u64;
            } else if command.starts_with("mcp:") {
                *stats.tool_usage.entry(command).or_default() += 1;
            }
        }
        // Compute averages
        for stats in agent_stats.values_mut() {
            if stats.sessions > 0 {
                stats.avg_tool_calls_per_session =
                    stats.total_tool_calls as f64 / stats.sessions as f64;
            }
        }

        Ok(TelemetryAggregate {
            total_invocations: total,
            avg_compile_ms: avg_compile,
            avg_map_ms: avg_map,
            total_errors,
            total_warnings,
            command_counts,
            language_percentages,
            top_error_codes,
            agent_stats,
        })
    }
}
