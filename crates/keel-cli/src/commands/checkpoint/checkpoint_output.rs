//! Output formatting for `keel checkpoint` — text and JSON renderers.

use std::collections::HashMap;

use keel_core::types::GraphNode;

use super::{AffectedCaller, ChangeKind, NodeChange};

/// Format checkpoint output as human-readable text.
#[allow(clippy::too_many_arguments)]
pub(super) fn format_text(
    since_ref: &str,
    timestamp: &str,
    changed_files: &[String],
    changes: &[NodeChange],
    removed: &[GraphNode],
    impact: &HashMap<String, Vec<AffectedCaller>>,
    error_count: usize,
    warning_count: usize,
    session_log: &[String],
    _llm: bool,
) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "CHECKPOINT since={} ({})\n\n",
        since_ref, timestamp
    ));

    // Changes section
    out.push_str(&format!("Changes: ({} files)\n", changed_files.len()));
    for change in changes {
        let prefix = match &change.kind {
            ChangeKind::Added => "+",
            ChangeKind::Changed { .. } => "~",
        };
        let detail = match &change.kind {
            ChangeKind::Added => "added".to_string(),
            ChangeKind::Changed { signature_changed } => {
                if *signature_changed {
                    "changed (signature change)".to_string()
                } else {
                    "changed".to_string()
                }
            }
        };
        out.push_str(&format!(
            "  {} {}: {} {} [{}]\n",
            prefix, change.node.file_path, detail, change.node.name, change.node.hash
        ));
    }
    for node in removed {
        out.push_str(&format!(
            "  - {}: removed {} [{}]\n",
            node.file_path, node.name, node.hash
        ));
    }

    // Impact section
    if !impact.is_empty() {
        out.push('\n');
        out.push_str("Impact:\n");
        for (changed_node, callers) in impact.iter() {
            out.push_str(&format!(
                "  \u{26a0} {} caller(s) affected by {}:\n",
                callers.len(),
                changed_node
            ));
            for caller in callers.iter() {
                out.push_str(&format!(
                    "    {} [{}] in {}:{}\n",
                    caller.name, caller.hash, caller.file, caller.line
                ));
            }
        }
    }

    // Violations section
    out.push('\n');
    out.push_str(&format!(
        "Violations:\n  {} error(s), {} warning(s) outstanding\n",
        error_count, warning_count
    ));

    // Session log
    if !session_log.is_empty() {
        out.push('\n');
        out.push_str("Session:\n");
        for line in session_log {
            out.push_str(&format!("  {}\n", line));
        }
    }

    out
}

/// Format checkpoint output as JSON.
#[allow(clippy::too_many_arguments)]
pub(super) fn format_json(
    since_ref: &str,
    timestamp: &str,
    changed_files: &[String],
    changes: &[NodeChange],
    removed: &[GraphNode],
    impact: &HashMap<String, Vec<AffectedCaller>>,
    error_count: usize,
    warning_count: usize,
    session_log: &[String],
) -> String {
    let changes_json: Vec<serde_json::Value> = changes
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

    let impact_json: Vec<serde_json::Value> = impact
        .iter()
        .map(|(changed_node, callers): (&String, &Vec<AffectedCaller>)| {
            let callers_json: Vec<serde_json::Value> = callers
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

    let result = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "command": "checkpoint",
        "since": since_ref,
        "timestamp": timestamp,
        "files_changed": changed_files.len(),
        "changes": changes_json,
        "impact": impact_json,
        "violations": {
            "errors": error_count,
            "warnings": warning_count,
        },
        "session": session_log,
    });

    format!(
        "{}\n",
        serde_json::to_string_pretty(&result).unwrap_or_default()
    )
}
