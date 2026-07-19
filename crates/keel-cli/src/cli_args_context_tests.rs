//! Argument-parsing tests for the context commands (`skeleton`, `focus`).
//!
//! Kept in a separate file from `cli_args_tests.rs` so that file stays under
//! the 800-line cap and additive changes merge cleanly.

use super::{Cli, Commands};
use clap::Parser;

fn parse(args: &[&str]) -> Cli {
    Cli::try_parse_from(args).expect("failed to parse CLI args")
}

fn parse_err(args: &[&str]) {
    Cli::try_parse_from(args).expect_err("expected parse failure");
}

#[test]
fn parse_skeleton_defaults() {
    match parse(&["keel", "skeleton", "src/a.ts"]).command {
        Commands::Skeleton {
            file,
            docs,
            private,
            budget,
        } => {
            assert_eq!(file, "src/a.ts");
            assert!(!docs);
            assert!(!private);
            assert!(budget.is_none());
        }
        _ => panic!("expected Skeleton"),
    }
}

#[test]
fn parse_skeleton_all_flags() {
    match parse(&[
        "keel",
        "skeleton",
        "src/a.ts",
        "--docs",
        "--private",
        "--budget",
        "800",
    ])
    .command
    {
        Commands::Skeleton {
            docs,
            private,
            budget,
            ..
        } => {
            assert!(docs);
            assert!(private);
            assert_eq!(budget, Some(800));
        }
        _ => panic!("expected Skeleton"),
    }
}

#[test]
fn parse_skeleton_missing_file() {
    parse_err(&["keel", "skeleton"]);
}

#[test]
fn parse_focus_defaults() {
    match parse(&["keel", "focus", "abc123"]).command {
        Commands::Focus {
            target,
            depth,
            budget,
        } => {
            assert_eq!(target, "abc123");
            assert_eq!(depth, 2); // default
            assert!(budget.is_none());
        }
        _ => panic!("expected Focus"),
    }
}

#[test]
fn parse_focus_with_depth_and_budget() {
    match parse(&[
        "keel", "focus", "src/a.ts", "--depth", "3", "--budget", "500",
    ])
    .command
    {
        Commands::Focus {
            target,
            depth,
            budget,
        } => {
            assert_eq!(target, "src/a.ts");
            assert_eq!(depth, 3);
            assert_eq!(budget, Some(500));
        }
        _ => panic!("expected Focus"),
    }
}

#[test]
fn parse_focus_missing_target() {
    parse_err(&["keel", "focus"]);
}
