use clap::{Parser, Subcommand};

mod commands;

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

    let formatter: Box<dyn keel_output::OutputFormatter> = if cli.json {
        Box::new(keel_output::json::JsonFormatter)
    } else if cli.llm {
        Box::new(keel_output::llm::LlmFormatter)
    } else {
        Box::new(keel_output::human::HumanFormatter)
    };

    let exit_code = match cli.command {
        Commands::Init => commands::init::run(&*formatter, cli.verbose),
        Commands::Map { llm_verbose, scope, strict } => {
            commands::map::run(&*formatter, cli.verbose, llm_verbose, scope, strict)
        }
        Commands::Discover { hash, depth, suggest_placement } => {
            commands::discover::run(&*formatter, cli.verbose, hash, depth, suggest_placement)
        }
        Commands::Compile { files, batch_start, batch_end, strict, suppress } => {
            commands::compile::run(
                &*formatter, cli.verbose, files, batch_start, batch_end, strict, suppress,
            )
        }
        Commands::Where { hash } => {
            commands::where_cmd::run(&*formatter, cli.verbose, hash)
        }
        Commands::Explain { error_code, hash, tree } => {
            commands::explain::run(&*formatter, cli.verbose, error_code, hash, tree)
        }
        Commands::Serve { mcp, http, watch } => {
            commands::serve::run(&*formatter, cli.verbose, mcp, http, watch)
        }
        Commands::Deinit => commands::deinit::run(&*formatter, cli.verbose),
        Commands::Stats => commands::stats::run(&*formatter, cli.verbose),
    };

    std::process::exit(exit_code);
}
