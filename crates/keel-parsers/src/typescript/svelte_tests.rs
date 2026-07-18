//! Unit tests for `.svelte` script extraction.

use super::*;

/// Every extraction must leave byte length and line count untouched.
fn assert_shape_preserved(input: &str, output: &str) {
    assert_eq!(input.len(), output.len(), "byte length changed");
    assert_eq!(
        input.matches('\n').count(),
        output.matches('\n').count(),
        "line count changed"
    );
}

#[test]
fn extracts_plain_script_block() {
    let src = "<script>\nlet a = 1;\n</script>\n<h1>hi</h1>\n";
    let out = extract_script_source(src);
    assert_shape_preserved(src, &out);
    assert!(out.contains("let a = 1;"));
    assert!(!out.contains("h1"));
}

#[test]
fn extracts_lang_ts_script_block() {
    let src = "<script lang=\"ts\">\nexport function f(x: number): number { return x; }\n</script>\n<div/>\n";
    let out = extract_script_source(src);
    assert_shape_preserved(src, &out);
    assert!(out.contains("export function f(x: number): number"));
    assert!(!out.contains("div"));
    assert!(
        !out.contains("lang="),
        "the open tag itself must be blanked"
    );
}

#[test]
fn preserves_line_numbers_of_definitions() {
    let src =
        "<div>markup</div>\n\n<script lang=\"ts\">\n  export function target() {}\n</script>\n";
    let out = extract_script_source(src);
    assert_shape_preserved(src, &out);

    // `target` must still sit on line 4 (1-based) after blanking.
    let line = out
        .lines()
        .position(|l| l.contains("export function target"))
        .expect("definition retained")
        + 1;
    assert_eq!(line, 4);
}

#[test]
fn handles_multiple_script_blocks() {
    let src = "<script context=\"module\">\nexport const PRE = 1;\n</script>\n<p>x</p>\n<script>\nlet b = 2;\n</script>\n";
    let out = extract_script_source(src);
    assert_shape_preserved(src, &out);
    assert!(out.contains("export const PRE = 1;"));
    assert!(out.contains("let b = 2;"));
    assert!(!out.contains("<p>"));
}

#[test]
fn component_without_script_yields_blank_source() {
    let src = "<h1>Just markup</h1>\n<style>h1 { color: red; }</style>\n";
    let out = extract_script_source(src);
    assert_shape_preserved(src, &out);
    assert!(
        out.trim().is_empty(),
        "expected all-blank output, got {out:?}"
    );
}

#[test]
fn style_block_is_not_treated_as_script() {
    let src = "<script>\nlet a = 1;\n</script>\n<style>\n.x { content: '</script>'; }\n</style>\n";
    let out = extract_script_source(src);
    assert_shape_preserved(src, &out);
    assert!(out.contains("let a = 1;"));
    assert!(!out.contains("color"));
}

#[test]
fn non_ascii_markup_keeps_byte_offsets() {
    let src = "<p>Grüße — Ünïcode</p>\n<script>\nconst x = 'ä';\n</script>\n";
    let out = extract_script_source(src);
    assert_shape_preserved(src, &out);
    // The script body keeps its own multi-byte content intact.
    assert!(out.contains("const x = 'ä';"));
    // The offset of the script body is identical in both strings.
    assert_eq!(
        src.find("const x").unwrap(),
        out.find("const x").unwrap(),
        "byte offset of script body shifted"
    );
}

#[test]
fn unterminated_script_is_dropped_not_extended_to_eof() {
    // Extending an unterminated region to EOF would swallow markup into the
    // TypeScript grammar; dropping it yields a safe empty module instead.
    let src = "<script>\nlet a = 1;\n<h1>markup {name}</h1>\n";
    let out = extract_script_source(src);
    assert_shape_preserved(src, &out);
    assert!(out.trim().is_empty());
}

#[test]
fn svelte_head_text_in_string_does_not_swallow_later_scripts() {
    // "<svelte:head>" inside a string literal has no close tag; it must not
    // create an exclusion range that drops every later script block.
    let src = "<script lang=\"ts\">\nconst TPL = '<svelte:head>';\n</script>\n<script context=\"module\">\nexport function load(): number { return 1; }\n</script>\n";
    let out = extract_script_source(src);
    assert_shape_preserved(src, &out);
    assert!(out.contains("const TPL"));
    assert!(out.contains("export function load()"));
}

#[test]
fn svelte_head_string_before_real_head_block_keeps_scripts() {
    // The false open in the string must not pair with the REAL head block's
    // close tag — that range would swallow the second script entirely.
    let src = "<script context=\"module\">\nexport const HEAD_TAG = \"<svelte:head\";\n</script>\n<script>\nexport function greet(): string { return \"hi\"; }\n</script>\n<svelte:head><title>Page</title></svelte:head>\n";
    let out = extract_script_source(src);
    assert_shape_preserved(src, &out);
    assert!(out.contains("HEAD_TAG"));
    assert!(out.contains("export function greet()"));
    assert!(!out.contains("<title>"));
}

#[test]
fn script_inside_svelte_head_is_not_component_source() {
    // Vendor snippets (analytics tags) in <svelte:head> must not produce
    // graph nodes or E002/E003 noise the developer cannot fix.
    let src = "<svelte:head>\n  <script>function gtag(){dataLayer.push(arguments)}</script>\n</svelte:head>\n<script lang=\"ts\">\nexport function real(): number { return 1; }\n</script>\n<h1>hi</h1>\n";
    let out = extract_script_source(src);
    assert_shape_preserved(src, &out);
    assert!(!out.contains("gtag"), "head script must be blanked");
    assert!(out.contains("export function real()"));
}

#[test]
fn self_closing_script_tag_has_no_body() {
    let src = "<script src=\"x.js\" />\n<h1>hi</h1>\n";
    let out = extract_script_source(src);
    assert_shape_preserved(src, &out);
    assert!(out.trim().is_empty());
}

#[test]
fn tag_name_must_match_exactly() {
    let src = "<scripty>not a script</scripty>\n";
    let out = extract_script_source(src);
    assert!(out.trim().is_empty());
}

#[test]
fn detects_svelte_files_by_extension() {
    assert!(is_svelte_file(std::path::Path::new("/a/B.svelte")));
    assert!(!is_svelte_file(std::path::Path::new("/a/B.ts")));
}
