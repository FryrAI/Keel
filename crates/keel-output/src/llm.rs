use crate::OutputFormatter;
use keel_enforce::types::{CompileResult, DiscoverResult, ExplainResult, Violation};

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
