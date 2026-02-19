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
    assert!(matches!(cli.command, Commands::Init { merge: false, yes: false }));
}

#[test]
fn parse_init_merge() {
    let cli = parse(&["keel", "init", "--merge"]);
    assert!(matches!(cli.command, Commands::Init { merge: true, yes: false }));
}

#[test]
fn parse_init_yes() {
    let cli = parse(&["keel", "init", "--yes"]);
    assert!(matches!(cli.command, Commands::Init { merge: false, yes: true }));
}

#[test]
fn parse_init_yes_short() {
    let cli = parse(&["keel", "init", "-y"]);
    assert!(matches!(cli.command, Commands::Init { merge: false, yes: true }));
}

#[test]
fn parse_map_defaults() {
    let cli = parse(&["keel", "map"]);
    match cli.command {
        Commands::Map { llm_verbose, scope, strict, depth, tier3 } => {
            assert!(!llm_verbose);
            assert!(scope.is_none());
            assert!(!strict);
            assert_eq!(depth, 1);
            assert!(!tier3);
        }
        _ => panic!("expected Map"),
    }
}

#[test]
fn parse_map_all_flags() {
    let cli = parse(&["keel", "map", "--llm-verbose", "--scope", "auth,core", "--strict", "--depth", "2", "--tier3"]);
    match cli.command {
        Commands::Map { llm_verbose, scope, strict, depth, tier3 } => {
            assert!(llm_verbose);
            assert_eq!(scope.as_deref(), Some("auth,core"));
            assert!(strict);
            assert_eq!(depth, 2);
            assert!(tier3);
        }
        _ => panic!("expected Map"),
    }
}

#[test]
fn parse_discover_required_query() {
    let cli = parse(&["keel", "discover", "abc123"]);
    match cli.command {
        Commands::Discover { query, depth, suggest_placement, name, context } => {
            assert_eq!(query, "abc123");
            assert_eq!(depth, 1);
            assert!(!suggest_placement);
            assert!(!name);
            assert!(context.is_none());
        }
        _ => panic!("expected Discover"),
    }
}

#[test]
fn parse_discover_with_depth() {
    let cli = parse(&["keel", "discover", "h1", "--depth", "3", "--suggest-placement"]);
    match cli.command {
        Commands::Discover { query, depth, suggest_placement, .. } => {
            assert_eq!(query, "h1");
            assert_eq!(depth, 3);
            assert!(suggest_placement);
        }
        _ => panic!("expected Discover"),
    }
}

#[test]
fn parse_discover_name_mode() {
    let cli = parse(&["keel", "discover", "validate_token", "--name"]);
    match cli.command {
        Commands::Discover { query, name, .. } => {
            assert_eq!(query, "validate_token");
            assert!(name);
        }
        _ => panic!("expected Discover"),
    }
}

#[test]
fn parse_discover_missing_query() {
    parse_err(&["keel", "discover"]);
}

