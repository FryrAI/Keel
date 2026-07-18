// Tests for SvelteKit frontend resolution: `$lib` aliases via the tsconfig
// `extends` chain, the `$lib -> src/lib` fallback when `.svelte-kit/` has not
// been generated, and `.svelte` components entering the graph as modules that
// `.ts` files can import.
//
// Note: the tree-sitter TypeScript query matches each import_statement more than
// once (named-import pattern + side-effect catch-all), so a single import can
// produce multiple Import entries. Tests assert on "any import matches".

use std::path::Path;

use keel_parsers::resolver::LanguageResolver;
use keel_parsers::typescript::TsResolver;

/// Build a SvelteKit-shaped project on disk.
///
/// When `generate_svelte_kit` is true, the `.svelte-kit/tsconfig.json` that
/// `svelte-kit sync` would emit is written too; otherwise the project looks like
/// a freshly cloned worktree where only the stub `tsconfig.json` exists.
fn make_sveltekit_project(dir: &Path, generate_svelte_kit: bool) {
    std::fs::create_dir_all(dir.join("src/lib")).unwrap();
    std::fs::create_dir_all(dir.join("src/routes")).unwrap();

    std::fs::write(
        dir.join("svelte.config.js"),
        "import adapter from '@sveltejs/adapter-node';\nexport default { kit: { adapter: adapter() } };\n",
    )
    .unwrap();

    // Real SvelteKit apps have exactly this stub, which is useless on its own.
    std::fs::write(
        dir.join("tsconfig.json"),
        r#"{
  // generated config carries the $lib paths
  "extends": "./.svelte-kit/tsconfig.json",
  "compilerOptions": { "strict": true }
}"#,
    )
    .unwrap();

    if generate_svelte_kit {
        std::fs::create_dir_all(dir.join(".svelte-kit")).unwrap();
        std::fs::write(
            dir.join(".svelte-kit/tsconfig.json"),
            r#"{
  "compilerOptions": {
    "paths": { "$lib": ["../src/lib"], "$lib/*": ["../src/lib/*"] }
  }
}"#,
        )
        .unwrap();
    }

    std::fs::write(
        dir.join("src/lib/format.ts"),
        "export function formatDate(d: Date): string { return d.toISOString(); }\n",
    )
    .unwrap();

    std::fs::write(
        dir.join("src/lib/Card.svelte"),
        "<script lang=\"ts\">\n  import { formatDate } from '$lib/format';\n\n  export let when: Date;\n\n  export function label(prefix: string): string {\n    return prefix + formatDate(when);\n  }\n</script>\n\n<div class=\"card\">{label('at ')}</div>\n\n<style>\n  .card { color: red; }\n</style>\n",
    )
    .unwrap();
}

#[test]
/// `$lib` declared in the generated `.svelte-kit/tsconfig.json` must be picked up
/// through the root tsconfig's `extends`.
fn test_lib_alias_resolved_through_extends_chain() {
    let dir = tempfile::tempdir().unwrap();
    make_sveltekit_project(dir.path(), true);

    let resolver = TsResolver::with_project_root(dir.path());
    let source = "import { formatDate } from '$lib/format';\nformatDate(new Date());\n";
    let path = dir.path().join("src/routes/page.ts");
    let result = resolver.parse_file(&path, source);

    let expected = dir.path().join("src/lib/format.ts");
    assert!(
        result
            .imports
            .iter()
            .any(|i| Path::new(&i.source) == expected),
        "$lib/format should resolve to {}, got {:?}",
        expected.display(),
        result.imports.iter().map(|i| &i.source).collect::<Vec<_>>()
    );
}

#[test]
/// A fresh worktree has no `.svelte-kit/`; `$lib` must still resolve to `src/lib`.
fn test_lib_alias_falls_back_without_generated_config() {
    let dir = tempfile::tempdir().unwrap();
    make_sveltekit_project(dir.path(), false);

    let resolver = TsResolver::with_project_root(dir.path());
    let source = "import { formatDate } from '$lib/format';\nformatDate(new Date());\n";
    let path = dir.path().join("src/routes/page.ts");
    let result = resolver.parse_file(&path, source);

    let expected = dir.path().join("src/lib/format.ts");
    assert!(
        result
            .imports
            .iter()
            .any(|i| Path::new(&i.source) == expected),
        "$lib fallback should resolve to {}, got {:?}",
        expected.display(),
        result.imports.iter().map(|i| &i.source).collect::<Vec<_>>()
    );
}

