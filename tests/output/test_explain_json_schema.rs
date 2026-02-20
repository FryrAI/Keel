// Tests for explain command JSON output schema (Spec 008 - Output Formats)
use keel_enforce::types::*;
use keel_output::json::JsonFormatter;
use keel_output::OutputFormatter;

fn sample_explain() -> ExplainResult {
    ExplainResult {
        version: env!("CARGO_PKG_VERSION").into(),
        command: "explain".into(),
        error_code: "E001".into(),
        hash: "abc12345678".into(),
        confidence: 0.92,
        resolution_tier: "tree-sitter".into(),
        resolution_chain: vec![
            ResolutionStep {
                kind: "call".into(),
                file: "src/main.rs".into(),
                line: 8,
                text: "call edge at src/main.rs:8".into(),
            },
            ResolutionStep {
                kind: "import".into(),
                file: "src/handler.rs".into(),
                line: 1,
                text: "import edge at src/handler.rs:1".into(),
            },
        ],
        summary: "E001 on `handleRequest` in src/handler.rs:5".into(),
    }
}

#[test]
fn test_explain_json_includes_query() {
    let fmt = JsonFormatter;
    let out = fmt.format_explain(&sample_explain());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(parsed["error_code"], "E001");
    assert_eq!(parsed["hash"], "abc12345678");
    assert_eq!(parsed["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(parsed["command"], "explain");
}

#[test]
fn test_explain_json_resolution_chain() {
    let fmt = JsonFormatter;
    let out = fmt.format_explain(&sample_explain());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    let chain = parsed["resolution_chain"].as_array().unwrap();
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0]["kind"], "call");
    assert_eq!(chain[0]["file"], "src/main.rs");
    assert_eq!(chain[0]["line"], 8);
    assert_eq!(chain[1]["kind"], "import");
}

#[test]
fn test_explain_json_confidence_and_tier() {
    let fmt = JsonFormatter;
    let out = fmt.format_explain(&sample_explain());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    let conf = parsed["confidence"].as_f64().unwrap();
    assert!((conf - 0.92).abs() < 0.001);
    assert_eq!(parsed["resolution_tier"], "tree-sitter");
}

#[test]
fn test_explain_json_summary_present() {
    let fmt = JsonFormatter;
    let out = fmt.format_explain(&sample_explain());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    let summary = parsed["summary"].as_str().unwrap();
    assert!(summary.contains("E001"));
    assert!(summary.contains("handleRequest"));
}

#[test]
fn test_explain_json_roundtrip() {
    let fmt = JsonFormatter;
    let original = sample_explain();
    let json = fmt.format_explain(&original);
    let deserialized: ExplainResult = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.error_code, "E001");
    assert_eq!(deserialized.hash, "abc12345678");
    assert_eq!(deserialized.resolution_chain.len(), 2);
    assert_eq!(deserialized.confidence, 0.92);
    assert_eq!(deserialized.summary, original.summary);
}
