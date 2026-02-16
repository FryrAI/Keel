// Tests for the LanguageResolver trait contract (Spec 001 - Tree-sitter Foundation)

use std::path::Path;

use keel_parsers::go::GoResolver;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::LanguageResolver;
use keel_parsers::rust_lang::RustLangResolver;
use keel_parsers::typescript::TsResolver;

#[test]
/// Every LanguageResolver implementation must return the correct language identifier.
fn test_resolver_language_identifier() {
    let ts = TsResolver::new();
    let py = PyResolver::new();
    let go = GoResolver::new();
    let rs = RustLangResolver::new();

    assert_eq!(ts.language(), "typescript");
    assert_eq!(py.language(), "python");
    assert_eq!(go.language(), "go");
    assert_eq!(rs.language(), "rust");
}

#[test]
#[ignore = "BUG: LanguageResolver trait has no supports_extension method"]
/// Every LanguageResolver must correctly identify supported file extensions.
fn test_resolver_supported_extensions() {
    // The LanguageResolver trait does not define a supports_extension() method.
    // File extension detection is handled by treesitter::detect_language() instead.
    // This test would require adding a method to the frozen contract.
}

#[test]
/// parse_file must return a consistent set of definitions for a given input.
fn test_resolver_parse_file_consistency() {
    let ts = TsResolver::new();
    let source = "export function greet(name: string): string { return name; }";
    let path = Path::new("test.ts");

    let result1 = ts.parse_file(path, source);
    let result2 = ts.parse_file(path, source);

    assert_eq!(
        result1.definitions.len(),
        result2.definitions.len(),
        "two parses of same input must produce same definition count"
    );
    for (d1, d2) in result1.definitions.iter().zip(result2.definitions.iter()) {
        assert_eq!(d1.name, d2.name, "definition names must match");
        assert_eq!(d1.kind, d2.kind, "definition kinds must match");
        assert_eq!(d1.signature, d2.signature, "signatures must match");
        assert_eq!(d1.line_start, d2.line_start);
        assert_eq!(d1.line_end, d2.line_end);
    }

    assert_eq!(
        result1.references.len(),
        result2.references.len(),
        "reference counts must match"
    );
    assert_eq!(
        result1.imports.len(),
        result2.imports.len(),
        "import counts must match"
    );
}

#[test]
/// parse_file on an empty file should return an empty set of definitions.
fn test_resolver_parse_empty_file() {
    let ts = TsResolver::new();
    let py = PyResolver::new();
    let go = GoResolver::new();
    let rs = RustLangResolver::new();

    let ts_result = ts.parse_file(Path::new("empty.ts"), "");
    let py_result = py.parse_file(Path::new("empty.py"), "");
    let go_result = go.parse_file(Path::new("empty.go"), "package main\n");
    let rs_result = rs.parse_file(Path::new("empty.rs"), "");

    use keel_core::types::NodeKind;
    // Each file gets an auto-created Module node; no other definitions expected
    let ts_non_mod: Vec<_> = ts_result.definitions.iter().filter(|d| d.kind != NodeKind::Module).collect();
    let py_non_mod: Vec<_> = py_result.definitions.iter().filter(|d| d.kind != NodeKind::Module).collect();
    let go_non_mod: Vec<_> = go_result.definitions.iter().filter(|d| d.kind != NodeKind::Module).collect();
    let rs_non_mod: Vec<_> = rs_result.definitions.iter().filter(|d| d.kind != NodeKind::Module).collect();

    assert!(ts_non_mod.is_empty(), "empty TS file should have no non-module definitions");
    assert!(py_non_mod.is_empty(), "empty Python file should have no non-module definitions");
    assert!(go_non_mod.is_empty(), "empty Go file should have no non-module definitions");
    assert!(rs_non_mod.is_empty(), "empty Rust file should have no non-module definitions");
}

#[test]
/// parse_file on a file with syntax errors should return partial results.
fn test_resolver_parse_file_with_syntax_errors() {
    let ts = TsResolver::new();
    // First function is valid, second has syntax errors
    let source = r#"
function validFn(x: number): number { return x; }
function broken(x { {{{{ return; }
function alsoValid(y: string): string { return y; }
"#;
    let result = ts.parse_file(Path::new("partial.ts"), source);

    // tree-sitter is error-tolerant: it should still extract some definitions
    // At minimum validFn should be found; alsoValid may or may not depending
    // on how tree-sitter error-recovers
    assert!(
        result.definitions.iter().any(|d| d.name == "validFn"),
        "valid function before syntax error should be extracted: {:?}",
        result
            .definitions
            .iter()
            .map(|d| &d.name)
            .collect::<Vec<_>>()
    );
}

#[test]
/// resolve_call_edge should return a ResolvedEdge with confidence score.
fn test_resolver_resolve_call() {
    use keel_parsers::resolver::CallSite;

    let ts = TsResolver::new();

    // Parse a file with a function definition
    let source = r#"
export function foo(x: number): number { return x + 1; }
foo(42);
"#;
    ts.parse_file(Path::new("call_test.ts"), source);

    // Attempt to resolve a call to foo within the same file
    let call = CallSite {
        file_path: "call_test.ts".into(),
        line: 3,
        callee_name: "foo".into(),
        receiver: None,
    };

    let edge = ts.resolve_call_edge(&call);
    assert!(
        edge.is_some(),
        "same-file call to foo should resolve"
    );
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "foo");
    assert!(
        edge.confidence > 0.0 && edge.confidence <= 1.0,
        "confidence should be in (0.0, 1.0], got {}",
        edge.confidence
    );
}

#[test]
/// The LanguageResolver trait must be object-safe for dynamic dispatch.
fn test_resolver_trait_object_safety() {
    // Create Box<dyn LanguageResolver> for each implementation
    let resolvers: Vec<Box<dyn LanguageResolver>> = vec![
        Box::new(TsResolver::new()),
        Box::new(PyResolver::new()),
        Box::new(GoResolver::new()),
        Box::new(RustLangResolver::new()),
    ];

    // Verify dynamic dispatch works
    let expected_languages = ["typescript", "python", "go", "rust"];
    for (resolver, expected) in resolvers.iter().zip(expected_languages.iter()) {
        assert_eq!(
            resolver.language(),
            *expected,
            "dynamic dispatch should work for {}",
            expected
        );
    }

    // Verify they are Send + Sync (required by the trait)
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<TsResolver>();
    assert_send_sync::<PyResolver>();
    assert_send_sync::<GoResolver>();
    assert_send_sync::<RustLangResolver>();
}
