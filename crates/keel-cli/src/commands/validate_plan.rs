//! `keel validate-plan` — check a plan against the dependency graph before
//! execution, via the shared [`keel_enforce::validate_plan`] core.

use std::io::Read;

use keel_enforce::validate_plan::validate_plan;
use keel_output::OutputFormatter;

/// Run `keel validate-plan <file|->`. Always exits 0 — the report is the
/// product, not a gate.
pub fn run(formatter: &dyn OutputFormatter, verbose: bool, plan: String) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("keel validate-plan: failed to get current directory: {}", e);
            return 2;
        }
    };

    let keel_dir = keel_core::paths::keel_dir(&cwd);
    if !keel_dir.exists() {
        eprintln!("keel validate-plan: not initialized. Run `keel init` first.");
        return 2;
    }

    let plan_text = match read_plan(&plan) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("keel validate-plan: failed to read plan: {}", e);
            return 2;
        }
    };

    let db_path = keel_dir.join("graph.db");
    let store = match keel_core::sqlite::SqliteGraphStore::open(db_path.to_str().unwrap_or("")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("keel validate-plan: failed to open graph database: {}", e);
            return 2;
        }
    };

    let result = validate_plan(&store, &plan_text);
    if verbose {
        eprintln!(
            "keel validate-plan: {} action(s), {} symbol(s) detected",
            result.actions.len(),
            result.symbols_detected,
        );
    }

    let rendered = formatter.format_validate_plan(&result);
    if !rendered.is_empty() {
        println!("{}", rendered);
    }

    0
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
