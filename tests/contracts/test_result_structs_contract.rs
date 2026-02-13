/// Contract tests for CompileResult, DiscoverResult, and ExplainResult.
///
/// These tests verify that the result structs can be serialized to JSON
/// and deserialized back without loss, which is critical since the JSON
/// output schemas in tests/schemas/ depend on these shapes.
use keel_enforce::types::{
    AffectedNode, CalleeInfo, CallerInfo, CompileInfo, CompileResult,
    DiscoverResult, ExistingNode, ExplainResult, ModuleContext, NodeInfo,
    ResolutionStep, Violation,
};

// ---------------------------------------------------------------------------
// CompileResult round-trip
// ---------------------------------------------------------------------------

#[test]
fn compile_result_serializes_to_json() {
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

    let json = serde_json::to_string(&result);
    assert!(json.is_ok(), "CompileResult should serialize to JSON");
}

#[test]
fn compile_result_round_trips() {
    let original = CompileResult {
        version: "0.1.0".to_string(),
        command: "compile".to_string(),
        status: "error".to_string(),
        files_analyzed: vec!["src/lib.rs".to_string(), "src/main.rs".to_string()],
        errors: vec![Violation {
            code: "E001".to_string(),
            severity: "ERROR".to_string(),
            category: "broken_caller".to_string(),
            message: "Test error".to_string(),
            file: "src/lib.rs".to_string(),
            line: 42,
            hash: "testhash0001".to_string(),
            confidence: 0.95,
            resolution_tier: "tier1".to_string(),
            fix_hint: Some("Fix this".to_string()),
            suppressed: false,
            suppress_hint: Some("Add suppress comment".to_string()),
            affected: vec![AffectedNode {
                hash: "affectedhash".to_string(),
                name: "affected_fn".to_string(),
                file: "src/other.rs".to_string(),
                line: 10,
            }],
            suggested_module: None,
            existing: None,
        }],
        warnings: vec![Violation {
            code: "W001".to_string(),
            severity: "WARNING".to_string(),
            category: "placement".to_string(),
            message: "Bad placement".to_string(),
            file: "src/utils.rs".to_string(),
            line: 5,
            hash: "warnhash0001".to_string(),
            confidence: 0.70,
            resolution_tier: "tier1".to_string(),
            fix_hint: None,
            suppressed: false,
            suppress_hint: None,
            affected: vec![],
            suggested_module: Some("src/auth.rs".to_string()),
            existing: None,
        }],
        info: CompileInfo {
            nodes_updated: 3,
            edges_updated: 5,
            hashes_changed: vec!["testhash0001".to_string()],
        },
    };

    let json = serde_json::to_string(&original).unwrap();
    let deserialized: CompileResult = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.version, original.version);
    assert_eq!(deserialized.command, original.command);
    assert_eq!(deserialized.status, original.status);
    assert_eq!(deserialized.files_analyzed.len(), original.files_analyzed.len());
    assert_eq!(deserialized.errors.len(), original.errors.len());
    assert_eq!(deserialized.warnings.len(), original.warnings.len());
    assert_eq!(deserialized.errors[0].code, "E001");
    assert_eq!(deserialized.errors[0].confidence, 0.95);
    assert_eq!(deserialized.warnings[0].suggested_module, Some("src/auth.rs".to_string()));
}

#[test]
fn compile_result_violation_with_existing_node() {
    let violation = Violation {
        code: "W002".to_string(),
        severity: "WARNING".to_string(),
        category: "duplicate_name".to_string(),
        message: "Duplicate name".to_string(),
        file: "src/a.rs".to_string(),
        line: 10,
        hash: "dupe_hash001".to_string(),
        confidence: 0.85,
        resolution_tier: "tier1".to_string(),
        fix_hint: None,
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: Some(ExistingNode {
            hash: "existing_hash".to_string(),
            file: "src/b.rs".to_string(),
            line: 20,
        }),
    };

    let json = serde_json::to_string(&violation).unwrap();
    let deser: Violation = serde_json::from_str(&json).unwrap();
    assert!(deser.existing.is_some());
    assert_eq!(deser.existing.unwrap().hash, "existing_hash");
}

// ---------------------------------------------------------------------------
// DiscoverResult round-trip
// ---------------------------------------------------------------------------

#[test]
fn discover_result_serializes_to_json() {
    let result = DiscoverResult {
        version: "0.1.0".to_string(),
        command: "discover".to_string(),
        target: NodeInfo {
            hash: "target_hash1".to_string(),
            name: "my_function".to_string(),
            signature: "fn my_function(x: i32) -> bool".to_string(),
            file: "src/lib.rs".to_string(),
            line_start: 10,
            line_end: 25,
            docstring: Some("Does something important.".to_string()),
            type_hints_present: true,
            has_docstring: true,
        },
        upstream: vec![],
        downstream: vec![],
        module_context: ModuleContext {
            module: "src/lib.rs".to_string(),
            sibling_functions: vec!["other_fn".to_string()],
            responsibility_keywords: vec!["core".to_string()],
            function_count: 5,
            external_endpoints: vec![],
        },
    };

    let json = serde_json::to_string(&result);
    assert!(json.is_ok(), "DiscoverResult should serialize to JSON");
}

