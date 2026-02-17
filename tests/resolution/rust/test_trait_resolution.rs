// Tests for Rust trait resolution (Spec 005 - Rust Resolution)
//
// Tier 2 heuristics: text-based extraction of `impl Trait for Type` blocks
// enables method resolution on known concrete types (confidence 0.70) and
// dyn Trait dispatch (confidence 0.40). Full trait resolution is Tier 3.

use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::resolver::{CallSite, LanguageResolver};
use keel_parsers::rust_lang::RustLangResolver;

#[test]
/// Trait definitions should be extracted as Class kind definitions.
fn test_trait_definition_extraction() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub trait LanguageResolver {
    fn resolve(&self) -> bool;
    fn language(&self) -> &str;
}
"#;
    let result = resolver.parse_file(Path::new("resolver.rs"), source);

    let trait_def = result
        .definitions
        .iter()
        .find(|d| d.name == "LanguageResolver");
    assert!(trait_def.is_some(), "should find LanguageResolver trait");
    assert_eq!(trait_def.unwrap().kind, NodeKind::Class);
    assert!(
        trait_def.unwrap().is_public,
        "pub trait should be public"
    );
}

#[test]
/// Trait with default method implementations should extract the methods.
fn test_trait_default_method_extraction() {
    let resolver = RustLangResolver::new();
    let source = r#"
pub trait Validator {
    fn validate(&self) -> bool;

    fn is_valid(&self) -> bool {
        self.validate()
    }
}
"#;
    let result = resolver.parse_file(Path::new("validator.rs"), source);

    let trait_def = result.definitions.iter().find(|d| d.name == "Validator");
    assert!(trait_def.is_some(), "should find Validator trait");

    // tree-sitter should capture the default method implementation
    let is_valid = result.definitions.iter().find(|d| d.name == "is_valid");
    assert!(
        is_valid.is_some(),
        "should find is_valid default method"
    );
}

#[test]
/// Trait method calls on known concrete type should resolve via impl block.
/// Tier 2 heuristic: `impl Trait for Type` is extracted, and when the
/// receiver is the concrete type, resolution succeeds at confidence 0.70.
fn test_trait_method_concrete_resolution() {
    let resolver = RustLangResolver::new();
    let source = r#"
trait Greeter {
    fn greet(&self) -> String;
}

struct EnglishGreeter;

impl Greeter for EnglishGreeter {
    fn greet(&self) -> String {
        "Hello!".to_string()
    }
}

fn main() {
    let g = EnglishGreeter;
    g.greet();
}
"#;
    let path = Path::new("greeter.rs");
    resolver.parse_file(path, source);

    // Resolve greet() with receiver type "EnglishGreeter"
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "greeter.rs".into(),
        line: 16,
        callee_name: "greet".into(),
        receiver: Some("EnglishGreeter".into()),
    });
    assert!(
        edge.is_some(),
        "should resolve greet() on EnglishGreeter via trait impl"
    );
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "greet");
    assert!(
        (edge.confidence - 0.70).abs() < 0.01,
        "concrete trait resolution confidence should be 0.70, got {}",
        edge.confidence
    );
    assert_eq!(edge.resolution_tier, "tier2");
}

#[test]
/// Dynamic dispatch (dyn Trait) should produce low-confidence edges.
/// Tier 2 heuristic: when receiver is "dyn TraitName", scan all impl
/// blocks for the trait and return first candidate at confidence 0.40.
fn test_dyn_trait_resolution() {
    let resolver = RustLangResolver::new();
    let source = r#"
trait Formatter {
    fn format(&self, data: &str) -> String;
}

struct JsonFormatter;
struct XmlFormatter;

impl Formatter for JsonFormatter {
    fn format(&self, data: &str) -> String {
        format!("{{{}}}", data)
    }
}

impl Formatter for XmlFormatter {
    fn format(&self, data: &str) -> String {
        format!("<data>{}</data>", data)
    }
}

fn process(f: &dyn Formatter) {
    f.format("test");
}
"#;
    let path = Path::new("formatter.rs");
    resolver.parse_file(path, source);

    // Resolve format() with dyn Formatter receiver
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "formatter.rs".into(),
        line: 22,
        callee_name: "format".into(),
        receiver: Some("dyn Formatter".into()),
    });
    assert!(
        edge.is_some(),
        "should resolve format() on dyn Formatter to a candidate impl"
    );
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "format");
    // dyn trait: low confidence (0.40)
    assert!(
        (edge.confidence - 0.40).abs() < 0.01,
        "dyn trait resolution confidence should be 0.40, got {}",
        edge.confidence
    );
    assert_eq!(edge.resolution_tier, "tier2");
}

#[test]
#[ignore = "TIER3: requires generic constraint solving -- deferred by design"]
/// Trait bounds on generics should constrain resolution candidates.
fn test_trait_bound_resolution() {
    // `fn process<T: LanguageResolver>(resolver: &T)` requires understanding
    // generic bounds to filter resolution candidates.
}

#[test]
#[ignore = "TIER3: requires trait hierarchy traversal (rust-analyzer) -- deferred by design"]
/// Supertraits should include their methods in the resolution scope.
fn test_supertrait_method_resolution() {
    // Resolving methods from supertraits requires parsing the trait
    // hierarchy (trait AdvancedResolver: LanguageResolver + Debug).
}

#[test]
#[ignore = "TIER3: requires type-level inference (rust-analyzer) -- deferred by design"]
/// Associated types in traits should be resolved to concrete types in implementations.
fn test_associated_type_resolution() {
    // Resolving `type Output;` to its concrete type requires finding
    // the relevant impl block and extracting the associated type.
}

#[test]
#[ignore = "TIER3: requires where clause constraint analysis (rust-analyzer) -- deferred by design"]
/// Where clauses should constrain trait resolution the same as inline bounds.
fn test_where_clause_resolution() {
    // `fn process<T>(r: &T) where T: LanguageResolver + Send` requires
    // parsing where clauses to determine type constraints.
}