#[test]
/// A `.svelte` component becomes a module node and its script-block definitions
/// and imports enter the graph.
fn test_svelte_component_definitions_enter_graph() {
    let dir = tempfile::tempdir().unwrap();
    make_sveltekit_project(dir.path(), true);

    let resolver = TsResolver::with_project_root(dir.path());
    let path = dir.path().join("src/lib/Card.svelte");
    let source = std::fs::read_to_string(&path).unwrap();
    let result = resolver.parse_file(&path, &source);

    let module = result
        .definitions
        .iter()
        .find(|d| d.kind == keel_core::types::NodeKind::Module)
        .expect("component must be a module node");
    assert_eq!(module.name, "Card");

    let label = result
        .definitions
        .iter()
        .find(|d| d.name == "label")
        .expect("function inside <script> must be extracted");
    assert_eq!(
        label.line_start, 6,
        "line number must point at the real line in the .svelte file"
    );

    let expected = dir.path().join("src/lib/format.ts");
    assert!(
        result
            .imports
            .iter()
            .any(|i| Path::new(&i.source) == expected),
        "the component's $lib import must resolve, got {:?}",
        result.imports.iter().map(|i| &i.source).collect::<Vec<_>>()
    );
}

#[test]
/// A `.ts` file importing a `.svelte` component resolves to that component file,
/// so the edge is no longer dangling.
fn test_ts_file_import_of_svelte_component_resolves() {
    let dir = tempfile::tempdir().unwrap();
    make_sveltekit_project(dir.path(), true);

    let resolver = TsResolver::with_project_root(dir.path());
    let source = "import Card from '$lib/Card.svelte';\nexport const C = Card;\n";
    let path = dir.path().join("src/routes/+page.ts");
    let result = resolver.parse_file(&path, source);

    let expected = dir.path().join("src/lib/Card.svelte");
    assert!(
        result
            .imports
            .iter()
            .any(|i| Path::new(&i.source) == expected),
        "'.svelte' import should resolve to {}, got {:?}",
        expected.display(),
        result.imports.iter().map(|i| &i.source).collect::<Vec<_>>()
    );
}

#[test]
/// Relative `.svelte` imports resolve as well.
fn test_relative_svelte_import_resolves() {
    let dir = tempfile::tempdir().unwrap();
    make_sveltekit_project(dir.path(), true);

    let resolver = TsResolver::with_project_root(dir.path());
    let source = "import Card from './Card.svelte';\nexport const C = Card;\n";
    let path = dir.path().join("src/lib/index.ts");
    let result = resolver.parse_file(&path, source);

    let expected = dir.path().join("src/lib/Card.svelte");
    assert!(
        result
            .imports
            .iter()
            .any(|i| Path::new(&i.source) == expected),
        "relative .svelte import should resolve, got {:?}",
        result.imports.iter().map(|i| &i.source).collect::<Vec<_>>()
    );
}

#[test]
/// `$app/*` is a SvelteKit virtual module and must stay an external specifier
/// rather than being reported as an unresolved local path.
fn test_sveltekit_virtual_modules_stay_external() {
    let dir = tempfile::tempdir().unwrap();
    make_sveltekit_project(dir.path(), true);

    let resolver = TsResolver::with_project_root(dir.path());
    let source = "import { goto } from '$app/navigation';\nimport { PUBLIC_X } from '$env/static/public';\nexport function nav(): void { goto(PUBLIC_X); }\n";
    let path = dir.path().join("src/routes/nav.ts");
    let result = resolver.parse_file(&path, source);

    for spec in ["$app/navigation", "$env/static/public"] {
        assert!(
            result.imports.iter().any(|i| i.source == spec),
            "{spec} must be preserved verbatim, got {:?}",
            result.imports.iter().map(|i| &i.source).collect::<Vec<_>>()
        );
    }
}
