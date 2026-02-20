// Tests for Rust mod declaration resolution (Spec 005 - Rust Resolution)
//
// All tests in this module require filesystem access to resolve `mod foo;`
// declarations to their corresponding files (foo.rs or foo/mod.rs).
// This is a Tier 2 feature not available at the tree-sitter parser layer.

use std::fs;
use std::path::Path;

use keel_parsers::resolver::{CallSite, LanguageResolver};
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
    assert!(
        helper.is_some(),
        "should find helper() inside inline mod block"
    );

    let main_fn = result.definitions.iter().find(|d| d.name == "main");
    assert!(main_fn.is_some(), "should find main() function");
}

#[test]
/// `mod foo;` declaration should resolve to foo.rs or foo/mod.rs.
fn test_mod_declaration_resolves_to_file() {
    let dir = std::env::temp_dir().join("keel_test_mod_resolve");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();

    // Create lib.rs with mod declaration
    let lib_content = "mod parser;\nfn main() { parser::parse(); }";
    fs::write(dir.join("src/lib.rs"), lib_content).unwrap();

    // Create parser.rs as the target module file
    fs::write(dir.join("src/parser.rs"), "pub fn parse() -> bool { true }").unwrap();

    let resolver = RustLangResolver::new();
    let result = resolver.parse_file(&dir.join("src/lib.rs"), lib_content);

    // The mod declaration should create an import entry for parser
    let parser_import = result
        .imports
        .iter()
        .find(|i| i.imported_names.contains(&"parser".to_string()));
    assert!(
        parser_import.is_some(),
        "should have import entry for mod parser declaration"
    );

    // The import source should point to parser.rs
    let imp = parser_import.unwrap();
    assert!(
        imp.source.ends_with("parser.rs"),
        "mod declaration should resolve to parser.rs, got: {}",
        imp.source
    );

    // Verify mod_paths are populated
    let mod_paths = resolver.get_mod_paths();
    assert!(
        mod_paths.contains_key("parser"),
        "mod_paths should have 'parser'"
    );
    assert!(
        mod_paths["parser"].ends_with("parser.rs"),
        "parser mod path should end with parser.rs"
    );

    // Verify call edge resolution works for parser::parse()
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: dir.join("src/lib.rs").to_string_lossy().to_string(),
        line: 2,
        callee_name: "parser::parse".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "should resolve parser::parse() call edge");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "parse");
    assert!(
        edge.target_file.ends_with("parser.rs"),
        "target file should be parser.rs, got: {}",
        edge.target_file
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
/// `mod foo;` should prefer foo.rs over foo/mod.rs (Rust 2018+ edition).
fn test_mod_declaration_prefers_file_over_dir() {
    let dir = std::env::temp_dir().join("keel_test_mod_prefer");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/parser")).unwrap();

    let lib_content = "mod parser;";
    fs::write(dir.join("src/lib.rs"), lib_content).unwrap();

    // Create both forms
    fs::write(dir.join("src/parser.rs"), "pub fn parse() {}").unwrap();
    fs::write(dir.join("src/parser/mod.rs"), "pub fn parse() {}").unwrap();

    let resolver = RustLangResolver::new();
    resolver.parse_file(&dir.join("src/lib.rs"), lib_content);

    let mod_paths = resolver.get_mod_paths();
    assert!(
        mod_paths.contains_key("parser"),
        "should have parser in mod_paths"
    );

    // Rust 2018+ prefers parser.rs over parser/mod.rs
    let resolved = &mod_paths["parser"];
    assert!(
        resolved.ends_with("parser.rs") && !resolved.ends_with("mod.rs"),
        "should prefer parser.rs over parser/mod.rs, got: {}",
        resolved.display()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
/// Nested mod declarations should resolve through the directory hierarchy.
fn test_nested_mod_resolution() {
    let dir = std::env::temp_dir().join("keel_test_nested_mod");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/graph")).unwrap();

    // lib.rs declares mod graph
    let lib_content = "mod graph;";
    fs::write(dir.join("src/lib.rs"), lib_content).unwrap();

    // graph/mod.rs declares mod store
    let graph_content = "mod store;\npub fn build() {}";
    fs::write(dir.join("src/graph/mod.rs"), graph_content).unwrap();

    // graph/store.rs is the nested module
    fs::write(dir.join("src/graph/store.rs"), "pub fn save() {}").unwrap();

    let resolver = RustLangResolver::new();

    // Parse lib.rs — should find graph module
    resolver.parse_file(&dir.join("src/lib.rs"), lib_content);
    let mod_paths = resolver.get_mod_paths();
    assert!(mod_paths.contains_key("graph"), "should resolve mod graph");
    assert!(
        mod_paths["graph"].ends_with("graph/mod.rs"),
        "graph should resolve to graph/mod.rs, got: {}",
        mod_paths["graph"].display()
    );

    // Parse graph/mod.rs — should find store module
    resolver.parse_file(&dir.join("src/graph/mod.rs"), graph_content);
    let mod_paths = resolver.get_mod_paths();
    assert!(mod_paths.contains_key("store"), "should resolve mod store");
    assert!(
        mod_paths["store"].ends_with("store.rs"),
        "store should resolve to store.rs, got: {}",
        mod_paths["store"].display()
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
/// `#[path = "..."]` attribute should override the default file path resolution.
fn test_mod_path_attribute() {
    let dir = std::env::temp_dir().join("keel_test_mod_path_attr");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/custom")).unwrap();

    let lib_content = "#[path = \"custom/my_module.rs\"]\nmod mymod;";
    fs::write(dir.join("src/lib.rs"), lib_content).unwrap();
    fs::write(dir.join("src/custom/my_module.rs"), "pub fn custom_fn() {}").unwrap();

    let resolver = RustLangResolver::new();
    resolver.parse_file(&dir.join("src/lib.rs"), lib_content);

    let mod_paths = resolver.get_mod_paths();
    assert!(
        mod_paths.contains_key("mymod"),
        "should have mymod in mod_paths"
    );

    let resolved = &mod_paths["mymod"];
    assert!(
        resolved.ends_with("custom/my_module.rs"),
        "#[path] attribute should override default resolution, got: {}",
        resolved.display()
    );

    let _ = fs::remove_dir_all(&dir);
}
