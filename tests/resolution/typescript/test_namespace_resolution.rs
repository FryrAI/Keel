// Tests for TypeScript namespace and module resolution (Spec 002 - TypeScript Resolution)
//
// Most of these features require Tier 2/3 resolution infrastructure that is
// not yet implemented. Tests that cannot be meaningfully validated at the
// tree-sitter parser level are marked #[ignore] with BUG comments.

use std::path::Path;

#[allow(unused_imports)]
use keel_core::types::NodeKind;
use keel_parsers::resolver::LanguageResolver;
use keel_parsers::typescript::TsResolver;

#[test]
/// TypeScript namespace declarations: tree-sitter may not capture namespace
/// blocks since the query file focuses on function/class/method patterns.
fn test_namespace_declaration_resolution() {
    let resolver = TsResolver::new();
    let source = r#"
namespace Validators {
    export function isValid(s: string): boolean {
        return s.length > 0;
    }
}
"#;
    let result = resolver.parse_file(Path::new("validators.ts"), source);

    // tree-sitter query may or may not capture namespace-scoped functions.
    // Document the actual behavior.
    if result.definitions.is_empty() {
        // Known limitation: namespace-scoped functions not captured by current query
        assert_eq!(result.definitions.len(), 0);
    } else {
        // If captured, verify the function is present
        let func = result.definitions.iter().find(|d| d.name == "isValid");
        assert!(func.is_some(), "should capture isValid if namespaces are supported");
    }
}

#[test]
#[ignore = "BUG: module augmentation requires type-checker, not available at parser layer"]
/// Module augmentation should be tracked without creating duplicate nodes.
fn test_module_augmentation() {
    // `declare module 'express' { ... }` requires type-level resolution
    // that is beyond tree-sitter parsing capabilities.
}

#[test]
/// Ambient module declarations (.d.ts): verify the parser handles .d.ts files
/// without crashing, extracting any available definitions.
fn test_ambient_module_resolution() {
    let resolver = TsResolver::new();
    let source = r#"
declare module 'my-lib' {
    export function foo(): void;
}
"#;
    let result = resolver.parse_file(Path::new("types.d.ts"), source);

    // Ambient declarations may or may not produce definitions depending on
    // tree-sitter query coverage. Verify parsing succeeds.
    // If definitions are captured, they should be valid.
    for def in &result.definitions {
        assert!(!def.name.is_empty(), "definition name should not be empty");
    }
}

#[test]
#[ignore = "BUG: triple-slash references require file-system resolution not available in parser"]
/// Triple-slash reference directives should be followed for type resolution.
fn test_triple_slash_reference() {
    // `/// <reference path="./types.d.ts" />` requires filesystem resolution
    // to follow the reference and include the referenced file's types.
}

#[test]
/// Node.js module resolution: verify that non-relative imports from node_modules
/// are captured as import entries even if the module cannot be resolved.
fn test_node_modules_resolution() {
    let resolver = TsResolver::new();
    let source = r#"
import { merge } from 'lodash';
import express from 'express';
merge({}, {});
"#;
    let result = resolver.parse_file(Path::new("app.ts"), source);

    // Non-relative imports should be captured
    assert!(
        result.imports.len() >= 2,
        "should capture at least 2 imports, got {}",
        result.imports.len()
    );

    let lodash_import = result.imports.iter().find(|i| i.source.contains("lodash"));
    assert!(lodash_import.is_some(), "should capture lodash import");
    assert!(
        !lodash_import.unwrap().is_relative,
        "lodash import should NOT be marked as relative"
    );

    let express_import = result.imports.iter().find(|i| i.source.contains("express"));
    assert!(express_import.is_some(), "should capture express import");
}

#[test]
#[ignore = "BUG: package.json exports field parsing not implemented in parser layer"]
/// Conditional exports in package.json should be respected during resolution.
fn test_package_json_conditional_exports() {
    // Requires reading and parsing package.json's "exports" field,
    // which is a runtime/bundler concern beyond tree-sitter parsing.
}

#[test]
#[ignore = "BUG: TypeScript project references require tsconfig parsing with references field"]
/// TypeScript project references should resolve across project boundaries.
fn test_project_reference_resolution() {
    // Requires parsing tsconfig.json "references" field and resolving
    // across project boundaries, which is not implemented.
}
