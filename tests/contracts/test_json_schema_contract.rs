/// Contract tests for JSON schema compliance.
///
/// These tests verify that the serialized output from CompileResult,
/// DiscoverResult, MapResult (when implemented), and ExplainResult
/// matches the JSON schemas defined in tests/schemas/.
use keel_enforce::types::{
    AffectedNode, CalleeInfo, CallerInfo, CompileInfo, CompileResult, DiscoverResult,
    ExistingNode, ExplainResult, MapResult, MapSummary, ModuleContext, ModuleEntry, NodeInfo,
    ResolutionStep, Violation,
};

fn validate_against_schema(json_value: &serde_json::Value, schema_str: &str) {
    let schema: serde_json::Value = serde_json::from_str(schema_str).unwrap();
    let validator = jsonschema::validator_for(&schema)
        .expect("Failed to compile JSON schema");
    let errors: Vec<_> = validator.iter_errors(json_value).collect();
    if !errors.is_empty() {
        let msgs: Vec<String> = errors
            .iter()
            .map(|e| format!("  - {} (at {})", e, e.instance_path))
            .collect();
        panic!(
            "JSON schema validation failed:\n{}",
            msgs.join("\n")
        );
    }
}

// ---------------------------------------------------------------------------
// Compile output schema
// ---------------------------------------------------------------------------

#[test]
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
    validate_against_schema(&json_value, schema_str);
}

#[test]
fn compile_output_with_errors_matches_schema() {
    let result = CompileResult {
        version: "0.1.0".to_string(),
        command: "compile".to_string(),
        status: "error".to_string(),
        files_analyzed: vec!["src/api.rs".to_string(), "src/auth.rs".to_string()],
        errors: vec![
            Violation {
                code: "E001".to_string(),
                severity: "ERROR".to_string(),
                category: "broken_caller".to_string(),
                message: "Function `authenticate` signature changed".to_string(),
                file: "src/api.rs".to_string(),
                line: 10,
                hash: "fn_api_00009".to_string(),
                confidence: 0.95,
                resolution_tier: "tier1".to_string(),
                fix_hint: Some("Update call at src/api.rs:10".to_string()),
                suppressed: false,
                suppress_hint: Some("Add suppress comment".to_string()),
                affected: vec![AffectedNode {
                    hash: "fn_auth_00001".to_string(),
                    name: "authenticate".to_string(),
                    file: "src/auth.rs".to_string(),
                    line: 5,
                }],
                suggested_module: None,
                existing: None,
            },
            Violation {
                code: "E005".to_string(),
                severity: "ERROR".to_string(),
                category: "arity_mismatch".to_string(),
                message: "Function expects 3 args, caller passes 2".to_string(),
                file: "src/api.rs".to_string(),
                line: 25,
                hash: "fn_api_00010".to_string(),
                confidence: 1.0,
                resolution_tier: "tier1".to_string(),
                fix_hint: Some("Add missing argument".to_string()),
                suppressed: false,
                suppress_hint: None,
                affected: vec![],
                suggested_module: None,
                existing: None,
            },
        ],
        warnings: vec![],
        info: CompileInfo {
            nodes_updated: 2,
            edges_updated: 1,
            hashes_changed: vec!["fn_auth_00001".to_string()],
        },
    };

    let json_value = serde_json::to_value(&result).unwrap();
    let schema_str = include_str!("../schemas/compile_output.schema.json");
    validate_against_schema(&json_value, schema_str);
}

#[test]
fn compile_output_with_warnings_matches_schema() {
    let result = CompileResult {
        version: "0.1.0".to_string(),
        command: "compile".to_string(),
        status: "warning".to_string(),
        files_analyzed: vec!["src/utils.rs".to_string(), "src/db.rs".to_string()],
        errors: vec![],
        warnings: vec![
            Violation {
                code: "W001".to_string(),
                severity: "WARNING".to_string(),
                category: "placement".to_string(),
                message: "Function may belong in another module".to_string(),
                file: "src/utils.rs".to_string(),
                line: 5,
                hash: "fn_util_00017".to_string(),
                confidence: 0.72,
                resolution_tier: "tier1".to_string(),
                fix_hint: None,
                suppressed: false,
                suppress_hint: Some("Add suppress comment".to_string()),
                affected: vec![],
                suggested_module: Some("src/auth.rs".to_string()),
                existing: None,
            },
            Violation {
                code: "W002".to_string(),
                severity: "WARNING".to_string(),
                category: "duplicate_name".to_string(),
                message: "Duplicate function name across modules".to_string(),
                file: "src/db.rs".to_string(),
                line: 17,
                hash: "fn_db_000014".to_string(),
                confidence: 0.85,
                resolution_tier: "tier1".to_string(),
                fix_hint: None,
                suppressed: false,
                suppress_hint: None,
                affected: vec![],
                suggested_module: None,
                existing: Some(ExistingNode {
                    hash: "fn_cache_0042".to_string(),
                    file: "src/cache.rs".to_string(),
                    line: 30,
                }),
            },
        ],
        info: CompileInfo {
            nodes_updated: 0,
            edges_updated: 0,
            hashes_changed: vec![],
        },
    };

    let json_value = serde_json::to_value(&result).unwrap();
    let schema_str = include_str!("../schemas/compile_output.schema.json");
    validate_against_schema(&json_value, schema_str);
}

