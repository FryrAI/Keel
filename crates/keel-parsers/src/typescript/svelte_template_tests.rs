//! Template-scan fixtures for the shapes SvelteKit apps actually ship (T1.3).
//!
//! Two things are pinned here:
//! 1. an *imported* binding named from markup yields a
//!    [`ReferenceKind::Template`] reference (it used to yield nothing, so the
//!    imported function read as dead code), while a locally defined one keeps
//!    its `Call` reference;
//! 2. the scan does not desync on a long, realistic component — the
//!    `large_template_*` tests are the regression guard for the investigation
//!    written up at the top of `svelte.rs`.

use std::collections::HashSet;

use super::svelte::extract_template_references;
use crate::resolver::{Reference, ReferenceKind};

fn set(names: &[&str]) -> HashSet<String> {
    names.iter().map(|s| s.to_string()).collect()
}

/// Scan `src` with `imported` as the only seed (no local definitions).
fn imported_refs(src: &str, imported: &[&str]) -> Vec<Reference> {
    extract_template_references(src, &set(&[]), &set(imported), "Page.svelte")
}

fn names(refs: &[Reference]) -> HashSet<&str> {
    refs.iter().map(|r| r.name.as_str()).collect()
}

/// Every reference must be a template reference — never a `Call`, which would
/// let a lexical markup match reach E001/E004/E005.
fn assert_all_template(refs: &[Reference]) {
    assert!(
        refs.iter().all(|r| r.kind == ReferenceKind::Template),
        "imported markup hits must be Template refs: {:?}",
        refs.iter().map(|r| (&r.name, &r.kind)).collect::<Vec<_>>()
    );
}

#[test]
fn at_const_in_markup_references_an_imported_function() {
    // The production shape: `{@const}` inside an `{#each}` body calling a
    // helper that lives in another module.
    let src = "<script lang=\"ts\">\n\
         import { completenessPct } from '$lib/portfolio/model';\n\
         let rows = [];\n\
         </script>\n\
         {#each rows as v}\n\
           {@const pct = completenessPct(v)}\n\
           <div>{pct}%</div>\n\
         {/each}\n";
    let refs = imported_refs(src, &["completenessPct"]);
    assert!(
        names(&refs).contains("completenessPct"),
        "{{@const}} call must be seen: {refs:?}"
    );
    assert_all_template(&refs);
    assert_eq!(refs[0].line, 6, "line must point at the markup line");
}

#[test]
fn each_block_body_references_an_imported_function() {
    let src = "<script>\n\
         import { fristLabel } from '$lib/portfolio/model';\n\
         </script>\n\
         {#each rows as r (r.id)}\n\
           <span>{fristLabel(r)}</span>\n\
         {/each}\n";
    assert!(names(&imported_refs(src, &["fristLabel"])).contains("fristLabel"));
}

#[test]
fn snippet_block_body_references_an_imported_function() {
    let src = "<script>\n\
         import { typFromAz } from '$lib/portfolio/model';\n\
         </script>\n\
         {#snippet row(v)}\n\
           <td>{typFromAz(v.az)}</td>\n\
         {/snippet}\n\
         {@render row(first)}\n";
    assert!(names(&imported_refs(src, &["typFromAz"])).contains("typFromAz"));
}

#[test]
fn await_block_branches_reference_imported_functions() {
    let src = "<script>\n\
         import { offenTotal, fristUrgency } from '$lib/portfolio/model';\n\
         </script>\n\
         {#await promise}\n\
           <p>lädt…</p>\n\
         {:then value}\n\
           <p>{offenTotal(value)}</p>\n\
         {:catch err}\n\
           <p>{fristUrgency(err)}</p>\n\
         {/await}\n";
    let refs = imported_refs(src, &["offenTotal", "fristUrgency"]);
    let found = names(&refs);
    assert!(found.contains("offenTotal"), "then-branch: {found:?}");
    assert!(found.contains("fristUrgency"), "catch-branch: {found:?}");
}

#[test]
fn derived_rune_in_markup_references_an_imported_function() {
    // Svelte 5 runes read as ordinary calls to the scan; a `$derived`
    // initializer written inline in markup must still resolve.
    let src = "<script>\n\
         import { applyView } from '$lib/portfolio/model';\n\
         </script>\n\
         {#if ready}\n\
           <List rows={applyView(view, today)} />\n\
         {/if}\n";
    assert!(names(&imported_refs(src, &["applyView"])).contains("applyView"));
}

