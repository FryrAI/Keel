//! `keel validate-plan` — check a plan against the dependency graph *before*
//! execution, so an agent sees the blast radius of a risky change up front.
//!
//! Deliberately simple (KISS): scan the plan for symbol names that exist in
//! the graph and for action keywords near them, then report callers at risk,
//! a risk level, and a callers-first suggested order. This is heuristic
//! assistance, not a gate — it never fails. Shared by the CLI command and the
//! MCP `keel/validate-plan` tool.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use keel_core::store::GraphStore;
use keel_core::types::{GraphNode, NodeKind};

use crate::checkpoint::{callers_of, CallerRef};
use crate::validate_plan_findings::{detect_plan_findings, PlanContext};
use crate::validate_plan_reuse::detect_reuse_findings;

pub use crate::validate_plan_findings::PlanFinding;

/// One detected (action, symbol) pair with its risk assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanAction {
    /// Detected action: `remove`, `rename`, `change_signature`, or `add_param`.
    pub action: String,
    pub symbol: String,
    pub hash: String,
    pub file: String,
    pub line: u32,
    pub caller_count: usize,
    pub callers: Vec<CallerRef>,
    /// `HIGH`, `MEDIUM`, or `LOW`.
    pub risk: String,
    pub suggested_order: String,
    /// How many times the symbol is named in the plan.
    pub mentions: usize,
}

/// The validation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanValidationResult {
    pub version: String,
    pub command: String,
    pub actions: Vec<PlanAction>,
    pub symbols_detected: usize,
    pub files_detected: Vec<String>,
    /// True when no graph-relevant actions were detected.
    pub unrecognized: bool,
    /// Plan-time findings (`P001`/`P002`/advisory-only `P003`). Omitted from JSON when empty so a
    /// clean plan serializes exactly as it did before the `P` namespace
    /// existed — the never-fails report shape is a contract.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<PlanFinding>,
}

impl PlanValidationResult {
    /// True when at least one finding is still live (not circuit-breaker
    /// downgraded) — i.e. `--strict` should exit 1.
    pub fn has_live_findings(&self) -> bool {
        self.findings
            .iter()
            .any(|f| matches!(f.code.as_str(), "P001" | "P002") && !f.downgraded)
    }
}

/// Split text into identifier-like tokens (length >= 3) for word-boundary
/// matching against graph node names.
fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            cur.push(ch);
        } else if !cur.is_empty() {
            if cur.len() >= 3 {
                out.push(std::mem::take(&mut cur));
            } else {
                cur.clear();
            }
        }
    }
    if cur.len() >= 3 {
        out.push(cur);
    }
    out
}

/// Detect an action keyword in a lowercased line, with a risk rank
/// (higher = more destructive) used to pick the strongest action per symbol.
fn line_action(line_lower: &str) -> Option<(&'static str, u8)> {
    if line_lower.contains("rename") {
        Some(("rename", 3))
    } else if line_lower.contains("remove")
        || line_lower.contains("delete")
        || line_lower.contains("drop ")
    {
        Some(("remove", 3))
    } else if line_lower.contains("signature") {
        Some(("change_signature", 2))
    } else if line_lower.contains("add param")
        || line_lower.contains("add a param")
        || line_lower.contains("new param")
        || line_lower.contains("parameter")
        || line_lower.contains("add arg")
        || line_lower.contains("argument")
    {
        Some(("add_param", 2))
    } else {
        None
    }
}

fn risk_level(action: &str, callers: usize) -> String {
    let level = match action {
        "remove" | "rename" if callers > 0 => "HIGH",
        "change_signature" | "add_param" if callers > 0 => "MEDIUM",
        _ => "LOW",
    };
    level.to_string()
}

fn order_hint(action: &str, symbol: &str, callers: usize, risk: &str) -> String {
    let verb = action.replace('_', " ");
    if callers > 0 && risk != "LOW" {
        format!("Update {callers} caller(s) first, then {verb} `{symbol}`")
    } else if callers > 0 {
        format!("{verb} `{symbol}` (review {callers} caller(s))")
    } else {
        format!("Safe: no callers of `{symbol}`")
    }
}

