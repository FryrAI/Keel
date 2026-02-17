//! Wraps command execution with telemetry recording.
//!
//! Silently fails — telemetry never blocks the CLI.

use std::path::Path;
use std::time::Duration;

use keel_core::config::KeelConfig;
use keel_core::telemetry::{self, TelemetryStore};

/// Metrics collected during command execution.
#[derive(Debug, Default)]
pub struct EventMetrics {
    pub error_count: u32,
    pub warning_count: u32,
    pub node_count: u32,
    pub edge_count: u32,
    pub language_mix: std::collections::HashMap<String, u32>,
    pub resolution_tiers: std::collections::HashMap<String, u32>,
    pub circuit_breaker_events: u32,
}

/// Record a telemetry event after a command completes.
/// Silently returns on any failure — never blocks the CLI.
pub fn record_event(
    keel_dir: &Path,
    config: &KeelConfig,
    command: &str,
    duration: Duration,
    exit_code: i32,
    metrics: EventMetrics,
) {
    if !config.telemetry.enabled {
        return;
    }

    let db_path = keel_dir.join("telemetry.db");
    let store = match TelemetryStore::open(&db_path) {
        Ok(s) => s,
        Err(_) => return,
    };

    let mut event = telemetry::new_event(command, duration.as_millis() as u64, exit_code);
    event.error_count = metrics.error_count;
    event.warning_count = metrics.warning_count;
    event.node_count = metrics.node_count;
    event.edge_count = metrics.edge_count;
    event.language_mix = metrics.language_mix;
    event.resolution_tiers = metrics.resolution_tiers;
    event.circuit_breaker_events = metrics.circuit_breaker_events;

    let _ = store.record(&event);
}

/// Extract a static command name string from the CLI command variant.
pub fn command_name(command: &crate::cli_args::Commands) -> &'static str {
    use crate::cli_args::Commands;
    match command {
        Commands::Init { .. } => "init",
        Commands::Map { .. } => "map",
        Commands::Discover { .. } => "discover",
        Commands::Search { .. } => "search",
        Commands::Compile { .. } => "compile",
        Commands::Check { .. } => "check",
        Commands::Where { .. } => "where",
        Commands::Explain { .. } => "explain",
        Commands::Fix { .. } => "fix",
        Commands::Name { .. } => "name",
        Commands::Analyze { .. } => "analyze",
        Commands::Serve { .. } => "serve",
        Commands::Watch => "watch",
        Commands::Deinit => "deinit",
        Commands::Stats => "stats",
        Commands::Config { .. } => "config",
    }
}
