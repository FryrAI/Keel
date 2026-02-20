// Tests for Go capitalization-based visibility (Spec 004 - Go Resolution)
use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::go::GoResolver;
use keel_parsers::resolver::{CallSite, LanguageResolver};

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
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "ProcessData");
    assert!(
        defs[0].is_public,
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
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "helper");
    assert!(
        !defs[0].is_public,
        "Lowercase function should be unexported (package-private)"
    );
}

#[test]
/// Cross-package calls to unexported functions should resolve with low confidence.
fn test_cross_package_unexported_call_error() {
    let resolver = GoResolver::new();

    // Parse caller that imports a package and calls an unexported name
    let source = r#"
package main

import "github.com/user/project/internal"

func main() {
    internal.helper()
}
"#;
    resolver.parse_file(Path::new("/project/main.go"), source);

    // Resolve the qualified call to a lowercase (unexported) function
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "/project/main.go".into(),
        line: 7,
        callee_name: "internal.helper".into(),
        receiver: None,
    });
    assert!(
        edge.is_some(),
        "Should resolve cross-package unexported call (with low confidence)"
    );
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "helper");
    assert!(
        edge.confidence <= 0.50,
        "Unexported cross-package call should have low confidence, got: {}",
        edge.confidence
    );
    assert_eq!(edge.resolution_tier, "tier2_heuristic");
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
    assert!(!type_defs.is_empty(), "Should find User struct definition");
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

    assert!(!user_service.is_empty(), "Should find UserService struct");
    assert!(
        user_service[0].is_public,
        "UserService should be exported (uppercase)"
    );

    assert!(!config_def.is_empty(), "Should find config struct");
    assert!(
        !config_def[0].is_public,
        "config should be unexported (lowercase)"
    );
}
