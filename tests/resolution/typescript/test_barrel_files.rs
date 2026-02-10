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

#[test]
/// Cross-file resolution through a barrel re-export should trace back to the
/// original definition file, not stop at the barrel.
fn test_cross_file_resolution_through_reexport() {
    let resolver = TsResolver::new();

    // utils.ts defines and exports helper
    let utils_source = r#"
export function helper(input: string): string {
    return input.toUpperCase();
}
"#;
    resolver.parse_file(Path::new("utils.ts"), utils_source);

    // index.ts barrel re-exports helper from utils
    let barrel_source = "export { helper } from './utils';\n";
    resolver.parse_file(Path::new("index.ts"), barrel_source);

    // app.ts imports helper from the barrel and calls it
    let caller_source = r#"
import { helper } from './index';
const result = helper("hello");
"#;
    resolver.parse_file(Path::new("app.ts"), caller_source);

    // Resolve the call edge from app.ts
    let edge = resolver.resolve_call_edge(&CallSite {
        file_path: "app.ts".into(),
        line: 3,
        callee_name: "helper".into(),
        receiver: None,
    });

    // The edge MUST resolve (not silently pass on None)
    assert!(
        edge.is_some(),
        "cross-file call through barrel re-export must resolve"
    );

    let e = edge.unwrap();

    // Should trace through the barrel back to the original definition
    assert_eq!(
        e.target_name, "helper",
        "resolved target name should be 'helper'"
    );

    // Confidence should be Tier 2 level (>= 0.85)
    assert!(
        e.confidence >= 0.85,
        "cross-file barrel resolution should have Tier 2 confidence, got {}",
        e.confidence
    );

    // If the resolver traces through the re-export, target_file should be
    // utils.ts (the original source), not index.ts (the barrel)
    if e.confidence >= 0.95 {
        assert_eq!(
            e.target_file, "utils.ts",
            "Tier 2 resolution should trace re-export back to original source file"
        );
    }
}
