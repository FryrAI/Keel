// Tests for Go interface resolution (Spec 004 - Go Resolution)
//
// Tests that tree-sitter extracts Go interface type definitions.
// Interface satisfaction checking and dynamic dispatch resolution
// require type inference not available at the parser layer.

use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::go::GoResolver;
use keel_parsers::resolver::LanguageResolver;

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
#[ignore = "BUG: interface method resolution requires cross-type satisfaction checking"]
/// Interface method calls should resolve to all implementing types.
fn test_interface_method_resolution() {
    // Requires analyzing all types in the package to determine which
    // structs satisfy the interface, then linking method calls to all
    // matching implementations. This is a Tier 2+ feature.
}

#[test]
#[ignore = "BUG: implicit interface satisfaction requires method set analysis"]
/// Interface satisfaction is implicit in Go (no explicit implements keyword).
fn test_implicit_interface_satisfaction() {
    // Go uses structural typing — a type implements an interface if it has
    // all the required methods. Checking this requires cross-type analysis.
}

#[test]
#[ignore = "BUG: empty interface resolution requires type inference"]
/// Empty interface (interface{}/any) should match all types.
fn test_empty_interface_resolution() {
    // interface{} accepts any value. Resolution of method calls on
    // interface{} parameters has very low confidence since any type
    // could be passed.
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

    // All three interfaces should be extracted
    let reader = result.definitions.iter().find(|d| d.name == "Reader");
    let writer = result.definitions.iter().find(|d| d.name == "Writer");
    let rw = result.definitions.iter().find(|d| d.name == "ReadWriter");

    assert!(reader.is_some(), "should find Reader interface");
    assert!(writer.is_some(), "should find Writer interface");
    assert!(rw.is_some(), "should find ReadWriter interface");
}

#[test]
#[ignore = "BUG: dynamic dispatch confidence scoring requires interface resolution"]
/// Dynamic dispatch through interfaces should produce WARNING not ERROR on low confidence.
fn test_interface_dispatch_warning_not_error() {
    // Low-confidence dynamic dispatch through interfaces should produce
    // WARNING, not ERROR. This is an enforcement-layer concern that
    // requires interface resolution results from Tier 2.
}
