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
// JSX element usage as references (W005 false-positive fix)
// ---------------------------------------------------------------------------
//
// typescript.scm has no JSX captures, so a component used only as `<Comp />`
// produced zero references and read as W005 dead code. typescript_jsx.scm
// (compiled ONLY against the TSX grammar) fixes this.

#[test]
fn test_jsx_self_closing_component_usage_is_a_reference() {
    use crate::resolver::ReferenceKind;

    let resolver = TsResolver::new();
    let source = r#"
function Comp() {
    return <span>hi</span>;
}

export function App() {
    return <Comp />;
}
"#;
    let result = resolver.parse_file(Path::new("App.tsx"), source);
    let comp_ref = result
        .references
        .iter()
        .find(|r| r.name == "Comp")
        .unwrap_or_else(|| {
            panic!(
                "JSX self-closing usage must count as a reference, got {:?}",
                result
                    .references
                    .iter()
                    .map(|r| &r.name)
                    .collect::<Vec<_>>()
            )
        });
    assert_eq!(
        comp_ref.kind,
        ReferenceKind::Value,
        "JSX usage is a value reference, not a call"
    );
}

#[test]
fn test_jsx_paired_element_component_usage_is_a_reference() {
    let resolver = TsResolver::new();
    let source = r#"
function Panel() {
    return <div />;
}

export function App() {
    return <Panel><span>child</span></Panel>;
}
"#;
    let result = resolver.parse_file(Path::new("App.tsx"), source);
    let names: Vec<&str> = result.references.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"Panel"),
        "opening tag of a paired JSX element must count as a reference: {names:?}"
    );
}

#[test]
fn test_jsx_member_expression_component_usage_is_a_reference() {
    let resolver = TsResolver::new();
    let source = r#"
export function App() {
    return <Foo.Bar />;
}
"#;
    let result = resolver.parse_file(Path::new("App.tsx"), source);
    let names: std::collections::HashSet<&str> =
        result.references.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains("Foo.Bar"),
        "namespaced JSX name must be captured whole: {names:?}"
    );
    assert!(
        names.contains("Foo"),
        "the root of a namespaced JSX name must also count as a usage: {names:?}"
    );
}

#[test]
fn test_jsx_intrinsic_elements_yield_no_reference() {
    let resolver = TsResolver::new();
    let source = r#"
export function App() {
    return <div className="x"><span>hi</span></div>;
}
"#;
    let result = resolver.parse_file(Path::new("App.tsx"), source);
    let names: Vec<&str> = result.references.iter().map(|r| r.name.as_str()).collect();
    assert!(
        !names.contains(&"div") && !names.contains(&"span"),
        "lowercase intrinsic HTML elements must not become references: {names:?}"
    );
}

#[test]
fn test_jsx_attribute_value_identifier_is_a_reference() {
    use crate::resolver::ReferenceKind;

    let resolver = TsResolver::new();
    let source = r#"
function clickHandler() {}

export function App() {
    return <C onClick={clickHandler} label="x" />;
}
"#;
    let result = resolver.parse_file(Path::new("App.tsx"), source);
    let names: Vec<&str> = result.references.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"clickHandler"),
        "a bare identifier in a JSX attribute value must count as a reference, \
         even though it's lowercase: {names:?}"
    );
    assert!(
        !names.contains(&"onClick"),
        "the attribute NAME must never be captured as a reference: {names:?}"
    );
    let handler_ref = result
        .references
        .iter()
        .find(|r| r.name == "clickHandler")
        .expect("clickHandler must be present");
    assert_eq!(handler_ref.kind, ReferenceKind::Value);
}

#[test]
fn test_jsx_child_expression_identifier_is_a_reference() {
    let resolver = TsResolver::new();
    let source = r#"
function child() {}

export function App() {
    return <div>{child}</div>;
}
"#;
    let result = resolver.parse_file(Path::new("App.tsx"), source);
    let names: Vec<&str> = result.references.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.contains(&"child"),
        "a bare identifier used as a JSX expression child must count as a reference: {names:?}"
    );
}

#[test]
fn test_plain_ts_file_still_parses_with_jsx_query_added() {
    // Guards the TSX-only wiring in queries::query_for_language: the JSX
    // fragment must never reach a plain .ts file's query compilation.
    let resolver = TsResolver::new();
    let source = r#"
function helper(): void {}

export function run(): void {
    helper();
}
"#;
    let result = resolver.parse_file(Path::new("plain.ts"), source);
    assert!(
        result.definitions.iter().any(|d| d.name == "helper"),
        "got {:?}",
        result.definitions
    );
    assert!(
        result.references.iter().any(|r| r.name == "helper"),
        "got {:?}",
        result.references
    );
}

// ---------------------------------------------------------------------------
// Svelte components
// ---------------------------------------------------------------------------

#[test]
fn test_svelte_component_without_script_yields_no_definitions() {
    // The whole-file Module node is created by `keel map`'s first pass (one
    // path-named module per walked file), NOT by the parser. A script-less
    // component therefore parses cleanly to zero definitions; it still becomes
    // a module node in the graph via the map pass.
    let resolver = TsResolver::new();
    let source = "<h1>no script here</h1>\n";
    let result = resolver.parse_file(Path::new("src/lib/Plain.svelte"), source);
    assert!(
        result.definitions.is_empty(),
        "parser must not inject a synthetic module def, got {:?}",
        result
            .definitions
            .iter()
            .map(|d| &d.name)
            .collect::<Vec<_>>()
    );
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
