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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("failed to parse CLI args")
    }

    fn parse_err(args: &[&str]) -> clap::error::Error {
        Cli::try_parse_from(args).expect_err("expected parse failure")
    }

    // --- Subcommand wiring ---

    #[test]
    fn parse_init() {
        let cli = parse(&["keel", "init"]);
        assert!(matches!(cli.command, Commands::Init));
    }

    #[test]
    fn parse_map_defaults() {
        let cli = parse(&["keel", "map"]);
        match cli.command {
            Commands::Map { llm_verbose, scope, strict } => {
                assert!(!llm_verbose);
                assert!(scope.is_none());
                assert!(!strict);
            }
            _ => panic!("expected Map"),
        }
    }

    #[test]
    fn parse_map_all_flags() {
        let cli = parse(&["keel", "map", "--llm-verbose", "--scope", "auth,core", "--strict"]);
        match cli.command {
            Commands::Map { llm_verbose, scope, strict } => {
                assert!(llm_verbose);
                assert_eq!(scope.as_deref(), Some("auth,core"));
                assert!(strict);
            }
            _ => panic!("expected Map"),
        }
    }

    #[test]
    fn parse_discover_required_hash() {
        let cli = parse(&["keel", "discover", "abc123"]);
        match cli.command {
            Commands::Discover { hash, depth, suggest_placement } => {
                assert_eq!(hash, "abc123");
                assert_eq!(depth, 1); // default
                assert!(!suggest_placement);
            }
            _ => panic!("expected Discover"),
        }
    }

    #[test]
    fn parse_discover_with_depth() {
        let cli = parse(&["keel", "discover", "h1", "--depth", "3", "--suggest-placement"]);
        match cli.command {
            Commands::Discover { hash, depth, suggest_placement } => {
                assert_eq!(hash, "h1");
                assert_eq!(depth, 3);
                assert!(suggest_placement);
            }
            _ => panic!("expected Discover"),
        }
    }

    #[test]
    fn parse_discover_missing_hash() {
        parse_err(&["keel", "discover"]);
    }

    #[test]
    fn parse_compile_no_files() {
        let cli = parse(&["keel", "compile"]);
        match cli.command {
            Commands::Compile { files, batch_start, batch_end, strict, suppress } => {
                assert!(files.is_empty());
                assert!(!batch_start);
                assert!(!batch_end);
                assert!(!strict);
                assert!(suppress.is_none());
            }
            _ => panic!("expected Compile"),
        }
    }

    #[test]
    fn parse_compile_with_files() {
        let cli = parse(&["keel", "compile", "src/main.rs", "src/lib.rs"]);
        match cli.command {
            Commands::Compile { files, .. } => {
                assert_eq!(files, vec!["src/main.rs", "src/lib.rs"]);
            }
            _ => panic!("expected Compile"),
        }
    }

    #[test]
    fn parse_compile_batch_start() {
        let cli = parse(&["keel", "compile", "--batch-start"]);
        match cli.command {
            Commands::Compile { batch_start, batch_end, .. } => {
                assert!(batch_start);
                assert!(!batch_end);
            }
            _ => panic!("expected Compile"),
        }
    }

    #[test]
    fn parse_compile_batch_end() {
        let cli = parse(&["keel", "compile", "--batch-end"]);
        match cli.command {
            Commands::Compile { batch_start, batch_end, .. } => {
                assert!(!batch_start);
                assert!(batch_end);
            }
            _ => panic!("expected Compile"),
        }
    }

    #[test]
    fn parse_compile_strict_and_suppress() {
        let cli = parse(&["keel", "compile", "--strict", "--suppress", "W001"]);
        match cli.command {
            Commands::Compile { strict, suppress, .. } => {
                assert!(strict);
                assert_eq!(suppress.as_deref(), Some("W001"));
            }
            _ => panic!("expected Compile"),
        }
    }

    #[test]
    fn parse_where_cmd() {
        let cli = parse(&["keel", "where", "xyz789"]);
        match cli.command {
            Commands::Where { hash } => assert_eq!(hash, "xyz789"),
            _ => panic!("expected Where"),
        }
    }

    #[test]
    fn parse_where_missing_hash() {
        parse_err(&["keel", "where"]);
    }

    #[test]
    fn parse_explain() {
        let cli = parse(&["keel", "explain", "E001", "abc123"]);
        match cli.command {
            Commands::Explain { error_code, hash, tree } => {
                assert_eq!(error_code, "E001");
                assert_eq!(hash, "abc123");
                assert!(!tree);
            }
            _ => panic!("expected Explain"),
        }
    }

    #[test]
    fn parse_explain_with_tree() {
        let cli = parse(&["keel", "explain", "E001", "abc123", "--tree"]);
        match cli.command {
            Commands::Explain { tree, .. } => assert!(tree),
            _ => panic!("expected Explain"),
        }
    }

    #[test]
    fn parse_explain_missing_args() {
        parse_err(&["keel", "explain"]);
        parse_err(&["keel", "explain", "E001"]); // missing hash
    }

    #[test]
    fn parse_serve_mcp() {
        let cli = parse(&["keel", "serve", "--mcp"]);
        match cli.command {
            Commands::Serve { mcp, http, watch } => {
                assert!(mcp);
                assert!(!http);
                assert!(!watch);
            }
            _ => panic!("expected Serve"),
        }
    }

    #[test]
    fn parse_serve_http_watch() {
        let cli = parse(&["keel", "serve", "--http", "--watch"]);
        match cli.command {
            Commands::Serve { mcp, http, watch } => {
                assert!(!mcp);
                assert!(http);
                assert!(watch);
            }
            _ => panic!("expected Serve"),
        }
    }

    #[test]
    fn parse_deinit() {
        let cli = parse(&["keel", "deinit"]);
        assert!(matches!(cli.command, Commands::Deinit));
    }

    #[test]
    fn parse_stats() {
        let cli = parse(&["keel", "stats"]);
        assert!(matches!(cli.command, Commands::Stats));
    }

    // --- Global flags ---

    #[test]
    fn global_json_flag() {
        let cli = parse(&["keel", "--json", "stats"]);
        assert!(cli.json);
        assert!(!cli.llm);
        assert!(!cli.verbose);
    }

    #[test]
    fn global_llm_flag() {
        let cli = parse(&["keel", "--llm", "init"]);
        assert!(!cli.json);
        assert!(cli.llm);
    }

    #[test]
    fn global_verbose_flag() {
        let cli = parse(&["keel", "--verbose", "map"]);
        assert!(cli.verbose);
    }

    #[test]
    fn global_flags_after_subcommand() {
        // clap global flags can appear after the subcommand too
        let cli = parse(&["keel", "compile", "--json", "--verbose"]);
        assert!(cli.json);
        assert!(cli.verbose);
    }

    #[test]
    fn multiple_global_flags() {
        let cli = parse(&["keel", "--json", "--verbose", "deinit"]);
        assert!(cli.json);
        assert!(cli.verbose);
    }

    // --- Error cases ---

    #[test]
    fn no_subcommand_is_error() {
        parse_err(&["keel"]);
    }

    #[test]
    fn unknown_subcommand_is_error() {
        parse_err(&["keel", "foobar"]);
    }

    #[test]
    fn unknown_flag_is_error() {
        parse_err(&["keel", "--not-a-flag", "init"]);
    }
}
