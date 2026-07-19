use keel_enforce::types::{DiscoverResult, FileSymbols};

/// Formats a file's symbol listing (or a name lookup) in LLM-compact format.
///
/// File mode (`path` set) prints a `FILE ... symbols=N` header with one indented
/// line per symbol; name mode prints one `name hash=... file:line ...` line per
/// match.
pub fn format_file_symbols(result: &FileSymbols) -> String {
    let mut out = String::new();
    if let Some(path) = &result.path {
        out.push_str(&format!("FILE {} symbols={}\n", path, result.symbols.len()));
        for s in &result.symbols {
            out.push_str(&format!(
                "  {} {} hash={} line={} callers={} callees={}\n",
                s.kind, s.name, s.hash, s.line, s.callers, s.callees,
            ));
        }
    } else {
        for s in &result.symbols {
            out.push_str(&format!(
                "{} hash={} {}:{} callers={} callees={}\n",
                s.name, s.hash, s.file, s.line, s.callers, s.callees,
            ));
        }
    }
    out
}

/// Formats a discover result showing a node's callers, callees, module context, and body.
pub fn format_discover(result: &DiscoverResult) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "DISCOVER hash={} name={} file={}:{}-{}\n",
        result.target.hash,
        result.target.name,
        result.target.file,
        result.target.line_start,
        result.target.line_end,
    ));

    if !result.upstream.is_empty() {
        out.push_str(&format!("CALLERS count={}\n", result.upstream.len()));
        for c in &result.upstream {
            out.push_str(&format!(
                "  d={} {}@{}:{} sig={}\n",
                c.distance, c.hash, c.file, c.call_line, c.signature
            ));
        }
    }

    if !result.downstream.is_empty() {
        out.push_str(&format!("CALLEES count={}\n", result.downstream.len()));
        for c in &result.downstream {
            out.push_str(&format!(
                "  d={} {}@{}:{} sig={}\n",
                c.distance, c.hash, c.file, c.call_line, c.signature
            ));
        }
    }

    if !result.module_context.module.is_empty() {
        out.push_str(&format!(
            "MODULE {} fns={}\n",
            result.module_context.module, result.module_context.function_count,
        ));
    }

    // Append body context if present
    if let Some(ref ctx) = result.body_context {
        let header = if ctx.truncated {
            format!(
                "BODY (first {} of {} lines):",
                ctx.lines.lines().count(),
                ctx.line_count
            )
        } else {
            format!("BODY ({} lines):", ctx.line_count)
        };
        out.push_str(&format!("{}\n{}\n", header, ctx.lines));
    }

    out
}
