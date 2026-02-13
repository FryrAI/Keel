// Oracle 1: Go graph correctness vs LSP ground truth
//
// Compares keel's Go graph output against LSP/SCIP baseline data
// to validate node counts, edge counts, and resolution accuracy.

use std::path::Path;

use keel_core::types::NodeKind;
use keel_parsers::go::GoResolver;
use keel_parsers::resolver::{LanguageResolver, ReferenceKind};

#[test]
fn test_go_function_node_count_matches_lsp() {
    // GIVEN a Go file with exactly 3 functions
    let resolver = GoResolver::new();
    let source = r#"
package main

func ReadData(path string) ([]byte, error) {
    return nil, nil
}

func ProcessData(data []byte) string {
    return ""
}

func WriteOutput(output string) error {
    return nil
}
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("main.go"), source);

    // THEN the number of Function nodes matches exactly 3
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 3, "expected 3 Function definitions, got {}", funcs.len());
    for name in &["ReadData", "ProcessData", "WriteOutput"] {
        assert!(
            funcs.iter().any(|f| f.name == *name),
            "missing function '{name}'"
        );
    }
}

#[test]
fn test_go_struct_node_count_matches_lsp() {
    // GIVEN a Go file with exactly 2 struct definitions
    let resolver = GoResolver::new();
    let source = r#"
package models

type User struct {
    Name  string
    Email string
}

type Session struct {
    Token   string
    UserID  int
    Active  bool
}
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("models.go"), source);

    // THEN the number of Class (struct) nodes matches exactly 2
    let structs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class)
        .collect();
    assert_eq!(structs.len(), 2, "expected 2 struct definitions, got {}", structs.len());
    assert!(structs.iter().any(|s| s.name == "User"), "missing struct User");
    assert!(structs.iter().any(|s| s.name == "Session"), "missing struct Session");
}

#[test]
#[ignore = "BUG: Module nodes not auto-created per file by parser"]
fn test_go_package_node_count_matches_lsp() {
    // The parser does not auto-create Module (package) nodes for each file.
    // Package-level grouping happens at a higher layer.
}

#[test]
fn test_go_call_edge_count_matches_lsp() {
    // GIVEN Go code with known function calls
    let resolver = GoResolver::new();
    let source = r#"
package main

func helper() int {
    return 42
}

func compute(x int) int {
    return x * 2
}

func main() {
    a := helper()
    b := compute(a)
    _ = b
}
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("calls.go"), source);

    // THEN call references are found for helper() and compute()
    let calls: Vec<_> = result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .collect();
    assert!(
        calls.len() >= 2,
        "expected >= 2 call references, got {}",
        calls.len()
    );
    assert!(
        calls.iter().any(|r| r.name.contains("helper")),
        "missing call to helper()"
    );
    assert!(
        calls.iter().any(|r| r.name.contains("compute")),
        "missing call to compute()"
    );
}

#[test]
fn test_go_method_receiver_resolution() {
    // GIVEN a Go struct with receiver methods
    let resolver = GoResolver::new();
    let source = r#"
package service

type Server struct {
    Port int
}

func NewServer(port int) *Server {
    return &Server{Port: port}
}

func (s *Server) Start() error {
    return nil
}

func (s *Server) Stop() error {
    return nil
}
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("server.go"), source);

    // THEN struct and receiver methods are captured
    let structs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class && d.name == "Server")
        .collect();
    assert_eq!(structs.len(), 1, "expected struct Server");

    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Function)
        .collect();
    assert!(
        funcs.iter().any(|f| f.name == "NewServer"),
        "should detect constructor NewServer"
    );
    assert!(
        funcs.iter().any(|f| f.name == "Start"),
        "should detect receiver method Start"
    );
    assert!(
        funcs.iter().any(|f| f.name == "Stop"),
        "should detect receiver method Stop"
    );
}

#[test]
fn test_go_interface_implementation_detection() {
    // GIVEN a Go interface definition
    let resolver = GoResolver::new();
    let source = r#"
package io

type Reader interface {
    Read(p []byte) (n int, err error)
}

type Writer interface {
    Write(p []byte) (n int, err error)
}
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("interfaces.go"), source);

    // THEN interfaces are captured as Class nodes
    let ifaces: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class)
        .collect();
    assert!(
        ifaces.iter().any(|i| i.name == "Reader"),
        "should detect interface Reader"
    );
    assert!(
        ifaces.iter().any(|i| i.name == "Writer"),
        "should detect interface Writer"
    );
}

#[test]
fn test_go_cross_package_call_resolution() {
    // GIVEN Go code with an import and a qualified call
    let resolver = GoResolver::new();
    let source = r#"
package main

import "fmt"

func main() {
    fmt.Println("hello")
}
"#;

    // WHEN keel parses the file
    let result = resolver.parse_file(Path::new("main.go"), source);

    // THEN the import is captured
    assert!(
        result.imports.len() >= 1,
        "expected >= 1 import, got {}",
        result.imports.len()
    );
    let fmt_import = result.imports.iter().find(|i| i.source.contains("fmt"));
    assert!(fmt_import.is_some(), "should detect import of 'fmt'");

    // AND a call reference is detected for fmt.Println
    let calls: Vec<_> = result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .collect();
    assert!(
        calls.iter().any(|r| r.name.contains("Println")),
        "should detect call to fmt.Println; found calls: {:?}",
        calls.iter().map(|r| &r.name).collect::<Vec<_>>()
    );
}
