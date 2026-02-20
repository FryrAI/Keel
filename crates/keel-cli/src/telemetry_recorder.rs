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

    try_send_remote(config, &event);
}

/// Fire-and-forget remote telemetry send.
/// Spawns a background thread so the CLI never blocks on network I/O.
/// Silently swallows all errors — telemetry must never degrade UX.
fn try_send_remote(config: &KeelConfig, event: &telemetry::TelemetryEvent) {
    if !config.telemetry.remote {
        return;
    }

    let endpoint = config.telemetry.effective_endpoint().to_string();
    let body = match serde_json::to_string(event) {
        Ok(b) => b,
        Err(_) => return,
    };

    std::thread::spawn(move || {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(5)))
            .build()
            .new_agent();
        let _ = agent
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .send(body.as_bytes());
    });
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
        Commands::Context { .. } => "context",
        Commands::Serve { .. } => "serve",
        Commands::Watch => "watch",
        Commands::Deinit => "deinit",
        Commands::Stats => "stats",
        Commands::Config { .. } => "config",
        Commands::Upgrade { .. } => "upgrade",
        Commands::Completion { .. } => "completion",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_core::config::{KeelConfig, TelemetryConfig};
    use keel_core::telemetry;

    #[test]
    fn test_try_send_remote_skips_when_disabled() {
        let config = KeelConfig {
            telemetry: TelemetryConfig {
                enabled: true,
                remote: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let event = telemetry::new_event("compile", 100, 0);
        // Should return immediately without attempting network I/O
        try_send_remote(&config, &event);
    }

    #[test]
    fn test_telemetry_event_serializes() {
        let mut event = telemetry::new_event("compile", 150, 0);
        event.error_count = 2;
        event.warning_count = 5;
        event.node_count = 100;
        event.edge_count = 200;
        event.language_mix.insert("typescript".to_string(), 60);

        let json = serde_json::to_string(&event).expect("TelemetryEvent should serialize");
        assert!(json.contains("\"command\":\"compile\""));
        assert!(json.contains("\"duration_ms\":150"));
        assert!(json.contains("\"error_count\":2"));
        assert!(json.contains("\"typescript\":60"));
    }
}
