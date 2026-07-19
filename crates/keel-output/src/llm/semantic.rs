//! Compact LLM formatter for `keel map --semantic`.

use keel_enforce::semantic::{SemanticMapResult, SemanticSymbol};

const MAX_SYMBOLS: usize = 12;

/// Render a semantic map in the compact LLM format.
pub fn format_semantic_map(result: &SemanticMapResult) -> String {
    let mut out = format!("SEMANTIC modules={}\n", result.modules.len());

    for m in &result.modules {
        out.push_str(&format!("MODULE {}\n", m.path));
        if !m.summary.is_empty() {
            out.push_str(&format!("  summary: {}\n", m.summary));
        }
        out.push_str(&format!("  when: {}\n", m.when_to_use));
        push_symbols(&mut out, "fn", &m.public_functions);
        push_symbols(&mut out, "type", &m.public_types);
    }

    out
}

fn push_symbols(out: &mut String, label: &str, syms: &[SemanticSymbol]) {
    for s in syms.iter().take(MAX_SYMBOLS) {
        out.push_str(&format!(
            "  {} {} hash={} :: {}\n",
            label, s.name, s.hash, s.signature
        ));
    }
    let more = syms.len().saturating_sub(MAX_SYMBOLS);
    if more > 0 {
        out.push_str(&format!("  {label} ... +{more} more\n"));
    }
}
