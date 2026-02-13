pub mod json;
pub mod llm;
pub mod human;
pub(crate) mod human_helpers;
pub mod token_budget;

use keel_enforce::types::{
    CompileResult, DiscoverResult, ExplainResult, FixResult, MapResult, NameResult,
};

pub trait OutputFormatter {
    fn format_compile(&self, result: &CompileResult) -> String;
    fn format_discover(&self, result: &DiscoverResult) -> String;
    fn format_explain(&self, result: &ExplainResult) -> String;
    fn format_map(&self, result: &MapResult) -> String;
    fn format_fix(&self, result: &FixResult) -> String;
    fn format_name(&self, result: &NameResult) -> String;
}
