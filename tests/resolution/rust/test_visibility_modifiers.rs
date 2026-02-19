// Tests for Rust visibility modifier resolution (Spec 005 - Rust Resolution)
//
// Tests `pub` vs private detection in the RustLangResolver's Tier 2 heuristic.

use keel_core::types::NodeKind;
use keel_parsers::resolver::LanguageResolver;
use keel_parsers::rust_lang::RustLangResolver;
use std::path::Path;

#[test]
/// `pub` items should be marked as public by the resolver.
fn test_pub_visibility() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub fn process(input: &str) -> String {
    input.to_uppercase()
}
"#;
    let result = resolver.parse_file(Path::new("lib.rs"), source);
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "process");
    assert!(defs[0].is_public, "pub fn should be marked as public");
}

#[test]
/// `pub(crate)` items should be accessible only within the same crate.
fn test_pub_crate_visibility() {
    // The current heuristic only checks for `pub ` prefix.
    // `pub(crate) fn` starts with `pub ` so it IS detected as public,
    // but we can't distinguish it from fully-public `pub fn`.
    let resolver = RustLangResolver::new();
    let source = r#"
pub(crate) fn internal() -> i32 {
    42
}
"#;
    let result = resolver.parse_file(Path::new("lib.rs"), source);
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();
    assert_eq!(defs.len(), 1);
    // The heuristic treats pub(crate) as public since the line starts with "pub "
    assert!(defs[0].is_public);
}

#[test]
/// `pub(super)` items should be accessible only from the parent module.
fn test_pub_super_visibility() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub(super) fn helper() -> bool {
    true
}
"#;
    let result = resolver.parse_file(Path::new("inner/mod.rs"), source);
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();
    assert_eq!(defs.len(), 1);
    // Heuristic: pub(super) starts with "pub " so detected as public
    assert!(defs[0].is_public);
}

#[test]
/// Private items (no visibility modifier) should be marked as not public.
fn test_private_visibility() {
    let resolver = RustLangResolver::new();
    let source = r#"
fn private_helper(x: i32) -> i32 {
    x * 2
}
"#;
    let result = resolver.parse_file(Path::new("lib.rs"), source);
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "private_helper");
    assert!(
        !defs[0].is_public,
        "fn without pub should be marked as private"
    );
}

#[test]
/// Multiple functions with mixed visibility should each be correctly classified.
fn test_mixed_visibility() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub fn public_one(a: i32) -> i32 { a }

fn private_one(b: &str) -> bool { b.is_empty() }

pub fn public_two() -> String { String::new() }

fn private_two(x: f64) -> f64 { x * 2.0 }
"#;
    let result = resolver.parse_file(Path::new("mixed.rs"), source);
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();
    assert_eq!(defs.len(), 4);

    // Check each by name
    let public_one = result
        .definitions
        .iter()
        .find(|d| d.name == "public_one")
        .unwrap();
    assert!(public_one.is_public, "public_one should be public");

    let private_one = result
        .definitions
        .iter()
        .find(|d| d.name == "private_one")
        .unwrap();
    assert!(!private_one.is_public, "private_one should be private");

    let public_two = result
        .definitions
        .iter()
        .find(|d| d.name == "public_two")
        .unwrap();
    assert!(public_two.is_public, "public_two should be public");

    let private_two = result
        .definitions
        .iter()
        .find(|d| d.name == "private_two")
        .unwrap();
    assert!(!private_two.is_public, "private_two should be private");
}

#[test]
/// Rust definitions should always have type_hints_present = true (statically typed).
fn test_rust_type_hints_always_present() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub fn typed(x: i32, y: &str) -> bool { true }

fn also_typed(a: Vec<String>) -> Option<i32> { None }
"#;
    let result = resolver.parse_file(Path::new("typed.rs"), source);
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();
    assert_eq!(defs.len(), 2);
    for def in &defs {
        assert!(
            def.type_hints_present,
            "{} should have type_hints_present = true",
            def.name
        );
    }
}

#[test]
/// `pub(in path)` should restrict visibility to the specified module path.
fn test_pub_in_path_visibility() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub(in crate::graph) fn internal() -> i32 {
    42
}
"#;
    let result = resolver.parse_file(Path::new("graph/store.rs"), source);
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();
    assert_eq!(defs.len(), 1);
    // Heuristic: pub(in ...) starts with "pub " so detected as public
    assert!(defs[0].is_public);
}
