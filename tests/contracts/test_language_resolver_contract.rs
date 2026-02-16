/// Contract tests for the LanguageResolver trait.
///
/// These tests verify that all 4 language resolvers implement the
/// LanguageResolver trait correctly. Each resolver must:
/// - Return the correct language name
/// - Accept arbitrary content in parse_file without panicking
/// - Return valid (possibly empty) results from all methods
use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::resolver::LanguageResolver;
use keel_parsers::typescript::TsResolver;
use keel_parsers::python::PyResolver;
use keel_parsers::go::GoResolver;
use keel_parsers::rust_lang::RustLangResolver;

// ---------------------------------------------------------------------------
// TypeScript resolver contract
// ---------------------------------------------------------------------------

#[test]
fn ts_resolver_returns_correct_language() {
    let resolver = TsResolver::new();
    assert_eq!(resolver.language(), "typescript");
}

#[test]
fn ts_resolver_parse_file_empty_content() {
    let resolver = TsResolver::new();
    let result = resolver.parse_file(Path::new("test.ts"), "");
    // Only the auto-created Module node; no other definitions
    let non_mod: Vec<_> = result.definitions.iter().filter(|d| d.kind != NodeKind::Module).collect();
    assert!(non_mod.is_empty());
    assert!(result.references.is_empty());
    assert!(result.imports.is_empty());
}

#[test]
fn ts_resolver_parse_file_simple_function() {
    let resolver = TsResolver::new();
    let content = "export function greet(name: string): string { return `Hello ${name}`; }";
    let result = resolver.parse_file(Path::new("test.ts"), content);
    let funcs: Vec<_> = result.definitions.iter().filter(|d| d.kind == NodeKind::Function).collect();
    assert!(!funcs.is_empty(), "Should find at least one function definition");
    assert_eq!(funcs[0].name, "greet");
}

#[test]
fn ts_resolver_resolve_definitions_returns_vec() {
    let resolver = TsResolver::new();
    let defs = resolver.resolve_definitions(Path::new("nonexistent.ts"));
    let _ = defs;
}

#[test]
fn ts_resolver_resolve_references_returns_vec() {
    let resolver = TsResolver::new();
    let refs = resolver.resolve_references(Path::new("nonexistent.ts"));
    let _ = refs;
}

#[test]
fn ts_resolver_resolve_call_edge_returns_option() {
    let resolver = TsResolver::new();
    let call_site = keel_parsers::resolver::CallSite {
        file_path: "test.ts".to_string(),
        line: 1,
        callee_name: "greet".to_string(),
        receiver: None,
    };
    let edge = resolver.resolve_call_edge(&call_site);
    let _ = edge;
}

// ---------------------------------------------------------------------------
// Python resolver contract
// ---------------------------------------------------------------------------

#[test]
fn py_resolver_returns_correct_language() {
    let resolver = PyResolver::new();
    assert_eq!(resolver.language(), "python");
}

#[test]
fn py_resolver_parse_file_empty_content() {
    let resolver = PyResolver::new();
    let result = resolver.parse_file(Path::new("test.py"), "");
    let non_mod: Vec<_> = result.definitions.iter().filter(|d| d.kind != NodeKind::Module).collect();
    assert!(non_mod.is_empty());
    assert!(result.references.is_empty());
    assert!(result.imports.is_empty());
}

#[test]
fn py_resolver_parse_file_simple_function() {
    let resolver = PyResolver::new();
    let content = "def greet(name: str) -> str:\n    return f'Hello {name}'";
    let result = resolver.parse_file(Path::new("test.py"), content);
    let funcs: Vec<_> = result.definitions.iter().filter(|d| d.kind == NodeKind::Function).collect();
    assert!(!funcs.is_empty(), "Should find at least one function definition");
    assert_eq!(funcs[0].name, "greet");
}

#[test]
fn py_resolver_resolve_definitions_returns_vec() {
    let resolver = PyResolver::new();
    let defs = resolver.resolve_definitions(Path::new("nonexistent.py"));
    let _ = defs;
}

#[test]
fn py_resolver_resolve_references_returns_vec() {
    let resolver = PyResolver::new();
    let refs = resolver.resolve_references(Path::new("nonexistent.py"));
    let _ = refs;
}

#[test]
fn py_resolver_resolve_call_edge_returns_option() {
    let resolver = PyResolver::new();
    let call_site = keel_parsers::resolver::CallSite {
        file_path: "test.py".to_string(),
        line: 1,
        callee_name: "greet".to_string(),
        receiver: None,
    };
    let edge = resolver.resolve_call_edge(&call_site);
    let _ = edge;
}

