//! Minimal `.svelte` single-file-component support.
//!
//! Svelte components are not valid TypeScript, but their `<script>` blocks are.
//! [`extract_script_source`] returns a copy of the component in which everything
//! outside a `<script>` block is blanked to spaces, leaving the script bodies
//! byte-for-byte in place.
//!
//! Blanking (rather than slicing) is deliberate: byte offsets and line numbers
//! are preserved exactly, so definition line numbers reported by tree-sitter
//! point at the real lines of the `.svelte` file.
//!
//! The template/markup section is never parsed. To keep template-driven usage
//! from reading as dead code, [`extract_template_references`] still does a
//! lexical scan of the markup for identifiers that name script definitions or
//! imported bindings.
//!
//! ## The 400-line file that "desynced" (it did not)
//!
//! A 599-line SvelteKit route defined `refreshOverview` in its `<script>` and
//! used it at line 122 as `<FristenPanel onRejected={refreshOverview} />`, yet
//! reported zero callers — while the identical shape resolved in a small
//! fixture. The obvious suspect was this scanner: a brace/quote tokenizer that
//! desyncs somewhere in a long file would silently stop recognising every
//! expression after the desync point, and nobody had bounded that.
//!
//! Instrumenting the `(skip-region, depth, quote)` state of the scan over that
//! exact file disproved it. The scan stayed in sync end to end — it jumped both
//! skip regions exactly on their boundaries, ended at `depth == 0, quote == 0`,
//! and *did* report `refreshOverview` at line 122 and `completenessPct` at line
//! 222. Long files, nested `{#if}`/`{#each}`, template literals with `${}`, and
//! German typographic quotes all pass through it intact (`large_template_*`
//! tests below pin this).
//!
//! The two real causes were elsewhere:
//! 1. `treesitter::imports` collapsed every named import down to the first
//!    specifier, so nothing recorded that the statement's other bindings
//!    existed — the caller-side symptom looked identical to a scanner miss.
//! 2. This scan was seeded with local definitions only, so an *imported*
//!    function used exclusively from markup (`{completenessPct(v)}`) was
//!    invisible by construction.
//!
//! Both are fixed; the lesson kept here is that a scanner miss and a missing
//! seed present the same way from `keel search`, so instrument before rewriting.

use std::collections::HashSet;

use crate::resolver::{Reference, ReferenceKind};

/// Returns true if `path` is a Svelte single-file component.
pub(crate) fn is_svelte_file(path: &std::path::Path) -> bool {
    path.extension().is_some_and(|e| e == "svelte")
}

/// Rewrites a `.svelte` component so only its `<script>` block contents remain.
///
/// Every byte outside a script body — markup, styles, and the `<script>` /
/// `</script>` tags themselves — is replaced with a space, except newlines and
/// carriage returns which are kept. The result therefore has the same byte
/// length, the same line count, and the same offsets as the input, and is valid
/// input for the TypeScript grammar.
///
/// All script blocks are kept, including `<script context="module">` and
/// `<script lang="ts">`. A component with no script block yields an all-blank
/// string, which still parses to an (empty) module.
pub(crate) fn extract_script_source(content: &str) -> String {
    let bytes = content.as_bytes();
    let mut out = blank_template(bytes);

    for (start, end) in script_regions(bytes) {
        out[start..end].copy_from_slice(&bytes[start..end]);
    }

    // `out` is the original bytes in script regions (valid UTF-8 boundaries,
    // since regions start/end at tag boundaries) and ASCII elsewhere. Fail
    // CLOSED: if a region boundary ever split a multi-byte char, fall back to
    // the all-blank template (empty module) rather than handing raw markup to
    // the TypeScript grammar.
    String::from_utf8(out).unwrap_or_else(|_| {
        String::from_utf8(blank_template(bytes)).expect("blank template is pure ASCII")
    })
}

/// The blanking rule, in one place: newlines and carriage returns survive,
/// every other byte becomes a space. Pure ASCII output, so always valid UTF-8.
fn blank_template(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .map(|&b| if b == b'\n' || b == b'\r' { b } else { b' ' })
        .collect()
}

