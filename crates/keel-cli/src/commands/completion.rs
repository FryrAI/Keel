use clap::CommandFactory;
use clap_complete::{generate, Shell};

use crate::cli_args::Cli;

/// Run `keel completion <shell>` -- generate shell completion scripts for the given shell.
pub fn run(shell: &str) -> i32 {
    let shell = match shell.to_lowercase().as_str() {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "elvish" => Shell::Elvish,
        "powershell" | "ps" => Shell::PowerShell,
        _ => {
            eprintln!("error: unsupported shell '{shell}'");
            eprintln!("supported: bash, zsh, fish, elvish, powershell");
            return 2;
        }
    };

    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "keel", &mut std::io::stdout());
    0
}
