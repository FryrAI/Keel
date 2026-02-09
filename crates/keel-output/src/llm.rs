use crate::OutputFormatter;
use keel_enforce::types::{CompileResult, DiscoverResult, ExplainResult};

pub struct LlmFormatter;

impl OutputFormatter for LlmFormatter {
    fn format_compile(&self, _result: &CompileResult) -> String {
        // TODO: Token-optimized LLM format
        String::new()
    }
    fn format_discover(&self, _result: &DiscoverResult) -> String {
        String::new()
    }
    fn format_explain(&self, _result: &ExplainResult) -> String {
        String::new()
    }
}