/// Finds the byte ranges of every `<script>` body in `bytes`.
///
/// Each range starts immediately after the opening tag's `>` and ends at the
/// `<` of the matching `</script`. Two deliberate exclusions:
/// - scripts inside `<svelte:head>` (vendor snippets like analytics tags) are
///   not component source and are skipped;
/// - an unterminated `<script>` is dropped entirely rather than letting the
///   region run to end of input and swallow markup.
fn script_regions(bytes: &[u8]) -> Vec<(usize, usize)> {
    let head_ranges = svelte_head_ranges(bytes);
    let mut regions = Vec::new();
    let mut i = 0usize;

    while let Some(tag_start) = find_ci(bytes, b"<script", i) {
        if let Some(&(_, head_end)) = head_ranges
            .iter()
            .find(|(s, e)| (*s..*e).contains(&tag_start))
        {
            i = head_end;
            continue;
        }
        // Reject `<scriptFoo` — the tag name must end here.
        let after_name = tag_start + b"<script".len();
        match bytes.get(after_name) {
            Some(c) if c.is_ascii_whitespace() || *c == b'>' || *c == b'/' => {}
            _ => {
                i = after_name;
                continue;
            }
        }

        let Some(open_end) = find_tag_end(bytes, after_name) else {
            break;
        };
        // Self-closing `<script src="..." />` has no body.
        if bytes[open_end - 1] == b'/' {
            i = open_end + 1;
            continue;
        }
        let body_start = open_end + 1;

        match find_ci(bytes, b"</script", body_start) {
            Some(close_start) => {
                regions.push((body_start, close_start));
                i = close_start + b"</script".len();
            }
            None => break,
        }
    }

    regions
}

/// Byte ranges spanning `<svelte:head>...</svelte:head>` blocks (inclusive of
/// the closing tag).
///
/// False opens must never produce a range that swallows later `<script>`
/// blocks, so a candidate is rejected unless (a) it is really a tag (the name
/// is followed by `>`, `/`, or whitespace), (b) its open tag closes, and
/// (c) no other `<svelte:head` open sits between it and the matched close —
/// if one does, the close belongs to that later, real block (the candidate
/// was text inside a string literal).
fn svelte_head_ranges(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut i = 0usize;
    while let Some(start) = find_ci(bytes, b"<svelte:head", i) {
        let after_name = start + b"<svelte:head".len();
        let is_tag = matches!(bytes.get(after_name),
            Some(c) if c.is_ascii_whitespace() || *c == b'>' || *c == b'/');
        if !is_tag {
            i = after_name;
            continue;
        }
        let Some(open_end) = find_tag_end(bytes, after_name) else {
            i = after_name;
            continue;
        };
        let close = find_ci(bytes, b"</svelte:head", open_end);
        let next_open = find_ci(bytes, b"<svelte:head", open_end);
        match close {
            Some(close) if next_open.is_none_or(|n| n > close) => {
                let end = (close + b"</svelte:head>".len()).min(bytes.len());
                ranges.push((start, end));
                i = end;
            }
            _ => i = after_name,
        }
    }
    ranges
}

/// Returns the index of the `>` closing the tag that starts at `from`,
/// skipping over quoted attribute values.
fn find_tag_end(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => quote = Some(b),
            None if b == b'>' => return Some(i),
            None => {}
        }
        i += 1;
    }
    None
}

