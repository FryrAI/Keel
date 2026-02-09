// Tests for TypeScript path alias resolution via tsconfig (Spec 002 - TypeScript Resolution)
//
// use keel_parsers::typescript::OxcResolver;

#[test]
#[ignore = "Not yet implemented"]
/// Path aliases defined in tsconfig.json should resolve correctly.
fn test_tsconfig_path_alias_resolution() {
    // GIVEN tsconfig.json with paths: { "@utils/*": ["src/utils/*"] }
    // WHEN `import { parse } from '@utils/parser'` is resolved
    // THEN it resolves to src/utils/parser.ts
}

#[test]
#[ignore = "Not yet implemented"]
/// Multiple path alias mappings should all resolve correctly.
fn test_multiple_path_aliases() {
    // GIVEN tsconfig.json with @utils/*, @types/*, @components/*
    // WHEN imports using each alias are resolved
    // THEN each resolves to the correct physical path
}

#[test]
#[ignore = "Not yet implemented"]
/// Path aliases with baseUrl should combine baseUrl and paths correctly.
fn test_path_alias_with_base_url() {
    // GIVEN tsconfig.json with baseUrl: "src" and paths: { "@/*": ["./*"] }
    // WHEN `import { App } from '@/App'` is resolved
    // THEN it resolves to src/App.ts
}

#[test]
#[ignore = "Not yet implemented"]
/// Path aliases in extends tsconfig should be inherited by child configs.
fn test_path_alias_tsconfig_extends() {
    // GIVEN a tsconfig.json that extends a base tsconfig with path aliases
    // WHEN imports using the inherited aliases are resolved
    // THEN they resolve correctly using the base config's paths
}

#[test]
#[ignore = "Not yet implemented"]
/// Non-existent path alias targets should produce a resolution error.
fn test_path_alias_missing_target() {
    // GIVEN tsconfig.json with paths: { "@ghost/*": ["nonexistent/*"] }
    // WHEN `import { x } from '@ghost/module'` is resolved
    // THEN a resolution error is returned indicating the target path doesn't exist
}

#[test]
#[ignore = "Not yet implemented"]
/// Path alias resolution should use oxc_resolver for 30x faster resolution than webpack.
fn test_path_alias_uses_oxc_resolver() {
    // GIVEN a project with tsconfig path aliases
    // WHEN 1000 path alias imports are resolved
    // THEN resolution completes in under 100ms (leveraging oxc_resolver speed)
}

#[test]
#[ignore = "Not yet implemented"]
/// Wildcard path aliases should match any subpath pattern.
fn test_path_alias_wildcard_matching() {
    // GIVEN tsconfig.json with paths: { "@lib/*": ["packages/*/src"] }
    // WHEN `import { util } from '@lib/core'` is resolved
    // THEN it resolves to packages/core/src
}
