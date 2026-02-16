// Tests for Rust mod declaration resolution (Spec 005 - Rust Resolution)
//
// All tests in this module require filesystem access to resolve `mod foo;`
// declarations to their corresponding files (foo.rs or foo/mod.rs).
// This is a Tier 2 feature not available at the tree-sitter parser layer.

use std::path::Path;

use keel_parsers::resolver::LanguageResolver;
use keel_parsers::rust_lang::RustLangResolver;

#[test]
/// Inline mod blocks should parse and extract definitions from the inner scope.
fn test_inline_mod_block_parsing() {
    let resolver = RustLangResolver::new();
    let source = r#"
mod inner {
    pub fn helper() -> i32 {
        42
    }
}

fn main() {
    inner::helper();
}
"#;
    let result = resolver.parse_file(Path::new("lib.rs"), source);

    // tree-sitter should capture the function inside the inline mod block
    let helper = result.definitions.iter().find(|d| d.name == "helper");
    assert!(helper.is_some(), "should find helper() inside inline mod block");

    let main_fn = result.definitions.iter().find(|d| d.name == "main");
    assert!(main_fn.is_some(), "should find main() function");
}

#[test]
#[ignore = "BUG: mod declaration file resolution requires filesystem walking"]
/// `mod foo;` declaration should resolve to foo.rs or foo/mod.rs.
fn test_mod_declaration_resolves_to_file() {
    // Requires filesystem access: given lib.rs with `mod parser;` and a file
    // src/parser.rs, the mod declaration should resolve to src/parser.rs.
}

#[test]
#[ignore = "BUG: mod file preference requires filesystem access"]
/// `mod foo;` should prefer foo.rs over foo/mod.rs (Rust 2018+ edition).
fn test_mod_declaration_prefers_file_over_dir() {
    // Requires filesystem walking. When both src/parser.rs and
    // src/parser/mod.rs exist, Rust 2018+ prefers src/parser.rs.
}

#[test]
#[ignore = "BUG: nested mod chain resolution requires filesystem traversal"]
/// Nested mod declarations should resolve through the directory hierarchy.
fn test_nested_mod_resolution() {
    // Requires traversing lib.rs -> mod graph -> graph/mod.rs -> mod store ->
    // graph/store.rs. This is filesystem-level resolution beyond tree-sitter.
}

#[test]
#[ignore = "BUG: #[path] attribute parsing for mod resolution not implemented"]
/// `#[path = "..."]` attribute should override the default file path resolution.
fn test_mod_path_attribute() {
    // Requires parsing `#[path = "custom/my_module.rs"]` attribute and using
    // it to override the default mod declaration file resolution.
}