// ---------------------------------------------------------------------------
// Discover output schema
// ---------------------------------------------------------------------------

#[test]
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
    validate_against_schema(&json_value, schema_str);
}

#[test]
fn discover_output_with_upstream_downstream_matches_schema() {
    let result = DiscoverResult {
        version: "0.1.0".to_string(),
        command: "discover".to_string(),
        target: NodeInfo {
            hash: "target_hash02".to_string(),
            name: "handle_request".to_string(),
            signature: "fn handle_request(req: Request) -> Response".to_string(),
            file: "src/api.rs".to_string(),
            line_start: 5,
            line_end: 30,
            docstring: Some("Handles incoming HTTP requests.".to_string()),
            type_hints_present: true,
            has_docstring: true,
        },
        upstream: vec![
            CallerInfo {
                hash: "caller_hash01".to_string(),
                name: "main".to_string(),
                signature: "fn main()".to_string(),
                file: "src/main.rs".to_string(),
                line: 1,
                docstring: None,
                call_line: 15,
            },
            CallerInfo {
                hash: "caller_hash02".to_string(),
                name: "router".to_string(),
                signature: "fn router(path: &str)".to_string(),
                file: "src/router.rs".to_string(),
                line: 10,
                docstring: Some("Routes requests.".to_string()),
                call_line: 20,
            },
        ],
        downstream: vec![CalleeInfo {
            hash: "callee_hash01".to_string(),
            name: "authenticate".to_string(),
            signature: "fn authenticate(token: &str) -> User".to_string(),
            file: "src/auth.rs".to_string(),
            line: 5,
            docstring: Some("Authenticates user.".to_string()),
            call_line: 10,
        }],
        module_context: ModuleContext {
            module: "src/api.rs".to_string(),
            sibling_functions: vec![
                "parse_body".to_string(),
                "send_response".to_string(),
            ],
            responsibility_keywords: vec!["api".to_string(), "http".to_string()],
            function_count: 4,
            external_endpoints: vec!["GET /api/users".to_string()],
        },
    };

    let json_value = serde_json::to_value(&result).unwrap();
    let schema_str = include_str!("../schemas/discover_output.schema.json");
    validate_against_schema(&json_value, schema_str);
}

// ---------------------------------------------------------------------------
// Map output schema
// ---------------------------------------------------------------------------

#[test]
fn map_output_matches_schema() {
    let result = MapResult {
        version: "0.1.0".to_string(),
        command: "map".to_string(),
        summary: MapSummary {
            total_nodes: 10,
            total_edges: 15,
            modules: 3,
            functions: 7,
            classes: 2,
            external_endpoints: 1,
            languages: vec!["typescript".to_string(), "python".to_string()],
            type_hint_coverage: 0.85,
            docstring_coverage: 0.60,
        },
        modules: vec![
            ModuleEntry {
                path: "src/api.ts".to_string(),
                function_count: 4,
                class_count: 1,
                edge_count: 8,
                responsibility_keywords: Some(vec!["api".to_string(), "http".to_string()]),
                external_endpoints: Some(vec!["GET /api/users".to_string()]),
            },
            ModuleEntry {
                path: "src/auth.py".to_string(),
                function_count: 3,
                class_count: 1,
                edge_count: 7,
                responsibility_keywords: None,
                external_endpoints: None,
            },
        ],
    };

    let json_value = serde_json::to_value(&result).unwrap();
    let schema_str = include_str!("../schemas/map_output.schema.json");
    validate_against_schema(&json_value, schema_str);
}

#[test]
fn map_output_empty_modules_matches_schema() {
    let result = MapResult {
        version: "0.1.0".to_string(),
        command: "map".to_string(),
        summary: MapSummary {
            total_nodes: 0,
            total_edges: 0,
            modules: 0,
            functions: 0,
            classes: 0,
            external_endpoints: 0,
            languages: vec![],
            type_hint_coverage: 0.0,
            docstring_coverage: 0.0,
        },
        modules: vec![],
    };

    let json_value = serde_json::to_value(&result).unwrap();
    let schema_str = include_str!("../schemas/map_output.schema.json");
    validate_against_schema(&json_value, schema_str);
}

// ---------------------------------------------------------------------------
// Explain output schema
// ---------------------------------------------------------------------------

#[test]
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
    validate_against_schema(&json_value, schema_str);
}

#[test]
fn explain_output_with_multi_step_chain_matches_schema() {
    let result = ExplainResult {
        version: "0.1.0".to_string(),
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
