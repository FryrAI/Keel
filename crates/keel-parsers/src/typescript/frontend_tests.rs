//! Tests for frontend-specific TS handling: TSX/JSX grammar selection and
//! `.svelte` single-file components.

use std::path::Path;

use crate::resolver::LanguageResolver;
use crate::typescript::TsResolver;

// ---------------------------------------------------------------------------
// Grammar selection (.tsx/.jsx must use the TSX grammar)
// ---------------------------------------------------------------------------

#[test]
fn test_grammar_selected_by_extension() {
    use crate::typescript::grammar_for_path;
    assert_eq!(grammar_for_path(Path::new("a/b.tsx")), "tsx");
    assert_eq!(grammar_for_path(Path::new("a/b.jsx")), "tsx");
    assert_eq!(grammar_for_path(Path::new("a/b.ts")), "typescript");
    assert_eq!(grammar_for_path(Path::new("a/b.js")), "typescript");
    assert_eq!(grammar_for_path(Path::new("a/B.svelte")), "typescript");
}

#[test]
fn test_tsx_file_with_jsx_yields_definitions() {
    let resolver = TsResolver::new();
    let source = r#"
import React from "react";

export function Badge({ label }: { label: string }) {
    return <span className="badge">{label}</span>;
}
"#;
    let result = resolver.parse_file(Path::new("Badge.tsx"), source);
    let names: Vec<&str> = result
        .definitions
        .iter()
        .filter(|d| d.kind == keel_core::types::NodeKind::Function)
        .map(|d| d.name.as_str())
        .collect();
    assert!(
        names.contains(&"Badge"),
        "JSX component must be extracted with the tsx grammar, got {names:?}"
    );
}

#[test]
fn test_jsx_file_with_jsx_yields_definitions() {
    let resolver = TsResolver::new();
    let source = r#"
export function Card(props) {
    return <div className="card">{props.children}</div>;
}
"#;
    let result = resolver.parse_file(Path::new("Card.jsx"), source);
    let names: Vec<&str> = result
        .definitions
        .iter()
        .filter(|d| d.kind == keel_core::types::NodeKind::Function)
        .map(|d| d.name.as_str())
        .collect();
    assert!(names.contains(&"Card"), "got {names:?}");
}

// ---------------------------------------------------------------------------
// Svelte components
// ---------------------------------------------------------------------------

#[test]
fn test_svelte_component_is_a_module_node() {
    let resolver = TsResolver::new();
    let source = "<h1>no script here</h1>\n";
    let result = resolver.parse_file(Path::new("src/lib/Plain.svelte"), source);
    let modules: Vec<_> = result
        .definitions
        .iter()
        .filter(|d| d.kind == keel_core::types::NodeKind::Module)
        .collect();
    assert_eq!(modules.len(), 1, "script-less component is still a module");
    assert_eq!(modules[0].name, "Plain");
}

#[test]
fn test_svelte_script_definitions_and_line_numbers() {
    let resolver = TsResolver::new();
    let source = "<script lang=\"ts\">\n  import { helper } from '$lib/util';\n\n  export function onClick(id: number): void {\n    helper(id);\n  }\n</script>\n\n<button on:click={() => onClick(1)}>go</button>\n";
    let result = resolver.parse_file(Path::new("src/lib/Btn.svelte"), source);

    let func = result
        .definitions
        .iter()
        .find(|d| d.name == "onClick")
        .expect("function inside <script> must be extracted");
    assert_eq!(
        func.line_start, 4,
        "line number must match the original .svelte file"
    );
    assert!(func.type_hints_present);

    assert!(
        result.imports.iter().any(|i| i.source.contains("util")),
        "imports inside <script> must be extracted"
    );
}

#[test]
fn test_svelte_template_only_handlers_become_references() {
    // Handlers wired up only from the (blanked) template markup must still show
    // up as references, so W005 doesn't read them as dead. A script fn used
    // nowhere stays unreferenced. (issue #39)
    let resolver = TsResolver::new();
    let source = "<script lang=\"ts\">\n\
         function addZuschlag() {}\n\
         function startEdit() {}\n\
         function reallyUnused() {}\n\
         let editing = false;\n\
         </script>\n\
         <button on:click={addZuschlag}>add</button>\n\
         {#if editing}\n\
         <span>{startEdit()}</span>\n\
         {/if}\n";
    let result = resolver.parse_file(Path::new("src/lib/Form.svelte"), source);
    let names: std::collections::HashSet<&str> =
        result.references.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains("addZuschlag"),
        "template on:click handler counts as a reference: {names:?}"
    );
    assert!(
        names.contains("startEdit"),
        "{{#if}}-block call counts as a reference: {names:?}"
    );
    assert!(
        !names.contains("reallyUnused"),
        "a script fn used nowhere stays unreferenced: {names:?}"
    );
}

#[test]
fn test_svelte_supported_extension_registered() {
    let resolver = TsResolver::new();
    assert!(resolver.supported_extensions().contains(&"svelte"));
}

#[test]
fn test_sveltekit_virtual_modules_left_external() {
    let resolver = TsResolver::new();
    let source = "<script lang=\"ts\">\n  import { goto } from '$app/navigation';\n  export function nav(): void { goto('/x'); }\n</script>\n";
    let result = resolver.parse_file(Path::new("src/routes/+page.svelte"), source);
    let app_import = result
        .imports
        .iter()
        .find(|i| i.source.starts_with("$app"))
        .expect("$app import retained as-is");
    assert_eq!(app_import.source, "$app/navigation");
    assert!(!app_import.is_relative);
}
