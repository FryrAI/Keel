use std::path::Path;

use super::*;
use crate::resolver::ReferenceKind;
use keel_core::types::NodeKind;

#[test]
fn test_parse_typescript_function() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
function greet(name: string): string {
    return `Hello, ${name}!`;
}
"#;
    let result = parser
        .parse_file("typescript", Path::new("test.ts"), source)
        .unwrap();
    assert_eq!(result.definitions.len(), 1);
    assert_eq!(result.definitions[0].name, "greet");
    assert_eq!(result.definitions[0].kind, NodeKind::Function);
}

#[test]
fn test_parse_python_function() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
def greet(name: str) -> str:
    return f"Hello, {name}!"
"#;
    let result = parser
        .parse_file("python", Path::new("test.py"), source)
        .unwrap();
    assert_eq!(result.definitions.len(), 1);
    assert_eq!(result.definitions[0].name, "greet");
}

#[test]
fn test_parse_go_function() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
package main

func greet(name string) string {
    return "Hello, " + name
}
"#;
    let result = parser
        .parse_file("go", Path::new("test.go"), source)
        .unwrap();
    assert_eq!(result.definitions.len(), 1);
    assert_eq!(result.definitions[0].name, "greet");
}

#[test]
fn test_parse_rust_function() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
"#;
    let result = parser
        .parse_file("rust", Path::new("test.rs"), source)
        .unwrap();
    assert_eq!(result.definitions.len(), 1);
    assert_eq!(result.definitions[0].name, "greet");
}

#[test]
fn test_parse_typescript_class() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
class UserService {
    getUser(id: number): User {
        return this.db.find(id);
    }
}
"#;
    let result = parser
        .parse_file("typescript", Path::new("service.ts"), source)
        .unwrap();
    let classes: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == NodeKind::Class)
        .collect();
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].name, "UserService");
}

#[test]
fn test_parse_typescript_imports() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
import { foo, bar } from './utils';
import axios from 'axios';
"#;
    let result = parser
        .parse_file("typescript", Path::new("app.ts"), source)
        .unwrap();
    assert!(result.imports.len() >= 2);
    let relative: Vec<_> = result.imports.iter().filter(|i| i.is_relative).collect();
    assert!(!relative.is_empty());
}

#[test]
fn test_parse_typescript_calls() {
    let mut parser = TreeSitterParser::new();
    let source = r#"
function main() {
    const result = greet("world");
    console.log(result);
}
"#;
    let result = parser
        .parse_file("typescript", Path::new("main.ts"), source)
        .unwrap();
    let calls: Vec<_> = result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .collect();
    assert!(calls.len() >= 2);
}

#[test]
fn test_detect_language() {
    assert_eq!(detect_language(Path::new("foo.ts")), Some("typescript"));
    assert_eq!(detect_language(Path::new("bar.py")), Some("python"));
    assert_eq!(detect_language(Path::new("baz.go")), Some("go"));
    assert_eq!(detect_language(Path::new("qux.rs")), Some("rust"));
    assert_eq!(detect_language(Path::new("readme.md")), None);
}

#[test]
fn test_unsupported_language() {
    let mut parser = TreeSitterParser::new();
    let result = parser.parse_file("haskell", Path::new("test.hs"), "main = putStrLn");
    assert!(result.is_err());
}
