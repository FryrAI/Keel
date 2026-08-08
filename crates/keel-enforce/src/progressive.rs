//! Progressive adoption: annotation/documentation debt on code the current
//! change did NOT touch reports as WARNING, not ERROR.
//!
//! "Untouched" means: the definition's computed hash equals the stored graph
//! hash AND the node carries no `previous_hashes` — i.e. it has not been
//! modified since the last full `keel map` (the engine appends the old hash
//! to `previous_hashes` whenever a compile syncs a changed hash, and a full
//! map re-baselines by clearing them). The moment a function is edited it
//! escalates to ERROR and STAYS escalated across compiles until fixed —
//! touch it, fix it. This implements the documented adoption model
//! (new/modified code = ERROR, pre-existing = WARNING) without a baseline
//! file.
//!
//! KNOWN LIMITATION: because a full `keel map` re-baselines, escalation
//! state does not survive a re-map — edited-but-unfixed debt returns to
//! WARNING afterward. The debt stays VISIBLE (warnings still print and are
//! counted in stats/telemetry); only its blocking status resets. Making
//! escalation survive re-maps needs per-node provenance beyond what the
//! graph stores today.

use keel_core::types::GraphNode;
use keel_parsers::resolver::FileIndex;

use crate::types::Violation;

/// Codes eligible for the pre-existing downgrade. Structural breakage
/// (E001/E004/E005) always stays at full severity — a broken caller is a
/// broken caller regardless of when the function was written.
const PROGRESSIVE_CODES: &[&str] = &["E002", "E003"];

/// Downgrade E002/E003 ERRORs on untouched functions to WARNING.
pub fn apply_progressive_adoption(
    violations: Vec<Violation>,
    file: &FileIndex,
    existing_nodes: &[GraphNode],
) -> Vec<Violation> {
    violations
        .into_iter()
        .map(|mut v| {
            if v.severity == "ERROR" && PROGRESSIVE_CODES.contains(&v.code.as_str()) {
                // Which stored node IS this def is `bind_to_node`'s question,
                // not first-name-wins: a file can hold several `run`s across
                // impl blocks. Untouched then means the bound node still
                // carries this exact body AND has no rename history —
                // modified-earlier-this-session functions carry their old hash
                // in previous_hashes and must NOT decay back to "pre-existing".
                let untouched = file
                    .definitions
                    .iter()
                    .find(|d| d.line_start == v.line)
                    .and_then(|def| {
                        crate::violations_util::bind_to_node(def, file, existing_nodes)
                            .map(|node| (node, def))
                    })
                    .is_some_and(|(node, def)| {
                        node.previous_hashes.is_empty()
                            && crate::violations_util::node_hash_matches(node, def, &file.file_path)
                    });
                if untouched {
                    v.severity = "WARNING".to_string();
                    v.fix_hint = Some(format!(
                        "{} (pre-existing — escalates to ERROR when this function is edited)",
                        v.fix_hint.unwrap_or_default()
                    ));
                }
            }
            v
        })
        .collect()
}

#[cfg(test)]
#[path = "progressive_tests.rs"]
mod tests;
