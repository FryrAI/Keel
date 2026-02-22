//! keel CLI — structural code enforcement for LLM coding agents.
//!
//! This binary provides the `keel` command with subcommands for initialization,
//! mapping, compilation, discovery, and serving. See `keel --help` for usage.

use std::time::Instant;

use clap::Parser;

mod auth;
mod cli_args;
mod commands;
mod telemetry_recorder;

use cli_args::{Cli, Commands};

fn main() {
    let cli = Cli::parse();

    // Extract depth values before creating formatter (needed for LLM depth-awareness)
    let (map_depth, compile_depth) = match &cli.command {
        Commands::Map { depth, .. } => (*depth, 1),
        Commands::Compile { depth, .. } => (1, *depth),
        _ => (1, 1),
    };

    let formatter: Box<dyn keel_output::OutputFormatter> = if cli.json {
        Box::new(keel_output::json::JsonFormatter)
    } else if cli.llm {
        Box::new(
            keel_output::llm::LlmFormatter::with_depths(map_depth, compile_depth)
                .with_max_tokens(cli.max_tokens),
        )
    } else {
        Box::new(keel_output::human::HumanFormatter)
    };

    let cmd_name = telemetry_recorder::command_name(&cli.command);
    let start = Instant::now();
    let client_name = telemetry_recorder::detect_client();

    let (exit_code, metrics) = match cli.command {
        Commands::Init { merge, yes } => {
            (commands::init::run(&*formatter, cli.verbose, merge, yes), Default::default())
        }
        Commands::Map {
            llm_verbose,
            scope,
            strict,
            depth,
            tier3,
        } => commands::map::run(
            &*formatter,
            cli.verbose,
            llm_verbose,
            scope,
            strict,
            depth,
            tier3,
        ),
        Commands::Discover {
            query,
            depth,
            suggest_placement,
            name,
            context,
        } => (commands::discover::run(
            &*formatter,
            cli.verbose,
            query,
            depth,
            suggest_placement,
            name,
            context,
        ), Default::default()),
        Commands::Search { term, kind } => {
            (commands::search::run(&*formatter, cli.verbose, cli.json, cli.llm, term, kind), Default::default())
        }
        Commands::Compile {
            files,
            batch_start,
            batch_end,
            strict,
            tier3,
            suppress,
            depth,
            changed,
            since,
            delta,
            timeout,
        } => {
            // tier3 flag is accepted but not yet wired into compile
            let _ = tier3;
            commands::compile::run(
                &*formatter,
                cli.verbose,
                files,
                batch_start,
                batch_end,
                strict,
                suppress,
                depth,
                changed,
                since,
                delta,
                timeout,
            )
        }
        Commands::Check { query, name } => {
            (commands::check::run(&*formatter, cli.verbose, query, name), Default::default())
        }
        Commands::Where { hash } => {
            (commands::where_cmd::run(&*formatter, cli.verbose, hash, cli.json), Default::default())
        }
        Commands::Explain {
            error_code,
            hash,
            tree,
            depth,
        } => (commands::explain::run(&*formatter, cli.verbose, error_code, hash, tree, depth), Default::default()),
        Commands::Fix {
            hashes,
            file,
            apply,
        } => (commands::fix::run(&*formatter, cli.verbose, hashes, file, apply), Default::default()),
        Commands::Name {
            description,
            module,
            kind,
        } => (commands::name::run(&*formatter, cli.verbose, description, module, kind), Default::default()),
        Commands::Analyze { file } => (commands::analyze::run(&*formatter, cli.verbose, file), Default::default()),
        Commands::Context { file } => {
            (commands::context::run(&*formatter, cli.verbose, file, cli.json, cli.llm), Default::default())
        }
        Commands::Serve { mcp, http, watch } => {
            (commands::serve::run(&*formatter, cli.verbose, mcp, http, watch), Default::default())
        }
        Commands::Watch => (commands::watch::run(cli.verbose), Default::default()),
        Commands::Deinit => (commands::deinit::run(&*formatter, cli.verbose), Default::default()),
        Commands::Stats => (commands::stats::run(&*formatter, cli.verbose, cli.json), Default::default()),
        Commands::Config { key, value } => {
            (commands::config::run(&*formatter, cli.verbose, key, value), Default::default())
        }
        Commands::Upgrade { version, yes } => (commands::upgrade::run(version, yes), Default::default()),
        Commands::Completion { shell } => (commands::completion::run(&shell), Default::default()),
        Commands::Login => (commands::login::run(cli.verbose), Default::default()),
        Commands::Logout => (commands::logout::run(cli.verbose), Default::default()),
        Commands::Push { yes } => {
            (commands::push::run(&*formatter, cli.verbose, yes), Default::default())
        }
    };

    // Record telemetry (silently fails — never blocks CLI)
    if let Ok(cwd) = std::env::current_dir() {
        let keel_dir = cwd.join(".keel");
        if keel_dir.exists() {
            let config = keel_core::config::KeelConfig::load(&keel_dir);
            let mut metrics = metrics;
            metrics.client_name = client_name;
            telemetry_recorder::record_event(
                &keel_dir,
                &config,
                cmd_name,
                start.elapsed(),
                exit_code,
                metrics,
            );
        }
    }

    std::process::exit(exit_code);
}
