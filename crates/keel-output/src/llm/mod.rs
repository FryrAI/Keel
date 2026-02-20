pub mod analyze;
pub mod check;
pub mod compile;
pub mod discover;
pub mod explain;
pub mod fix;
pub mod map;
pub mod name;
pub mod violation;

use crate::OutputFormatter;
use keel_enforce::types::{
    AnalyzeResult, CheckResult, CompileDelta, CompileResult, DiscoverResult, ExplainResult,
    FixResult, MapResult, NameResult,
};

pub struct LlmFormatter {
    /// Depth for map output (0-3). Default: 1.
    pub map_depth: u32,
    /// Depth for compile output (0-2). Default: 1.
    pub compile_depth: u32,
    /// Max token budget for output truncation. Default: 500.
    pub max_tokens: usize,
}

impl LlmFormatter {
    pub fn new() -> Self {
        Self {
            map_depth: 1,
            compile_depth: 1,
            max_tokens: 500,
        }
    }

    pub fn with_depths(map_depth: u32, compile_depth: u32) -> Self {
        Self {
            map_depth,
            compile_depth,
            max_tokens: 500,
        }
    }

    pub fn with_max_tokens(mut self, max_tokens: Option<usize>) -> Self {
        if let Some(t) = max_tokens {
            self.max_tokens = t;
        }
        self
    }
}

impl Default for LlmFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFormatter for LlmFormatter {
    fn format_compile(&self, result: &CompileResult) -> String {
        compile::format_compile(result, self.compile_depth, self.max_tokens)
    }

    fn format_discover(&self, result: &DiscoverResult) -> String {
        discover::format_discover(result)
    }

    fn format_explain(&self, result: &ExplainResult) -> String {
        explain::format_explain(result)
    }

    fn format_map(&self, result: &MapResult) -> String {
        map::format_map(result, self.map_depth, self.max_tokens)
    }

    fn format_fix(&self, result: &FixResult) -> String {
        fix::format_fix(result)
    }

    fn format_name(&self, result: &NameResult) -> String {
        name::format_name(result)
    }

    fn format_check(&self, result: &CheckResult) -> String {
        check::format_check(result)
    }

    fn format_compile_delta(&self, delta: &CompileDelta) -> String {
        compile::format_compile_delta(delta)
    }

    fn format_analyze(&self, result: &AnalyzeResult) -> String {
        analyze::format_analyze(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_enforce::types::*;

    #[test]
    fn test_llm_clean_compile_is_empty() {
        let fmt = LlmFormatter::new();
        let result = CompileResult {
            version: env!("CARGO_PKG_VERSION").into(),
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
        };
        assert!(fmt.format_compile(&result).is_empty());
    }

    #[test]
    fn test_llm_compile_with_violations() {
        let fmt = LlmFormatter::with_depths(1, 2); // depth 2 for full detail
        let result = CompileResult {
            version: env!("CARGO_PKG_VERSION").into(),
            command: "compile".into(),
            status: "error".into(),
            files_analyzed: vec!["src/lib.rs".into()],
            errors: vec![Violation {
                code: "E001".into(),
                severity: "ERROR".into(),
                category: "broken_caller".into(),
                message: "Signature of `foo` changed".into(),
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
        };
        let out = fmt.format_compile(&result);
        assert!(out.contains("COMPILE files=1 errors=1 warnings=0"));
        assert!(out.contains("E001 broken_caller hash=abc12345678"));
        assert!(out.contains("conf=0.92"));
        assert!(out.contains("callers=1"));
        assert!(out.contains("FIX: Update callers of `foo`"));
        assert!(out.contains("AFFECTED: def11111111@src/bar.rs:20"));
    }

    #[test]
    fn test_llm_discover() {
        let fmt = LlmFormatter::new();
        let result = DiscoverResult {
            version: env!("CARGO_PKG_VERSION").into(),
            command: "discover".into(),
            target: NodeInfo {
                hash: "abc12345678".into(),
                name: "handle".into(),
                signature: "fn handle(r: Req) -> Res".into(),
                file: "src/h.rs".into(),
                line_start: 5,
                line_end: 20,
                docstring: None,
                type_hints_present: true,
                has_docstring: false,
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
            downstream: vec![],
            module_context: ModuleContext {
                module: "src/h.rs".into(),
                sibling_functions: vec![],
                responsibility_keywords: vec![],
                function_count: 1,
                external_endpoints: vec![],
            },
            body_context: None,
        };
        let out = fmt.format_discover(&result);
        assert!(out.contains("DISCOVER hash=abc12345678 name=handle"));
        assert!(out.contains("CALLERS count=1"));
        assert!(out.contains("d=1 cal11111111@src/main.rs:8"));
        assert!(out.contains("MODULE src/h.rs fns=1"));
    }

    #[test]
    fn test_llm_explain() {
        let fmt = LlmFormatter::new();
        let result = ExplainResult {
            version: env!("CARGO_PKG_VERSION").into(),
            command: "explain".into(),
            error_code: "E001".into(),
            hash: "abc12345678".into(),
            confidence: 0.92,
            resolution_tier: "tree-sitter".into(),
            resolution_chain: vec![ResolutionStep {
                kind: "call".into(),
                file: "src/main.rs".into(),
                line: 8,
                text: "call edge at src/main.rs:8".into(),
            }],
            summary: "E001 on `handle` in src/h.rs:5".into(),
        };
        let out = fmt.format_explain(&result);
        assert!(out.contains("EXPLAIN E001 hash=abc12345678 conf=0.92 tier=tree-sitter"));
        assert!(out.contains("call src/main.rs:8"));
        assert!(out.contains("SUMMARY E001 on `handle`"));
    }
}