#[test]
fn discover_result_round_trips() {
    let original = DiscoverResult {
        version: "0.1.0".to_string(),
        command: "discover".to_string(),
        target: NodeInfo {
            hash: "target_hash2".to_string(),
            name: "handle_request".to_string(),
            signature: "fn handle_request(req: Request) -> Response".to_string(),
            file: "src/api.rs".to_string(),
            line_start: 5,
            line_end: 30,
            docstring: None,
            type_hints_present: true,
            has_docstring: false,
        },
        upstream: vec![CallerInfo {
            hash: "caller_hash01".to_string(),
            name: "main".to_string(),
            signature: "fn main()".to_string(),
            file: "src/main.rs".to_string(),
            line: 1,
            docstring: None,
            call_line: 15,
            distance: 1,
        }],
        downstream: vec![CalleeInfo {
            hash: "callee_hash01".to_string(),
            name: "authenticate".to_string(),
            signature: "fn authenticate(token: &str) -> User".to_string(),
            file: "src/auth.rs".to_string(),
            line: 5,
            docstring: Some("Authenticates user.".to_string()),
            call_line: 10,
            distance: 1,
        }],
        module_context: ModuleContext {
            module: "src/api.rs".to_string(),
            sibling_functions: vec!["parse_body".to_string(), "send_response".to_string()],
            responsibility_keywords: vec!["api".to_string(), "http".to_string()],
            function_count: 4,
            external_endpoints: vec!["GET /api/users".to_string()],
        },
    };

    let json = serde_json::to_string(&original).unwrap();
    let deserialized: DiscoverResult = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.target.hash, "target_hash2");
    assert_eq!(deserialized.upstream.len(), 1);
    assert_eq!(deserialized.downstream.len(), 1);
    assert_eq!(deserialized.upstream[0].call_line, 15);
    assert_eq!(deserialized.downstream[0].name, "authenticate");
    assert_eq!(deserialized.module_context.function_count, 4);
}

// ---------------------------------------------------------------------------
// ExplainResult round-trip
// ---------------------------------------------------------------------------

#[test]
fn explain_result_serializes_to_json() {
    let result = ExplainResult {
        version: "0.1.0".to_string(),
        command: "explain".to_string(),
        error_code: "E001".to_string(),
        hash: "explain_hash1".to_string(),
        confidence: 0.95,
        resolution_tier: "tier1".to_string(),
        resolution_chain: vec![],
        summary: "Function signature changed, breaking callers.".to_string(),
    };

    let json = serde_json::to_string(&result);
    assert!(json.is_ok(), "ExplainResult should serialize to JSON");
}

#[test]
fn explain_result_round_trips() {
    let original = ExplainResult {
        version: "0.1.0".to_string(),
        command: "explain".to_string(),
        error_code: "E001".to_string(),
        hash: "explain_hash2".to_string(),
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
        ],
        summary: "The call to `authenticate` at src/api.rs:10 was resolved via import at line 1, then matched to the definition in src/auth.rs.".to_string(),
    };

    let json = serde_json::to_string(&original).unwrap();
    let deserialized: ExplainResult = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.error_code, "E001");
    assert_eq!(deserialized.hash, "explain_hash2");
    assert_eq!(deserialized.confidence, 0.92);
    assert_eq!(deserialized.resolution_chain.len(), 2);
    assert_eq!(deserialized.resolution_chain[0].kind, "import");
    assert_eq!(deserialized.resolution_chain[1].kind, "call");
}

// ---------------------------------------------------------------------------
// JSON structure tests
// ---------------------------------------------------------------------------

#[test]
fn compile_result_json_has_required_fields() {
    let result = CompileResult {
        version: "0.1.0".to_string(),
        command: "compile".to_string(),
        status: "ok".to_string(),
        files_analyzed: vec![],
        errors: vec![],
        warnings: vec![],
        info: CompileInfo {
            nodes_updated: 0,
            edges_updated: 0,
            hashes_changed: vec![],
        },
    };

    let json: serde_json::Value = serde_json::to_value(&result).unwrap();
    assert!(json.get("version").is_some());
    assert!(json.get("command").is_some());
    assert!(json.get("status").is_some());
    assert!(json.get("files_analyzed").is_some());
    assert!(json.get("errors").is_some());
    assert!(json.get("warnings").is_some());
    assert!(json.get("info").is_some());
}

#[test]
fn discover_result_json_has_required_fields() {
    let result = DiscoverResult {
        version: "0.1.0".to_string(),
        command: "discover".to_string(),
        target: NodeInfo {
            hash: "test".to_string(),
            name: "test".to_string(),
            signature: "fn test()".to_string(),
            file: "test.rs".to_string(),
            line_start: 1,
            line_end: 5,
            docstring: None,
            type_hints_present: false,
            has_docstring: false,
        },
        upstream: vec![],
        downstream: vec![],
        module_context: ModuleContext {
            module: "test.rs".to_string(),
            sibling_functions: vec![],
            responsibility_keywords: vec![],
            function_count: 0,
            external_endpoints: vec![],
        },
    };

    let json: serde_json::Value = serde_json::to_value(&result).unwrap();
    assert!(json.get("version").is_some());
    assert!(json.get("command").is_some());
    assert!(json.get("target").is_some());
    assert!(json.get("upstream").is_some());
    assert!(json.get("downstream").is_some());
    assert!(json.get("module_context").is_some());
}

#[test]
fn explain_result_json_has_required_fields() {
    let result = ExplainResult {
        version: "0.1.0".to_string(),
        command: "explain".to_string(),
        error_code: "E001".to_string(),
        hash: "test".to_string(),
        confidence: 0.5,
        resolution_tier: "tier1".to_string(),
        resolution_chain: vec![],
        summary: "Test".to_string(),
    };

    let json: serde_json::Value = serde_json::to_value(&result).unwrap();
    assert!(json.get("version").is_some());
    assert!(json.get("command").is_some());
    assert!(json.get("error_code").is_some());
    assert!(json.get("hash").is_some());
    assert!(json.get("confidence").is_some());
    assert!(json.get("resolution_tier").is_some());
    assert!(json.get("resolution_chain").is_some());
    assert!(json.get("summary").is_some());
}
