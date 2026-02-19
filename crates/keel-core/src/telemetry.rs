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
    pub node_count: u32,
    pub edge_count: u32,
    pub language_mix: HashMap<String, u32>,
    pub resolution_tiers: HashMap<String, u32>,
    pub circuit_breaker_events: u32,
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
                circuit_breaker_events INTEGER DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_events_command ON events(command);",
        )?;
        Ok(())
    }

    /// Record a single telemetry event.
    pub fn record(&self, event: &TelemetryEvent) -> Result<(), GraphError> {
        let lang_json = serde_json::to_string(&event.language_mix).unwrap_or_default();
        let tier_json = serde_json::to_string(&event.resolution_tiers).unwrap_or_default();
        self.conn.execute(
            "INSERT INTO events (timestamp, command, duration_ms, exit_code,
             error_count, warning_count, node_count, edge_count,
             language_mix, resolution_tiers, circuit_breaker_events)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
            ],
        )?;
        Ok(())
    }

    /// Retrieve recent events, most recent first.
    pub fn recent_events(&self, limit: u32) -> Result<Vec<TelemetryEvent>, GraphError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, command, duration_ms, exit_code,
                    error_count, warning_count, node_count, edge_count,
                    language_mix, resolution_tiers, circuit_breaker_events
             FROM events ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            let lang_str: String = row.get(9)?;
            let tier_str: String = row.get(10)?;
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
        let cutoff = format!(
            "datetime('now', '-{} days')",
            days
        );

        let total: u64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM events WHERE timestamp >= {cutoff}"),
            [],
            |r| r.get(0),
        )?;

        let total_errors: u64 = self.conn.query_row(
            &format!("SELECT COALESCE(SUM(error_count), 0) FROM events WHERE timestamp >= {cutoff}"),
            [],
            |r| r.get(0),
        )?;

        let total_warnings: u64 = self.conn.query_row(
            &format!("SELECT COALESCE(SUM(warning_count), 0) FROM events WHERE timestamp >= {cutoff}"),
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

        // Language percentages — average across all events
        let mut lang_stmt = self.conn.prepare(
            &format!("SELECT language_mix FROM events WHERE timestamp >= {cutoff}"),
        )?;
        let mut lang_totals: HashMap<String, f64> = HashMap::new();
        let mut lang_count = 0u64;
        let lang_rows = lang_stmt.query_map([], |row| row.get::<_, String>(0))?;
        for row in lang_rows {
            let json_str = row?;
            if let Ok(map) = serde_json::from_str::<HashMap<String, u32>>(&json_str) {
                if !map.is_empty() {
                    lang_count += 1;
                    for (lang, pct) in map {
                        *lang_totals.entry(lang).or_default() += pct as f64;
                    }
                }
            }
        }
        let language_percentages: HashMap<String, f64> = if lang_count > 0 {
            lang_totals.into_iter().map(|(k, v)| (k, v / lang_count as f64)).collect()
        } else {
            HashMap::new()
        };

        Ok(TelemetryAggregate {
            total_invocations: total,
            avg_compile_ms: avg_compile,
            avg_map_ms: avg_map,
            total_errors,
            total_warnings,
            command_counts,
            language_percentages,
        })
    }

    /// Delete events older than N days.
    pub fn prune(&self, days: u32) -> Result<u64, GraphError> {
        let deleted = self.conn.execute(
            &format!("DELETE FROM events WHERE timestamp < datetime('now', '-{} days')", days),
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
    }
}

/// ISO 8601 UTC timestamp without pulling in chrono.
fn chrono_utc_now() -> String {
    // Use SystemTime for a dependency-free UTC timestamp
    let now = std::time::SystemTime::now();
    let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();
    // Convert to rough ISO 8601 — good enough for SQLite datetime comparisons
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate date from days since epoch (1970-01-01)
    let (year, month, day) = days_to_ymd(days_since_epoch);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
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
