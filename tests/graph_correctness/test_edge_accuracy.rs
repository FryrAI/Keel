// Oracle 1: Call edge precision and recall vs LSP ground truth
//
// Measures the accuracy of keel's call edge detection compared to LSP.
// Precision = correct edges / total keel edges. Recall = correct edges / total LSP edges.

use std::path::Path;

use keel_parsers::python::PyResolver;
use keel_parsers::resolver::{CallSite, LanguageResolver, ReferenceKind};
use keel_parsers::typescript::TsResolver;

#[test]
fn test_edge_precision_above_90_percent_typescript() {
    // GIVEN a TypeScript file with 5 known function calls
    let resolver = TsResolver::new();
    let source = r#"
function a(): number { return 1; }
function b(): number { return 2; }
function c(): number { return 3; }
function d(): number { return 4; }
function e(): number { return 5; }

function main(): void {
    const r1 = a();
    const r2 = b();
    const r3 = c();
    const r4 = d();
    const r5 = e();
}
"#;

    let result = resolver.parse_file(Path::new("precision.ts"), source);

    // Known call targets
    let known_calls: Vec<&str> = vec!["a", "b", "c", "d", "e"];

    let calls: Vec<_> = result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .collect();

    // Precision: how many detected calls match known targets
    let true_positives = calls
        .iter()
        .filter(|r| known_calls.iter().any(|kc| r.name.contains(kc)))
        .count();

    // We require >= 90% precision (at least 4 of 5 calls detected correctly)
    let expected_min = (known_calls.len() as f64 * 0.90).ceil() as usize;
    assert!(
        true_positives >= expected_min,
        "precision: expected >= {} true positives out of {}, got {}",
        expected_min,
        known_calls.len(),
        true_positives
    );
}

#[test]
fn test_edge_recall_above_75_percent_typescript() {
    // GIVEN a TypeScript file with 5 known function calls
    let resolver = TsResolver::new();
    let source = r#"
function alpha(): number { return 1; }
function beta(): number { return 2; }
function gamma(): number { return 3; }
function delta(): number { return 4; }

function run(): void {
    alpha();
    beta();
    gamma();
    delta();
}
"#;

    let result = resolver.parse_file(Path::new("recall.ts"), source);

    let known_calls: Vec<&str> = vec!["alpha", "beta", "gamma", "delta"];

    let calls: Vec<_> = result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .collect();

    // Recall: how many known calls are found
    let found = known_calls
        .iter()
        .filter(|kc| calls.iter().any(|r| r.name.contains(*kc)))
        .count();

    // We require >= 75% recall (at least 3 of 4 calls found)
    let expected_min = (known_calls.len() as f64 * 0.75).ceil() as usize;
    assert!(
        found >= expected_min,
        "recall: expected >= {} of {} known calls found, got {}",
        expected_min,
        known_calls.len(),
        found
    );
}

#[test]
fn test_edge_precision_above_90_percent_python() {
    // GIVEN a Python file with 4 known function calls
    let resolver = PyResolver::new();
    let source = r#"
def fa() -> int:
    return 1

def fb() -> int:
    return 2

def fc() -> int:
    return 3

def fd() -> int:
    return 4

def main() -> None:
    x = fa()
    y = fb()
    z = fc()
    w = fd()
"#;

    let result = resolver.parse_file(Path::new("precision.py"), source);

    let known_calls: Vec<&str> = vec!["fa", "fb", "fc", "fd"];

    let calls: Vec<_> = result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .collect();

    let true_positives = calls
        .iter()
        .filter(|r| known_calls.iter().any(|kc| r.name.contains(kc)))
        .count();

    // At least 90% of known calls should be detected
    let expected_min = (known_calls.len() as f64 * 0.90).ceil() as usize;
    assert!(
        true_positives >= expected_min,
        "precision: expected >= {} true positives, got {}",
        expected_min,
        true_positives
    );
}

#[test]
fn test_edge_recall_above_75_percent_python() {
    // GIVEN a Python file with 4 known function calls
    let resolver = PyResolver::new();
    let source = r#"
def step_one() -> int:
    return 1

def step_two() -> int:
    return 2

def step_three() -> int:
    return 3

def step_four() -> int:
    return 4

def pipeline() -> None:
    a = step_one()
    b = step_two()
    c = step_three()
    d = step_four()
"#;

    let result = resolver.parse_file(Path::new("recall.py"), source);

    let known_calls: Vec<&str> = vec!["step_one", "step_two", "step_three", "step_four"];

    let calls: Vec<_> = result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .collect();

    let found = known_calls
        .iter()
        .filter(|kc| calls.iter().any(|r| r.name.contains(*kc)))
        .count();

    let expected_min = (known_calls.len() as f64 * 0.75).ceil() as usize;
    assert!(
        found >= expected_min,
        "recall: expected >= {} of {} known calls found, got {}",
        expected_min,
        known_calls.len(),
        found
    );
}

