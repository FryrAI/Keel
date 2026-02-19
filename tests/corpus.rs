//! Corpus tests — clone real repos and verify keel map/compile on them.
//! Run with: cargo test --features corpus --test corpus -- --ignored
#![cfg(feature = "corpus")]

#[path = "corpus/test_cobra.rs"]
mod test_cobra;
#[path = "corpus/test_httpx.rs"]
mod test_httpx;
#[path = "corpus/test_ky.rs"]
mod test_ky;
#[path = "corpus/test_serde.rs"]
mod test_serde;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Directory where corpus repos are cloned.
fn corpus_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("test-corpus");
    p
}

/// Path to the built `keel` binary.
fn keel_binary() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("target");
    p.push("debug");
    p.push("keel");
    p
}

/// Clone `url` at the given `tag` into `test-corpus/{name}`.
///
/// Returns `Some(path)` on success, `None` if the clone fails (e.g. no
/// network).  If the directory already exists **and** the requested tag is
/// already checked out, the clone is skipped.
pub fn ensure_repo(name: &str, url: &str, tag: &str) -> Option<PathBuf> {
    let dest = corpus_dir().join(name);

    if dest.exists() {
        // Already cloned — make sure the right tag is checked out.
        let status = Command::new("git")
            .args(["checkout", tag])
            .current_dir(&dest)
            .status()
            .ok()?;
        if status.success() {
            return Some(dest);
        }
    }

    // Fresh clone at the requested tag.
    std::fs::create_dir_all(corpus_dir()).ok()?;
    let status = Command::new("git")
        .args(["clone", "--depth", "1", "--branch", tag, url])
        .arg(&dest)
        .status()
        .ok()?;

    if status.success() {
        Some(dest)
    } else {
        None
    }
}

/// Run the `keel` binary with the given arguments in `dir`.
///
/// Returns `(exit_code, stdout, stderr)`.
pub fn run_keel(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let Output {
        status,
        stdout,
        stderr,
    } = Command::new(keel_binary())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to execute keel binary");

    (
        status.code().unwrap_or(-1),
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    )
}

/// Count the number of nodes stored in `graph.db` for the repo at `dir`.
pub fn count_graph_nodes(dir: &Path) -> usize {
    let db_path = dir.join(".keel").join("graph.db");
    let conn =
        rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("failed to open graph.db");

    conn.query_row("SELECT COUNT(*) FROM nodes", [], |row| {
        row.get::<_, usize>(0)
    })
    .expect("failed to count nodes")
}
