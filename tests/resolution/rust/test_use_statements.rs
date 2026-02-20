// Tests for Rust use statement resolution (Spec 005 - Rust Resolution)
//
// Tests that tree-sitter extracts use statements into Import structs
// and that the Rust resolver's call_edge resolution uses them.

use std::path::Path;

use keel_parsers::resolver::{CallSite, LanguageResolver};
use keel_parsers::rust_lang::RustLangResolver;

#[test]
/// Simple use statement should be extracted as an import with the full path.
fn test_simple_use_resolution() {
    let resolver = RustLangResolver::new();
    let source = r#"
use crate::graph::GraphNode;

pub fn process(node: &GraphNode) -> bool {
    true
}
"#;
    let result = resolver.parse_file(Path::new("processor.rs"), source);
    assert!(
        !result.imports.is_empty(),
        "should have at least one import"
    );
    let import = &result.imports[0];
    // After cross-file resolution, crate:: paths resolve to file paths (e.g. src/graph.rs)
    // or remain as the original path if no Cargo.toml is found
    assert!(
        import.source.contains("graph"),
        "import source should reference 'graph' module, got: {}",
        import.source
    );
    assert!(
        import.is_relative,
        "crate:: paths should be marked relative"
    );
}

#[test]
/// Call to an imported function should resolve via the import's source path.
fn test_use_enables_call_resolution() {
    let resolver = RustLangResolver::new();
    let source = r#"
use crate::utils::compute;

fn main() {
    compute();
}
"#;
    let path = Path::new("main.rs");
    let result = resolver.parse_file(path, source);

    // The import should have been extracted
    assert!(!result.imports.is_empty(), "should have imports");

    // Now resolve the call edge for `compute()`
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "main.rs".into(),
        line: 5,
        callee_name: "compute".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "compute() should resolve via import");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "compute");
    assert!(
        edge.confidence >= 0.50,
        "confidence should be at least 0.50, got: {}",
        edge.confidence
    );
}

#[test]
/// Grouped use statements should be extracted (tree-sitter captures the full path).
fn test_grouped_use_resolution() {
    let resolver = RustLangResolver::new();
    let source = r#"
use crate::graph::{GraphNode, GraphEdge, NodeKind};

pub fn process() -> bool { true }
"#;
    let result = resolver.parse_file(Path::new("processor.rs"), source);
    // Tree-sitter may extract this as one import with the full use_list path
    assert!(
        !result.imports.is_empty(),
        "grouped use should produce at least one import"
    );
}

#[test]
/// Use with `super` keyword should be marked as relative and resolve paths.
fn test_use_super_resolution() {
    let resolver = RustLangResolver::new();
    let source = r#"
use super::common::Config;

pub fn load() -> bool { true }
"#;
    let result = resolver.parse_file(Path::new("sub/module.rs"), source);
    assert!(!result.imports.is_empty(), "should have imports");
    let import = &result.imports[0];
    assert!(
        import.is_relative,
        "super:: paths should be marked as relative"
    );
    assert!(
        import.source.contains("super::") || import.source.contains("common"),
        "import source should reference super or resolved path, got: {}",
        import.source
    );
}

#[test]
/// Qualified path call (module::func) should resolve when module is imported.
fn test_qualified_call_via_import() {
    let resolver = RustLangResolver::new();
    let source = r#"
use crate::utils;

fn main() {
    utils::compute();
}
"#;
    let path = Path::new("main.rs");
    resolver.parse_file(path, source);

    // Resolve qualified call utils::compute
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "main.rs".into(),
        line: 5,
        callee_name: "utils::compute".into(),
        receiver: None,
    });
    // This works via the qualified path resolution in resolve_call_edge
    // which looks for module imports matching the prefix
    assert!(
        edge.is_some(),
        "utils::compute should resolve via module import"
    );
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "compute");
}

#[test]
/// Same-file function call should resolve without any imports.
fn test_same_file_call_no_import_needed() {
    let resolver = RustLangResolver::new();
    let source = r#"
fn helper() -> i32 { 42 }

fn main() {
    helper();
}
"#;
    let path = Path::new("main.rs");
    resolver.parse_file(path, source);

    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "main.rs".into(),
        line: 5,
        callee_name: "helper".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "same-file call should resolve");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "helper");
    assert_eq!(edge.target_file, "main.rs");
    assert!(
        edge.confidence >= 0.90,
        "same-file resolution should have high confidence, got: {}",
        edge.confidence
    );
}

#[test]
/// Glob use (`use module::*`) should create a wildcard import edge.
fn test_glob_use_resolution() {
    let resolver = RustLangResolver::new();
    let source = r#"
use crate::prelude::*;

fn main() {}
"#;
    let path = Path::new("glob_use.rs");
    let result = resolver.parse_file(path, source);

    // Should have a wildcard import with the module path (minus ::*)
    let glob_imp = result.imports.iter().find(|i| i.source.contains("prelude"));
    assert!(
        glob_imp.is_some(),
        "should have prelude wildcard import, got: {:?}",
        result.imports.iter().map(|i| &i.source).collect::<Vec<_>>()
    );
    let glob_imp = glob_imp.unwrap();
    assert!(
        glob_imp.imported_names.is_empty(),
        "wildcard import should have empty imported_names, got: {:?}",
        glob_imp.imported_names
    );
    assert!(glob_imp.is_relative, "crate:: import should be relative");
}

#[test]
/// Use statement with alias should track the renamed import.
fn test_use_with_alias() {
    let resolver = RustLangResolver::new();
    let source = r#"
use crate::utils::compute as calc;

fn main() {
    calc();
}
"#;
    let path = Path::new("alias.rs");
    let result = resolver.parse_file(path, source);

    // The alias "calc" should be in imported_names
    let has_alias = result
        .imports
        .iter()
        .any(|i| i.imported_names.contains(&"calc".to_string()));
    assert!(
        has_alias,
        "alias 'calc' should be tracked in imported_names, got: {:?}",
        result
            .imports
            .iter()
            .map(|i| &i.imported_names)
            .collect::<Vec<_>>()
    );

    // Call to calc() should resolve via the aliased import
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "alias.rs".into(),
        line: 5,
        callee_name: "calc".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "calc() should resolve via alias import");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "calc");
}

#[test]
/// Use with `self` keyword should resolve to the module itself.
fn test_use_self_resolution() {
    let resolver = RustLangResolver::new();
    let source = r#"
use crate::graph::{self, GraphNode};

fn main() {
    let _n = GraphNode::new();
}
"#;
    let path = Path::new("self_use.rs");
    let result = resolver.parse_file(path, source);

    // Should have both "graph" (from self) and "GraphNode" in imports
    let names: Vec<String> = result
        .imports
        .iter()
        .flat_map(|i| i.imported_names.clone())
        .collect();
    assert!(
        names.contains(&"GraphNode".to_string()),
        "should import GraphNode, got: {:?}",
        names
    );
    assert!(
        names.contains(&"graph".to_string()),
        "self should import module name 'graph', got: {:?}",
        names
    );
}
