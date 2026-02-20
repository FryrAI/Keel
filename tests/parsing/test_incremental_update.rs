// Tests for incremental parsing updates (Spec 001 - Tree-sitter Foundation)
//
// Since there is no explicit incremental API in the parsers (they re-parse
// whole files), we test by parsing a file, modifying it, re-parsing, and
// verifying that the delta is correctly reflected in the results.

use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::resolver::LanguageResolver;
use keel_parsers::typescript::TsResolver;

#[test]
/// Modifying a single function body should change that function's body_text
/// while leaving the other function unchanged.
fn test_incremental_single_function_change() {
    let resolver = TsResolver::new();
    let path = Path::new("incremental.ts");

    let source_v1 = r#"
function alpha(x: number): number {
    return x + 1;
}

function beta(y: string): string {
    return y.toUpperCase();
}
"#;
    let result_v1 = resolver.parse_file(path, source_v1);
    let funcs_v1: Vec<_> = result_v1
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs_v1.len(), 2);

    let alpha_v1 = funcs_v1.iter().find(|d| d.name == "alpha").unwrap();
    let beta_v1 = funcs_v1.iter().find(|d| d.name == "beta").unwrap();

    // Change alpha's body, leave beta untouched
    let source_v2 = r#"
function alpha(x: number): number {
    return x * 2;
}

function beta(y: string): string {
    return y.toUpperCase();
}
"#;
    let result_v2 = resolver.parse_file(path, source_v2);
    let funcs_v2: Vec<_> = result_v2
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs_v2.len(), 2);

    let alpha_v2 = funcs_v2.iter().find(|d| d.name == "alpha").unwrap();
    let beta_v2 = funcs_v2.iter().find(|d| d.name == "beta").unwrap();

    // Alpha body changed
    assert_ne!(alpha_v1.body_text, alpha_v2.body_text);
    // Beta body stayed the same
    assert_eq!(beta_v1.body_text, beta_v2.body_text);
}

#[test]
/// Adding a new function to an existing file should increase definition count.
fn test_incremental_new_function_added() {
    let resolver = TsResolver::new();
    let path = Path::new("add_func.ts");

    let source_v1 = r#"
function alpha(x: number): number {
    return x + 1;
}
"#;
    let result_v1 = resolver.parse_file(path, source_v1);
    let funcs_v1: Vec<_> = result_v1
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs_v1.len(), 1);

    let source_v2 = r#"
function alpha(x: number): number {
    return x + 1;
}

function beta(y: string): string {
    return y.toUpperCase();
}
"#;
    let result_v2 = resolver.parse_file(path, source_v2);
    let funcs_v2: Vec<_> = result_v2
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs_v2.len(), 2);

    let names: Vec<&str> = funcs_v2.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"beta"));
}

#[test]
/// Deleting a function from a file should reduce definition count.
fn test_incremental_function_deleted() {
    let resolver = TsResolver::new();
    let path = Path::new("del_func.ts");

    let source_v1 = r#"
function alpha(x: number): number {
    return x + 1;
}

function beta(y: string): string {
    return y.toUpperCase();
}
"#;
    let result_v1 = resolver.parse_file(path, source_v1);
    let funcs_v1: Vec<_> = result_v1
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs_v1.len(), 2);

    // Remove beta
    let source_v2 = r#"
function alpha(x: number): number {
    return x + 1;
}
"#;
    let result_v2 = resolver.parse_file(path, source_v2);
    let funcs_v2: Vec<_> = result_v2
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs_v2.len(), 1);
    assert_eq!(funcs_v2[0].name, "alpha");
}

#[test]
/// Parsing the same content at a different path should reflect the new
/// file_path in definitions.
fn test_incremental_file_rename() {
    let resolver = TsResolver::new();

    let source = r#"
function greet(name: string): string {
    return "Hello, " + name;
}
"#;
    let path_a = Path::new("old.ts");
    let result_a = resolver.parse_file(path_a, source);
    let funcs_a: Vec<_> = result_a
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs_a.len(), 1);
    assert!(funcs_a[0].file_path.contains("old.ts"));

    let path_b = Path::new("new.ts");
    let result_b = resolver.parse_file(path_b, source);
    let funcs_b: Vec<_> = result_b
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs_b.len(), 1);
    assert!(funcs_b[0].file_path.contains("new.ts"));

    // Content is the same, path changed
    assert_ne!(funcs_a[0].file_path, funcs_b[0].file_path);
}

#[test]
/// Re-parsing with only whitespace/comment changes should produce the same
/// set of definitions (same names, same count).
fn test_incremental_no_structural_change() {
    let resolver = TsResolver::new();
    let path = Path::new("whitespace.ts");

    let source_v1 = r#"
function greet(name: string): string {
    return "Hello, " + name;
}
"#;
    let result_v1 = resolver.parse_file(path, source_v1);

    // Add whitespace and comments only
    let source_v2 = r#"
// This is a comment
function greet(name: string): string {

    return "Hello, " + name;

}
"#;
    let result_v2 = resolver.parse_file(path, source_v2);

    // Same definitions by name and count
    assert_eq!(result_v1.definitions.len(), result_v2.definitions.len());
    // Find greet in both
    let greet_v1 = result_v1
        .definitions
        .iter()
        .find(|d| d.name == "greet")
        .unwrap();
    let greet_v2 = result_v2
        .definitions
        .iter()
        .find(|d| d.name == "greet")
        .unwrap();
    assert_eq!(greet_v1.name, greet_v2.name);
}

#[test]
/// Parsing two separate files should produce definitions from both.
fn test_incremental_new_file_added() {
    let resolver = TsResolver::new();

    let source_a = r#"
function alpha(x: number): number { return x; }
"#;
    let source_b = r#"
function beta(y: string): string { return y; }
"#;

    let result_a = resolver.parse_file(Path::new("a.ts"), source_a);
    let result_b = resolver.parse_file(Path::new("b.ts"), source_b);

    let funcs_a: Vec<_> = result_a
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs_a.len(), 1);
    assert_eq!(funcs_a[0].name, "alpha");

    let funcs_b: Vec<_> = result_b
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs_b.len(), 1);
    assert_eq!(funcs_b[0].name, "beta");

    // resolve_definitions should return cached results for both (includes module)
    let defs_a = resolver.resolve_definitions(Path::new("a.ts"));
    let defs_b = resolver.resolve_definitions(Path::new("b.ts"));
    assert!(defs_a.iter().any(|d| d.name == "alpha"));
    assert!(defs_b.iter().any(|d| d.name == "beta"));
}

#[test]
/// resolve_definitions for a file that was never parsed should return empty.
fn test_incremental_file_deleted() {
    let resolver = TsResolver::new();

    // Parse a file first
    let source = "function hello(): void {}";
    resolver.parse_file(Path::new("exists.ts"), source);
    assert!(resolver
        .resolve_definitions(Path::new("exists.ts"))
        .iter()
        .any(|d| d.name == "hello"));

    // Ask for definitions of a file that was never parsed (simulates deletion
    // — the resolver has no cached data for this path)
    let defs = resolver.resolve_definitions(Path::new("never_parsed.ts"));
    assert!(defs.is_empty());
}
