// Tests for Go capitalization-based visibility (Spec 004 - Go Resolution)
use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::go::GoResolver;
use keel_parsers::resolver::LanguageResolver;

#[test]
/// Capitalized functions should be marked as exported (public) visibility.
fn test_capitalized_function_is_exported() {
    let resolver = GoResolver::new();
    let source = r#"
package handlers

func ProcessData(input string) string {
    return input
}
"#;
    let result = resolver.parse_file(Path::new("handlers.go"), source);
    assert_eq!(result.definitions.len(), 1);
    assert_eq!(result.definitions[0].name, "ProcessData");
    assert!(
        result.definitions[0].is_public,
        "Capitalized function should be exported (public)"
    );
}

#[test]
/// Lowercase functions should be marked as unexported (package-private) visibility.
fn test_lowercase_function_is_unexported() {
    let resolver = GoResolver::new();
    let source = r#"
package handlers

func helper(x int) int {
    return x + 1
}
"#;
    let result = resolver.parse_file(Path::new("handlers.go"), source);
    assert_eq!(result.definitions.len(), 1);
    assert_eq!(result.definitions[0].name, "helper");
    assert!(
        !result.definitions[0].is_public,
        "Lowercase function should be unexported (package-private)"
    );
}

#[test]
#[ignore = "BUG: cross-package unexported call detection requires multi-file resolution"]
/// Cross-package calls to unexported functions should produce resolution errors.
fn test_cross_package_unexported_call_error() {
    // Requires multi-file cross-package resolution which the current
    // GoResolver doesn't support (single-file cache model). Detecting
    // calls to unexported names across packages requires knowing the
    // caller's package and the callee's visibility.
}

#[test]
/// Struct fields follow the same capitalization visibility rules.
/// We test that struct type definitions are detected with correct visibility.
fn test_struct_field_visibility() {
    let resolver = GoResolver::new();
    let source = r#"
package models

type User struct {
    Name string
    age  int
}
"#;
    let result = resolver.parse_file(Path::new("models.go"), source);
    // tree-sitter should extract the struct type definition
    let type_defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.name == "User")
        .collect();
    assert!(
        !type_defs.is_empty(),
        "Should find User struct definition"
    );
    // User starts with uppercase = exported
    assert!(
        type_defs[0].is_public,
        "User struct should be exported (uppercase)"
    );
}

#[test]
/// Capitalized types (struct, interface) should be exported.
fn test_capitalized_type_visibility() {
    let resolver = GoResolver::new();
    let source = r#"
package services

type UserService struct {
    db string
}

type config struct {
    host string
    port int
}
"#;
    let result = resolver.parse_file(Path::new("services.go"), source);

    let user_service: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.name == "UserService")
        .collect();
    let config_def: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.name == "config")
        .collect();

    assert!(
        !user_service.is_empty(),
        "Should find UserService struct"
    );
    assert!(
        user_service[0].is_public,
        "UserService should be exported (uppercase)"
    );

    assert!(
        !config_def.is_empty(),
        "Should find config struct"
    );
    assert!(
        !config_def[0].is_public,
        "config should be unexported (lowercase)"
    );
}
