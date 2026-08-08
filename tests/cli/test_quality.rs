// Tests for `keel quality` — the time axis: a reading, a snapshot per commit,
// and a trend that refuses to lie.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use tempfile::TempDir;

use crate::common::{git, keel};

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

/// A committed, mapped repo with one oversized source file (900 lines, well
/// past the 400-line default budget) and one dead private function.
fn fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(
        root,
        "src/lib.ts",
        "export function execute(cmd: number): number {\n  return cmd;\n}\n",
    );
    write(
        root,
        "src/main.ts",
        "import { execute } from './lib';\nexport function run(): number {\n  return execute(1);\n}\n",
    );
    let filler = "// pad\n".repeat(900);
    write(
        root,
        "src/big.ts",
        &format!("{filler}export function big(): number {{\n  return 1;\n}}\n"),
    );

    git(root, &["init", "-q"]);
    assert!(keel(root, &["init"]).status.success(), "keel init failed");
    assert!(keel(root, &["map"]).status.success(), "keel map failed");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "base"]);
    dir
}

/// Commit an empty change so the repo gets a distinct HEAD, then re-map so the
/// snapshot is measured against a rebuilt graph (the case the series exists to
/// survive).
fn commit_and_remap(root: &Path, message: &str) {
    git(root, &["commit", "-q", "--allow-empty", "-m", message]);
    assert!(keel(root, &["map"]).status.success(), "keel map failed");
}

