use crate::OutputFormatter;
use keel_enforce::types::{CompileResult, DiscoverResult, ExplainResult, MapResult, Violation};

pub struct HumanFormatter;

impl OutputFormatter for HumanFormatter {
    fn format_compile(&self, result: &CompileResult) -> String {
        if result.errors.is_empty() && result.warnings.is_empty() {
            return String::new(); // Clean compile = empty stdout
        }

        let mut out = String::new();

        for v in &result.errors {
            out.push_str(&format_violation_human(v));
        }
        for v in &result.warnings {
            out.push_str(&format_violation_human(v));
        }

        // Summary line
        if !result.errors.is_empty() || !result.warnings.is_empty() {
            out.push_str(&format!(
                "\n{} error(s), {} warning(s) in {} file(s)\n",
                result.errors.len(),
                result.warnings.len(),
                result.files_analyzed.len(),
            ));
        }

        out
    }

    fn format_discover(&self, result: &DiscoverResult) -> String {
        let mut out = String::new();
        let t = &result.target;

        out.push_str(&format!(
            "{} [{}]\n  --> {}:{}-{}\n  sig: {}\n",
            t.name, t.hash, t.file, t.line_start, t.line_end, t.signature,
        ));

        if let Some(doc) = &t.docstring {
            out.push_str(&format!("  doc: {}\n", doc));
        }

        if !result.upstream.is_empty() {
            out.push_str(&format!("\nCallers ({}):\n", result.upstream.len()));
            for c in &result.upstream {
                out.push_str(&format!(
                    "  {} [{}] at {}:{}\n",
                    c.name, c.hash, c.file, c.call_line,
                ));
            }
        }

        if !result.downstream.is_empty() {
            out.push_str(&format!("\nCallees ({}):\n", result.downstream.len()));
            for c in &result.downstream {
                out.push_str(&format!(
                    "  {} [{}] at {}:{}\n",
                    c.name, c.hash, c.file, c.call_line,
                ));
            }
        }

        if !result.module_context.module.is_empty() {
            let mc = &result.module_context;
            out.push_str(&format!(
                "\nModule: {} ({} functions)\n",
                mc.module, mc.function_count,
            ));
            if !mc.responsibility_keywords.is_empty() {
                out.push_str(&format!(
                    "  keywords: {}\n",
                    mc.responsibility_keywords.join(", ")
                ));
            }
        }

        out
    }

    fn format_explain(&self, result: &ExplainResult) -> String {
        let mut out = String::new();

        out.push_str(&format!(
            "Explanation for {} on hash {}\n",
            result.error_code, result.hash,
        ));
        out.push_str(&format!(
            "  confidence: {:.0}%  tier: {}\n\n",
            result.confidence * 100.0,
            result.resolution_tier,
        ));

        out.push_str("Resolution chain:\n");
        for (i, step) in result.resolution_chain.iter().enumerate() {
            out.push_str(&format!(
                "  {}. [{}] {}:{} — {}\n",
                i + 1,
                step.kind,
                step.file,
                step.line,
                step.text,
            ));
        }

        out.push_str(&format!("\n{}\n", result.summary));
        out
    }

    fn format_map(&self, result: &MapResult) -> String {
        let s = &result.summary;
        let mut out = format!(
            "Map: {} nodes, {} edges, {} modules, {} functions, {} classes\n",
            s.total_nodes, s.total_edges, s.modules, s.functions, s.classes,
        );
        out.push_str(&format!(
            "Languages: {}  Type hints: {:.0}%  Docstrings: {:.0}%\n",
            s.languages.join(", "),
            s.type_hint_coverage * 100.0,
            s.docstring_coverage * 100.0,
        ));
        for m in &result.modules {
            out.push_str(&format!(
                "  {} ({} fns, {} classes, {} edges)\n",
                m.path, m.function_count, m.class_count, m.edge_count,
            ));
        }
        out
    }
}

