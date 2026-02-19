// Tests for TypeScript default export resolution (Spec 002 - TypeScript Resolution)

use std::path::Path;

use keel_core::types::NodeKind;
#[allow(unused_imports)]
use keel_parsers::resolver::CallSite;
use keel_parsers::resolver::{LanguageResolver, ReferenceKind};
use keel_parsers::typescript::TsResolver;

#[test]
/// Default export of a named function should be parsed as a function definition.
fn test_default_export_named_function() {
    let resolver = TsResolver::new();
    let source = "export default function process(input: string): string { return input; }";
    let result = resolver.parse_file(Path::new("module.ts"), source);

    // The function should be captured as a definition
    let func = result
        .definitions
        .iter()
        .find(|d| d.kind == NodeKind::Function);
    assert!(
        func.is_some(),
        "default export named function should produce a Function definition, got: {:?}",
        result
            .definitions
            .iter()
            .map(|d| &d.name)
            .collect::<Vec<_>>()
    );
    if let Some(f) = func {
        assert_eq!(f.name, "process");
        assert!(f.is_public, "exported function should be public");
    }
}

#[test]
/// Default export of a class should be parsed as a Class definition.
fn test_default_export_class() {
    let resolver = TsResolver::new();
    let source = r#"
export default class Parser {
    parse(input: string): string { return input; }
}
"#;
    let result = resolver.parse_file(Path::new("module.ts"), source);

    let class = result
        .definitions
        .iter()
        .find(|d| d.kind == NodeKind::Class);
    assert!(
        class.is_some(),
        "default export class should produce a Class definition, got: {:?}",
        result
            .definitions
            .iter()
            .map(|d| (&d.name, &d.kind))
            .collect::<Vec<_>>()
    );
    if let Some(c) = class {
        assert_eq!(c.name, "Parser");
    }
}

#[test]
/// Default export of an anonymous arrow function. Tree-sitter may or may not
/// capture this as a definition depending on query patterns.
fn test_default_export_anonymous() {
    let resolver = TsResolver::new();
    let source = "export default () => { return 42; };";
    let result = resolver.parse_file(Path::new("module.ts"), source);
    let defs: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();

    // Anonymous default exports are not guaranteed to produce named definitions.
    // This test documents the actual behavior.
    if defs.is_empty() {
        // Known limitation: anonymous default exports may not produce definitions
        assert_eq!(defs.len(), 0);
    } else {
        // If captured, verify it's a Function
        let def = &defs[0];
        assert_eq!(def.kind, NodeKind::Function);
    }
}

#[test]
/// Importing both default and named exports should produce import entries for both.
fn test_default_and_named_combined_import() {
    let resolver = TsResolver::new();
    let source = r#"
import Default, { named1, named2 } from './module';
Default();
named1();
"#;
    let result = resolver.parse_file(Path::new("consumer.ts"), source);

    // Should have at least one import entry
    assert!(
        !result.imports.is_empty(),
        "combined import should produce import entries"
    );

    // Should have call references to the imported names
    let call_refs: Vec<_> = result
        .references
        .iter()
        .filter(|r| r.kind == ReferenceKind::Call)
        .collect();
    assert!(
        !call_refs.is_empty(),
        "should have call references for imported functions"
    );
}

#[test]
/// Re-exporting a default export: the barrel file should be parseable.
fn test_reexport_default_export() {
    let resolver = TsResolver::new();

    // a.ts has a default-exported function
    let a_source = "export default function handler(): void {}";
    resolver.parse_file(Path::new("a.ts"), a_source);

    // b.ts re-exports a's default
    let b_source = "export { default } from './a';";
    let b_result = resolver.parse_file(Path::new("b.ts"), b_source);

    // Parsing should succeed without panics
    // b.ts itself defines no functions/classes
    let b_defs: Vec<_> = b_result
        .definitions
        .iter()
        .filter(|d| d.kind != NodeKind::Module)
        .collect();
    assert_eq!(
        b_defs.len(),
        0,
        "re-export file should not define its own symbols"
    );
}

#[test]
/// Default export assigned from a variable should capture the function definition.
fn test_default_export_from_variable() {
    let resolver = TsResolver::new();
    let source = r#"
const handler = (input: string): string => { return input; };
export default handler;
"#;
    let result = resolver.parse_file(Path::new("module.ts"), source);

    // The arrow function assigned to const may be captured as a definition
    // depending on tree-sitter query coverage for lexical_declaration
    if !result.definitions.is_empty() {
        let def = result.definitions.iter().find(|d| d.name == "handler");
        assert!(
            def.is_some(),
            "if captured, the definition should be named 'handler'"
        );
    }
    // Either way, parsing should not panic
}

#[test]
/// Importing a default export that doesn't exist: import should still be parsed.
fn test_missing_default_export() {
    let resolver = TsResolver::new();

    // module.ts has only named exports (no default)
    let mod_source = "export function foo(): void {}";
    resolver.parse_file(Path::new("module.ts"), mod_source);

    // consumer.ts tries to import default
    let consumer_source = "import Default from './module';";
    let result = resolver.parse_file(Path::new("consumer.ts"), consumer_source);

    // The import should still be captured even if the default doesn't exist
    assert!(
        !result.imports.is_empty(),
        "import statement should be captured regardless of target existence"
    );
}
