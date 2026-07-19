use xxhash_rust::xxh64::xxh64;

const BASE62_CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Encode a u64 value as a base62 string (11 chars, zero-padded).
fn base62_encode(mut value: u64) -> String {
    if value == 0 {
        return "0".repeat(11);
    }
    let mut result = Vec::with_capacity(11);
    while value > 0 {
        let idx = (value % 62) as usize;
        result.push(BASE62_CHARS[idx]);
        value /= 62;
    }
    // Pad to 11 chars
    while result.len() < 11 {
        result.push(b'0');
    }
    result.reverse();
    String::from_utf8(result).expect("base62 chars are valid UTF-8")
}

/// Compute the keel hash for a function/class node.
///
/// hash = base62(xxhash64(canonical_signature + body_normalized + docstring))
///
/// - `canonical_signature`: normalized function declaration (name, params with types, return type)
/// - `body_normalized`: AST-based normalized function body (whitespace/comments stripped)
/// - `docstring`: the docstring content, or empty string if none
pub fn compute_hash(canonical_signature: &str, body_normalized: &str, docstring: &str) -> String {
    let mut input = String::with_capacity(
        canonical_signature.len() + body_normalized.len() + docstring.len() + 2,
    );
    input.push_str(canonical_signature);
    input.push('\0'); // separator
    input.push_str(body_normalized);
    input.push('\0'); // separator
    input.push_str(docstring);

    let hash_value = xxh64(input.as_bytes(), 0);
    base62_encode(hash_value)
}

/// Compute a disambiguated hash when a collision is detected.
/// Appends the file path to the input to create a unique hash.
pub fn compute_hash_disambiguated(
    canonical_signature: &str,
    body_normalized: &str,
    docstring: &str,
    file_path: &str,
) -> String {
    let mut input = String::with_capacity(
        canonical_signature.len() + body_normalized.len() + docstring.len() + file_path.len() + 3,
    );
    input.push_str(canonical_signature);
    input.push('\0');
    input.push_str(body_normalized);
    input.push('\0');
    input.push_str(docstring);
    input.push('\0');
    input.push_str(file_path);

    let hash_value = xxh64(input.as_bytes(), 0);
    base62_encode(hash_value)
}

/// Lexical comment/string family for [`normalize_body_for_hash`].
#[derive(Clone, Copy, PartialEq)]
enum BodySyntax {
    /// C-family: `//` line comments, `/* */` block comments; `"`, `'`, and
    /// backtick string literals. Covers TypeScript/JavaScript, Go, Rust, Svelte.
    CFamily,
    /// Python: `#` line comments (no block comments); `"`, `'`, `"""`, `'''`
    /// string literals.
    Python,
}

/// Map a caller's language name (as produced by the parsers' `detect_language`)
/// to its lexical comment/string family. Anything that is not Python is treated
/// as C-family — the safe default for our four supported languages and for
/// unknown extensions.
fn body_syntax(lang: &str) -> BodySyntax {
    match lang {
        "python" => BodySyntax::Python,
        _ => BodySyntax::CFamily,
    }
}