fn format_violation_human(v: &Violation) -> String {
    let severity_label = match v.severity.as_str() {
        "ERROR" => "error",
        "WARNING" => "warning",
        "INFO" => "info",
        _ => "note",
    };

    let mut out = format!(
        "{}[{}]: {}\n  --> {}:{}\n",
        severity_label, v.code, v.message, v.file, v.line,
    );

    if let Some(fix) = &v.fix_hint {
        out.push_str(&format!("   = fix: {}\n", fix));
    }

    if !v.affected.is_empty() {
        let caller_list: Vec<String> = v
            .affected
            .iter()
            .map(|a| format!("{}:{}", a.file, a.line))
            .collect();
        out.push_str(&format!("   = callers: {}\n", caller_list.join(", ")));
    }

    if let Some(module) = &v.suggested_module {
        out.push_str(&format!("   = suggested module: {}\n", module));
    }

    if let Some(existing) = &v.existing {
        out.push_str(&format!(
            "   = also at: {}:{}\n",
            existing.file, existing.line
        ));
    }

    if v.suppressed {
        if let Some(hint) = &v.suppress_hint {
            out.push_str(&format!("   = {}\n", hint));
        }
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

    #[test]
    fn test_human_clean_compile_is_empty() {
        let fmt = HumanFormatter;
        let out = fmt.format_compile(&clean_compile());
        assert!(out.is_empty(), "Clean compile must produce empty output");
    }

    #[test]
    fn test_human_compile_error_format() {
        let fmt = HumanFormatter;
        let result = CompileResult {
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
                hashes_changed: vec![],
            },
        };
        let out = fmt.format_compile(&result);
        assert!(out.contains("error[E001]: Signature of `foo` changed"));
        assert!(out.contains("--> src/lib.rs:10"));
        assert!(out.contains("= fix: Update callers of `foo`"));
        assert!(out.contains("= callers: src/bar.rs:20"));
        assert!(out.contains("1 error(s), 0 warning(s) in 1 file(s)"));
    }

    #[test]
    fn test_human_warning_with_suggested_module() {
        let fmt = HumanFormatter;
        let result = CompileResult {
            version: "0.1.0".into(),
            command: "compile".into(),
            status: "warning".into(),
            files_analyzed: vec!["src/utils.rs".into()],
            errors: vec![],
            warnings: vec![Violation {
                code: "W001".into(),
                severity: "WARNING".into(),
                category: "placement".into(),
                message: "Function `handleAuth` may belong in module `src/auth.rs`".into(),
                file: "src/utils.rs".into(),
                line: 5,
                hash: String::new(),
                confidence: 0.6,
                resolution_tier: "heuristic".into(),
                fix_hint: Some("Consider moving `handleAuth` to `src/auth.rs`".into()),
                suppressed: false,
                suppress_hint: None,
                affected: vec![],
                suggested_module: Some("src/auth.rs".into()),
                existing: None,
            }],
            info: CompileInfo {
                nodes_updated: 0,
                edges_updated: 0,
                hashes_changed: vec![],
            },
        };
        let out = fmt.format_compile(&result);
        assert!(out.contains("warning[W001]"));
        assert!(out.contains("= suggested module: src/auth.rs"));
    }

    #[test]
    fn test_human_duplicate_name_warning() {
        let fmt = HumanFormatter;
        let result = CompileResult {
            version: "0.1.0".into(),
            command: "compile".into(),
            status: "warning".into(),
            files_analyzed: vec!["src/a.rs".into()],
            errors: vec![],
            warnings: vec![Violation {
                code: "W002".into(),
                severity: "WARNING".into(),
                category: "duplicate_name".into(),
                message: "Function `process` also exists in `src/b.rs`".into(),
                file: "src/a.rs".into(),
                line: 3,
                hash: String::new(),
                confidence: 0.7,
                resolution_tier: "heuristic".into(),
                fix_hint: None,
                suppressed: false,
                suppress_hint: None,
                affected: vec![],
                suggested_module: None,
                existing: Some(ExistingNode {
                    hash: "dup11111111".into(),
                    file: "src/b.rs".into(),
                    line: 10,
                }),
            }],
            info: CompileInfo {
                nodes_updated: 0,
                edges_updated: 0,
                hashes_changed: vec![],
            },
        };
        let out = fmt.format_compile(&result);
        assert!(out.contains("warning[W002]"));
        assert!(out.contains("= also at: src/b.rs:10"));
    }

    #[test]
    fn test_human_discover() {
        let fmt = HumanFormatter;
        let result = DiscoverResult {
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
            }],
            downstream: vec![],
            module_context: ModuleContext {
                module: "src/handler.rs".into(),
                sibling_functions: vec![],
                responsibility_keywords: vec!["http".into()],
                function_count: 1,
                external_endpoints: vec![],
            },
        };
        let out = fmt.format_discover(&result);
        assert!(out.contains("handleRequest [abc12345678]"));
        assert!(out.contains("--> src/handler.rs:5-20"));
        assert!(out.contains("doc: Handles incoming requests"));
        assert!(out.contains("Callers (1):"));
        assert!(out.contains("main [cal11111111] at src/main.rs:8"));
        assert!(out.contains("Module: src/handler.rs (1 functions)"));
        assert!(out.contains("keywords: http"));
    }

    #[test]
    fn test_human_explain() {
        let fmt = HumanFormatter;
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
            summary: "E001 on `handleRequest` in src/handler.rs:5".into(),
        };
        let out = fmt.format_explain(&result);
        assert!(out.contains("Explanation for E001 on hash abc12345678"));
        assert!(out.contains("confidence: 92%"));
        assert!(out.contains("tier: tree-sitter"));
        assert!(out.contains("1. [call] src/main.rs:8"));
        assert!(out.contains("E001 on `handleRequest`"));
    }

    #[test]
    fn test_human_suppressed_violation() {
        let fmt = HumanFormatter;
        let result = CompileResult {
            version: "0.1.0".into(),
            command: "compile".into(),
            status: "ok".into(),
            files_analyzed: vec!["src/lib.rs".into()],
            errors: vec![],
            warnings: vec![Violation {
                code: "S001".into(),
                severity: "INFO".into(),
                category: "broken_caller".into(),
                message: "Signature of `foo` changed".into(),
                file: "src/lib.rs".into(),
                line: 10,
                hash: "abc12345678".into(),
                confidence: 0.92,
                resolution_tier: "tree-sitter".into(),
                fix_hint: None,
                suppressed: true,
                suppress_hint: Some("Suppressed via keel suppress E001".into()),
                affected: vec![],
                suggested_module: None,
                existing: None,
            }],
            info: CompileInfo {
                nodes_updated: 0,
                edges_updated: 0,
                hashes_changed: vec![],
            },
        };
        let out = fmt.format_compile(&result);
        assert!(out.contains("info[S001]"));
        assert!(out.contains("Suppressed via keel suppress E001"));
    }
}
