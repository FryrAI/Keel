//! `keel checkpoint` — generate a compact, compaction-resilient session state summary.
//!
//! Summarizes changes since a commit/time: files modified, functions
//! added/changed/removed, structural impact, and current violations.

use std::collections::HashMap;
use std::process::Command;

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, GraphNode, NodeKind};
use keel_output::OutputFormatter;

mod checkpoint_output;

/// Classification of a node change.
#[derive(Debug, Clone)]
pub(crate) enum ChangeKind {
    Added,
    Changed { signature_changed: bool },
}

/// A tracked change to a graph node.
#[derive(Debug, Clone)]
pub(crate) struct NodeChange {
    pub(crate) kind: ChangeKind,
    pub(crate) node: GraphNode,
}

/// An upstream caller affected by a changed node.
#[derive(Debug, Clone)]
pub(crate) struct AffectedCaller {
    pub(crate) name: String,
    pub(crate) hash: String,
    pub(crate) file: String,
    pub(crate) line: u32,
}

/// Run `keel checkpoint` — generate a compact session state summary.
///
/// Summarizes changes since a commit/time: files modified, functions
/// added/changed/removed, structural impact, and current violations.
pub fn run(
    _formatter: &dyn OutputFormatter,
    verbose: bool,
    since: Option<String>,
    staged: bool,
    output: Option<String>,
    json: bool,
    llm: bool,
) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel checkpoint: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel checkpoint: not initialized. Run `keel init` first.");
        return 2;
    }

    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or("")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel checkpoint: failed to open graph database: {}", e);
            return 2;
        }
    };

    let changed_files = match get_changed_files(staged, &since) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("keel checkpoint: git diff failed: {}", e);
            return 2;
        }
    };

    if verbose {
        eprintln!("keel checkpoint: {} changed file(s)", changed_files.len());
    }

    let (changes, removed) = classify_changes(&store, &changed_files);
    let impact = compute_impact(&store, &changes, &removed);
    let (error_count, warning_count) = load_violation_counts(&keel_dir);
    let since_ref = build_since_ref(staged, &since);
    let session_log = get_session_log(&since_ref);
    let timestamp = get_timestamp();

    let content = if json {
        checkpoint_output::format_json(
            &since_ref,
            &timestamp,
            &changed_files,
            &changes,
            &removed,
            &impact,
            error_count,
            warning_count,
            &session_log,
        )
    } else {
        checkpoint_output::format_text(
            &since_ref,
            &timestamp,
            &changed_files,
            &changes,
            &removed,
            &impact,
            error_count,
            warning_count,
            &session_log,
            llm,
        )
    };

    if let Some(path) = output {
        if let Err(e) = std::fs::write(&path, &content) {
            eprintln!("keel checkpoint: failed to write to {}: {}", path, e);
            return 2;
        }
        if verbose {
            eprintln!("keel checkpoint: written to {}", path);
        }
    } else {
        print!("{}", content);
    }

    0
}

/// Get changed files from git based on flags.
fn get_changed_files(staged: bool, since: &Option<String>) -> Result<Vec<String>, String> {
    let args: Vec<&str> = if staged {
        vec!["diff", "--cached", "--name-only"]
    } else if let Some(ref commit) = since {
        vec!["diff", commit.as_str(), "--name-only"]
    } else {
        vec!["diff", "HEAD", "--name-only"]
    };

    let output = Command::new("git")
        .args(&args)
        .output()
        .map_err(|e| format!("failed to run git: {}", e))?;

    if !output.status.success() {
        let fallback = Command::new("git")
            .args(["diff", "--cached", "--name-only"])
            .output()
            .map_err(|e| format!("git fallback failed: {}", e))?;
        let text = String::from_utf8_lossy(&fallback.stdout);
        return Ok(parse_file_list(&text));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_file_list(&text))
}

/// Parse newline-separated file list, filtering to supported extensions.
fn parse_file_list(text: &str) -> Vec<String> {
    const SUPPORTED: &[&str] = &["rs", "py", "ts", "tsx", "js", "jsx", "go"];
    text.lines()
        .filter(|l| !l.is_empty())
        .filter(|l| {
            std::path::Path::new(l)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| SUPPORTED.contains(&e))
                .unwrap_or(false)
        })
        .map(|s| s.to_string())
        .collect()
}

/// Classify each node in changed files as added or changed.
fn classify_changes(
    store: &dyn GraphStore,
    changed_files: &[String],
) -> (Vec<NodeChange>, Vec<GraphNode>) {
    let mut changes = Vec::new();
    let removed = Vec::new();

    for file in changed_files {
        let nodes = store.get_nodes_in_file(file);
        for node in &nodes {
            if node.kind == NodeKind::Module {
                continue;
            }
            let prev_hashes = store.get_previous_hashes(node.id);
            if prev_hashes.is_empty() && node.previous_hashes.is_empty() {
                changes.push(NodeChange {
                    kind: ChangeKind::Added,
                    node: node.clone(),
                });
            } else {
                changes.push(NodeChange {
                    kind: ChangeKind::Changed {
                        signature_changed: prev_hashes
                            .iter()
                            .chain(node.previous_hashes.iter())
                            .any(|_| true),
                    },
                    node: node.clone(),
                });
            }
        }
    }

    (changes, removed)
}

/// Find upstream callers affected by changed/removed nodes.
fn compute_impact(
    store: &dyn GraphStore,
    changes: &[NodeChange],
    removed: &[GraphNode],
) -> HashMap<String, Vec<AffectedCaller>> {
    let mut impact: HashMap<String, Vec<AffectedCaller>> = HashMap::new();

    let affected: Vec<&GraphNode> = changes
        .iter()
        .filter(|c| matches!(c.kind, ChangeKind::Changed { .. }))
        .map(|c| &c.node)
        .chain(removed.iter())
        .collect();

    for node in affected {
        let incoming = store.get_edges(node.id, EdgeDirection::Incoming);
        let callers: Vec<AffectedCaller> = incoming
            .iter()
            .filter_map(|edge| {
                let src = store.get_node_by_id(edge.source_id)?;
                if src.file_path == node.file_path {
                    return None;
                }
                Some(AffectedCaller {
                    name: src.name.clone(),
                    hash: src.hash.clone(),
                    file: src.file_path.clone(),
                    line: src.line_start,
                })
            })
            .collect();

        if !callers.is_empty() {
            let key = format!("{} [{}]", node.name, node.hash);
            impact.insert(key, callers);
        }
    }

    impact
}

/// Load violation counts from the latest snapshot.
fn load_violation_counts(keel_dir: &std::path::Path) -> (usize, usize) {
    use keel_enforce::snapshot::ViolationSnapshot;
    match ViolationSnapshot::load(keel_dir) {
        Some(snap) => (snap.errors.len(), snap.warnings.len()),
        None => (0, 0),
    }
}

/// Build the reference string for the "since" marker.
fn build_since_ref(staged: bool, since: &Option<String>) -> String {
    if staged {
        "staged".to_string()
    } else {
        since.clone().unwrap_or_else(|| "HEAD".to_string())
    }
}

/// Get commit log since a reference point.
fn get_session_log(since_ref: &str) -> Vec<String> {
    if since_ref == "staged" || since_ref == "HEAD" {
        return Vec::new();
    }
    let output = Command::new("git")
        .args(["log", "--oneline", &format!("{}..HEAD", since_ref)])
        .output();
    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

/// Get current timestamp as ISO 8601 string.
fn get_timestamp() -> String {
    let output = Command::new("date").args(["--iso-8601=seconds"]).output();
    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => "unknown".to_string(),
    }
}
