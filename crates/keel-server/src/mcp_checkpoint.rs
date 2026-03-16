//! MCP checkpoint handler — generate a compact session state summary.

use std::collections::HashMap;
use std::process::Command;

use serde_json::Value;

use keel_core::store::GraphStore;
use keel_core::types::{EdgeDirection, GraphNode, NodeKind};

use crate::mcp::{internal_err, lock_store, JsonRpcError, SharedStore};

/// Handle the `keel/checkpoint` MCP tool call.
///
/// Returns a structured summary of changes since a commit, including
/// affected callers (impact) and current violation counts.
pub(crate) fn handle_checkpoint(
    store: &SharedStore,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let since = params
        .as_ref()
        .and_then(|p| p.get("since"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let staged = params
        .as_ref()
        .and_then(|p| p.get("staged"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Get changed files via git
    let changed_files = get_changed_files(staged, &since).map_err(internal_err)?;

    let store = lock_store(store)?;

    // Classify changes and compute impact
    let (changes, removed) = classify_changes(&*store, &changed_files);
    let impact = compute_impact(&*store, &changes, &removed);

    // Load violation counts
    let (error_count, warning_count) = load_violation_counts();

    // Get session log
    let since_ref = if staged {
        "staged".to_string()
    } else {
        since.unwrap_or_else(|| "HEAD".to_string())
    };
    let session_log = get_session_log(&since_ref);

    // Build changes array
    let changes_json: Vec<Value> = changes
        .iter()
        .map(|c| {
            let (status, sig_changed) = match &c.kind {
                ChangeKind::Added => ("added", false),
                ChangeKind::Changed { signature_changed } => ("changed", *signature_changed),
            };
            serde_json::json!({
                "status": status,
                "name": c.node.name,
                "hash": c.node.hash,
                "file": c.node.file_path,
                "line": c.node.line_start,
                "kind": c.node.kind.as_str(),
                "signature_changed": sig_changed,
            })
        })
        .chain(removed.iter().map(|n| {
            serde_json::json!({
                "status": "removed",
                "name": n.name,
                "hash": n.hash,
                "file": n.file_path,
                "line": n.line_start,
                "kind": n.kind.as_str(),
                "signature_changed": false,
            })
        }))
        .collect();

    // Build impact array
    let impact_json: Vec<Value> = impact
        .iter()
        .map(|(changed_node, callers)| {
            let callers_json: Vec<Value> = callers
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "name": c.name,
                        "hash": c.hash,
                        "file": c.file,
                        "line": c.line,
                    })
                })
                .collect();
            serde_json::json!({
                "changed_node": changed_node,
                "affected_callers": callers_json,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "command": "checkpoint",
        "since": since_ref,
        "files_changed": changed_files.len(),
        "changes": changes_json,
        "impact": impact_json,
        "violations": {
            "errors": error_count,
            "warnings": warning_count,
        },
        "session": session_log,
    }))
}

// --- Internal types ---

#[derive(Debug, Clone)]
enum ChangeKind {
    Added,
    Changed { signature_changed: bool },
}

#[derive(Debug, Clone)]
struct NodeChange {
    kind: ChangeKind,
    node: GraphNode,
}

#[derive(Debug, Clone)]
struct AffectedCaller {
    name: String,
    hash: String,
    file: String,
    line: u32,
}

// --- Internal helpers ---

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
                        signature_changed: true,
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

    let all_affected: Vec<&GraphNode> = changes
        .iter()
        .filter(|c| matches!(c.kind, ChangeKind::Changed { .. }))
        .map(|c| &c.node)
        .chain(removed.iter())
        .collect();

    for node in all_affected {
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
fn load_violation_counts() -> (usize, usize) {
    let keel_dir = std::env::current_dir()
        .map(|p| p.join(".keel"))
        .unwrap_or_default();
    use keel_enforce::snapshot::ViolationSnapshot;
    match ViolationSnapshot::load(&keel_dir) {
        Some(snap) => (snap.errors.len(), snap.warnings.len()),
        None => (0, 0),
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
