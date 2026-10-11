//! A graph per linked git worktree (issue #107, item C; design v4 rules R1-R8).
//!
//! Acceptance tests T1-T7, T9 and the integration half of T10, written before the
//! feature. Every test drives the real `keel` binary on a synthetic repository:
//! `alpha(value)` in `alpha.py`, `beta()` in `beta.py` calling `alpha(1)`, a main
//! checkout, and a linked worktree on a branch one commit ahead. Layout variants
//! (T8) live in `graph_per_worktree_layouts.rs`.

#[path = "common/mod.rs"]
mod common;
#[path = "common/worktree_graph.rs"]
mod wg;

use std::fs;
use std::path::Path;
use std::process::Output;

use wg::*;

fn assert_refusal(o: &Output, what: &str) {
    assert_eq!(
        o.status.code(),
        Some(2),
        "{what}: expected exit 2\n{}",
        describe(o)
    );
    assert!(
        o.stdout.is_empty(),
        "{what}: the refusal must leave stdout empty\n{}",
        describe(o)
    );
    let stderr = err(o);
    assert!(
        stderr
            .lines()
            .next()
            .unwrap_or("")
            .starts_with(REFUSAL_PREFIX)
            && stderr.contains("keel map"),
        "{what}: first stderr line must start with the refusal prefix and name `keel map`\n{}",
        describe(o)
    );
}

fn main_state(p: &Pair) -> Snap {
    let s = snap(&graph_db(&p.main));
    assert_populated(&s, false);
    s
}

// ------------------------------------------------------------------- T1

/// T1, direction one: the violation is made and compiled in the main checkout.
#[test]
fn t1_violation_in_main_is_invisible_to_the_worktree() {
    let p = pair_mapped();
    assert_violation_isolated(&p.main, &p.main, "alpha.py", &p.wt);
}

/// T1, direction two: the violation is made and compiled in the worktree.
#[test]
fn t1_violation_in_worktree_is_invisible_to_main() {
    let p = pair_mapped();
    assert_violation_isolated(&p.wt, &p.wt, "alpha.py", &p.main);
}

// ------------------------------------------------------------------- T2

/// T2: mapping the worktree writes under the worktree's git directory and
/// leaves the main checkout's `.keel` file list, logical dump and markers alone.
#[test]
fn t2_map_in_worktree_builds_its_graph_under_the_worktree_git_dir() {
    let p = pair_new_worktree();
    // The dump first: a read-only connection to a WAL database leaves its
    // `-wal`/`-shm` files behind, which must not read as files the map added.
    let main_before = main_state(&p);
    let main_files = entries(&p.main.join(".keel"));
    keel_ok(&p.wt, &["map"]);
    let own = abs_git_dir(&p.wt).join("keel").join("graph.db");
    assert!(
        own.is_file(),
        "worktree graph.db is not under its git directory: {}",
        own.display()
    );
    let wt_dump = snap(&own);
    assert_populated(&wt_dump, true);
    assert_eq!(
        entries(&p.main.join(".keel")),
        main_files,
        "main's .keel file list changed"
    );
    assert_eq!(
        snap(&p.main.join(".keel/graph.db")),
        main_before,
        "main's graph changed"
    );
}

/// T2: the worktree's tree stays clean through `map`, `compile` and `compile --delta`.
/// GREEN-pin: graph state is gitignored today and must stay out of the tree.
#[test]
fn t2_worktree_status_is_clean_after_map_compile_and_delta() {
    let p = pair_new_worktree();
    let clean = |what: &str| {
        assert_eq!(
            git_out(&p.wt, &["status", "--porcelain"]),
            "",
            "worktree dirty after {what}"
        );
    };
    keel_ok(&p.wt, &["map"]);
    clean("map");
    keel_ok(&p.wt, &["compile", "alpha.py"]);
    clean("compile");
    keel_ok(&p.wt, &["compile", "--delta", "alpha.py"]);
    clean("compile --delta");
}

