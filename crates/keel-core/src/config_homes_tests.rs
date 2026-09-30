use crate::config::{HomeSeverity, KeelConfig};
use serde_json::json;

#[test]
fn homes_string_or_array_and_roundtrip() {
    let cfg: KeelConfig = serde_json::from_value(json!({
        "version": "test", "languages": ["rust"], "enforce": {"homes": "error"},
        "homes": [
            {"name": "a", "patterns": ["date_naive()"], "home": "src/time.rs", "scope": ["src"]},
            {"name": "b", "patterns": ["CURRENT_DATE"], "home": [], "scope": "crates/*/src"}
        ]
    }))
    .unwrap();
    assert_eq!(cfg.homes.len(), 2);
    assert_eq!(cfg.homes[0].home, ["src/time.rs"]);
    assert_eq!(cfg.homes[1].scope, ["crates/*/src"]);
    assert_eq!(cfg.enforce.homes, HomeSeverity::Error);
    assert_eq!(
        cfg,
        serde_json::from_str::<KeelConfig>(&serde_json::to_string(&cfg).unwrap()).unwrap()
    );
}

#[test]
fn homes_malformed_and_duplicate_rules_preserve_other_settings() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("keel.json"),
        json!({
            "version": "test", "languages": ["rust"], "enforce": {"docstrings": false},
            "review": {"gate": ["E007"]},
            "homes": [
                42, {"name": "", "patterns": ["x"]}, {"name": "no-pattern"},
                {"name": "empty-pattern", "patterns": [""]},
                {"name": "bad-glob", "patterns": ["x"], "home": "["},
                {"name": "bad-type", "patterns": ["x"], "scope": 42},
                {"name": "valid", "patterns": ["first"]},
                {"name": "valid", "patterns": ["second"]}
            ]
        })
        .to_string(),
    )
    .unwrap();
    let cfg = KeelConfig::load(dir.path());
    assert_eq!(cfg.version, "test");
    assert!(!cfg.enforce.docstrings);
    assert_eq!(cfg.review.gate, ["E007"]);
    assert_eq!(cfg.homes.len(), 1);
    assert_eq!(cfg.homes[0].patterns, ["first"]);
    KeelConfig::sync_version(dir.path(), "updated").unwrap();
    assert_eq!(KeelConfig::load(dir.path()).homes, cfg.homes);
}

#[test]
fn homes_wrong_container_preserves_config() {
    let cfg: KeelConfig = serde_json::from_value(json!({
        "version": "test", "languages": [], "homes": "oops", "enforce": {"placement": false}
    }))
    .unwrap();
    assert!(cfg.homes.is_empty());
    assert!(!cfg.enforce.placement);
}