#[test]
fn parse_compile_no_files() {
    let cli = parse(&["keel", "compile"]);
    match cli.command {
        Commands::Compile { files, batch_start, batch_end, strict, suppress, depth, changed, since, delta, .. } => {
            assert!(files.is_empty());
            assert!(!batch_start);
            assert!(!batch_end);
            assert!(!strict);
            assert!(suppress.is_none());
            assert_eq!(depth, 1);
            assert!(!changed);
            assert!(since.is_none());
            assert!(!delta);
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
        Commands::Explain { error_code, hash, tree, depth } => {
            assert_eq!(error_code, "E001");
            assert_eq!(hash, "abc123");
            assert!(!tree);
            assert_eq!(depth, 1); // default
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
fn parse_explain_with_depth() {
    let cli = parse(&["keel", "explain", "E001", "abc123", "--depth", "3"]);
    match cli.command {
        Commands::Explain { depth, .. } => assert_eq!(depth, 3),
        _ => panic!("expected Explain"),
    }
}

#[test]
fn parse_explain_depth_zero() {
    let cli = parse(&["keel", "explain", "E001", "abc123", "--depth", "0"]);
    match cli.command {
        Commands::Explain { depth, .. } => assert_eq!(depth, 0),
        _ => panic!("expected Explain"),
    }
}

#[test]
fn parse_explain_missing_args() {
    parse_err(&["keel", "explain"]);
    parse_err(&["keel", "explain", "E001"]);
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

// --- Fix command ---

#[test]
fn parse_fix_no_args() {
    let cli = parse(&["keel", "fix"]);
    match cli.command {
        Commands::Fix { hashes, file, apply } => {
            assert!(hashes.is_empty());
            assert!(file.is_none());
            assert!(!apply);
        }
        _ => panic!("expected Fix"),
    }
}

#[test]
fn parse_fix_with_hashes() {
    let cli = parse(&["keel", "fix", "abc123", "def456"]);
    match cli.command {
        Commands::Fix { hashes, .. } => {
            assert_eq!(hashes, vec!["abc123", "def456"]);
        }
        _ => panic!("expected Fix"),
    }
}

#[test]
fn parse_fix_with_file_and_apply() {
    let cli = parse(&["keel", "fix", "--file", "src/auth.rs", "--apply"]);
    match cli.command {
        Commands::Fix { file, apply, .. } => {
            assert_eq!(file.as_deref(), Some("src/auth.rs"));
            assert!(apply);
        }
        _ => panic!("expected Fix"),
    }
}

// --- Name command ---

#[test]
fn parse_name_basic() {
    let cli = parse(&["keel", "name", "validate user JWT token"]);
    match cli.command {
        Commands::Name { description, module, kind } => {
            assert_eq!(description, "validate user JWT token");
            assert!(module.is_none());
            assert!(kind.is_none());
        }
        _ => panic!("expected Name"),
    }
}

#[test]
fn parse_name_with_options() {
    let cli = parse(&["keel", "name", "handle auth", "--module", "src/auth.rs", "--kind", "fn"]);
    match cli.command {
        Commands::Name { description, module, kind } => {
            assert_eq!(description, "handle auth");
            assert_eq!(module.as_deref(), Some("src/auth.rs"));
            assert_eq!(kind.as_deref(), Some("fn"));
        }
        _ => panic!("expected Name"),
    }
}

#[test]
fn parse_name_missing_description() {
    parse_err(&["keel", "name"]);
}

// --- Depth flags ---

#[test]
fn parse_map_depth_flag() {
    let cli = parse(&["keel", "map", "--depth", "0"]);
    match cli.command {
        Commands::Map { depth, .. } => assert_eq!(depth, 0),
        _ => panic!("expected Map"),
    }
}

#[test]
fn parse_compile_depth_flag() {
    let cli = parse(&["keel", "compile", "--depth", "2"]);
    match cli.command {
        Commands::Compile { depth, .. } => assert_eq!(depth, 2),
        _ => panic!("expected Compile"),
    }
}

// --- Search command ---

#[test]
fn parse_search_basic() {
    let cli = parse(&["keel", "search", "validate"]);
    match cli.command {
        Commands::Search { term, kind } => {
            assert_eq!(term, "validate");
            assert!(kind.is_none());
        }
        _ => panic!("expected Search"),
    }
}

#[test]
fn parse_search_with_kind() {
    let cli = parse(&["keel", "search", "auth", "--kind", "function"]);
    match cli.command {
        Commands::Search { term, kind } => {
            assert_eq!(term, "auth");
            assert_eq!(kind.as_deref(), Some("function"));
        }
        _ => panic!("expected Search"),
    }
}

// --- Watch command ---

#[test]
fn parse_watch() {
    let cli = parse(&["keel", "watch"]);
    assert!(matches!(cli.command, Commands::Watch));
}

// --- Compile --changed ---

#[test]
fn parse_compile_changed() {
    let cli = parse(&["keel", "compile", "--changed"]);
    match cli.command {
        Commands::Compile { changed, since, .. } => {
            assert!(changed);
            assert!(since.is_none());
        }
        _ => panic!("expected Compile"),
    }
}

#[test]
fn parse_compile_since() {
    let cli = parse(&["keel", "compile", "--since", "abc123"]);
    match cli.command {
        Commands::Compile { changed, since, .. } => {
            assert!(!changed);
            assert_eq!(since.as_deref(), Some("abc123"));
        }
        _ => panic!("expected Compile"),
    }
}

// --- Check command ---

#[test]
fn parse_check_basic() {
    let cli = parse(&["keel", "check", "abc123"]);
    match cli.command {
        Commands::Check { query, name } => {
            assert_eq!(query, "abc123");
            assert!(!name);
        }
        _ => panic!("expected Check"),
    }
}

#[test]
fn parse_check_name_mode() {
    let cli = parse(&["keel", "check", "validate_token", "--name"]);
    match cli.command {
        Commands::Check { query, name } => {
            assert_eq!(query, "validate_token");
            assert!(name);
        }
        _ => panic!("expected Check"),
    }
}

#[test]
fn parse_check_missing_query() {
    parse_err(&["keel", "check"]);
}

// --- Analyze command ---

#[test]
fn parse_analyze_basic() {
    let cli = parse(&["keel", "analyze", "src/main.rs"]);
    match cli.command {
        Commands::Analyze { file } => {
            assert_eq!(file, "src/main.rs");
        }
        _ => panic!("expected Analyze"),
    }
}

#[test]
fn parse_analyze_missing_file() {
    parse_err(&["keel", "analyze"]);
}

// --- Discover --context ---

#[test]
fn parse_discover_context_flag_no_value() {
    let cli = parse(&["keel", "discover", "abc123", "--context"]);
    match cli.command {
        Commands::Discover { context, .. } => {
            assert_eq!(context, Some(5)); // default_missing_value
        }
        _ => panic!("expected Discover"),
    }
}

#[test]
fn parse_discover_context_with_value() {
    let cli = parse(&["keel", "discover", "abc123", "--context", "20"]);
    match cli.command {
        Commands::Discover { context, .. } => {
            assert_eq!(context, Some(20));
        }
        _ => panic!("expected Discover"),
    }
}

// --- Compile --delta ---

#[test]
fn parse_compile_delta() {
    let cli = parse(&["keel", "compile", "--delta"]);
    match cli.command {
        Commands::Compile { delta, .. } => {
            assert!(delta);
        }
        _ => panic!("expected Compile"),
    }
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
