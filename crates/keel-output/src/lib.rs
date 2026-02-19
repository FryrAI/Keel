//! Output formatters for keel command results.
//!
//! Provides three output modes:
//! - **JSON** (`--json`): Machine-readable structured output
//! - **LLM** (default): Compact format optimized for AI coding agents
//! - **Human** (`--human`): Colored, formatted output for terminal users

pub mod human;
pub(crate) mod human_helpers;
pub mod json;
pub mod llm;
pub mod token_budget;

use keel_enforce::types::{
    AnalyzeResult, CheckResult, CompileDelta, CompileResult, DiscoverResult, ExplainResult,
    FixResult, MapResult, NameResult,
};

pub trait OutputFormatter {
    fn format_compile(&self, result: &CompileResult) -> String;
    fn format_discover(&self, result: &DiscoverResult) -> String;
    fn format_explain(&self, result: &ExplainResult) -> String;
    fn format_map(&self, result: &MapResult) -> String;
    fn format_fix(&self, result: &FixResult) -> String;
    fn format_name(&self, result: &NameResult) -> String;
    fn format_check(&self, result: &CheckResult) -> String;
    fn format_compile_delta(&self, delta: &CompileDelta) -> String;
    fn format_analyze(&self, result: &AnalyzeResult) -> String;
}
