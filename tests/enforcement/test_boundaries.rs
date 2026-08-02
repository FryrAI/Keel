//! Fixture-driven integration tests for W009 `new_cross_boundary_dep` and its
//! opt-in E006 `layer_violation` escalation, driven through the real `keel`
//! binary (init -> map -> edit -> compile --json).
//!
//! The fixture is an npm-workspaces monorepo, because W009 is deliberately
//! silent in a repo that declares no boundaries: `packages/core` and
//! `packages/harness` are two declared packages, and `core`'s `execute` is the
//! most-called public symbol, so it is the façade the fix_hint must name.

use std::fs;
use std::path::Path;

use serde_json::Value;
use tempfile::TempDir;

use crate::common::{assert_no_violation, compile_json, find_violation, keel, keel_bin};

/// `packages/core`: the `execute` façade plus the `rasterIngest` internal
/// nothing outside the package is supposed to reach for.
const CORE_SRC: &str = "/** Run a job through the package façade. */\n\
                        export function execute(job: string): string {\n\
                        \x20 return rasterIngest(job);\n\
                        }\n\
                        /** Ingest one raster job. */\n\
                        export function rasterIngest(job: string): string {\n\
                        \x20 return job + \"-ingested\";\n\
                        }\n";

/// Two in-package callers of `execute`, so it outranks `rasterIngest` as the
/// most-called public symbol and is the façade the fix_hint must name.
const CORE_API: &str = "import { execute } from \"./index\";\n\
                        /** Queue one job. */\n\
                        export function enqueue(job: string): string {\n\
                        \x20 return execute(job);\n\
                        }\n\
                        /** Replay one job. */\n\
                        export function replay(job: string): string {\n\
                        \x20 return execute(job);\n\
                        }\n";

/// `packages/harness/src/setup.ts`: gives the harness module a stored, purely
/// in-package call edge, which is what satisfies W009's module-level bootstrap
/// guard.
const HARNESS_SETUP: &str = "/** Prepare the harness. */\n\
                             export function prepare(): string {\n\
                             \x20 return \"ready\";\n\
                             }\n";

/// `packages/harness/src/run.ts` before the erosion: no dependency on `core`
/// at all, only on its own package.
const HARNESS_RUN: &str = "import { prepare } from \"./setup\";\n\
                           /** Run the harness. */\n\
                           export function run(): string {\n\
                           \x20 return prepare();\n\
                           }\n";

/// The erosion: reach straight into a `core` internal.
const HARNESS_RUN_ERODED: &str = "import { prepare } from \"./setup\";\n\
                                  import { rasterIngest } from \"core\";\n\
                                  /** Run the harness. */\n\
                                  export function run(): string {\n\
                                  \x20 return rasterIngest(prepare());\n\
                                  }\n";

/// Create + map an npm-workspaces project with `packages/core` and
/// `packages/harness`.
fn workspace_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    write(
        dir.path(),
        "package.json",
        "{\n  \"name\": \"root\",\n  \"private\": true,\n  \"workspaces\": [\"packages/*\"]\n}\n",
    );
    write(dir.path(), "packages/core/src/index.ts", CORE_SRC);
    write(dir.path(), "packages/core/src/api.ts", CORE_API);
    write(dir.path(), "packages/harness/src/setup.ts", HARNESS_SETUP);
    write(dir.path(), "packages/harness/src/run.ts", HARNESS_RUN);
    let out = keel(dir.path(), &["init"]);
    assert!(
        out.status.success(),
        "keel init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        fs::read_to_string(dir.path().join(".keel/keel.json"))
            .unwrap()
            .contains("\"enabled\": true"),
        "the fixture must be detected as a monorepo, or W009 stays silent by design"
    );
    keel(dir.path(), &["map"]);
    dir
}

fn write(root: &Path, rel: &str, content: &str) {
    let full = root.join(rel);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, content).unwrap();
}

