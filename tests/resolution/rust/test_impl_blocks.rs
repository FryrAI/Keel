// Tests for Rust impl block resolution (Spec 005 - Rust Resolution)
//
// Tests that tree-sitter extracts methods from impl blocks as definitions
// and that type-aware resolution works for impl methods, trait impls,
// generic impls, and self.method() calls.
//
// Known limitation: method visibility detection inside impl blocks is
// incorrect because tree-sitter's @def.method.parent captures the impl_item
// node (not the inner function_item), so line_start points to the `impl` line
// rather than the `pub fn` line. This causes rust_is_public() to check the
// wrong line. Methods inside impl blocks always appear as private.

use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::resolver::{CallSite, LanguageResolver};
use keel_parsers::rust_lang::RustLangResolver;

#[test]
/// Methods in an inherent impl block should be extracted as definitions.
fn test_inherent_impl_method_extraction() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub struct GraphStore {
    data: Vec<String>,
}

impl GraphStore {
    pub fn new() -> Self {
        GraphStore { data: vec![] }
    }

    pub fn insert(&mut self, item: String) -> bool {
        self.data.push(item);
        true
    }
}
"#;
    let result = resolver.parse_file(Path::new("store.rs"), source);

    let struct_def = result.definitions.iter().find(|d| d.name == "GraphStore");
    assert!(struct_def.is_some(), "should find GraphStore struct");
    assert_eq!(struct_def.unwrap().kind, NodeKind::Class);

    let new_def = result.definitions.iter().find(|d| d.name == "new");
    assert!(new_def.is_some(), "should find new() method");
    assert_eq!(new_def.unwrap().kind, NodeKind::Function);

    let insert_def = result.definitions.iter().find(|d| d.name == "insert");
    assert!(insert_def.is_some(), "should find insert() method");

    let method_names: Vec<&str> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .map(|d| d.name.as_str())
        .collect();
    assert!(method_names.contains(&"new"), "should contain new");
    assert!(method_names.contains(&"insert"), "should contain insert");
}

#[test]
/// Impl block methods should have correct visibility detection.
fn test_impl_method_visibility_known_limitation() {
    let resolver = RustLangResolver::new();
    let source = r#"
struct Parser {
    input: String,
}

impl Parser {
    fn internal_parse(&self) -> bool {
        true
    }

    pub fn parse(&self) -> bool {
        self.internal_parse()
    }
}
"#;
    let result = resolver.parse_file(Path::new("parser.rs"), source);

    let internal = result
        .definitions
        .iter()
        .find(|d| d.name == "internal_parse");
    assert!(internal.is_some(), "should find internal_parse method");
    assert!(
        !internal.unwrap().is_public,
        "non-pub impl method should be private"
    );

    let public = result.definitions.iter().find(|d| d.name == "parse");
    assert!(public.is_some(), "should find parse method");
    assert!(
        public.unwrap().is_public,
        "pub impl method should be public"
    );
}

#[test]
/// Same-file call to an impl method should resolve via same-file lookup.
fn test_same_file_impl_method_call() {
    let resolver = RustLangResolver::new();
    let source = r#"
struct Store {
    data: Vec<i32>,
}

impl Store {
    fn new() -> Self {
        Store { data: vec![] }
    }
}

fn main() {
    let store = Store::new();
}
"#;
    let path = Path::new("store.rs");
    resolver.parse_file(path, source);

    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "store.rs".into(),
        line: 13,
        callee_name: "new".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "new() should resolve in same file");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "new");
    assert_eq!(edge.target_file, "store.rs");
}

#[test]
/// Trait definitions should be extracted with Class kind.
fn test_trait_definition_extraction() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub trait Resolver {
    fn resolve(&self) -> bool;
    fn language(&self) -> &str;
}
"#;
    let result = resolver.parse_file(Path::new("traits.rs"), source);

    let trait_def = result.definitions.iter().find(|d| d.name == "Resolver");
    assert!(trait_def.is_some(), "should find Resolver trait");
    assert_eq!(trait_def.unwrap().kind, NodeKind::Class);
    assert!(trait_def.unwrap().is_public, "pub trait should be public");
}

#[test]
/// Enum definitions should be extracted with Class kind.
fn test_enum_definition_extraction() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub enum Status {
    Active,
    Inactive,
    Pending,
}
"#;
    let result = resolver.parse_file(Path::new("types.rs"), source);

    let enum_def = result.definitions.iter().find(|d| d.name == "Status");
    assert!(enum_def.is_some(), "should find Status enum");
    assert_eq!(enum_def.unwrap().kind, NodeKind::Class);
    assert!(enum_def.unwrap().is_public, "pub enum should be public");
}

#[test]
/// Struct definitions should be extracted with Class kind.
fn test_struct_definition_extraction() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub struct Config {
    name: String,
    value: i32,
}

