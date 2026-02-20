//! MCP fix handler — compiles files, then returns violations with fix plans.

use serde_json::Value;

use keel_enforce::fix_generator::generate_fix_plans;
use keel_enforce::types::FixResult;

use crate::mcp::{internal_err, lock_store, JsonRpcError, SharedEngine, SharedStore};
use crate::parse_shared::parse_file_to_index;

pub(crate) fn handle_fix(
    store: &SharedStore,
    engine: &SharedEngine,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let files: Vec<String> = params
        .as_ref()
        .and_then(|p| p.get("files").cloned())
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    let file_indexes: Vec<_> = files
        .iter()
        .filter_map(|p| parse_file_to_index(p))
        .collect();

    let mut engine = engine.lock().map_err(|_| JsonRpcError {
        code: -32603,
        message: "Engine lock poisoned".into(),
    })?;

    let compile_result = engine.compile(&file_indexes);

    // Collect all violations (errors + warnings) and generate fix plans
    let all_violations: Vec<_> = compile_result
        .errors
        .iter()
        .chain(compile_result.warnings.iter())
        .collect();

    let store = lock_store(store)?;
    let plans = generate_fix_plans(&all_violations, &*store);

    let result = FixResult {
        version: env!("CARGO_PKG_VERSION").into(),
        command: "fix".into(),
        violations_addressed: plans.len() as u32,
        files_affected: {
            let mut files_set = std::collections::HashSet::new();
            for plan in &plans {
                for action in &plan.actions {
                    files_set.insert(action.file.clone());
                }
            }
            files_set.len() as u32
        },
        plans,
    };

    serde_json::to_value(result).map_err(internal_err)
}
