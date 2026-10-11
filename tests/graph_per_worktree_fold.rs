//! Pins for the fold of the worktree-graph review (design v4, Amendment 1).
//!
//! B3/B4/B5/B7 are rows the review showed to be broken (RED when written); P1-P8
//! pin call sites that were right but unpinned: every pin is a POSITIVE
//! difference between the worktree's graph and the main checkout's (a node only
//! one of them has), so reading the wrong graph changes the output. The helper
//! rows B1-B3 are unit tests in `crates/keel-core/src/config_tests.rs`.

#[path = "common/mod.rs"]
mod common;
#[path = "common/worktree_graph.rs"]
mod wg;

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use serde_json::json;
use wg::*;

/// File names, sizes and modification times directly inside `dir`.
fn tree(dir: &Path) -> Vec<(String, u64, std::time::SystemTime)> {
    let mut all: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            let m = e.metadata().unwrap();
            (
                e.file_name().to_string_lossy().to_string(),
                m.len(),
                m.modified().unwrap(),
            )
        })
        .collect();
    all.sort();
    all
}

fn own_graph_dir(p: &Pair) -> std::path::PathBuf {
    abs_git_dir(&p.wt).join("keel")
}

// ------------------------------------------------------------------- B2, B3

/// B2: a config write keeps the file's mode (0600 stays 0600).
#[cfg(unix)] // unix: mode bits (PermissionsExt)
#[test]
fn b2_config_write_keeps_the_destination_mode() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = initialised_repo();
    let file = tmp.path().join(".keel/keel.json");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    keel_ok(tmp.path(), &["config", "enforce.max_file_lines", "500"]);
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(
        fs::read_to_string(&file).unwrap().contains("500"),
        "the write did not land"
    );
}

/// B2: a read-only `keel.json` is refused, as an in-place write was: non-zero
/// exit, content and mode kept, no temporary file left.
#[cfg(unix)] // unix: mode bits (PermissionsExt)
#[test]
fn b2_config_write_refuses_a_read_only_destination() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = initialised_repo();
    let (file, dir) = (tmp.path().join(".keel/keel.json"), tmp.path().join(".keel"));
    fs::set_permissions(&file, fs::Permissions::from_mode(0o444)).unwrap();
    let (content, names) = (fs::read_to_string(&file).unwrap(), entries(&dir));
    let o = keel(tmp.path(), &["config", "enforce.max_file_lines", "500"]);
    assert_ne!(
        o.status.code(),
        Some(0),
        "a read-only keel.json was replaced\n{}",
        describe(&o)
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        content,
        "content changed"
    );
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o444
    );
    assert_eq!(entries(&dir), names, "a temporary file was left behind");
}

/// E4: a `keel.json` its owner cannot write (0464: another class has the write
/// bit) is refused as an in-place write was: exit 2, bytes unchanged.
#[cfg(unix)] // unix: mode bits (PermissionsExt); uid check
#[test]
fn e4_config_write_refuses_a_file_its_owner_cannot_write() {
    use std::os::unix::fs::PermissionsExt;
    if running_as_root() {
        return;
    }
    let tmp = initialised_repo();
    let file = tmp.path().join(".keel/keel.json");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o464)).unwrap();
    let bytes = fs::read(&file).unwrap();
    let o = keel(tmp.path(), &["config", "enforce.max_file_lines", "500"]);
    assert_eq!(o.status.code(), Some(2), "{}", describe(&o));
    assert_eq!(fs::read(&file).unwrap(), bytes, "keel.json changed");
}

/// E4: the refusal line carries the OS error, as an in-place write's did.
#[cfg(unix)] // unix: mode bits (PermissionsExt); uid check
#[test]
fn e4_config_write_refusal_names_the_os_error() {
    use std::os::unix::fs::PermissionsExt;
    if running_as_root() {
        return;
    }
    let tmp = initialised_repo();
    let file = tmp.path().join(".keel/keel.json");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o444)).unwrap();
    let o = keel(tmp.path(), &["config", "enforce.max_file_lines", "500"]);
    assert!(
        err(&o).contains("Permission denied (os error 13)"),
        "{}",
        describe(&o)
    );
}