/// Emits a [`Reference`] for every script-defined or imported identifier that
/// appears in a Svelte template expression — `{ident}`, `on:click={ident}`,
/// `{#if ident}`, `bind:value={ident}`, `{@const x = ident(v)}`, and the like.
///
/// A handler wired up only from markup would otherwise look like dead code to
/// tree-sitter. This is a deliberately lexical scan (no Svelte parser):
/// `<script>`/`<style>` bodies are skipped in place, and only text inside
/// `{ ... }` expression braces is tokenised — static attribute strings and prose
/// are ignored, string literals within the braces are skipped, and each
/// identifier is matched whole-word against `defined` and `imported`.
///
/// The two sets differ in what the match proves, so they produce different
/// references:
/// - `defined` — this component's own `<script>` functions and classes. A hit
///   is a `Call` reference: the definition is right there in the same file.
/// - `imported` — local binding names introduced by the component's imports
///   (named, default, and namespace-alias). A hit is a
///   [`ReferenceKind::Template`] reference, which becomes a `uses` edge at
///   `TEMPLATE_LEXICAL` confidence under the `tier1_template` tier. It carries
///   no argument list, so it must never reach E001/E004/E005 or a fix plan; it
///   exists so W005, `discover` and `focus` see the usage.
///
/// Scope, deliberately: `defined` is filtered to functions and classes by the
/// caller, so an imported *constant* or Svelte *store* used from markup stays
/// invisible — those have no graph node to point at. Markup component tags
/// (`<FristenPanel/>`) are not matched either; they are not brace expressions
/// and adding them is a separate change with its own coupling-metric fallout.
pub(crate) fn extract_template_references(
    content: &str,
    defined: &HashSet<String>,
    imported: &HashSet<String>,
    file_path: &str,
) -> Vec<Reference> {
    if defined.is_empty() && imported.is_empty() {
        return Vec::new();
    }
    let bytes = content.as_bytes();

    // `<script>` and `<style>` bodies are not template markup. Collect their
    // byte ranges once and sort by start so the scan can jump over each in place
    // — no blanked full-buffer copy. Blanking previously turned these bytes into
    // spaces, which never changed the brace/quote state; the only visible effect
    // was newline counting, preserved on each jump below, so this is equivalent.
    let mut skips: Vec<(usize, usize)> = script_regions(bytes)
        .into_iter()
        .chain(style_regions(bytes))
        .collect();
    skips.sort_unstable_by_key(|&(start, _)| start);

    let mut refs = Vec::new();
    let mut seen: HashSet<(String, u32)> = HashSet::new();

    let mut i = 0usize;
    let mut line = 1u32;
    let mut depth: i32 = 0;
    let mut quote: u8 = 0;
    let mut next_skip = 0usize;

    while i < bytes.len() {
        // Jump over the next script/style body, counting its newlines so line
        // numbers stay accurate. Regions are non-overlapping and sorted, and the
        // scan advances one byte at a time up to each region's start (idents are
        // only read at `depth > 0`, and the byte before a body is always the
        // tag's `>`), so `i` always lands exactly on the boundary.
        if next_skip < skips.len() && i == skips[next_skip].0 {
            let (start, end) = skips[next_skip];
            line += bytes[start..end].iter().filter(|&&b| b == b'\n').count() as u32;
            i = end;
            next_skip += 1;
            continue;
        }

        let b = bytes[i];
        if b == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if depth == 0 {
            if b == b'{' {
                depth = 1;
            }
            i += 1;
            continue;
        }
        if quote != 0 {
            if b == quote {
                quote = 0;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' | b'\'' | b'`' => {
                quote = b;
                i += 1;
            }
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                i += 1;
            }
            _ if is_ident_start(b) => {
                let start = i;
                i += 1;
                while i < bytes.len() && is_ident_part(bytes[i]) {
                    i += 1;
                }
                let word = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
                // A local definition wins over an import of the same name: it
                // is the binding the markup actually sees.
                let kind = if defined.contains(word) {
                    Some(ReferenceKind::Call)
                } else if imported.contains(word) {
                    Some(ReferenceKind::Template)
                } else {
                    None
                };
                if let Some(kind) = kind {
                    if seen.insert((word.to_string(), line)) {
                        refs.push(Reference {
                            name: word.to_string(),
                            file_path: file_path.to_string(),
                            line,
                            kind,
                            resolved_to: None,
                            call_arity: None,
                        });
                    }
                }
            }
            _ => i += 1,
        }
    }

    refs
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$'
}

fn is_ident_part(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Byte ranges of every `<style>` body — the CSS analogue of
/// [`script_regions`], minus the `<svelte:head>` handling (component styles
/// never live there). The template scan jumps over these ranges in place so
/// CSS `{ }` blocks are not mistaken for Svelte expressions.
fn style_regions(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut regions = Vec::new();
    let mut i = 0usize;
    while let Some(tag_start) = find_ci(bytes, b"<style", i) {
        let after_name = tag_start + b"<style".len();
        match bytes.get(after_name) {
            Some(c) if c.is_ascii_whitespace() || *c == b'>' || *c == b'/' => {}
            _ => {
                i = after_name;
                continue;
            }
        }
        let Some(open_end) = find_tag_end(bytes, after_name) else {
            break;
        };
        if bytes[open_end - 1] == b'/' {
            i = open_end + 1;
            continue;
        }
        let body_start = open_end + 1;
        match find_ci(bytes, b"</style", body_start) {
            Some(close_start) => {
                regions.push((body_start, close_start));
                i = close_start + b"</style".len();
            }
            None => break,
        }
    }
    regions
}

/// ASCII case-insensitive search for `needle` in `haystack` starting at `from`.
fn find_ci(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= haystack.len() || needle.is_empty() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle))
        .map(|p| p + from)
}

#[cfg(test)]
#[path = "svelte_tests.rs"]
mod tests;
