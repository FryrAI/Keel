// Tests for Go receiver method resolution (Spec 004 - Go Resolution)
//
// Tests that tree-sitter extracts Go methods with receivers as definitions
// and verifies same-file call edge resolution for method calls.
// Advanced features like auto-deref and method promotion require type
// inference not available at the parser layer.

use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::go::GoResolver;
use keel_parsers::resolver::{CallSite, LanguageResolver};

#[test]
/// Pointer receiver methods should be extracted as definitions by tree-sitter.
fn test_pointer_receiver_method() {
    let resolver = GoResolver::new();
    let source = r#"
package service

type Service struct {
    name string
}

func (s *Service) Process() string {
    return s.name
}
"#;
    let result = resolver.parse_file(Path::new("service.go"), source);

    // tree-sitter should capture the struct and the method
    let struct_def = result.definitions.iter().find(|d| d.name == "Service");
    assert!(struct_def.is_some(), "should find Service struct");
    assert_eq!(struct_def.unwrap().kind, NodeKind::Class);

    let method_def = result.definitions.iter().find(|d| d.name == "Process");
    assert!(
        method_def.is_some(),
        "should find Process method with pointer receiver"
    );
    assert_eq!(method_def.unwrap().kind, NodeKind::Function);
    // Capitalized method = exported
    assert!(
        method_def.unwrap().is_public,
        "Process should be exported (capitalized)"
    );
}

#[test]
/// Value receiver methods should be extracted as definitions.
fn test_value_receiver_method() {
    let resolver = GoResolver::new();
    let source = r#"
package service

type Service struct {
    name string
}

func (s Service) String() string {
    return s.name
}
"#;
    let result = resolver.parse_file(Path::new("service.go"), source);

    let method_def = result.definitions.iter().find(|d| d.name == "String");
    assert!(
        method_def.is_some(),
        "should find String method with value receiver"
    );
    assert_eq!(method_def.unwrap().kind, NodeKind::Function);
}

#[test]
#[ignore = "BUG: Go auto-deref resolution requires type inference not in parser"]
/// Value receiver methods should also be callable on pointers (Go auto-deref).
fn test_value_receiver_on_pointer() {
    // Go auto-dereferences pointers for value receiver method calls.
    // Requires type inference to know that the caller has a pointer type.
}

#[test]
#[ignore = "BUG: embedded struct method promotion requires cross-type analysis"]
/// Embedded struct methods should be promoted and resolvable on the outer struct.
fn test_embedded_struct_method_promotion() {
    // Method promotion requires understanding struct embedding relationships
    // and merging method sets, which is beyond tree-sitter parsing.
}

#[test]
#[ignore = "BUG: method shadowing detection requires type-aware resolution"]
/// Method name collisions between embedded struct and outer struct should resolve to outer.
fn test_method_name_collision_outer_wins() {
    // Determining which method shadows which requires understanding the
    // embedding hierarchy, a Tier 2+ feature.
}
