//! CLI-parse tests for the session/planning commands: `map --semantic`,
//! `checkpoint`, and `validate-plan`. Split from `cli_args_tests.rs` to keep
//! each test file under the size cap.

use super::*;
use clap::Parser;

fn parse(args: &[&str]) -> Cli {
    Cli::try_parse_from(args).expect("failed to parse CLI args")
}

#[test]
fn parse_map_semantic() {
    match parse(&["keel", "map", "--semantic"]).command {
        Commands::Map { semantic, .. } => assert!(semantic),
        _ => panic!("expected Map"),
    }
}

#[test]
fn parse_checkpoint_defaults() {
    match parse(&["keel", "checkpoint"]).command {
        Commands::Checkpoint {
            since,
            staged,
            output,
        } => assert!(since.is_none() && !staged && output.is_none()),
        _ => panic!("expected Checkpoint"),
    }
}

#[test]
fn parse_checkpoint_since_staged_output() {
    match parse(&[
        "keel",
        "checkpoint",
        "--since",
        "main",
        "--staged",
        "-o",
        "cp.md",
    ])
    .command
    {
        Commands::Checkpoint {
            since,
            staged,
            output,
        } => {
            assert_eq!(since.as_deref(), Some("main"));
            assert!(staged);
            assert_eq!(output.as_deref(), Some("cp.md"));
        }
        _ => panic!("expected Checkpoint"),
    }
}

#[test]
fn parse_validate_plan_file() {
    match parse(&["keel", "validate-plan", "plan.md"]).command {
        Commands::ValidatePlan { plan, strict } => {
            assert_eq!(plan, "plan.md");
            assert!(
                !strict,
                "strict must default off — the never-fails contract"
            );
        }
        _ => panic!("expected ValidatePlan"),
    }
}

#[test]
fn parse_validate_plan_stdin() {
    match parse(&["keel", "validate-plan", "-"]).command {
        Commands::ValidatePlan { plan, .. } => assert_eq!(plan, "-"),
        _ => panic!("expected ValidatePlan"),
    }
}

#[test]
fn parse_validate_plan_strict() {
    match parse(&["keel", "validate-plan", "-", "--strict"]).command {
        Commands::ValidatePlan { plan, strict } => {
            assert_eq!(plan, "-");
            assert!(strict);
        }
        _ => panic!("expected ValidatePlan"),
    }
}

#[test]
fn parse_validate_plan_missing_arg() {
    Cli::try_parse_from(["keel", "validate-plan"]).expect_err("expected parse failure");
}

#[test]
fn parse_review_base() {
    match parse(&["keel", "review", "--base", "origin/main"]).command {
        Commands::Review {
            base,
            format,
            gate,
            annotations_file,
        } => {
            assert_eq!(base, "origin/main");
            assert!(format.is_none(), "no CI protocol unless asked for");
            assert!(!gate, "review never gates by default");
            assert!(annotations_file.is_none());
        }
        _ => panic!("expected Review"),
    }
}

#[test]
fn parse_review_github_format_and_gate() {
    match parse(&[
        "keel", "review", "--base", "main", "--format", "github", "--gate",
    ])
    .command
    {
        Commands::Review { format, gate, .. } => {
            assert_eq!(format, Some(crate::cli_args::WireFormat::Github));
            assert!(gate);
        }
        _ => panic!("expected Review"),
    }
}

#[test]
fn parse_review_annotations_file() {
    match parse(&[
        "keel",
        "review",
        "--base",
        "main",
        "--annotations-file",
        "/tmp/annotations.txt",
    ])
    .command
    {
        Commands::Review {
            annotations_file, ..
        } => {
            assert_eq!(annotations_file.as_deref(), Some("/tmp/annotations.txt"));
        }
        _ => panic!("expected Review"),
    }
}

#[test]
fn parse_review_rejects_an_unknown_format() {
    Cli::try_parse_from(["keel", "review", "--base", "main", "--format", "gitlab"])
        .expect_err("only formats keel implements may parse");
}

#[test]
fn parse_review_requires_base() {
    Cli::try_parse_from(["keel", "review"]).expect_err("expected parse failure");
}

#[test]
fn parse_quality_defaults_to_the_current_reading() {
    match parse(&["keel", "quality"]).command {
        Commands::Quality {
            snapshot,
            trend,
            since,
            last,
        } => {
            assert!(!snapshot, "reading the graph never writes by default");
            assert!(!trend);
            assert!(since.is_none() && last.is_none());
        }
        _ => panic!("expected Quality"),
    }
}

#[test]
fn parse_quality_snapshot_and_trend_windows() {
    match parse(&["keel", "quality", "--snapshot"]).command {
        Commands::Quality { snapshot, .. } => assert!(snapshot),
        _ => panic!("expected Quality"),
    }
    match parse(&["keel", "quality", "--trend", "--last", "20"]).command {
        Commands::Quality { trend, last, .. } => {
            assert!(trend);
            assert_eq!(last, Some(20));
        }
        _ => panic!("expected Quality"),
    }
    match parse(&["keel", "quality", "--trend", "--since", "a1b2c3d"]).command {
        Commands::Quality { trend, since, .. } => {
            assert!(trend);
            assert_eq!(since.as_deref(), Some("a1b2c3d"));
        }
        _ => panic!("expected Quality"),
    }
}

#[test]
fn parse_quality_rejects_contradictory_flags() {
    // Capturing a reading and reporting the series are two different runs.
    Cli::try_parse_from(["keel", "quality", "--snapshot", "--trend"])
        .expect_err("--snapshot and --trend are mutually exclusive");
    // A window needs a series to window.
    Cli::try_parse_from(["keel", "quality", "--last", "5"]).expect_err("--last requires --trend");
    Cli::try_parse_from(["keel", "quality", "--since", "abc"])
        .expect_err("--since requires --trend");
    // Two windows are one too many.
    Cli::try_parse_from([
        "keel", "quality", "--trend", "--since", "abc", "--last", "5",
    ])
    .expect_err("--since and --last are mutually exclusive");
}
