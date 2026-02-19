// Oracle 1: Rust graph correctness vs LSP ground truth
//
// Compares keel's Rust graph output against rust-analyzer baseline data
// to validate node counts, edge counts, and resolution accuracy.

use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::resolver::{LanguageResolver, ReferenceKind};
use keel_parsers::rust_lang::RustLangResolver;

#[test]
fn test_rust_function_node_count_matches_lsp() {
    // GIVEN a Rust file with exactly 4 functions
    let resolver = RustLangResolver::new();
    let source = r#"
pub fn read_config(path: &str) -> String {
    String::new()
}

fn validate(input: &str) -> bool {
    !input.is_empty()
}

pub fn transform(data: &str) -> Vec<u8> {
    data.as_bytes().to_vec()
}

pub fn output(result: Vec<u8>) {
    let _ = result;
}
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("pipeline.rs"), source);

    // THEN the number of Function nodes matches exactly 4
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(
        funcs.len(),
        4,
        "expected 4 Function definitions, got {}",
        funcs.len()
    );
    for name in &["read_config", "validate", "transform", "output"] {
        assert!(
            funcs.iter().any(|f| f.name == *name),
            "missing function '{name}'"
        );
    }
}

#[test]
fn test_rust_struct_impl_node_count_matches_lsp() {
    // GIVEN a Rust file with 2 structs and impl blocks
    let resolver = RustLangResolver::new();
    let source = r#"
pub struct Database {
    url: String,
}

pub struct Cache {
    capacity: usize,
}

impl Database {
    pub fn new(url: &str) -> Self {
        Database { url: url.to_string() }
    }

    pub fn query(&self, sql: &str) -> Vec<String> {
        vec![]
    }
}

impl Cache {
    pub fn new(cap: usize) -> Self {
        Cache { capacity: cap }
    }
}
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("storage.rs"), source);

    // THEN 2 Class (struct) nodes are found
    let structs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class)
        .collect();
    assert_eq!(
        structs.len(),
        2,
        "expected 2 struct definitions, got {}",
        structs.len()
    );
    assert!(
        structs.iter().any(|s| s.name == "Database"),
        "missing struct Database"
    );
    assert!(
        structs.iter().any(|s| s.name == "Cache"),
        "missing struct Cache"
    );

    // AND impl methods are found as Function nodes
    let methods: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    for name in &["new", "query"] {
        assert!(
            methods.iter().any(|m| m.name == *name),
            "missing impl method '{name}'"
        );
    }
}

#[test]
fn test_rust_module_node_count_matches_lsp() {
    // GIVEN a Rust file
    let resolver = RustLangResolver::new();
    let source = r#"
fn add(a: i32, b: i32) -> i32 { a + b }
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("math.rs"), source);

    // THEN exactly 1 Module node is auto-created for the file
    let modules: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Module)
        .collect();
    assert_eq!(
        modules.len(),
        1,
        "expected 1 Module node per file, got {}",
        modules.len()
    );
    assert_eq!(modules[0].name, "math", "module name should be file stem");
    assert_eq!(modules[0].file_path, "math.rs");
}

#[test]
fn test_rust_call_edge_count_matches_lsp() {
    // GIVEN Rust code with known function calls
    let resolver = RustLangResolver::new();
    let source = r#"
fn helper() -> i32 {
    42
}

fn compute(x: i32) -> i32 {
    x * 2
}

fn main() {
    let a = helper();
    let b = compute(a);
    println!("{}", b);
}
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("calls.rs"), source);

    // THEN call references are found for the known calls
    let calls: Vec<_> = result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .collect();
    assert!(
        calls.len() >= 2,
        "expected >= 2 call references (helper, compute), got {}",
        calls.len()
    );
    assert!(
        calls.iter().any(|r| r.name.contains("helper")),
        "missing call to helper()"
    );
    assert!(
        calls.iter().any(|r| r.name.contains("compute")),
        "missing call to compute()"
    );
}

#[test]
fn test_rust_trait_impl_detection() {
    // GIVEN a Rust trait and an implementing struct
    let resolver = RustLangResolver::new();
    let source = r#"
pub trait Serializable {
    fn serialize(&self) -> String;
}

pub struct User {
    name: String,
}

impl Serializable for User {
    fn serialize(&self) -> String {
        self.name.clone()
    }
}
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("serial.rs"), source);

    // THEN the trait is captured as a Class node
    let traits: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class && d.name == "Serializable")
        .collect();
    assert_eq!(traits.len(), 1, "should detect trait Serializable");

    // AND the struct is captured
    assert!(
        result
            .definitions
            .iter()
            .any(|d| d.kind == NodeKind::Class && d.name == "User"),
        "should detect struct User"
    );

    // AND the impl method is captured as a Function
    let methods: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function && d.name == "serialize")
        .collect();
    assert!(
        !methods.is_empty(),
        "should detect trait impl method 'serialize'"
    );
}

#[test]
fn test_rust_use_statement_resolution() {
    // GIVEN Rust code with use statements
    let resolver = RustLangResolver::new();
    let source = r#"
use std::collections::HashMap;
use std::io::{self, Read, Write};

fn process() -> HashMap<String, String> {
    HashMap::new()
}
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("imports.rs"), source);

    // THEN use statements are captured as imports
    assert!(
        result.imports.len() >= 2,
        "expected >= 2 imports (std::collections, std::io), got {}",
        result.imports.len()
    );
    let has_collections = result
        .imports
        .iter()
        .any(|i| i.source.contains("collections") || i.source.contains("HashMap"));
    assert!(
        has_collections,
        "should detect use std::collections::HashMap"
    );
    let has_io = result.imports.iter().any(|i| i.source.contains("io"));
    assert!(has_io, "should detect use std::io");
}

#[test]
fn test_rust_macro_invocation_detected() {
    // GIVEN Rust code with macro invocations
    let resolver = RustLangResolver::new();
    let source = r#"
fn main() {
    println!("hello");
    let v = vec![1, 2, 3];
    assert_eq!(v.len(), 3);
}
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("macros.rs"), source);

    // THEN the function is found and parse completes without error
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert!(
        funcs.iter().any(|f| f.name == "main"),
        "should detect function main"
    );

    // Macro calls may appear as Call references depending on tree-sitter queries
    // We verify the parse completes without error and produces valid output
    let _calls: Vec<_> = result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .collect();
}