/// B3: `keel.json` as a symlink to `widget/keel.json`: after a config write the
/// link is still a link and its target holds the new content.
#[cfg(unix)] // unix: symlink API (std::os::unix::fs::symlink)
#[test]
fn b3_config_write_through_a_symlink_keeps_the_link() {
    let tmp = initialised_repo();
    let dir = tmp.path().join(".keel");
    fs::create_dir(dir.join("widget")).unwrap();
    fs::rename(dir.join("keel.json"), dir.join("widget/keel.json")).unwrap();
    std::os::unix::fs::symlink("widget/keel.json", dir.join("keel.json")).unwrap();
    keel_ok(tmp.path(), &["config", "enforce.max_file_lines", "500"]);
    let meta = fs::symlink_metadata(dir.join("keel.json")).unwrap();
    assert!(
        meta.file_type().is_symlink(),
        "the link was replaced by a regular file"
    );
    assert_eq!(
        fs::read_link(dir.join("keel.json")).unwrap(),
        Path::new("widget/keel.json")
    );
    assert!(
        fs::read_to_string(dir.join("widget/keel.json"))
            .unwrap()
            .contains("500"),
        "target not written"
    );
    assert_eq!(
        entries(&dir.join("widget")),
        ["keel.json"].map(String::from).into(),
        "temp file in the target dir"
    );
}

// ------------------------------------------------------------------- B4

/// B4: `keel deinit` in main, then in the worktree: the worktree's graph
/// directory is gone and the second deinit exits 0.
#[test]
fn b4_deinit_in_worktree_after_main_removes_its_graph_dir() {
    let p = pair_mapped();
    let own = own_graph_dir(&p);
    assert!(
        own.is_dir(),
        "precondition: the worktree has a graph_dir at {}",
        own.display()
    );
    let first = keel(&p.main, &["deinit"]);
    assert_eq!(first.status.code(), Some(0), "{}", describe(&first));
    assert!(
        !p.main.join(".keel").exists(),
        "precondition: main's .keel is gone"
    );
    let second = keel(&p.wt, &["deinit"]);
    assert_eq!(second.status.code(), Some(0), "{}", describe(&second));
    assert!(
        !own.exists(),
        "the worktree's graph_dir survived: {}\n{}",
        own.display(),
        describe(&second)
    );
}

// ------------------------------------------------------------------- B5

/// B5: after main's deinit, `keel name` in the worktree refuses exactly as
/// `keel compile` does (exit 2, the not-initialised line) and leaves the
/// worktree's graph directory as it was.
#[test]
fn b5_name_refuses_when_the_shared_keel_dir_is_gone() {
    let p = pair_mapped();
    keel_ok(&p.main, &["deinit"]);
    let own = own_graph_dir(&p);
    assert!(own.is_dir(), "precondition: the worktree has a graph_dir");
    let before = tree(&own);
    let compile = keel(&p.wt, &["compile", "alpha.py"]);
    assert_eq!(
        compile.status.code(),
        Some(2),
        "reference: {}",
        describe(&compile)
    );
    let name = keel(&p.wt, &["name", "widget"]);
    assert_eq!(name.status.code(), Some(2), "{}", describe(&name));
    assert!(out(&name).is_empty(), "{}", describe(&name));
    assert!(
        err(&name).contains("not initialized"),
        "{}",
        describe(&name)
    );
    assert_eq!(
        err(&name).replace("keel name", "keel compile"),
        err(&compile),
        "the refusal line differs from compile's"
    );
    assert_eq!(
        tree(&own),
        before,
        "keel name touched the worktree's graph directory"
    );
}

