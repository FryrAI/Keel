/// Contract tests for discover output JSON schema compliance.
use keel_enforce::types::{
    CalleeInfo, CallerInfo, DiscoverResult, ModuleContext, NodeInfo,
};

use super::test_schema_helpers::validate_against_schema;

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
                distance: 1,
            },
            CallerInfo {
                hash: "caller_hash02".to_string(),
                name: "router".to_string(),
                signature: "fn router(path: &str)".to_string(),
                file: "src/router.rs".to_string(),
                line: 10,
                docstring: Some("Routes requests.".to_string()),
                call_line: 20,
                distance: 1,
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
            distance: 1,
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