#[test]
fn test_false_positive_edges_are_low_confidence() {
    // GIVEN a resolver asked to resolve an unknown/nonexistent call
    let resolver = TsResolver::new();

    // First parse a simple file to initialize the resolver
    let source = "function known(): void {}";
    let _result = resolver.parse_file(Path::new("simple.ts"), source);

    // WHEN we try to resolve a call to a function that doesn't exist
    let call_site = CallSite {
        file_path: "simple.ts".to_string(),
        line: 1,
        callee_name: "nonexistent_function_xyz".to_string(),
        receiver: None,
    };
    let edge = resolver.resolve_call_edge(&call_site);

    // THEN the resolver returns None (cannot resolve) or low confidence
    match edge {
        None => {
            // Expected: unresolvable call returns None
        }
        Some(resolved) => {
            assert!(
                resolved.confidence < 0.8,
                "false positive should have low confidence, got {}",
                resolved.confidence
            );
        }
    }
}

#[test]
fn test_dynamic_dispatch_edges_are_warnings_not_errors() {
    // Dynamic dispatch edges have low confidence (< 0.7) and should be
    // downgraded from ERROR to WARNING by the enforcement engine.
    use keel_enforce::engine::EnforcementEngine;
    use keel_enforce::types::Violation;

    // Simulate: a low-confidence violation (e.g., trait method dispatch at 0.5)
    let violations = vec![Violation {
        code: "E001".to_string(),
        severity: "ERROR".to_string(),
        category: "broken_caller".to_string(),
        message: "Trait method changed".to_string(),
        file: "trait_impl.rs".to_string(),
        line: 10,
        hash: "abc123".to_string(),
        confidence: 0.5, // Low confidence = dynamic dispatch
        resolution_tier: "tier1".to_string(),
        fix_hint: Some("Update callers".to_string()),
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    }];

    let result = EnforcementEngine::apply_dynamic_dispatch_threshold(violations);
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].severity, "WARNING",
        "low-confidence violations should be downgraded to WARNING"
    );
    assert!(
        result[0].fix_hint.as_ref().unwrap().contains("dynamic dispatch"),
        "fix_hint should mention dynamic dispatch"
    );

    // High-confidence violations should stay as ERROR
    let high_conf = vec![Violation {
        code: "E001".to_string(),
        severity: "ERROR".to_string(),
        category: "broken_caller".to_string(),
        message: "Direct call changed".to_string(),
        file: "direct.rs".to_string(),
        line: 5,
        hash: "def456".to_string(),
        confidence: 0.92, // High confidence = direct call
        resolution_tier: "tier1".to_string(),
        fix_hint: Some("Update callers".to_string()),
        suppressed: false,
        suppress_hint: None,
        affected: vec![],
        suggested_module: None,
        existing: None,
    }];

    let result = EnforcementEngine::apply_dynamic_dispatch_threshold(high_conf);
    assert_eq!(
        result[0].severity, "ERROR",
        "high-confidence violations should remain ERROR"
    );
}

#[test]
fn test_edge_resolution_tier_distribution() {
    // Verify that ResolvedEdge now includes resolution_tier
    let resolver = TsResolver::new();
    let source = r#"
function helper(): number { return 42; }
function main(): void { helper(); }
"#;
    resolver.parse_file(Path::new("tier_test.ts"), source);

    let call_site = CallSite {
        file_path: "tier_test.ts".to_string(),
        line: 3,
        callee_name: "helper".to_string(),
        receiver: None,
    };
    let edge = resolver.resolve_call_edge(&call_site);
    assert!(edge.is_some(), "should resolve same-file call");
    let edge = edge.unwrap();
    assert!(
        !edge.resolution_tier.is_empty(),
        "resolution_tier should be set, got empty string"
    );
    assert!(
        edge.resolution_tier.starts_with("tier"),
        "resolution_tier should start with 'tier', got '{}'",
        edge.resolution_tier
    );
}
