//! Configuration file loading for keel.
//!
//! Reads `.keel/keel.json` and provides typed access to all settings.
//! Falls back to sensible defaults when the config file is missing or incomplete.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Top-level keel configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeelConfig {
    pub version: String,
    pub languages: Vec<String>,
    #[serde(default)]
    pub enforce: EnforceConfig,
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    #[serde(default)]
    pub tier: Tier,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    #[serde(default)]
    pub monorepo: MonorepoConfig,
    #[serde(default)]
    pub tier3: Tier3Config,
    /// Stable random identifier for telemetry project deduplication.
    /// Generated at `keel init` time; avoids path-based hash inflation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telemetry_id: Option<String>,
}

/// Product tier — gates feature access.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    #[default]
    Free,
    Team,
    Enterprise,
}

/// Telemetry configuration — privacy-safe event tracking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub remote: bool,
    #[serde(default)]
    pub endpoint: Option<String>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            remote: true,
            endpoint: None,
        }
    }
}

impl TelemetryConfig {
    /// Returns the configured telemetry endpoint URL, falling back to the default keel API.
    pub fn effective_endpoint(&self) -> &str {
        self.endpoint
            .as_deref()
            .unwrap_or("https://keel.engineer/api/telemetry")
    }
}

/// Monorepo detection and cross-package configuration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MonorepoConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub packages: Vec<String>,
}

/// Tier 3 (LSP/SCIP) resolution configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tier3Config {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub scip_paths: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub lsp_commands: std::collections::HashMap<String, Vec<String>>,
    #[serde(default = "default_true")]
    pub prefer_scip: bool,
}

impl Default for Tier3Config {
    fn default() -> Self {
        Self {
            enabled: false,
            scip_paths: std::collections::HashMap::new(),
            lsp_commands: std::collections::HashMap::new(),
            prefer_scip: true,
        }
    }
}

/// Enforcement severity toggles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnforceConfig {
    #[serde(default = "default_true")]
    pub type_hints: bool,
    #[serde(default = "default_true")]
    pub docstrings: bool,
    #[serde(default = "default_true")]
    pub placement: bool,
    /// Progressive adoption: E002/E003 on functions the current change did
    /// NOT touch (stored hash unchanged) are reported as WARNING instead of
    /// ERROR, so adopting keel on a legacy repo doesn't flood errors.
    #[serde(default = "default_true")]
    pub progressive: bool,
    /// W005: warn on private functions with no callers in the graph.
    #[serde(default = "default_true")]
    pub dead_code: bool,
    /// W006: warn when a function body is identical (whitespace-normalized)
    /// to an existing function elsewhere in the graph.
    #[serde(default = "default_true")]
    pub duplication: bool,
    /// W007: warn when a compiled file exceeds `max_file_lines` and grew.
    #[serde(default = "default_true")]
    pub oversized_files: bool,
    /// Line budget used by the W007 oversized-file check.
    #[serde(default = "default_max_file_lines")]
    pub max_file_lines: u32,
}

/// Circuit breaker tuning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    #[serde(default = "default_max_failures")]
    pub max_failures: u32,
}

/// Batch mode tuning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchConfig {
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
}

fn default_true() -> bool {
    true
}
fn default_max_failures() -> u32 {
    3
}
fn default_timeout_seconds() -> u64 {
    60
}
fn default_max_file_lines() -> u32 {
    400
}

impl Default for EnforceConfig {
    fn default() -> Self {
        Self {
            type_hints: true,
            docstrings: true,
            placement: true,
            progressive: true,
            dead_code: true,
            duplication: true,
            oversized_files: true,
            max_file_lines: 400,
        }
    }
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            max_failures: default_max_failures(),
        }
    }
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: default_timeout_seconds(),
        }
    }
}

impl Default for KeelConfig {
    fn default() -> Self {
        Self {
            version: "0.1.0".to_string(),
            languages: vec![],
            enforce: EnforceConfig::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
            batch: BatchConfig::default(),
            ignore_patterns: vec![],
            tier: Tier::default(),
            telemetry: TelemetryConfig::default(),
            monorepo: MonorepoConfig::default(),
            tier3: Tier3Config::default(),
            telemetry_id: None,
        }
    }
}

impl KeelConfig {
    /// Load configuration from `.keel/keel.json` inside the given keel directory.
    /// Returns defaults if the file doesn't exist or can't be parsed.
    pub fn load(keel_dir: &Path) -> Self {
        let config_path = keel_dir.join("keel.json");
        let content = match std::fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };
        match serde_json::from_str(&content) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!(
                    "keel: warning: failed to parse {}: {}, using defaults",
                    config_path.display(),
                    e
                );
                Self::default()
            }
        }
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