/// T2: the delta baseline is per worktree - it lands in the worktree's git
/// directory and never in the main checkout's `.keel`.
#[test]
fn t2_delta_baseline_is_per_worktree() {
    let p = pair_mapped();
    keel_ok(&p.wt, &["compile", "--delta", "alpha.py"]);
    let own = abs_git_dir(&p.wt).join("keel").join("last_compile.json");
    assert!(
        own.is_file(),
        "no delta baseline in the worktree's git directory: {}",
        own.display()
    );
    assert!(
        !p.main.join(".keel/last_compile.json").exists(),
        "the worktree's delta baseline was written into main's .keel"
    );
}

// ------------------------------------------------------------------- T3

/// T3: every compile mode in a never-mapped worktree refuses with the prefix
/// and names `keel map`, though the main checkout has a graph.
#[test]
fn t3_new_worktree_every_compile_mode_refuses() {
    let p = pair_new_worktree();
    let calls: [(&str, Vec<&str>); 3] = [
        ("explicit file", vec!["compile", "alpha.py"]),
        ("--changed", vec!["compile", "--changed"]),
        ("bare compile", vec!["compile"]),
    ];
    for (what, args) in calls {
        assert_refusal(&keel(&p.wt, &args), what);
    }
}

/// T3: `map --cached` in a never-mapped worktree refuses with its own prefix.
#[test]
fn t3_new_worktree_map_cached_refuses() {
    let p = pair_new_worktree();
    let o = keel(&p.wt, &["map", "--cached"]);
    assert_eq!(o.status.code(), Some(2), "{}", describe(&o));
    assert!(
        o.stdout.is_empty(),
        "must not print the main checkout's map\n{}",
        describe(&o)
    );
    assert!(
        err(&o).contains("no usable graph") && err(&o).contains("keel map"),
        "{}",
        describe(&o)
    );
}

/// T3: the refused compiles leave the main checkout's logical dump unchanged.
#[test]
fn t3_refused_worktree_compile_leaves_main_graph_unchanged() {
    let p = pair_new_worktree();
    let before = main_state(&p);
    let o = keel(&p.wt, &["compile", "alpha.py"]);
    assert_refusal(&o, "explicit file");
    assert_eq!(
        snap(&p.main.join(".keel/graph.db")),
        before,
        "main's graph changed"
    );
}

// ------------------------------------------------------------------- T4

/// A never-mapped main checkout: initialised and committed, never mapped.
fn never_mapped_oracle() -> tempfile::TempDir {
    let tmp = tempfile::TempDir::new().unwrap();
    seed_repo(tmp.path(), true);
    tmp
}

fn assert_reader_matches_never_mapped(args: &[&str]) {
    let oracle_dir = never_mapped_oracle();
    let oracle = keel(oracle_dir.path(), args);
    assert_eq!(
        oracle.status.code(),
        Some(2),
        "oracle must be a refusal\n{}",
        describe(&oracle)
    );
    let p = pair_new_worktree();
    let got = keel(&p.wt, args);
    let text = format!("{}{}", out(&got), err(&got)).to_lowercase();
    assert!(
        !text.contains("unable to open"),
        "database-open error text\n{}",
        describe(&got)
    );
    assert_eq!(
        got.status.code(),
        oracle.status.code(),
        "worktree vs never-mapped main\n{}",
        describe(&got)
    );
    assert_eq!(
        out(&got),
        out(&oracle),
        "stdout differs from a never-mapped main checkout"
    );
    assert_eq!(
        err(&got),
        err(&oracle),
        "stderr differs from a never-mapped main checkout"
    );
}

/// T4 (R4): `map --cached` as the very first command in a new worktree.
#[test]
fn t4_first_map_cached_in_new_worktree_matches_never_mapped_main() {
    assert_reader_matches_never_mapped(&["map", "--cached"]);
}

/// T4 (R4): `discover --name` as the very first command in a new worktree.
#[test]
fn t4_first_discover_in_new_worktree_matches_never_mapped_main() {
    assert_reader_matches_never_mapped(&["discover", "--name", "alpha"]);
}