/// B5 (R8): the refusal comes before any graph file is opened - an unreadable
/// `graph.db` must not turn it into an open error.
#[cfg(unix)] // unix: graph.db made unreadable with chmod; uid check
#[test]
fn b5_name_opens_no_graph_file_when_uninitialised() {
    use std::os::unix::fs::PermissionsExt;
    if running_as_root() {
        return;
    }
    let p = pair_mapped();
    keel_ok(&p.main, &["deinit"]);
    let db = own_graph_dir(&p).join("graph.db");
    fs::set_permissions(&db, fs::Permissions::from_mode(0o000)).unwrap();
    let name = keel(&p.wt, &["name", "widget"]);
    fs::set_permissions(&db, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(name.status.code(), Some(2), "{}", describe(&name));
    assert!(
        err(&name).contains("not initialized"),
        "opened the graph first\n{}",
        describe(&name)
    );
}

// ------------------------------------------------------------------- B7

/// B7: `keel init --yes` in a linked worktree whose git directory cannot take a
/// new directory stops with R4's one line before anything is written - the main
/// checkout (not initialised) gets no `.keel` and no ignore-file edit.
#[cfg(unix)] // unix: git directory made read-only with chmod; uid check
#[test]
fn b7_init_in_a_worktree_with_uncreatable_graph_dir_writes_nothing() {
    if running_as_root() {
        return;
    }
    let tmp = tempfile::TempDir::new().unwrap();
    let (main, wt) = (tmp.path().join("main"), tmp.path().join("wt"));
    seed_repo(&main, false);
    common::git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            wt.to_str().unwrap(),
        ],
    );
    let git_dir = abs_git_dir(&wt);
    let _locked = Locked::new(&git_dir);
    let o = keel(&wt, &["init", "--yes"]);
    assert_one_line_naming(&o, &git_dir.join("keel"));
    assert!(
        !main.join(".keel").exists(),
        "main gained a .keel directory"
    );
    assert_eq!(
        git_out(&main, &["status", "--porcelain"]),
        "",
        "main changed"
    );
    assert!(
        !wt.join(".keel").exists() && !wt.join(".keelignore").exists(),
        "the worktree was written to"
    );
    assert_eq!(
        git_out(&wt, &["status", "--porcelain"]),
        "",
        "worktree changed"
    );
}

// ------------------------------------------------------------------- P1

fn main_dump(p: &Pair) -> Snap {
    snap(&p.main.join(".keel/graph.db"))
}

/// Shared P1 setup: `gamma` (with a caller) lives only in the worktree's graph,
/// `delta_only` only in main's.
fn server_fixture() -> (Pair, String, String) {
    let p = pair_mapped();
    main_only_node(&p);
    write(
        &p.wt,
        "gamma_user.py",
        "from gamma import gamma\n\n\ndef gamma_user() -> int:\n    \"\"\"Call gamma.\"\"\"\n    return gamma()\n",
    );
    commit_all(&p.wt, "add gamma_user");
    keel_ok(&p.wt, &["map"]);
    let gamma = hash_of(&p.wt, "gamma");
    let delta_only = hash_of(&p.main, "delta_only");
    (p, gamma, delta_only)
}

/// `gamma` gains a required parameter: E001 only where `gamma_user` is in the graph.
fn break_gamma(wt: &Path) {
    write(
        wt,
        "gamma.py",
        "def gamma(x: int) -> int:\n    \"\"\"Gamma.\"\"\"\n    return x\n",
    );
}

/// P1: the real `keel serve --mcp` in a worktree answers from that worktree's
/// graph and takes that worktree's lock (main's is held throughout).
#[test]
fn p1_mcp_server_in_a_worktree_uses_the_worktree_graph() {
    let (p, gamma, delta_only) = server_fixture();
    let main_before = main_dump(&p);
    let _main_lock = keel_core::graph_lock::try_acquire(&p.main.join(".keel")).unwrap();
    let mut mcp = Mcp::start(&p.wt);
    let (failed, text) = mcp.call("keel/discover", json!({"hash": gamma}));
    assert!(
        !failed && text.contains("gamma"),
        "worktree-only node not found: {text}"
    );
    let (failed, text) = mcp.call("keel/discover", json!({"hash": delta_only}));
    assert!(
        failed,
        "main-only node found through the worktree's server: {text}"
    );
    break_gamma(&p.wt);
    let (failed, text) = mcp.call("keel/compile", json!({"files": ["gamma.py"]}));
    assert!(!failed && text.contains("E001"), "{text}");
    assert_eq!(
        main_dump(&p),
        main_before,
        "the server wrote into main's graph"
    );
}

