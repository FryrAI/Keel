use std::io::Read;

use keel_core::store::GraphStore;
use keel_core::types::EdgeDirection;

use super::validate_plan_output;

/// The type of action described in a plan step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PlanAction {
    Add,
    Remove,
    Rename,
    Modify,
}

impl PlanAction {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            PlanAction::Add => "Add",
            PlanAction::Remove => "Remove",
            PlanAction::Rename => "Rename",
            PlanAction::Modify => "Modify",
        }
    }
}

/// A single parsed step in the plan.
#[derive(Debug, Clone)]
struct PlanStep {
    action: PlanAction,
    name: String,
}

/// Risk level for a plan step or the overall plan.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RiskLevel {
    Low,
    Medium,
    High,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "LOW"),
            RiskLevel::Medium => write!(f, "MEDIUM"),
            RiskLevel::High => write!(f, "HIGH"),
        }
    }
}

/// Caller info resolved from the graph.
pub(crate) struct CallerInfo {
    pub(crate) name: String,
    pub(crate) hash: String,
    pub(crate) file_path: String,
    pub(crate) line: u32,
}

/// Validated result for a single plan step.
pub(crate) struct StepResult {
    pub(crate) step_number: usize,
    pub(crate) action: PlanAction,
    pub(crate) name: String,
    pub(crate) hash: Option<String>,
    pub(crate) risk: RiskLevel,
    pub(crate) callers: Vec<CallerInfo>,
}

/// Run `keel validate-plan <file>` -- validate a plan against the dependency graph.
///
/// Parses a plan file (or stdin with `-`) for function modifications,
/// then checks each against the graph for affected callers.
pub fn run(verbose: bool, input: String, json: bool, llm: bool) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel validate-plan: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = cwd.join(".keel");
    if !keel_dir.exists() {
        eprintln!("keel validate-plan: not initialized. Run `keel init` first.");
        return 2;
    }

    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or("")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel validate-plan: failed to open graph database: {}", e);
            return 2;
        }
    };

    // Read plan text from stdin or file
    let plan_text = if input == "-" {
        let mut buf = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
            eprintln!("keel validate-plan: failed to read stdin: {}", e);
            return 2;
        }
        buf
    } else {
        match std::fs::read_to_string(&input) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("keel validate-plan: failed to read '{}': {}", input, e);
                return 2;
            }
        }
    };

    if plan_text.trim().is_empty() {
        eprintln!("keel validate-plan: empty plan input");
        return 2;
    }

    let steps = parse_plan(&plan_text);
    if steps.is_empty() {
        eprintln!("keel validate-plan: no actionable steps found in plan");
        return 1;
    }

    if verbose {
        eprintln!(
            "keel validate-plan: parsed {} steps from input",
            steps.len()
        );
    }

    let results = validate_steps(&store, &steps);
    let overall_risk = results
        .iter()
        .map(|r| r.risk.clone())
        .max()
        .unwrap_or(RiskLevel::Low);

    if json {
        validate_plan_output::print_json(&results, &overall_risk);
    } else {
        validate_plan_output::print_text(&results, &overall_risk, llm);
    }

    0
}

/// Parse plan text into structured steps by matching action keywords.
fn parse_plan(text: &str) -> Vec<PlanStep> {
    let mut steps = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }

        let lower = trimmed.to_lowercase();

        if let Some(step) = try_parse_line(&lower) {
            steps.push(step);
        }
    }

    steps
}

/// Try to parse a single line into a plan step.
fn try_parse_line(lower: &str) -> Option<PlanStep> {
    // Strip common list prefixes: "- ", "* ", "1. ", "1) "
    let stripped = strip_list_prefix(lower);

    let (action, rest) = detect_action(&stripped)?;
    let name = extract_name(rest.trim())?;

    Some(PlanStep { action, name })
}

/// Strip common list/bullet prefixes from a line.
fn strip_list_prefix(s: &str) -> String {
    let s = s.trim();
    // "- " or "* "
    if let Some(rest) = s.strip_prefix("- ").or_else(|| s.strip_prefix("* ")) {
        return rest.to_string();
    }
    // "1. " or "1) " style numbered lists
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
/// Returns (action, remainder) if found.
fn detect_action(s: &str) -> Option<(PlanAction, &str)> {
    let action_patterns: &[(&[&str], PlanAction)] = &[
        (&["rename "], PlanAction::Rename),
        (&["remove ", "delete "], PlanAction::Remove),
        (
            &["modify ", "change ", "update ", "refactor "],
            PlanAction::Modify,
        ),
        (
            &["add ", "create ", "implement ", "introduce "],
            PlanAction::Add,
        ),
    ];

    for (keywords, action) in action_patterns {
        for kw in *keywords {
            if let Some(rest) = s.strip_prefix(kw) {
                return Some((action.clone(), rest));
            }
        }
    }

    None
}

/// Extract a function/symbol name from the remainder after the action keyword.
///
/// Tries to grab the first word that looks like an identifier.
/// Handles patterns like "rename X to Y" by taking X.
fn extract_name(rest: &str) -> Option<String> {
    // Take the first token that looks like an identifier
    let token = rest
        .split(|c: char| c.is_whitespace() || c == '(' || c == ',')
        .next()?;

    // Strip backticks or quotes
    let cleaned = token.trim_matches(|c: char| c == '`' || c == '\'' || c == '"');
    if cleaned.is_empty() {
        return None;
    }

    // Must contain at least one alphabetic character
    if !cleaned.chars().any(|c| c.is_alphabetic()) {
        return None;
    }

    Some(cleaned.to_string())
}

/// Validate each plan step against the graph, collecting caller info.
fn validate_steps(store: &dyn GraphStore, steps: &[PlanStep]) -> Vec<StepResult> {
    steps
        .iter()
        .enumerate()
        .map(|(i, step)| validate_one_step(store, i + 1, step))
        .collect()
}

/// Validate a single plan step: look up the function and count callers.
fn validate_one_step(store: &dyn GraphStore, step_number: usize, step: &PlanStep) -> StepResult {
    // Add actions are always safe (no existing callers to break)
    if step.action == PlanAction::Add {
        return StepResult {
            step_number,
            action: step.action.clone(),
            name: step.name.clone(),
            hash: None,
            risk: RiskLevel::Low,
            callers: Vec::new(),
        };
    }

    // Look up the function in the graph
    let nodes = store.find_nodes_by_name(&step.name, "", "");
    let node = match nodes.first() {
        Some(n) => n,
        None => {
            // Function not in graph -- treat as low risk (might be new or external)
            return StepResult {
                step_number,
                action: step.action.clone(),
                name: step.name.clone(),
                hash: None,
                risk: RiskLevel::Low,
                callers: Vec::new(),
            };
        }
    };

    // Get incoming edges (callers)
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

/// Classify the risk level based on action type and caller count.
fn classify_risk(action: &PlanAction, caller_count: usize) -> RiskLevel {
    match action {
        PlanAction::Add => RiskLevel::Low,
        PlanAction::Modify => match caller_count {
            0 => RiskLevel::Low,
            1..=5 => RiskLevel::Medium,
            _ => RiskLevel::High,
        },
        PlanAction::Rename | PlanAction::Remove => {
            if caller_count > 0 {
                RiskLevel::High
            } else {
                RiskLevel::Low
            }
        }
    }
}
