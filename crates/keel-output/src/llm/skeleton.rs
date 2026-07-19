//! Terse LLM format for `keel skeleton` — a compressed signature-only view.

use crate::token_budget::render_with_budget;
use keel_enforce::types::{SkeletonResult, SkeletonSymbol};

/// Format a skeleton in LLM-compact form.
///
/// One header line, one `IMPORTS:` line, then one block per symbol (signature
/// plus optional `doc:`). When `budget` is set, symbols are truncated to fit
/// with a `... +N more (raise --budget)` trailer; entries are kept whole.
pub fn format_skeleton(result: &SkeletonResult, budget: Option<usize>) -> String {
    let mut header = format!(
        "SKELETON {} lang={} symbols={}\n",
        result.file,
        result.language,
        result.symbols.len(),
    );
    if !result.imports.is_empty() {
        header.push_str(&format!("IMPORTS: {}\n", result.imports.join(", ")));
    }

    let entries: Vec<String> = result.symbols.iter().map(render_symbol).collect();

    match budget {
        Some(max_tokens) => render_with_budget(&header, &entries, max_tokens),
        None => {
            let mut out = header;
            for e in &entries {
                out.push_str(e);
            }
            out
        }
    }
}

/// Render one symbol as a self-contained block (signature + optional docstring).
fn render_symbol(s: &SkeletonSymbol) -> String {
    let vis = if s.is_public { "" } else { " [priv]" };
    let mut block = format!("{}  L{}{}\n", s.signature, s.line, vis);
    if let Some(doc) = &s.docstring {
        // Keep docstrings to a single compact line.
        let one_line = doc.split_whitespace().collect::<Vec<_>>().join(" ");
        block.push_str(&format!("  doc: {}\n", one_line));
    }
    block
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(name: &str, sig: &str, line: u32, doc: Option<&str>) -> SkeletonSymbol {
        SkeletonSymbol {
            kind: "function".into(),
            name: name.into(),
            signature: sig.into(),
            is_public: true,
            line,
            docstring: doc.map(|d| d.into()),
        }
    }

    fn result(symbols: Vec<SkeletonSymbol>) -> SkeletonResult {
        SkeletonResult {
            version: "0".into(),
            command: "skeleton".into(),
            file: "src/a.ts".into(),
            language: "typescript".into(),
            imports: vec!["./z".into()],
            symbols,
        }
    }

    #[test]
    fn renders_signatures_without_bodies() {
        let r = result(vec![sym("handle", "function handle(r: Req): Res", 5, None)]);
        let out = format_skeleton(&r, None);
        assert!(out.contains("SKELETON src/a.ts lang=typescript symbols=1"));
        assert!(out.contains("IMPORTS: ./z"));
        assert!(out.contains("function handle(r: Req): Res  L5"));
        // No docstring line when none was requested.
        assert!(!out.contains("doc:"));
    }

    #[test]
    fn docs_are_included_when_present() {
        let r = result(vec![sym(
            "f",
            "def f(x: int) -> str",
            3,
            Some("Does\n  the thing."),
        )]);
        let out = format_skeleton(&r, None);
        assert!(out.contains("doc: Does the thing."));
    }

    #[test]
    fn budget_truncates_with_complete_entries_and_trailer() {
        let symbols: Vec<SkeletonSymbol> = (0..30)
            .map(|i| {
                sym(
                    &format!("fn_{i}"),
                    &format!("function fn_{i}(a: LongType): Res"),
                    i,
                    None,
                )
            })
            .collect();
        let out = format_skeleton(&result(symbols), Some(20));
        assert!(out.contains("more (raise --budget)"));
        // Header preserved.
        assert!(out.starts_with("SKELETON src/a.ts"));
        // Every non-header, non-trailer line is a complete signature entry.
        for line in out.lines() {
            if line.starts_with("SKELETON")
                || line.starts_with("IMPORTS:")
                || line.contains("more (raise --budget)")
            {
                continue;
            }
            assert!(
                line.contains("function fn_"),
                "partial entry leaked: {line}"
            );
        }
    }
}
