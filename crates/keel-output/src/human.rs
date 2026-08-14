use crate::human_helpers::format_violation_human;
use crate::OutputFormatter;
use keel_enforce::checkpoint::CheckpointResult;
use keel_enforce::quality::QualityReport;
use keel_enforce::review::ReviewResult;
use keel_enforce::semantic::SemanticMapResult;
use keel_enforce::types::{
    AnalyzeResult, AuditResult, CheckResult, CompileDelta, CompileResult, DiscoverResult,
    ExplainResult, FileSymbols, FixResult, FocusResult, MapResult, NameResult, SkeletonResult,
    StatsResult,
};
use keel_enforce::validate_plan::PlanValidationResult;

/// Human-readable terminal formatter (colored, laid out for people).
///
/// By contract this formatter ignores any output token budget (`--budget` /
/// `--max-tokens`): those tune LLM-directed output only (see
/// [`crate::llm::LlmFormatter`]), so human output is never budget-truncated.
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

        if let Some(ref ctx) = result.body_context {
            let header = if ctx.truncated {
                format!(
                    "\nBody (first {} of {} lines):",
                    ctx.lines.lines().count(),
                    ctx.line_count
                )
            } else {
                format!("\nBody ({} lines):", ctx.line_count)
            };
            out.push_str(&format!("{}\n{}\n", header, ctx.lines));
        }

        out
    }

    fn format_file_symbols(&self, result: &FileSymbols) -> String {
        // A symbol listing is already a flat, human-legible table — the human
        // and LLM renderings are byte-identical, so share the one implementation
        // rather than letting two copies drift.
        crate::llm::discover::format_file_symbols(result)
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
                out.push_str(&format!(
                    "    - {}\n    + {}\n",
                    action.old_text, action.new_text
                ));
            }
            out.push('\n');
        }
        out
    }

    fn format_name(&self, result: &NameResult) -> String {
        crate::human_name::render_name(result)
    }

    fn format_check(&self, result: &CheckResult) -> String {
        let mut out = format!(
            "{} [{}] risk={} health={}\n  --> {}:{}-{}\n",
            result.target.name,
            result.target.hash,
            result.risk.level,
            result.risk.health,
            result.target.file,
            result.target.line_start,
            result.target.line_end,
        );
        out.push_str(&format!(
            "  callers={} (cross-file={}, cross-module={}), callees={}\n",
            result.risk.caller_count,
            result.risk.cross_file_callers,
            result.risk.cross_module_callers,
            result.risk.callee_count,
        ));
        if let Some(ref summary) = result.risk.caller_summary {
            out.push_str(&format!("  {}\n", summary));
            for c in result.risk.callers.iter().take(5) {
                out.push_str(&format!(
                    "    {} [{}] at {}:{}\n",
                    c.name, c.hash, c.file, c.line
                ));
            }
            if result.risk.callers.len() > 5 {
                out.push_str(&format!(
                    "    ... and {} more (use --verbose for full list)\n",
                    result.risk.callers.len() - 5
                ));
            }
        }
        if result.risk.is_public_api {
            out.push_str("  PUBLIC API\n");
        }
        for v in &result.violations {
            out.push_str(&format!("  violation: [{}] {}\n", v.code, v.message));
        }
        for s in &result.suggestions {
            out.push_str(&format!("  suggestion: [{}] {}\n", s.kind, s.message));
        }
        out
    }

    fn format_compile_delta(&self, delta: &CompileDelta) -> String {
        let mut out = format!(
            "Compile delta: net {} errors, net {} warnings\n",
            if delta.net_errors >= 0 {
                format!("+{}", delta.net_errors)
            } else {
                delta.net_errors.to_string()
            },
            if delta.net_warnings >= 0 {
                format!("+{}", delta.net_warnings)
            } else {
                delta.net_warnings.to_string()
            },
        );
        for k in &delta.new_errors {
            out.push_str(&format!(
                "  + ERROR [{}] {} at {}:{}\n",
                k.code, k.hash, k.file, k.line
            ));
        }
        for k in &delta.resolved_errors {
            out.push_str(&format!(
                "  - ERROR [{}] {} at {}:{}\n",
                k.code, k.hash, k.file, k.line
            ));
        }
        for k in &delta.new_warnings {
            out.push_str(&format!(
                "  + WARN  [{}] {} at {}:{}\n",
                k.code, k.hash, k.file, k.line
            ));
        }
        for k in &delta.resolved_warnings {
            out.push_str(&format!(
                "  - WARN  [{}] {} at {}:{}\n",
                k.code, k.hash, k.file, k.line
            ));
        }
        out.push_str(&format!(
            "  Total: {} errors, {} warnings\n",
            delta.total_errors, delta.total_warnings,
        ));
        out
    }

    fn format_audit(&self, result: &AuditResult) -> String {
        crate::radar::format_audit_display(result)
    }

    fn format_analyze(&self, result: &AnalyzeResult) -> String {
        let s = &result.structure;
        let mut out = format!(
            "Analyze: {} ({} lines, {} functions, {} classes)\n",
            result.file, s.line_count, s.function_count, s.class_count,
        );
        for f in &s.functions {
            out.push_str(&format!(
                "  {} [{}] lines {}-{} ({} lines) callers={} callees={}{}\n",
                f.name,
                f.hash,
                f.line_start,
                f.line_end,
                f.lines,
                f.callers,
                f.callees,
                if f.is_public { " PUB" } else { "" },
            ));
        }
        if !result.smells.is_empty() {
            out.push_str(&format!("\nSmells ({}):\n", result.smells.len()));
            for smell in &result.smells {
                out.push_str(&format!("  [{}] {}\n", smell.severity, smell.message));
            }
        }
        if !result.refactor_opportunities.is_empty() {
            out.push_str(&format!(
                "\nRefactoring ({}):\n",
                result.refactor_opportunities.len()
            ));
            for r in &result.refactor_opportunities {
                out.push_str(&format!("  {}\n", r.message));
            }
        }
        out
    }

    fn format_skeleton(&self, result: &SkeletonResult) -> String {
        let mut out = format!(
            "Skeleton: {} ({}, {} symbols)\n",
            result.file,
            result.language,
            result.symbols.len(),
        );
        if !result.imports.is_empty() {
            out.push_str(&format!("Imports: {}\n", result.imports.join(", ")));
        }
        for s in &result.symbols {
            out.push_str(&format!(
                "  {}{}  L{}\n",
                if s.is_public { "" } else { "[private] " },
                s.signature,
                s.line,
            ));
            if let Some(doc) = &s.docstring {
                out.push_str(&format!("    doc: {}\n", doc));
            }
        }
        out
    }

    fn format_focus(&self, result: &FocusResult) -> String {
        let mut out = format!(
            "Focus: {} (depth {}, {} files, {} callers at risk)\n",
            result.target,
            result.depth,
            result.files.len(),
            result.callers.len(),
        );
        out.push_str("\nFiles to read (ranked):\n");
        for f in &result.files {
            out.push_str(&format!(
                "  {} [{}] distance={}\n",
                f.path, f.role, f.distance,
            ));
            for s in &f.symbols {
                out.push_str(&format!(
                    "    {} [{}] L{} callers={} callees={}\n",
                    s.name, s.hash, s.line, s.callers, s.callees,
                ));
            }
        }
        if !result.callers.is_empty() {
            out.push_str("\nSymbols at risk (callers):\n");
            for c in &result.callers {
                out.push_str(&format!(
                    "  {} [{}] {}:{} distance={} callers={}\n",
                    c.name, c.hash, c.file, c.line, c.distance, c.callers,
                ));
            }
        }
        out.push_str(&format!(
            "\nSuggested read order: {}\n",
            result.read_order.join(" -> "),
        ));
        out
    }

    fn format_checkpoint(&self, result: &CheckpointResult) -> String {
        let mut out = format!(
            "Checkpoint ({}): {} file(s) changed, {} error(s), {} warning(s)\n",
            result.range,
            result.files.len(),
            result.error_count,
            result.warning_count,
        );
        for fd in &result.files {
            out.push_str(&format!("\n{}:\n", fd.file));
            for s in &fd.added {
                out.push_str(&format!("  + {} [{}]\n", s.name, s.hash));
            }
            for s in &fd.changed {
                out.push_str(&format!("  ~ {} [{}]\n", s.name, s.hash));
            }
            for s in &fd.removed {
                out.push_str(&format!("  - {} [{}]\n", s.name, s.hash));
            }
        }
        if !result.affected_callers.is_empty() {
            out.push_str("\nCallers at risk:\n");
            for ac in &result.affected_callers {
                out.push_str(&format!("  {}:\n", ac.symbol));
                for c in &ac.callers {
                    out.push_str(&format!("    {} at {}:{}\n", c.name, c.file, c.line));
                }
            }
        }
        if !result.violations.is_empty() {
            out.push_str("\nOutstanding violations:\n");
            for v in &result.violations {
                out.push_str(&format!(
                    "  [{}] {} at {}:{} — {}\n",
                    v.code, v.severity, v.file, v.line, v.message
                ));
            }
        }
        if !result.commits.is_empty() {
            out.push_str("\nRecent commits:\n");
            for c in &result.commits {
                out.push_str(&format!("  {}\n", c));
            }
        }
        out
    }

    fn format_validate_plan(&self, result: &PlanValidationResult) -> String {
        if result.unrecognized && result.findings.is_empty() {
            return "No graph-relevant actions detected in the plan.\n".to_string();
        }
        let mut out = format!(
            "Plan validation: {} action(s), {} symbol(s), {} finding(s) detected\n",
            result.actions.len(),
            result.symbols_detected,
            result.findings.len(),
        );
        for f in &result.findings {
            out.push_str(&format!("\n[{}] {} {}\n", f.code, f.severity, f.message));
            if !f.hash.is_empty() {
                out.push_str(&format!("  --> {}:{} (hash={})\n", f.file, f.line, f.hash));
            }
            out.push_str(&format!("  fix: {}\n", f.fix_hint));
        }
        for a in &result.actions {
            out.push_str(&format!(
                "\n[{}] {} `{}` (hash={})\n  --> {}:{}\n  risk: {} ({} caller(s))\n",
                a.risk, a.action, a.symbol, a.hash, a.file, a.line, a.risk, a.caller_count,
            ));
            for c in &a.callers {
                out.push_str(&format!(
                    "    caller: {} at {}:{}\n",
                    c.name, c.file, c.line
                ));
            }
            out.push_str(&format!("  order: {}\n", a.suggested_order));
        }
        if !result.files_detected.is_empty() {
            out.push_str(&format!(
                "\nFiles referenced: {}\n",
                result.files_detected.join(", ")
            ));
        }
        out
    }

    fn format_review(&self, result: &ReviewResult) -> String {
        crate::human_review::format_review_human(result)
    }

    fn format_quality(&self, result: &QualityReport) -> String {
        crate::quality_fmt::human(result)
    }

    fn format_stats(&self, result: &StatsResult) -> String {
        crate::stats_fmt::human(result)
    }

    fn format_semantic_map(&self, result: &SemanticMapResult) -> String {
        let mut out = format!("Semantic map: {} module(s)\n", result.modules.len());
        for m in &result.modules {
            out.push_str(&format!("\n{}\n", m.path));
            if !m.summary.is_empty() {
                out.push_str(&format!("  summary: {}\n", m.summary));
            }
            out.push_str(&format!("  when to use: {}\n", m.when_to_use));
            for f in &m.public_functions {
                out.push_str(&format!("  fn {} [{}] — {}\n", f.name, f.hash, f.signature));
            }
            for t in &m.public_types {
                out.push_str(&format!(
                    "  type {} [{}] — {}\n",
                    t.name, t.hash, t.signature
                ));
            }
        }
        out
    }
}
