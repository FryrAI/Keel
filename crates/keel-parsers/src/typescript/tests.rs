use std::collections::HashMap;
use std::path::Path;

use crate::resolver::{CallSite, LanguageResolver};
use crate::typescript::helpers::{
    extract_string_literal, is_js_file, js_has_jsdoc_type_hints, resolve_path_alias,
    ts_has_type_hints,
};
use crate::typescript::semantic::extract_reexports;
use crate::typescript::TsResolver;

#[test]
fn test_ts_resolver_parse_function() {
    let resolver = TsResolver::new();
    let source = r#"
export function greet(name: string): string {
    return `Hello, ${name}!`;
}
"#;
    let result = resolver.parse_file(Path::new("test.ts"), source);
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == keel_core::types::NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(funcs[0].name, "greet");
    assert!(funcs[0].type_hints_present);
    assert!(funcs[0].is_public);
}

#[test]
fn test_ts_resolver_parse_class() {
    let resolver = TsResolver::new();
    let source = r#"
class UserService {
    getUser(id: number): User {
        return this.db.find(id);
    }
}
"#;
    let result = resolver.parse_file(Path::new("service.ts"), source);
    let classes: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == keel_core::types::NodeKind::Class)
        .collect();
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].name, "UserService");
}

#[test]
fn test_ts_resolver_caches_results() {
    let resolver = TsResolver::new();
    let source = "function hello() { return 1; }";
    let path = Path::new("cached.ts");
    resolver.parse_file(path, source);
    let defs = resolver.resolve_definitions(path);
    let funcs: Vec<_> = defs
        .iter()
        .filter(|d| d.kind == keel_core::types::NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(funcs[0].name, "hello");
}

#[test]
fn test_ts_resolver_same_file_call_edge() {
    let resolver = TsResolver::new();
    let source = r#"
function helper() { return 1; }
function main() { helper(); }
"#;
    let path = Path::new("edge.ts");
    resolver.parse_file(path, source);
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "edge.ts".into(),
        line: 3,
        callee_name: "helper".into(),
        receiver: None,
    });
    assert!(edge.is_some());
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "helper");
    assert!(edge.confidence >= 0.90);
}

#[test]
fn test_ts_has_type_hints() {
    assert!(ts_has_type_hints("greet(name: string) -> string"));
    assert!(!ts_has_type_hints("greet(name)"));
}

#[test]
fn test_oxc_semantic_enrichment() {
    let resolver = TsResolver::new();
    let source = r#"
export function add(a: number, b: number): number {
    return a + b;
}

function internal(x: number): number {
    return x * 2;
}
"#;
    let result = resolver.parse_file(Path::new("math.ts"), source);
    let exported: Vec<_> = result.definitions.iter().filter(|d| d.is_public).collect();
    let private: Vec<_> = result.definitions.iter().filter(|d| !d.is_public).collect();
    // oxc_semantic should detect `export` on `add` but not `internal`
    assert!(!exported.is_empty(), "should have exported symbols");
    assert!(!private.is_empty(), "should have private symbols");
}

#[test]
fn test_barrel_file_reexport_parsing() {
    let reexports = extract_reexports(
        r#"
export { UserService } from './user-service';
export { AuthService as Auth } from './auth-service';
export * from './utils';
"#,
    );
    assert_eq!(reexports.len(), 2);
    assert_eq!(
        reexports.get("UserService").unwrap(),
        &("./user-service".to_string(), "UserService".to_string())
    );
    assert_eq!(
        reexports.get("Auth").unwrap(),
        &("./auth-service".to_string(), "AuthService".to_string())
    );
}

#[test]
fn test_path_alias_resolution() {
    let mut aliases = HashMap::new();
    aliases.insert(
        "@components".to_string(),
        "/project/src/components".to_string(),
    );
    aliases.insert("@utils".to_string(), "/project/src/utils".to_string());

    assert_eq!(
        resolve_path_alias("@components/Button", &aliases),
        Some("/project/src/components/Button".to_string())
    );
    assert_eq!(
        resolve_path_alias("@utils", &aliases),
        Some("/project/src/utils".to_string())
    );
    assert_eq!(resolve_path_alias("./local", &aliases), None);
}