struct InternalState {
    data: Vec<u8>,
}
"#;
    let result = resolver.parse_file(Path::new("config.rs"), source);

    let config = result.definitions.iter().find(|d| d.name == "Config");
    assert!(config.is_some(), "should find Config struct");
    assert_eq!(config.unwrap().kind, NodeKind::Class);
    assert!(config.unwrap().is_public, "pub struct should be public");

    let internal = result
        .definitions
        .iter()
        .find(|d| d.name == "InternalState");
    assert!(internal.is_some(), "should find InternalState struct");
    assert!(
        !internal.unwrap().is_public,
        "struct without pub should be private"
    );
}

#[test]
/// Multiple impl blocks for the same type across files contribute methods.
fn test_multiple_impl_blocks() {
    let resolver = RustLangResolver::new();
    // File 1: struct + first impl block
    let source1 = r#"
pub struct Store {
    data: Vec<String>,
}

impl Store {
    pub fn new() -> Self {
        Store { data: vec![] }
    }
}
"#;
    // File 2: second impl block for same type
    let source2 = r#"
impl Store {
    pub fn insert(&mut self, item: String) {
        self.data.push(item);
    }
}
"#;
    let path1 = Path::new("store.rs");
    let path2 = Path::new("store_ext.rs");
    resolver.parse_file(path1, source1);
    resolver.parse_file(path2, source2);

    // self.insert() should resolve via the impl_map (both files contribute)
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "store.rs".into(),
        line: 8,
        callee_name: "insert".into(),
        receiver: Some("self".into()),
    });
    assert!(
        edge.is_some(),
        "self.insert() should resolve from merged impl blocks"
    );
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "insert");
    assert!(
        edge.confidence >= 0.80,
        "inherent impl self-call confidence should be >= 0.80, got {}",
        edge.confidence
    );
}

#[test]
/// `impl Trait for Type` should be tracked and enable trait method resolution.
fn test_trait_impl_resolution() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub trait Display {
    fn fmt(&self) -> String;
}

pub struct Point {
    x: i32,
    y: i32,
}

impl Display for Point {
    fn fmt(&self) -> String {
        format!("({}, {})", self.x, self.y)
    }
}
"#;
    let path = Path::new("point.rs");
    resolver.parse_file(path, source);

    // Resolve fmt() on a Point receiver via trait impl linkage
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "point.rs".into(),
        line: 15,
        callee_name: "fmt".into(),
        receiver: Some("Point".into()),
    });
    assert!(
        edge.is_some(),
        "Point.fmt() should resolve via impl Display for Point"
    );
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "fmt");
    assert!(
        edge.confidence >= 0.65,
        "trait impl confidence should be >= 0.65, got {}",
        edge.confidence
    );
}

#[test]
/// Generic impl blocks should resolve with lower confidence.
fn test_generic_impl_resolution() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub struct Wrapper<T> {
    inner: T,
}

impl<T> Wrapper<T> {
    pub fn new(value: T) -> Self {
        Wrapper { inner: value }
    }

    pub fn get(&self) -> &T {
        &self.inner
    }
}
"#;
    let path = Path::new("wrapper.rs");
    resolver.parse_file(path, source);

    // self.get() inside generic impl should resolve with lower confidence
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "wrapper.rs".into(),
        line: 12,
        callee_name: "get".into(),
        receiver: Some("self".into()),
    });
    assert!(edge.is_some(), "self.get() in generic impl should resolve");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "get");
    assert!(
        edge.confidence <= 0.65,
        "generic impl confidence should be <= 0.65 (lower than concrete), got {}",
        edge.confidence
    );
}

#[test]
/// self.method() calls should resolve to the current impl block.
fn test_self_method_call_resolution() {
    let resolver = RustLangResolver::new();
    let source = r#"
struct Parser {
    input: String,
}

impl Parser {
    fn tokenize(&self) -> Vec<String> {
        self.input.split(' ').map(|s| s.to_string()).collect()
    }

    fn parse(&self) -> bool {
        let tokens = self.tokenize();
        !tokens.is_empty()
    }
}
"#;
    let path = Path::new("parser.rs");
    resolver.parse_file(path, source);

    // self.tokenize() inside parse() should resolve to Parser's impl block
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "parser.rs".into(),
        line: 12,
        callee_name: "tokenize".into(),
        receiver: Some("self".into()),
    });
    assert!(
        edge.is_some(),
        "self.tokenize() should resolve to Parser impl block"
    );
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "tokenize");
    assert_eq!(edge.target_file, "parser.rs");
    assert!(
        edge.confidence >= 0.80,
        "self-call resolution confidence should be >= 0.80, got {}",
        edge.confidence
    );
}
