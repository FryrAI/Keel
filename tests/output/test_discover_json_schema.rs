// Tests for discover command JSON output schema (Spec 008 - Output Formats)
use keel_enforce::types::*;
use keel_output::json::JsonFormatter;
use keel_output::OutputFormatter;

fn sample_discover() -> DiscoverResult {
    DiscoverResult {
        version: "0.1.0".into(),
        command: "discover".into(),
        target: NodeInfo {
            hash: "abc12345678".into(),
            name: "handleRequest".into(),
            signature: "fn handleRequest(req: Request) -> Response".into(),
            file: "src/handler.rs".into(),
            line_start: 5,
            line_end: 20,
            docstring: Some("Handles incoming requests".into()),
            type_hints_present: true,
            has_docstring: true,
        },
        upstream: vec![CallerInfo {
            hash: "cal11111111".into(),
            name: "main".into(),
            signature: "fn main()".into(),
            file: "src/main.rs".into(),
            line: 1,
            docstring: None,
            call_line: 8,
            distance: 1,
        }],
        downstream: vec![CalleeInfo {
            hash: "dep11111111".into(),
            name: "processBody".into(),
            signature: "fn processBody(body: &str) -> Result".into(),
            file: "src/body.rs".into(),
            line: 10,
            docstring: None,
            call_line: 15,
            distance: 1,
        }],
        module_context: ModuleContext {
            module: "src/handler.rs".into(),
            sibling_functions: vec!["handleRequest".into(), "handleError".into()],
            responsibility_keywords: vec!["http".into(), "request".into()],
            function_count: 2,
            external_endpoints: vec!["GET /api/data".into()],
        },
        body_context: None,
    }
}

fn isolated_discover() -> DiscoverResult {
    DiscoverResult {
        version: "0.1.0".into(),
        command: "discover".into(),
        target: NodeInfo {
            hash: "iso12345678".into(),
            name: "helperFn".into(),
            signature: "fn helperFn()".into(),
            file: "src/util.rs".into(),
            line_start: 1,
            line_end: 3,
            docstring: None,
            type_hints_present: true,
            has_docstring: false,
        },
        upstream: vec![],
        downstream: vec![],
        module_context: ModuleContext {
            module: "src/util.rs".into(),
            sibling_functions: vec![],
            responsibility_keywords: vec![],
            function_count: 1,
            external_endpoints: vec![],
        },
        body_context: None,
    }
}

#[test]
fn test_discover_json_has_target_node() {
    let fmt = JsonFormatter;
    let out = fmt.format_discover(&sample_discover());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert!(parsed["target"].is_object());
    assert_eq!(parsed["target"]["hash"], "abc12345678");
    assert_eq!(parsed["target"]["name"], "handleRequest");
    assert_eq!(parsed["target"]["file"], "src/handler.rs");
    assert_eq!(parsed["target"]["line_start"], 5);
    assert_eq!(parsed["target"]["line_end"], 20);
    assert_eq!(parsed["target"]["docstring"], "Handles incoming requests");
}

#[test]
fn test_discover_json_callers_array() {
    let fmt = JsonFormatter;
    let out = fmt.format_discover(&sample_discover());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    let upstream = parsed["upstream"].as_array().unwrap();
    assert_eq!(upstream.len(), 1);
    assert_eq!(upstream[0]["hash"], "cal11111111");
    assert_eq!(upstream[0]["name"], "main");
    assert_eq!(upstream[0]["call_line"], 8);
}

#[test]
fn test_discover_json_callees_array() {
    let fmt = JsonFormatter;
    let out = fmt.format_discover(&sample_discover());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    let downstream = parsed["downstream"].as_array().unwrap();
    assert_eq!(downstream.len(), 1);
    assert_eq!(downstream[0]["hash"], "dep11111111");
    assert_eq!(downstream[0]["name"], "processBody");
    assert_eq!(downstream[0]["call_line"], 15);
}

#[test]
fn test_discover_json_edge_metadata() {
    let fmt = JsonFormatter;
    let out = fmt.format_discover(&sample_discover());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    let mc = &parsed["module_context"];
    assert_eq!(mc["function_count"], 2);
    assert_eq!(mc["module"], "src/handler.rs");
    let keywords = mc["responsibility_keywords"].as_array().unwrap();
    assert!(keywords.iter().any(|k| k == "http"));
    let endpoints = mc["external_endpoints"].as_array().unwrap();
    assert!(endpoints
        .iter()
        .any(|e| e.as_str().unwrap().contains("GET")));
}

#[test]
fn test_discover_json_isolated_node() {
    let fmt = JsonFormatter;
    let out = fmt.format_discover(&isolated_discover());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_eq!(parsed["upstream"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["downstream"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["target"]["name"], "helperFn");
}
