// Tests for TypeScript default export resolution (Spec 002 - TypeScript Resolution)
//
// use keel_parsers::typescript::OxcResolver;

#[test]
#[ignore = "Not yet implemented"]
/// Default export of a named function should resolve to that function.
fn test_default_export_named_function() {
    // GIVEN module.ts with `export default function process() {}`
    // WHEN `import process from './module'` is resolved
    // THEN it resolves to the process function in module.ts
}

#[test]
#[ignore = "Not yet implemented"]
/// Default export of a class should resolve to that class.
fn test_default_export_class() {
    // GIVEN module.ts with `export default class Parser {}`
    // WHEN `import Parser from './module'` is resolved
    // THEN it resolves to the Parser class in module.ts
}

#[test]
#[ignore = "Not yet implemented"]
/// Default export of an anonymous function should resolve to the module's default.
fn test_default_export_anonymous() {
    // GIVEN module.ts with `export default () => {}`
    // WHEN `import handler from './module'` is resolved
    // THEN it resolves to the anonymous default export of module.ts
}

#[test]
#[ignore = "Not yet implemented"]
/// Importing both default and named exports should resolve both.
fn test_default_and_named_combined_import() {
    // GIVEN module.ts with default export and named exports
    // WHEN `import Default, { named1, named2 } from './module'` is resolved
    // THEN both the default and named exports resolve correctly
}

#[test]
#[ignore = "Not yet implemented"]
/// Re-exporting a default export should maintain the resolution chain.
fn test_reexport_default_export() {
    // GIVEN a.ts default-exports a function, b.ts re-exports a's default
    // WHEN the re-exported default is imported and resolved
    // THEN it traces back to the original function in a.ts
}

#[test]
#[ignore = "Not yet implemented"]
/// Default export assigned from a variable should resolve to the variable's value.
fn test_default_export_from_variable() {
    // GIVEN module.ts with `const handler = () => {}; export default handler;`
    // WHEN `import handler from './module'` is resolved
    // THEN it resolves to the handler arrow function
}

#[test]
#[ignore = "Not yet implemented"]
/// Importing a default export that doesn't exist should produce a resolution error.
fn test_missing_default_export() {
    // GIVEN module.ts with only named exports (no default)
    // WHEN `import Default from './module'` is resolved
    // THEN a resolution error is returned
}
