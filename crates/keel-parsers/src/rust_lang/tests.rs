use std::path::Path;

use super::*;

#[test]
fn test_rust_resolver_docstring_extraction() {
    let resolver = RustLangResolver::new();
    let source = "/// Says hello.\npub fn greet(name: &str) -> String {\n    format!(\"Hello, {}!\", name)\n}\n";
    let result = resolver.parse_file(Path::new("test.rs"), source);
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == keel_core::types::NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(
        funcs[0].docstring.as_deref(),
        Some("Says hello."),
        "RustLangResolver should preserve docstrings from tree-sitter"
    );
}

#[test]
fn test_rust_resolver_parse_function() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
"#;
    let result = resolver.parse_file(Path::new("test.rs"), source);
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == keel_core::types::NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(funcs[0].name, "greet");
    assert!(funcs[0].is_public);
    assert!(funcs[0].type_hints_present);
}

#[test]
fn test_rust_resolver_private_function() {
    let resolver = RustLangResolver::new();
    let source = r#"
fn internal_helper(x: i32) -> i32 {
    x + 1
}
"#;
    let result = resolver.parse_file(Path::new("test.rs"), source);
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == keel_core::types::NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert!(!funcs[0].is_public);
}

#[test]
fn test_rust_resolver_caches_results() {
    let resolver = RustLangResolver::new();
    let source = "fn hello() {}";
    let path = Path::new("cached.rs");
    resolver.parse_file(path, source);
    let defs = resolver.resolve_definitions(path);
    let funcs: Vec<_> = defs
        .iter()
        .filter(|d| d.kind == keel_core::types::NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
}

#[test]
fn test_rust_resolver_same_file_call_edge() {
    let resolver = RustLangResolver::new();
    let source = r#"
fn helper() -> i32 { 1 }
fn main() { helper(); }
"#;
    let path = Path::new("edge.rs");
    resolver.parse_file(path, source);
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "edge.rs".into(),
        line: 3,
        callee_name: "helper".into(),
        receiver: None,
    });
    assert!(edge.is_some());
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "helper");
    assert!(edge.confidence >= 0.90);
}

#[test]
fn test_rust_resolver_parses_use_imports() {
    let resolver = RustLangResolver::new();
    let source = r#"
use crate::store::GraphStore;
use super::utils::helper;

fn main() {
    let s = GraphStore::new();
    helper();
}
"#;
    let path = Path::new("test_imports.rs");
    let result = resolver.parse_file(path, source);
    assert!(
        result.imports.len() >= 2,
        "expected at least 2 imports, got {}",
        result.imports.len()
    );
    let store_imp = result.imports.iter().find(|i| {
        i.source.contains("store") && i.imported_names.contains(&"GraphStore".to_string())
    });
    assert!(
        store_imp.is_some(),
        "should have store import with GraphStore name"
    );
    assert!(store_imp.unwrap().is_relative);
}

#[test]
fn test_rust_resolver_cross_file_call_via_import() {
    let resolver = RustLangResolver::new();
    let source = r#"
use crate::store::GraphStore;

fn main() {
    GraphStore::new();
}
"#;
    let path = Path::new("test_cross.rs");
    resolver.parse_file(path, source);
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "test_cross.rs".into(),
        line: 5,
        callee_name: "GraphStore".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "should resolve GraphStore via use import");
}
