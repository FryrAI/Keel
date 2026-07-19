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
}
