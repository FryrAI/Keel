// Oracle 1: Cross-language graph correctness
//
// Validates that keel produces correct graphs for projects containing
// multiple languages, ensuring no interference between language resolvers.

use std::path::Path;

use keel_core::hash::compute_hash_disambiguated;
use keel_core::types::NodeKind;
use keel_parsers::go::GoResolver;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::LanguageResolver;
use keel_parsers::rust_lang::RustLangResolver;
use keel_parsers::treesitter::detect_language;
use keel_parsers::typescript::TsResolver;

#[test]
/// Parsing files in all 4 languages should yield independent, correct results.
fn test_mixed_project_total_node_count() {
    let ts = TsResolver::new();
    let py = PyResolver::new();
    let go = GoResolver::new();
    let rs = RustLangResolver::new();

    // Each file has exactly 2 functions
    let ts_result = ts.parse_file(
        Path::new("app.ts"),
        "function tsA(): void {} function tsB(): void {}",
    );
    let py_result = py.parse_file(
        Path::new("app.py"),
        "def pyA() -> None:\n    pass\n\ndef pyB() -> None:\n    pass\n",
    );
    let go_result = go.parse_file(
        Path::new("app.go"),
        "package main\nfunc GoA() {}\nfunc GoB() {}",
    );
    let rs_result = rs.parse_file(Path::new("app.rs"), "fn rs_a() {} fn rs_b() {}");

    // Total definition count = sum of per-language counts (no duplication)
    let total = ts_result.definitions.len()
        + py_result.definitions.len()
        + go_result.definitions.len()
        + rs_result.definitions.len();

    assert!(ts_result.definitions.len() >= 2, "TS should have >= 2 defs");
    assert!(py_result.definitions.len() >= 2, "Py should have >= 2 defs");
    assert!(go_result.definitions.len() >= 2, "Go should have >= 2 defs");
    assert!(rs_result.definitions.len() >= 2, "Rust should have >= 2 defs");
    assert!(total >= 8, "total definitions should be >= 8, got {total}");
}

#[test]
/// Parsing separate language files should not create cross-language references.
fn test_mixed_project_no_cross_language_edges() {
    let ts = TsResolver::new();
    let py = PyResolver::new();

    let ts_result = ts.parse_file(Path::new("utils.ts"), "function helper(): void {}");
    let py_result = py.parse_file(
        Path::new("utils.py"),
        "def helper() -> None:\n    pass\n",
    );

    // TS references should only refer to TS file
    for r in &ts_result.references {
        assert_eq!(
            r.file_path, "utils.ts",
            "TS reference should be in TS file, got {}",
            r.file_path
        );
    }
    // Py references should only refer to Py file
    for r in &py_result.references {
        assert_eq!(
            r.file_path, "utils.py",
            "Py reference should be in Py file, got {}",
            r.file_path
        );
    }
}

#[test]
/// Per-language definition counts should be accurate regardless of other languages.
fn test_mixed_project_per_language_accuracy() {
    let ts = TsResolver::new();
    let py = PyResolver::new();
    let go = GoResolver::new();
    let rs = RustLangResolver::new();

    let ts_result = ts.parse_file(
        Path::new("three.ts"),
        "function a(): void {} function b(): void {} function c(): void {}",
    );
    let py_result = py.parse_file(
        Path::new("two.py"),
        "def x() -> None:\n    pass\n\ndef y() -> None:\n    pass\n",
    );
    let go_result = go.parse_file(Path::new("one.go"), "package main\nfunc Single() {}");
    let rs_result = rs.parse_file(Path::new("four.rs"), "fn w() {} fn x() {} fn y() {} fn z() {}");

    let ts_funcs = ts_result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .count();
    let py_funcs = py_result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .count();
    let go_funcs = go_result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .count();
    let rs_funcs = rs_result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .count();

    assert_eq!(ts_funcs, 3, "TS should detect 3 functions");
    assert_eq!(py_funcs, 2, "Python should detect 2 functions");
    assert_eq!(go_funcs, 1, "Go should detect 1 function");
    assert_eq!(rs_funcs, 4, "Rust should detect 4 functions");
}

#[test]
/// File extension detection should assign the correct language.
fn test_language_detection_by_extension() {
    assert_eq!(detect_language(Path::new("app.ts")), Some("typescript"));
    assert_eq!(detect_language(Path::new("component.tsx")), Some("tsx"));
    assert_eq!(detect_language(Path::new("main.py")), Some("python"));
    assert_eq!(detect_language(Path::new("server.go")), Some("go"));
    assert_eq!(detect_language(Path::new("lib.rs")), Some("rust"));
    assert_eq!(detect_language(Path::new("index.js")), Some("javascript"));

    // Non-source files should return None
    assert_eq!(detect_language(Path::new("README.md")), None);
    assert_eq!(detect_language(Path::new("data.json")), None);
    assert_eq!(detect_language(Path::new("style.css")), None);
}

#[test]
/// Same function name in different languages should produce different hashes.
fn test_same_function_name_different_languages() {
    let ts = TsResolver::new();
    let py = PyResolver::new();

    let ts_result = ts.parse_file(
        Path::new("process.ts"),
        "function process(data: string): string { return data; }",
    );
    let py_result = py.parse_file(
        Path::new("process.py"),
        "def process(data: str) -> str:\n    return data\n",
    );

    let ts_def = ts_result
        .definitions
        .iter()
        .find(|d| d.name == "process")
        .expect("TS should have 'process' definition");
    let py_def = py_result
        .definitions
        .iter()
        .find(|d| d.name == "process")
        .expect("Python should have 'process' definition");

    // Different file paths => different hashes (disambiguated by file path)
    let ts_hash = compute_hash_disambiguated(
        &ts_def.signature,
        &ts_def.body_text,
        ts_def.docstring.as_deref().unwrap_or(""),
        &ts_def.file_path,
    );
    let py_hash = compute_hash_disambiguated(
        &py_def.signature,
        &py_def.body_text,
        py_def.docstring.as_deref().unwrap_or(""),
        &py_def.file_path,
    );
    assert_ne!(
        ts_hash, py_hash,
        "same-named functions in different languages should have different hashes"
    );
}

#[test]
/// Every parsed file should produce at least one definition.
fn test_mixed_project_graph_completeness() {
    let ts = TsResolver::new();
    let py = PyResolver::new();
    let go = GoResolver::new();
    let rs = RustLangResolver::new();

    let files: Vec<(&dyn LanguageResolver, &str, &str)> = vec![
        (&ts, "a.ts", "function a(): void {}"),
        (&py, "b.py", "def b() -> None:\n    pass\n"),
        (&go, "c.go", "package main\nfunc C() {}"),
        (&rs, "d.rs", "fn d() {}"),
    ];

    for (resolver, path, source) in files {
        let result = resolver.parse_file(Path::new(path), source);
        assert!(
            !result.definitions.is_empty(),
            "file {path} should have at least one definition"
        );
    }
}
