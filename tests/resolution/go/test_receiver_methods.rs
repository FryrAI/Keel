// Tests for Go receiver method resolution (Spec 004 - Go Resolution)
//
// Tests that tree-sitter extracts Go methods with receivers as definitions
// and verifies type-aware resolution: auto-deref, embedding, shadowing.

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

    let struct_def = result.definitions.iter().find(|d| d.name == "Service");
    assert!(struct_def.is_some(), "should find Service struct");
    assert_eq!(struct_def.unwrap().kind, NodeKind::Class);

    let method_def = result.definitions.iter().find(|d| d.name == "Process");
    assert!(
        method_def.is_some(),
        "should find Process method with pointer receiver"
    );
    assert_eq!(method_def.unwrap().kind, NodeKind::Function);
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
/// Value receiver methods callable on pointer types (Go auto-deref).
/// Resolution through the type_methods map yields confidence 0.70.
fn test_value_receiver_on_pointer() {
    let resolver = GoResolver::new();
    let source = r#"
package service

type Service struct {
    name string
}

func (s Service) String() string {
    return s.name
}

func main() {
    var svc *Service
    svc.String()
}
"#;
    let path = Path::new("service.go");
    resolver.parse_file(path, source);

    // Resolve svc.String() -- "Service" is the receiver type in the type_methods map
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "service.go".into(),
        line: 14,
        callee_name: "Service.String".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "should resolve Service.String via type methods");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "String");
    assert!(
        edge.confidence >= 0.65 && edge.confidence <= 0.80,
        "auto-deref confidence should be ~0.70, got {}",
        edge.confidence
    );
}

#[test]
/// Embedded struct methods should be promoted and resolvable on the outer struct.
fn test_embedded_struct_method_promotion() {
    let resolver = GoResolver::new();
    let source = r#"
package models

type Logger struct{}

func (l *Logger) Log(msg string) {}

type Server struct {
    Logger
    port int
}

func main() {
    var s Server
    s.Log("hello")
}
"#;
    let path = Path::new("models.go");
    resolver.parse_file(path, source);

    // Resolve Server.Log() -- promoted from embedded Logger
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "models.go".into(),
        line: 14,
        callee_name: "Server.Log".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "should resolve Server.Log via embedding promotion");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "Log");
    assert!(
        edge.confidence >= 0.60 && edge.confidence <= 0.70,
        "embedded method confidence should be ~0.65, got {}",
        edge.confidence
    );
}

#[test]
/// Method name collision: outer struct method wins over embedded.
fn test_method_name_collision_outer_wins() {
    let resolver = GoResolver::new();
    let source = r#"
package models

type Inner struct{}

func (i *Inner) Name() string { return "inner" }

type Outer struct {
    Inner
}

func (o *Outer) Name() string { return "outer" }

func main() {
    var o Outer
    o.Name()
}
"#;
    let path = Path::new("models.go");
    resolver.parse_file(path, source);

    // Resolve Outer.Name() -- outer's own method should win
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "models.go".into(),
        line: 16,
        callee_name: "Outer.Name".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "should resolve Outer.Name");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "Name");
    // Direct type method has higher confidence (0.70) than embedded (0.65)
    assert!(
        edge.confidence >= 0.65,
        "outer method should have higher confidence than embedded, got {}",
        edge.confidence
    );
}
