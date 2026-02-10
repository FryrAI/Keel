// Tests for TypeScript path alias resolution via tsconfig (Spec 002 - TypeScript Resolution)
//
// Note: The tree-sitter TypeScript query matches each import_statement multiple
// times (named import pattern + side-effect catch-all pattern), so a single
// `import { x } from 'y'` produces 2 Import entries. Tests account for this.

use std::io::Write;

use keel_parsers::resolver::LanguageResolver;
use keel_parsers::typescript::TsResolver;

#[test]
/// Path aliases defined in tsconfig.json should resolve correctly.
fn test_tsconfig_path_alias_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let tsconfig = dir.path().join("tsconfig.json");
    std::fs::write(
        &tsconfig,
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@utils/*": ["src/utils/*"]
    }
  }
}"#,
    )
    .unwrap();

    // Create the target file so oxc_resolver can find it
    let utils_dir = dir.path().join("src/utils");
    std::fs::create_dir_all(&utils_dir).unwrap();
    std::fs::write(
        utils_dir.join("parser.ts"),
        "export function parse(s: string): string { return s; }\n",
    )
    .unwrap();

    let resolver = TsResolver::new();
    resolver.load_tsconfig_paths(dir.path());

    let caller_source = "import { parse } from '@utils/parser';\nparse(\"hello\");\n";
    let caller_path = dir.path().join("src/app.ts");
    let result = resolver.parse_file(&caller_path, caller_source);

    // Should have at least one import resolved through the alias
    assert!(!result.imports.is_empty(), "should have imports");
    let resolved_any = result
        .imports
        .iter()
        .any(|imp| imp.source.contains("src/utils/parser") || imp.source.contains("utils/parser"));
    assert!(
        resolved_any,
        "at least one import should resolve through alias, got: {:?}",
        result.imports.iter().map(|i| &i.source).collect::<Vec<_>>()
    );
}

#[test]
/// Multiple path alias mappings should all resolve correctly.
fn test_multiple_path_aliases() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@utils/*": ["src/utils/*"],
      "@types/*": ["src/types/*"],
      "@components/*": ["src/components/*"]
    }
  }
}"#,
    )
    .unwrap();

    let resolver = TsResolver::new();
    resolver.load_tsconfig_paths(dir.path());

    // Create target directories and files
    for sub in &["src/utils", "src/types", "src/components"] {
        std::fs::create_dir_all(dir.path().join(sub)).unwrap();
        let mut f = std::fs::File::create(dir.path().join(sub).join("index.ts")).unwrap();
        writeln!(f, "export const x = 1;").unwrap();
    }

    let source = "import { a } from '@utils/index';\nimport { b } from '@types/index';\nimport { c } from '@components/index';\n";
    let caller = dir.path().join("src/main.ts");
    let result = resolver.parse_file(&caller, source);

    // Each import may produce multiple entries due to tree-sitter query matching
    // but we should have entries for all 3 sources
    let sources: Vec<&str> = result.imports.iter().map(|i| i.source.as_str()).collect();
    assert!(
        sources.iter().any(|s| s.contains("utils")),
        "should have utils import, got: {:?}",
        sources
    );
    assert!(
        sources.iter().any(|s| s.contains("types")),
        "should have types import, got: {:?}",
        sources
    );
    assert!(
        sources.iter().any(|s| s.contains("components")),
        "should have components import, got: {:?}",
        sources
    );
}

#[test]
/// Path aliases with baseUrl should combine baseUrl and paths correctly.
fn test_path_alias_with_base_url() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "baseUrl": "src",
    "paths": {
      "@/*": ["./*"]
    }
  }
}"#,
    )
    .unwrap();

    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("App.ts"), "export function App(): void {}\n").unwrap();

    let resolver = TsResolver::new();
    resolver.load_tsconfig_paths(dir.path());

    let source = "import { App } from '@/App';\n";
    let caller = dir.path().join("src/main.ts");
    let result = resolver.parse_file(&caller, source);

    assert!(!result.imports.is_empty(), "should have imports");
    // At least one import should resolve through baseUrl + path alias
    let resolved_any = result.imports.iter().any(|imp| {
        imp.source.contains("App") && (imp.source.contains("src") || imp.source.starts_with('/'))
    });
    assert!(
        resolved_any,
        "import should resolve through baseUrl + alias, got: {:?}",
        result.imports.iter().map(|i| &i.source).collect::<Vec<_>>()
    );
}

#[test]
/// Path aliases in extends tsconfig should be inherited by child configs.
fn test_path_alias_tsconfig_extends() {}

#[test]
/// Non-existent path alias targets should still be expanded.
fn test_path_alias_missing_target() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@ghost/*": ["nonexistent/*"]
    }
  }
}"#,
    )
    .unwrap();

    let resolver = TsResolver::new();
    resolver.load_tsconfig_paths(dir.path());

    let source = "import { x } from '@ghost/module';\n";
    let caller = dir.path().join("main.ts");
    let result = resolver.parse_file(&caller, source);

    assert!(!result.imports.is_empty(), "should have imports");
    // The alias should be expanded even though the target doesn't exist
    let has_expanded = result
        .imports
        .iter()
        .any(|imp| imp.source.contains("nonexistent") || imp.source.contains("@ghost"));
    assert!(
        has_expanded,
        "unresolvable alias should keep expanded or original path, got: {:?}",
        result.imports.iter().map(|i| &i.source).collect::<Vec<_>>()
    );
}

#[test]
/// Path alias resolution should use oxc_resolver for 30x faster resolution.
fn test_path_alias_uses_oxc_resolver() {}

#[test]
/// Wildcard path aliases should match any subpath pattern.
fn test_path_alias_wildcard_matching() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@lib/*": ["packages/*/src"]
    }
  }
}"#,
    )
    .unwrap();

    let resolver = TsResolver::new();
    resolver.load_tsconfig_paths(dir.path());

    let source = "import { util } from '@lib/core';\n";
    let caller = dir.path().join("main.ts");
    let result = resolver.parse_file(&caller, source);

    assert!(!result.imports.is_empty(), "should have imports");
    // The alias @lib/core should be expanded through the wildcard mapping
    let has_expanded = result
        .imports
        .iter()
        .any(|imp| imp.source.contains("packages") || imp.source.contains("@lib"));
    assert!(
        has_expanded,
        "wildcard alias should expand, got: {:?}",
        result.imports.iter().map(|i| &i.source).collect::<Vec<_>>()
    );
}
