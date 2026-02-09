use crate::OutputFormatter;
use keel_enforce::types::{CompileResult, DiscoverResult, ExplainResult};

pub struct JsonFormatter;

impl OutputFormatter for JsonFormatter {
    fn format_compile(&self, result: &CompileResult) -> String {
        serde_json::to_string_pretty(result).unwrap_or_default()
    }
    fn format_discover(&self, result: &DiscoverResult) -> String {
        serde_json::to_string_pretty(result).unwrap_or_default()
    }
    fn format_explain(&self, result: &ExplainResult) -> String {
        serde_json::to_string_pretty(result).unwrap_or_default()
    }
}