// ---------------------------------------------------------------------------
// Go resolver contract
// ---------------------------------------------------------------------------

#[test]
fn go_resolver_returns_correct_language() {
    let resolver = GoResolver::new();
    assert_eq!(resolver.language(), "go");
}

#[test]
fn go_resolver_parse_file_empty_content() {
    let resolver = GoResolver::new();
    let result = resolver.parse_file(Path::new("test.go"), "");
    let non_mod: Vec<_> = result.definitions.iter().filter(|d| d.kind != NodeKind::Module).collect();
    assert!(non_mod.is_empty());
    assert!(result.references.is_empty());
    assert!(result.imports.is_empty());
}

#[test]
fn go_resolver_parse_file_simple_function() {
    let resolver = GoResolver::new();
    let content = "package main\n\nfunc Greet(name string) string {\n\treturn \"Hello \" + name\n}";
    let result = resolver.parse_file(Path::new("test.go"), content);
    let funcs: Vec<_> = result.definitions.iter().filter(|d| d.kind == NodeKind::Function).collect();
    assert!(!funcs.is_empty(), "Should find at least one function definition");
    assert_eq!(funcs[0].name, "Greet");
}

#[test]
fn go_resolver_resolve_definitions_returns_vec() {
    let resolver = GoResolver::new();
    let defs = resolver.resolve_definitions(Path::new("nonexistent.go"));
    let _ = defs;
}

#[test]
fn go_resolver_resolve_references_returns_vec() {
    let resolver = GoResolver::new();
    let refs = resolver.resolve_references(Path::new("nonexistent.go"));
    let _ = refs;
}

#[test]
fn go_resolver_resolve_call_edge_returns_option() {
    let resolver = GoResolver::new();
    let call_site = keel_parsers::resolver::CallSite {
        file_path: "test.go".to_string(),
        line: 1,
        callee_name: "Greet".to_string(),
        receiver: None,
    };
    let edge = resolver.resolve_call_edge(&call_site);
    let _ = edge;
}

// ---------------------------------------------------------------------------
// Rust resolver contract
// ---------------------------------------------------------------------------

#[test]
fn rust_resolver_returns_correct_language() {
    let resolver = RustLangResolver::new();
    assert_eq!(resolver.language(), "rust");
}

#[test]
fn rust_resolver_parse_file_empty_content() {
    let resolver = RustLangResolver::new();
    let result = resolver.parse_file(Path::new("test.rs"), "");
    let non_mod: Vec<_> = result.definitions.iter().filter(|d| d.kind != NodeKind::Module).collect();
    assert!(non_mod.is_empty());
    assert!(result.references.is_empty());
    assert!(result.imports.is_empty());
}

#[test]
fn rust_resolver_parse_file_simple_function() {
    let resolver = RustLangResolver::new();
    let content = "pub fn greet(name: &str) -> String {\n    format!(\"Hello {}\", name)\n}";
    let result = resolver.parse_file(Path::new("test.rs"), content);
    let funcs: Vec<_> = result.definitions.iter().filter(|d| d.kind == NodeKind::Function).collect();
    assert!(!funcs.is_empty(), "Should find at least one function definition");
    assert_eq!(funcs[0].name, "greet");
}

#[test]
fn rust_resolver_resolve_definitions_returns_vec() {
    let resolver = RustLangResolver::new();
    let defs = resolver.resolve_definitions(Path::new("nonexistent.rs"));
    let _ = defs;
}

#[test]
fn rust_resolver_resolve_references_returns_vec() {
    let resolver = RustLangResolver::new();
    let refs = resolver.resolve_references(Path::new("nonexistent.rs"));
    let _ = refs;
}

#[test]
fn rust_resolver_resolve_call_edge_returns_option() {
    let resolver = RustLangResolver::new();
    let call_site = keel_parsers::resolver::CallSite {
        file_path: "test.rs".to_string(),
        line: 1,
        callee_name: "greet".to_string(),
        receiver: None,
    };
    let edge = resolver.resolve_call_edge(&call_site);
    let _ = edge;
}

// ---------------------------------------------------------------------------
// Cross-resolver trait object test
// ---------------------------------------------------------------------------

#[test]
fn all_resolvers_are_object_safe() {
    let resolvers: Vec<Box<dyn LanguageResolver>> = vec![
        Box::new(TsResolver::new()),
        Box::new(PyResolver::new()),
        Box::new(GoResolver::new()),
        Box::new(RustLangResolver::new()),
    ];

    let expected_languages = ["typescript", "python", "go", "rust"];
    for (resolver, expected) in resolvers.iter().zip(expected_languages.iter()) {
        assert_eq!(resolver.language(), *expected);
    }
}