/// Validate a plan against the stored dependency graph. Never fails.
pub fn validate_plan(store: &dyn GraphStore, plan: &str) -> PlanValidationResult {
    // Mention counts across the whole plan.
    let mut mentions: HashMap<String, usize> = HashMap::new();
    for tok in tokenize(plan) {
        *mentions.entry(tok).or_default() += 1;
    }

    // Resolve which mentioned tokens are known symbols (exact, non-module).
    // Only tokens are resolved: a `name(...)` claim the tokenizer skipped
    // (shorter than three characters, or non-ASCII) gets no entry here, and the
    // finding pass reads that absence as "cannot tell" and stays quiet — the
    // deliberate precision floor, not a gap to be closed.
    //
    // Each token is looked up exactly ONCE and the whole result kept, because
    // the P001/P002 pass needs the same lists (modules included). When several
    // nodes share a name, keep the one with the most callers — computing each
    // candidate's callers exactly ONCE (the previous code re-evaluated
    // `callers_of` inside a `sort_by_key` comparator) and caching the winner's
    // list for reuse when building actions below (avoiding the previous third
    // `callers_of` call). The strict `>` keeps the first candidate achieving
    // the max, matching the earlier stable-sort tie-break, so output is
    // unchanged.
    let mut nodes_by_name: HashMap<String, Vec<GraphNode>> = HashMap::new();
    let mut symbol_node: HashMap<String, GraphNode> = HashMap::new();
    let mut symbol_callers: HashMap<String, Vec<CallerRef>> = HashMap::new();
    for tok in mentions.keys() {
        let nodes = store.find_nodes_by_name(tok, "", "");
        let mut best: Option<(&GraphNode, Vec<CallerRef>)> = None;
        for cand in nodes.iter().filter(|n| n.kind != NodeKind::Module) {
            let callers = callers_of(store, cand);
            if best.as_ref().is_none_or(|(_, b)| callers.len() > b.len()) {
                best = Some((cand, callers));
            }
        }
        if let Some((winner, callers)) = best {
            symbol_node.insert(tok.clone(), winner.clone());
            symbol_callers.insert(tok.clone(), callers);
        }
        nodes_by_name.insert(tok.clone(), nodes);
    }

    // Pair action keywords with symbols on the same line; keep the strongest.
    let mut sym_action: HashMap<String, (&'static str, u8)> = HashMap::new();
    for line in plan.lines() {
        let action = match line_action(&line.to_lowercase()) {
            Some(a) => a,
            None => continue,
        };
        let line_tokens: HashSet<String> = tokenize(line).into_iter().collect();
        for name in symbol_node.keys() {
            if line_tokens.contains(name) {
                let entry = sym_action.entry(name.clone()).or_insert(action);
                if action.1 > entry.1 {
                    *entry = action;
                }
            }
        }
    }

    let mut actions: Vec<PlanAction> = sym_action
        .iter()
        .map(|(name, (action, _))| {
            let node = &symbol_node[name];
            // Reuse the caller list cached when this winner was chosen above.
            let callers = symbol_callers.get(name).cloned().unwrap_or_default();
            let caller_count = callers.len();
            let risk = risk_level(action, caller_count);
            let suggested_order = order_hint(action, name, caller_count, &risk);
            PlanAction {
                action: (*action).to_string(),
                symbol: name.clone(),
                hash: node.hash.clone(),
                file: node.file_path.clone(),
                line: node.line_start,
                caller_count,
                callers: callers.into_iter().take(20).collect(),
                risk,
                suggested_order,
                mentions: mentions.get(name).copied().unwrap_or(0),
            }
        })
        .collect();

    // Rank by mention count (desc), then symbol name for stability.
    actions.sort_by(|a, b| b.mentions.cmp(&a.mentions).then(a.symbol.cmp(&b.symbol)));

    let files_detected: Vec<String> = store
        .get_all_modules()
        .into_iter()
        .map(|m| m.file_path)
        .filter(|p| !p.is_empty() && plan.contains(p.as_str()))
        .collect();

    // P001/P002 read the resolution work above rather than re-querying.
    let mut findings = detect_plan_findings(
        plan,
        &PlanContext {
            symbol_node: &symbol_node,
            nodes_by_name: &nodes_by_name,
            symbol_callers: &symbol_callers,
            actions: &sym_action,
        },
    );
    findings.extend(detect_reuse_findings(store, plan));
    findings.sort_by(|a, b| a.code.cmp(&b.code).then(a.symbol.cmp(&b.symbol)));
    findings.truncate(20);

    PlanValidationResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        command: "validate-plan".to_string(),
        unrecognized: actions.is_empty(),
        symbols_detected: symbol_node.len(),
        actions,
        files_detected,
        findings,
    }
}

#[cfg(test)]
#[path = "validate_plan_tests.rs"]
mod tests;
