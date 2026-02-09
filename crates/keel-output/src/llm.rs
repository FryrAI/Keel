use crate::OutputFormatter;
use keel_enforce::types::{CompileResult, DiscoverResult, ExplainResult, MapResult, Violation};

pub struct LlmFormatter;

impl OutputFormatter for LlmFormatter {
    fn format_compile(&self, result: &CompileResult) -> String {
        if result.errors.is_empty() && result.warnings.is_empty() {
            return String::new(); // Clean compile = empty stdout
        }

        let mut out = String::new();

        out.push_str(&format!(
            "COMPILE files={} errors={} warnings={}\n",
            result.files_analyzed.len(),
            result.errors.len(),
            result.warnings.len(),
        ));

        for v in &result.errors {
            out.push_str(&format_violation_llm(v));
        }
        for v in &result.warnings {
            out.push_str(&format_violation_llm(v));
        }

        out
    }

    fn format_discover(&self, result: &DiscoverResult) -> String {
        let mut out = String::new();

        out.push_str(&format!(
            "DISCOVER hash={} name={} file={}:{}-{}\n",
            result.target.hash,
            result.target.name,
            result.target.file,
            result.target.line_start,
            result.target.line_end,
        ));

        if !result.upstream.is_empty() {
            out.push_str(&format!("CALLERS count={}\n", result.upstream.len()));
            for c in &result.upstream {
                out.push_str(&format!(
                    "  {}@{}:{} sig={}\n",
                    c.hash, c.file, c.call_line, c.signature
                ));
            }
        }

        if !result.downstream.is_empty() {
            out.push_str(&format!("CALLEES count={}\n", result.downstream.len()));
            for c in &result.downstream {
                out.push_str(&format!(
                    "  {}@{}:{} sig={}\n",
                    c.hash, c.file, c.call_line, c.signature
                ));
            }
        }

        if !result.module_context.module.is_empty() {
            out.push_str(&format!(
                "MODULE {} fns={}\n",
                result.module_context.module,
                result.module_context.function_count,
            ));
        }

        out
    }

    fn format_explain(&self, result: &ExplainResult) -> String {
        let mut out = String::new();

        out.push_str(&format!(
            "EXPLAIN {} hash={} conf={:.2} tier={}\n",
            result.error_code, result.hash, result.confidence, result.resolution_tier,
        ));

        for step in &result.resolution_chain {
            out.push_str(&format!(
                "  {} {}:{} {}\n",
                step.kind, step.file, step.line, step.text,
            ));
        }

        out.push_str(&format!("SUMMARY {}\n", result.summary));
        out
    }

    fn format_map(&self, result: &MapResult) -> String {
        let s = &result.summary;
        let mut out = format!(
            "MAP nodes={} edges={} modules={} fns={} classes={}\n",
            s.total_nodes, s.total_edges, s.modules, s.functions, s.classes,
        );
        out.push_str(&format!(
            "LANGS {} HINTS={:.2} DOCS={:.2}\n",
            s.languages.join(","),
            s.type_hint_coverage,
            s.docstring_coverage,
        ));
        for m in &result.modules {
            out.push_str(&format!(
                "  {} fns={} cls={} edges={}\n",
                m.path, m.function_count, m.class_count, m.edge_count,
            ));
        }
        out
    }
}

fn format_violation_llm(v: &Violation) -> String {
    let mut out = format!(
        "{} {} hash={} conf={:.2} tier={}",
        v.code, v.category, v.hash, v.confidence, v.resolution_tier,
    );

    if !v.affected.is_empty() {
        out.push_str(&format!(" callers={}", v.affected.len()));
    }

    out.push('\n');

    if let Some(fix) = &v.fix_hint {
        out.push_str(&format!("  FIX: {}\n", fix));
    }

    if !v.affected.is_empty() {
        let affected_strs: Vec<String> = v
            .affected
            .iter()
            .map(|a| format!("{}@{}:{}", a.hash, a.file, a.line))
            .collect();
        out.push_str(&format!("  AFFECTED: {}\n", affected_strs.join(" ")));
    }

    out
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
        }
    }

    #[test]
    fn test_llm_clean_compile_is_empty() {
        let fmt = LlmFormatter;
        let out = fmt.format_compile(&clean_compile());
        assert!(out.is_empty(), "Clean compile must produce empty output");
    }

    #[test]
    fn test_llm_compile_with_violations() {
        let fmt = LlmFormatter;
        let out = fmt.format_compile(&compile_with_error());
        assert!(out.contains("COMPILE files=1 errors=1 warnings=0"));
        assert!(out.contains("E001 broken_caller hash=abc12345678"));
        assert!(out.contains("conf=0.92"));
        assert!(out.contains("callers=1"));
        assert!(out.contains("FIX: Update callers of `foo`"));
        assert!(out.contains("AFFECTED: def11111111@src/bar.rs:20"));
    }

    #[test]
    fn test_llm_discover() {
        let fmt = LlmFormatter;
        let result = DiscoverResult {
            version: "0.1.0".into(),
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
            }],
            downstream: vec![],
            module_context: ModuleContext {
                module: "src/h.rs".into(),
                sibling_functions: vec![],
                responsibility_keywords: vec![],
                function_count: 1,
                external_endpoints: vec![],
            },
        };
        let out = fmt.format_discover(&result);
        assert!(out.contains("DISCOVER hash=abc12345678 name=handle"));
        assert!(out.contains("CALLERS count=1"));
        assert!(out.contains("cal11111111@src/main.rs:8"));
        assert!(out.contains("MODULE src/h.rs fns=1"));
    }

    #[test]
    fn test_llm_explain() {
        let fmt = LlmFormatter;
        let result = ExplainResult {
            version: "0.1.0".into(),
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
