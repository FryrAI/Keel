// Oracle 1: Node completeness -- all functions/classes found vs LSP
//
// Validates that keel detects all function and class definitions that the
// LSP reports, ensuring no structural elements are missed during parsing.

use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::go::GoResolver;
use keel_parsers::python::PyResolver;
use keel_parsers::resolver::LanguageResolver;
use keel_parsers::rust_lang::RustLangResolver;
use keel_parsers::typescript::TsResolver;

#[test]
fn test_all_top_level_functions_found_typescript() {
    // GIVEN a TypeScript file with 3 known top-level function definitions
    let resolver = TsResolver::new();
    let source = r#"
function alpha(x: number): number { return x; }
function beta(s: string): string { return s; }
function gamma(): void { console.log("hi"); }
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("funcs.ts"), source);

    // THEN every top-level function is found
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert!(
        funcs.len() >= 3,
        "expected >= 3 functions, got {}",
        funcs.len()
    );
    for name in &["alpha", "beta", "gamma"] {
        assert!(
            funcs.iter().any(|f| f.name == *name),
            "missing function '{name}'"
        );
    }
}

#[test]
fn test_all_class_methods_found_typescript() {
    // GIVEN a TypeScript class with 3 methods
    let resolver = TsResolver::new();
    let source = r#"
class Calculator {
    add(a: number, b: number): number { return a + b; }
    subtract(a: number, b: number): number { return a - b; }
    multiply(a: number, b: number): number { return a * b; }
}
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("calc.ts"), source);

    // THEN the class and all 3 methods are found
    let class_defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class)
        .collect();
    assert!(
        class_defs.iter().any(|d| d.name == "Calculator"),
        "should find class Calculator"
    );

    let methods: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    for name in &["add", "subtract", "multiply"] {
        assert!(
            methods.iter().any(|m| m.name == *name),
            "missing method '{name}'"
        );
    }
}

#[test]
fn test_all_top_level_functions_found_python() {
    // GIVEN a Python file with 3 top-level function definitions
    let resolver = PyResolver::new();
    let source = r#"
def fetch_data(url: str) -> dict:
    return {}

def process_data(data: dict) -> list:
    return []

def save_results(results: list) -> None:
    pass
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("pipeline.py"), source);

    // THEN all 3 functions are found
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert!(
        funcs.len() >= 3,
        "expected >= 3 functions, got {}",
        funcs.len()
    );
    for name in &["fetch_data", "process_data", "save_results"] {
        assert!(
            funcs.iter().any(|f| f.name == *name),
            "missing function '{name}'"
        );
    }
}

#[test]
fn test_all_class_methods_found_python() {
    // GIVEN a Python class with methods
    let resolver = PyResolver::new();
    let source = r#"
class DataStore:
    def __init__(self, path: str) -> None:
        self.path = path

    def load(self) -> dict:
        return {}

    def save(self, data: dict) -> None:
        pass
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("store.py"), source);

    // THEN class and methods are found
    let classes: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class)
        .collect();
    assert!(
        classes.iter().any(|c| c.name == "DataStore"),
        "should find class DataStore"
    );

    let methods: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    for name in &["__init__", "load", "save"] {
        assert!(
            methods.iter().any(|m| m.name == *name),
            "missing method '{name}'"
        );
    }
}

#[test]
fn test_exported_functions_detected_go() {
    // GIVEN a Go file with exported (capitalized) and unexported functions
    let resolver = GoResolver::new();
    let source = r#"
package service

func ProcessData(data []byte) error {
    return nil
}

func handleInternal(msg string) {
}

func Validate(input string) bool {
    return true
}
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("service.go"), source);

    // THEN both exported and unexported functions are found with correct is_public
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert!(
        funcs.len() >= 3,
        "expected >= 3 functions, got {}",
        funcs.len()
    );

    let exported = funcs.iter().find(|f| f.name == "ProcessData");
    assert!(
        exported.is_some(),
        "should find exported function ProcessData"
    );
    assert!(exported.unwrap().is_public, "ProcessData should be public");

    let unexported = funcs.iter().find(|f| f.name == "handleInternal");
    assert!(
        unexported.is_some(),
        "should find unexported function handleInternal"
    );
    assert!(
        !unexported.unwrap().is_public,
        "handleInternal should not be public"
    );

    let validate = funcs.iter().find(|f| f.name == "Validate");
    assert!(validate.is_some(), "should find exported function Validate");
    assert!(validate.unwrap().is_public, "Validate should be public");
}

#[test]
fn test_impl_methods_detected_rust() {
    // GIVEN a Rust struct with impl block containing methods
    let resolver = RustLangResolver::new();
    let source = r#"
pub struct Config {
    debug: bool,
}

impl Config {
    pub fn new() -> Self {
        Config { debug: false }
    }

    pub fn set_debug(&mut self, val: bool) {
        self.debug = val;
    }

    fn internal_reset(&mut self) {
        self.debug = false;
    }
}
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("config.rs"), source);

    // THEN the struct and all impl methods are captured
    let structs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class && d.name == "Config")
        .collect();
    assert_eq!(structs.len(), 1, "expected struct Config");

    let methods: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    for name in &["new", "set_debug", "internal_reset"] {
        assert!(
            methods.iter().any(|m| m.name == *name),
            "missing impl method '{name}'"
        );
    }
}

#[test]
fn test_no_phantom_nodes_in_graph() {
    // GIVEN a file with exactly 2 functions and nothing else
    let resolver = TsResolver::new();
    let source = r#"
function one(): void {}
function two(): void {}
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("minimal.ts"), source);

    // THEN there are exactly 2 Function definitions (no phantom extras)
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(
        funcs.len(),
        2,
        "expected exactly 2 function definitions, got {} ({:?})",
        funcs.len(),
        funcs.iter().map(|f| &f.name).collect::<Vec<_>>()
    );
    assert!(funcs.iter().any(|f| f.name == "one"), "missing 'one'");
    assert!(funcs.iter().any(|f| f.name == "two"), "missing 'two'");
}
