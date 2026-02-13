use crate::human_helpers::format_violation_human;
use crate::OutputFormatter;
use keel_enforce::types::{
    CompileResult, DiscoverResult, ExplainResult, FixResult, MapResult, NameResult,
};

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

    fn format_fix(&self, result: &FixResult) -> String {
        if result.plans.is_empty() {
            return "No violations to fix.\n".to_string();
        }
        let mut out = format!(
            "Fix plan: {} violations in {} files\n\n",
            result.violations_addressed, result.files_affected,
        );
        for plan in &result.plans {
            out.push_str(&format!(
                "[{}] {} on `{}` (hash={})\n",
                plan.code, plan.category, plan.target_name, plan.hash,
            ));
            out.push_str(&format!("  Cause: {}\n", plan.cause));
            for action in &plan.actions {
                out.push_str(&format!("  Fix {}:{}:\n", action.file, action.line));
                out.push_str(&format!("    - {}\n    + {}\n", action.old_text, action.new_text));
            }
            out.push('\n');
        }
        out
    }

    fn format_name(&self, result: &NameResult) -> String {
        if result.suggestions.is_empty() {
            return format!("No naming suggestions for \"{}\".\n", result.description);
        }
        let best = &result.suggestions[0];
        let mut out = format!("Naming suggestion for \"{}\"\n\n", result.description);
        out.push_str(&format!(
            "  Location: {} (score: {:.0}%)\n",
            best.location,
            best.score * 100.0,
        ));
        out.push_str(&format!("  Suggested name: {}\n", best.suggested_name));
        out.push_str(&format!("  Convention: {}\n", best.convention));
        if let Some(after) = &best.insert_after {
            out.push_str(&format!("  Insert after: {}\n", after));
        }
        if !best.siblings.is_empty() {
            out.push_str(&format!("  Siblings: {}\n", best.siblings.join(", ")));
        }
        out
    }
}
