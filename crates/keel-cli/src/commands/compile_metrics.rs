use std::collections::HashMap;
use std::path::Path;

use crate::telemetry_recorder::EventMetrics;

/// Build telemetry metrics from a compile result.
pub fn build_compile_metrics(
    result: &keel_enforce::types::CompileResult,
    target_files: &[String],
) -> EventMetrics {
    let mut error_codes = HashMap::new();
    for v in result.errors.iter().chain(result.warnings.iter()) {
        *error_codes.entry(v.code.clone()).or_default() += 1;
    }

    EventMetrics {
        error_count: result.errors.len() as u32,
        warning_count: result.warnings.len() as u32,
        node_count: result.info.nodes_updated,
        edge_count: result.info.edges_updated,
        error_codes,
        language_mix: build_language_mix(target_files),
        ..Default::default()
    }
}

/// Build language mix percentages from a list of file paths.
pub fn build_language_mix(files: &[String]) -> HashMap<String, u32> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    for file in files {
        if let Some(lang) = keel_parsers::treesitter::detect_language(Path::new(file)) {
            *counts.entry(lang.to_string()).or_default() += 1;
        }
    }
    let total = counts.values().sum::<u32>();
    if total == 0 {
        return HashMap::new();
    }
    counts
        .into_iter()
        .map(|(lang, count)| (lang, (count * 100) / total))
        .collect()
}
