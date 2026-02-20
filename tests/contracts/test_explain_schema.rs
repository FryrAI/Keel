/// Contract tests for explain output JSON schema compliance.
use keel_enforce::types::{ExplainResult, ResolutionStep};

use super::test_schema_helpers::validate_against_schema;

#[test]
fn explain_output_matches_schema() {
    let result = ExplainResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "explain".to_string(),
        error_code: "E001".to_string(),
        hash: "testhash0001".to_string(),
        confidence: 0.95,
        resolution_tier: "tier1".to_string(),
        resolution_chain: vec![ResolutionStep {
            kind: "call".to_string(),
            file: "src/api.rs".to_string(),
            line: 10,
            text: "authenticate(token)".to_string(),
        }],
        summary: "Resolved via direct call.".to_string(),
    };

    let json_value = serde_json::to_value(&result).unwrap();
    let schema_str = include_str!("../schemas/explain_output.schema.json");
    validate_against_schema(&json_value, schema_str);
}

#[test]
fn explain_output_with_multi_step_chain_matches_schema() {
    let result = ExplainResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "explain".to_string(),
        error_code: "E001".to_string(),
        hash: "explain_hash3".to_string(),
        confidence: 0.92,
        resolution_tier: "tier2".to_string(),
        resolution_chain: vec![
            ResolutionStep {
                kind: "import".to_string(),
                file: "src/api.rs".to_string(),
                line: 1,
                text: "use crate::auth::authenticate;".to_string(),
            },
            ResolutionStep {
                kind: "call".to_string(),
                file: "src/api.rs".to_string(),
                line: 10,
                text: "authenticate(token)".to_string(),
            },
            ResolutionStep {
                kind: "type_ref".to_string(),
                file: "src/auth.rs".to_string(),
                line: 5,
                text: "fn authenticate(token: &str) -> User".to_string(),
            },
        ],
        summary: "Resolved via import + call + type reference.".to_string(),
    };

    let json_value = serde_json::to_value(&result).unwrap();
    let schema_str = include_str!("../schemas/explain_output.schema.json");
    validate_against_schema(&json_value, schema_str);
}

// ---------------------------------------------------------------------------
// Schema file validation (cross-cutting, lives here as the "general" schema tests)
// ---------------------------------------------------------------------------

#[test]
fn all_schema_files_are_valid_json() {
    let schemas = [
        include_str!("../schemas/compile_output.schema.json"),
        include_str!("../schemas/discover_output.schema.json"),
        include_str!("../schemas/map_output.schema.json"),
        include_str!("../schemas/explain_output.schema.json"),
    ];

    for (i, schema_str) in schemas.iter().enumerate() {
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(schema_str);
        assert!(
            parsed.is_ok(),
            "Schema file {} is not valid JSON: {:?}",
            i,
            parsed.err()
        );
    }
}

#[test]
fn all_schemas_have_required_meta_fields() {
    let schemas = [
        (
            "compile",
            include_str!("../schemas/compile_output.schema.json"),
        ),
        (
            "discover",
            include_str!("../schemas/discover_output.schema.json"),
        ),
        ("map", include_str!("../schemas/map_output.schema.json")),
        (
            "explain",
            include_str!("../schemas/explain_output.schema.json"),
        ),
    ];

    for (name, schema_str) in &schemas {
        let schema: serde_json::Value = serde_json::from_str(schema_str).unwrap();
        assert!(
            schema.get("$schema").is_some(),
            "{} schema missing $schema field",
            name
        );
        assert!(
            schema.get("title").is_some(),
            "{} schema missing title field",
            name
        );
        assert!(
            schema.get("type").is_some(),
            "{} schema missing type field",
            name
        );
        assert!(
            schema.get("required").is_some(),
            "{} schema missing required field",
            name
        );
        assert!(
            schema.get("properties").is_some(),
            "{} schema missing properties field",
            name
        );
    }
}
