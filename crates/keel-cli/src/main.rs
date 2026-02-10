use clap::Parser;

mod cli_args;
mod commands;

use cli_args::{Cli, Commands};

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
            commands::where_cmd::run(&*formatter, cli.verbose, hash, cli.json)
        }
        Commands::Explain { error_code, hash, tree } => {
            commands::explain::run(&*formatter, cli.verbose, error_code, hash, tree)
        }
        Commands::Serve { mcp, http, watch } => {
            commands::serve::run(&*formatter, cli.verbose, mcp, http, watch)
        }
        Commands::Deinit => commands::deinit::run(&*formatter, cli.verbose),
        Commands::Stats => commands::stats::run(&*formatter, cli.verbose, cli.json),
    };

    std::process::exit(exit_code);
}