/// Strip comments from `body`, preserving string-literal contents **verbatim**.
///
/// Line comments are dropped up to (but not including) the newline; block
/// comments are replaced by a single space so they still separate tokens
/// (`a/*x*/b` → `a b`, never `ab`). String literals — including comment-looking
/// text inside them — are copied through unchanged, so `"// x"` stays `"// x"`.
///
/// This is a deliberately lexical pass (no AST), so it is a *pure, deterministic*
/// function of its input: `keel map` and `keel compile` always derive the same
/// normalized body for the same source. Exotic literals (Rust raw strings with
/// embedded quotes) may be handled imperfectly, but never non-deterministically.
fn strip_comments(body: &str, syntax: BodySyntax) -> String {
    let b = body.as_bytes();
    let n = b.len();
    let mut out: Vec<u8> = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        let c = b[i];

        // Line comments.
        if syntax == BodySyntax::Python && c == b'#' {
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if syntax == BodySyntax::CFamily && c == b'/' && i + 1 < n && b[i + 1] == b'/' {
            i += 2;
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        // Block comments (C-family only). Replace with a single space.
        if syntax == BodySyntax::CFamily && c == b'/' && i + 1 < n && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < n && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(n); // skip the closing */ (or land on EOF)
            out.push(b' ');
            continue;
        }

        // Rust lifetimes (`'a`) are an unpaired `'` — not a char literal.
        // Treating them as string openers would swallow the rest of the body
        // (and any trailing comments), so emit the tick as an ordinary token
        // and keep scanning. A `'` is a char literal only when it is `'\..'`
        // (escaped) or `'x'` (a single char followed immediately by `'`).
        if syntax == BodySyntax::CFamily && c == b'\'' {
            let is_char_lit = (i + 1 < n && b[i + 1] == b'\\')
                || (i + 2 < n && b[i + 1] != b'\'' && b[i + 2] == b'\'');
            if !is_char_lit {
                out.push(c);
                i += 1;
                continue;
            }
        }

        // String literals — content preserved verbatim.
        let is_string_delim =
            c == b'"' || c == b'\'' || (syntax == BodySyntax::CFamily && c == b'`');
        if is_string_delim {
            // Python triple-quoted string (`"""` / `'''`).
            if syntax == BodySyntax::Python && i + 2 < n && b[i + 1] == c && b[i + 2] == c {
                out.extend_from_slice(&b[i..i + 3]);
                i += 3;
                while i < n {
                    if b[i] == b'\\' && i + 1 < n {
                        out.push(b[i]);
                        out.push(b[i + 1]);
                        i += 2;
                        continue;
                    }
                    if i + 2 < n && b[i] == c && b[i + 1] == c && b[i + 2] == c {
                        out.extend_from_slice(&b[i..i + 3]);
                        i += 3;
                        break;
                    }
                    out.push(b[i]);
                    i += 1;
                }
                continue;
            }
            // Single-line string with backslash escapes.
            let quote = c;
            out.push(quote);
            i += 1;
            while i < n {
                let d = b[i];
                if d == b'\\' && i + 1 < n {
                    out.push(d);
                    out.push(b[i + 1]);
                    i += 2;
                    continue;
                }
                out.push(d);
                i += 1;
                if d == quote {
                    break;
                }
            }
            continue;
        }

        out.push(c);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Normalize a function body for **hash computation** (issue #36).
///
/// Strips comments (language-aware: `//` `/* */` for C-family, `#` for Python)
/// and collapses whitespace so that formatting-only and comment-only edits do
/// **not** change a node's hash, while any token-level code change does. String
/// literal contents are preserved verbatim — comment-looking text inside a
/// string is code, and changing it changes the hash.
///
/// Whitespace handling is language-aware:
/// - **C-family** (`{}`/`;` carry structure): all whitespace, *including
///   newlines*, collapses to single spaces, so brace/line-wrap reformatting is
///   invisible.
/// - **Python** (newlines/indentation are structural): lines are trimmed, blank
///   lines dropped, and line order preserved — parity with [`normalize_body`],
///   so re-indentation and blank-line churn are invisible without collapsing
///   distinct statements together.
///
/// This underlies `hash = base62(xxhash64(sig + body_normalized + docstring))`;
/// the caller still passes the docstring separately, so a docstring change
/// still changes the hash.
pub fn normalize_body_for_hash(body: &str, lang: &str) -> String {
    let syntax = body_syntax(lang);
    let stripped = strip_comments(body, syntax);
    match syntax {
        BodySyntax::Python => normalize_body(&stripped),
        BodySyntax::CFamily => stripped.split_whitespace().collect::<Vec<_>>().join(" "),
    }
}

/// Normalize a function body for duplicate detection.
///
/// Trims each line, drops blank lines, collapses runs of whitespace to a
/// single space, and rejoins with `\n`. The result is stable across pure
/// reformatting (re-indentation, added blank lines, realigned arguments).
///
/// **Whitespace-level only.** This is deliberately *not* the AST-based
/// normalization used by [`compute_hash`] and tracked in issue #36: it does
/// not strip comments, rename locals, or canonicalize syntax. Two bodies that
/// differ only in identifier names or comments will produce *different*
/// normalized text here.
pub fn normalize_body(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Minimum normalized body length worth indexing for duplicate detection.
///
/// Bodies shorter than this are trivial (`return null;`, `pass`) and collide
/// constantly without indicating real duplication. Enforcement thresholds must
/// be **>= this value**: indexing is gated on it, so a lower threshold would
/// query for bodies that were never indexed.
pub const MIN_INDEXED_BODY_LEN: usize = 40;

/// Fingerprint an *already normalized* body.
///
/// Use this when the caller has already computed [`normalize_body`] — for
/// example to length-gate against [`MIN_INDEXED_BODY_LEN`] — so the body is
/// not normalized twice. Passing non-normalized text produces a hash that will
/// not match [`compute_body_hash`] for the same input.
pub fn hash_normalized_body(normalized: &str) -> String {
    base62_encode(xxh64(normalized.as_bytes(), 0))
}

/// Compute the duplicate-detection fingerprint for a function body.
///
/// `body_hash = base62(xxhash64(normalize_body(body)))` — an 11-char string in
/// the same alphabet as [`compute_hash`], but a *separate* namespace: it keys
/// the body index, never the node identity.
///
/// Convenience wrapper over [`normalize_body`] + [`hash_normalized_body`]; if
/// you already hold the normalized text, call those directly.
pub fn compute_body_hash(body: &str) -> String {
    hash_normalized_body(&normalize_body(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_hash() {
        let h1 = compute_hash("fn foo(x: i32) -> i32", "x + 1", "Adds one");
        let h2 = compute_hash("fn foo(x: i32) -> i32", "x + 1", "Adds one");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_length() {
        let h = compute_hash("fn bar()", "{}", "");
        assert_eq!(h.len(), 11);
    }

    #[test]
    fn test_hash_changes_with_signature() {
        let h1 = compute_hash("fn foo(x: i32) -> i32", "x + 1", "");
        let h2 = compute_hash("fn foo(x: i64) -> i64", "x + 1", "");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_changes_with_body() {
        let h1 = compute_hash("fn foo(x: i32) -> i32", "x + 1", "");
        let h2 = compute_hash("fn foo(x: i32) -> i32", "x + 2", "");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_changes_with_docstring() {
        let h1 = compute_hash("fn foo()", "{}", "Does X");
        let h2 = compute_hash("fn foo()", "{}", "Does Y");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_disambiguated_hash_differs() {
        let h1 = compute_hash("fn foo()", "{}", "");
        let h2 = compute_hash_disambiguated("fn foo()", "{}", "", "src/a.rs");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_base62_encoding() {
        let encoded = base62_encode(0);
        assert_eq!(encoded.len(), 11);
        assert!(encoded.chars().all(|c| c == '0'));

        let encoded = base62_encode(1);
        assert_eq!(encoded.len(), 11);
    }

    // --- body hash (duplicate detection) ---

    #[test]
    fn test_body_hash_is_deterministic() {
        let body = "let x = 1;\nreturn x;";
        assert_eq!(compute_body_hash(body), compute_body_hash(body));
    }

    #[test]
    fn test_body_hash_length() {
        assert_eq!(compute_body_hash("return 1;").len(), 11);
    }

    /// Pure reformatting must not change the fingerprint.
    #[test]
    fn test_body_hash_stable_across_reformatting() {
        let original = "let x = 1;\nreturn x;";
        let reformatted = "    let    x   =  1;\n\n\t\treturn x;   \n";
        assert_eq!(
            compute_body_hash(original),
            compute_body_hash(reformatted),
            "reformatting must not change the body hash"
        );
    }

    #[test]
    fn test_body_hash_stable_across_indent_changes() {
        let flat = "if (a) {\nreturn 1;\n}";
        let indented = "        if (a) {\n            return 1;\n        }";
        assert_eq!(compute_body_hash(flat), compute_body_hash(indented));
    }

    #[test]
    fn test_body_hash_stable_across_blank_lines() {
        let dense = "let x = 1;\nreturn x;";
        let spaced = "let x = 1;\n\n\n   \n\nreturn x;\n";
        assert_eq!(compute_body_hash(dense), compute_body_hash(spaced));
    }

    #[test]
    fn test_body_hash_differs_for_different_bodies() {
        assert_ne!(
            compute_body_hash("return 1;"),
            compute_body_hash("return 2;")
        );
    }

    /// Line order is semantic — reordering must change the fingerprint.
    #[test]
    fn test_body_hash_differs_on_reordered_lines() {
        assert_ne!(
            compute_body_hash("a();\nb();"),
            compute_body_hash("b();\na();")
        );
    }

    /// Whitespace-level normalization does NOT see through renames — this is
    /// the documented boundary with issue #36's AST normalization.
    #[test]
    fn test_body_hash_differs_on_renamed_locals() {
        assert_ne!(
            compute_body_hash("let x = 1;\nreturn x;"),
            compute_body_hash("let y = 1;\nreturn y;")
        );
    }

    #[test]
    fn test_normalize_body_collapses_whitespace() {
        assert_eq!(normalize_body("  a   =    1  "), "a = 1");
    }

    #[test]
    fn test_normalize_body_drops_blank_lines() {
        assert_eq!(normalize_body("a;\n\n  \n\nb;"), "a;\nb;");
    }

    #[test]
    fn test_normalize_body_empty_input() {
        assert_eq!(normalize_body(""), "");
        assert_eq!(normalize_body("   \n\n  \n"), "");
    }

    /// An empty body still yields a valid, stable fingerprint.
    #[test]
    fn test_body_hash_of_empty_body() {
        let h = compute_body_hash("");
        assert_eq!(h.len(), 11);
        assert_eq!(h, compute_body_hash("   \n\n"));
    }

    /// The body hash is a separate namespace from the node hash.
    #[test]
    fn test_body_hash_differs_from_node_hash() {
        let body = "return 1;";
        assert_ne!(compute_body_hash(body), compute_hash("", body, ""));
    }

    /// Callers that already normalized (e.g. to length-gate) must get the same
    /// fingerprint as the convenience wrapper — no double normalization.
    #[test]
    fn test_hash_normalized_body_matches_compute_body_hash() {
        let body = "   let x = 1;\n\n   return x;  ";
        assert_eq!(
            hash_normalized_body(&normalize_body(body)),
            compute_body_hash(body)
        );
    }

    /// Normalizing twice is a no-op, so the split API cannot drift from the
    /// wrapper even if a caller over-normalizes.
    #[test]
    fn test_normalize_body_is_idempotent() {
        let body = "  a   =  1 \n\n  b = 2  ";
        let once = normalize_body(body);
        assert_eq!(normalize_body(&once), once);
        assert_eq!(hash_normalized_body(&once), compute_body_hash(body));
    }

    #[test]
    fn test_min_indexed_body_len_is_sane() {
        // A trivial body must fall below the gate; a real one above it.
        assert!(normalize_body("return null;").len() < MIN_INDEXED_BODY_LEN);
        assert!(
            normalize_body("let total = 0;\nfor (x of xs) { total += x; }\nreturn total;").len()
                >= MIN_INDEXED_BODY_LEN
        );
    }

    // --- body normalization for hashing (issue #36) ---

    /// Convenience: hash a body the way call sites do — normalize, then feed to
    /// `compute_hash` with a fixed signature/docstring.
    fn hbody(body: &str, lang: &str) -> String {
        compute_hash("sig", &normalize_body_for_hash(body, lang), "")
    }

    #[test]
    fn test_hash_stable_across_reformatting_rust() {
        let original = "{ let x = 1; return x + 1; }";
        let reformatted = "{\n    let x = 1;\n\n    return x + 1;\n}";
        assert_eq!(hbody(original, "rust"), hbody(reformatted, "rust"));
    }

    #[test]
    fn test_hash_stable_across_reformatting_typescript() {
        let original = "{ const x = 1; return x + 1; }";
        // Brace moved to its own line, re-indented, blank line added.
        let reformatted = "{\n  const x = 1;\n  return x + 1;\n}\n";
        assert_eq!(
            hbody(original, "typescript"),
            hbody(reformatted, "typescript")
        );
    }

    #[test]
    fn test_hash_stable_across_reformatting_python() {
        let original = "x = 1\nreturn x + 1";
        let reformatted = "    x = 1\n\n    return x + 1\n";
        assert_eq!(hbody(original, "python"), hbody(reformatted, "python"));
    }

    #[test]
    fn test_hash_stable_across_comment_edit_rust() {
        let with = "{ let x = 1; // set x\n return x; /* done */ }";
        let without = "{ let x = 1; return x; }";
        assert_eq!(hbody(with, "rust"), hbody(without, "rust"));
    }

    #[test]
    fn test_hash_stable_across_comment_edit_typescript() {
        let with = "{ const x = 1; // note\n /* block */ return x; }";
        let without = "{ const x = 1; return x; }";
        assert_eq!(hbody(with, "typescript"), hbody(without, "typescript"));
    }

    #[test]
    fn test_hash_stable_across_comment_edit_python() {
        let with = "x = 1  # set x\nreturn x  # give it back";
        let without = "x = 1\nreturn x";
        assert_eq!(hbody(with, "python"), hbody(without, "python"));
    }

    #[test]
    fn test_hash_changes_on_code_token_change() {
        assert_ne!(
            hbody("{ return x + 1; }", "rust"),
            hbody("{ return x + 2; }", "rust")
        );
        assert_ne!(
            hbody("return x + 1", "python"),
            hbody("return x + 2", "python")
        );
    }

    /// The docstring is hashed separately, so a doc change still changes the
    /// hash even though non-doc comments are stripped from the body.
    #[test]
    fn test_hash_changes_on_docstring_change() {
        let body = normalize_body_for_hash("{ return 1; }", "rust");
        assert_ne!(
            compute_hash("sig", &body, "Does X"),
            compute_hash("sig", &body, "Does Y")
        );
    }

    /// Comment-looking text inside a string literal is code and must be
    /// preserved verbatim — changing it changes the hash.
    #[test]
    fn test_hash_changes_on_comment_text_inside_string() {
        assert_ne!(
            hbody(r#"{ let s = "// hello"; }"#, "rust"),
            hbody(r#"{ let s = "// world"; }"#, "rust")
        );
        assert_ne!(
            hbody("s = \"# hello\"", "python"),
            hbody("s = \"# world\"", "python")
        );
        assert_ne!(
            hbody("s = '''# hello'''", "python"),
            hbody("s = '''# world'''", "python")
        );
    }

    /// A `//` sequence inside a string must not be treated as a comment, so
    /// trailing code after the string is preserved.
    #[test]
    fn test_string_with_comment_marker_not_stripped() {
        let n = normalize_body_for_hash(r#"let url = "http://x"; return url;"#, "rust");
        assert!(n.contains("http://x"), "string content dropped: {n}");
        assert!(n.contains("return url;"), "code after string dropped: {n}");
    }

    #[test]
    fn test_normalize_body_for_hash_cfamily_collapses_newlines() {
        // C-family: newlines are insignificant and collapse to spaces.
        assert_eq!(
            normalize_body_for_hash("{\n  a();\n  b();\n}", "go"),
            "{ a(); b(); }"
        );
    }

    #[test]
    fn test_normalize_body_for_hash_python_keeps_line_order() {
        // Python: statement lines are preserved (indentation trimmed).
        assert_eq!(
            normalize_body_for_hash("    a()\n    b()", "python"),
            "a()\nb()"
        );
    }

    #[test]
    fn test_normalize_body_for_hash_is_deterministic() {
        let body = "{ let x = 1; /* c */ return x; // done\n }";
        assert_eq!(
            normalize_body_for_hash(body, "rust"),
            normalize_body_for_hash(body, "rust")
        );
    }

    /// A Rust lifetime (`'a`) is an unpaired `'` and must not open a string,
    /// or a comment after it would not be stripped and the hash would churn.
    #[test]
    fn test_rust_lifetime_does_not_swallow_trailing_comment() {
        let with = "{ let x: &'a str = get(); // grab it\n use_it(x); }";
        let without = "{ let x: &'a str = get(); use_it(x); }";
        assert_eq!(
            hbody(with, "rust"),
            hbody(without, "rust"),
            "a comment after a lifetime must still be stripped"
        );
    }

    /// Char literals are still handled as strings, so their content is verbatim.
    #[test]
    fn test_rust_char_literal_preserved() {
        assert_ne!(
            hbody(r"{ let c = 'a'; }", "rust"),
            hbody(r"{ let c = 'b'; }", "rust")
        );
        // Escaped char literal is preserved and does not desync the scanner.
        let n = normalize_body_for_hash(r"{ let c = '\''; return c; }", "rust");
        assert!(n.contains("return c;"), "scanner desynced on '\\'': {n}");
    }
}
