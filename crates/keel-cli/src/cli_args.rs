use clap::{Parser, Subcommand, ValueEnum};

/// Machine surfaces that are not one of the three output formatters.
///
/// `--json` and `--llm` choose how keel *describes* a result; this chooses a
/// foreign protocol to emit it in. Only CI has one so far.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum WireFormat {
    /// GitHub Actions workflow commands — `::error file=..,line=..,title=[CODE]::msg`
    Github,
}

#[derive(Parser, Debug)]
#[command(
    name = "keel",
    version = env!("KEEL_VERSION_FULL"),
    about = "Structural code enforcement for LLM agents"
)]
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

    /// Max token budget for LLM output of compile/map/audit (default: 500)
    #[arg(long, global = true)]
    pub max_tokens: Option<usize>,

    /// Disable telemetry for this invocation (also: KEEL_NO_TELEMETRY=1)
    #[arg(long, global = true, env = "KEEL_NO_TELEMETRY")]
    pub no_telemetry: bool,

    /// Explicit calling-agent name for telemetry attribution, e.g. "claude-code"
    /// (overrides env-based detection). Set by the generated
    /// `.keel/hooks/post-edit.sh` — subprocess env vars like `CLAUDECODE` don't
    /// reliably survive the hook boundary.
    #[arg(long, global = true)]
    pub client: Option<String>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    /// Initialize keel in a repository
    Init {
        /// Merge with existing .keel/ configuration instead of failing
        #[arg(long)]
        merge: bool,
        /// Skip interactive prompt (use detected agents only)
        #[arg(long, short)]
        yes: bool,
        /// Rewrite the keel-managed block in existing agent doc files,
        /// regenerate `.keel/hooks/post-edit.sh`, and sync `.keel/keel.json`'s
        /// pinned version — the authorized fix for the drift `map`/`compile`
        /// warn about. Does not merge config or touch tool detection.
        #[arg(long)]
        update_docs: bool,
    },

    /// Full re-map of the codebase
    Map {
        /// Output depth: 0=summary, 1=modules+hotspots (default), 2=functions, 3=full graph
        #[arg(long, default_value = "1")]
        depth: u32,
        /// Enable Tier 3 (LSP/SCIP) resolution for unresolved references
        #[arg(long)]
        tier3: bool,
        /// Read from existing graph.db instead of re-parsing (fast, for hooks)
        #[arg(long)]
        cached: bool,
        /// Emit deterministic per-module semantic enrichment (summary, public API, when-to-use)
        #[arg(long)]
        semantic: bool,
    },

    /// Look up a function's callers, callees, and context (accepts hash, file path, or --name)
    Discover {
        /// Hash, file path, or function name to discover
        query: String,
        /// Number of hops to traverse (default: 1)
        #[arg(long, default_value = "1")]
        depth: u32,
        /// Look up by function name instead of hash
        #[arg(long)]
        name: bool,
        /// Include N lines of source code (default: 5 when flag present)
        #[arg(long, default_missing_value = "5", num_args = 0..=1)]
        context: Option<u32>,
    },

    /// Search the graph by function/class name
    Search {
        /// Name or substring to search for
        term: String,
        /// Filter by kind: function, class, module
        #[arg(long)]
        kind: Option<String>,
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
        /// Only compile files changed since last commit (git diff HEAD)
        #[arg(long)]
        changed: bool,
        /// Only compile files changed since a specific commit
        #[arg(long)]
        since: Option<String>,
        /// Show only new/resolved violations compared to last compile
        #[arg(long)]
        delta: bool,
        /// Soft time budget in milliseconds (warns on stderr when exceeded; violations still report and set the exit code)
        #[arg(long)]
        timeout: Option<u64>,
        /// Emit violations in a CI protocol instead of a keel format (`github`)
        #[arg(long, value_enum)]
        format: Option<WireFormat>,
    },

    /// Pre-edit risk assessment for a function
    Check {
        /// Hash, file path, or function name to check
        query: String,
        /// Look up by function name instead of hash
        #[arg(long)]
        name: bool,
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

    /// Watch files and auto-compile on changes
    Watch,

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
        /// Add deterministic semantic-concept candidates. Candidate-only:
        /// never emits W010/P003 and never gates.
        #[arg(long)]
        semantic: bool,
    },

    /// Analyze a file for structure, smells, and refactoring opportunities
    Analyze {
        /// File path to analyze
        file: String,
    },

    /// AI-readiness scorecard: structure, discoverability, navigation, agent config
    Audit {
        /// Only audit git-changed files (fast, for hooks)
        #[arg(long)]
        changed: bool,
        /// Exit 1 if any FAIL findings
        #[arg(long)]
        strict: bool,
        /// Exit 1 if total score < threshold
        #[arg(long)]
        min_score: Option<u32>,
        /// Only run one dimension: structure, discoverability, navigation, config
        #[arg(long)]
        dimension: Option<String>,
        /// Report every module cycle, including Rust ones (legal, idiomatic)
        /// and cycles longer than 8 modules
        #[arg(long)]
        strict_cycles: bool,
        /// Max findings to print with --llm (0 = no cap)
        #[arg(long, default_value_t = keel_output::llm::audit::DEFAULT_TOP)]
        top: usize,
    },

    /// Minimal structural context for safely editing a file
    Context {
        /// File path to get context for
        file: String,
    },

    /// Compressed signature-only view of a file (no bodies)
    Skeleton {
        /// File path to summarize
        file: String,
        /// Include docstrings
        #[arg(long)]
        docs: bool,
        /// Include private symbols (default: public only)
        #[arg(long)]
        private: bool,
        /// Token budget for LLM output only (truncates, keeping whole entries);
        /// ignored with --json/--human
        #[arg(long)]
        budget: Option<usize>,
    },

    /// Minimal context set for safely modifying a target (hash or file)
    Focus {
        /// Hash or file path to focus on
        target: String,
        /// Transitive-caller traversal depth (default: 2)
        #[arg(long, default_value = "2")]
        depth: u32,
        /// Token budget for LLM output only (truncates, keeping whole entries);
        /// ignored with --json/--human
        #[arg(long)]
        budget: Option<usize>,
    },
    /// Compact session-state summary for re-injection after context loss
    Checkpoint {
        /// Diff base commit (default: HEAD — uncommitted working-tree changes)
        #[arg(long)]
        since: Option<String>,
        /// Summarize staged (index) changes instead of the working tree
        #[arg(long)]
        staged: bool,
        /// Write the checkpoint to a file instead of stdout
        #[arg(long, short = 'o')]
        output: Option<String>,
    },

    /// Two-sided graph diff against a base ref: which contracts moved, and
    /// which callers the change left behind
    Review {
        /// Base ref to diff the working tree against (e.g. `main`, `origin/main`)
        #[arg(long)]
        base: String,
        /// Emit the new violations in a CI protocol instead of a keel format (`github`)
        #[arg(long, value_enum)]
        format: Option<WireFormat>,
        /// Exit 1 when the diff introduced a violation whose code is listed in
        /// `review.gate` in keel.json (empty by default: gates nothing)
        #[arg(long)]
        gate: bool,
        /// Additionally write the `--format github` annotations rendering to
        /// this path while the primary format still goes to stdout — one
        /// review run instead of two (annotations + comment body)
        #[arg(long)]
        annotations_file: Option<String>,
    },

    /// Countable maintainability metrics from the stored graph, and their
    /// trend across snapshots
    Quality {
        /// Capture the current reading as a snapshot against HEAD (one per
        /// merge; CI runs this on the default branch)
        #[arg(long, conflicts_with_all = ["trend", "export", "import"])]
        snapshot: bool,
        /// Report the stored series instead of the current reading
        #[arg(long, conflicts_with_all = ["export", "import"])]
        trend: bool,
        /// Start the trend at this commit (short prefixes accepted)
        #[arg(long, requires = "trend", conflicts_with = "last")]
        since: Option<String>,
        /// Cap the trend to the last N snapshots
        #[arg(long, requires = "trend")]
        last: Option<usize>,
        /// Export the stored snapshot history as JSON Lines (oldest to newest)
        #[arg(long, conflicts_with_all = ["snapshot", "trend", "import"])]
        export: Option<String>,
        /// Import snapshot history from a JSON Lines file, upserting by
        /// commit_sha
        #[arg(long, conflicts_with_all = ["snapshot", "trend", "export"])]
        import: Option<String>,
    },

    /// Validate a plan against the dependency graph before executing it
    ValidatePlan {
        /// Plan file to read (markdown/text), or `-` for stdin
        plan: String,

        /// Exit 1 when a P001/P002 plan finding is present (default: always exit 0)
        #[arg(long)]
        strict: bool,
    },

    /// Remove all keel-generated files
    Deinit,

    /// Display telemetry dashboard
    Stats,

    /// Get or set configuration values (dot-notation supported)
    Config {
        /// Configuration key (e.g., "tier", "telemetry.enabled")
        key: Option<String>,
        /// Value to set (omit to read current value)
        value: Option<String>,
    },

    /// Update keel to the latest version
    Upgrade {
        /// Target version (default: latest)
        version: Option<String>,
        /// Skip confirmation prompt
        #[arg(long, short)]
        yes: bool,
    },

    /// Generate shell completions
    Completion {
        /// Shell to generate completions for (bash, zsh, fish, elvish, powershell)
        shell: String,
    },

    /// Authenticate with keel cloud (opens browser)
    Login,

    /// Log out and remove stored credentials
    Logout,

    /// Push graph database to keel cloud
    Push {
        /// Skip confirmation prompt
        #[arg(long, short)]
        yes: bool,
    },
}

#[cfg(test)]
#[path = "cli_args_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "cli_args_context_tests.rs"]
mod context_tests;
#[cfg(test)]
#[path = "cli_args_misc_tests.rs"]
mod misc_tests;
#[cfg(test)]
#[path = "cli_args_session_tests.rs"]
mod session_tests;
