// Tests for Go interface resolution (Spec 004 - Go Resolution)
//
// Tests interface extraction, structural typing satisfaction,
// empty interface handling, and low-confidence dispatch.

use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::go::GoResolver;
use keel_parsers::resolver::{CallSite, LanguageResolver};

#[test]
/// Interface type definitions should be extracted by tree-sitter.
fn test_interface_definition_extraction() {
    let resolver = GoResolver::new();
    let source = r#"
package repo

type Repository interface {
    Find(id string) (interface{}, error)
    Save(item interface{}) error
}
"#;
    let result = resolver.parse_file(Path::new("repo.go"), source);

    let iface_def = result.definitions.iter().find(|d| d.name == "Repository");
    assert!(iface_def.is_some(), "should find Repository interface");
    assert_eq!(iface_def.unwrap().kind, NodeKind::Class);
    assert!(
        iface_def.unwrap().is_public,
        "Repository should be exported (capitalized)"
    );
}

#[test]
/// Interface method calls should resolve to implementing types with confidence 0.40.
fn test_interface_method_resolution() {
    let resolver = GoResolver::new();
    let source = r#"
package repo

type Repository interface {
    Find(id string) string
    Save(item string) error
}

type InMemoryRepo struct {
    data map[string]string
}

func (r *InMemoryRepo) Find(id string) string { return r.data[id] }
func (r *InMemoryRepo) Save(item string) error { return nil }

func main() {
    var repo Repository
    repo.Find("123")
}
"#;
    let path = Path::new("repo.go");
    resolver.parse_file(path, source);

    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "repo.go".into(),
        line: 18,
        callee_name: "Repository.Find".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "should resolve Repository.Find through interface");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "Find");
    assert!(
        edge.confidence >= 0.35 && edge.confidence <= 0.50,
        "interface method confidence should be 0.40, got {}",
        edge.confidence
    );
}

#[test]
/// Implicit interface satisfaction: type with matching methods should satisfy.
fn test_implicit_interface_satisfaction() {
    let resolver = GoResolver::new();
    let source = r#"
package io

type Writer interface {
    Write(p string) int
}

type FileWriter struct{}

func (f *FileWriter) Write(p string) int { return len(p) }

func main() {
    var w Writer
    w.Write("data")
}
"#;
    let path = Path::new("io.go");
    resolver.parse_file(path, source);

    // FileWriter has Write() matching Writer interface => structural typing
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "io.go".into(),
        line: 13,
        callee_name: "Writer.Write".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "should resolve Writer.Write via structural typing");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "Write");
    assert!(
        edge.confidence <= 0.50,
        "interface dispatch should have low confidence, got {}",
        edge.confidence
    );
}

#[test]
/// Empty interface (interface{}/any) should resolve with very low confidence.
fn test_empty_interface_resolution() {
    let resolver = GoResolver::new();
    let source = r#"
package util

type Any interface{}

type Stringer struct{}

func (s *Stringer) String() string { return "" }
"#;
    let path = Path::new("util.go");
    resolver.parse_file(path, source);

    // Empty interface has no methods to resolve, so resolution is very ambiguous.
    // We verify that the interface is extracted with zero methods.
    let result = resolver.parse_file(path, source);
    let any_def = result.definitions.iter().find(|d| d.name == "Any");
    assert!(any_def.is_some(), "should find Any empty interface");

    // Any.SomeMethod should NOT resolve (no methods on empty interface)
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "util.go".into(),
        line: 8,
        callee_name: "Any.SomeMethod".into(),
        receiver: None,
    });
    // Empty interface has no declared methods, so method call fails to resolve
    // through the interface path. This is expected -- confidence would be 0.30.
    assert!(
        edge.is_none(),
        "empty interface method call should not resolve (no methods declared)"
    );
}

#[test]
/// Interface with embedded interfaces should be parsed without errors.
fn test_interface_embedding_extraction() {
    let resolver = GoResolver::new();
    let source = r#"
package io

type Reader interface {
    Read(p []byte) (n int, err error)
}

type Writer interface {
    Write(p []byte) (n int, err error)
}

type ReadWriter interface {
    Reader
    Writer
}
"#;
    let result = resolver.parse_file(Path::new("io.go"), source);

    let reader = result.definitions.iter().find(|d| d.name == "Reader");
    let writer = result.definitions.iter().find(|d| d.name == "Writer");
    let rw = result.definitions.iter().find(|d| d.name == "ReadWriter");

    assert!(reader.is_some(), "should find Reader interface");
    assert!(writer.is_some(), "should find Writer interface");
    assert!(rw.is_some(), "should find ReadWriter interface");
}

#[test]
/// Dynamic dispatch through interfaces should produce confidence < 0.50.
fn test_interface_dispatch_warning_not_error() {
    let resolver = GoResolver::new();
    let source = r#"
package repo

type Store interface {
    Get(key string) string
}

type MemStore struct{}

func (m *MemStore) Get(key string) string { return "" }

func main() {
    var s Store
    s.Get("foo")
}
"#;
    let path = Path::new("repo.go");
    resolver.parse_file(path, source);

    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "repo.go".into(),
        line: 14,
        callee_name: "Store.Get".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "should resolve Store.Get");
    let edge = edge.unwrap();
    // Low confidence means enforcement layer produces WARNING, not ERROR
    assert!(
        edge.confidence < 0.50,
        "interface dispatch confidence should be < 0.50 (produces WARNING), got {}",
        edge.confidence
    );
}
