//! `keel validate-plan` — check a plan against the dependency graph before
//! execution, via the shared [`keel_enforce::validate_plan`] core.

use std::io::Read;
use std::path::Path;

use keel_core::sqlite::SqliteGraphStore;
use keel_enforce::circuit_breaker::{BreakerAction, CircuitBreaker};
use keel_enforce::validate_plan::{validate_plan, PlanFinding, PlanValidationResult};
use keel_output::OutputFormatter;

/// Pseudo-file the plan-finding circuit-breaker entries are recorded against.
///
/// A plan claim belongs to no source file, and using a real one would let a
/// later `keel compile` of that file clear the plan counter through the
/// breaker's provenance sweep. This marker can never collide with a path.
const PLAN_SCOPE: &str = "<plan>";

/// Run `keel validate-plan <file|-> [--strict]`.
///
/// Detected risk never gates: an analyzable plan exits 0 and the report is the
/// product. `--strict` is the single opt-in exception — it exits 1 when a live
/// (not circuit-breaker downgraded) `P001`/`P002` finding is present. Internal
/// errors (no initialized graph, unreadable plan input) still exit 2 per the
/// CLI contract.
pub fn run(formatter: &dyn OutputFormatter, verbose: bool, plan: String, strict: bool) -> i32 {
    let ctx = match super::open_repo("validate-plan") {
        Ok(x) => x,
        Err(code) => return code,
    };
    let store = ctx.store;

    let plan_text = match read_plan(&plan) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("keel validate-plan: failed to read plan: {}", e);
            return 2;
        }
    };

    let mut result = validate_plan(&store, &plan_text);
    apply_circuit_breaker(&store, &ctx.keel_dir, &mut result);

    if verbose {
        eprintln!(
            "keel validate-plan: {} action(s), {} symbol(s), {} finding(s) detected",
            result.actions.len(),
            result.symbols_detected,
            result.findings.len(),
        );
    }

    let rendered = formatter.format_validate_plan(&result);
    if !rendered.is_empty() {
        println!("{}", rendered);
    }

    if strict && result.has_live_findings() {
        1
    } else {
        0
    }
}

/// Route plan findings through the shared circuit breaker so an agent that
/// keeps re-submitting the same claim is nudged rather than deadlocked.
///
/// The fingerprint is the claim text, matching the breaker's "count fix
/// attempts, not compiles" rule: a reworded-but-still-wrong claim advances the
/// counter, a byte-identical re-submission does not. Three strikes downgrade
/// the finding to INFO, which drops it out of `--strict`'s exit code.
fn apply_circuit_breaker(
    store: &SqliteGraphStore,
    keel_dir: &Path,
    result: &mut PlanValidationResult,
) {
    let rows = match store.load_circuit_breaker() {
        Ok(rows) => rows,
        Err(_) => return,
    };
    // Nothing tracked and nothing to track: skip the persist round-trip.
    if rows.is_empty() && result.findings.is_empty() {
        return;
    }

    let config = keel_core::config::KeelConfig::load(keel_dir);
    let mut breaker = CircuitBreaker::with_max_failures(config.circuit_breaker.max_failures);
    breaker.import_state(&rows);

    let mut active: Vec<(String, String)> = Vec::with_capacity(result.findings.len());
    for finding in &mut result.findings {
        let ident = finding_identity(finding);
        let action = breaker.record_failure(&finding.code, &ident, &finding.claimed, PLAN_SCOPE);
        if action == BreakerAction::Downgrade {
            finding.downgraded = true;
            finding.severity = "INFO".to_string();
        }
        active.push((finding.code.clone(), ident));
    }
    breaker.reconcile_scope(PLAN_SCOPE, &active);

    let _ = store.save_circuit_breaker(&breaker.export_state());
}

/// Breaker identity for a finding: the stored hash when there is one (P002),
/// the symbol name otherwise (P001 has nothing stored to hash).
fn finding_identity(finding: &PlanFinding) -> String {
    if finding.hash.is_empty() {
        finding.symbol.clone()
    } else {
        finding.hash.clone()
    }
}

/// Read the plan from a file path, or stdin when `plan` is `-`.
fn read_plan(plan: &str) -> std::io::Result<String> {
    if plan == "-" {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        std::fs::read_to_string(plan)
    }
}
