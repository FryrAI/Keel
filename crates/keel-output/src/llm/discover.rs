use keel_enforce::types::DiscoverResult;

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
            result.module_context.module,
            result.module_context.function_count,
        ));
    }

    out
}
