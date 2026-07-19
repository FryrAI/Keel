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
fn test_roundtrip_all_non_default_values() {
    // Build a KeelConfig with every field set to a non-default value.
    let original = KeelConfig {
        version: "99.88.77".to_string(),
        languages: vec![
            "typescript".to_string(),
            "python".to_string(),
            "go".to_string(),
            "rust".to_string(),
        ],
        enforce: EnforceConfig {
            type_hints: false,      // default is true
            docstrings: false,      // default is true
            placement: false,       // default is true
            progressive: false,     // default is true
            dead_code: false,       // default is true
            duplication: false,     // default is true
            oversized_files: false, // default is true
            max_file_lines: 123,    // default is 400
        },
        circuit_breaker: CircuitBreakerConfig {
            max_failures: 42, // default is 3
        },
        batch: BatchConfig {
            timeout_seconds: 999, // default is 60
        },
        ignore_patterns: vec![
            "vendor/**".to_string(),
            "node_modules/**".to_string(),
            "*.generated.ts".to_string(),
        ],
        tier: Tier::Enterprise,
        telemetry: TelemetryConfig {
            enabled: false,
            remote: false,
            endpoint: Some("https://custom.example.com/telemetry".to_string()),
        },
        naming_conventions: NamingConventionsConfig {
            style: Some("snake_case".to_string()),
            prefixes: vec!["keel_".to_string(), "test_".to_string()],
        },
        monorepo: MonorepoConfig {
            enabled: true,
            kind: Some("CargoWorkspace".to_string()),
            packages: vec!["core".to_string(), "cli".to_string()],
        },
        tier3: Tier3Config {
            enabled: true,
            scip_paths: {
                let mut m = std::collections::HashMap::new();
                m.insert("typescript".to_string(), ".scip/index.scip".to_string());
                m
            },
            lsp_commands: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "python".to_string(),
                    vec!["pyright-langserver".to_string(), "--stdio".to_string()],
                );
                m
            },
            prefer_scip: false,
        },
        telemetry_id: Some("a1b2c3d4e5f60718a1b2c3d4e5f60718".to_string()),
    };

    // Serialize to JSON
    let json =
        serde_json::to_string_pretty(&original).expect("KeelConfig should serialize to JSON");

    // Deserialize back
    let roundtripped: KeelConfig =
        serde_json::from_str(&json).expect("KeelConfig JSON should deserialize back");

    // Whole-struct equality (enabled by PartialEq derive)
    assert_eq!(
        original, roundtripped,
        "Round-tripped config must match original"
    );

    // Belt-and-suspenders: also verify each field individually so failures
    // are easy to diagnose if the PartialEq impl ever changes.
    assert_eq!(roundtripped.version, "99.88.77");
    assert_eq!(
        roundtripped.languages,
        vec!["typescript", "python", "go", "rust"]
    );
    assert!(!roundtripped.enforce.type_hints);
    assert!(!roundtripped.enforce.docstrings);
    assert!(!roundtripped.enforce.placement);
    assert_eq!(roundtripped.circuit_breaker.max_failures, 42);
    assert_eq!(roundtripped.batch.timeout_seconds, 999);
    assert_eq!(
        roundtripped.ignore_patterns,
        vec!["vendor/**", "node_modules/**", "*.generated.ts"]
    );
    assert_eq!(roundtripped.tier, Tier::Enterprise);
    assert!(!roundtripped.telemetry.enabled);
    assert!(!roundtripped.telemetry.remote);
    assert_eq!(
        roundtripped.telemetry.endpoint,
        Some("https://custom.example.com/telemetry".to_string())
    );
    assert_eq!(
        roundtripped.naming_conventions.style,
        Some("snake_case".to_string())
    );
    assert_eq!(
        roundtripped.naming_conventions.prefixes,
        vec!["keel_", "test_"]
    );
    assert!(roundtripped.monorepo.enabled);
    assert_eq!(
        roundtripped.monorepo.kind,
        Some("CargoWorkspace".to_string())
    );
    assert_eq!(roundtripped.monorepo.packages, vec!["core", "cli"]);
    assert!(roundtripped.tier3.enabled);
    assert_eq!(
        roundtripped.tier3.scip_paths.get("typescript").unwrap(),
        ".scip/index.scip"
    );
    assert_eq!(
        roundtripped.tier3.lsp_commands.get("python").unwrap(),
        &vec!["pyright-langserver", "--stdio"]
    );
    assert!(!roundtripped.tier3.prefer_scip);
    assert_eq!(
        roundtripped.telemetry_id,
        Some("a1b2c3d4e5f60718a1b2c3d4e5f60718".to_string())
    );
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

#[test]
fn test_tier_roundtrip() {
    for (tier, expected_json) in [
        (Tier::Free, "\"free\""),
        (Tier::Team, "\"team\""),
        (Tier::Enterprise, "\"enterprise\""),
    ] {
        let json = serde_json::to_string(&tier).unwrap();
        assert_eq!(json, expected_json);
        let parsed: Tier = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, tier);
    }
}

#[test]
fn test_telemetry_defaults() {
    let cfg = TelemetryConfig::default();
    assert!(cfg.enabled);
    assert!(cfg.remote);
    assert!(cfg.endpoint.is_none());
    assert_eq!(
        cfg.effective_endpoint(),
        "https://keel.engineer/api/telemetry"
    );
}

#[test]
fn test_backward_compat_old_json_without_new_fields() {
    // Old-style JSON without tier, telemetry, or naming_conventions
    let old_json = r#"{
        "version": "0.1.0",
        "languages": ["typescript"],
        "enforce": { "type_hints": true, "docstrings": true, "placement": true },
        "circuit_breaker": { "max_failures": 3 },
        "batch": { "timeout_seconds": 60 },
        "ignore_patterns": []
    }"#;
    let cfg: KeelConfig = serde_json::from_str(old_json).unwrap();
    assert_eq!(cfg.tier, Tier::Free);
    assert!(cfg.telemetry.enabled);
    assert!(cfg.telemetry.remote);
    assert!(cfg.naming_conventions.style.is_none());
    assert!(cfg.naming_conventions.prefixes.is_empty());
    assert!(!cfg.monorepo.enabled);
    assert!(cfg.monorepo.kind.is_none());
    assert!(cfg.monorepo.packages.is_empty());
    assert!(!cfg.tier3.enabled);
    assert!(cfg.tier3.scip_paths.is_empty());
    assert!(cfg.tier3.lsp_commands.is_empty());
    assert!(cfg.tier3.prefer_scip);
    assert!(cfg.telemetry_id.is_none());
}

#[test]
fn test_naming_conventions_roundtrip() {
    let nc = NamingConventionsConfig {
        style: Some("camelCase".to_string()),
        prefixes: vec!["app_".to_string(), "lib_".to_string()],
    };
    let json = serde_json::to_string(&nc).unwrap();
    let parsed: NamingConventionsConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, nc);
}