/// The HTTP server listens on the fixed port 4815, so its tests run one at a time.
static PORT: Mutex<()> = Mutex::new(());

/// P1: `keel serve --http` in a worktree, same difference.
#[test]
fn p1_http_server_in_a_worktree_uses_the_worktree_graph() {
    let _port = PORT.lock().unwrap_or_else(|e| e.into_inner());
    let (p, gamma, delta_only) = server_fixture();
    let main_before = main_dump(&p);
    let _main_lock = keel_core::graph_lock::try_acquire(&p.main.join(".keel")).unwrap();
    let mut server = Server::start(&p.wt, &["serve", "--http", "--no-telemetry"]);
    let start = std::time::Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", 4815)).is_err() {
        assert!(
            server.child.try_wait().unwrap().is_none(),
            "serve --http exited (port 4815 busy?)"
        );
        assert!(start.elapsed() < WAIT, "serve --http never listened");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let (status, body) = http("GET", &format!("/discover/{gamma}"), "");
    assert!(
        status == 200 && body.contains("gamma"),
        "worktree-only node: {status} {body}"
    );
    let (status, _) = http("GET", &format!("/discover/{delta_only}"), "");
    assert_eq!(
        status, 404,
        "main-only node found through the worktree's server"
    );
    break_gamma(&p.wt);
    let (status, body) = http("POST", "/compile", r#"{"files":["gamma.py"]}"#);
    assert!(status == 200 && body.contains("E001"), "{status} {body}");
    assert_eq!(
        main_dump(&p),
        main_before,
        "the server wrote into main's graph"
    );
}

/// P1: `keel watch` in a worktree recompiles a changed file while main's graph
/// lock is held (the watcher takes the worktree's lock), and writes nothing to main.
#[test]
fn p1_watch_in_a_worktree_takes_the_worktrees_lock() {
    let p = pair_mapped();
    let main_before = main_dump(&p);
    let _main_lock = keel_core::graph_lock::try_acquire(&p.main.join(".keel")).unwrap();
    let mut watch = Server::start(&p.wt, &["watch", "--verbose"]);
    watch.wait_for_stderr("Watching for changes");
    let mut recompiled = false;
    for round in 0..6 {
        write(
            &p.wt,
            "omega.py",
            &format!("def omega() -> int:\n    \"\"\"Omega {round}.\"\"\"\n    return 9\n"),
        );
        if watch.saw_stderr("recompiled", std::time::Duration::from_secs(3)) {
            recompiled = true;
            break;
        }
    }
    assert!(
        recompiled,
        "the watcher never recompiled while main's lock was held"
    );
    assert_eq!(
        main_dump(&p),
        main_before,
        "the watcher wrote into main's graph"
    );
}

// ------------------------------------------------------------------- P2

/// P2: the delta baseline is READ per worktree. Main's baseline (clean) differs
/// from the worktree's (one error): a second `compile --delta` in the worktree
/// reports `+0` against its own baseline, not `+1` against main's.
#[test]
fn p2_delta_baseline_is_read_from_the_worktree() {
    let p = pair_mapped();
    keel_ok(&p.main, &["compile", "--delta", "alpha.py"]);
    write(&p.wt, "alpha.py", ALPHA_BROKEN);
    keel(&p.wt, &["compile", "--delta", "alpha.py"]);
    let second = keel(&p.wt, &["compile", "--delta", "alpha.py"]);
    assert!(
        out(&second).contains("net +0 errors"),
        "{}",
        describe(&second)
    );
}

/// P2: `keel map` resets only its own checkout's baseline.
#[test]
fn p2_map_resets_only_its_own_baseline() {
    let p = pair_mapped();
    keel_ok(&p.main, &["compile", "--delta", "alpha.py"]);
    keel_ok(&p.wt, &["compile", "--delta", "alpha.py"]);
    let (mine, main_s) = (
        own_graph_dir(&p).join("last_compile.json"),
        p.main.join(".keel/last_compile.json"),
    );
    assert!(
        mine.is_file() && main_s.is_file(),
        "precondition: both baselines exist"
    );
    keel_ok(&p.wt, &["map"]);
    assert!(
        !mine.exists(),
        "the worktree's map kept its own stale baseline"
    );
    assert!(
        main_s.is_file(),
        "the worktree's map deleted main's baseline"
    );
}

// ------------------------------------------------------------------- P3

/// A `keel.json` writer replaced the file by rename (a new inode) and left no
/// temporary file beside it.
#[cfg(unix)] // unix: inode comparison (MetadataExt)
fn assert_published_by_rename(dir: &Path, run: impl FnOnce()) {
    use std::os::unix::fs::MetadataExt;
    let (file, keel_dir) = (dir.join(".keel/keel.json"), dir.join(".keel"));
    let (names, inode) = (entries(&keel_dir), fs::metadata(&file).unwrap().ino());
    run();
    assert_ne!(
        fs::metadata(&file).unwrap().ino(),
        inode,
        "keel.json was rewritten in place"
    );
    assert_eq!(
        entries(&keel_dir),
        names,
        "a temporary file was left behind"
    );
    serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&file).unwrap()).unwrap();
}

