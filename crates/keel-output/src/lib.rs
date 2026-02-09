pub mod json;
pub mod llm;
pub mod human;

use keel_enforce::types::{CompileResult, DiscoverResult, ExplainResult};

pub trait OutputFormatter {
    fn format_compile(&self, result: &CompileResult) -> String;
    fn format_discover(&self, result: &DiscoverResult) -> String;
    fn format_explain(&self, result: &ExplainResult) -> String;
}
