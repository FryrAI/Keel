// Integration tests: Python `ty` subprocess wiring (issue #33).
//
// `keel map` builds its Python resolver via `PyResolver::detect()`, which picks
// up a `ty` binary from PATH when present and falls back to heuristics when it
// is absent. These tests drive that end-to-end with a shell-script `ty` stub so
// no real ty install is required. (Unix-only: the stub is a POSIX shell script.)

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use tempfile::TempDir;

fn keel_bin() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("keel");
    if path.exists() {
        return path;
    }
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    workspace.join("target/debug/keel")
}

/// Create a temp dir holding an executable `ty` stub. Every `ty check ...`
/// invocation appends a line to the file at `$KEEL_TY_MARKER`.
fn make_ty_stub() -> TempDir {
    let bin = TempDir::new().unwrap();
    let stub = bin.path().join("ty");
    fs::write(
        &stub,
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then echo 'ty 0.0.0-stub'; exit 0; fi\n\
         if [ -n \"$KEEL_TY_MARKER\" ]; then echo \"$@\" >> \"$KEEL_TY_MARKER\"; fi\n\
         echo '[]'\n\
         exit 0\n",
    )
    .unwrap();
    let mut perms = fs::metadata(&stub).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&stub, perms).unwrap();
    bin
}

/// A python project whose call cannot be resolved by heuristics, forcing the
/// resolver to fall through to the ty Tier-2 step.
fn setup_py_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/app.py"),
        "def main() -> int:\n    return mystery_call()\n",
    )
    .unwrap();
    dir
}

fn path_with(prefix: &std::path::Path) -> String {
    let base = std::env::var("PATH").unwrap_or_default();
    format!("{}:{}", prefix.display(), base)
}

#[test]
fn test_map_picks_up_ty_from_path() {
    let stub = make_ty_stub();
    let project = setup_py_project();
    let marker = project.path().join("ty_invoked.log");
    let keel = keel_bin();

    let init = Command::new(&keel)
        .arg("init")
        .current_dir(project.path())
        .output()
        .unwrap();
    assert!(init.status.success(), "init failed: {:?}", init.stderr);

    // Map with the stub on PATH: PyResolver::detect() must find `ty`, and the
    // unresolved `mystery_call()` must fall through to a ty check invocation.
    let map = Command::new(&keel)
        .arg("map")
        .current_dir(project.path())
        .env("PATH", path_with(stub.path()))
        .env("KEEL_TY_MARKER", &marker)
        .output()
        .unwrap();
    assert!(
        map.status.success(),
        "map with ty stub failed: {}",
        String::from_utf8_lossy(&map.stderr)
    );
    assert!(
        marker.exists(),
        "the ty stub should have been invoked during map (PyResolver picked up ty)"
    );
}

#[test]
fn test_map_falls_back_cleanly_without_ty() {
    let project = setup_py_project();
    let keel = keel_bin();

    let init = Command::new(&keel)
        .arg("init")
        .current_dir(project.path())
        .output()
        .unwrap();
    assert!(init.status.success(), "init failed: {:?}", init.stderr);

    // A PATH with no `ty`: detect() returns None, map still succeeds on
    // heuristics alone.
    let map = Command::new(&keel)
        .arg("map")
        .current_dir(project.path())
        .env("PATH", "/nonexistent-keel-test-dir")
        .output()
        .unwrap();
    assert!(
        map.status.success(),
        "map without ty must still succeed: {}",
        String::from_utf8_lossy(&map.stderr)
    );
}
