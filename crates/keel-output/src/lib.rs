//! Output formatters for keel command results.
//!
//! Provides three output modes:
//! - **JSON** (`--json`): Machine-readable structured output
//! - **LLM** (default): Compact format optimized for AI coding agents
//! - **Human** (`--human`): Colored, formatted output for terminal users
//!
//! Plus one CI-only surface that is not a formatter: `github` renders
//! violations as GitHub Actions workflow commands (`--format github`, see the
//! `github` module).

pub mod github;
pub mod human;
pub(crate) mod human_helpers;
pub(crate) mod human_review;
pub mod json;
pub mod llm;
pub mod radar;
pub mod token_budget;

use keel_enforce::checkpoint::CheckpointResult;
use keel_enforce::review::ReviewResult;
use keel_enforce::semantic::SemanticMapResult;
use keel_enforce::types::{
    AnalyzeResult, AuditResult, CheckResult, CompileDelta, CompileResult, DiscoverResult,
    ExplainResult, FileSymbols, FixResult, FocusResult, MapResult, NameResult, SkeletonResult,
};
use keel_enforce::validate_plan::PlanValidationResult;

pub trait OutputFormatter {
    fn format_compile(&self, result: &CompileResult) -> String;
    fn format_discover(&self, result: &DiscoverResult) -> String;
    fn format_file_symbols(&self, result: &FileSymbols) -> String;
    fn format_explain(&self, result: &ExplainResult) -> String;
    fn format_map(&self, result: &MapResult) -> String;
    fn format_fix(&self, result: &FixResult) -> String;
    fn format_name(&self, result: &NameResult) -> String;
    fn format_check(&self, result: &CheckResult) -> String;
    fn format_compile_delta(&self, delta: &CompileDelta) -> String;
    fn format_analyze(&self, result: &AnalyzeResult) -> String;
    fn format_audit(&self, result: &AuditResult) -> String;
    fn format_skeleton(&self, result: &SkeletonResult) -> String;
    fn format_focus(&self, result: &FocusResult) -> String;
    /// Format a `keel checkpoint` session summary.
    fn format_checkpoint(&self, result: &CheckpointResult) -> String;
    /// Format a `keel validate-plan` report.
    fn format_validate_plan(&self, result: &PlanValidationResult) -> String;
    /// Format a `keel map --semantic` enriched map.
    fn format_semantic_map(&self, result: &SemanticMapResult) -> String;
    /// Format a `keel review --base <ref>` PR cover letter.
    fn format_review(&self, result: &ReviewResult) -> String;
}
