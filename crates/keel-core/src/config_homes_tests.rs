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
                {"name": "valid", "patterns": ["first"], "home": "src/time.rs", "scope": "src"},
                {"name": "valid", "patterns": ["second"]}
            ], "unknown": {"keep": [1, 2, 3]}
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
    let path = dir.path().join("keel.json");
    let mut expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    expected["version"] = json!("updated");
    KeelConfig::sync_version(dir.path(), "updated").unwrap();
    let actual: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(KeelConfig::load(dir.path()).homes, cfg.homes);
}

#[test]
fn homes_unknown_severity_keeps_gate_and_other_settings() {
    let cfg: KeelConfig = serde_json::from_value(json!({
        "version": "test", "languages": ["rust"], "enforce": {"homes": "Error", "docstrings": false},
        "review": {"gate": ["E007"]}, "homes": [{"name": "x", "patterns": ["x"]}]
    })).unwrap();
    assert_eq!(cfg.enforce.homes, HomeSeverity::Warning);
    assert_eq!(cfg.review.gate, ["E007"]);
    assert_eq!(cfg.languages, ["rust"]);
    assert!(!cfg.enforce.docstrings);
    assert_eq!(cfg.homes.len(), 1);
}

#[test]
fn homes_glob_spellings_normalize_and_root_home_is_rejected() {
    for spelling in ["src/", "./src", "././src/", "/src", ".//src", "src//./"] {
        let cfg: KeelConfig = serde_json::from_value(json!({
            "version": "test", "languages": ["rust"],
            "homes": [{"name": "x", "patterns": ["x"], "scope": spelling}]
        }))
        .unwrap();
        assert_eq!(cfg.homes[0].scope, ["src"]);
    }
    for spelling in [
        "src/time/",
        "./src/time",
        "././src/time/",
        "/src/time",
        ".//src//time/",
    ] {
        let cfg: KeelConfig = serde_json::from_value(json!({
            "version": "test", "languages": ["rust"],
            "homes": [{"name": "x", "patterns": ["x"], "home": spelling}]
        }))
        .unwrap();
        assert_eq!(cfg.homes[0].home, ["src/time"]);
    }
    for spelling in [".", "./", "/", "././", "//./"] {
        let cfg: KeelConfig = serde_json::from_value(json!({
            "version": "test", "languages": ["rust"],
            "homes": [{"name": "scope", "patterns": ["x"], "scope": spelling},
                {"name": "home", "patterns": ["x"], "home": spelling}]
        }))
        .unwrap();
        assert_eq!(cfg.homes.len(), 1);
        assert!(cfg.homes[0].scope.is_empty());
    }
}

#[test]
fn homes_parent_components_are_rejected_without_losing_other_settings() {
    for key in ["home", "scope"] {
        for spelling in ["../src", "src/../time", "/src/./../time"] {
            let mut bad = json!({"name": "parent", "patterns": ["x"]});
            bad[key] = json!(spelling);
            let cfg: KeelConfig = serde_json::from_value(json!({
                "version": "test", "languages": ["rust"], "review": {"gate": ["E007"]},
                "homes": [bad, {"name": "valid", "patterns": ["x"], "scope": "src"}]
            }))
            .unwrap();
            assert_eq!(cfg.homes.len(), 1);
            assert_eq!(cfg.homes[0].name, "valid");
            assert_eq!(cfg.review.gate, ["E007"]);
        }
    }
}

#[test]
fn homes_sync_version_preserves_trailing_newline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keel.json");
    for newline in ["", "\n"] {
        std::fs::write(&path, format!("{{\"version\":\"old\"}}{newline}")).unwrap();
        KeelConfig::sync_version(dir.path(), "updated").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().ends_with('\n'),
            !newline.is_empty()
        );
    }
}

#[test]
fn homes_sync_version_refuses_invalid_json_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keel.json");
    for text in ["{bad json", "[]", "null"] {
        std::fs::write(&path, text).unwrap();
        assert!(KeelConfig::sync_version(dir.path(), "updated").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    }
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
