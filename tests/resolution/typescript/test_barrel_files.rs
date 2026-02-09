// Tests for TypeScript barrel file resolution (Spec 002 - TypeScript Resolution)
//
// use keel_parsers::typescript::OxcResolver;

#[test]
#[ignore = "Not yet implemented"]
/// Importing from a barrel index.ts should resolve to the original source module.
fn test_barrel_index_resolution() {
    // GIVEN an index.ts that re-exports from ./utils and ./types
    // WHEN `import { parse } from './';` is resolved
    // THEN the resolution traces through index.ts to the original module
}

#[test]
#[ignore = "Not yet implemented"]
/// Nested barrel files (index re-exporting from another index) should fully resolve.
fn test_nested_barrel_resolution() {
    // GIVEN src/index.ts -> src/utils/index.ts -> src/utils/parser.ts
    // WHEN `import { parse } from './utils';` is resolved
    // THEN the resolution traces through both barrel files to parser.ts
}

#[test]
#[ignore = "Not yet implemented"]
/// Barrel files with selective re-exports should only resolve exported symbols.
fn test_barrel_selective_reexport() {
    // GIVEN index.ts with `export { parse } from './parser'` (not `export *`)
    // WHEN `import { parse, validate } from './'` is resolved
    // THEN parse resolves correctly; validate fails resolution
}

#[test]
#[ignore = "Not yet implemented"]
/// Barrel files using export * should resolve all symbols from the source module.
fn test_barrel_star_export() {
    // GIVEN index.ts with `export * from './parser'`
    // WHEN any symbol from parser.ts is imported via index.ts
    // THEN it resolves correctly to parser.ts
}

#[test]
#[ignore = "Not yet implemented"]
/// Barrel files with renamed exports should track the alias chain.
fn test_barrel_renamed_export() {
    // GIVEN index.ts with `export { parse as parseData } from './parser'`
    // WHEN `import { parseData } from './'` is resolved
    // THEN it resolves to the `parse` function in parser.ts
}

#[test]
#[ignore = "Not yet implemented"]
/// Circular barrel re-exports should be detected and not cause infinite loops.
fn test_barrel_circular_detection() {
    // GIVEN a.ts re-exports from b.ts, b.ts re-exports from a.ts
    // WHEN resolution is attempted through this cycle
    // THEN the resolver detects the cycle and reports it without hanging
}

#[test]
#[ignore = "Not yet implemented"]
/// Resolution through barrel files should report resolution_tier=2 (Oxc enhancer).
fn test_barrel_resolution_tier() {
    // GIVEN an import resolved through a barrel file
    // WHEN the resolution result is examined
    // THEN resolution_tier is 2 (Oxc Tier 2 enhancer)
}