fn initialised_repo() -> tempfile::TempDir {
    let tmp = tempfile::TempDir::new().unwrap();
    seed_repo(tmp.path(), true);
    tmp
}

/// P3: `keel config <key> <value>`.
#[cfg(unix)] // unix: inode comparison, via assert_published_by_rename
#[test]
fn p3_config_writer_publishes_by_rename() {
    let tmp = initialised_repo();
    assert_published_by_rename(tmp.path(), || {
        keel_ok(tmp.path(), &["config", "enforce.max_file_lines", "500"]);
    });
}

/// P3: `keel init --merge`.
#[cfg(unix)] // unix: inode comparison, via assert_published_by_rename
#[test]
fn p3_init_merge_writer_publishes_by_rename() {
    let tmp = initialised_repo();
    assert_published_by_rename(tmp.path(), || {
        keel_ok(tmp.path(), &["init", "--merge", "--yes"]);
    });
}

/// P3: `keel init --update-docs` (the version sync).
#[cfg(unix)] // unix: inode comparison, via assert_published_by_rename
#[test]
fn p3_update_docs_version_sync_publishes_by_rename() {
    let tmp = initialised_repo();
    let file = tmp.path().join(".keel/keel.json");
    let text = fs::read_to_string(&file).unwrap();
    let old = text.replacen(env!("CARGO_PKG_VERSION"), "0.0.1", 1);
    assert_ne!(old, text, "fixture: no version to drift");
    fs::write(&file, old).unwrap();
    assert_published_by_rename(tmp.path(), || {
        keel_ok(tmp.path(), &["init", "--update-docs"]);
    });
    assert!(fs::read_to_string(&file)
        .unwrap()
        .contains(env!("CARGO_PKG_VERSION")));
}

// ------------------------------------------------------------------- P4-P7

/// A working-tree edit only the worktree's graph can judge: `gamma` takes no
/// arguments there, and main's graph has no `gamma` at all.
fn call_gamma_with_an_argument(wt: &Path) {
    let beta = "from alpha import alpha\nfrom gamma import gamma\n\n\ndef beta() -> int:\n    \"\"\"Beta.\"\"\"\n    return alpha(1) + gamma(1)\n";
    write(wt, "beta.py", beta);
}

/// P4: `keel checkpoint` (the engine's second store handle) judges against the
/// worktree's graph.
#[test]
fn p4_checkpoint_in_a_worktree_judges_against_its_own_graph() {
    let p = pair_mapped();
    call_gamma_with_an_argument(&p.wt);
    let o = keel_ok(&p.wt, &["checkpoint", "--llm"]);
    assert!(
        out(&o).contains("E005") && out(&o).contains("gamma"),
        "{}",
        describe(&o)
    );
}

