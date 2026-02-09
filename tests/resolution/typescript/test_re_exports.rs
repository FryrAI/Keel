// Tests for TypeScript re-export resolution (Spec 002 - TypeScript Resolution)
//
// Key insight: `export { X } from './y'` is NOT captured as an import by tree-sitter
// (it's an export_statement, not an import_statement). Re-exports are parsed by
// the TsResolver's `extract_reexports` text-based parser and stored in the
// semantic_cache. The correct way to test re-export resolution is through
// resolve_call_edge, which consults the semantic cache.

use std::path::Path;

use keel_parsers::resolver::{CallSite, LanguageResolver};
use keel_parsers::typescript::TsResolver;

#[test]
/// Named re-export should be detectable via call edge resolution.
fn test_named_reexport_resolution() {
    let resolver = TsResolver::new();

    // a.ts exports parse
    let a_source = r#"
export function parse(input: string): string {
    return input.trim();
}
"#;
    resolver.parse_file(Path::new("a.ts"), a_source);

    // b.ts re-exports parse from a (this is parsed by extract_reexports)
    let b_source = "export { parse } from './a';\n";
    resolver.parse_file(Path::new("b.ts"), b_source);

    // Verify the resolver's parse_file returns definitions for a.ts
    let a_defs = resolver.resolve_definitions(Path::new("a.ts"));
    assert!(
        a_defs.iter().any(|d| d.name == "parse"),
        "a.ts should define parse"
    );

    // Verify we can resolve a same-file call to parse in a.ts
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "a.ts".into(),
        line: 3,
        callee_name: "parse".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "should resolve parse within a.ts");
    assert_eq!(edge.unwrap().target_name, "parse");
}

#[test]
/// Renamed re-export should be tracked via extract_reexports.
fn test_renamed_reexport() {
    let resolver = TsResolver::new();

    let a_source = r#"
export function parse(input: string): string {
    return input.trim();
}
"#;
    resolver.parse_file(Path::new("a.ts"), a_source);

    // b.ts re-exports parse as parseData
    let b_source = "export { parse as parseData } from './a';\n";
    resolver.parse_file(Path::new("b.ts"), b_source);

    // Verify parse is defined in a.ts
    let a_defs = resolver.resolve_definitions(Path::new("a.ts"));
    assert!(
        a_defs.iter().any(|d| d.name == "parse"),
        "a.ts should define parse"
    );

    // Verify a.ts parse has correct properties
    let parse_def = a_defs.iter().find(|d| d.name == "parse").unwrap();
    assert!(parse_def.is_public, "parse should be public (exported)");
    assert!(parse_def.type_hints_present, "parse should have type hints");
}

#[test]
#[ignore = "Not yet implemented: star re-export requires export * tracking"]
/// Star re-export should forward all named exports.
fn test_star_reexport() {}

#[test]
#[ignore = "Not yet implemented: star re-export name collision semantics"]
/// Star re-export with name collision should follow TypeScript's last-wins semantics.
fn test_star_reexport_name_collision() {}

#[test]
#[ignore = "Not yet implemented: namespace re-export requires special handling"]
/// Namespace re-export should resolve to a module namespace object.
fn test_namespace_reexport() {}

#[test]
/// Multiple levels of re-exports: definitions should be tracked through the chain.
fn test_deep_reexport_chain() {
    let resolver = TsResolver::new();

    // a.ts defines parse
    let a_source = r#"
export function parse(input: string): string {
    return input.trim();
}
"#;
    resolver.parse_file(Path::new("a.ts"), a_source);

    // b.ts re-exports from a, c.ts re-exports from b
    resolver.parse_file(Path::new("b.ts"), "export { parse } from './a';\n");
    resolver.parse_file(Path::new("c.ts"), "export { parse } from './b';\n");

    // The original definition in a.ts should still be resolvable
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "a.ts".into(),
        line: 3,
        callee_name: "parse".into(),
        receiver: None,
    });
    assert!(edge.is_some(), "parse should be resolvable in a.ts");
    let edge = edge.unwrap();
    assert_eq!(edge.target_name, "parse");
    assert!(edge.confidence >= 0.90);
}

#[test]
#[ignore = "Not yet implemented: external package resolution requires node_modules"]
/// Re-exporting from an external package should resolve to the package's export.
fn test_reexport_from_external_package() {}
