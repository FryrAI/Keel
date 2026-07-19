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
//! lexical scan of the markup for identifiers that name script definitions.

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

/// Emits a [`Reference`] for every script-defined identifier that appears in a
/// Svelte template expression — `{ident}`, `on:click={ident}`, `{#if ident}`,
/// `bind:value={ident}`, `{ident(...)}`, and the like.
///
/// The blanked template is invisible to tree-sitter, so a handler wired up only
/// from markup would otherwise look like dead code. This is a deliberately
/// lexical scan (no Svelte parser): `<script>`/`<style>` bodies are blanked,
/// then only text inside `{ ... }` expression braces is tokenised — static
/// attribute strings and prose are ignored, string literals within the braces
/// are skipped, and each identifier is matched whole-word against `defined`.
/// Matches become `Call` references attributed to the component file, which is
/// enough for W005 caller counts and for `keel map` to draw call edges.
pub(crate) fn extract_template_references(
    content: &str,
    defined: &HashSet<String>,
    file_path: &str,
) -> Vec<Reference> {
    if defined.is_empty() {
        return Vec::new();
    }
    let markup = markup_only(content.as_bytes());
    let mut refs = Vec::new();
    let mut seen: HashSet<(String, u32)> = HashSet::new();

    let mut i = 0usize;
    let mut line = 1u32;
    let mut depth: i32 = 0;
    let mut quote: u8 = 0;

    while i < markup.len() {
        let b = markup[i];
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
                while i < markup.len() && is_ident_part(markup[i]) {
                    i += 1;
                }
                let word = std::str::from_utf8(&markup[start..i]).unwrap_or("");
                if defined.contains(word) && seen.insert((word.to_string(), line)) {
                    refs.push(Reference {
                        name: word.to_string(),
                        file_path: file_path.to_string(),
                        line,
                        kind: ReferenceKind::Call,
                        resolved_to: None,
                    });
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

/// A copy of `bytes` with `<script>` and `<style>` bodies blanked to spaces
/// (newlines preserved for line accuracy), leaving only the template markup.
fn markup_only(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    for (start, end) in script_regions(bytes)
        .into_iter()
        .chain(style_regions(bytes))
    {
        for b in &mut out[start..end] {
            if *b != b'\n' && *b != b'\r' {
                *b = b' ';
            }
        }
    }
    out
}

/// Byte ranges of every `<style>` body — the CSS analogue of
/// [`script_regions`], minus the `<svelte:head>` handling (component styles
/// never live there). Blanked before template scanning so CSS `{ }` blocks are
/// not mistaken for Svelte expressions.
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
