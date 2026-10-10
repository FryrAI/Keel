//! W013 — a `SECURITY DEFINER` routine whose `search_path` does not end with
//! `pg_temp`.
//!
//! PostgreSQL searches the session's temporary schema first unless `pg_temp`
//! is listed, so such a routine resolves an unqualified relation to a temp
//! table its caller made, and runs whatever hangs off it with the owner's
//! rights. The effective list is the routine's own `SET search_path` as later
//! `ALTER FUNCTION … SET/RESET` statements leave it, so the check replays every
//! SQL file of the routine's directory in the order a migration runner applies
//! them. It reads those files from disk: the graph stores no statement text.

mod history;
mod lex;
mod statements;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_review;
mod types;

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use keel_core::types::{GraphNode, NodeKind};
use keel_parsers::resolver::FileIndex;
use keel_parsers::treesitter::detect_language;

use crate::types::Violation;
use history::Finding;

const FIX_HINT: &str = "append pg_temp as the last search_path entry, \
     e.g. `SET search_path = pg_catalog, public, pg_temp`";

/// The W013 check for one compile: each directory is replayed once.
pub struct DefinerSearchPath {
    root: PathBuf,
    by_dir: HashMap<PathBuf, Replay>,
}

/// One directory's SQL files in applied order, and the findings per file.
struct Replay {
    names: Vec<String>,
    found: HashMap<usize, Vec<Finding>>,
}

impl DefinerSearchPath {
    /// A check that resolves graph paths against the project `root`.
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            by_dir: HashMap::new(),
        }
    }

    /// W013 findings for the routines `file` creates.
    pub fn check_file(&mut self, file: &FileIndex, existing_nodes: &[GraphNode]) -> Vec<Violation> {
        let path = Path::new(&file.file_path);
        if !is_sql(path) {
            return Vec::new();
        }
        let absolute = self.root.join(path);
        let (Some(dir), Some(name)) = (absolute.parent(), absolute.file_name()) else {
            return Vec::new();
        };
        let replay = self
            .by_dir
            .entry(dir.to_path_buf())
            .or_insert_with(|| replay(dir));
        let Some(source) = replay.names.iter().position(|one| name == one.as_str()) else {
            return Vec::new();
        };
        let found = replay.found.get(&source).into_iter().flatten();
        found
            .map(|finding| violation(path, finding, &replay.names, existing_nodes))
            .collect()
    }
}

fn is_sql(path: &Path) -> bool {
    detect_language(path) == Some("sql")
}

fn replay(dir: &Path) -> Replay {
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_applied(name))
        .collect();
    names.sort_by(|a, b| applied_order(a, b));
    // Statements are ASCII: a file in another ASCII-compatible encoding still
    // yields them. An unreadable file contributes none and keeps its place.
    let texts: Vec<String> = names
        .iter()
        .map(|name| fs::read(dir.join(name)).unwrap_or_default())
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .collect();
    let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
    Replay {
        found: history::findings(&texts),
        names,
    }
}

/// Whether a forward migration run applies the file called `name`: any SQL
/// file but the `.down.sql` half of an up/down pair.
fn is_applied(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.ends_with(".sql") && !name.ends_with(".down.sql")
}

/// The order a migration runner applies two files in: by name, except that
/// Flyway's repeatable `R__` files run after every versioned one.
fn applied_order(a: &str, b: &str) -> Ordering {
    let repeatable = |name: &str| name.starts_with("R__");
    repeatable(a)
        .cmp(&repeatable(b))
        .then_with(|| natural_cmp(a, b))
}

/// File names in the order a migration runner applies them: a run of digits
/// compares as a number, so `9_x.sql` precedes `10_x.sql`.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut left, mut right) = (a.as_bytes(), b.as_bytes());
    while let (Some(x), Some(y)) = (left.first(), right.first()) {
        // A version ends at `__`: `V1__a` precedes `V1.1__b` and `V1_1__b`.
        if left.starts_with(b"__") && continues_version(right) {
            return Ordering::Less;
        }
        if right.starts_with(b"__") && continues_version(left) {
            return Ordering::Greater;
        }
        if x.is_ascii_digit() && y.is_ascii_digit() {
            let (number, rest) = split_number(left);
            let (other, other_rest) = split_number(right);
            let order = number.len().cmp(&other.len()).then(number.cmp(other));
            if order != Ordering::Equal {
                return order;
            }
            (left, right) = (rest, other_rest);
        } else if x != y {
            return x.cmp(y);
        } else {
            (left, right) = (&left[1..], &right[1..]);
        }
    }
    left.len().cmp(&right.len()).then(a.cmp(b))
}

/// Whether `rest` goes on with a sub-version (`.1`, `_1`).
fn continues_version(rest: &[u8]) -> bool {
    matches!(rest, [b'.' | b'_', digit, ..] if digit.is_ascii_digit())
}

/// A leading run of digits without its leading zeros, and what follows it.
fn split_number(bytes: &[u8]) -> (&[u8], &[u8]) {
    let end = bytes
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .unwrap_or(bytes.len());
    let zeros = bytes[..end]
        .iter()
        .take_while(|byte| **byte == b'0')
        .count();
    (&bytes[zeros..end], &bytes[end..])
}

/// `name` beside the graph path `path`.
fn finding_file(path: &Path, name: &str) -> String {
    let path = path.to_string_lossy();
    match path.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{name}"),
        None => name.to_string(),
    }
}

fn violation(path: &Path, finding: &Finding, names: &[String], nodes: &[GraphNode]) -> Violation {
    let state = match &finding.list {
        None => "sets no search_path".to_string(),
        Some(list) => format!(
            "has search_path `{}`, which does not end with pg_temp",
            list.join(", ")
        ),
    };
    let changed = finding.changed_at.map_or(String::new(), |site| {
        let verb = if finding.list.is_some() {
            "set"
        } else {
            "reset"
        };
        // Graph paths use `/` on every platform.
        let file = finding_file(path, &names[site.source]);
        format!(" ({verb} at {file}:{})", site.line)
    });
    // The stored node starts above the `CREATE` keyword (at the end of the
    // previous statement), so it is bound by name and line span.
    let node = nodes.iter().find(|node| {
        node.kind == NodeKind::Function
            && node.name.eq_ignore_ascii_case(&finding.name)
            && (node.line_start..=node.line_end).contains(&finding.line)
    });
    Violation {
        code: "W013".to_string(),
        severity: "WARNING".to_string(),
        category: "definer_search_path".to_string(),
        message: format!(
            "SECURITY DEFINER routine `{}` {state}{changed}: \
             the caller's temporary schema is searched first",
            finding.name
        ),
        file: path.to_string_lossy().to_string(),
        line: finding.line,
        hash: node.map(|node| node.hash.clone()).unwrap_or_default(),
        confidence: 0.9,
        resolution_tier: "heuristic".to_string(),
        fix_hint: Some(FIX_HINT.to_string()),
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    }
}
