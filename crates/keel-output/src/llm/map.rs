use keel_enforce::types::MapResult;
use crate::token_budget;

/// Format map result at depth 0: summary only (~2 lines).
pub fn format_map_depth0(result: &MapResult) -> String {
    let s = &result.summary;
    format!(
        "MAP nodes={} edges={} modules={} fns={} classes={}\n",
        s.total_nodes, s.total_edges, s.modules, s.functions, s.classes,
    )
}

/// Format map result at depth 1: modules + hotspots + keywords.
pub fn format_map_depth1(result: &MapResult) -> String {
    let s = &result.summary;
    let mut out = format!(
        "MAP nodes={} edges={} modules={} fns={} classes={}\n",
        s.total_nodes, s.total_edges, s.modules, s.functions, s.classes,
    );
    out.push_str(&format!(
        "LANGS {} HINTS={:.1}% DOCS={:.1}%\n",
        s.languages.join(","),
        s.type_hint_coverage,
        s.docstring_coverage,
    ));

    // Hotspots section
    if !result.hotspots.is_empty() {
        out.push_str("HOTSPOTS (most connected):\n");
        for h in &result.hotspots {
            out.push_str(&format!(
                "  {} callers={} callees={}",
                h.path, h.callers, h.callees,
            ));
            if !h.keywords.is_empty() {
                out.push_str(&format!(" [{}]", h.keywords.join(",")));
            }
            out.push('\n');
        }
    }

    // Module entries with keywords and function names
    for m in &result.modules {
        out.push_str(&format!(
            "MODULE {} fns={} cls={} edges={}",
            m.path, m.function_count, m.class_count, m.edge_count,
        ));
        if let Some(kw) = &m.responsibility_keywords {
            if !kw.is_empty() {
                out.push_str(&format!(" [{}]", kw.join(",")));
            }
        }
        out.push('\n');
        // List function names with hashes (agent-friendly)
        for f in &m.function_names {
            out.push_str(&format!(
                "  {} hash={} callers={} callees={}\n",
                f.name, f.hash, f.callers, f.callees,
            ));
        }
    }
    out
}

/// Format map result at depth 2: function-level with signatures.
pub fn format_map_depth2(result: &MapResult) -> String {
    let mut out = format_map_depth1(result);

    if !result.functions.is_empty() {
        out.push_str("FUNCTIONS:\n");
        for f in &result.functions {
            out.push_str(&format!(
                "  {} hash={} {}:{} callers={} callees={} pub={}\n",
                f.name, f.hash, f.file, f.line, f.callers, f.callees, f.is_public,
            ));
            out.push_str(&format!("    sig: {}\n", f.signature));
        }
    }
    out
}

/// Format map result at depth 3: full graph dump (debug).
pub fn format_map_depth3(result: &MapResult) -> String {
    let mut out = String::from("WARNING: depth=3 produces unbounded output (debug only)\n");
    out.push_str(&format_map_depth2(result));
    out
}

/// Format map at the given depth (0, 1, 2, 3). Default = 1.
/// Applies token budget truncation to depth 1+ output.
pub fn format_map(result: &MapResult, depth: u32, max_tokens: usize) -> String {
    let raw = match depth {
        0 => return format_map_depth0(result), // depth 0 is always tiny
        2 => format_map_depth2(result),
        3.. => format_map_depth3(result),
        _ => format_map_depth1(result),
    };
    // Apply token budget to depth 1+ output
    let lines: Vec<String> = raw.lines().map(|l| l.to_string()).collect();
    let (kept, overflow) = token_budget::truncate_to_budget(&lines, max_tokens);
    let mut out: String = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    if overflow > 0 {
        out.push_str(&format!(
            "... +{} more line(s) (use --depth 0 for summary or increase --max-tokens)\n",
            overflow
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use keel_enforce::types::*;

    fn sample_map() -> MapResult {
        MapResult {
            version: "0.1.0".into(),
            command: "map".into(),
            summary: MapSummary {
                total_nodes: 142,
                total_edges: 298,
                modules: 12,
                functions: 45,
                classes: 8,
                external_endpoints: 3,
                languages: vec!["go".into(), "python".into()],
                type_hint_coverage: 85.0,
                docstring_coverage: 62.5,
            },
            modules: vec![
                ModuleEntry {
                    path: "src/auth/".into(),
                    function_count: 12,
                    class_count: 2,
                    edge_count: 31,
                    responsibility_keywords: Some(vec!["auth".into(), "jwt".into()]),
                    external_endpoints: None,
                    function_names: vec![],
                },
                ModuleEntry {
                    path: "src/handlers/".into(),
                    function_count: 8,
                    class_count: 0,
                    edge_count: 20,
                    responsibility_keywords: Some(vec!["http".into(), "api".into()]),
                    external_endpoints: None,
                    function_names: vec![],
                },
            ],
            hotspots: vec![
                HotspotEntry {
                    path: "src/auth/middleware.rs".into(),
                    name: "validate_token".into(),
                    hash: "abc12345678".into(),
                    callers: 23,
                    callees: 8,
                    keywords: vec!["auth".into(), "jwt".into()],
                },
            ],
            depth: 1,
            functions: vec![],
        }
    }

    #[test]
    fn test_depth0_summary_only() {
        let out = format_map(&sample_map(), 0, 500);
        assert_eq!(out.lines().count(), 1);
        assert!(out.contains("MAP nodes=142 edges=298"));
    }

    #[test]
    fn test_depth1_has_hotspots_and_modules() {
        let out = format_map(&sample_map(), 1, 500);
        assert!(out.contains("HOTSPOTS"));
        assert!(out.contains("callers=23 callees=8"));
        assert!(out.contains("MODULE src/auth/"));
        assert!(out.contains("[auth,jwt]"));
    }

    #[test]
    fn test_depth2_has_functions() {
        let mut m = sample_map();
        m.functions.push(FunctionEntry {
            hash: "fn1".into(),
            name: "validate_token".into(),
            signature: "fn validate_token(token: &str) -> bool".into(),
            file: "src/auth/middleware.rs".into(),
            line: 15,
            callers: 23,
            callees: 8,
            is_public: true,
        });
        let out = format_map(&m, 2, 500);
        assert!(out.contains("FUNCTIONS:"));
        assert!(out.contains("validate_token hash=fn1"));
        assert!(out.contains("sig: fn validate_token"));
    }

    #[test]
    fn test_depth3_warns() {
        let out = format_map(&sample_map(), 3, 500);
        assert!(out.contains("WARNING: depth=3"));
    }
}
