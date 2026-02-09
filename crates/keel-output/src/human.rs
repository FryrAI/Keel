use crate::OutputFormatter;
use keel_enforce::types::{CompileResult, DiscoverResult, ExplainResult};

pub struct HumanFormatter;

impl OutputFormatter for HumanFormatter {
    fn format_compile(&self, _result: &CompileResult) -> String {
        // TODO: Colored human-readable format
        String::new()
    }
    fn format_discover(&self, _result: &DiscoverResult) -> String {
        String::new()
    }
    fn format_explain(&self, _result: &ExplainResult) -> String {
        String::new()
    }
}
