//! Output formatting for `keel validate-plan`.
//!
//! Separated from validate_plan.rs to stay under the 400-line file limit.

use super::validate_plan::{CallerInfo, PlanAction, RiskLevel, StepResult};

/// Print results as JSON.
pub(super) fn print_json(results: &[StepResult], overall_risk: &RiskLevel) {
    let steps: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            let callers: Vec<serde_json::Value> = r
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
                "action": r.action.label().to_lowercase(),
                "name": r.name,
                "hash": r.hash,
                "risk": format!("{}", r.risk),
                "caller_count": r.callers.len(),
                "callers": callers,
            })
        })
        .collect();

    println!(
        "{}",
        serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "command": "validate-plan",
            "step_count": results.len(),
            "overall_risk": format!("{}", overall_risk),
            "steps": steps,
        })
    );
}

/// Print results as human-readable text.
pub(super) fn print_text(results: &[StepResult], overall_risk: &RiskLevel, llm: bool) {
    if llm {
        println!(
            "PLAN ANALYSIS ({} steps, risk: {})",
            results.len(),
            overall_risk
        );
    } else {
        println!(
            "PLAN ANALYSIS ({} steps, risk: {})\n",
            results.len(),
            overall_risk
        );
    }

    for r in results {
        print_step(r, llm);
    }

    // Print suggested order if there are any non-low risk steps
    let has_risky = results
        .iter()
        .any(|r| r.risk != RiskLevel::Low || !r.callers.is_empty());
    if has_risky {
        print_suggested_order(results);
    }
}

/// Print a single step result with appropriate marker.
fn print_step(r: &StepResult, llm: bool) {
    let hash_str = r.hash.as_deref().unwrap_or("");
    let hash_display = if hash_str.is_empty() {
        String::new()
    } else {
        format!(" [{}]", hash_str)
    };

    match r.risk {
        RiskLevel::Low => {
            let reason = if r.action == PlanAction::Add {
                "no conflicts"
            } else if r.callers.is_empty() {
                "0 callers"
            } else {
                "no conflicts"
            };
            let marker = if llm { "OK" } else { "\u{2713}" };
            println!(
                "  {} Step {}: {} {}{} -- {}",
                marker,
                r.step_number,
                r.action.label(),
                r.name,
                hash_display,
                reason,
            );
        }
        RiskLevel::Medium => {
            let marker = if llm { "WARN" } else { "\u{26a0}" };
            println!(
                "  {} Step {}: {} {}{} -- {} callers will need updates",
                marker,
                r.step_number,
                r.action.label(),
                r.name,
                hash_display,
                r.callers.len(),
            );
            print_callers(&r.callers);
        }
        RiskLevel::High => {
            let verb = if r.action == PlanAction::Remove || r.action == PlanAction::Rename {
                "will break"
            } else {
                "will need updates"
            };
            let marker = if llm { "FAIL" } else { "\u{2717}" };
            println!(
                "  {} Step {}: {} {}{} -- {} callers {}",
                marker,
                r.step_number,
                r.action.label(),
                r.name,
                hash_display,
                r.callers.len(),
                verb,
            );
            print_callers(&r.callers);
        }
    }
}

/// Print caller details indented under a step.
fn print_callers(callers: &[CallerInfo]) {
    for c in callers {
        println!(
            "    Caller: {} [{}] {}:{}",
            c.name, c.hash, c.file_path, c.line,
        );
    }
}

/// Print a suggested execution order for the plan.
///
/// Orders: add steps first, then modify (with caller updates), then remove/rename last.
fn print_suggested_order(results: &[StepResult]) {
    let mut adds = Vec::new();
    let mut modifies = Vec::new();
    let mut removes = Vec::new();

    for r in results {
        match r.action {
            PlanAction::Add => adds.push(r.step_number),
            PlanAction::Modify => modifies.push(r.step_number),
            PlanAction::Rename | PlanAction::Remove => removes.push(r.step_number),
        }
    }

    let mut order_parts = Vec::new();
    for s in &adds {
        order_parts.push(format!("Step {}", s));
    }
    for s in &modifies {
        let r = &results[s - 1];
        if !r.callers.is_empty() {
            order_parts.push(format!("update callers of {}", r.name));
        }
        order_parts.push(format!("Step {}", s));
    }
    for s in &removes {
        let r = &results[s - 1];
        if !r.callers.is_empty() {
            order_parts.push(format!("update callers of {}", r.name));
        }
        order_parts.push(format!("Step {}", s));
    }

    if !order_parts.is_empty() {
        println!();
        println!("Suggested order: {}", order_parts.join(" -> "));
    }
}
