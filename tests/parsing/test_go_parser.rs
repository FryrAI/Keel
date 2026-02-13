// Tests for Go tree-sitter parser (Spec 001 - Tree-sitter Foundation)

use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::go::GoResolver;
use keel_parsers::resolver::{LanguageResolver, ReferenceKind};

#[test]
/// Parsing a Go file with a package-level function should produce a Function node.
fn test_go_parse_function() {
    let resolver = GoResolver::new();
    let source = r#"
package main

func ProcessData(input []byte) (Result, error) {
    return Result{}, nil
}
"#;
    let result = resolver.parse_file(Path::new("test.go"), source);
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1, "expected exactly 1 function definition");
    assert_eq!(funcs[0].name, "ProcessData");
    assert_eq!(funcs[0].kind, NodeKind::Function);
}

#[test]
/// Parsing a Go struct type should produce a Class node (struct maps to Class).
fn test_go_parse_struct() {
    let resolver = GoResolver::new();
    let source = r#"
package main

type UserService struct {
    db *sql.DB
}
"#;
    let result = resolver.parse_file(Path::new("test.go"), source);
    let classes: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class)
        .collect();
    assert_eq!(classes.len(), 1, "expected exactly 1 class (struct) definition");
    assert_eq!(classes[0].name, "UserService");
    assert_eq!(classes[0].kind, NodeKind::Class);
}

#[test]
/// Parsing Go receiver methods should produce Function nodes (methods map to Function).
fn test_go_parse_receiver_method() {
    let resolver = GoResolver::new();
    let source = r#"
package main

type UserService struct {
    db *sql.DB
}

func (s *UserService) GetUser(id string) (*User, error) {
    return nil, nil
}
"#;
    let result = resolver.parse_file(Path::new("test.go"), source);
    // The method should be captured as a Function node
    let methods: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function && d.name == "GetUser")
        .collect();
    assert_eq!(methods.len(), 1, "expected method GetUser as Function node");
    assert_eq!(methods[0].name, "GetUser");
}

#[test]
/// Parsing Go interfaces should produce a Class node (interfaces map to Class).
fn test_go_parse_interface() {
    let resolver = GoResolver::new();
    let source = r#"
package main

type Repository interface {
    Find(id string) (*Entity, error)
}
"#;
    let result = resolver.parse_file(Path::new("test.go"), source);
    let classes: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class)
        .collect();
    assert_eq!(classes.len(), 1, "expected exactly 1 class (interface) definition");
    assert_eq!(classes[0].name, "Repository");
    assert_eq!(classes[0].kind, NodeKind::Class);
}

#[test]
/// Parsing Go import blocks should produce imports.
fn test_go_parse_imports() {
    let resolver = GoResolver::new();
    let source = r#"
package main

import (
    "fmt"
    "github.com/pkg/errors"
)

func main() {
    fmt.Println("hello")
}
"#;
    let result = resolver.parse_file(Path::new("test.go"), source);
    assert!(
        result.imports.len() >= 2,
        "expected at least 2 imports, got {}",
        result.imports.len()
    );
    // Check that "fmt" is captured as an import source
    let fmt_import = result.imports.iter().find(|i| i.source.contains("fmt"));
    assert!(fmt_import.is_some(), "should have fmt import");
    // Check that the github import is captured
    let errors_import = result
        .imports
        .iter()
        .find(|i| i.source.contains("errors"));
    assert!(errors_import.is_some(), "should have errors import");
}

#[test]
/// Parsing Go code with function calls should produce call references.
fn test_go_parse_call_sites() {
    let resolver = GoResolver::new();
    let source = r#"
package main

import "fmt"

func helper() string {
    return "world"
}

func main() {
    name := helper()
    fmt.Println(name)
}
"#;
    let result = resolver.parse_file(Path::new("test.go"), source);
    let calls: Vec<_> = result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .collect();
    assert!(
        calls.len() >= 2,
        "expected at least 2 call references (helper, fmt.Println), got {}",
        calls.len()
    );
    // Verify that at least one call references "helper"
    let helper_call = calls.iter().find(|r| r.name.contains("helper"));
    assert!(helper_call.is_some(), "should have a reference to helper()");
}

#[test]
/// Go exported (capitalized) functions should have is_public=true, unexported have is_public=false.
fn test_go_exported_function_visibility() {
    let resolver = GoResolver::new();
    let source = r#"
package main

func ProcessData() string {
    return "data"
}

func helper() string {
    return "help"
}
"#;
    let result = resolver.parse_file(Path::new("test.go"), source);
    assert_eq!(result.definitions.len(), 2, "expected 2 function definitions");
    let exported = result
        .definitions
        .iter()
        .find(|d| d.name == "ProcessData")
        .expect("should find ProcessData");
    let unexported = result
        .definitions
        .iter()
        .find(|d| d.name == "helper")
        .expect("should find helper");
    assert!(exported.is_public, "ProcessData should be public (exported)");
    assert!(
        !unexported.is_public,
        "helper should be private (unexported)"
    );
}

#[test]
/// Parsing Go init functions should capture them as Function nodes.
fn test_go_parse_init_function() {
    let resolver = GoResolver::new();
    let source = r#"
package main

func init() {
    setupDatabase()
}
"#;
    let result = resolver.parse_file(Path::new("test.go"), source);
    let init_fns: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.name == "init" && d.kind == NodeKind::Function)
        .collect();
    assert_eq!(init_fns.len(), 1, "expected init function to be captured");
    assert_eq!(init_fns[0].name, "init");
    assert_eq!(init_fns[0].kind, NodeKind::Function);
}
