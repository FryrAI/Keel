/// Contract tests for JSON schema compliance.
///
/// These tests verify that the serialized output from CompileResult,
/// DiscoverResult, MapResult (when implemented), and ExplainResult
/// matches the JSON schemas defined in tests/schemas/.
///
/// All tests are #[ignore] until a JSON schema validation crate is
/// integrated (e.g. `jsonschema`) and the full output pipeline is
/// implemented.
use keel_enforce::types::{
    CompileInfo, CompileResult, DiscoverResult, ExplainResult, ModuleContext,
    NodeInfo, ResolutionStep,
};

// ---------------------------------------------------------------------------
// Compile output schema
// ---------------------------------------------------------------------------

#[test]
#[ignore = "Not yet implemented: requires jsonschema validation crate"]
fn compile_output_matches_schema() {
    let result = CompileResult {
        version: "0.1.0".to_string(),
        command: "compile".to_string(),
        status: "ok".to_string(),
        files_analyzed: vec!["src/main.rs".to_string()],
        errors: vec![],
        warnings: vec![],
        info: CompileInfo {
            nodes_updated: 0,
            edges_updated: 0,
            hashes_changed: vec![],
        },
    };

    let json_value = serde_json::to_value(&result).unwrap();
    let schema_str = include_str!("../schemas/compile_output.schema.json");
    let _schema: serde_json::Value = serde_json::from_str(schema_str).unwrap();

    // TODO: Validate json_value against schema using jsonschema crate
    // let compiled = jsonschema::JSONSchema::compile(&schema).unwrap();
    // assert!(compiled.validate(&json_value).is_ok());
    let _ = json_value;
}

#[test]
#[ignore = "Not yet implemented: requires jsonschema validation crate"]
fn compile_output_with_errors_matches_schema() {
    // TODO: Create a CompileResult with errors and validate against schema
    // Ensures the violation sub-schema is tested too
}

#[test]
#[ignore = "Not yet implemented: requires jsonschema validation crate"]
fn compile_output_with_warnings_matches_schema() {
    // TODO: Create a CompileResult with W001/W002 and validate against schema
    // Ensures suggested_module and existing fields are tested
}

// ---------------------------------------------------------------------------
// Discover output schema
// ---------------------------------------------------------------------------

#[test]
#[ignore = "Not yet implemented: requires jsonschema validation crate"]
fn discover_output_matches_schema() {
    let result = DiscoverResult {
        version: "0.1.0".to_string(),
        command: "discover".to_string(),
        target: NodeInfo {
            hash: "testhash0001".to_string(),
            name: "my_func".to_string(),
            signature: "fn my_func()".to_string(),
            file: "src/lib.rs".to_string(),
            line_start: 1,
            line_end: 10,
            docstring: None,
            type_hints_present: true,
            has_docstring: false,
        },
        upstream: vec![],
        downstream: vec![],
        module_context: ModuleContext {
            module: "src/lib.rs".to_string(),
            sibling_functions: vec![],
            responsibility_keywords: vec![],
            function_count: 1,
            external_endpoints: vec![],
        },
    };

    let json_value = serde_json::to_value(&result).unwrap();
    let schema_str = include_str!("../schemas/discover_output.schema.json");
    let _schema: serde_json::Value = serde_json::from_str(schema_str).unwrap();

    // TODO: Validate json_value against schema
    let _ = json_value;
}

#[test]
#[ignore = "Not yet implemented: requires jsonschema validation crate"]
fn discover_output_with_upstream_downstream_matches_schema() {
    // TODO: Create DiscoverResult with populated upstream/downstream arrays
    // and validate against schema
}

// ---------------------------------------------------------------------------
// Map output schema
// ---------------------------------------------------------------------------

#[test]
#[ignore = "Not yet implemented: MapResult struct not yet defined"]
fn map_output_matches_schema() {
    // TODO: Once MapResult struct is defined, serialize and validate
    // against tests/schemas/map_output.schema.json
    let schema_str = include_str!("../schemas/map_output.schema.json");
    let _schema: serde_json::Value = serde_json::from_str(schema_str).unwrap();
}

// ---------------------------------------------------------------------------
// Explain output schema
// ---------------------------------------------------------------------------

#[test]
#[ignore = "Not yet implemented: requires jsonschema validation crate"]
fn explain_output_matches_schema() {
    let result = ExplainResult {
        version: "0.1.0".to_string(),
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
    let _schema: serde_json::Value = serde_json::from_str(schema_str).unwrap();

    // TODO: Validate json_value against schema
    let _ = json_value;
}

// ---------------------------------------------------------------------------
// Schema files are valid JSON
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
        ("compile", include_str!("../schemas/compile_output.schema.json")),
        ("discover", include_str!("../schemas/discover_output.schema.json")),
        ("map", include_str!("../schemas/map_output.schema.json")),
        ("explain", include_str!("../schemas/explain_output.schema.json")),
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
