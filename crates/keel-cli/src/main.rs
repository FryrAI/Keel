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
mod update_check;

use cli_args::{Cli, Commands};

fn main() {
    let cli = Cli::parse();

    // Extract depth values before creating formatter (needed for LLM depth-awareness)
    let (map_depth, compile_depth) = match &cli.command {
        Commands::Map { depth, .. } => (*depth, 1),
        Commands::Compile { depth, .. } => (1, *depth),
        _ => (1, 1),
    };

    // `--budget` on skeleton/focus drives LLM truncation for those commands.
    let budget = match &cli.command {
        Commands::Skeleton { budget, .. } => *budget,
        Commands::Focus { budget, .. } => *budget,
        _ => None,
    };

    // `--top` on audit caps how many ranked findings the LLM output prints.
    let audit_top = match &cli.command {
        Commands::Audit { top, .. } => Some(*top),
        _ => None,
    };

    let formatter: Box<dyn keel_output::OutputFormatter> = if cli.json {
        Box::new(keel_output::json::JsonFormatter)
    } else if cli.llm {
        Box::new(
            keel_output::llm::LlmFormatter::with_depths(map_depth, compile_depth)
                .with_max_tokens(cli.max_tokens)
                .with_budget(budget)
                .with_audit_top(audit_top),
        )
    } else {
        Box::new(keel_output::human::HumanFormatter)
    };

    let no_telemetry = cli.no_telemetry;

    // Show update notification if a newer version was found by a previous check.
    update_check::maybe_notify();

    let cmd_name = telemetry_recorder::command_name(&cli.command);
    let start = Instant::now();
    let client_name = cli
        .client
        .clone()
        .or_else(telemetry_recorder::detect_client);

    let (exit_code, metrics) = match cli.command {
        Commands::Init {
            merge,
            yes,
            update_docs,
        } => (
            commands::init::run(&*formatter, cli.verbose, merge, yes, update_docs),
            Default::default(),
        ),
        Commands::Map {
            depth,
            tier3,
            cached,
            semantic,
        } => commands::map::run(&*formatter, cli.verbose, depth, tier3, cached, semantic),
        Commands::Discover {
            query,
            depth,
            name,
            context,
        } => (
            commands::discover::run(&*formatter, cli.verbose, query, depth, name, context),
            Default::default(),
        ),
        Commands::Search { term, kind } => (
            commands::search::run(&*formatter, cli.verbose, cli.json, cli.llm, term, kind),
            Default::default(),
        ),
        Commands::Compile {
            files,
            batch_start,
            batch_end,
            strict,
            suppress,
            // `depth` is read above, before the formatter is built.
            depth: _,
            changed,
            since,
            delta,
            timeout,
            format,
        } => commands::compile::run(
            &*formatter,
            cli.verbose,
            commands::compile::CompileArgs {
                files,
                batch_start,
                batch_end,
                strict,
                suppress,
                changed,
                since,
                delta,
                timeout,
                github: format == Some(cli_args::WireFormat::Github),
            },
        ),
        Commands::Check { query, name } => (
            commands::check::run(&*formatter, cli.verbose, query, name),
            Default::default(),
        ),
        Commands::Where { hash } => (
            commands::where_cmd::run(&*formatter, cli.verbose, hash, cli.json),
            Default::default(),
        ),
        Commands::Explain {
            error_code,
            hash,
            tree,
            depth,
        } => (
            commands::explain::run(&*formatter, cli.verbose, error_code, hash, tree, depth),
            Default::default(),
        ),
        Commands::Fix {
            hashes,
            file,
            apply,
        } => (
            commands::fix::run(&*formatter, cli.verbose, hashes, file, apply),
            Default::default(),
        ),
        Commands::Name {
            description,
            module,
            kind,
        } => (
            commands::name::run(&*formatter, cli.verbose, description, module, kind),
            Default::default(),
        ),
        Commands::Analyze { file } => (
            commands::analyze::run(&*formatter, cli.verbose, file),
            Default::default(),
        ),
        Commands::Audit {
            changed,
            strict,
            min_score,
            dimension,
            strict_cycles,
            top: _,
        } => (
            commands::audit::run(
                &*formatter,
                cli.verbose,
                changed,
                strict,
                min_score,
                dimension,
                strict_cycles,
            ),
            Default::default(),
        ),
        Commands::Context { file } => (
            commands::context::run(&*formatter, cli.verbose, file, cli.json, cli.llm),
            Default::default(),
        ),
        Commands::Skeleton {
            file,
            docs,
            private,
            budget: _,
        } => (
            commands::skeleton::run(&*formatter, cli.verbose, file, docs, private),
            Default::default(),
        ),
        Commands::Focus {
            target,
            depth,
            budget: _,
        } => (
            commands::focus::run(&*formatter, cli.verbose, target, depth),
            Default::default(),
        ),
        Commands::Checkpoint {
            since,
            staged,
            output,
        } => (
            commands::checkpoint::run(&*formatter, cli.verbose, since, staged, output),
            Default::default(),
        ),
        Commands::Review { base, format, gate } => (
            commands::review::run(
                &*formatter,
                cli.verbose,
                base,
                format == Some(cli_args::WireFormat::Github),
                gate,
            ),
            Default::default(),
        ),
        Commands::Quality {
            snapshot,
            trend,
            since,
            last,
        } => (
            commands::quality::run(
                &*formatter,
                cli.verbose,
                commands::quality::QualityArgs {
                    snapshot,
                    trend,
                    since,
                    last,
                },
            ),
            Default::default(),
        ),
        Commands::ValidatePlan { plan } => (
            commands::validate_plan::run(&*formatter, cli.verbose, plan),
            Default::default(),
        ),
        Commands::Serve { mcp, http, watch } => (
            commands::serve::run(&*formatter, cli.verbose, mcp, http, watch, no_telemetry),
            Default::default(),
        ),
        Commands::Watch => (commands::watch::run(cli.verbose), Default::default()),
        Commands::Deinit => (
            commands::deinit::run(&*formatter, cli.verbose),
            Default::default(),
        ),
        Commands::Stats => (
            commands::stats::run(&*formatter, cli.verbose, cli.json, cli.llm),
            Default::default(),
        ),
        Commands::Config { key, value } => (
            commands::config::run(&*formatter, cli.verbose, key, value),
            Default::default(),
        ),
        Commands::Upgrade { version, yes } => {
            (commands::upgrade::run(version, yes), Default::default())
        }
        Commands::Completion { shell } => (commands::completion::run(&shell), Default::default()),
        Commands::Login => (commands::login::run(cli.verbose), Default::default()),
        Commands::Logout => (commands::logout::run(cli.verbose), Default::default()),
        Commands::Push { yes } => (
            commands::push::run(&*formatter, cli.verbose, yes),
            Default::default(),
        ),
    };

    // Check for updates in the background (at most once per 24h).
    update_check::maybe_check_async(no_telemetry);

    // Record telemetry and wait for remote send before exit.
    // The remote thread has a 2s timeout so the CLI never hangs.
    let telemetry_handle = if !no_telemetry {
        if let Ok(cwd) = std::env::current_dir() {
            let keel_dir = keel_core::paths::keel_dir(&cwd);
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
                )
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Wait for remote telemetry to finish before exiting — except on keel's
    // hot path (T1.1), which must never block on a network round trip.
    // `try_send_remote` already refuses hot-path commands structurally (no
    // handle to join in the first place), so this check is redundant in
    // practice; it stays as an explicit, hard-coded second guard so nothing
    // downstream can silently reintroduce a blocking join for one of them.
    // Without the join at all, process::exit would kill the send thread
    // mid-request for every other command.
    if let Some(handle) = telemetry_handle {
        if !telemetry_recorder::hot_path_commands().contains(&cmd_name) {
            let _ = handle.join();
        }
    }

    std::process::exit(exit_code);
}