#[test]
fn test_cross_file_symbol_stitching() {
    let resolver = TsResolver::new();

    // Parse the "target" module first so its symbols are in the semantic cache
    let target_source = r#"
export function fetchUser(id: number): Promise<User> {
    return db.query(id);
}
"#;
    resolver.parse_file(Path::new("user-service.ts"), target_source);

    // Parse the "caller" module that imports from the target
    let caller_source = r#"
import { fetchUser } from './user-service';

function handleRequest() {
    fetchUser(42);
}
"#;
    let caller_path = Path::new("handler.ts");
    resolver.parse_file(caller_path, caller_source);

    // The import won't resolve via oxc_resolver (no real filesystem),
    // but the call edge should still resolve via Tier 1
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "handler.ts".into(),
        line: 5,
        callee_name: "fetchUser".into(),
        receiver: None,
    });
    assert!(edge.is_some());
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "fetchUser");
    assert!(edge.confidence >= 0.85);
}

#[test]
fn test_extract_string_literal() {
    assert_eq!(
        extract_string_literal("from './module'"),
        Some("./module".to_string())
    );
    assert_eq!(
        extract_string_literal(r#"from "./module""#),
        Some("./module".to_string())
    );
    assert_eq!(extract_string_literal("no quotes here"), None);
}

#[test]
fn test_is_js_file() {
    assert!(is_js_file(Path::new("app.js")));
    assert!(is_js_file(Path::new("component.jsx")));
    assert!(is_js_file(Path::new("utils.mjs")));
    assert!(is_js_file(Path::new("config.cjs")));
    assert!(!is_js_file(Path::new("app.ts")));
    assert!(!is_js_file(Path::new("component.tsx")));
    assert!(!is_js_file(Path::new("no_ext")));
}

#[test]
fn test_js_has_jsdoc_type_hints_present() {
    let source = r#"/**
 * Greet a user.
 * @param {string} name - The user's name
 * @returns {string} A greeting
 */
function greet(name) {
    return `Hello, ${name}!`;
}"#;
    // Function is on line 6 (1-based)
    assert!(js_has_jsdoc_type_hints(source, 6));
}

#[test]
fn test_js_has_jsdoc_type_hints_absent() {
    let source = r#"// Just a regular comment
function greet(name) {
    return `Hello, ${name}!`;
}"#;
    assert!(!js_has_jsdoc_type_hints(source, 2));
}

#[test]
fn test_js_has_jsdoc_no_param_tags() {
    let source = r#"/**
 * Greet a user.
 */
function greet(name) {
    return `Hello, ${name}!`;
}"#;
    // JSDoc block exists but no @param/@returns
    assert!(!js_has_jsdoc_type_hints(source, 4));
}

#[test]
fn test_js_file_with_jsdoc_parsed_correctly() {
    let resolver = TsResolver::new();
    let source = r#"/**
 * Add two numbers.
 * @param {number} a - First number
 * @param {number} b - Second number
 * @returns {number} The sum
 */
export function add(a, b) {
    return a + b;
}"#;
    let result = resolver.parse_file(Path::new("math.js"), source);
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == keel_core::types::NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(funcs[0].name, "add");
    assert!(
        funcs[0].type_hints_present,
        "JSDoc @param should set type_hints_present"
    );
}

#[test]
fn test_js_file_without_jsdoc_no_type_hints() {
    let resolver = TsResolver::new();
    let source = r#"
export function add(a, b) {
    return a + b;
}
"#;
    let result = resolver.parse_file(Path::new("math.js"), source);
    let funcs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == keel_core::types::NodeKind::Function)
        .collect();
    assert_eq!(funcs.len(), 1);
    assert_eq!(funcs[0].name, "add");
    assert!(
        !funcs[0].type_hints_present,
        "No JSDoc should mean type_hints_present is false"
    );
}