// ------------------------------------------------------------------- T5

#[cfg(unix)] // unix: a FIFO, `mkfifo` and a shell git wrapper
mod concurrent {
    use super::*;
    use std::io::Write;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    const WAIT: Duration = Duration::from_secs(60);

    /// A `keel map` whose first `git` call blocks on a FIFO, so it holds the
    /// graph lock until `finish`. Dropping kills it, so a failed test never leaks it.
    pub struct HeldMap {
        child: Child,
        fifo: std::path::PathBuf,
        _bin: tempfile::TempDir,
    }

    impl HeldMap {
        pub fn start(dir: &Path) -> HeldMap {
            let bin = tempfile::TempDir::new().unwrap();
            let (fifo, done, reached) = (
                bin.path().join("fifo"),
                bin.path().join("done"),
                bin.path().join("reached"),
            );
            let path = std::env::var_os("PATH").unwrap_or_default();
            let real = std::env::split_paths(&path)
                .map(|d| d.join("git"))
                .find(|c| c.is_file())
                .expect("git on PATH");
            let wrapper = bin.path().join("git");
            common::write_executable(
                &wrapper,
                &format!(
                    "#!/bin/sh\nif [ ! -e '{d}' ]; then\n  : > '{d}'\n  : > '{r}'\n  read _hold < '{f}'\nfi\nexec '{g}' \"$@\"\n",
                    d = done.display(),
                    r = reached.display(),
                    f = fifo.display(),
                    g = real.display()
                ),
            );
            assert!(
                Command::new("mkfifo")
                    .arg(&fifo)
                    .status()
                    .unwrap()
                    .success(),
                "mkfifo"
            );
            let mut dirs = vec![bin.path().to_path_buf()];
            dirs.extend(std::env::split_paths(&path));
            let child = Command::new(common::keel_bin())
                .arg("map")
                .current_dir(dir)
                .env("PATH", std::env::join_paths(dirs).unwrap())
                .env("KEEL_NO_TELEMETRY", "true")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let mut held = HeldMap {
                child,
                fifo,
                _bin: bin,
            };
            let start = Instant::now();
            while !reached.exists() {
                assert!(
                    held.child.try_wait().unwrap().is_none(),
                    "the held map exited before its git call"
                );
                assert!(
                    start.elapsed() < WAIT,
                    "the held map never reached its git call"
                );
                std::thread::sleep(Duration::from_millis(25));
            }
            held
        }

        fn release(&self) {
            if let Ok(mut f) = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&self.fifo)
            {
                let _ = f.write_all(b"go\n");
            }
        }

