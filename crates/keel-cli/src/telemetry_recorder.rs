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
    pub error_codes: std::collections::HashMap<String, u32>,
    pub client_name: Option<String>,
    pub violations_resolved: u32,
    pub violations_persisted: u32,
    pub violations_new: u32,
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
    event.error_codes = metrics.error_codes;
    event.client_name = metrics.client_name;
    event.violations_resolved = metrics.violations_resolved;
    event.violations_persisted = metrics.violations_persisted;
    event.violations_new = metrics.violations_new;

    let _ = store.record(&event);

    // Enforce 90-day retention — silently prune old events
    let _ = store.prune(90);

    try_send_remote(config, keel_dir, &event);
}

/// Sanitized payload for remote telemetry. Strips `id`, truncates timestamp
/// to hour, and buckets node/edge counts to prevent fingerprinting.
#[derive(Debug, serde::Serialize)]
struct RemotePayload {
    project_hash: String,
    version: String,
    timestamp_hour: String,
    command: String,
    duration_ms: u64,
    exit_code: i32,
    error_count: u32,
    warning_count: u32,
    node_count_bucket: String,
    edge_count_bucket: String,
    language_mix: std::collections::HashMap<String, u32>,
    resolution_tiers: std::collections::HashMap<String, u32>,
    circuit_breaker_events: u32,
    error_codes: std::collections::HashMap<String, u32>,
    client_name: Option<String>,
    violations_resolved: u32,
    violations_persisted: u32,
    violations_new: u32,
}

/// Compute a privacy-safe hash of the project root path.
/// Uses xxhash64 of the canonicalized path, formatted as hex.
fn compute_project_hash(keel_dir: &Path) -> String {
    let project_root = keel_dir.parent().unwrap_or(keel_dir);
    let canonical = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let hash = xxhash_rust::xxh64::xxh64(canonical.to_string_lossy().as_bytes(), 0);
    format!("{:016x}", hash)
}

/// Truncate a timestamp to the hour: `2026-02-23 14:35:00` → `2026-02-23 14:00:00`.
fn truncate_to_hour(ts: &str) -> String {
    // Timestamp format: YYYY-MM-DD HH:MM:SS (SQLite native)
    if ts.len() >= 13 {
        format!("{}:00:00", &ts[..13])
    } else {
        ts.to_string()
    }
}

/// Bucket a count into a human-readable range to prevent fingerprinting.
fn bucket_count(n: u32) -> String {
    match n {
        0 => "0".into(),
        1..=10 => "1-10".into(),
        11..=50 => "11-50".into(),
        51..=100 => "51-100".into(),
        101..=500 => "101-500".into(),
        501..=1000 => "501-1k".into(),
        1001..=5000 => "1k-5k".into(),
        5001..=10000 => "5k-10k".into(),
        _ => "10k+".into(),
    }
}

/// Build a sanitized remote payload from a telemetry event.
fn sanitize_for_remote(event: &telemetry::TelemetryEvent, keel_dir: &Path) -> RemotePayload {
    RemotePayload {
        project_hash: compute_project_hash(keel_dir),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp_hour: truncate_to_hour(&event.timestamp),
        command: event.command.clone(),
        duration_ms: event.duration_ms,
        exit_code: event.exit_code,
        error_count: event.error_count,
        warning_count: event.warning_count,
        node_count_bucket: bucket_count(event.node_count),
        edge_count_bucket: bucket_count(event.edge_count),
        language_mix: event.language_mix.clone(),
        resolution_tiers: event.resolution_tiers.clone(),
        circuit_breaker_events: event.circuit_breaker_events,
        error_codes: event.error_codes.clone(),
        client_name: event.client_name.clone(),
        violations_resolved: event.violations_resolved,
        violations_persisted: event.violations_persisted,
        violations_new: event.violations_new,
    }
}

/// Fire-and-forget remote telemetry send.
/// Spawns a background thread so the CLI never blocks on network I/O.
/// Silently swallows all errors — telemetry must never degrade UX.
///
/// When the user is logged in, dual-sends: anonymous aggregate first,
/// then authenticated user-scoped telemetry. Both use the same thread
/// and ureq Agent (connection pooling).
fn try_send_remote(config: &KeelConfig, keel_dir: &Path, event: &telemetry::TelemetryEvent) {
    if !config.telemetry.remote {
        return;
    }

    let endpoint = config.telemetry.effective_endpoint().to_string();
    let payload = sanitize_for_remote(event, keel_dir);
    let body = match serde_json::to_string(&payload) {
        Ok(b) => b,
        Err(_) => return,
    };

    // Load credentials before spawning thread (fast fs read, ~1ms)
    let creds_token = crate::auth::load_credentials()
        .filter(|c| !c.is_expired())
        .map(|c| c.access_token);

    std::thread::spawn(move || {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(5)))
            .build()
            .new_agent();

        // 1. Anonymous aggregate (always)
        let _ = agent
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .send(body.as_bytes());

        // 2. User-scoped (only when logged in)
        if let Some(token) = creds_token {
            let user_endpoint = endpoint.replace("/telemetry", "/telemetry/user");
            let _ = agent
                .post(&user_endpoint)
                .header("Content-Type", "application/json")
                .header("Authorization", &format!("Bearer {token}"))
                .send(body.as_bytes());
        }
    });
}

/// Detect the calling agent/client from environment variables.
/// Returns `None` for direct human use.
pub fn detect_client() -> Option<String> {
    detect_client_with(|k| std::env::var(k).ok())
}

/// Testable version of `detect_client` that takes an env lookup function.
fn detect_client_with<F: Fn(&str) -> Option<String>>(env_var: F) -> Option<String> {
    if env_var("CLAUDECODE").is_some() {
        return Some("claude-code".into());
    }
    if env_var("CURSOR_CLI").is_some() {
        return Some("cursor".into());
    }
    if env_var("VSCODE_PID").is_some() {
        return Some("vscode".into());
    }
    if env_var("WINDSURF_SESSION").is_some() {
        return Some("windsurf".into());
    }
    if env_var("CI").is_some() {
        return Some("ci".into());
    }
    None
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
        Commands::Login => "login",
        Commands::Logout => "logout",
        Commands::Push { .. } => "push",
    }
}

#[cfg(test)]
#[path = "telemetry_recorder_tests.rs"]
mod tests;
