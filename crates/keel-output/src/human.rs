use crate::OutputFormatter;
use keel_enforce::types::{CompileResult, DiscoverResult, ExplainResult, Violation};

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