fn stdout_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn reading_prints_four_metrics_and_exits_zero() {
    let dir = fixture();
    let out = keel(dir.path(), &["quality", "--llm"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "quality never gates: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = stdout_of(&out);
    assert!(
        stdout.starts_with("QUALITY "),
        "unexpected output: {stdout}"
    );
    for metric in [
        "files_over_budget",
        "cycle_count",
        "dead_private_fns",
        "cross_module_edge_ratio",
    ] {
        assert!(stdout.contains(metric), "missing {metric}: {stdout}");
    }
    assert!(
        stdout.contains("files_over_budget=1"),
        "the 900-line source file should be over budget: {stdout}"
    );
}

/// End-to-end wiring for `clone_loc_ratio` (issue #66): `keel map` measures
/// fragment clones, the metric reads them back. A copied 8-line block sits in
/// two otherwise different functions — invisible to W006, which compares whole
/// bodies — so a reading of 0 here means the map never wrote the measurements.
#[test]
fn clone_loc_ratio_sees_a_block_copied_between_two_functions() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let shared = (0..8)
        .map(|i| format!("  const step{i} = rows[{i}].value * weight + offset;\n  total = total + step{i};\n"))
        .collect::<String>();
    write(
        root,
        "src/a.ts",
        &format!("export function alpha(rows: number[], weight: number, offset: number): number {{\n  let total = 0;\n{shared}  return total;\n}}\n"),
    );
    write(
        root,
        "src/b.ts",
        &format!("export function beta(rows: number[], weight: number, offset: number): number {{\n  let total = 100;\n  console.log('beta');\n{shared}  return total * 2;\n}}\n"),
    );
    git(root, &["init", "-q"]);
    assert!(keel(root, &["init"]).status.success(), "keel init failed");
    assert!(keel(root, &["map"]).status.success(), "keel map failed");

    let out = keel(root, &["quality", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    let ratio = json["metrics"]["clone_loc_ratio"].as_f64().expect("metric");
    assert!(
        ratio > 0.0,
        "a block copied between two functions must register: {ratio}"
    );
}

#[test]
fn reading_json_carries_the_versioned_metrics_blob() {
    let dir = fixture();
    let out = keel(dir.path(), &["quality", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(json["command"], "quality");
    assert_eq!(json["metrics"]["version"], 1);
    assert_eq!(json["snapshot_saved"], false);
    assert!(json["metrics"]["files_over_budget"].as_u64().unwrap() >= 1);
}

#[test]
fn snapshot_is_idempotent_per_commit() {
    let dir = fixture();
    let root = dir.path();

    for _ in 0..3 {
        let out = keel(root, &["quality", "--snapshot", "--llm"]);
        assert_eq!(out.status.code(), Some(0));
        assert!(stdout_of(&out).contains("snapshot=saved"));
    }

    // Three captures at one commit own one point, not three.
    let trend = keel(root, &["quality", "--trend", "--llm"]);
    assert_eq!(trend.status.code(), Some(0));
    let stdout = stdout_of(&trend);
    assert!(
        stdout.contains("QUALITY-TREND points=1"),
        "one commit must own exactly one snapshot: {stdout}"
    );
}

#[test]
fn trend_survives_re_maps_and_attributes_moves_to_commits() {
    let dir = fixture();
    let root = dir.path();

    // Snapshot 1: the fixture as committed.
    assert!(keel(root, &["quality", "--snapshot"]).status.success());

    // Snapshot 2: a second oversized file, at a new commit, after a full re-map
    // (`keel map` calls clear_all — the series must survive it).
    let filler = "// pad\n".repeat(900);
    write(
        root,
        "src/big2.ts",
        &format!("{filler}export function big2(): number {{\n  return 2;\n}}\n"),
    );
    git(root, &["add", "-A"]);
    commit_and_remap(root, "grow");
    assert!(keel(root, &["quality", "--snapshot"]).status.success());

    // Snapshot 3: no source change, new commit, another re-map.
    commit_and_remap(root, "noop");
    assert!(keel(root, &["quality", "--snapshot"]).status.success());

    let out = keel(root, &["quality", "--trend", "--llm"]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = stdout_of(&out);
    assert!(
        stdout.contains("QUALITY-TREND points=3"),
        "three maps must not erase the series: {stdout}"
    );
    let over = stdout
        .lines()
        .find(|l| l.starts_with("files_over_budget "))
        .unwrap_or_else(|| panic!("no files_over_budget line in: {stdout}"));
    assert!(
        over.contains("1 -> 2") && over.contains("worsening"),
        "growth should read as worsening: {over}"
    );
    assert!(
        over.contains("step=+1@"),
        "the move should be attributed to a commit: {over}"
    );
}

#[test]
fn trend_windows_narrow_the_series() {
    let dir = fixture();
    let root = dir.path();

    assert!(keel(root, &["quality", "--snapshot"]).status.success());
    commit_and_remap(root, "second");
    assert!(keel(root, &["quality", "--snapshot"]).status.success());
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .unwrap();
    let second_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();
    commit_and_remap(root, "third");
    assert!(keel(root, &["quality", "--snapshot"]).status.success());

    let last = keel(root, &["quality", "--trend", "--last", "2", "--llm"]);
    assert!(stdout_of(&last).contains("points=2"));

    // A short prefix is what a human types; the stored SHA is the full one.
    let since = keel(
        root,
        &["quality", "--trend", "--since", &second_sha[..7], "--llm"],
    );
    assert_eq!(since.status.code(), Some(0));
    assert!(
        stdout_of(&since).contains("points=2"),
        "--since is inclusive of its own snapshot: {}",
        stdout_of(&since)
    );

    // An unknown ref is a report about an empty window, not an error.
    let unknown = keel(
        root,
        &["quality", "--trend", "--since", "deadbeef", "--llm"],
    );
    assert_eq!(unknown.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("no snapshot at commit deadbeef"));
}

#[test]
fn trend_refuses_to_compare_across_metrics_versions() {
    let dir = fixture();
    let root = dir.path();
    assert!(keel(root, &["quality", "--snapshot"]).status.success());

    // Forge a snapshot written by a future keel whose metric definitions differ.
    let db = root.join(".keel/graph.db");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "INSERT INTO quality_snapshots (commit_sha, metrics) VALUES ('future', ?1)",
        [r#"{"version":99,"files_over_budget":0}"#],
    )
    .unwrap();
    drop(conn);

    let out = keel(root, &["quality", "--trend", "--llm"]);
    assert_eq!(out.status.code(), Some(0), "a refusal is still a report");
    let stdout = stdout_of(&out);
    assert!(
        stdout.contains("refused=version_mismatch"),
        "mixing definitions must be refused: {stdout}"
    );
    assert!(
        !stdout.contains("worsening") && !stdout.contains("improving"),
        "a refused comparison reports no direction: {stdout}"
    );
}

/// The documented budget is <100ms: a reading is a handful of graph queries
/// over an already-built database — no parse, no subprocess, no network.
///
/// The bound asserted here is deliberately far looser than that (process spawn,
/// a debug binary and a loaded CI box together eat most of a second on their
/// own), and it is not a measurement of the target. It is a tripwire for the
/// class of regression that turns a graph read into something else entirely —
/// a repo re-parse, a `git log` per snapshot, a network join — which costs
/// seconds, not milliseconds.
#[test]
fn a_reading_stays_far_inside_its_documented_budget() {
    let dir = fixture();
    let start = Instant::now();
    let out = keel(dir.path(), &["quality", "--llm"]);
    let elapsed = start.elapsed();
    assert_eq!(out.status.code(), Some(0));
    assert!(
        elapsed < Duration::from_secs(2),
        "keel quality took {elapsed:?}; the documented target is <100ms and this \
         2s ceiling only fires on a gross regression"
    );
}

#[test]
fn empty_series_explains_how_to_start_one() {
    let dir = fixture();
    let out = keel(dir.path(), &["quality", "--trend"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout_of(&out).contains("keel quality --snapshot"));
}

/// #64's acceptance criterion — two consecutive CI runs on different commits
/// must produce a `--trend` with >=2 points — hinges on the graph cache being
/// keyed per commit: CI's db for a new commit never saw the prior commit's
/// snapshot row (unlike a local repo re-mapping the SAME `.keel/graph.db`,
/// which already carries it — see `trend_survives_re_maps_and_attributes...`).
/// A deleted `graph.db` before the re-map stands in for that fresh-per-commit
/// cache; without `--import` this would be exactly the
/// `without_import_a_fresh_db_starts_its_own_series` case below.
#[test]
fn export_then_import_into_a_fresh_db_reproduces_the_trend() {
    let dir = fixture();
    let root = dir.path();
    assert!(keel(root, &["quality", "--snapshot"]).status.success());
    let history = root.join("history.jsonl");
    assert!(
        keel(root, &["quality", "--export", history.to_str().unwrap()])
            .status
            .success()
    );
    assert!(!fs::read_to_string(&history).unwrap().trim().is_empty());

    git(root, &["commit", "-q", "--allow-empty", "-m", "second"]);
    fs::remove_file(root.join(".keel/graph.db")).unwrap();
    assert!(keel(root, &["map"]).status.success(), "keel map failed");
    assert!(
        keel(root, &["quality", "--import", history.to_str().unwrap()])
            .status
            .success()
    );
    assert!(keel(root, &["quality", "--snapshot"]).status.success());

    let trend = keel(root, &["quality", "--trend", "--llm"]);
    assert!(
        stdout_of(&trend).contains("points=2"),
        "two consecutive runs on different commits must give >=2 trend points: {}",
        stdout_of(&trend)
    );
}

/// Control for the test above: `keel map`'s `clear_all` never touches
/// `quality_snapshots`, so a re-map alone (same db file) already carries prior
/// commits' rows — that's the "developer's local `.keel/graph.db`" case the
/// action.yml comments call out. The scenario the CLI surface actually needs
/// to solve is CI's *per-commit graph cache*: a genuinely fresh db that never
/// saw those rows, which a deleted `graph.db` stands in for here. Without the
/// import, that fresh db owns only its own snapshot.
#[test]
fn without_import_a_fresh_db_starts_its_own_series() {
    let dir = fixture();
    let root = dir.path();
    assert!(keel(root, &["quality", "--snapshot"]).status.success());
    let history = root.join("history.jsonl");
    assert!(
        keel(root, &["quality", "--export", history.to_str().unwrap()])
            .status
            .success()
    );

    git(root, &["commit", "-q", "--allow-empty", "-m", "second"]);
    fs::remove_file(root.join(".keel/graph.db")).unwrap();
    assert!(keel(root, &["map"]).status.success(), "keel map failed");
    assert!(keel(root, &["quality", "--snapshot"]).status.success());

    let trend = keel(root, &["quality", "--trend", "--llm"]);
    assert!(
        stdout_of(&trend).contains("points=1"),
        "a fresh db that never saw the history must not fabricate points: {}",
        stdout_of(&trend)
    );
}

/// Re-importing the same history twice must not stack duplicate points — the
/// `commit_sha` UNIQUE upsert makes import naturally idempotent.
#[test]
fn reimporting_the_same_history_does_not_duplicate_points() {
    let dir = fixture();
    let root = dir.path();
    assert!(keel(root, &["quality", "--snapshot"]).status.success());
    let history = root.join("history.jsonl");
    assert!(
        keel(root, &["quality", "--export", history.to_str().unwrap()])
            .status
            .success()
    );

    for _ in 0..2 {
        assert!(
            keel(root, &["quality", "--import", history.to_str().unwrap()])
                .status
                .success()
        );
    }

    let trend = keel(root, &["quality", "--trend", "--llm"]);
    assert!(
        stdout_of(&trend).contains("points=1"),
        "re-importing the same commit_sha twice must not duplicate the row"
    );
}
