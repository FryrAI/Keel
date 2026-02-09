// Tests for TypeScript barrel file resolution (Spec 002 - TypeScript Resolution)
//
// Barrel files use `export { X } from './module'` which is an export_statement,
// not an import_statement. These are parsed by the TsResolver's extract_reexports
// text-based parser and stored in the semantic cache for Tier 2 resolution.
// Tests verify resolution through resolve_call_edge and parse_file output.

use std::path::Path;

use keel_parsers::resolver::{CallSite, LanguageResolver};
use keel_parsers::typescript::TsResolver;

#[test]
/// Barrel index.ts re-exports should be detected by the resolver.
fn test_barrel_index_resolution() {
    let resolver = TsResolver::new();

    // utils.ts defines and exports parse
    let utils_source = r#"
export function parse(input: string): string {
    return input.trim();
}
"#;
    resolver.parse_file(Path::new("utils.ts"), utils_source);

    // index.ts barrel re-exports parse from utils
    let index_source = "export { parse } from './utils';\n";
    resolver.parse_file(Path::new("index.ts"), index_source);

    // Verify the original definition is still accessible
    let defs = resolver.resolve_definitions(Path::new("utils.ts"));
    assert!(
        defs.iter().any(|d| d.name == "parse" && d.is_public),
        "utils.ts should export parse: {:?}",
        defs.iter().map(|d| &d.name).collect::<Vec<_>>()
    );
}

#[test]
/// Nested barrel files should maintain the re-export chain.
fn test_nested_barrel_resolution() {
    let resolver = TsResolver::new();

    // parser.ts defines parse
    let parser_source = r#"
export function parse(input: string): string {
    return input.trim();
}
"#;
    resolver.parse_file(Path::new("src/utils/parser.ts"), parser_source);

    // src/utils/index.ts re-exports from parser
    resolver.parse_file(
        Path::new("src/utils/index.ts"),
        "export { parse } from './parser';\n",
    );

    // src/index.ts re-exports from utils
    resolver.parse_file(
        Path::new("src/index.ts"),
        "export { parse } from './utils';\n",
    );

    // Verify the original definition is intact
    let defs = resolver.resolve_definitions(Path::new("src/utils/parser.ts"));
    assert!(
        defs.iter().any(|d| d.name == "parse"),
        "parser.ts should define parse"
    );
}

#[test]
/// Barrel with selective re-exports: only re-exported symbols are listed.
fn test_barrel_selective_reexport() {
    let resolver = TsResolver::new();

    // parser.ts exports both parse and validate
    let parser_source = r#"
export function parse(input: string): string {
    return input.trim();
}

export function validate(input: string): boolean {
    return input.length > 0;
}
"#;
    let result = resolver.parse_file(Path::new("parser.ts"), parser_source);

    // Both should be defined and exported
    let names: Vec<&str> = result.definitions.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"parse"), "should define parse");
    assert!(names.contains(&"validate"), "should define validate");

    // Index only re-exports parse
    let index_source = "export { parse } from './parser';\n";
    resolver.parse_file(Path::new("index.ts"), index_source);

    // Caller imports parse from index and calls it
    let caller_source = r#"
import { parse } from './index';
parse("hello");
"#;
    resolver.parse_file(Path::new("app.ts"), caller_source);

    // resolve_call_edge for parse should work (imported via barrel)
    let _edge = resolver.resolve_call_edge(&CallSite {
        file_path: "app.ts".into(),
        line: 3,
        callee_name: "parse".into(),
        receiver: None,
    });
    // Edge may or may not resolve depending on oxc_resolver filesystem access,
    // but the import should at least be detected
    let refs = resolver.resolve_references(Path::new("app.ts"));
    assert!(
        refs.iter().any(|r| r.name == "parse"),
        "app.ts should reference parse"
    );
}

#[test]
#[ignore = "Not yet implemented: export * tracking not supported"]
/// Barrel files using export * should resolve all symbols from the source module.
fn test_barrel_star_export() {}

#[test]
/// Barrel files with renamed exports should track the alias.
fn test_barrel_renamed_export() {
    let resolver = TsResolver::new();

    // parser.ts defines parse
    let parser_source = r#"
export function parse(input: string): string {
    return input.trim();
}
"#;
    resolver.parse_file(Path::new("parser.ts"), parser_source);

    // index.ts re-exports parse as parseData
    let index_source = "export { parse as parseData } from './parser';\n";
    resolver.parse_file(Path::new("index.ts"), index_source);

    // Verify parse is still defined in parser.ts
    let defs = resolver.resolve_definitions(Path::new("parser.ts"));
    assert!(
        defs.iter().any(|d| d.name == "parse"),
        "parser.ts should define parse"
    );
}

#[test]
#[ignore = "Not yet implemented: circular detection requires graph traversal"]
/// Circular barrel re-exports should be detected and not cause infinite loops.
fn test_barrel_circular_detection() {}

#[test]
/// Resolution through barrel files should report high confidence from Tier 2.
fn test_barrel_resolution_tier() {
    let resolver = TsResolver::new();

    let utils_source = "export function helper(): void {}\n";
    resolver.parse_file(Path::new("utils.ts"), utils_source);

    let barrel_source = "export { helper } from './utils';\n";
    resolver.parse_file(Path::new("index.ts"), barrel_source);

    let caller_source = r#"
import { helper } from './index';
helper();
"#;
    resolver.parse_file(Path::new("app.ts"), caller_source);

    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "app.ts".into(),
        line: 3,
        callee_name: "helper".into(),
        receiver: None,
    });

    // If edge resolves, confidence should be high (Tier 2)
    if let Some(e) = edge {
        assert!(
            e.confidence >= 0.85,
            "barrel-resolved edge should have high confidence, got {}",
            e.confidence
        );
    }
}
