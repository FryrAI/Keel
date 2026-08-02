use serde::Serialize;

/// Re-exported so formatters can name the telemetry rollup carried by
/// [`StatsResult`] without depending on `keel-core` directly.
pub use keel_core::telemetry::TelemetryAggregate;

/// One `keel stats` reading: the graph totals, the optional 30-day telemetry
/// rollup, and the two `--verbose` extras.
///
/// `Serialize` only — `TelemetryAggregate` is write-only too, and nothing ever
/// reads a stats report back.
#[derive(Debug, Clone, Serialize)]
pub struct StatsResult {
    pub version: String,
    pub command: String,
    pub modules: usize,
    pub functions: u32,
    pub files: usize,
    pub edges: u32,
    pub uses_edges: u32,
    /// Per-kind edge breakdown, printed by the human format only. Skipped in
    /// JSON because `--json` has never carried these keys and widening a
    /// machine contract is not a formatting change.
    #[serde(skip)]
    pub calls_edges: u32,
    #[serde(skip)]
    pub imports_edges: u32,
    #[serde(skip)]
    pub contains_edges: u32,
    /// Absent when the project has no `telemetry.db` or has recorded nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<TelemetryAggregate>,
    /// `--verbose` only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_path: Option<String>,
    /// `--verbose` only, and absent when the version row is unreadable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<u32>,
}
