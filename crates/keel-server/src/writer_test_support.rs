//! Disk-row snapshots and an isolated Git fixture for writer regressions.

use rusqlite::{Connection, OpenFlags};
use std::path::Path;

/// Snapshot primary-key row contents in every graph and persistent state table.
pub(super) fn state(root: &Path) -> Vec<(String, Vec<Vec<String>>)> {
    let db = Connection::open_with_flags(
        root.join(".keel/graph.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut names = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let tables: Vec<String> = names
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    tables
        .into_iter()
        .map(|table| {
            let mut statement = db
                .prepare(&format!("SELECT * FROM \"{table}\" ORDER BY rowid"))
                .unwrap();
            let columns = statement.column_count();
            let rows = statement
                .query_map([], |row| {
                    Ok((0..columns)
                        .map(|i| format!("{:?}", row.get_ref(i).unwrap()))
                        .collect())
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            (table, rows)
        })
        .collect()
}

fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

/// Materialize a Git repository tracking the original hello.rs without Git writes.
/// The objects and v2 index below encode one commit of `fn hello() -> i32 { 1 }`.
pub(super) fn checkpoint_git_fixture(root: &Path) {
    let git = root.join(".git");
    std::fs::create_dir_all(git.join("refs/heads")).unwrap();
    std::fs::create_dir_all(git.join("objects")).unwrap();
    std::fs::write(
        git.join("config"),
        "[core]\nrepositoryformatversion = 0\nbare = false\n",
    )
    .unwrap();
    std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(
        git.join("refs/heads/main"),
        "284362a55b86924c6c034e82fe3cbe7680f86cfe\n",
    )
    .unwrap();
    std::fs::write(git.join("index"), bytes("444952430000000200000001000000000000000000000000000000000000000000000000000081a400000000000000000000001864ada2c9e13e63dbefb1a9974d886112e2136d1f000868656c6c6f2e727300003154866055dbec1540c2ca3fb3aab7704374d4cb")).unwrap();
    for (hash, contents) in [
        ("64ada2c9e13e63dbefb1a9974d886112e2136d1f", "789c4bcac94f5230326148cb53c848cdc9c9d7d054d0b553c8343652a8563054a8e50200986e088b"),
        ("4b684ef4f13b0e27461e84ce6bcbf2aed9883ab6", "789c2b294a4d5530366330343030333151c848cdc9c9d72b2a664859bbe8e443bbe4dbef37ae9ceedb9128f44838571e0054771132"),
        ("284362a55b86924c6c034e82fe3cbe7680f86cfe", "789c8d8dc10ac23010053de72bf62e48d2ae318248ef1ef50792ed0b2d36a6d42df8f916bfc039cc6960a496322ab923ef740188930f8cccd9b5c9a239b177082cf049526e22fa73086d4cdec45587bad00d98e881b7d2453777f8c4324f38482d57b2b4b71b467e13c59fb9b9033dc90079ce757ca9f902c77c345d"),
    ] {
        let dir = git.join("objects").join(&hash[..2]);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(&hash[2..]), bytes(contents)).unwrap();
    }
}
