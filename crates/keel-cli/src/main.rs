use clap::Parser;

mod cli_args;
mod commands;

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

    let exit_code = match cli.command {
        Commands::Init { merge } => commands::init::run(&*formatter, cli.verbose, merge),
        Commands::Map { llm_verbose, scope, strict, depth } => {
            commands::map::run(&*formatter, cli.verbose, llm_verbose, scope, strict, depth)
        }
        Commands::Discover { query, depth, suggest_placement, name } => {
            commands::discover::run(
                &*formatter, cli.verbose, query, depth, suggest_placement, name,
            )
        }
        Commands::Search { term, kind } => {
            commands::search::run(&*formatter, cli.verbose, cli.json, cli.llm, term, kind)
        }
        Commands::Compile {
            files, batch_start, batch_end, strict, suppress, depth,
            changed, since,
        } => {
            commands::compile::run(
                &*formatter, cli.verbose, files, batch_start, batch_end, strict,
                suppress, depth, changed, since,
            )
        }
        Commands::Where { hash } => {
            commands::where_cmd::run(&*formatter, cli.verbose, hash, cli.json)
        }
        Commands::Explain { error_code, hash, tree, depth } => {
            commands::explain::run(&*formatter, cli.verbose, error_code, hash, tree, depth)
        }
        Commands::Fix { hashes, file, apply } => {
            commands::fix::run(&*formatter, cli.verbose, hashes, file, apply)
        }
        Commands::Name { description, module, kind } => {
            commands::name::run(&*formatter, cli.verbose, description, module, kind)
        }
        Commands::Serve { mcp, http, watch } => {
            commands::serve::run(&*formatter, cli.verbose, mcp, http, watch)
        }
        Commands::Watch => commands::watch::run(cli.verbose),
        Commands::Deinit => commands::deinit::run(&*formatter, cli.verbose),
        Commands::Stats => commands::stats::run(&*formatter, cli.verbose, cli.json),
    };

    std::process::exit(exit_code);
}
