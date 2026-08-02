//! Argument-parsing tests for parse errors, the global `--no-telemetry`
//! flag, the cloud commands (`login`/`logout`/`push`), and `audit`.
//!
//! Kept in a separate file from `cli_args_tests.rs` so that file stays under
//! the 800-line cap and additive changes merge cleanly.

use super::{Cli, Commands};
use clap::Parser;

fn parse(args: &[&str]) -> Cli {
    Cli::try_parse_from(args).expect("failed to parse CLI args")
}

fn parse_err(args: &[&str]) -> clap::error::Error {
    Cli::try_parse_from(args).expect_err("expected parse failure")
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

// --- --no-telemetry flag ---

#[test]
fn no_telemetry_default_is_false() {
    let cli = parse(&["keel", "compile"]);
    assert!(!cli.no_telemetry);
}

#[test]
fn no_telemetry_flag_before_subcommand() {
    let cli = parse(&["keel", "--no-telemetry", "compile"]);
    assert!(cli.no_telemetry);
}

#[test]
fn no_telemetry_flag_after_subcommand() {
    let cli = parse(&["keel", "compile", "--no-telemetry"]);
    assert!(cli.no_telemetry);
}

// --- Login / Logout / Push ---

#[test]
fn parse_login() {
    let cli = parse(&["keel", "login"]);
    assert!(matches!(cli.command, Commands::Login));
}

#[test]
fn parse_logout() {
    let cli = parse(&["keel", "logout"]);
    assert!(matches!(cli.command, Commands::Logout));
}

#[test]
fn parse_push_defaults() {
    let cli = parse(&["keel", "push"]);
    assert!(matches!(cli.command, Commands::Push { yes: false }));
}

#[test]
fn parse_push_yes_long() {
    let cli = parse(&["keel", "push", "--yes"]);
    assert!(matches!(cli.command, Commands::Push { yes: true }));
}

#[test]
fn parse_push_yes_short() {
    let cli = parse(&["keel", "push", "-y"]);
    assert!(matches!(cli.command, Commands::Push { yes: true }));
}

// --- Audit ---

#[test]
fn parse_audit_defaults() {
    let cli = parse(&["keel", "audit"]);
    assert!(matches!(
        cli.command,
        Commands::Audit {
            changed: false,
            strict: false,
            min_score: None,
            dimension: None,
            strict_cycles: false,
            top: 20,
        }
    ));
}

#[test]
fn parse_audit_strict_cycles() {
    let cli = parse(&["keel", "audit", "--strict-cycles"]);
    assert!(matches!(
        cli.command,
        Commands::Audit {
            strict_cycles: true,
            ..
        }
    ));
}

#[test]
fn parse_audit_top() {
    let cli = parse(&["keel", "audit", "--top", "0"]);
    assert!(matches!(cli.command, Commands::Audit { top: 0, .. }));
}
