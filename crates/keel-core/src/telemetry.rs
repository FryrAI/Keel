//! Privacy-safe telemetry engine for keel.
//!
//! Stores aggregate command metrics in a separate `telemetry.db` SQLite database.
//! **By design**, no fields exist for file paths, function names, source code,
//! git history, or any user-identifiable information.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection};

use crate::types::GraphError;

/// A single telemetry event recorded after a command completes.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TelemetryEvent {
    pub id: Option<i64>,
    pub timestamp: String,
    pub command: String,
    pub duration_ms: u64,
    pub exit_code: i32,
    pub error_count: u32,
    pub warning_count: u32,
    /// Graph node count for CLI commands; repurposed as tool_call_count for MCP session events.
    pub node_count: u32,
    pub edge_count: u32,
    pub language_mix: HashMap<String, u32>,
    pub resolution_tiers: HashMap<String, u32>,
    pub circuit_breaker_events: u32,
    pub error_codes: HashMap<String, u32>,
    pub client_name: Option<String>,
}

/// Per-agent adoption metrics.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AgentStats {
    pub sessions: u64,
    pub total_tool_calls: u64,
    pub avg_tool_calls_per_session: f64,
    pub tool_usage: HashMap<String, u64>,
}

/// Aggregated telemetry over a time window.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TelemetryAggregate {
    pub total_invocations: u64,
    pub avg_compile_ms: Option<f64>,
    pub avg_map_ms: Option<f64>,
    pub total_errors: u64,
    pub total_warnings: u64,
    pub command_counts: HashMap<String, u64>,
    pub language_percentages: HashMap<String, f64>,
    pub top_error_codes: HashMap<String, u64>,
    pub agent_stats: HashMap<String, AgentStats>,
}

/// SQLite-backed telemetry store (separate from graph.db).
pub struct TelemetryStore {
    conn: Connection,
}

impl TelemetryStore {
    /// Open or create `telemetry.db` at the given path.
    pub fn open(path: &Path) -> Result<Self, GraphError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        let store = Self { conn };
        store.initialize_schema()?;
        Ok(store)
    }

    /// Create an in-memory telemetry store (for testing).
    pub fn in_memory() -> Result<Self, GraphError> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.initialize_schema()?;
        Ok(store)
    }

    fn initialize_schema(&self) -> Result<(), GraphError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                command TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                exit_code INTEGER NOT NULL,
                error_count INTEGER DEFAULT 0,
                warning_count INTEGER DEFAULT 0,
                node_count INTEGER DEFAULT 0,
                edge_count INTEGER DEFAULT 0,
                language_mix TEXT DEFAULT '{}',
                resolution_tiers TEXT DEFAULT '{}',
                circuit_breaker_events INTEGER DEFAULT 0,
                error_codes TEXT DEFAULT '{}',
                client_name TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_events_command ON events(command);",
        )?;
        // Migrate existing tables that lack the new columns
        let _ = self
            .conn
            .execute_batch("ALTER TABLE events ADD COLUMN error_codes TEXT DEFAULT '{}'");
        let _ = self
            .conn
            .execute_batch("ALTER TABLE events ADD COLUMN client_name TEXT");
        Ok(())
    }

    /// Record a single telemetry event.
    pub fn record(&self, event: &TelemetryEvent) -> Result<(), GraphError> {
        let lang_json = serde_json::to_string(&event.language_mix).unwrap_or_default();
        let tier_json = serde_json::to_string(&event.resolution_tiers).unwrap_or_default();
        let codes_json = serde_json::to_string(&event.error_codes).unwrap_or_default();
        self.conn.execute(
            "INSERT INTO events (timestamp, command, duration_ms, exit_code,
             error_count, warning_count, node_count, edge_count,
             language_mix, resolution_tiers, circuit_breaker_events,
             error_codes, client_name)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                event.timestamp,
                event.command,
                event.duration_ms,
                event.exit_code,
                event.error_count,
                event.warning_count,
                event.node_count,
                event.edge_count,
                lang_json,
                tier_json,
                event.circuit_breaker_events,
                codes_json,
                event.client_name,
            ],
        )?;
        Ok(())
    }

    /// Retrieve recent events, most recent first.
    pub fn recent_events(&self, limit: u32) -> Result<Vec<TelemetryEvent>, GraphError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, command, duration_ms, exit_code,
                    error_count, warning_count, node_count, edge_count,
                    language_mix, resolution_tiers, circuit_breaker_events,
                    error_codes, client_name
             FROM events ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            let lang_str: String = row.get(9)?;
            let tier_str: String = row.get(10)?;
            let codes_str: String = row.get::<_, Option<String>>(12)?.unwrap_or_default();
            Ok(TelemetryEvent {
                id: Some(row.get(0)?),
                timestamp: row.get(1)?,
                command: row.get(2)?,
                duration_ms: row.get(3)?,
                exit_code: row.get(4)?,
                error_count: row.get(5)?,
                warning_count: row.get(6)?,
                node_count: row.get(7)?,
                edge_count: row.get(8)?,
                language_mix: serde_json::from_str(&lang_str).unwrap_or_default(),
                resolution_tiers: serde_json::from_str(&tier_str).unwrap_or_default(),
                circuit_breaker_events: row.get(11)?,
                error_codes: serde_json::from_str(&codes_str).unwrap_or_default(),
                client_name: row.get(13)?,
            })
        })?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

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

    /// Delete events older than N days.
    pub fn prune(&self, days: u32) -> Result<u64, GraphError> {
        let deleted = self.conn.execute(
            &format!(
                "DELETE FROM events WHERE timestamp < datetime('now', '-{} days')",
                days
            ),
            [],
        )?;
        Ok(deleted as u64)
    }
}

/// Create a new `TelemetryEvent` with the current UTC timestamp.
pub fn new_event(command: &str, duration_ms: u64, exit_code: i32) -> TelemetryEvent {
    TelemetryEvent {
        id: None,
        timestamp: chrono_utc_now(),
        command: command.to_string(),
        duration_ms,
        exit_code,
        error_count: 0,
        warning_count: 0,
        node_count: 0,
        edge_count: 0,
        language_mix: HashMap::new(),
        resolution_tiers: HashMap::new(),
        circuit_breaker_events: 0,
        error_codes: HashMap::new(),
        client_name: None,
    }
}

/// UTC timestamp in SQLite native format (`YYYY-MM-DD HH:MM:SS`).
fn chrono_utc_now() -> String {
    // Use SystemTime for a dependency-free UTC timestamp
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate date from days since epoch (1970-01-01)
    let (year, month, day) = days_to_ymd(days_since_epoch);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Simplified Gregorian calendar conversion
    let mut year = 1970;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let months: [u64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1;
    for &m in &months {
        if days < m {
            break;
        }
        days -= m;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

#[cfg(test)]
#[path = "telemetry_tests.rs"]
mod tests;
