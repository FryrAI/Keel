// Tests for Rust tree-sitter parser (Spec 001 - Tree-sitter Foundation)

use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::resolver::{LanguageResolver, ReferenceKind};
use keel_parsers::rust_lang::RustLangResolver;

#[test]
/// Parsing a Rust file with a fn item should produce a Function node.
fn test_rust_parse_function() {
    let resolver = RustLangResolver::new();
    let source = r#"
fn process(data: &[u8]) -> Result<Output, Error> {
    todo!()
}
"#;
    let result = resolver.parse_file(Path::new("test.rs"), source);
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1, "expected exactly 1 function definition");
    assert_eq!(funcs[0].name, "process");
    assert_eq!(funcs[0].kind, NodeKind::Function);
}

#[test]
/// Parsing a Rust struct should produce a Class node.
fn test_rust_parse_struct() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub struct GraphStore {
    db: Connection,
}
"#;
    let result = resolver.parse_file(Path::new("test.rs"), source);
    let classes: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class)
        .collect();
    assert_eq!(classes.len(), 1, "expected exactly 1 class (struct) definition");
    assert_eq!(classes[0].name, "GraphStore");
    assert_eq!(classes[0].kind, NodeKind::Class);
}

#[test]
/// Parsing Rust impl blocks should produce Function nodes for methods inside them.
fn test_rust_parse_impl_block() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub struct GraphStore {
    db: Connection,
}

impl GraphStore {
    pub fn new(path: &str) -> Self {
        GraphStore { db: Connection::open(path) }
    }

    pub fn get(&self, id: u64) -> Option<Node> {
        None
    }
}
"#;
    let result = resolver.parse_file(Path::new("test.rs"), source);
    // Methods inside impl blocks should be captured as Function nodes
    let methods: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert!(
        methods.len() >= 2,
        "expected at least 2 methods (new, get), got {}",
        methods.len()
    );
    let new_method = methods.iter().find(|d| d.name == "new");
    assert!(new_method.is_some(), "should find method 'new'");
    let get_method = methods.iter().find(|d| d.name == "get");
    assert!(get_method.is_some(), "should find method 'get'");
}

#[test]
/// Parsing Rust trait definitions should produce a Class node (traits map to Class).
fn test_rust_parse_trait() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub trait LanguageResolver {
    fn resolve(&self, path: &str) -> Vec<String>;
    fn language(&self) -> &str;
}
"#;
    let result = resolver.parse_file(Path::new("test.rs"), source);
    let traits: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class && d.name == "LanguageResolver")
        .collect();
    assert_eq!(traits.len(), 1, "expected trait to be captured as Class node");
    assert_eq!(traits[0].name, "LanguageResolver");
    assert_eq!(traits[0].kind, NodeKind::Class);
}

#[test]
/// Parsing Rust use statements should produce imports.
fn test_rust_parse_use_statements() {
    let resolver = RustLangResolver::new();
    let source = r#"
use crate::graph::{GraphNode, GraphEdge};
use std::collections::HashMap;

fn main() {}
"#;
    let result = resolver.parse_file(Path::new("test.rs"), source);
    assert!(
        result.imports.len() >= 2,
        "expected at least 2 imports, got {}",
        result.imports.len()
    );
    // Check that the crate import is marked as relative
    let crate_import = result
        .imports
        .iter()
        .find(|i| i.source.contains("graph"));
    assert!(crate_import.is_some(), "should have crate::graph import");
    assert!(
        crate_import.unwrap().is_relative,
        "crate:: import should be marked as relative"
    );
    // Check that std import is not relative
    let std_import = result
        .imports
        .iter()
        .find(|i| i.source.contains("HashMap") || i.source.contains("collections"));
    assert!(std_import.is_some(), "should have std::collections import");
    assert!(
        !std_import.unwrap().is_relative,
        "std:: import should not be marked as relative"
    );
}

#[test]
/// Parsing Rust code with function calls should produce call references.
fn test_rust_parse_call_sites() {
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
    let result = resolver.parse_file(Path::new("test.rs"), source);
    let calls: Vec<_> = result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .collect();
    assert!(
        calls.len() >= 2,
        "expected at least 2 call references (helper, compute), got {}",
        calls.len()
    );
    let helper_call = calls.iter().find(|r| r.name.contains("helper"));
    assert!(helper_call.is_some(), "should have a reference to helper()");
    let compute_call = calls.iter().find(|r| r.name.contains("compute"));
    assert!(
        compute_call.is_some(),
        "should have a reference to compute()"
    );
}

#[test]
/// Parsing Rust enum definitions should produce a Class node (enums map to Class).
fn test_rust_parse_enum() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub enum NodeKind {
    Function,
    Class,
    Module,
}
"#;
    let result = resolver.parse_file(Path::new("test.rs"), source);
    let enums: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class && d.name == "NodeKind")
        .collect();
    assert_eq!(enums.len(), 1, "expected enum to be captured as Class node");
    assert_eq!(enums[0].name, "NodeKind");
    assert_eq!(enums[0].kind, NodeKind::Class);
}

#[test]
/// Parsing Rust doc comments (///) -- tree-sitter does not extract doc comments
/// into the docstring field, so docstring should be None.
fn test_rust_parse_doc_comments() {
    let resolver = RustLangResolver::new();
    let source = r#"
/// Processes input data and returns the result.
fn process() -> i32 {
    42
}
"#;
    let result = resolver.parse_file(Path::new("test.rs"), source);
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.name == "process")
        .collect();
    assert_eq!(funcs.len(), 1, "expected function 'process' to be captured");
    // The tree-sitter layer sets docstring to None (line 180 of treesitter/mod.rs).
    // A higher-level pass may populate it later, but at this level it is None.
    assert!(
        funcs[0].docstring.is_none(),
        "tree-sitter layer does not extract doc comments; docstring should be None"
    );
}
