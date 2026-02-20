// Tests for keel explain result structure (Spec 006 - Enforcement Engine)
use keel_enforce::types::{ExplainResult, ResolutionStep};

#[test]
fn test_explain_resolution_chain_structure() {
    let result = ExplainResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "explain".to_string(),
        error_code: "E001".to_string(),
        hash: "abc12345678".to_string(),
        confidence: 0.92,
        resolution_tier: "tree-sitter".to_string(),
        resolution_chain: vec![
            ResolutionStep {
                kind: "import".to_string(),
                file: "main.py".to_string(),
                line: 1,
                text: "from lib import foo".to_string(),
            },
            ResolutionStep {
                kind: "call".to_string(),
                file: "main.py".to_string(),
                line: 5,
                text: "foo(42)".to_string(),
            },
        ],
        summary: "Call to foo resolved via import".to_string(),
    };

    assert_eq!(result.error_code, "E001");
    assert_eq!(result.hash, "abc12345678");
    assert_eq!(result.resolution_chain.len(), 2);
    assert_eq!(result.resolution_chain[0].kind, "import");
    assert_eq!(result.resolution_chain[1].kind, "call");
}

#[test]
fn test_explain_includes_confidence() {
    let result = ExplainResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "explain".to_string(),
        error_code: "E001".to_string(),
        hash: "hash123".to_string(),
        confidence: 0.85,
        resolution_tier: "tier_2".to_string(),
        resolution_chain: vec![],
        summary: "Resolved via Tier 2".to_string(),
    };

    assert!(result.confidence > 0.0);
    assert!(result.confidence <= 1.0);
}

#[test]
fn test_explain_shows_resolution_tier() {
    let result = ExplainResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "explain".to_string(),
        error_code: "E001".to_string(),
        hash: "hash123".to_string(),
        confidence: 0.95,
        resolution_tier: "tree-sitter".to_string(),
        resolution_chain: vec![],
        summary: "Tier 1 resolution".to_string(),
    };

    assert_eq!(result.resolution_tier, "tree-sitter");
}

#[test]
fn test_explain_result_serialization() {
    let result = ExplainResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "explain".to_string(),
        error_code: "E001".to_string(),
        hash: "abc12345678".to_string(),
        confidence: 0.92,
        resolution_tier: "tree-sitter".to_string(),
        resolution_chain: vec![ResolutionStep {
            kind: "call".to_string(),
            file: "main.py".to_string(),
            line: 10,
            text: "process()".to_string(),
        }],
        summary: "Direct call".to_string(),
    };

    let json = serde_json::to_string(&result).unwrap();
    let parsed: ExplainResult = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.error_code, "E001");
    assert_eq!(parsed.hash, "abc12345678");
    assert_eq!(parsed.resolution_chain.len(), 1);
}

#[test]
fn test_explain_version_and_command() {
    let result = ExplainResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "explain".to_string(),
        error_code: "E005".to_string(),
        hash: "xyz".to_string(),
        confidence: 0.7,
        resolution_tier: "heuristic".to_string(),
        resolution_chain: vec![],
        summary: String::new(),
    };

    assert_eq!(result.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(result.command, "explain");
}

#[test]
fn test_explain_empty_chain() {
    let result = ExplainResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "explain".to_string(),
        error_code: "E002".to_string(),
        hash: "h".to_string(),
        confidence: 1.0,
        resolution_tier: "tree-sitter".to_string(),
        resolution_chain: vec![],
        summary: "No chain".to_string(),
    };

    assert!(result.resolution_chain.is_empty());
    assert_eq!(result.confidence, 1.0);
}
