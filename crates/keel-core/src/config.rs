//! Configuration file loading for keel.
//!
//! Reads `.keel/keel.json` and provides typed access to all settings.
//! Falls back to sensible defaults when the config file is missing or incomplete.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Top-level keel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// Enforcement severity toggles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforceConfig {
    #[serde(default = "default_true")]
    pub type_hints: bool,
    #[serde(default = "default_true")]
    pub docstrings: bool,
    #[serde(default = "default_true")]
    pub placement: bool,
}

/// Circuit breaker tuning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    #[serde(default = "default_max_failures")]
    pub max_failures: u32,
}

/// Batch mode tuning.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Default for EnforceConfig {
    fn default() -> Self {
        Self {
            type_hints: true,
            docstrings: true,
            placement: true,
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
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_default_config() {
        let cfg = KeelConfig::default();
        assert_eq!(cfg.version, "0.1.0");
        assert_eq!(cfg.circuit_breaker.max_failures, 3);
        assert_eq!(cfg.batch.timeout_seconds, 60);
        assert!(cfg.enforce.type_hints);
        assert!(cfg.enforce.docstrings);
        assert!(cfg.enforce.placement);
    }

    #[test]
    fn test_load_missing_file() {
        let cfg = KeelConfig::load(Path::new("/nonexistent"));
        assert_eq!(cfg.circuit_breaker.max_failures, 3);
    }

    #[test]
    fn test_load_valid_config() {
        let dir = tempfile::tempdir().unwrap();
        let config = serde_json::json!({
            "version": "0.2.0",
            "languages": ["typescript", "python"],
            "circuit_breaker": { "max_failures": 5 },
            "batch": { "timeout_seconds": 120 }
        });
        fs::write(dir.path().join("keel.json"), config.to_string()).unwrap();
        let cfg = KeelConfig::load(dir.path());
        assert_eq!(cfg.version, "0.2.0");
        assert_eq!(cfg.circuit_breaker.max_failures, 5);
        assert_eq!(cfg.batch.timeout_seconds, 120);
        assert_eq!(cfg.languages, vec!["typescript", "python"]);
    }

    #[test]
    fn test_load_partial_config() {
        let dir = tempfile::tempdir().unwrap();
        let config = serde_json::json!({
            "version": "0.1.0",
            "languages": ["go"]
        });
        fs::write(dir.path().join("keel.json"), config.to_string()).unwrap();
        let cfg = KeelConfig::load(dir.path());
        assert_eq!(cfg.circuit_breaker.max_failures, 3); // default
        assert_eq!(cfg.batch.timeout_seconds, 60); // default
        assert!(cfg.enforce.type_hints); // default
    }
}
