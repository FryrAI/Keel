//! `KEEL_NO_TELEMETRY` is the documented opt-out. No value of it may make a
//! keel command fail: a hook that runs `keel` with the variable set would
//! otherwise exit 2 on every call, which agent tools treat as non-blocking.
//! Any value except an empty or a false-like one opts out.

#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::process::Command;

use common::keel_bin;

/// Runs `keel completion bash` (no graph, no network needed) in a directory
/// that holds an empty `.keel/`, and reports whether the run recorded a
/// telemetry event there. Asserts the command itself succeeded.
#[track_caller]
fn records_telemetry(value: Option<&str>) -> bool {
    let dir = tempfile::tempdir().unwrap();
    let keel_dir = dir.path().join(".keel");
    fs::create_dir(&keel_dir).unwrap();
    let mut command = Command::new(keel_bin());
    command.args(["completion", "bash"]).current_dir(dir.path());
    match value {
        Some(value) => command.env("KEEL_NO_TELEMETRY", value),
        None => command.env_remove("KEEL_NO_TELEMETRY"),
    };
    let out = command.output().expect("keel runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "KEEL_NO_TELEMETRY={value:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.stdout.is_empty(),
        "KEEL_NO_TELEMETRY={value:?} printed no completion script"
    );
    keel_dir.join("telemetry.db").exists()
}

#[test]
fn the_documented_value_opts_out() {
    assert!(!records_telemetry(Some("1")));
}

#[test]
fn any_other_value_that_is_not_false_like_opts_out() {
    for value in ["true", "yes", "on", "TRUE", "anything"] {
        assert!(!records_telemetry(Some(value)), "{value:?} must opt out");
    }
}

#[test]
fn a_false_like_value_or_no_variable_keeps_telemetry_on() {
    assert!(records_telemetry(None), "control: unset records an event");
    for value in ["0", "false", "off"] {
        assert!(records_telemetry(Some(value)), "{value:?} must not opt out");
    }
}

#[test]
fn an_empty_value_is_not_an_opt_out() {
    assert!(records_telemetry(Some("")));
}
