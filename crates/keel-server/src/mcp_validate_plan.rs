//! MCP validate-plan handler -- validate a structured plan against the dependency graph.

use serde_json::Value;

use keel_core::store::GraphStore;
use keel_core::types::EdgeDirection;

use crate::mcp::{lock_store, missing_param, JsonRpcError, SharedStore};

/// Handle the `keel/validate-plan` MCP tool call.
///
/// Takes a `plan_text` string parameter containing the plan to validate.
/// Parses action keywords and checks each against the graph for affected callers.
pub(crate) fn handle_validate_plan(
    store: &SharedStore,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let plan_text = params
        .as_ref()
        .and_then(|p| p.get("plan_text"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| missing_param("plan_text"))?
        .to_string();

    if plan_text.trim().is_empty() {
        return Err(JsonRpcError {
            code: -32602,
            message: "plan_text is empty".into(),
        });
    }

    let store = lock_store(store)?;
    let steps = parse_plan(&plan_text);

    if steps.is_empty() {
        return Ok(serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "command": "validate-plan",
            "step_count": 0,
            "overall_risk": "LOW",
            "steps": [],
            "message": "No actionable steps found in plan",
        }));
    }

    let results: Vec<StepResult> = steps
        .iter()
        .enumerate()
        .map(|(i, step)| validate_step(&*store, i + 1, step))
        .collect();

    let overall_risk = results.iter().map(|r| r.risk).max().unwrap_or(Risk::Low);

    let step_values: Vec<Value> = results.iter().map(step_to_json).collect();

    Ok(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "command": "validate-plan",
        "step_count": results.len(),
        "overall_risk": overall_risk.label(),
        "steps": step_values,
    }))
}

// --- Plan parsing (shared logic, kept simple) ---

#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    Add,
    Remove,
    Rename,
    Modify,
}

impl Action {
    fn label(&self) -> &'static str {
        match self {
            Action::Add => "add",
            Action::Remove => "remove",
            Action::Rename => "rename",
            Action::Modify => "modify",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Risk {
    Low,
    Medium,
    High,
}

impl Risk {
    fn label(&self) -> &'static str {
        match self {
            Risk::Low => "LOW",
            Risk::Medium => "MEDIUM",
            Risk::High => "HIGH",
        }
    }
}

struct PlanStep {
    action: Action,
    name: String,
}

struct CallerInfo {
    name: String,
    hash: String,
    file_path: String,
    line: u32,
}

struct StepResult {
    step_number: usize,
    action: Action,
    name: String,
    hash: Option<String>,
    risk: Risk,
    callers: Vec<CallerInfo>,
}

/// Parse plan text into structured steps by matching action keywords.
fn parse_plan(text: &str) -> Vec<PlanStep> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
                return None;
            }
            parse_line(trimmed)
        })
        .collect()
}

/// Try to parse a single line into a plan step.
fn parse_line(raw: &str) -> Option<PlanStep> {
    let lower = strip_list_prefix(&raw.to_lowercase());
    let (action, rest) = detect_action(&lower)?;
    let name = extract_name(rest.trim())?;
    Some(PlanStep { action, name })
}

/// Strip common list/bullet prefixes.
fn strip_list_prefix(s: &str) -> String {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("- ").or_else(|| s.strip_prefix("* ")) {
        return rest.to_string();
    }
    if let Some(dot_pos) = s.find(". ") {
        if dot_pos <= 3 && s[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
            return s[dot_pos + 2..].to_string();
        }
    }
    if let Some(paren_pos) = s.find(") ") {
        if paren_pos <= 3 && s[..paren_pos].chars().all(|c| c.is_ascii_digit()) {
            return s[paren_pos + 2..].to_string();
        }
    }
    s.to_string()
}

/// Detect the action keyword at the start of a line.
fn detect_action(s: &str) -> Option<(Action, &str)> {
    let patterns: &[(&[&str], Action)] = &[
        (&["rename "], Action::Rename),
        (&["remove ", "delete "], Action::Remove),
        (
            &["modify ", "change ", "update ", "refactor "],
            Action::Modify,
        ),
        (
            &["add ", "create ", "implement ", "introduce "],
            Action::Add,
        ),
    ];

    for (keywords, action) in patterns {
        for kw in *keywords {
            if let Some(rest) = s.strip_prefix(kw) {
                return Some((action.clone(), rest));
            }
        }
    }
    None
}

/// Extract a function/symbol name from text after the action keyword.
fn extract_name(rest: &str) -> Option<String> {
    let token = rest
        .split(|c: char| c.is_whitespace() || c == '(' || c == ',')
        .next()?;
    let cleaned = token.trim_matches(|c: char| c == '`' || c == '\'' || c == '"');
    if cleaned.is_empty() || !cleaned.chars().any(|c| c.is_alphabetic()) {
        return None;
    }
    Some(cleaned.to_string())
}

// --- Graph validation ---

/// Validate a single plan step against the graph.
fn validate_step(store: &dyn GraphStore, step_number: usize, step: &PlanStep) -> StepResult {
    if step.action == Action::Add {
        return StepResult {
            step_number,
            action: step.action.clone(),
            name: step.name.clone(),
            hash: None,
            risk: Risk::Low,
            callers: Vec::new(),
        };
    }

    let nodes = store.find_nodes_by_name(&step.name, "", "");
    let node = match nodes.first() {
        Some(n) => n,
        None => {
            return StepResult {
                step_number,
                action: step.action.clone(),
                name: step.name.clone(),
                hash: None,
                risk: Risk::Low,
                callers: Vec::new(),
            };
        }
    };

    let incoming = store.get_edges(node.id, EdgeDirection::Incoming);
    let callers: Vec<CallerInfo> = incoming
        .iter()
        .filter_map(|edge| {
            let src = store.get_node_by_id(edge.source_id)?;
            Some(CallerInfo {
                name: src.name,
                hash: src.hash,
                file_path: src.file_path,
                line: src.line_start,
            })
        })
        .collect();

    let risk = classify_risk(&step.action, callers.len());

    StepResult {
        step_number,
        action: step.action.clone(),
        name: step.name.clone(),
        hash: Some(node.hash.clone()),
        risk,
        callers,
    }
}

/// Classify risk based on action type and caller count.
fn classify_risk(action: &Action, caller_count: usize) -> Risk {
    match action {
        Action::Add => Risk::Low,
        Action::Modify => match caller_count {
            0 => Risk::Low,
            1..=5 => Risk::Medium,
            _ => Risk::High,
        },
        Action::Rename | Action::Remove => {
            if caller_count > 0 {
                Risk::High
            } else {
                Risk::Low
            }
        }
    }
}

/// Convert a step result to a JSON value.
fn step_to_json(r: &StepResult) -> Value {
    let callers: Vec<Value> = r
        .callers
        .iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name,
                "hash": c.hash,
                "file": c.file_path,
                "line": c.line,
            })
        })
        .collect();

    serde_json::json!({
        "step": r.step_number,
        "action": r.action.label(),
        "name": r.name,
        "hash": r.hash,
        "risk": r.risk.label(),
        "caller_count": r.callers.len(),
        "callers": callers,
    })
}