        /// Release the hold; the map must then finish successfully.
        pub fn finish(mut self) {
            self.release();
            let start = Instant::now();
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    assert!(status.success(), "the held map failed: {status:?}");
                    return;
                }
                assert!(
                    start.elapsed() < WAIT,
                    "the held map did not finish after release"
                );
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }

    impl Drop for HeldMap {
        fn drop(&mut self) {
            self.release();
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// `keel <args>` with a bound: a hang fails the test instead of blocking the suite.
    pub fn keel_bounded(dir: &Path, args: &[&str]) -> Output {
        let mut child = Command::new(common::keel_bin())
            .args(args)
            .current_dir(dir)
            .env("KEEL_NO_TELEMETRY", "true")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let start = Instant::now();
        while child.try_wait().unwrap().is_none() {
            if start.elapsed() > Duration::from_secs(30) {
                let _ = child.kill();
                panic!(
                    "`keel {}` did not finish in 30s while the other worktree's map was held",
                    args.join(" ")
                );
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        child.wait_with_output().unwrap()
    }
}

/// T5: a compile in the main checkout completes and checks while the
/// worktree's map is held mid-run.
#[cfg(unix)] // unix: needs the unix-only HeldMap (FIFO and shell wrapper)
#[test]
fn t5_compile_in_main_does_not_wait_for_a_held_worktree_map() {
    let p = pair_new_worktree();
    let held = concurrent::HeldMap::start(&p.wt);
    write(&p.main, "alpha.py", ALPHA_BROKEN);
    let o = concurrent::keel_bounded(&p.main, &["compile", "alpha.py"]);
    assert_eq!(
        o.status.code(),
        Some(1),
        "the compile must run and check\n{}",
        describe(&o)
    );
    assert!(out(&o).contains("E001"), "{}", describe(&o));
    held.finish();
}

/// T5: a map in the main checkout completes while the worktree's map is held.
#[cfg(unix)] // unix: needs the unix-only HeldMap (FIFO and shell wrapper)
#[test]
fn t5_map_in_main_does_not_wait_for_a_held_worktree_map() {
    let p = pair_new_worktree();
    let held = concurrent::HeldMap::start(&p.wt);
    let o = concurrent::keel_bounded(&p.main, &["map"]);
    assert_eq!(o.status.code(), Some(0), "{}", describe(&o));
    held.finish();
}

// ------------------------------------------------------------------- T6

/// Turn off the rules that make `gadget.py` a violation, in main's config.
fn disable_hint_rules(main: &Path) {
    let file = main.join(".keel/keel.json");
    let text = fs::read_to_string(&file).unwrap();
    let edited = text.replace("\"type_hints\": true", "\"type_hints\": false");
    assert_ne!(edited, text, "fixture: keel.json has no type_hints rule");
    fs::write(file, edited).unwrap();
}

fn config_edit_in_main_governs_the_worktree(untrack: bool) {
    let p = pair(true, untrack);
    write(
        &p.wt,
        "gadget.py",
        "def gadget(x):\n    \"\"\"Gadget.\"\"\"\n    return x\n",
    );
    let before = keel(&p.wt, &["compile", "gadget.py"]);
    assert!(
        out(&before).contains("E002"),
        "precondition: the untyped function is a violation\n{}",
        describe(&before)
    );
    disable_hint_rules(&p.main);
    let after = keel(&p.wt, &["compile", "gadget.py"]);
    assert!(
        !out(&after).contains("E002"),
        "the worktree ignored main's edited keel.json\n{}",
        describe(&after)
    );
    assert_eq!(after.status.code(), Some(0), "{}", describe(&after));
}

/// T6: main's `keel.json` is tracked and dirty. GREEN-pin: the worktree keeps
/// reading the main checkout's config (R2).
#[test]
fn t6_tracked_dirty_main_config_governs_worktree_compile() {
    config_edit_in_main_governs_the_worktree(false);
}

/// T6: main's `keel.json` is untracked. GREEN-pin: same rule.
#[test]
fn t6_untracked_main_config_governs_worktree_compile() {
    config_edit_in_main_governs_the_worktree(true);
}

// ------------------------------------------------------------------- T7

/// T7 (R6): `keel deinit` in the worktree removes the worktree's `graph_dir`
/// and leaves the main checkout's `.keel` and graph intact.
#[test]
fn t7_deinit_in_worktree_spares_the_main_checkouts_keel_dir() {
    let p = pair_mapped();
    let main_before = snap(&graph_db(&p.main));
    let own_dir = abs_git_dir(&p.wt).join("keel");
    let had_own = own_dir.is_dir();
    let o = keel(&p.wt, &["deinit"]);
    assert_eq!(o.status.code(), Some(0), "{}", describe(&o));
    assert!(
        p.main.join(".keel").is_dir(),
        "deinit in the worktree deleted main's .keel"
    );
    assert!(
        p.main.join(".keel/keel.json").is_file(),
        "main's keel.json is gone"
    );
    assert_eq!(
        snap(&p.main.join(".keel/graph.db")),
        main_before,
        "main's graph changed"
    );
    assert_populated(&main_before, false);
    assert!(
        had_own,
        "precondition: the worktree had a graph_dir at {}",
        own_dir.display()
    );
    assert!(
        !own_dir.exists(),
        "the worktree's graph_dir survived deinit"
    );
}
