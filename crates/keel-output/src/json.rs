use crate::OutputFormatter;
use keel_enforce::types::{CompileResult, DiscoverResult, ExplainResult, MapResult};

pub struct JsonFormatter;

impl OutputFormatter for JsonFormatter {
    fn format_compile(&self, result: &CompileResult) -> String {
        serde_json::to_string_pretty(result).unwrap_or_default()
    }
    fn format_discover(&self, result: &DiscoverResult) -> String {
        serde_json::to_string_pretty(result).unwrap_or_default()
    }
    fn format_explain(&self, result: &ExplainResult) -> String {
        serde_json::to_string_pretty(result).unwrap_or_default()
    }
    fn format_map(&self, result: &MapResult) -> String {
        serde_json::to_string_pretty(result).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_enforce::types::*;

    fn clean_compile() -> CompileResult {
        CompileResult {
            version: "0.1.0".into(),
            command: "compile".into(),
            status: "ok".into(),
            files_analyzed: vec!["src/main.rs".into()],
            errors: vec![],
            warnings: vec![],
            info: CompileInfo {
                nodes_updated: 0,
                edges_updated: 0,
                hashes_changed: vec![],
            },
        }
    }

    fn compile_with_error() -> CompileResult {
        CompileResult {
            version: "0.1.0".into(),
            command: "compile".into(),
            status: "error".into(),
            files_analyzed: vec!["src/lib.rs".into()],
            errors: vec![Violation {
                code: "E001".into(),
                severity: "ERROR".into(),
                category: "broken_caller".into(),
                message: "Signature of `foo` changed; 2 caller(s) need updating".into(),
                file: "src/lib.rs".into(),
                line: 10,
                hash: "abc12345678".into(),
                confidence: 0.92,
                resolution_tier: "tree-sitter".into(),
                fix_hint: Some("Update callers of `foo`".into()),
                suppressed: false,
                suppress_hint: None,
                affected: vec![AffectedNode {
                    hash: "def11111111".into(),
                    name: "bar".into(),
                    file: "src/bar.rs".into(),
                    line: 20,
                }],
                suggested_module: None,
                existing: None,
            }],
            warnings: vec![],
            info: CompileInfo {
                nodes_updated: 1,
                edges_updated: 0,
                hashes_changed: vec!["abc12345678".into()],
            },
        }
    }

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
        }
    }

    fn sample_explain() -> ExplainResult {
        ExplainResult {
            version: "0.1.0".into(),
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
    fn test_json_compile_clean() {
        let fmt = JsonFormatter;
        let out = fmt.format_compile(&clean_compile());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["status"], "ok");
        assert_eq!(parsed["errors"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_json_compile_with_error() {
        let fmt = JsonFormatter;
        let out = fmt.format_compile(&compile_with_error());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["status"], "error");
        assert_eq!(parsed["errors"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["errors"][0]["code"], "E001");
        assert_eq!(parsed["errors"][0]["confidence"], 0.92);
        assert_eq!(parsed["errors"][0]["affected"][0]["name"], "bar");
    }

    #[test]
    fn test_json_compile_roundtrip() {
        let fmt = JsonFormatter;
        let original = compile_with_error();
        let json = fmt.format_compile(&original);
        let deserialized: CompileResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.status, original.status);
        assert_eq!(deserialized.errors.len(), original.errors.len());
        assert_eq!(deserialized.errors[0].code, original.errors[0].code);
    }

    #[test]
    fn test_json_discover() {
        let fmt = JsonFormatter;
        let out = fmt.format_discover(&sample_discover());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["target"]["name"], "handleRequest");
        assert_eq!(parsed["upstream"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["downstream"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["module_context"]["function_count"], 2);
    }

    #[test]
    fn test_json_explain() {
        let fmt = JsonFormatter;
        let out = fmt.format_explain(&sample_explain());
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["error_code"], "E001");
        assert_eq!(parsed["confidence"], 0.92);
        assert_eq!(parsed["resolution_chain"].as_array().unwrap().len(), 2);
    }
}