#[test]
fn german_typographic_quotes_do_not_swallow_later_expressions() {
    // „…" and — are multi-byte and must not be mistaken for string delimiters;
    // an ASCII apostrophe in prose sits outside any brace and must not either.
    let src = "<script>\n\
         import { fristLabel } from '$lib/portfolio/model';\n\
         </script>\n\
         <p>Die Frist „Verfahrenseröffnung\u{201c} — Ben's Notiz — läuft ab.</p>\n\
         <p>{fristLabel(row)}</p>\n";
    let refs = imported_refs(src, &["fristLabel"]);
    assert!(
        names(&refs).contains("fristLabel"),
        "expression after typographic quotes must still be scanned: {refs:?}"
    );
}

#[test]
fn a_local_definition_wins_over_an_import_of_the_same_name() {
    let src = "<script>\n\
         import { refresh } from '$lib/x';\n\
         function refresh() {}\n\
         </script>\n\
         <button onclick={refresh}>go</button>\n";
    let refs = extract_template_references(src, &set(&["refresh"]), &set(&["refresh"]), "P.svelte");
    assert_eq!(refs.len(), 1);
    assert_eq!(
        refs[0].kind,
        ReferenceKind::Call,
        "the same-file definition is the binding the markup sees"
    );
}

#[test]
fn component_tags_are_not_matched() {
    // Deferred (T3.3): matching `<FristenPanel/>` needs a non-brace scan branch
    // and adds a route->component edge per child, which moves audit coupling
    // metrics. Only the handler *inside* the braces is a reference today.
    let src = "<script>\n\
         import FristenPanel from '$lib/graph/FristenPanel.svelte';\n\
         import { refreshOverview } from '$lib/x';\n\
         </script>\n\
         <FristenPanel onRejected={refreshOverview} />\n";
    let refs = imported_refs(src, &["FristenPanel", "refreshOverview"]);
    let found = names(&refs);
    assert!(found.contains("refreshOverview"), "{found:?}");
    assert!(
        !found.contains("FristenPanel"),
        "component tags stay out of the graph until T3.3: {found:?}"
    );
}

/// Builds a component in the shape that made the scanner a suspect: a long
/// `<script>` full of braces, then ~350 lines of markup with nested
/// `{#if}`/`{#each}`, template literals, quoted attributes, German prose, and
/// only then the handler prop under test — followed by a `<style>` block.
fn large_component() -> String {
    let mut s = String::from("<script lang=\"ts\">\n");
    s.push_str("  import { completenessPct, fristLabel } from '$lib/portfolio/model';\n");
    for i in 0..60 {
        s.push_str(&format!(
            "  function helper{i}(x: number): number {{ if (x > {i}) {{ return x; }} return {i}; }}\n"
        ));
    }
    s.push_str("  function refreshOverview() { res = null; }\n");
    s.push_str("</script>\n\n");
    for i in 0..80 {
        s.push_str("{#if row.ok}\n");
        s.push_str(&format!(
            "  <a class=\"chip\" href={{`/verfahren/${{row.id}}/{i}`}} title='Ben\\'s Fall'>{{row.az}}</a>\n"
        ));
        s.push_str("  {#each rows as r (r.id)}\n");
        s.push_str("    <span class=\"lbl\">„Frist\u{201c} — {r.name} …</span>\n");
        s.push_str("  {/each}\n");
        s.push_str("{/if}\n");
    }
    s.push_str("<FristenPanel onRejected={refreshOverview} />\n");
    s.push_str("{#each rows as v}{@const pct = completenessPct(v)}<i>{pct}</i>{/each}\n");
    s.push_str("<style>\n  .chip { color: red; }\n  .lbl { width: 3px; }\n</style>\n");
    s
}

#[test]
fn large_template_still_sees_a_late_local_handler() {
    let src = large_component();
    assert!(
        src.lines().count() > 400,
        "fixture must be large enough to matter"
    );
    let refs = extract_template_references(
        &src,
        &set(&["refreshOverview"]),
        &set(&["completenessPct", "fristLabel"]),
        "Page.svelte",
    );
    let found = names(&refs);
    assert!(
        found.contains("refreshOverview"),
        "handler prop past 400 lines of markup must still be seen: {found:?}"
    );
    assert!(
        found.contains("completenessPct"),
        "imported helper after the handler must still be seen: {found:?}"
    );
    assert!(
        !found.contains("fristLabel"),
        "an unused import must NOT be invented: {found:?}"
    );
}

#[test]
fn large_template_reports_the_right_line_numbers() {
    let src = large_component();
    let refs = extract_template_references(
        &src,
        &set(&["refreshOverview"]),
        &set(&["completenessPct"]),
        "Page.svelte",
    );
    for r in &refs {
        let line = src.lines().nth(r.line as usize - 1).expect("line in range");
        assert!(
            line.contains(&r.name),
            "{} reported at line {} which reads {line:?}",
            r.name,
            r.line
        );
    }
}
