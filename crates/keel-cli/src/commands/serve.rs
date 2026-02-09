use keel_output::OutputFormatter;

/// Run `keel serve` — start persistent server (MCP/HTTP/watch).
/// Delegates to keel-server crate.
pub fn run(
    _formatter: &dyn OutputFormatter,
    verbose: bool,
    mcp: bool,
    http: bool,
    watch: bool,
) -> i32 {
    if !mcp && !http && !watch {
        eprintln!("keel serve: at least one of --mcp, --http, or --watch required");
        return 2;
    }

    if verbose {
        let mut modes = Vec::new();
        if mcp { modes.push("MCP"); }
        if http { modes.push("HTTP"); }
        if watch { modes.push("watch"); }
        eprintln!("keel serve: starting with modes: {}", modes.join(", "));
    }

    // TODO: Delegate to keel-server crate (Agent C's work).
    // keel_server::run(mcp, http, watch)
    eprintln!("keel serve: server implementation pending (keel-server crate)");
    2
}