/// Set `architecture.deny` in the project's keel.json.
fn set_deny(dir: &Path, pairs: &[(&str, &str)]) {
    let cfg_path = dir.join(".keel/keel.json");
    let raw = fs::read_to_string(&cfg_path).unwrap();
    let mut cfg: Value = serde_json::from_str(&raw).unwrap();
    let deny: Vec<Value> = pairs
        .iter()
        .map(|(a, b)| serde_json::json!([a, b]))
        .collect();
    cfg["architecture"] = serde_json::json!({ "deny": deny });
    fs::write(&cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
}

/// A tree nobody touched must produce no W009 and, since it is otherwise
/// clean, no output at all.
#[test]
fn test_w009_silent_on_an_unchanged_tree() {
    let dir = workspace_project();
    let out = keel(dir.path(), &["compile", "packages/harness/src/run.ts"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.trim().is_empty(),
        "clean compile must print nothing, got: {stdout}"
    );
    assert_eq!(out.status.code(), Some(0));

    let result = compile_json(dir.path(), "packages/harness/src/run.ts");
    assert_no_violation(&result, "W009");
}

/// Reaching past `core`'s façade into one of its internals is exactly one
/// W009, and the fix_hint names the façade.
#[test]
fn test_w009_fires_on_a_new_cross_package_dependency() {
    let dir = workspace_project();
    write(
        dir.path(),
        "packages/harness/src/run.ts",
        HARNESS_RUN_ERODED,
    );

    let result = compile_json(dir.path(), "packages/harness/src/run.ts");
    let v = find_violation(&result, "W009");
    assert_eq!(v["severity"], "WARNING");
    assert_eq!(v["category"], "new_cross_boundary_dep");
    assert_eq!(v["confidence"].as_f64().unwrap(), 0.9);
    let hint = v["fix_hint"].as_str().unwrap_or_default();
    assert!(
        hint.contains("execute"),
        "fix_hint must name core's most-called public symbol: {hint}"
    );
    let count = result["warnings"]
        .as_array()
        .map(|w| w.iter().filter(|v| v["code"] == "W009").count())
        .unwrap_or(0);
    assert_eq!(count, 1, "one warning per newly depended-on boundary");
}

/// W009 is a decision-moment signal, not a standing report: once the compile
/// that introduced the edge has synced it into the graph, the dependency is
/// part of the baseline and stops firing.
#[test]
fn test_w009_fires_at_the_decision_and_then_baselines() {
    let dir = workspace_project();
    write(
        dir.path(),
        "packages/harness/src/run.ts",
        HARNESS_RUN_ERODED,
    );

    let first = compile_json(dir.path(), "packages/harness/src/run.ts");
    find_violation(&first, "W009");

    let second = compile_json(dir.path(), "packages/harness/src/run.ts");
    assert_no_violation(&second, "W009");
}

/// The module-level (not file-level) bootstrap guard: a file that did not
/// exist at map time still fires, because gating on the file would exempt
/// every newly created file by definition.
#[test]
fn test_w009_fires_for_a_brand_new_file() {
    let dir = workspace_project();
    write(
        dir.path(),
        "packages/harness/src/fresh.ts",
        "import { rasterIngest } from \"core\";\n\
         /** A file that did not exist at map time. */\n\
         export function fresh(): string {\n  return rasterIngest(\"x\");\n}\n",
    );

    let result = compile_json(dir.path(), "packages/harness/src/fresh.ts");
    let v = find_violation(&result, "W009");
    assert_eq!(v["category"], "new_cross_boundary_dep");
}

/// A repo with no declared packages gets no guessed boundaries — the same
/// cross-directory edit stays silent.
#[test]
fn test_w009_silent_in_a_flat_repo() {
    let dir = TempDir::new().unwrap();
    write(dir.path(), "packages/core/src/index.ts", CORE_SRC);
    write(dir.path(), "packages/core/src/api.ts", CORE_API);
    write(dir.path(), "packages/harness/src/setup.ts", HARNESS_SETUP);
    write(dir.path(), "packages/harness/src/run.ts", HARNESS_RUN);
    keel(dir.path(), &["init"]);
    keel(dir.path(), &["map"]);
    write(
        dir.path(),
        "packages/harness/src/run.ts",
        HARNESS_RUN_ERODED,
    );

    let result = compile_json(dir.path(), "packages/harness/src/run.ts");
    assert_no_violation(&result, "W009");
}

/// An explicitly denied ordered pair escalates to E006 — an ERROR that gates
/// exit 1 and carries a fix_hint.
#[test]
fn test_e006_escalates_a_denied_pair() {
    let dir = workspace_project();
    set_deny(dir.path(), &[("harness", "core")]);
    write(
        dir.path(),
        "packages/harness/src/run.ts",
        HARNESS_RUN_ERODED,
    );

    let out = std::process::Command::new(keel_bin())
        .args(["compile", "packages/harness/src/run.ts", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "a layer violation must gate the compile"
    );
    let result: Value = serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).unwrap();
    let v = find_violation(&result, "E006");
    assert_eq!(v["severity"], "ERROR");
    assert_eq!(v["category"], "layer_violation");
    assert!(
        v["fix_hint"].as_str().is_some_and(|h| !h.is_empty()),
        "every ERROR carries a fix_hint"
    );
}