/// P5: `keel fix` (both store opens) plans against the worktree's graph.
#[test]
fn p5_fix_in_a_worktree_plans_from_its_own_graph() {
    let p = pair_mapped();
    call_gamma_with_an_argument(&p.wt);
    let o = keel_ok(&p.wt, &["fix", "--file", "beta.py", "--llm"]);
    assert!(!out(&o).contains("0 violations"), "{}", describe(&o));
    assert!(
        out(&o).contains("gamma") || out(&o).contains("E005"),
        "{}",
        describe(&o)
    );
}

/// P6: `keel name` suggests reusing a symbol only the worktree's graph has.
#[test]
fn p6_name_in_a_worktree_reads_its_own_graph() {
    let p = pair_mapped();
    let o = keel_ok(&p.wt, &["name", "gamma", "--llm"]);
    assert!(out(&o).contains("REUSE? gamma"), "{}", describe(&o));
    let main = keel_ok(&p.main, &["name", "gamma", "--llm"]);
    assert!(
        !out(&main).contains("REUSE? gamma"),
        "fixture: main knows gamma\n{}",
        describe(&main)
    );
}

/// P7: `keel init --merge` in a worktree takes ITS graph lock: main's lock being
/// held does not stop it.
#[test]
fn p7_init_merge_in_a_worktree_takes_its_own_lock() {
    let p = pair_mapped();
    let _held = keel_core::graph_lock::try_acquire(&p.main.join(".keel")).unwrap();
    let o = keel(&p.wt, &["init", "--merge", "--yes"]);
    assert_eq!(o.status.code(), Some(0), "{}", describe(&o));
}

/// P7: `keel init --merge` in a worktree resets the breaker in ITS graph, not main's.
#[test]
fn p7_init_merge_in_a_worktree_resets_its_own_breaker() {
    let p = pair_mapped();
    let seed = |db: &Path| {
        let conn = rusqlite::Connection::open(db).unwrap();
        conn.execute(
            "INSERT INTO circuit_breaker (error_code, hash, consecutive_failures, downgraded, provenance_file, last_body_hash) \
             VALUES ('E001', 'h1', 1, 0, '', '')",
            [],
        )
        .unwrap();
    };
    let rows = |db: &Path| -> i64 {
        let conn = rusqlite::Connection::open(db).unwrap();
        conn.query_row("SELECT count(*) FROM circuit_breaker", [], |r| r.get(0))
            .unwrap()
    };
    let (main_db, wt_db) = (p.main.join(".keel/graph.db"), graph_db(&p.wt));
    seed(&main_db);
    seed(&wt_db);
    keel_ok(&p.wt, &["init", "--merge", "--yes"]);
    assert_eq!(rows(&wt_db), 0, "the worktree's breaker was not reset");
    assert_eq!(
        rows(&main_db),
        1,
        "main's breaker was reset by the worktree's init"
    );
}

// ------------------------------------------------------------------- P8

/// P8: `keel push` in a never-mapped worktree looks at the worktree's graph
/// (absent), not main's; no network is reached (credentials are fake, the
/// confirmation is declined).
#[test]
fn p8_push_in_a_new_worktree_does_not_upload_mains_graph() {
    let p = pair_new_worktree();
    let home = tempfile::TempDir::new().unwrap();
    write(
        home.path(),
        ".keel/credentials.json",
        r#"{"access_token":"a","refresh_token":"r","expires_at":99999999999}"#,
    );
    let run = |dir: &Path| {
        let mut child = std::process::Command::new(common::keel_bin())
            .arg("push")
            .current_dir(dir)
            .env("HOME", home.path())
            .env("USERPROFILE", home.path())
            .env("KEEL_NO_TELEMETRY", "true")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        std::io::Write::write_all(&mut child.stdin.take().unwrap(), b"n\n").unwrap();
        child.wait_with_output().unwrap()
    };
    let main = run(&p.main);
    assert!(
        err(&main).contains("push graph.db"),
        "reference: main reaches the prompt\n{}",
        describe(&main)
    );
    let wt = run(&p.wt);
    assert_eq!(wt.status.code(), Some(2), "{}", describe(&wt));
    assert!(err(&wt).contains("no graph.db found"), "{}", describe(&wt));
}
