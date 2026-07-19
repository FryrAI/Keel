//! Terse LLM format for `keel focus` — the minimal context set for an edit.

use crate::token_budget::render_with_budget;
use keel_enforce::types::{FocusFile, FocusResult};

/// Format a focus result in LLM-compact form.
///
/// Header, then one block per file (ranked). When `budget` is set, files are
/// truncated to fit with a `... +N more (raise --budget)` trailer; each file
/// block is kept whole. A compact `AT-RISK` list and `READ-ORDER` footer always
/// follow so the caller keeps the affected symbols and read sequence.
pub fn format_focus(result: &FocusResult, budget: Option<usize>) -> String {
    let header = format!(
        "FOCUS target={} depth={} files={} callers={}\nFILES (read ranked):\n",
        result.target,
        result.depth,
        result.files.len(),
        result.callers.len(),
    );

    let entries: Vec<String> = result.files.iter().map(render_file).collect();

    let mut out = match budget {
        Some(max_tokens) => render_with_budget(&header, &entries, max_tokens),
        None => {
            let mut s = header;
            for e in &entries {
                s.push_str(e);
            }
            s
        }
    };

    if !result.callers.is_empty() {
        out.push_str("AT-RISK (callers):\n");
        for c in &result.callers {
            out.push_str(&format!(
                "  {} {}@{}:{} d={} callers={}\n",
                c.name, c.hash, c.file, c.line, c.distance, c.callers,
            ));
        }
    }
    if !result.read_order.is_empty() {
        out.push_str(&format!("READ-ORDER: {}\n", result.read_order.join(" > ")));
    }
    out
}

/// Render one file as a self-contained block: a header line plus its symbols.
fn render_file(f: &FocusFile) -> String {
    let mut block = format!(
        "  {} role={} dist={} syms={}\n",
        f.path,
        f.role,
        f.distance,
        f.symbols.len(),
    );
    for s in &f.symbols {
        block.push_str(&format!(
            "    {} hash={} L{} callers={} callees={} rel={}\n",
            s.name, s.hash, s.line, s.callers, s.callees, s.relation,
        ));
    }
    block
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_enforce::types::FocusSymbol;

    fn fsym(name: &str, hash: &str, file: &str, distance: u32, relation: &str) -> FocusSymbol {
        FocusSymbol {
            name: name.into(),
            hash: hash.into(),
            file: file.into(),
            line: 1,
            callers: 2,
            callees: 1,
            distance,
            relation: relation.into(),
        }
    }

    fn sample() -> FocusResult {
        FocusResult {
            version: "0".into(),
            command: "focus".into(),
            target: "targetxxxxx".into(),
            depth: 2,
            files: vec![
                FocusFile {
                    path: "src/target.rs".into(),
                    role: "target".into(),
                    distance: 0,
                    symbols: vec![fsym("target", "targetxxxxx", "src/target.rs", 0, "target")],
                },
                FocusFile {
                    path: "src/callee.rs".into(),
                    role: "callee".into(),
                    distance: 1,
                    symbols: vec![fsym("callee", "calleexxxxx", "src/callee.rs", 1, "callee")],
                },
            ],
            callers: vec![fsym(
                "caller1",
                "caller1xxxx",
                "src/caller1.rs",
                1,
                "caller",
            )],
            read_order: vec!["src/callee.rs".into(), "src/target.rs".into()],
        }
    }

    #[test]
    fn renders_files_risk_and_read_order() {
        let out = format_focus(&sample(), None);
        assert!(out.contains("FOCUS target=targetxxxxx depth=2 files=2 callers=1"));
        assert!(out.contains("src/target.rs role=target dist=0"));
        assert!(out.contains("AT-RISK (callers):"));
        assert!(out.contains("caller1 caller1xxxx@src/caller1.rs:1 d=1"));
        assert!(out.contains("READ-ORDER: src/callee.rs > src/target.rs"));
    }

    #[test]
    fn budget_truncates_files_with_trailer() {
        let mut r = sample();
        r.files = (0..30)
            .map(|i| FocusFile {
                path: format!("src/file_{i}.rs"),
                role: "caller".into(),
                distance: 2,
                symbols: vec![fsym("f", "hashhhhhhhh", "src/file.rs", 2, "caller")],
            })
            .collect();
        let out = format_focus(&r, Some(20));
        assert!(out.contains("more (raise --budget)"));
        // Footer always survives.
        assert!(out.contains("READ-ORDER:"));
    }
}
