/// Contract tests for map output JSON schema compliance.
use keel_enforce::types::{MapResult, MapSummary, ModuleEntry};

use super::test_schema_helpers::validate_against_schema;

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
        hotspots: vec![],
        depth: 1,
        functions: vec![],
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
        hotspots: vec![],
        depth: 1,
        functions: vec![],
    };

    let json_value = serde_json::to_value(&result).unwrap();
    let schema_str = include_str!("../schemas/map_output.schema.json");
    validate_against_schema(&json_value, schema_str);
}
