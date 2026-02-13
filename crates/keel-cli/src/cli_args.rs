use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "keel", version, about = "Structural code enforcement for LLM agents")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output as structured JSON
    #[arg(long, global = true)]
    pub json: bool,

    /// Output as token-optimized LLM format
    #[arg(long, global = true)]
    pub llm: bool,

    /// Include info block in output
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Max token budget for LLM output (default: 500)
    #[arg(long, global = true)]
    pub max_tokens: Option<usize>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    /// Initialize keel in a repository
    Init,

    /// Full re-map of the codebase
    Map {
        /// LLM format with full signatures
        #[arg(long)]
        llm_verbose: bool,
        /// Comma-separated module names for scoped maps
        #[arg(long)]
        scope: Option<String>,
        /// Exit non-zero on any ERROR-level violations
        #[arg(long)]
        strict: bool,
        /// Output depth: 0=summary, 1=modules+hotspots (default), 2=functions, 3=full graph
        #[arg(long, default_value = "1")]
        depth: u32,
    },

    /// Look up a function's callers, callees, and context
    Discover {
        /// Function hash to discover
        hash: String,
        /// Number of hops to traverse (default: 1)
        #[arg(long, default_value = "1")]
        depth: u32,
        /// Return top 3 placement suggestions
        #[arg(long)]
        suggest_placement: bool,
    },

    /// Incrementally validate after file changes
    Compile {
        /// Files to compile (empty = all changed)
        files: Vec<String>,
        /// Begin batch mode
        #[arg(long)]
        batch_start: bool,
        /// End batch mode
        #[arg(long)]
        batch_end: bool,
        /// Treat warnings as errors
        #[arg(long)]
        strict: bool,
        /// Suppress a specific error/warning code
        #[arg(long)]
        suppress: Option<String>,
        /// Output depth: 0=counts, 1=grouped by file (default), 2=full detail
        #[arg(long, default_value = "1")]
        depth: u32,
    },

    /// Resolve a hash to file:line
    Where {
        /// Function hash to locate
        hash: String,
    },

    /// Show resolution reasoning for an error
    Explain {
        /// Error code (e.g., E001)
        error_code: String,
        /// Function hash
        hash: String,
        /// Human-readable tree output
        #[arg(long)]
        tree: bool,
        /// Resolution depth: 0=summary, 1=first hop (default), 2=two hops, 3=full chain
        #[arg(long, default_value = "1", value_parser = clap::value_parser!(u32).range(0..=3))]
        depth: u32,
    },

    /// Run persistent server (MCP/HTTP/watch)
    Serve {
        /// MCP over stdio
        #[arg(long)]
        mcp: bool,
        /// HTTP API on localhost:4815
        #[arg(long)]
        http: bool,
        /// File system watcher
        #[arg(long)]
        watch: bool,
    },

    /// Generate fix plans for violations
    Fix {
        /// Violation hashes to fix (empty = all)
        hashes: Vec<String>,
        /// Fix only violations in this file
        #[arg(long)]
        file: Option<String>,
        /// Apply fixes (writes files). Default: plan-only
        #[arg(long)]
        apply: bool,
    },

    /// Suggest names and locations for new code
    Name {
        /// Natural-language description of what to add
        description: String,
        /// Constrain search to this module/file
        #[arg(long)]
        module: Option<String>,
        /// Kind of entity: fn, class, method
        #[arg(long)]
        kind: Option<String>,
    },

    /// Remove all keel-generated files
    Deinit,

    /// Display telemetry dashboard
    Stats,
}

#[cfg(test)]
#[path = "cli_args_tests.rs"]
mod tests;
