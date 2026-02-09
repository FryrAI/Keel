use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "keel", version, about = "Structural code enforcement for LLM agents")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output as structured JSON
    #[arg(long, global = true)]
    json: bool,

    /// Output as token-optimized LLM format
    #[arg(long, global = true)]
    llm: bool,

    /// Include info block in output
    #[arg(long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
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

    /// Remove all keel-generated files
    Deinit,

    /// Display telemetry dashboard
    Stats,
}

fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.command {
        Commands::Init => { eprintln!("keel init: not yet implemented"); 2 }
        Commands::Map { .. } => { eprintln!("keel map: not yet implemented"); 2 }
        Commands::Discover { .. } => { eprintln!("keel discover: not yet implemented"); 2 }
        Commands::Compile { .. } => { eprintln!("keel compile: not yet implemented"); 2 }
        Commands::Where { .. } => { eprintln!("keel where: not yet implemented"); 2 }
        Commands::Explain { .. } => { eprintln!("keel explain: not yet implemented"); 2 }
        Commands::Serve { .. } => { eprintln!("keel serve: not yet implemented"); 2 }
        Commands::Deinit => { eprintln!("keel deinit: not yet implemented"); 2 }
        Commands::Stats => { eprintln!("keel stats: not yet implemented"); 2 }
    };

    std::process::exit(exit_code);
}
