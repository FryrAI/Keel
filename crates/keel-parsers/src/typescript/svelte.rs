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
//! The template/markup section is never parsed.

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
