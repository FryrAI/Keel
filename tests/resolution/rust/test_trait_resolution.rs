// Tests for Rust trait resolution (Spec 005 - Rust Resolution)
//
// Trait resolution requires type inference to determine which impl block
// provides a method for a given type. This is a Tier 2/3 feature that
// would be handled by rust-analyzer integration.
//
// Tests that can verify tree-sitter extraction of trait-related syntax
// are implemented; tests requiring type inference are marked #[ignore].

use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::resolver::LanguageResolver;
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
#[ignore = "BUG: trait method concrete resolution requires type inference"]
/// Trait method calls should resolve to the correct impl for the concrete type.
fn test_trait_method_concrete_resolution() {
    // Requires knowing the concrete type of the receiver to find the
    // correct impl block. This is rust-analyzer (Tier 3) territory.
}

#[test]
#[ignore = "BUG: dyn Trait resolution requires type inference"]
/// Dynamic dispatch (dyn Trait) should produce low-confidence edges to all implementors.
fn test_dyn_trait_resolution() {
    // Requires scanning all impl blocks for the trait and producing
    // candidate edges with low confidence for each implementor.
}

#[test]
#[ignore = "BUG: trait bound resolution requires generic type analysis"]
/// Trait bounds on generics should constrain resolution candidates.
fn test_trait_bound_resolution() {
    // `fn process<T: LanguageResolver>(resolver: &T)` requires understanding
    // generic bounds to filter resolution candidates.
}

#[test]
#[ignore = "BUG: supertrait resolution requires trait hierarchy analysis"]
/// Supertraits should include their methods in the resolution scope.
fn test_supertrait_method_resolution() {
    // Resolving methods from supertraits requires parsing the trait
    // hierarchy (trait AdvancedResolver: LanguageResolver + Debug).
}

#[test]
#[ignore = "BUG: associated type resolution requires type inference"]
/// Associated types in traits should be resolved to concrete types in implementations.
fn test_associated_type_resolution() {
    // Resolving `type Output;` to its concrete type requires finding
    // the relevant impl block and extracting the associated type.
}

#[test]
#[ignore = "BUG: where clause resolution requires type constraint analysis"]
/// Where clauses should constrain trait resolution the same as inline bounds.
fn test_where_clause_resolution() {
    // `fn process<T>(r: &T) where T: LanguageResolver + Send` requires
    // parsing where clauses to determine type constraints.
}
