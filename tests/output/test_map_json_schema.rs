// Tests for map command JSON output schema (Spec 008 - Output Formats)
use keel_enforce::types::*;
use keel_output::json::JsonFormatter;
use keel_output::OutputFormatter;

fn sample_map() -> MapResult {
    MapResult {
        version: "0.1.0".into(),
        command: "map".into(),
        summary: MapSummary {
            total_nodes: 42,
            total_edges: 65,
            modules: 5,
            functions: 30,
            classes: 7,
            external_endpoints: 3,
            languages: vec!["python".into(), "typescript".into()],
            type_hint_coverage: 0.85,
            docstring_coverage: 0.60,
        },
        modules: vec![
            ModuleEntry {
                path: "src/main.py".into(),
                function_count: 10,
                class_count: 2,
                edge_count: 15,
                responsibility_keywords: Some(vec!["entry".into(), "main".into()]),
                external_endpoints: Some(vec!["POST /api/run".into()]),
            },
            ModuleEntry {
                path: "src/utils.py".into(),
                function_count: 8,
                class_count: 1,
                edge_count: 12,
                responsibility_keywords: None,
                external_endpoints: None,
            },
        ],
        hotspots: vec![],
        depth: 1,
        functions: vec![],
    }
}

#[test]
fn test_map_json_summary() {
    let fmt = JsonFormatter;
    let out = fmt.format_map(&sample_map());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    let summary = &parsed["summary"];
    assert_eq!(summary["total_nodes"], 42);
    assert_eq!(summary["total_edges"], 65);
    assert_eq!(summary["modules"], 5);
    assert_eq!(summary["functions"], 30);
    assert_eq!(summary["classes"], 7);
}

#[test]
fn test_map_json_language_breakdown() {
    let fmt = JsonFormatter;
    let out = fmt.format_map(&sample_map());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    let langs = parsed["summary"]["languages"].as_array().unwrap();
    assert_eq!(langs.len(), 2);
    assert!(langs.iter().any(|l| l == "python"));
    assert!(langs.iter().any(|l| l == "typescript"));
}

#[test]
fn test_map_json_coverage_metrics() {
    let fmt = JsonFormatter;
    let out = fmt.format_map(&sample_map());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    let hint_cov = parsed["summary"]["type_hint_coverage"].as_f64().unwrap();
    assert!((hint_cov - 0.85).abs() < 0.001);
    let doc_cov = parsed["summary"]["docstring_coverage"].as_f64().unwrap();
    assert!((doc_cov - 0.60).abs() < 0.001);
}

#[test]
fn test_map_json_modules_array() {
    let fmt = JsonFormatter;
    let out = fmt.format_map(&sample_map());
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    let modules = parsed["modules"].as_array().unwrap();
    assert_eq!(modules.len(), 2);
    assert_eq!(modules[0]["path"], "src/main.py");
    assert_eq!(modules[0]["function_count"], 10);
    assert_eq!(modules[0]["class_count"], 2);
    assert_eq!(modules[0]["edge_count"], 15);
}

#[test]
fn test_map_json_roundtrip() {
    let fmt = JsonFormatter;
    let original = sample_map();
    let json = fmt.format_map(&original);
    let deserialized: MapResult = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.summary.total_nodes, 42);
    assert_eq!(deserialized.summary.total_edges, 65);
    assert_eq!(deserialized.modules.len(), 2);
    assert_eq!(deserialized.modules[0].path, "src/main.py");
    assert_eq!(deserialized.version, "0.1.0");
    assert_eq!(deserialized.command, "map");
}
